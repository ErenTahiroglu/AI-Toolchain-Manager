//! Phase 3A recipe-registry tests: synthetic recipes only. No real tool has a reviewed recipe.

use super::*;
use crate::{
    catalog,
    discovery::reference::ReferenceDigest,
    plan::{self, parse_plan},
    synthetic::{self, testkit::*, Scenario},
};
use serde_json::json;

fn codes(p: &plan::InstallPlan) -> Vec<&str> {
    p.eligibility.blockers.iter().map(|b| b.code.as_str()).collect()
}

fn scenario() -> Scenario {
    let c = conn();
    Scenario::new(&base(&c), "synthetic-tool", "1.0.0", this_platform(), b"payload")
}

fn problems_after(edit: &dyn Fn(&mut ReviewedRecipe, &mut ReferenceSet), rereview: bool) -> Vec<&'static str> {
    let mut sc = scenario();
    edit(&mut sc.reviewed, &mut sc.refs);
    if rereview {
        synthetic::rereview(&mut sc.reviewed);
    }
    problems(&sc.reviewed, &sc.refs)
}

// ---- the real registry is empty ---------------------------------------------------------------------------------------

#[test]
fn the_built_in_recipe_list_and_the_authenticated_reference_set_are_empty_and_every_real_entry_stays_unapproved() {
    assert_eq!(BUILT_IN_RECIPES_JSON.trim(), "[]");
    assert_eq!(RecipeRegistry::production().len(), 0);
    let c = conn();
    assert!(ReferenceSet::production(&c).unwrap().authenticated.is_empty());
    let host = plan::HostFacts { platform: this_platform().to_string(), elevated: Some(false) };
    let mut n = 0;
    for e in catalog::list_catalog(&c).unwrap() {
        for platform in plan::PLATFORMS {
            for surface in plan::SURFACES {
                let r = plan::PlanRequest {
                    entry_id: e.entry_id.clone(),
                    platform_target: platform.to_string(),
                    agent_surface: surface.to_string(),
                    scope: "user".to_string(),
                    method_id: None,
                    project_alias: None,
                };
                let p = plan::plan_for(&c, &r, &host).unwrap();
                assert_eq!(p.recipe.state, "none", "{} {platform} {surface}", e.entry_id);
                assert!(codes(&p).contains(&"no_approved_recipe") && !codes(&p).contains(&"user_approval_missing"));
                n += 1;
            }
        }
    }
    assert!(n >= 100);
}

#[test]
fn only_synthetic_recipes_can_be_registered_outside_the_built_in_list() {
    let sc = scenario();
    assert!(RecipeRegistry::synthetic(vec![sc.reviewed.clone()]).is_ok());
    let mut real = sc.reviewed.clone();
    real.recipe.synthetic = false;
    assert!(RecipeRegistry::synthetic(vec![real]).is_err());
    let mut named_like_real = sc.reviewed.clone();
    named_like_real.recipe.tool_id = "gitleaks".to_string();
    assert!(RecipeRegistry::synthetic(vec![named_like_real]).is_err());
    let reg = RecipeRegistry::synthetic(vec![sc.reviewed.clone()]).unwrap();
    assert_eq!(reg.find("synthetic-tool", this_platform(), "local-cli", "user").len(), 1);
    assert!(reg.find("synthetic-tool", other_platform(), "local-cli", "user").is_empty());
    assert!(reg.find("synthetic-tool", this_platform(), "local-cli", "project").is_empty());
    assert!(reg.find("gitleaks", this_platform(), "local-cli", "user").is_empty());
}

// ---- validation: every missing or wrong piece of evidence is its own refusal ----------------------------------------------

#[test]
fn the_synthetic_recipe_is_valid_and_its_digest_follows_its_content() {
    let sc = scenario();
    assert!(problems(&sc.reviewed, &sc.refs).is_empty());
    let v = validate(&sc.reviewed, &sc.refs).unwrap();
    assert_eq!(v.digest().len(), 64);
    let mut other = sc.reviewed.clone();
    other.recipe.revision = 2;
    assert_ne!(recipe_digest(&other.recipe), v.digest());
    // permissions are part of what an approval binds
    let mut more = sc.reviewed.recipe.clone();
    more.permissions.push("network_download".to_string());
    let v2 = ValidatedRecipe { reviewed: ReviewedRecipe { recipe: more, review: sc.reviewed.review.clone() }, digest: String::new() };
    assert_ne!(v.permissions_digest(), v2.permissions_digest());
}

