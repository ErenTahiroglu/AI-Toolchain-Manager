//! Phase 3D-3: the smallest defensible evidence representations a REAL recipe needs, and the policy that reads them.
//!
//! Two records, both data (JSON), both trusted ONLY when they are compiled into the application (a code-reviewed list that is EMPTY in this
//! build, exactly like the authenticated-reference list). A recipe can claim neither by saying so:
//!
//! * [`SourceAttestation`] -- connects, for one exact file, (1) its digest, (2) the official verified attestation, (3) the repository identity,
//!   (4) the release workflow identity, (5) the pinned tag and commit, (6) the retained evidence hashes and (7) the reviewed Windows/macOS
//!   artifact. `sourceAttested: true` on a non-synthetic recipe is accepted only if the recipe names such a record and EVERY binding holds.
//! * [`LocalUseLicense`] -- a licence acceptance that is expressly scoped to ONE local, non-redistributing use of ONE exact file. It makes the
//!   recipe state `accepted_local_use`, which is a different state from `accepted`: it can never satisfy a check that asks for redistribution
//!   rights ([`redistribution_permitted`]).
//!
//! The policy is generic: nothing in it names jq. A future tool needs its own record, reviewed and compiled in.

use crate::recipe::Recipe;
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const GITHUB_ACTIONS_ISSUER: &str = "https://token.actions.githubusercontent.com";
pub const LOCAL_USE_SCOPE: &str = "local_use_no_redistribution";
pub const LOCAL_USE_DECISION: &str = "LICENSE_PASS_LOCAL_USE";

