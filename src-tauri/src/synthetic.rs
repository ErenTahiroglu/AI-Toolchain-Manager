//! Fully synthetic fixtures for the Phase 3A recipe / approval / lifecycle tests and the headless self-check.
//!
//! Everything here is made up: a tool called `synthetic-<name>`, an "artifact" that is a few bytes behind a synthetic native
//! header, a repository that does not exist, a reviewer that is a test label. No real tool, URL, digest or reference appears,
//! and nothing here is reachable from a command.
#![allow(dead_code)] // used by the tests and by the self-check probe only

use crate::{
    catalog::CatalogEntry,
    discovery::reference::{self, ArtifactKind, ReferenceDigest, ReferenceSet},
    plan::{self, HostFacts, InstallPlan, PlanRequest},
    recipe::{
        self, AuthEvidence, DigestScope, FsChange, Recipe, RecipeArtifact, RecipeLicense, RecipeSource, RecipeTarget, ReviewRecord,
        ReviewedRecipe, RollbackContract, StagingEvidence, ValidatedRecipe,
    },
};
use serde_json::{json, Value};

pub const SYNTHETIC_COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// The synthetic artifact for `platform`: a native header for that platform followed by `payload`.
pub fn artifact_bytes(platform: &str, payload: &[u8]) -> Vec<u8> {
    reference::synthetic_file(reference::native_executable_format(platform), payload)
}