#[test]
fn a_recipe_without_independent_authentication_of_its_artifact_is_refused() {
    assert!(problems_after(&|_, refs| refs.authenticated.clear(), true).contains(&"artifact_not_authenticated"));
    // an UNauthenticated reference with the same digest does not count
    assert!(problems_after(
        &|_, refs| {
            let r = refs.authenticated.remove(0);
            refs.unauthenticated.push(r);
        },
        true
    )
    .contains(&"artifact_not_authenticated"));
    // an authenticated reference for another version, platform, tool, digest or format does not count
    type Edit = Box<dyn Fn(&mut ReferenceDigest)>;
    let variants: Vec<(&str, Edit)> = vec![
        ("version", Box::new(|r| r.version = "2.0.0".to_string())),
        ("platform", Box::new(|r| r.platform_target = if cfg!(windows) { MAC } else { WIN }.to_string())),
        ("tool", Box::new(|r| r.entry_id = "synthetic-other".to_string())),
        ("digest", Box::new(|r| r.sha256 = "f".repeat(64))),
        ("kind", Box::new(|r| r.kind = ArtifactKind::Archive)),
        ("format", Box::new(|r| r.format = "script-shebang".to_string())),
    ];
    for (name, edit) in variants {
        let p = problems_after(&|_, refs| edit(&mut refs.authenticated[0]), true);
        assert!(p.contains(&"artifact_not_authenticated"), "{name}: {p:?}");
    }
    // the recipe must also CLAIM authentication evidence of the right kind
    assert!(problems_after(&|rr, _| rr.recipe.authentication.retain(|e| e.kind != "authenticated_reference_digest"), true)
        .contains(&"artifact_not_authenticated"));
    assert!(problems_after(&|rr, _| rr.recipe.authentication[0].kind = "readme_says_so".to_string(), true).contains(&"authentication_kind"));
}

#[test]
fn conflicting_authenticated_references_for_the_same_version_make_the_recipe_unusable() {
    let p = problems_after(
        &|rr, refs| {
            let mut other = refs.authenticated[0].clone();
            other.sha256 = "a".repeat(64);
            refs.authenticated.push(other);
            let _ = rr;
        },
        true,
    );
    assert!(p.contains(&"authenticated_references_conflict"), "{p:?}");
}

#[test]
fn archive_digests_inexact_versions_and_unpinned_sources_are_refused() {
    assert!(problems_after(&|rr, _| rr.recipe.artifact.digest_scope = DigestScope::Archive, true).contains(&"digest_scope_not_exact_file"));
    for v in ["latest", "stable", "main", "^1.0.0", "~1", ">=1", "1.0 beta", "", "../1"] {
        assert!(problems_after(&|rr, _| rr.recipe.version = v.to_string(), true).contains(&"version_not_exact"), "{v:?}");
    }
    for c in ["z".repeat(40), "A".repeat(40), "a".repeat(39), "a".repeat(41), String::new(), "main".to_string()] {
        assert!(problems_after(&|rr, _| rr.recipe.source.immutable_commit = c.clone(), true).contains(&"source_not_immutable"), "{c:?}");
    }
    assert!(problems_after(&|rr, _| rr.recipe.source.repository = "not-a-repo".to_string(), true).contains(&"source_not_immutable"));
    assert!(problems_after(&|rr, _| rr.recipe.artifact.sha256 = "X".repeat(64), true).contains(&"artifact_identity"));
    assert!(problems_after(&|rr, _| rr.recipe.artifact.file_name = "../evil".to_string(), true).contains(&"artifact_identity"));
    assert!(problems_after(&|rr, _| rr.recipe.artifact.format = "pe-exe".to_string(), true).contains(&"artifact_format"));
    // a format that cannot exist on the target (a Mach-O file for Windows or the reverse)
    let wrong = if cfg!(windows) { "macho-arm64" } else { "pe-x64" };
    assert!(problems_after(&|rr, _| rr.recipe.artifact.format = wrong.to_string(), true).contains(&"artifact_format"));
}

