//! Phase 3D-3: the evidence records and the policy that reads them. The jq candidate files are read as data; nothing is registered, compiled in or
//! executed, and no jq byte is opened.

use super::*;
use crate::{
    discovery::reference::{ArtifactKind, ReferenceDigest, ReferenceSet},
    plan,
    recipe::{self, recipe_digest, ReviewedRecipe, StagingEvidence},
    synthetic::{self, testkit::*, Scenario},
};
use sha2::{Digest, Sha256};

const ATTESTATION: &str = include_str!("../../docs/phase-3d3/evidence/jq-1.8.2-windows-x64.source-attestation.json");
const LOCAL_USE: &str = include_str!("../../docs/phase-3d3/evidence/jq-1.8.2-windows-x64.local-use-license.json");
const CANDIDATE: &str = include_str!("../../docs/phase-3d3/jq-1.8.2-windows-x64.recipe.candidate.json");

fn attestation() -> SourceAttestation {
    serde_json::from_str(ATTESTATION).expect("the attestation record has exactly the record shape (unknown fields are refused)")
}

fn local_use() -> LocalUseLicense {
    serde_json::from_str(LOCAL_USE).expect("the local-use licence record has exactly the record shape")
}

fn candidate() -> ReviewedRecipe {
    serde_json::from_str(CANDIDATE).expect("the candidate has exactly the reviewed-recipe shape")
}

fn evidence() -> Evidence {
    Evidence { attestations: vec![attestation()], local_use_licenses: vec![local_use()] }
}

