//! Phase 3D-4 (B2): the supply of the ONE reviewed jq 1.8.2 / Windows x64 pilot -- compiled for tests and for a build made with the
//! non-default, Windows-only `jq-pilot` feature, and for nothing else.
//!
//! What this module assembles, from files that are committed, hash-pinned by tests and read at compile time (no path, URL, environment
//! variable or argument can name anything else):
//!
//! 1. the candidate recipe, UNCHANGED (its digest is the one the independent reviewer accepted);
//! 2. its review block, which is derived from ONE source: the B1 binding record (`docs/phase-3d4/b1-review-binding.json`). That record is
//!    checked against a verbatim capture of the reviewer's decision (Issue #23, comment 6299270819): every hash it carries must occur in
//!    that capture, the capture's own hash must match, and nothing the reviewer did not run may be presented as independent evidence;
//! 3. the one authenticated reference, the source-attestation record and the `LICENSE_PASS_LOCAL_USE` record;
//! 4. the notice bundle (the exact text the licence record is bound to) with its unresolved questions;
//! 5. the catalog row `jq` (the recipe's tool id) -- seeded only in this build;
//! 6. the fixed, read-only retained-file provider;
//! 7. the exact activation entry (recipe digest + artifact digest).
//!
//! **Two constants, two separate decisions.** [`B3_UNLOCK`] names the exact reviewed recipe digest (with it the supply holds the real provider and the
//! enablement entry). It is not a permission by itself: the one-installation allowance must also be ARMED by [`B3_ARMED_DATA_DIR_SHA256`]. While that is
//! `None` the allowance is INACTIVE and the service answers `pilot_not_armed` BEFORE any preview, so not one byte of the retained file is requested.
//! The allowance (`pilot_allowance`, Issue #24) is armed only by the commit that records the user's separate authorization and names the ONE isolated
//! data directory (as a digest, never a path); in any other data directory the service answers `pilot_wrong_data_directory`, and the allowance can be
//! spent exactly once. With [`B3_UNLOCK`] `None` the provider is [`LockedProvider`] (it never touches the file system) and the pilot gate is EMPTY.
//! Neither is a flag, a path, an environment variable or an argument. Tests exercise the whole path with SYNTHETIC bytes whose digests differ from
//! the real ones, through the very same assembly code.
//!
//! The module starts no process, opens no socket, reads no environment variable and changes no PATH, registry or agent setting.
#![allow(dead_code)]

use crate::{
    artifact_provider::{RetainedEntry, RetainedFileProvider},
    attestation::{Evidence, LocalUseLicense, SourceAttestation},
    audit::sha256_hex,
    catalog,
    discovery::reference::{ArtifactKind, ReferenceDigest, ReferenceSet},
    error::AppResult,
    install_flow::{ArtifactProvider, NoticeBundle, Sources},
    lifecycle::PilotRecipe,
    pilot_allowance::OneTimePilot,
    recipe::{self, Recipe, RecipeRegistry, ReviewRecord, ReviewedRecipe},
};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

pub const TOOL: &str = "jq";
/// The ONE folder (inside the application's own data folder) the retained file is read from, once unlocked. Never chosen by the interface.
pub const INPUT_FOLDER: &str = "jq-pilot-input";

/// The B3 unlock for the real bytes: the exact reviewed recipe digest (the activation CANDIDATE, Phase 3D-5). Anything else (including a wrong digest)
/// stays locked. It authorizes nothing by itself: the one-time allowance must also be armed ([`B3_ARMED_DATA_DIR_SHA256`]), and that is the user's decision.
pub const B3_UNLOCK: Option<&str> = Some("9c7a020d65d233e86577464a06c32b8f08fd4fd8403110e68fd551b674f7cca1");

/// The ONE isolated data directory the one-installation allowance is ARMED for (Phase 3D-5): the [`pilot_allowance::data_dir_digest`] of the directory the
/// separately authorized pilot will run in. `None` = the allowance is INACTIVE and nothing real can be previewed or installed, whatever
/// [`B3_UNLOCK`] says. It is set only by the commit that records the user's authorization, never before. The digest is a scope identifier (which
/// directory), not a secret and not an authentication credential.
pub const B3_ARMED_DATA_DIR_SHA256: Option<&str> = None;