#[test]
fn source_attestation_can_only_be_claimed_by_a_synthetic_recipe_through_synthetic_evidence() {
    let p = problems_after(&|rr, _| rr.recipe.authentication.retain(|e| e.kind != "synthetic_source_attestation"), true);
    assert!(p.contains(&"source_attestation_unverifiable"), "{p:?}");
    // a recipe that is not synthetic cannot claim it at all (and must also not use a synthetic tool id)
    let p = problems_after(&|rr, _| rr.recipe.synthetic = false, true);
    assert!(p.contains(&"source_attestation_unverifiable") && p.contains(&"synthetic_flag"), "{p:?}");
    // not claiming it is fine for validity; the plan then keeps its source blocker
    assert!(problems_after(
        &|rr, _| {
            rr.recipe.source.source_attested = false;
            rr.recipe.authentication.retain(|e| e.kind != "synthetic_source_attestation");
        },
        true
    )
    .is_empty());
}

#[test]
fn license_permissions_changes_network_lifecycle_and_rollback_contract_are_checked() {
    assert!(problems_after(&|rr, _| rr.recipe.license.state = "unknown".to_string(), true).contains(&"license_not_accepted"));
    assert!(problems_after(&|rr, _| rr.recipe.license.state = "blocked".to_string(), true).contains(&"license_not_accepted"));
    assert!(problems_after(&|rr, _| rr.recipe.permissions.push("run_as_administrator".to_string()), true).contains(&"permissions"));
    assert!(problems_after(&|rr, _| rr.recipe.permissions.retain(|p| p != "current_user_account"), true).contains(&"permissions"));
    assert!(problems_after(
        &|rr, _| rr.recipe.filesystem_changes.push(FsChange { area: "user_home".to_string(), kind: "replace".to_string() }),
        true
    )
    .contains(&"filesystem_changes"));
    assert!(problems_after(&|rr, _| rr.recipe.filesystem_changes.clear(), true).contains(&"filesystem_changes"));
    assert!(problems_after(&|rr, _| rr.recipe.network.push("anywhere".to_string()), true).contains(&"network"));
    // a declared destination needs the network permission
    assert!(problems_after(&|rr, _| rr.recipe.network.push("github_release_download".to_string()), true).contains(&"network"));
    assert!(problems_after(&|rr, _| rr.recipe.lifecycle_actions.push("self-update".to_string()), true).contains(&"lifecycle_actions"));
    assert!(problems_after(&|rr, _| rr.recipe.lifecycle_actions.clear(), true).contains(&"lifecycle_actions"));
    // "verified" rollback would need evidence from a real target: refused outright in this phase
    assert!(problems_after(&|rr, _| rr.recipe.rollback.state = "verified".to_string(), true).contains(&"rollback_contract"));
    assert!(problems_after(&|rr, _| rr.recipe.rollback.does_not_undo.clear(), true).contains(&"rollback_contract"));
    assert!(problems_after(&|rr, _| rr.recipe.rollback.undoes.clear(), true).contains(&"rollback_contract"));
    assert!(problems_after(&|rr, _| rr.recipe.rollback.undoes.push("everything".to_string()), true).contains(&"rollback_contract"));
}