fn reference_for(r: &ReviewedRecipe) -> ReferenceSet {
    ReferenceSet {
        authenticated: vec![ReferenceDigest {
            entry_id: r.recipe.tool_id.clone(),
            kind: ArtifactKind::Executable,
            format: r.recipe.artifact.format.clone(),
            platform_target: r.recipe.target.platform_target.clone(),
            version: r.recipe.version.clone(),
            sha256: r.recipe.artifact.sha256.clone(),
            source: "hypothetical reference used only by this test".into(),
        }],
        unauthenticated: vec![],
    }
}

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn sha_of(rel: &str) -> String {
    let bytes = std::fs::read(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
    Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect()
}

// ---- the records are exactly what they claim to be ---------------------------------------------------------------------------------------------

#[test]
fn the_retained_evidence_hashes_in_the_attestation_are_the_hashes_of_the_retained_files() {
    let a = attestation();
    assert!(a.retained_evidence.len() >= 5);
    for f in &a.retained_evidence {
        assert_eq!(sha_of(&f.path), f.sha256, "{}", f.path);
    }
    assert_eq!(sha_of("docs/phase-3d0/evidence/request-a-session-20261018/session-sha256.txt"), a.retained_session_listing_sha256);
    // the retained session listing names the very artifact digest the record is about
    let listing = std::fs::read_to_string(root().join("docs/phase-3d0/evidence/request-a-session-20261018/session-sha256.txt")).unwrap();
    assert!(listing.contains(&a.artifact.sha256));
    // and the recorded network incident is not hidden
    assert!(!a.verification.network_policy_compliant);
    assert!(a.verification.network_incident.contains("not accepted") || a.verification.network_incident.contains("NOT accepted"));
    assert_eq!(a.verification.offline_replay_contacted_hosts, 0);
}

#[test]
fn the_local_use_record_points_at_the_review_it_rests_on_and_at_the_exact_notice_text() {
    let l = local_use();
    assert_eq!(sha_of(&l.review_record_path), l.review_record_sha256);
    assert_eq!(sha_of("docs/phase-3d2/notices/THIRD-PARTY-NOTICES.jq-1.8.2.windows-x64.txt"), l.notice_bundle_sha256);
    assert_eq!(l.scope, LOCAL_USE_SCOPE);
    assert_eq!(l.decision, LOCAL_USE_DECISION);
    // the legal questions stay open for redistribution and are never silently dropped
    assert!(l.open_questions.iter().all(|q| !q.blocks_local_use && q.blocks_redistribution));
    assert_eq!(l.open_questions.len(), 3);
}

#[test]
fn production_evidence_is_empty_and_nothing_names_jq() {
    let e = Evidence::production();
    assert!(e.attestations.is_empty() && e.local_use_licenses.is_empty());
    assert_eq!(BUILT_IN_EVIDENCE_JSON, r#"{"attestations":[],"localUseLicenses":[]}"#);
    // a record cannot be smuggled in with an unknown field
    assert!(serde_json::from_str::<Evidence>(r#"{"attestations":[],"localUseLicenses":[],"extra":1}"#).is_err());
    let mut v: serde_json::Value = serde_json::from_str(ATTESTATION).unwrap();
    v["verification"]["trustMe"] = serde_json::json!(true);
    assert!(serde_json::from_value::<SourceAttestation>(v).is_err());
}

// ---- the attestation binds every field -----------------------------------------------------------------------------------------------------------

#[test]
fn the_attestation_substantiates_the_candidate_and_every_binding_matters() {
    let (a, c) = (attestation(), candidate());
    assert!(a.problems_for(&c.recipe).is_empty());
    let mutate = |f: &dyn Fn(&mut ReviewedRecipe)| {
        let mut x = candidate();
        f(&mut x);
        a.problems_for(&x.recipe)
    };
    assert!(mutate(&|r| r.recipe.artifact.sha256 = "0".repeat(64)).contains(&"attestation_artifact_differs"));
    assert!(mutate(&|r| r.recipe.artifact.size_bytes += 1).contains(&"attestation_artifact_differs"));
    assert!(mutate(&|r| r.recipe.artifact.file_name = "jq-other.exe".into()).contains(&"attestation_artifact_differs"));
    assert!(mutate(&|r| r.recipe.artifact.format = "pe-x86".into()).contains(&"attestation_artifact_differs"));
    assert!(mutate(&|r| r.recipe.source.immutable_commit = "1".repeat(40)).contains(&"attestation_commit_differs"));
    assert!(mutate(&|r| r.recipe.source.repository = "someone/jq".into()).contains(&"attestation_repository_differs"));
    assert!(mutate(&|r| r.recipe.version = "1.8.3".into()).contains(&"attestation_not_for_this_recipe"));
    assert!(mutate(&|r| r.recipe.tool_id = "yq".into()).contains(&"attestation_not_for_this_recipe"));
    assert!(mutate(&|r| r.recipe.target.platform_target = "macos-arm64".into()).contains(&"attestation_not_for_this_recipe"));
    // and the record's own claims
    let broken = |f: &dyn Fn(&mut SourceAttestation)| {
        let mut x = attestation();
        f(&mut x);
        x.problems_for(&c.recipe)
    };
    assert!(broken(&|x| x.workflow.issuer = "https://example.invalid".into()).contains(&"attestation_workflow_identity"));
    assert!(broken(&|x| x.workflow.runner_environment = "self-hosted".into()).contains(&"attestation_workflow_identity"));
    assert!(broken(&|x| x.workflow.git_ref = "refs/heads/master".into()).contains(&"attestation_workflow_identity"));
    assert!(
        broken(&|x| x.workflow.run_url = "https://github.com/other/repo/actions/runs/1".into()).contains(&"attestation_workflow_identity")
    );
    assert!(broken(&|x| x.workflow.path = "tools/ci.yml".into()).contains(&"attestation_workflow_identity"));
    assert!(broken(&|x| x.verification.checks_failed = 1).contains(&"attestation_verification_incomplete"));
    assert!(broken(&|x| x.verification.official_verifier_passed = false).contains(&"attestation_verification_incomplete"));
    assert!(broken(&|x| x.verification.exact_bytes_verified = false).contains(&"attestation_verification_incomplete"));
    assert!(broken(&|x| x.verification.gpg_signature_valid = false).contains(&"attestation_signature_missing"));
    assert!(broken(&|x| x.verification.gpg_fingerprint = "nothex".into()).contains(&"attestation_signature_missing"));
    assert!(broken(&|x| x.verification.acceptance.clear()).contains(&"attestation_unreviewed"));
    assert!(broken(&|x| x.retained_evidence.truncate(2)).contains(&"attestation_evidence_not_retained"));
    assert!(broken(&|x| x.retained_session_listing_sha256 = "x".into()).contains(&"attestation_evidence_not_retained"));
    assert!(broken(&|x| x.schema_version = 9).contains(&"attestation_schema"));
}

#[test]
fn a_recipe_cannot_claim_a_source_attestation_without_a_substantiating_record() {
    let c = candidate();
    let refs = reference_for(&c);
    // no record at all (the production state): the claim is refused
    assert!(recipe::problems(&c, &refs).contains(&"source_attestation_unverifiable"));
    assert!(recipe::problems_with(&c, &refs, &Evidence::none()).contains(&"source_attestation_unverifiable"));
    // a record that exists but names another file / another id
    let mut other = evidence();
    other.attestations[0].artifact.sha256 = "0".repeat(64);
    assert!(recipe::problems_with(&c, &refs, &other).contains(&"source_attestation_unverifiable"));
    let mut renamed = candidate();
    renamed.recipe.authentication[1].evidence_id = "some-other-record".into();
    assert!(recipe::problems_with(&renamed, &refs, &evidence()).contains(&"source_attestation_unverifiable"));
    // a recipe that merely says it is attested, with no authentication entry of that kind
    let mut bare = candidate();
    bare.recipe.authentication.retain(|e| e.kind != "source_attestation_evidence");
    assert!(recipe::problems_with(&bare, &refs, &evidence()).contains(&"source_attestation_unverifiable"));
    // with the substantiating record the claim holds
    assert!(!recipe::problems_with(&c, &refs, &evidence()).contains(&"source_attestation_unverifiable"));
    // a SYNTHETIC recipe cannot use the real evidence kind to claim an attestation, and a real one cannot use the synthetic kind
    let sc = Scenario::new(&base(&conn()), "synthetic-tool", "1.0.0", this_platform(), b"payload");
    let mut s = sc.reviewed.clone();
    s.recipe.authentication[1].kind = "source_attestation_evidence".into();
    assert!(recipe::problems_with(&s, &sc.refs, &evidence()).contains(&"source_attestation_unverifiable"));
    let mut real_synth_kind = candidate();
    real_synth_kind.recipe.authentication[1].kind = "synthetic_source_attestation".into();
    assert!(recipe::problems_with(&real_synth_kind, &refs, &evidence()).contains(&"source_attestation_unverifiable"));
}

// ---- the local-use licence ---------------------------------------------------------------------------------------------------------------------------

#[test]
fn local_use_acceptance_needs_a_covering_record_and_never_means_redistribution() {
    let c = candidate();
    let refs = reference_for(&c);
    assert_eq!(c.recipe.license.state, "accepted_local_use");
    assert!(recipe::problems(&c, &refs).contains(&"license_not_accepted"), "no record: refused");
    assert!(!recipe::problems_with(&c, &refs, &evidence()).contains(&"license_not_accepted"));
    // every way the record can stop covering the recipe
    let refuse = |f: &dyn Fn(&mut LocalUseLicense), g: &dyn Fn(&mut ReviewedRecipe)| {
        let mut e = evidence();
        f(&mut e.local_use_licenses[0]);
        let mut r = candidate();
        g(&mut r);
        recipe::problems_with(&r, &refs, &e).contains(&"license_not_accepted")
    };
    let keep = |_: &mut ReviewedRecipe| {};
    assert!(refuse(&|l| l.artifact_sha256 = "0".repeat(64), &keep));
    assert!(refuse(&|l| l.version = "1.8.3".into(), &keep));
    assert!(refuse(&|l| l.scope = "redistribution".into(), &keep));
    assert!(refuse(&|l| l.decision = "LICENSE_PASS".into(), &keep));
    assert!(refuse(&|l| l.spdx_expression = "MIT".into(), &keep));
    assert!(refuse(&|l| l.evidence_id = "R35".into(), &keep));
    assert!(refuse(&|l| l.open_questions[0].blocks_local_use = true, &keep));
    assert!(refuse(&|l| l.conditions.retain(|c| c.id != "C4"), &keep));
    assert!(refuse(&|l| l.notice_bundle_sha256 = "zz".into(), &keep));
    // the recipe may not exceed what was evaluated: no download, no update
    assert!(refuse(&|_| {}, &|r| r.recipe.network = vec!["github_release_download".into()]));
    assert!(refuse(&|_| {}, &|r| r.recipe.permissions.push("network_download".into())));
    assert!(refuse(&|_| {}, &|r| r.recipe.lifecycle_actions.push("update".into())));
    // a synthetic recipe cannot use it either
    assert!(refuse(&|_| {}, &|r| r.recipe.synthetic = true));
    // the state itself: anything but the two known values is refused, and the narrow one never grants redistribution
    assert!(refuse(&|_| {}, &|r| r.recipe.license.state = "unknown".into()));
    assert!(!redistribution_permitted(&c.recipe));
    let mut plain = candidate();
    plain.recipe.license.state = "accepted".into();
    assert!(redistribution_permitted(&plain.recipe));
}

// ---- the whole transition, on copies ---------------------------------------------------------------------------------------------------------------

#[test]
fn with_the_reviewed_evidence_the_candidate_has_exactly_two_blockers_left_and_the_plan_loses_the_source_blocker() {
    let mut c = candidate();
    let refs = reference_for(&c);
    let ev = evidence();
    // what is left is review and staging evidence: only an independent reviewer can supply them
    assert_eq!(recipe::problems_with(&c, &refs, &ev), vec!["not_reviewed", "staging_evidence"]);
    // (a test copy) after an independent review of THIS content and one independent staging check the recipe validates ...
    c.review.state = "reviewed".into();
    c.review.reviewer_id = "independent-reviewer-1".into();
    c.review.reviewed_recipe_digest = recipe_digest(&c.recipe);
    c.review.staging_evidence = vec![StagingEvidence {
        check_id: "staging-check-1".into(),
        state: "passed".into(),
        independent: true,
        evidence_digest: "e".repeat(64),
    }];
    let valid = recipe::validate_with(&c, &refs, &ev).expect("validates once reviewed");
    // ... and the plan's ONLY blocker is the user's approval: `source_origin_unverified` is gone because real evidence substantiates it
    let entry = synthetic::catalog_entry(&base(&conn()), "jq");
    let request = plan::PlanRequest {
        entry_id: "jq".into(),
        platform_target: "windows-x64".into(),
        agent_surface: "local-cli".into(),
        scope: "user".into(),
        method_id: None,
        project_alias: None,
    };
    let host = plan::HostFacts { platform: "windows-x64".into(), elevated: Some(false) };
    let p = plan::build_plan_with_evidence(&entry, &request, &host, &[], &[&c], &refs, &ev).unwrap();
    let codes: Vec<&str> = p.eligibility.blockers.iter().map(|b| b.code.as_str()).collect();
    assert_eq!(codes, vec!["user_approval_missing"], "{codes:?}");
    assert_eq!(p.entry.source_pin, "authenticated");
    assert!(!p.network.planned_use && p.network.destinations.is_empty());
    assert_eq!(valid.recipe().license.state, "accepted_local_use");
    // without the evidence the same reviewed recipe is invalid again, and the plan says why
    let none = plan::build_plan_with_evidence(&entry, &request, &host, &[], &[&c], &refs, &Evidence::none()).unwrap();
    assert_eq!(none.recipe.state, "invalid");
    assert!(
        none.recipe.problems.iter().any(|x| x == "source_attestation_unverifiable")
            && none.recipe.problems.iter().any(|x| x == "license_not_accepted")
    );
    // a later edit of the recipe invalidates the review AND is not covered by the records
    let mut edited = c.clone();
    edited.recipe.revision = 3;
    assert!(recipe::problems_with(&edited, &refs, &ev).contains(&"changed_after_review"));
    // the compiled-in lists stay empty
    assert_eq!(recipe::BUILT_IN_RECIPES_JSON.trim(), "[]");
    assert!(recipe::RecipeRegistry::production().len() == 0);
}

#[test]
fn the_candidate_is_the_draft_plus_exactly_the_agreed_changes() {
    let (c, d): (ReviewedRecipe, ReviewedRecipe) =
        (candidate(), serde_json::from_str(include_str!("../../docs/phase-3d1/jq-1.8.2-windows-x64.recipe.draft.json")).unwrap());
    // identity of the file, the source pin, the steps and the rollback contract are untouched
    assert_eq!(c.recipe.artifact, d.recipe.artifact);
    assert_eq!(c.recipe.source.immutable_commit, d.recipe.source.immutable_commit);
    assert_eq!(c.recipe.source.repository, d.recipe.source.repository);
    assert_eq!(c.recipe.steps, d.recipe.steps);
    assert_eq!(c.recipe.rollback, d.recipe.rollback);
    assert_eq!(c.recipe.target, d.recipe.target);
    assert_eq!(c.recipe.lifecycle_actions, vec!["install", "uninstall"]);
    // what changed: attested source, evidence kinds, local-use licence, no network/download, no staging-removal entry (staging is removed by the adapter itself)
    assert!(c.recipe.source.source_attested && !d.recipe.source.source_attested);
    assert!(c.recipe.network.is_empty() && !c.recipe.permissions.iter().any(|p| p == "network_download"));
    assert_eq!(c.recipe.revision, 2);
    // the review is NOT claimed: only an independent reviewer may set it
    assert_eq!(c.review.state, "draft");
    assert!(c.review.staging_evidence.is_empty());
    assert!(!c.recipe.synthetic);
}