const CANDIDATE_JSON: &str = include_str!("../../docs/phase-3d3/jq-1.8.2-windows-x64.recipe.candidate.json");
const BINDING_JSON: &str = include_str!("../../docs/phase-3d4/b1-review-binding.json");
const DECISION_CAPTURE: &str = include_str!("../../docs/phase-3d4/evidence/issue-23-comment-6299270819.md");
const ATTESTATION_JSON: &str = include_str!("../../docs/phase-3d3/evidence/jq-1.8.2-windows-x64.source-attestation.json");
const LOCAL_USE_JSON: &str = include_str!("../../docs/phase-3d3/evidence/jq-1.8.2-windows-x64.local-use-license.json");
const REFERENCE_JSON: &str = include_str!("../../docs/phase-3d3b/evidence/reference-entry.json");
const NOTICE_MANIFEST_JSON: &str = include_str!("../../docs/phase-3d2/notices/jq-1.8.2-windows-x64.notices-manifest.json");
const NOTICE_BUNDLE_TEXT: &str = include_str!("../../docs/phase-3d2/notices/THIRD-PARTY-NOTICES.jq-1.8.2.windows-x64.txt");

// ---------------------------------------------------------------------------------------------------------------------
// The inputs (owned strings, so tests can build a consistent synthetic copy or tamper with one piece)
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct Spec {
    pub candidate: String,
    pub binding: String,
    pub decision: String,
    pub attestation: String,
    pub license: String,
    pub reference: String,
    pub notice_manifest: String,
    pub notice_text: String,
}