#[test]
fn the_typed_steps_must_be_the_exact_template_bound_to_this_artifact() {
    let step = |rr: &mut ReviewedRecipe, i: usize, f: &dyn Fn(&mut Value)| f(&mut rr.recipe.steps[i]);
    assert!(
        problems_after(&|rr, _| step(rr, 1, &|v| v["expectedSha256"] = json!("b".repeat(64))), true).contains(&"steps"),
        "digest of another file"
    );
    assert!(
        problems_after(&|rr, _| step(rr, 0, &|v| v["url"] = json!("https://example.invalid/x")), true).contains(&"steps"),
        "extra parameter"
    );
    assert!(problems_after(&|rr, _| step(rr, 2, &|v| v["destinationId"] = json!("/etc")), true).contains(&"steps"));
    assert!(problems_after(&|rr, _| step(rr, 3, &|v| v["activationSlot"] = json!("native-plugin-manager")), true).contains(&"steps"));
    assert!(problems_after(&|rr, _| rr.recipe.steps.swap(0, 3), true).contains(&"steps"));
    assert!(problems_after(
        &|rr, _| {
            rr.recipe.steps.pop();
        },
        true
    )
    .contains(&"steps"));
    assert!(problems_after(
        &|rr, _| rr.recipe.steps.push(json!({ "operation": "register-native-plugin", "host": "local-cli", "pluginId": "x" })),
        true
    )
    .contains(&"steps"));
    assert!(problems_after(&|rr, _| rr.recipe.steps[0] = json!({ "operation": "run-shell", "command": "curl x | sh" }), true)
        .contains(&"steps"));
}

// ---- review is bound to content, and is not approval --------------------------------------------------------------------

#[test]
fn a_recipe_changed_after_review_a_draft_a_rejection_and_missing_staging_evidence_are_all_refused() {
    assert!(problems_after(&|rr, _| rr.recipe.revision = 7, false).contains(&"changed_after_review"));
    assert!(problems_after(&|rr, _| rr.recipe.permissions.push("network_download".to_string()), false).contains(&"changed_after_review"));
    assert!(problems_after(&|rr, _| rr.review.state = "draft".to_string(), true).contains(&"not_reviewed"));
    assert!(problems_after(&|rr, _| rr.review.state = "rejected".to_string(), true).contains(&"not_reviewed"));
    assert!(problems_after(&|rr, _| rr.review.staging_evidence.clear(), true).contains(&"staging_evidence"));
    assert!(problems_after(&|rr, _| rr.review.staging_evidence[0].independent = false, true).contains(&"staging_evidence"));
    assert!(problems_after(&|rr, _| rr.review.staging_evidence[0].state = "failed".to_string(), true).contains(&"staging_evidence"));
    assert!(
        problems_after(&|rr, _| rr.review.staging_evidence[0].evidence_digest = "short".to_string(), true).contains(&"staging_evidence")
    );
    // a reviewed digest copied from another recipe is not this recipe's review
    let other = scenario();
    let mut edited = other.reviewed.clone();
    edited.recipe.version = "2.0.0".to_string();
    assert!(problems(&edited, &other.refs).contains(&"changed_after_review"));
}

#[test]
fn recipes_deserialize_strictly_and_untrusted_catalog_text_is_not_a_recipe() {
    let sc = scenario();
    let mut v = serde_json::to_value(&sc.reviewed).unwrap();
    v["recipe"]["command"] = json!("rm -rf /");
    assert!(serde_json::from_value::<ReviewedRecipe>(v).is_err(), "unknown fields are refused");
    let mut v = serde_json::to_value(&sc.reviewed).unwrap();
    v["approval"] = json!({ "state": "approved" });
    assert!(serde_json::from_value::<ReviewedRecipe>(v).is_err(), "a recipe carries no approval");
    // catalog metadata dressed up as a recipe changes nothing: plans for real entries ignore it
    let c = conn();
    let mut e = catalog::get_entry(&c, "gitleaks").unwrap();
    e.record["installRecipe"] = serde_json::to_value(&sc.reviewed).unwrap();
    e.record["artifactEvidence"] = json!({ "sha256": sc.reviewed.recipe.artifact.sha256, "authenticated": true });
    e.record["installMethods"] = json!([{ "methodId": "verified-release-archive", "publisherPin": "1.0.0" }]);
    let host = plan::HostFacts { platform: WIN.to_string(), elevated: Some(false) };
    let p = plan::build_plan(&e, &synthetic::request("gitleaks", WIN), &host, &[]).unwrap();
    assert_eq!(p.recipe.state, "none");
    assert!(codes(&p).contains(&"no_approved_recipe"));
}

// ---- the plan that a validated recipe yields ------------------------------------------------------------------------------

