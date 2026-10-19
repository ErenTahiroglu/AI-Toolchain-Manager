//! Phase 3A: the reviewed install-recipe registry.
//!
//! A *recipe* is the typed, versioned statement of exactly what installing one tool version on one target would do. It is kept
//! apart from everything else on purpose:
//!
//! * **Catalog / README metadata never becomes a recipe.** A recipe is a separate record with its own review; nothing is
//!   derived from a catalog entry, a repository text or a user-typed command.
//! * **Review is not authorization.** A recipe can be `reviewed`; that says nothing about whether anyone agreed to apply it.
//!   Authorization is a separate, narrowly bound object (`approval.rs`).
//! * **A review is bound to the content it saw.** Changing any recipe field after review changes the recipe digest and the
//!   review no longer applies.
//! * **The artifact digest needs independent authentication.** It must equal a digest in the *authenticated* reference set
//!   (`discovery::reference`), for this tool, version, platform, kind and verified format. The compiled-in authenticated set is
//!   empty, and so is the compiled-in recipe list: every real catalog entry stays unapproved.
//! * Positive paths are exercised with fully synthetic fixtures only (`synthetic: true`).

#![allow(dead_code)] // the synthetic lifecycle is exercised by tests and the self-check only; no command reaches it

use crate::{
    attestation::Evidence,
    audit::{canonical_json, sha256_hex},
    discovery::reference::{ArtifactFormat, ArtifactKind, ReferenceSet},
    plan::{self, OperationKind},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const RECIPE_SCHEMA_VERSION: u32 = 1;

/// Compiled-in, reviewed recipes. EMPTY in this phase: no real tool has a reviewed recipe, and a test asserts it.
pub const BUILT_IN_RECIPES_JSON: &str = "[]";

pub const PERMISSIONS: &[&str] = &["current_user_account", "network_download", "write_manager_folders"];
pub const NETWORK_DESTINATIONS: &[&str] = &["github_release_download", "package_registry", "agent_plugin_registry"];
pub const FS_AREAS: &[&str] = &["manager_staging", "manager_versions", "manager_state"];
pub const FS_KINDS: &[&str] = &["create", "replace", "remove"];
pub const ROLLBACK_UNDOES: &[&str] = &["pointer_restore", "staging_removal", "version_folder_removal", "removed_version_restore"];
pub const ROLLBACK_NOT_UNDONE: &[&str] = &["tool_user_data", "external_effects", "agent_configuration"];
pub const LIFECYCLE_ACTIONS: &[&str] = &["install", "update", "uninstall"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeTarget {
    pub platform_target: String,
    pub agent_surface: String,
    /// `user` | `project`
    pub scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeSource {
    /// `owner/name`
    pub repository: String,
    /// Exactly 40 or 64 lower-case hexadecimal characters. A declaration until `source_attested` says otherwise.
    pub immutable_commit: String,
    /// Has the commit been bound to the repository by independent evidence? Only a synthetic recipe can claim it today, because
    /// no verifier for real source attestations exists yet.
    pub source_attested: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DigestScope {
    /// The digest is of the exact file that gets installed.
    ExactFile,
    /// The digest is of an archive. It can never stand for a file extracted from it.
    Archive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeArtifact {
    /// Always `primary`.
    pub artifact_id: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub digest_scope: DigestScope,
    /// Canonical format id ([`ArtifactFormat::as_str`]).
    pub format: String,
    /// Where the bytes come from, as a REVIEWED part of the recipe (Phase 3D-3B). `local_retained_file`: one already retained, verified local
    /// file, read through the fixed read-only provider -- no download, no network. Absent: the recipe's `network` list says where it downloads from
    /// (a recipe without a network list and without an origin is refused unless it is synthetic). Absent is digest-neutral: every recipe that
    /// existed before this field has the digest it always had.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

pub const ORIGIN_LOCAL_RETAINED_FILE: &str = "local_retained_file";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthEvidence {
    /// `authenticated_reference_digest` (looked up in the authenticated reference set) | `synthetic_source_attestation`
    pub kind: String,
    pub evidence_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeLicense {
    /// `accepted` | `blocked` | `unknown`
    pub state: String,
    pub spdx_id: String,
    /// `R` + 2–3 digits, a row of RESEARCH-SOURCES.md
    pub evidence_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FsChange {
    pub area: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RollbackContract {
    /// `designed` | `simulated` | `verified`. `verified` needs evidence from a real target and cannot be claimed in this phase.
    pub state: String,
    pub undoes: Vec<String>,
    pub does_not_undo: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Recipe {
    pub schema_version: u32,
    pub recipe_id: String,
    /// A new revision is a new recipe with a new digest; a reviewed revision is never edited.
    pub revision: u32,
    pub synthetic: bool,
    pub tool_id: String,
    /// Exact version, never a range or "latest".
    pub version: String,
    pub target: RecipeTarget,
    pub source: RecipeSource,
    pub artifact: RecipeArtifact,
    pub authentication: Vec<AuthEvidence>,
    pub license: RecipeLicense,
    pub permissions: Vec<String>,
    pub filesystem_changes: Vec<FsChange>,
    pub network: Vec<String>,
    /// Which lifecycle actions this recipe supports.
    pub lifecycle_actions: Vec<String>,
    /// The typed install operations (schema-shaped, see `plan::typed_is_valid`): fetch → verify → stage → activate.
    pub steps: Vec<Value>,
    pub rollback: RollbackContract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StagingEvidence {
    pub check_id: String,
    /// `passed` | `failed` | `blocked`
    pub state: String,
    pub independent: bool,
    pub evidence_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewRecord {
    /// `draft` | `reviewed` | `rejected`
    pub state: String,
    pub reviewer_id: String,
    /// The digest of the recipe content the reviewer saw.
    pub reviewed_recipe_digest: String,
    pub staging_evidence: Vec<StagingEvidence>,
}

/// A recipe together with its review. Approval is deliberately not part of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedRecipe {
    pub recipe: Recipe,
    pub review: ReviewRecord,
}

pub fn recipe_digest(recipe: &Recipe) -> String {
    match serde_json::to_value(recipe) {
        Ok(v) => sha256_hex(&canonical_json(&v)),
        Err(_) => String::new(),
    }
}

fn is_id(s: &str) -> bool {
    let b = s.as_bytes();
    (2..=64).contains(&b.len())
        && b[0].is_ascii_alphanumeric()
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'.' | b'_' | b'-'))
}

fn is_version(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && b[0].is_ascii_alphanumeric()
        && b.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'+' | b'_' | b'-'))
}

fn is_hex(s: &str, len: &[usize]) -> bool {
    len.contains(&s.len()) && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_file_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && b[0].is_ascii_alphanumeric()
        && b.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
        && !s.contains("..")
}

/// A recipe that passed every check below, bound to the digest it was checked at. Only [`validate`] makes one.
#[derive(Debug, Clone)]
pub struct ValidatedRecipe {
    reviewed: ReviewedRecipe,
    digest: String,
}

impl ValidatedRecipe {
    pub fn recipe(&self) -> &Recipe {
        &self.reviewed.recipe
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn permissions_digest(&self) -> String {
        let r = self.recipe();
        let v = serde_json::json!({ "permissions": r.permissions, "filesystemChanges": r.filesystem_changes, "network": r.network });
        sha256_hex(&canonical_json(&v))
    }
}

/// Why a recipe is not usable. Closed codes; the order is stable. No attestation and no local-use licence record is available here: a real
/// recipe can claim neither (see [`problems_with`]).
pub fn problems(rr: &ReviewedRecipe, refs: &ReferenceSet) -> Vec<&'static str> {
    problems_with(rr, refs, &Evidence::none())
}

/// The same, judged against the reviewed evidence records (Phase 3D-3). A claim that a real recipe's source is attested, or that its licence is
/// accepted for local use, holds only if a record in `ev` substantiates it field by field.
pub fn problems_with(rr: &ReviewedRecipe, refs: &ReferenceSet, ev: &Evidence) -> Vec<&'static str> {
    let r = &rr.recipe;
    let mut p: Vec<&'static str> = Vec::new();
    if r.schema_version != RECIPE_SCHEMA_VERSION {
        p.push("schema_version");
    }
    if !is_id(&r.recipe_id) || !is_id(&r.tool_id) || r.revision == 0 {
        p.push("identity");
    }
    if !is_version(&r.version) || matches!(r.version.as_str(), "latest" | "stable" | "main" | "master" | "head") {
        p.push("version_not_exact");
    }
    if !plan::PLATFORMS.contains(&r.target.platform_target.as_str())
        || !plan::SURFACES.contains(&r.target.agent_surface.as_str())
        || !plan::SCOPES.contains(&r.target.scope.as_str())
    {
        p.push("target");
    }
    if !is_hex(&r.source.immutable_commit, &[40, 64]) || r.source.repository.split('/').count() != 2 {
        p.push("source_not_immutable");
    }
    let a = &r.artifact;
    if a.artifact_id != "primary" || !is_file_name(&a.file_name) || a.size_bytes == 0 || !is_hex(&a.sha256, &[64]) {
        p.push("artifact_identity");
    }
    if a.digest_scope != DigestScope::ExactFile {
        // an archive digest never stands for the extracted file (policy since Phase 2C)
        p.push("digest_scope_not_exact_file");
    }
    // the source of the bytes is explicit: either declared network destinations, or a declared local retained file (never both, never neither)
    match a.origin.as_deref() {
        None if !r.synthetic && r.network.is_empty() => p.push("artifact_source_undeclared"),
        None => {}
        Some(ORIGIN_LOCAL_RETAINED_FILE) => {
            if !r.network.is_empty() || r.permissions.iter().any(|x| x == "network_download") {
                p.push("artifact_origin_conflict");
            }
        }
        Some(_) => p.push("artifact_origin_unknown"),
    }
    let format = ArtifactFormat::parse(&a.format).filter(|f| *f != ArtifactFormat::Unrecognised);
    match format {
        Some(f) if f.role().map(ArtifactKind::is_exact_file).unwrap_or(false) && f.consistent_with(&r.target.platform_target) => {}
        _ => p.push("artifact_format"),
    }
    // independent authentication: the digest must equal an AUTHENTICATED reference for this exact tool/version/platform/format
    let authenticated = r.authentication.iter().any(|e| e.kind == "authenticated_reference_digest")
        && format
            .map(|f| {
                refs.authenticated.iter().any(|x| {
                    x.entry_id == r.tool_id
                        && x.platform_target == r.target.platform_target
                        && x.version == r.version
                        && x.sha256 == a.sha256
                        && x.kind.is_exact_file()
                        && Some(x.kind) == f.role()
                        && x.format == a.format
                })
            })
            .unwrap_or(false);
    if !authenticated {
        p.push("artifact_not_authenticated");
    }
    // two authenticated references that disagree about this version make it ambiguous
    let same_version: Vec<&str> = refs
        .authenticated
        .iter()
        .filter(|x| x.entry_id == r.tool_id && x.platform_target == r.target.platform_target && x.version == r.version)
        .map(|x| x.sha256.as_str())
        .collect();
    if same_version.iter().any(|d| *d != a.sha256) {
        p.push("authenticated_references_conflict");
    }
    if r.source.source_attested {
        // a synthetic recipe claims it through the synthetic evidence kind; a REAL recipe only through a record that substantiates every binding
        let kinded = r.authentication.iter().any(|e| e.kind == "synthetic_source_attestation");
        let substantiated = !r.synthetic
            && r.authentication
                .iter()
                .filter(|e| e.kind == "source_attestation_evidence")
                .any(|e| ev.attestation(&e.evidence_id).map(|a| a.problems_for(r).is_empty()).unwrap_or(false));
        if !((r.synthetic && kinded) || substantiated) {
            p.push("source_attestation_unverifiable");
        }
    }
    if r.authentication.iter().any(|e| {
        !matches!(e.kind.as_str(), "authenticated_reference_digest" | "synthetic_source_attestation" | "source_attestation_evidence")
            || !is_id(&e.evidence_id)
    }) {
        p.push("authentication_kind");
    }
    let license_shape = !r.license.spdx_id.is_empty() && r.license.spdx_id.len() <= 40 && r.license.evidence_id.starts_with('R');
    let license_ok = match r.license.state.as_str() {
        "accepted" => license_shape,
        // accepted for ONE local, non-redistributing use of ONE exact file: valid only with a record that covers this very recipe
        "accepted_local_use" => {
            license_shape
                && !r.synthetic
                && ev.local_use_license(&r.license.evidence_id).map(|l| l.problems_for(r).is_empty()).unwrap_or(false)
        }
        _ => false,
    };
    if !license_ok {
        p.push("license_not_accepted");
    }
    if r.permissions.is_empty()
        || r.permissions.iter().any(|x| !PERMISSIONS.contains(&x.as_str()))
        || !r.permissions.iter().any(|x| x == "current_user_account")
    {
        p.push("permissions");
    }
    if r.filesystem_changes.is_empty()
        || r.filesystem_changes.iter().any(|c| !FS_AREAS.contains(&c.area.as_str()) || !FS_KINDS.contains(&c.kind.as_str()))
        || !r.permissions.iter().any(|x| x == "write_manager_folders")
    {
        p.push("filesystem_changes");
    }
    if r.network.iter().any(|n| !NETWORK_DESTINATIONS.contains(&n.as_str()))
        || (!r.network.is_empty() && !r.permissions.iter().any(|x| x == "network_download"))
    {
        p.push("network");
    }
    if r.lifecycle_actions.is_empty() || r.lifecycle_actions.iter().any(|x| !LIFECYCLE_ACTIONS.contains(&x.as_str())) {
        p.push("lifecycle_actions");
    }
    let kinds: Vec<Option<OperationKind>> = r
        .steps
        .iter()
        .map(|s| s.get("operation").and_then(Value::as_str).and_then(|n| OperationKind::ALL.into_iter().find(|k| k.as_str() == n)))
        .collect();
    let template = [
        OperationKind::FetchReleaseArtifact,
        OperationKind::VerifyArtifactDigest,
        OperationKind::StagePrivateCopy,
        OperationKind::ActivateStagedArtifact,
    ];
    let steps_ok = kinds.len() == template.len()
        && kinds.iter().zip(template).all(|(k, t)| *k == Some(t))
        && r.steps.iter().zip(template).all(|(s, t)| plan::typed_is_valid(t, s, &r.target.agent_surface))
        && r.steps[1].get("expectedSha256").and_then(Value::as_str) == Some(a.sha256.as_str())
        && r.steps[3].get("activationSlot").and_then(Value::as_str) == Some("manager-versioned-binary");
    if !steps_ok {
        p.push("steps");
    }
    let rb = &r.rollback;
    if !matches!(rb.state.as_str(), "designed" | "simulated")
        || rb.undoes.iter().any(|x| !ROLLBACK_UNDOES.contains(&x.as_str()))
        || rb.does_not_undo.iter().any(|x| !ROLLBACK_NOT_UNDONE.contains(&x.as_str()))
        || rb.undoes.is_empty()
        || !rb.does_not_undo.iter().any(|x| x == "tool_user_data")
    {
        // "verified" is refused outright: it needs evidence from a real target, which this phase does not have
        p.push("rollback_contract");
    }
    // ---- review, bound to the content that was reviewed
    let rv = &rr.review;
    if rv.state != "reviewed" {
        p.push("not_reviewed");
    } else if rv.reviewed_recipe_digest != recipe_digest(r) {
        p.push("changed_after_review");
    }
    if rv.reviewer_id.len() < 3 || rv.reviewer_id.len() > 64 {
        p.push("reviewer");
    }
    let good_evidence =
        rv.staging_evidence.iter().any(|e| e.state == "passed" && e.independent && is_hex(&e.evidence_digest, &[64]) && is_id(&e.check_id));
    if !good_evidence || rv.staging_evidence.iter().any(|e| e.state == "failed") {
        p.push("staging_evidence");
    }
    if r.synthetic != r.tool_id.starts_with("synthetic-") {
        p.push("synthetic_flag");
    }
    p
}

pub fn validate(rr: &ReviewedRecipe, refs: &ReferenceSet) -> Result<ValidatedRecipe, Vec<&'static str>> {
    validate_with(rr, refs, &Evidence::none())
}

pub fn validate_with(rr: &ReviewedRecipe, refs: &ReferenceSet, ev: &Evidence) -> Result<ValidatedRecipe, Vec<&'static str>> {
    let p = problems_with(rr, refs, ev);
    if p.is_empty() {
        Ok(ValidatedRecipe { digest: recipe_digest(&rr.recipe), reviewed: rr.clone() })
    } else {
        Err(p)
    }
}

/// The set of reviewed recipes the application knows. Production: the compiled-in list, which is empty.
#[derive(Debug, Clone, Default)]
pub struct RecipeRegistry {
    entries: Vec<ReviewedRecipe>,
}

impl RecipeRegistry {
    pub fn production() -> RecipeRegistry {
        let entries: Vec<ReviewedRecipe> = serde_json::from_str(BUILT_IN_RECIPES_JSON).unwrap_or_default();
        RecipeRegistry { entries }
    }

    /// A registry of synthetic recipes (tests and the self-check). Anything not marked synthetic is refused.
    pub fn synthetic(entries: Vec<ReviewedRecipe>) -> Result<RecipeRegistry, &'static str> {
        if entries.iter().any(|e| !e.recipe.synthetic || !e.recipe.tool_id.starts_with("synthetic-")) {
            return Err("only_synthetic_recipes_may_be_registered_outside_the_built_in_list");
        }
        Ok(RecipeRegistry { entries })
    }

    /// Tests only: a registry that may hold REAL (non-synthetic) recipes, to exercise the transition. No release or pilot build has this.
    #[cfg(test)]
    pub fn for_tests(entries: Vec<ReviewedRecipe>) -> RecipeRegistry {
        RecipeRegistry { entries }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Candidate recipes for a tool on a target (not yet validated).
    pub fn find(&self, tool_id: &str, platform: &str, surface: &str, scope: &str) -> Vec<&ReviewedRecipe> {
        self.entries
            .iter()
            .filter(|e| {
                let r = &e.recipe;
                r.tool_id == tool_id && r.target.platform_target == platform && r.target.agent_surface == surface && r.target.scope == scope
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "recipe_tests.rs"]
mod tests;