impl Spec {
    /// The committed files, exactly.
    pub fn committed() -> Spec {
        Spec {
            candidate: CANDIDATE_JSON.to_string(),
            binding: BINDING_JSON.to_string(),
            decision: DECISION_CAPTURE.to_string(),
            attestation: ATTESTATION_JSON.to_string(),
            license: LOCAL_USE_JSON.to_string(),
            reference: REFERENCE_JSON.to_string(),
            notice_manifest: NOTICE_MANIFEST_JSON.to_string(),
            notice_text: NOTICE_BUNDLE_TEXT.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The B1 binding: the reviewer-issued decision, tied to the exact recipe
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BindingSource {
    pub kind: String,
    pub repository: String,
    pub issue: u64,
    pub comment_id: u64,
    pub url: String,
    pub posted_by: String,
    pub posted_at: String,
    pub snapshot_path: String,
    pub snapshot_sha256: String,
    pub snapshot_note: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BindingReviewer {
    pub id: String,
    pub name: String,
    pub role: String,
    pub signature: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BindingAccepted {
    pub final_sha: String,
    pub recipe_id: String,
    pub revision: u32,
    pub recipe_digest: String,
    pub tool_id: String,
    pub version: String,
    pub platform_target: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub artifact_sha256: String,
    pub origin: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReviewerPublished {
    pub examined_zip_sha256: String,
    pub examined_final_tree: String,
    pub examined_candidate_tree: String,
    pub decision_file_sha256: String,
    pub source_report_sha256: String,
    pub check_results_sha256: String,
    pub checks: Value,
    pub fixture_bytes: Value,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SupportingCi {
    pub independent: bool,
    pub run_id: u64,
    pub url: String,
    pub head_sha: String,
    pub conclusion: String,
    pub jobs: Value,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Binding {
    pub schema_version: u32,
    pub purpose: String,
    pub source: BindingSource,
    pub reviewer: BindingReviewer,
    pub accepted: BindingAccepted,
    pub scope: Vec<String>,
    pub limitations: Vec<String>,
    pub not_independently_executed: Vec<String>,
    pub reviewer_published: ReviewerPublished,
    pub supporting_ci: SupportingCi,
    pub developer_checks: Vec<Value>,
    pub review: ReviewRecord,
}

/// What the capture of the reviewer's comment must literally say for this binding to stand (besides the hashes, which are checked one by one).
const DECISION_PHRASES: &[&str] = &[
    "B1 LIMITED-PILOT ACCEPTED",
    "does NOT mean full independently executed Rust/frontend staging",
    "NOT independently run by ChatGPT",
    "B2 GO (implementation and synthetic testing only)",
    "B3 real installation NO-GO",
    "Do not fabricate reviewer identities",
];

/// Checks the binding against the recipe and the capture of the reviewer's decision and returns the review block the recipe is promoted with.
/// Every refusal has its own closed code.
pub(crate) fn verify_binding(b: &Binding, recipe: &Recipe, decision: &str) -> Result<ReviewRecord, &'static str> {
    if b.schema_version != 1 {
        return Err("binding_schema");
    }
    // the capture of the decision is the one the binding names
    let s = &b.source;
    if sha256_hex(decision) != s.snapshot_sha256 {
        return Err("binding_capture_changed");
    }
    if s.kind != "github_issue_comment"
        || s.issue == 0
        || s.comment_id == 0
        || !s.url.ends_with(&format!("/issues/{}#issuecomment-{}", s.issue, s.comment_id))
        || s.repository.split('/').count() != 2
        || !s.url.contains(&s.repository)
    {
        return Err("binding_source");
    }
    // everything the binding claims the reviewer said, the capture says
    let a = &b.accepted;
    let p = &b.reviewer_published;
    let must_occur = [
        a.final_sha.as_str(),
        a.recipe_digest.as_str(),
        a.artifact_sha256.as_str(),
        p.examined_zip_sha256.as_str(),
        p.examined_final_tree.as_str(),
        p.examined_candidate_tree.as_str(),
        p.decision_file_sha256.as_str(),
        p.source_report_sha256.as_str(),
        p.check_results_sha256.as_str(),
    ];
    if must_occur.iter().any(|h| h.is_empty() || !decision.contains(h)) || DECISION_PHRASES.iter().any(|t| !decision.contains(t)) {
        return Err("binding_not_in_decision");
    }
    if !decision.contains(&b.reviewer.name) || b.reviewer.name.is_empty() || b.reviewer.id.len() < 3 || b.reviewer.id.len() > 64 {
        return Err("binding_reviewer");
    }
    // the exact recipe, unchanged
    if recipe::recipe_digest(recipe) != a.recipe_digest
        || recipe.recipe_id != a.recipe_id
        || recipe.revision != a.revision
        || recipe.tool_id != a.tool_id
        || recipe.version != a.version
        || recipe.target.platform_target != a.platform_target
        || recipe.artifact.file_name != a.file_name
        || recipe.artifact.size_bytes != a.size_bytes
        || recipe.artifact.sha256 != a.artifact_sha256
        || recipe.artifact.origin.as_deref() != Some(a.origin.as_str())
    {
        return Err("binding_recipe_differs");
    }
    // the review block: reviewed by the named reviewer, over this digest, with exactly the reviewer's own evidence and nothing else
    let r = &b.review;
    if r.state != "reviewed" || r.reviewer_id != b.reviewer.id || r.reviewed_recipe_digest != a.recipe_digest {
        return Err("binding_review_block");
    }
    let published = [p.check_results_sha256.as_str(), p.source_report_sha256.as_str()];
    let mut seen: Vec<&str> = Vec::new();
    for e in &r.staging_evidence {
        if !e.independent
            || e.state != "passed"
            || !published.contains(&e.evidence_digest.as_str())
            || seen.contains(&e.evidence_digest.as_str())
        {
            return Err("binding_evidence_not_the_reviewers");
        }
        seen.push(e.evidence_digest.as_str());
    }
    if seen.len() != published.len() {
        return Err("binding_evidence_incomplete");
    }
    // nothing is overclaimed: CI is recorded as supporting and not independent; the Rust/frontend/GUI runs are stated as NOT independent
    let ci = &b.supporting_ci;
    let not_run = ["rust-tests-48", "frontend-staging-checks-2", "windows-gui"];
    if ci.independent
        || ci.head_sha != a.final_sha
        || ci.conclusion != "success"
        || !not_run.iter().all(|x| b.not_independently_executed.iter().any(|y| y == x))
        || b.limitations.len() < 4
        || b.scope.is_empty()
    {
        return Err("binding_overclaims");
    }
    Ok(r.clone())
}

// ---------------------------------------------------------------------------------------------------------------------
// Assembly
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceEntry {
    entry_id: String,
    kind: String,
    format: String,
    platform_target: String,
    version: String,
    sha256: String,
    authentication: String,
}

pub(crate) struct Assembly {
    pub reviewed: ReviewedRecipe,
    pub reference: ReferenceDigest,
    pub evidence: Evidence,
    pub notice: NoticeBundle,
    pub activation: PilotRecipe,
    pub retained: RetainedEntry,
}

/// The notice bundle the application presents: the exact text the licence record is bound to, its components, and the questions that stay open.
fn notice_bundle(manifest: &Value, text: &str, license: &LocalUseLicense, recipe: &Recipe) -> Result<NoticeBundle, &'static str> {
    let declared = manifest.get("bundleSha256").and_then(Value::as_str).unwrap_or_default();
    if sha256_hex(text) != license.notice_bundle_sha256
        || declared != license.notice_bundle_sha256
        || manifest.get("bundleBytes").and_then(Value::as_u64) != Some(text.len() as u64)
        || manifest.get("tool").and_then(Value::as_str) != Some(recipe.tool_id.as_str())
        || manifest.get("version").and_then(Value::as_str) != Some(recipe.version.as_str())
        || manifest.get("writtenIntoToolFolder").and_then(Value::as_bool) != Some(false)
    {
        return Err("notices_do_not_match_the_license");
    }
    let field = |c: &Value, k: &str| c.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let components: Vec<String> = manifest
        .get("components")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|c| format!("{} — {} ({})", field(c, "name"), field(c, "spdx"), field(c, "legalStatus"))).collect())
        .unwrap_or_default();
    if components.is_empty() {
        return Err("notices_without_components");
    }
    let mut unresolved: Vec<String> = license
        .open_questions
        .iter()
        .map(|q| {
            format!(
                "{}: {} — {}",
                q.id,
                q.text,
                if q.blocks_redistribution {
                    "yeniden dağıtım için AÇIK; bu yerel, tek seferlik kullanımı engellemez."
                } else {
                    "açık."
                }
            )
        })
        .collect();
    unresolved.push(
        "Lisans kabulü YALNIZCA bu bilgisayardaki tek yerel kullanım içindir (LICENSE_PASS_LOCAL_USE); yeniden dağıtım, paketleme veya barındırma izni DEĞİLDİR."
            .to_string(),
    );
    Ok(NoticeBundle {
        tool_id: recipe.tool_id.clone(),
        version: recipe.version.clone(),
        title: format!("{} {} ({}) - üçüncü taraf bildirimleri", recipe.tool_id, recipe.version, recipe.target.platform_target),
        components,
        unresolved,
        text: text.to_string(),
    })
}

/// Builds and cross-checks every piece. Any failure returns a closed code and the caller falls back to the empty supply.
pub(crate) fn assemble(spec: &Spec) -> Result<Assembly, &'static str> {
    let candidate: ReviewedRecipe = serde_json::from_str(&spec.candidate).map_err(|_| "candidate_unreadable")?;
    let binding: Binding = serde_json::from_str(&spec.binding).map_err(|_| "binding_unreadable")?;
    let review = verify_binding(&binding, &candidate.recipe, &spec.decision)?;
    let reviewed = ReviewedRecipe { recipe: candidate.recipe, review };
    let r = &reviewed.recipe;

    let entry: ReferenceEntry = serde_json::from_str(&spec.reference).map_err(|_| "reference_unreadable")?;
    if entry.kind != "executable"
        || entry.entry_id != r.tool_id
        || entry.version != r.version
        || entry.platform_target != r.target.platform_target
        || entry.format != r.artifact.format
        || entry.sha256 != r.artifact.sha256
        || entry.authentication.is_empty()
    {
        return Err("reference_differs");
    }
    let reference = ReferenceDigest {
        entry_id: entry.entry_id,
        kind: ArtifactKind::Executable,
        format: entry.format,
        platform_target: entry.platform_target,
        version: entry.version,
        sha256: entry.sha256,
        source: entry.authentication,
    };
    let attestation: SourceAttestation = serde_json::from_str(&spec.attestation).map_err(|_| "attestation_unreadable")?;
    let license: LocalUseLicense = serde_json::from_str(&spec.license).map_err(|_| "license_unreadable")?;
    let manifest: Value = serde_json::from_str(&spec.notice_manifest).map_err(|_| "notices_unreadable")?;
    let notice = notice_bundle(&manifest, &spec.notice_text, &license, r)?;
    let evidence = Evidence { attestations: vec![attestation], local_use_licenses: vec![license] };

    // the very validator the workflow applies: reviewed, independently evidenced, attested, licensed for the local use, authenticated
    let refs = ReferenceSet { authenticated: vec![reference.clone()], unauthenticated: vec![] };
    let valid = recipe::validate_with(&reviewed, &refs, &evidence).map_err(|p| p.first().copied().unwrap_or("recipe_not_valid"))?;
    let activation = PilotRecipe {
        tool_id: r.tool_id.clone(),
        version: r.version.clone(),
        platform_target: r.target.platform_target.clone(),
        recipe_digest: valid.digest().to_string(),
        artifact_sha256: r.artifact.sha256.clone(),
    };
    let retained = RetainedEntry {
        tool_id: r.tool_id.clone(),
        version: r.version.clone(),
        platform_target: r.target.platform_target.clone(),
        file_name: r.artifact.file_name.clone(),
        size_bytes: r.artifact.size_bytes,
    };
    Ok(Assembly { reviewed, reference, evidence, notice, activation, retained })
}

// ---------------------------------------------------------------------------------------------------------------------
// The supply
// ---------------------------------------------------------------------------------------------------------------------

/// The provider while the real bytes are locked: it answers every request with a refusal and never touches the file system.
pub(crate) struct LockedProvider;

impl ArtifactProvider for LockedProvider {
    fn bytes(&self, _: &str, _: &str, _: &str) -> Result<Vec<u8>, &'static str> {
        Err("real_install_locked")
    }
}

/// Is the retained file unlocked for exactly this recipe? Only when the unlock constant names the recipe's digest.
pub(crate) fn unlocked_for(unlock: Option<&str>, recipe_digest: &str) -> bool {
    unlock == Some(recipe_digest)
}

/// The supply for `spec`, NOT armed for any data directory (the allowance is inactive). Tests arm it explicitly.
pub(crate) fn sources(spec: &Spec, input_folder: PathBuf, unlock: Option<&str>) -> Sources {
    sources_with(spec, input_folder, unlock, None)
}

/// The supply for `spec`. Locked unless `unlock` names the recipe's digest; the one-installation allowance (always present) is armed only for the
/// data directory `armed` names; the empty release supply if anything does not check out.
pub(crate) fn sources_with(spec: &Spec, input_folder: PathBuf, unlock: Option<&str>, armed: Option<&str>) -> Sources {
    let Ok(a) = assemble(spec) else { return Sources::production() };
    let unlocked = unlocked_for(unlock, &a.activation.recipe_digest);
    Sources {
        registry: RecipeRegistry::pilot(vec![a.reviewed]),
        extra_authenticated: vec![a.reference],
        artifacts: if unlocked { Box::new(RetainedFileProvider::new(input_folder, vec![a.retained])) } else { Box::new(LockedProvider) },
        notices: vec![a.notice],
        evidence: a.evidence,
        one_time: Some(OneTimePilot {
            tool_id: a.activation.tool_id.clone(),
            version: a.activation.version.clone(),
            platform_target: a.activation.platform_target.clone(),
            recipe_digest: a.activation.recipe_digest.clone(),
            artifact_sha256: a.activation.artifact_sha256.clone(),
            armed_data_dir_sha256: armed.map(str::to_string),
        }),
        pilot_gate: if unlocked { vec![a.activation] } else { vec![] },
    }
}

/// The supply of the `jq-pilot` application build: the committed files, on Windows, locked by [`B3_UNLOCK`]. Anywhere else it is empty.
pub fn sources_for_app(input_folder: PathBuf) -> Sources {
    if !cfg!(windows) {
        return Sources::production();
    }
    sources_with(&Spec::committed(), input_folder, B3_UNLOCK, B3_ARMED_DATA_DIR_SHA256)
}

/// The catalog row `jq` (the recipe's tool id). It states what is true of the row: metadata only, unreviewed, licence review required for any
/// wider use; it carries no digest and no claim of installation. Idempotent; a row the person removed is not brought back.
pub fn seed_catalog(conn: &Connection) -> AppResult<()> {
    let Ok(candidate) = serde_json::from_str::<ReviewedRecipe>(CANDIDATE_JSON) else { return Ok(()) };
    let r = &candidate.recipe;
    let (owner, name) = r.source.repository.split_once('/').unwrap_or(("jqlang", "jq"));
    let record = json!({
        "schemaVersion": 0,
        "toolId": r.tool_id,
        "synthetic": false,
        "repository": { "url": format!("https://github.com/{}", r.source.repository), "owner": owner, "name": name },
        "targetEvidence": [],
        "installMethods": [],
        "license": { "state": "unknown" },
        "privacy": { "telemetry": "none-declared", "mayTransmitProjectContent": false },
        "note": "Kayıt yalnızca üst veridir. Bu derlemede jq için tek, yerel, YALNIZCA KURULUM pilotu vardır; dosya çalıştırılmaz.",
    });
    let fingerprint = catalog::capability_fingerprint(&record, "executable_tool", "manual-only");
    let now = crate::audit::now_utc();
    let source_key = r.source.repository.to_ascii_lowercase();
    conn.execute(
        "INSERT OR IGNORE INTO catalog_entries
         (entry_id, kind, origin, display_name, purpose, source_key, repo_url, source_path, plugin_identity, review_state, metadata_state,
          execution_policy, license_state, license_spdx, enabled, revision, capability_fingerprint, record_json, added_at, updated_at)
         VALUES (?1, 'executable_tool', 'user', ?2, ?3, ?4, ?5, NULL, NULL, 'unreviewed', 'not-fetched',
                 'manual-only', 'review-required', NULL, 1, ?6, ?7, ?8, ?9, ?9)",
        rusqlite::params![
            r.tool_id,
            format!("jq {} (Windows x64) - yerel kurulum pilotu", r.version),
            "JSON işleme komut satırı aracı. Bu derlemede yalnızca tek seferlik, yerel, YALNIZCA KURULUM pilotu: dosya çalıştırılmaz, PATH'e eklenmez.",
            source_key,
            format!("https://github.com/{}", r.source.repository),
            format!("{}@{}", r.version, &r.source.immutable_commit[..r.source.immutable_commit.len().min(12)]),
            fingerprint,
            record.to_string(),
            now
        ],
    )?;
    Ok(())
}

/// A stable summary of what this build holds, for tests and the self-check: no digest, no path.
pub fn describe(sources: &Sources) -> Value {
    json!({
        "realRecipes": sources.registry.len(),
        "authenticatedReferences": sources.extra_authenticated.len(),
        "enablementEntries": sources.pilot_gate.len(),
        "oneTimeAllowances": usize::from(sources.one_time.is_some()),
        "notices": sources.notices.len(),
        "evidenceRecords": sources.evidence.attestations.len() + sources.evidence.local_use_licenses.len(),
    })
}