#[test]
fn a_validated_synthetic_recipe_yields_a_complete_draft_plan_whose_only_missing_piece_is_the_user_approval() {
    let c = conn();
    let sc = Scenario::new(&base(&c), "synthetic-tool", "1.0.0", this_platform(), b"payload");
    let p = sc.plan();
    assert_eq!(codes(&p), vec!["user_approval_missing"]);
    assert_eq!(p.recipe.state, "validated");
    assert_eq!(p.recipe.recipe_digest.as_deref(), Some(recipe_digest(&sc.reviewed.recipe).as_str()));
    assert!(p.recipe.synthetic && p.recipe.rollback_state.as_deref() == Some("simulated"));
    assert_eq!((p.entry.source_pin.as_str(), p.recipe.authentication.as_str()), ("authenticated", "authenticated_reference"));
    assert_eq!(p.steps.len(), 4);
    assert!(p.steps.iter().all(|s| s.typed.is_some() && s.missing_parameters.is_empty()));
    assert_eq!(p.rollback.reliability, "simulated");
    assert!(!p.rollback.available, "a plan never claims a working rollback");
    // a draft, not an approval, and never executable
    assert_eq!(
        (p.approval_state.as_str(), p.state.as_str(), p.executable, p.eligibility.can_execute),
        ("not_given", "preview_only", false, false)
    );
    assert!(p.unknowns.iter().any(|u| u.contains("SENTETİK")));
    assert_eq!(parse_plan(&serde_json::to_string(&p).unwrap()).unwrap(), p);
    // determinism
    assert_eq!(p.plan_id, sc.plan().plan_id);
}

#[test]
fn an_unreviewed_or_unauthenticated_or_mismatched_recipe_leaves_the_plan_exactly_as_blocked_as_before() {
    let c = conn();
    let sc = Scenario::new(&base(&c), "synthetic-tool", "1.0.0", this_platform(), b"payload");
    let host = synthetic::host(this_platform());
    let ask = |rr: &ReviewedRecipe, refs: &ReferenceSet, platform: &str| {
        plan::build_plan_with(&sc.entry, &synthetic::request("synthetic-tool", platform), &synthetic::host(platform), &[], &[rr], refs)
            .unwrap()
    };
    let mut draft = sc.reviewed.clone();
    draft.review.state = "draft".to_string();
    let p = ask(&draft, &sc.refs, this_platform());
    assert_eq!(p.recipe.state, "invalid");
    assert!(p.recipe.problems.contains(&"not_reviewed".to_string()));
    assert!(codes(&p).contains(&"no_approved_recipe") && !codes(&p).contains(&"user_approval_missing"));
    assert!(p.steps.is_empty() || p.steps.iter().any(|s| s.typed.is_none()), "no recipe steps leak into an invalid plan");
    let p = ask(&sc.reviewed, &ReferenceSet::default(), this_platform());
    assert!(p.recipe.problems.contains(&"artifact_not_authenticated".to_string()));
    // a perfectly valid recipe for ANOTHER platform does not answer this question
    let other = Scenario::new(&base(&c), "synthetic-tool", "1.0.0", other_platform(), b"payload");
    let p = ask(&other.reviewed, &other.refs, this_platform());
    assert_eq!(p.recipe.problems, vec!["recipe_does_not_match_the_question".to_string()]);
    assert!(codes(&p).contains(&"no_approved_recipe"));
    // two candidates for one question are ambiguous
    let p = plan::build_plan_with(
        &sc.entry,
        &synthetic::request("synthetic-tool", this_platform()),
        &host,
        &[],
        &[&sc.reviewed, &sc.reviewed],
        &sc.refs,
    )
    .unwrap();
    assert_eq!(p.recipe.problems, vec!["ambiguous_recipes".to_string()]);
    // a quarantined catalog entry stays blocked whatever the recipe says
    let mut quarantined = sc.entry.clone();
    quarantined.review_state = "quarantined".to_string();
    let p =
        plan::build_plan_with(&quarantined, &synthetic::request("synthetic-tool", this_platform()), &host, &[], &[&sc.reviewed], &sc.refs)
            .unwrap();
    assert!(codes(&p).contains(&"entry_quarantined") && codes(&p).contains(&"user_approval_missing"));
}