/// The compiled-in evidence. EMPTY: nothing is trusted until a reviewed change puts a record here.
pub const BUILT_IN_EVIDENCE_JSON: &str = r#"{"attestations":[],"localUseLicenses":[]}"#;

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttestedArtifact {
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub format: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttestedRepository {
    /// `owner/name`, exactly as the recipe's `source.repository`.
    pub slug: String,
    pub repository_id: u64,
    pub owner_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttestedSource {
    pub tag: String,
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttestedWorkflow {
    pub path: String,
    pub git_ref: String,
    pub issuer: String,
    pub runner_environment: String,
    pub trigger: String,
    pub run_url: String,
    pub transparency_log_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttestationVerification {
    /// What was run, by name (the official verifier, an offline replay, a supporting verifier, GPG).
    pub methods: Vec<String>,
    pub session_id: String,
    pub checks_total: u32,
    pub checks_failed: u32,
    pub official_verifier_passed: bool,
    /// The digest was computed over the downloaded BYTES and equals the attestation subject, the release digest and the publisher's checksum list.
    pub exact_bytes_verified: bool,
    pub gpg_fingerprint: String,
    pub gpg_signature_valid: bool,
    /// Recorded as it was: the first online run contacted a host outside the approved list. Not hidden, not repeated.
    pub network_policy_compliant: bool,
    pub network_incident: String,
    pub offline_replay_contacted_hosts: u32,
    pub acceptance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetainedFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceAttestation {
    pub schema_version: u32,
    pub evidence_id: String,
    pub tool_id: String,
    pub version: String,
    pub platform_target: String,
    pub artifact: AttestedArtifact,
    pub repository: AttestedRepository,
    pub source: AttestedSource,
    pub workflow: AttestedWorkflow,
    pub verification: AttestationVerification,
    pub retained_evidence: Vec<RetainedFile>,
    /// SHA-256 of the listing of the retained verification session (it lists the retained artifact's own digest).
    pub retained_session_listing_sha256: String,
}

impl SourceAttestation {
    /// Every way in which this record fails to substantiate `recipe`'s source claim. Empty = it does.
    pub fn problems_for(&self, recipe: &Recipe) -> Vec<&'static str> {
        let mut p = Vec::new();
        let a = &self.artifact;
        let ra = &recipe.artifact;
        if self.schema_version != SCHEMA_VERSION || self.evidence_id.len() < 3 {
            p.push("attestation_schema");
        }
        if self.tool_id != recipe.tool_id || self.version != recipe.version || self.platform_target != recipe.target.platform_target {
            p.push("attestation_not_for_this_recipe");
        }
        if a.file_name != ra.file_name
            || a.size_bytes != ra.size_bytes
            || a.sha256 != ra.sha256
            || a.format != ra.format
            || !is_hex(&a.sha256, 64)
        {
            p.push("attestation_artifact_differs");
        }
        if self.repository.slug != recipe.source.repository || self.repository.repository_id == 0 || self.repository.owner_id == 0 {
            p.push("attestation_repository_differs");
        }
        if self.source.commit != recipe.source.immutable_commit || !is_hex(&self.source.commit, 40) || self.source.tag.is_empty() {
            p.push("attestation_commit_differs");
        }
        let w = &self.workflow;
        if w.issuer != GITHUB_ACTIONS_ISSUER
            || w.runner_environment != "github-hosted"
            || !w.path.starts_with(".github/workflows/")
            || w.git_ref != format!("refs/tags/{}", self.source.tag)
            || !w.run_url.starts_with(&format!("https://github.com/{}/actions/runs/", self.repository.slug))
            || w.transparency_log_index == 0
        {
            p.push("attestation_workflow_identity");
        }
        let v = &self.verification;
        if !v.official_verifier_passed || !v.exact_bytes_verified || v.checks_failed != 0 || v.checks_total == 0 {
            p.push("attestation_verification_incomplete");
        }
        if !v.gpg_signature_valid || !is_hex(&v.gpg_fingerprint.to_ascii_lowercase(), 40) {
            p.push("attestation_signature_missing");
        }
        if v.acceptance.is_empty() || v.methods.len() < 2 || v.session_id.is_empty() {
            p.push("attestation_unreviewed");
        }
        if self.retained_evidence.len() < 5
            || self.retained_evidence.iter().any(|f| f.path.is_empty() || !is_hex(&f.sha256, 64))
            || !is_hex(&self.retained_session_listing_sha256, 64)
        {
            p.push("attestation_evidence_not_retained");
        }
        p
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Condition {
    pub id: String,
    pub text: String,
    pub satisfied_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenQuestion {
    pub id: String,
    pub text: String,
    /// Does this question prevent the evaluated local use? (A `true` here makes the record refuse to cover anything.)
    pub blocks_local_use: bool,
    /// Does it still prevent redistribution or bundling? Always kept visible.
    pub blocks_redistribution: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalUseLicense {
    pub schema_version: u32,
    /// Names a RESEARCH-SOURCES entry (`R...`): the written review record.
    pub evidence_id: String,
    pub scope: String,
    pub decision: String,
    pub tool_id: String,
    pub version: String,
    pub platform_target: String,
    pub artifact_sha256: String,
    pub spdx_expression: String,
    /// SHA-256 of the complete notice text that the application presents for this tool (a different text is not covered).
    pub notice_bundle_sha256: String,
    pub review_record_path: String,
    pub review_record_sha256: String,
    pub conditions: Vec<Condition>,
    pub open_questions: Vec<OpenQuestion>,
}

impl LocalUseLicense {
    /// Every way in which this record fails to cover `recipe` (the recipe's licence fields and effects included). Empty = it does.
    pub fn problems_for(&self, recipe: &Recipe) -> Vec<&'static str> {
        let mut p = Vec::new();
        if self.schema_version != SCHEMA_VERSION || self.scope != LOCAL_USE_SCOPE || self.decision != LOCAL_USE_DECISION {
            p.push("local_use_license_scope");
        }
        if self.tool_id != recipe.tool_id
            || self.version != recipe.version
            || self.platform_target != recipe.target.platform_target
            || self.artifact_sha256 != recipe.artifact.sha256
        {
            p.push("local_use_license_not_for_this_file");
        }
        if self.evidence_id != recipe.license.evidence_id
            || !self.evidence_id.starts_with('R')
            || self.spdx_expression != recipe.license.spdx_id
        {
            p.push("local_use_license_record_differs");
        }
        // the evaluated use is: the person's own copy of an upstream file, written into the manager's folder -- no hosting, no download by the
        // manager, no bundling. A recipe that downloads, or that does more than install/remove, is not what was evaluated.
        if !recipe.network.is_empty()
            || recipe.permissions.iter().any(|x| x == "network_download")
            || recipe.lifecycle_actions.iter().any(|a| a != "install" && a != "uninstall")
        {
            p.push("local_use_license_recipe_exceeds_scope");
        }
        let needed = ["C1", "C2", "C3", "C4", "C5"];
        if !needed.iter().all(|id| self.conditions.iter().any(|c| c.id == *id && !c.satisfied_by.is_empty())) {
            p.push("local_use_license_conditions_incomplete");
        }
        if self.open_questions.iter().any(|q| q.blocks_local_use)
            || !is_hex(&self.notice_bundle_sha256, 64)
            || !is_hex(&self.review_record_sha256, 64)
            || self.review_record_path.is_empty()
        {
            p.push("local_use_license_open_blocker");
        }
        p
    }
}

/// The evidence a recipe may be checked against. Production: the compiled-in list (empty). Tests and the candidate transition pass their own.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Evidence {
    pub attestations: Vec<SourceAttestation>,
    pub local_use_licenses: Vec<LocalUseLicense>,
}

impl Evidence {
    pub fn none() -> Evidence {
        Evidence::default()
    }

    pub fn production() -> Evidence {
        serde_json::from_str(BUILT_IN_EVIDENCE_JSON).unwrap_or_default()
    }

    pub fn attestation(&self, id: &str) -> Option<&SourceAttestation> {
        self.attestations.iter().find(|a| a.evidence_id == id)
    }

    pub fn local_use_license(&self, id: &str) -> Option<&LocalUseLicense> {
        self.local_use_licenses.iter().find(|l| l.evidence_id == id)
    }
}

/// Does the recipe's licence state grant REDISTRIBUTION rights? Only the plain `accepted` state does; `accepted_local_use` never does, so a
/// future distribution path that asks this question cannot be satisfied by the local-use pilot's review.
#[cfg_attr(not(test), allow(dead_code))]
pub fn redistribution_permitted(recipe: &Recipe) -> bool {
    recipe.license.state == "accepted"
}

#[cfg(test)]
#[path = "attestation_tests.rs"]
mod tests;