pub fn sha256_of(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// A synthetic authenticated reference (what a future, independently authenticated source would provide).
pub fn reference_for(tool: &str, version: &str, platform: &str, bytes: &[u8]) -> ReferenceDigest {
    ReferenceDigest {
        entry_id: tool.to_string(),
        kind: ArtifactKind::Executable,
        format: reference::native_executable_format(platform).as_str().to_string(),
        platform_target: platform.to_string(),
        version: version.to_string(),
        sha256: sha256_of(bytes),
        source: "synthetic test fixture".to_string(),
    }
}

/// A reviewed synthetic recipe for installing `bytes` as `tool` `version` on `platform` (surface `local-cli`, user scope).
pub fn reviewed_recipe(tool: &str, version: &str, platform: &str, bytes: &[u8]) -> ReviewedRecipe {
    let sha = sha256_of(bytes);
    let recipe = Recipe {
        schema_version: recipe::RECIPE_SCHEMA_VERSION,
        recipe_id: format!("{tool}-{version}-{platform}"),
        revision: 1,
        synthetic: true,
        tool_id: tool.to_string(),
        version: version.to_string(),
        target: RecipeTarget { platform_target: platform.to_string(), agent_surface: "local-cli".to_string(), scope: "user".to_string() },
        source: RecipeSource {
            repository: "synthetic/example".to_string(),
            immutable_commit: SYNTHETIC_COMMIT.to_string(),
            source_attested: true,
        },
        artifact: RecipeArtifact {
            artifact_id: "primary".to_string(),
            file_name: "artifact.bin".to_string(),
            size_bytes: bytes.len() as u64,
            sha256: sha.clone(),
            digest_scope: DigestScope::ExactFile,
            format: reference::native_executable_format(platform).as_str().to_string(),
            origin: None,
        },
        authentication: vec![
            AuthEvidence { kind: "authenticated_reference_digest".to_string(), evidence_id: "ref-synthetic-1".to_string() },
            AuthEvidence { kind: "synthetic_source_attestation".to_string(), evidence_id: "att-synthetic-1".to_string() },
        ],
        license: RecipeLicense { state: "accepted".to_string(), spdx_id: "MIT".to_string(), evidence_id: "R14".to_string() },
        permissions: vec!["current_user_account".to_string(), "write_manager_folders".to_string()],
        filesystem_changes: vec![
            FsChange { area: "manager_staging".to_string(), kind: "create".to_string() },
            FsChange { area: "manager_versions".to_string(), kind: "create".to_string() },
            FsChange { area: "manager_state".to_string(), kind: "replace".to_string() },
        ],
        network: vec![],
        lifecycle_actions: vec!["install".to_string(), "update".to_string(), "uninstall".to_string()],
        steps: steps(&sha),
        rollback: RollbackContract {
            state: "simulated".to_string(),
            undoes: ["pointer_restore", "staging_removal", "version_folder_removal", "removed_version_restore"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            does_not_undo: vec!["tool_user_data".to_string(), "external_effects".to_string()],
        },
    };
    let review = ReviewRecord {
        state: "reviewed".to_string(),
        reviewer_id: "synthetic-reviewer-1".to_string(),
        reviewed_recipe_digest: recipe::recipe_digest(&recipe),
        staging_evidence: vec![StagingEvidence {
            check_id: "synthetic-stage-check".to_string(),
            state: "passed".to_string(),
            independent: true,
            evidence_digest: "e".repeat(64),
        }],
    };
    ReviewedRecipe { recipe, review }
}

pub fn steps(sha: &str) -> Vec<Value> {
    vec![
        json!({ "operation": "fetch-release-artifact", "artifactId": "primary" }),
        json!({ "operation": "verify-artifact-digest", "artifactId": "primary", "expectedSha256": sha }),
        json!({ "operation": "stage-private-copy", "artifactId": "primary", "destinationId": "manager-owned-staging" }),
        json!({ "operation": "activate-staged-artifact", "artifactId": "primary", "activationSlot": "manager-versioned-binary", "activationMethod": "atomic-version-switch" }),
    ]
}

/// Re-reviews a recipe after a deliberate edit (the way a reviewer would): the review then covers the new content.
pub fn rereview(rr: &mut ReviewedRecipe) {
    rr.review.reviewed_recipe_digest = recipe::recipe_digest(&rr.recipe);
}

/// A synthetic catalog entry for `tool` (never a real catalog entry), built from a seeded one so every column is present.
pub fn catalog_entry(base: &CatalogEntry, tool: &str) -> CatalogEntry {
    let mut e = base.clone();
    e.entry_id = tool.to_string();
    e.display_name = format!("Synthetic {tool}");
    e.purpose = "Synthetic test fixture.".to_string();
    e.source_key = format!("synthetic/{tool}");
    e.repo_url = "https://example.invalid/synthetic/example".to_string();
    e.plugin_identity = None;
    e.review_state = "unreviewed".to_string();
    e.execution_policy = "manual-only".to_string();
    e.removed_at = None;
    e.record = json!({
        "repository": { "owner": "synthetic", "name": "example" },
        "installMethods": [], "targetEvidence": [], "dependencies": [],
        "sourceSnapshot": { "immutableCommit": null }, "privacy": { "telemetry": "none-declared", "mayTransmitProjectContent": false }
    });
    e
}

pub fn request(tool: &str, platform: &str) -> PlanRequest {
    PlanRequest {
        entry_id: tool.to_string(),
        platform_target: platform.to_string(),
        agent_surface: "local-cli".to_string(),
        scope: "user".to_string(),
        method_id: None,
        project_alias: None,
    }
}

pub fn host(platform: &str) -> HostFacts {
    HostFacts { platform: platform.to_string(), elevated: Some(false) }
}

/// Everything one synthetic scenario needs.
pub struct Scenario {
    pub platform: String,
    pub tool: String,
    pub entry: CatalogEntry,
    pub refs: ReferenceSet,
    pub reviewed: ReviewedRecipe,
    pub bytes: Vec<u8>,
}

impl Scenario {
    pub fn new(base_entry: &CatalogEntry, tool: &str, version: &str, platform: &str, payload: &[u8]) -> Scenario {
        let bytes = artifact_bytes(platform, payload);
        Scenario {
            platform: platform.to_string(),
            tool: tool.to_string(),
            entry: catalog_entry(base_entry, tool),
            refs: ReferenceSet { authenticated: vec![reference_for(tool, version, platform, &bytes)], unauthenticated: vec![] },
            reviewed: reviewed_recipe(tool, version, platform, &bytes),
            bytes,
        }
    }

    pub fn validated(&self) -> ValidatedRecipe {
        recipe::validate(&self.reviewed, &self.refs).expect("the synthetic recipe is valid")
    }

    pub fn plan(&self) -> InstallPlan {
        plan::build_plan_with(&self.entry, &request(&self.tool, &self.platform), &host(&self.platform), &[], &[&self.reviewed], &self.refs)
            .unwrap()
    }
}

/// Creates and verifies a synthetic approval in one step (self-check probe). Production policy would refuse every actor.
#[allow(clippy::too_many_arguments)]
pub fn approve_and_verify(
    conn: &rusqlite::Connection,
    valid: &ValidatedRecipe,
    plan: &InstallPlan,
    platform: &str,
    action: crate::approval::Action,
    from: Option<&str>,
    computer: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<crate::approval::VerifiedApproval, crate::approval::ApprovalError> {
    use crate::approval::{self, Actor, ApprovalPolicy, Situation, UserAction};
    let s = Situation { plan, recipe: valid, action, from_version: from, computer_id: computer, host_platform: platform, now, conn };
    let r = valid.recipe();
    let confirm = UserAction { confirmed_text: approval::expected_confirmation(action, &r.tool_id, &r.version, &r.target.platform_target) };
    let a = approval::approve(&s, None, Actor::synthetic("synthetic-probe-user"), &confirm, ApprovalPolicy::synthetic_only(), 600)?;
    approval::verify_approval(&a, &s, ApprovalPolicy::synthetic_only())
}

/// Shared helpers for the Phase 3A tests.
#[cfg(test)]
pub mod testkit {
    use super::*;
    use crate::{
        approval::{self, Action, Actor, Approval, ApprovalPolicy, Situation, UserAction, VerifiedApproval},
        catalog, db,
    };
    use chrono::{DateTime, TimeZone, Utc};
    use rusqlite::Connection;

    pub const WIN: &str = "windows-x64";
    pub const MAC: &str = "macos-arm64";

    pub fn this_platform() -> &'static str {
        if cfg!(windows) {
            WIN
        } else {
            MAC
        }
    }

    pub fn other_platform() -> &'static str {
        if cfg!(windows) {
            MAC
        } else {
            WIN
        }
    }

    pub fn conn() -> Connection {
        let c = db::open_memory().unwrap();
        catalog::seed_all(&c).unwrap();
        crate::projects::add_reference(&c, "proje-a", "Proje A", "").unwrap();
        c
    }

    pub fn base(c: &Connection) -> CatalogEntry {
        catalog::get_entry(c, "gitleaks").unwrap()
    }

    pub fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2027, 1, 1, 12, 0, 0).unwrap()
    }

    /// One synthetic scenario with its database, validated recipe and (blocked, draft) plan.
    pub struct Fx {
        pub conn: Connection,
        pub sc: Scenario,
        pub plan: InstallPlan,
        pub valid: ValidatedRecipe,
    }

    pub fn fx(tool: &str, version: &str, payload: &[u8]) -> Fx {
        fx_on(this_platform(), tool, version, payload)
    }

    pub fn fx_on(platform: &str, tool: &str, version: &str, payload: &[u8]) -> Fx {
        let conn = conn();
        let sc = Scenario::new(&base(&conn), tool, version, platform, payload);
        let plan = sc.plan();
        let valid = sc.validated();
        Fx { conn, sc, plan, valid }
    }

    pub fn situation<'a>(f: &'a Fx, action: Action, from: Option<&'a str>, computer: &'a str, now: DateTime<Utc>) -> Situation<'a> {
        Situation {
            plan: &f.plan,
            recipe: &f.valid,
            action,
            from_version: from,
            computer_id: computer,
            host_platform: &f.sc.platform,
            now,
            conn: &f.conn,
        }
    }

    pub fn confirm(action: Action, f: &Fx) -> UserAction {
        let r = f.valid.recipe();
        UserAction { confirmed_text: approval::expected_confirmation(action, &r.tool_id, &r.version, &r.target.platform_target) }
    }

    pub fn make_approval(f: &Fx, action: Action, from: Option<&str>, computer: &str, now: DateTime<Utc>) -> Approval {
        approval::approve(
            &situation(f, action, from, computer, now),
            None,
            Actor::synthetic("synthetic-user-1"),
            &confirm(action, f),
            ApprovalPolicy::synthetic_only(),
            600,
        )
        .expect("approval")
    }

    pub fn verified(f: &Fx, a: &Approval, action: Action, from: Option<&str>, computer: &str, now: DateTime<Utc>) -> VerifiedApproval {
        approval::verify_approval(a, &situation(f, action, from, computer, now), ApprovalPolicy::synthetic_only()).expect("verified")
    }
}