#[test]
fn a_forged_recipe_status_in_a_serialized_plan_is_refused() {
    let sc = scenario();
    let p = sc.plan();
    let reject = |f: &dyn Fn(&mut Value)| {
        let mut v = serde_json::to_value(&p).unwrap();
        f(&mut v);
        parse_plan(&v.to_string()).unwrap_err()
    };
    assert_eq!(reject(&|v| v["recipe"]["recipeDigest"] = json!("0".repeat(64))), "plan_id_does_not_match_content");
    assert_eq!(reject(&|v| v["recipe"]["state"] = json!("none")), "recipe_status_inconsistent");
    assert_eq!(reject(&|v| v["recipe"]["recipeDigest"] = Value::Null), "recipe_status_inconsistent");
    assert_eq!(reject(&|v| v["approvalState"] = json!("given")), "plan_claims_approval");
    assert_eq!(reject(&|v| v["eligibility"]["blockers"] = json!([{ "code": "no_approved_recipe", "text": "x" }])), "blockers_inconsistent");
    assert_eq!(reject(&|v| v["rollback"]["reliability"] = json!("verified")), "rollback_inconsistent");
    assert_eq!(reject(&|v| v["steps"][1]["typed"] = Value::Null), "untyped_step_without_missing_parameters");
}

// ---- Phase 3D-1: the jq 1.8.2 / Windows x64 DRAFT recipe is data in the documentation folder and stays non-executable ----------------

const JQ_DRAFT_JSON: &str = include_str!("../../docs/phase-3d1/jq-1.8.2-windows-x64.recipe.draft.json");
// The jq digest is deliberately NOT written as a literal in any Rust source (a Phase 3D-0 guard test scans the sources for it, so that no
// reference digest can be added unnoticed): it is read from the draft, and the independent literal comparison lives in tests/test_phase3d1_contract.py.

fn jq_draft() -> ReviewedRecipe {
    serde_json::from_str(JQ_DRAFT_JSON).expect("the draft has exactly the reviewed-recipe shape (unknown fields are refused)")
}

fn no_refs() -> ReferenceSet {
    ReferenceSet { authenticated: vec![], unauthenticated: vec![] }
}

#[test]
fn the_jq_draft_recipe_pins_exactly_the_verified_file_and_nothing_else() {
    let d = jq_draft();
    let r = &d.recipe;
    assert_eq!(
        (r.tool_id.as_str(), r.version.as_str(), r.target.platform_target.as_str(), r.target.scope.as_str()),
        ("jq", "1.8.2", "windows-x64", "user")
    );
    assert_eq!((r.artifact.file_name.as_str(), r.artifact.size_bytes), ("jq-windows-amd64.exe", 1_035_264));
    assert_eq!(r.artifact.sha256.len(), 64);
    assert!(r.artifact.sha256.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
    assert_eq!((r.artifact.digest_scope, r.artifact.format.as_str()), (DigestScope::ExactFile, "pe-x64"));
    assert_eq!(r.source.repository, "jqlang/jq");
    assert_eq!(r.source.immutable_commit, "34f7186b86743a083a589741b6cea95293524108");
    assert!(!r.source.source_attested, "no verifier for real source attestations exists: the draft does not claim one");
    assert_eq!(
        r.steps[1].get("expectedSha256").and_then(Value::as_str),
        Some(r.artifact.sha256.as_str()),
        "the verify step binds the same digest"
    );
    assert!(!r.synthetic && !r.tool_id.starts_with("synthetic-"));
    // declared effects: manager folders only, one release download, no PATH / agent / package-manager / launch capability exists in the vocabulary
    assert_eq!(r.permissions, vec!["current_user_account", "network_download", "write_manager_folders"]);
    assert_eq!(r.network, vec!["github_release_download"]);
    assert!(r.filesystem_changes.iter().all(|c| matches!(c.area.as_str(), "manager_staging" | "manager_versions" | "manager_state")));
    assert_eq!(r.lifecycle_actions, vec!["install", "uninstall"], "no update path in the first pilot");
    assert!(
        r.rollback.does_not_undo.iter().any(|x| x == "tool_user_data") && r.rollback.state == "designed",
        "rollback is designed, never claimed verified"
    );
    for forbidden in ["path", "agent", "package", "launch", "execute", "shell", "script"] {
        let blob = serde_json::to_string(&r.permissions).unwrap()
            + &serde_json::to_string(&r.filesystem_changes).unwrap()
            + &serde_json::to_string(&r.steps).unwrap();
        assert!(!blob.to_lowercase().contains(forbidden), "{forbidden}");
    }
}

#[test]
fn the_jq_draft_is_blocked_by_exactly_the_missing_authentication_license_review_and_evidence() {
    let d = jq_draft();
    let p = problems(&d, &no_refs());
    assert_eq!(p, vec!["artifact_not_authenticated", "license_not_accepted", "not_reviewed", "staging_evidence"], "{p:?}");
    assert!(validate(&d, &no_refs()).is_err());
    // even with an authenticated reference for this exact file (a future, separately reviewed step) the draft stays blocked by licence and review
    let mut refs = no_refs();
    refs.authenticated.push(ReferenceDigest {
        entry_id: "jq".into(),
        kind: ArtifactKind::Executable,
        format: "pe-x64".into(),
        platform_target: "windows-x64".into(),
        version: "1.8.2".into(),
        sha256: d.recipe.artifact.sha256.clone(),
        source: "hypothetical reference used only by this test".into(),
    });
    let p = problems(&d, &refs);
    assert_eq!(p, vec!["license_not_accepted", "not_reviewed", "staging_evidence"], "{p:?}");
    // and the steps, target, permissions, filesystem effects, network and rollback contract are already acceptable to the validator
    for code in [
        "steps",
        "permissions",
        "filesystem_changes",
        "network",
        "rollback_contract",
        "lifecycle_actions",
        "artifact_format",
        "digest_scope_not_exact_file",
        "source_not_immutable",
    ] {
        assert!(!p.contains(&code), "{code}");
    }
}

#[test]
fn the_jq_draft_is_not_registered_not_compiled_in_and_cannot_be_registered_as_a_real_recipe() {
    assert_eq!(BUILT_IN_RECIPES_JSON.trim(), "[]");
    assert_eq!(RecipeRegistry::production().len(), 0);
    assert!(RecipeRegistry::synthetic(vec![jq_draft()]).is_err(), "only synthetic recipes may be registered outside the built-in list");
    // the draft file is read by this test only: no production source refers to it
    for (name, text) in [
        ("recipe.rs", include_str!("recipe.rs")),
        ("plan.rs", include_str!("plan.rs")),
        ("commands.rs", include_str!("commands.rs")),
        ("adapter.rs", include_str!("adapter.rs")),
        ("lifecycle.rs", include_str!("lifecycle.rs")),
        ("lib.rs", include_str!("lib.rs")),
    ] {
        assert!(!text.contains("recipe.draft.json") && !text.contains("phase-3d1"), "{name}");
    }
    let c = conn();
    assert!(ReferenceSet::production(&c).unwrap().authenticated.is_empty(), "the production authenticated-reference set stays empty");
}

#[test]
fn a_reviewed_looking_edit_of_the_draft_changes_its_digest_so_a_review_never_carries_over() {
    let d = jq_draft();
    let before = recipe_digest(&d.recipe);
    let mut edited = d.clone();
    edited.recipe.revision = 2;
    assert_ne!(recipe_digest(&edited.recipe), before);
    let mut claimed = d.clone();
    claimed.review.state = "reviewed".into();
    claimed.review.reviewed_recipe_digest = before.clone();
    claimed.review.reviewer_id = "somebody".into();
    claimed.review.staging_evidence =
        vec![StagingEvidence { check_id: "x1".into(), state: "passed".into(), independent: true, evidence_digest: "e".repeat(64) }];
    // claiming a review is not enough: licence and authentication still block it
    let p = problems(&claimed, &no_refs());
    assert_eq!(p, vec!["artifact_not_authenticated", "license_not_accepted"], "{p:?}");
}
