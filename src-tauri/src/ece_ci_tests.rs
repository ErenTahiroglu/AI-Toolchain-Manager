//! ATM-GOV-02 P2A Gate 3: adversarial, synthetic, offline tests of `ece_ci`.
//!
//! Every verdict below comes from the REAL ECE evaluator (`evaluate_github_evidence_with_store`) fed by an in-memory
//! `MockGitHubProvider`. Nothing here opens a socket, reads a token, touches a file other than this repository's own sources,
//! or starts a process. The expected values are written out, not computed with the logic under test.

use crate::ece_ci::*;
use ece_lib::github::models::{
    GitHubArtifact, GitHubCommit, GitHubCommitDetails, GitHubJob, GitHubRepository, GitHubRunRepo, GitHubUser, GitHubWorkflowRun,
};
use ece_lib::github::{
    evaluate_github_evidence_with_store, DeclaredArtifactSpec, GitHubClientError, MockGitHubProvider, TrustedCheckPolicy,
};
use ece_lib::SchemaStore;
use serde_json::{json, Value};

/// The historical, green run this repository's CI evidence pilot already observed (dd37b04, run 41217731446, attempt 1).
const COMMIT: &str = "dd37b04f730569c3a8ad1b929ee6b80052ef22e2";
const NEWER_COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const RUN_ID: u64 = 41_217_731_446;

fn request() -> CiEvidenceRequest {
    CiEvidenceRequest::new(COMMIT, RUN_ID, 1).unwrap()
}

fn repo(id: u64) -> GitHubRepository {
    GitHubRepository {
        id,
        name: REPOSITORY_NAME.to_string(),
        full_name: REPOSITORY_FULL_NAME.to_string(),
        owner: GitHubUser { login: REPOSITORY_OWNER.to_string(), id: 149_682_795 },
        default_branch: Some("main".to_string()),
    }
}

fn commit(sha: &str, message: &str) -> GitHubCommit {
    GitHubCommit { sha: sha.to_string(), commit: GitHubCommitDetails { message: Some(message.to_string()), author: None } }
}

fn run(head_sha: &str, attempt: u32) -> GitHubWorkflowRun {
    GitHubWorkflowRun {
        id: RUN_ID,
        name: Some("CI".to_string()),
        head_sha: head_sha.to_string(),
        path: Some(WORKFLOW_PATH.to_string()),
        run_attempt: attempt,
        event: Some("push".to_string()),
        status: "completed".to_string(),
        conclusion: Some("success".to_string()),
        repository: Some(GitHubRunRepo { id: REPOSITORY_ID, full_name: REPOSITORY_FULL_NAME.to_string() }),
    }
}

fn job(id: u64, name: &str, status: &str, conclusion: Option<&str>) -> GitHubJob {
    GitHubJob {
        id,
        name: name.to_string(),
        status: status.to_string(),
        conclusion: conclusion.map(str::to_string),
        started_at: None,
        completed_at: None,
    }
}

fn green_jobs() -> Vec<GitHubJob> {
    vec![job(1, "windows-x64", "completed", Some("success")), job(2, "macos-arm64", "completed", Some("success"))]
}

fn healthy() -> MockGitHubProvider {
    let mut p = MockGitHubProvider::new();
    p.repo_result = Some(Ok(repo(REPOSITORY_ID)));
    p.commit_result = Some(Ok(commit(COMMIT, "synthetic commit")));
    p.run_result = Some(Ok(run(COMMIT, 1)));
    p.jobs_result = Some(Ok(green_jobs()));
    p.artifacts_result = Some(Ok(Vec::new()));
    p
}

fn jobs(list: Vec<GitHubJob>) -> MockGitHubProvider {
    let mut p = healthy();
    p.jobs_result = Some(Ok(list));
    p
}

/// What EVERY result must satisfy, whatever the evidence says: nothing a machine produces can be review or authorization.
fn assert_never_accepted(r: &CiEvidenceReport) {
    assert!(matches!(r.review, ReviewState::Unreviewed | ReviewState::NotEvaluated), "{r:?}");
    assert_eq!(r.authorization, AuthorizationState::AuthorizationPending, "{r:?}");
    assert!(matches!(r.freshness, Freshness::HistoricalBaseline | Freshness::NotEvaluated), "{r:?}");
    assert!(r.limitations.contains(&Limitation::NoIndependentReview), "{r:?}");
    assert!(r.limitations.contains(&Limitation::NoHumanAuthorization), "{r:?}");
    assert!(r.limitations.contains(&Limitation::NotCurrentHeadAttestation), "{r:?}");
    assert_eq!(r.provenance.ece_revision, ECE_REVISION);
    assert_eq!(r.provenance.policy_file_sha256, POLICY_FILE_SHA256);
    assert_eq!(r.provenance.repository_id, REPOSITORY_ID);
}

fn assert_fails(r: &CiEvidenceReport, outcome: Outcome, class: BlockingClass) {
    assert_never_accepted(r);
    assert_eq!(r.outcome, outcome, "{r:?}");
    assert!(r.blocking.contains(&class), "expected {class:?} in {r:?}");
    assert!(!r.machine_ci_verified(), "{r:?}");
}

// ------------------------------------------------------------------------------------------------------------------
// The positive case and the meaning of the vocabulary
// ------------------------------------------------------------------------------------------------------------------

#[test]
fn correct_historical_evidence_is_verified_pass_with_limitations_and_nothing_more() {
    let r = assess_ci_evidence(&healthy(), &request());
    assert_never_accepted(&r);
    assert_eq!(r.outcome, Outcome::AssessedLimitations);
    assert_eq!(r.ci, CiResult::VerifiedPass);
    assert_eq!(r.review, ReviewState::Unreviewed);
    assert_eq!(r.availability, EvidenceAvailability::NotProvided);
    assert!(r.blocking.is_empty() && r.error.is_none());
    assert!(r.machine_ci_verified());
    assert_eq!(r.provenance.commit_sha, COMMIT);
    assert_eq!((r.provenance.run_id, r.provenance.run_attempt), (RUN_ID, 1));
    assert_eq!(r.provenance.repository, "ErenTahiroglu/AI-Toolchain-Manager");
    assert_eq!(r.provenance.workflow_path, ".github/workflows/ci.yml");
    assert_eq!(r.provenance.policy_id, "atm-ci-two-runner-v1");
    // The envelope digest was not part of this run assessment, and ECE says so; the signature is never checked.
    assert!(r.limitations.contains(&Limitation::StandaloneRunAssessment));
    assert!(r.limitations.contains(&Limitation::CommitSignatureNotVerified));
}

#[test]
fn the_result_is_deterministic_apart_from_ece_ids_and_timestamps_which_it_does_not_carry() {
    let a = assess_ci_evidence(&healthy(), &request());
    let b = assess_ci_evidence(&healthy(), &request());
    assert_eq!(a, b);
}

#[test]
fn historical_evidence_is_never_a_statement_about_the_current_head() {
    // The repository has moved on: HEAD is a newer commit and the latest attempt of the run is 2. The request is still for the
    // historical commit, attempt 1. The evaluator never asks for HEAD and the result says so.
    let mut p = healthy();
    p.run_result = Some(Ok(run(COMMIT, 2)));
    let r = assess_ci_evidence(&p, &request());
    assert_eq!(r.outcome, Outcome::AssessedLimitations);
    assert_eq!(r.freshness, Freshness::HistoricalBaseline);
    assert!(r.limitations.contains(&Limitation::NotCurrentHeadAttestation));
    assert_never_accepted(&r);
}

#[test]
fn a_run_that_belongs_to_a_newer_head_does_not_verify_the_historical_commit() {
    let mut p = healthy();
    p.run_result = Some(Ok(run(NEWER_COMMIT, 1)));
    assert_fails(&assess_ci_evidence(&p, &request()), Outcome::AssessedFail, BlockingClass::CommitNotVerified);
}

// ------------------------------------------------------------------------------------------------------------------
// Identity, commit, workflow, attempt
// ------------------------------------------------------------------------------------------------------------------

#[test]
fn a_wrong_numeric_repository_id_fails() {
    for wrong in [1_432_704_795_u64, 1, REPOSITORY_ID + 1, REPOSITORY_ID - 1] {
        let mut p = healthy();
        p.repo_result = Some(Ok(repo(wrong)));
        assert_fails(&assess_ci_evidence(&p, &request()), Outcome::AssessedFail, BlockingClass::RepositoryIdentityMismatch);
    }
}

#[test]
fn a_run_from_another_repository_fails() {
    let mut p = healthy();
    let mut other = run(COMMIT, 1);
    other.repository = Some(GitHubRunRepo { id: 1_432_704_795, full_name: "ErenTahiroglu/Evidence-Checkpoint-Engine".to_string() });
    p.run_result = Some(Ok(other));
    let r = assess_ci_evidence(&p, &request());
    assert_never_accepted(&r);
    assert_eq!(r.outcome, Outcome::AssessedFail);
    assert!(!r.machine_ci_verified());
    // A run with no repository identity at all cannot be bound either.
    let mut q = healthy();
    let mut anonymous = run(COMMIT, 1);
    anonymous.repository = None;
    q.run_result = Some(Ok(anonymous));
    let r = assess_ci_evidence(&q, &request());
    assert_eq!(r.outcome, Outcome::AssessedFail);
    assert!(!r.machine_ci_verified());
}

#[test]
fn a_commit_that_is_not_the_requested_one_fails() {
    let mut p = healthy();
    p.commit_result = Some(Ok(commit(NEWER_COMMIT, "another commit")));
    assert_fails(&assess_ci_evidence(&p, &request()), Outcome::AssessedFail, BlockingClass::CommitNotVerified);
    let mut q = healthy();
    q.commit_result = Some(Err(GitHubClientError::NotFound { resource: "commit".into() }));
    assert_fails(&assess_ci_evidence(&q, &request()), Outcome::AssessedFail, BlockingClass::CommitNotVerified);
}

#[test]
fn a_different_workflow_fails() {
    for path in [Some(".github/workflows/other.yml"), Some(".github/workflows/ci.yaml"), Some("ci.yml"), Some(""), None] {
        let mut p = healthy();
        let mut r = run(COMMIT, 1);
        r.path = path.map(str::to_string);
        p.run_result = Some(Ok(r));
        assert_fails(&assess_ci_evidence(&p, &request()), Outcome::AssessedFail, BlockingClass::WorkflowMismatch);
    }
}

#[test]
fn a_requested_attempt_the_run_does_not_have_fails_and_an_older_attempt_is_still_historical() {
    let newer_request = CiEvidenceRequest::new(COMMIT, RUN_ID, 2).unwrap();
    assert_fails(&assess_ci_evidence(&healthy(), &newer_request), Outcome::AssessedFail, BlockingClass::RunAttemptMismatch);
    // Attempt 1 of a run whose latest attempt is 3 is a legitimate historical observation.
    let mut p = healthy();
    p.run_result = Some(Ok(run(COMMIT, 3)));
    assert_eq!(assess_ci_evidence(&p, &request()).outcome, Outcome::AssessedLimitations);
}

#[test]
fn a_run_that_is_not_completed_or_not_successful_fails() {
    let mut p = healthy();
    let mut r = run(COMMIT, 1);
    r.status = "in_progress".to_string();
    r.conclusion = None;
    p.run_result = Some(Ok(r));
    let report = assess_ci_evidence(&p, &request());
    assert_never_accepted(&report);
    assert!(!report.machine_ci_verified());
    assert_eq!(report.outcome, Outcome::AssessedFail);

    let mut q = healthy();
    let mut r = run(COMMIT, 1);
    r.conclusion = Some("failure".to_string());
    q.run_result = Some(Ok(r));
    let report = assess_ci_evidence(&q, &request());
    assert_eq!(report.outcome, Outcome::AssessedFail);
    assert!(!report.machine_ci_verified());
}

// ------------------------------------------------------------------------------------------------------------------
// Jobs
// ------------------------------------------------------------------------------------------------------------------

#[test]
fn missing_failed_skipped_cancelled_and_unfinished_required_jobs_fail() {
    let missing_one = jobs(vec![job(1, "windows-x64", "completed", Some("success"))]);
    assert_fails(&assess_ci_evidence(&missing_one, &request()), Outcome::AssessedFail, BlockingClass::RequiredJobsFailedOrMissing);
    let none = jobs(Vec::new());
    assert_fails(&assess_ci_evidence(&none, &request()), Outcome::AssessedFail, BlockingClass::RequiredJobsFailedOrMissing);

    for bad in ["failure", "timed_out", "neutral", "stale"] {
        let p = jobs(vec![job(1, "windows-x64", "completed", Some("success")), job(2, "macos-arm64", "completed", Some(bad))]);
        assert_fails(&assess_ci_evidence(&p, &request()), Outcome::AssessedFail, BlockingClass::RequiredJobsFailedOrMissing);
    }
    for bad in ["skipped", "cancelled", "action_required"] {
        let p = jobs(vec![job(1, "windows-x64", "completed", Some(bad)), job(2, "macos-arm64", "completed", Some("success"))]);
        assert_fails(&assess_ci_evidence(&p, &request()), Outcome::AssessedFail, BlockingClass::RequiredJobsSkippedOrCancelled);
    }
    let unfinished = jobs(vec![job(1, "windows-x64", "completed", Some("success")), job(2, "macos-arm64", "in_progress", None)]);
    assert_fails(&assess_ci_evidence(&unfinished, &request()), Outcome::AssessedFail, BlockingClass::RunOrJobsNotCompleted);
}

#[test]
fn job_names_are_matched_exactly() {
    for name in ["Windows x64", "windows-x64 ", " windows-x64", "WINDOWS-X64", "windows-x64-extra"] {
        let p = jobs(vec![job(1, name, "completed", Some("success")), job(2, "macos-arm64", "completed", Some("success"))]);
        let r = assess_ci_evidence(&p, &request());
        assert_never_accepted(&r);
        assert!(!r.machine_ci_verified(), "{name:?} must not satisfy windows-x64: {r:?}");
    }
}

#[test]
fn duplicate_and_contradictory_jobs_fail() {
    let identical = jobs(vec![
        job(1, "windows-x64", "completed", Some("success")),
        job(3, "windows-x64", "completed", Some("success")),
        job(2, "macos-arm64", "completed", Some("success")),
    ]);
    assert_fails(&assess_ci_evidence(&identical, &request()), Outcome::AssessedFail, BlockingClass::RequiredJobsFailedOrMissing);
    let contradictory = jobs(vec![
        job(1, "macos-arm64", "completed", Some("failure")),
        job(3, "macos-arm64", "completed", Some("success")),
        job(2, "windows-x64", "completed", Some("success")),
    ]);
    assert_fails(&assess_ci_evidence(&contradictory, &request()), Outcome::AssessedFail, BlockingClass::RequiredJobsFailedOrMissing);
}

#[test]
fn an_extra_job_is_refused_because_the_policy_forbids_extra_jobs() {
    // ECE ignores `allow_extra_jobs`; this module enforces it. All required jobs pass, yet the job set is not the policy's.
    let mut list = green_jobs();
    list.push(job(9, "unreviewed-extra", "completed", Some("success")));
    let r = assess_ci_evidence(&jobs(list), &request());
    assert_fails(&r, Outcome::AssessedFail, BlockingClass::UnexpectedJobSet);
    assert_eq!(r.ci, CiResult::VerifiedPass, "ECE's own dimension is reported unchanged");
}

// ------------------------------------------------------------------------------------------------------------------
// The operator-owned policy
// ------------------------------------------------------------------------------------------------------------------

fn embedded_policy_text() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/ece/atm-ci-two-runner-v1.json");
    std::fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

#[test]
fn the_embedded_policy_is_exactly_the_independently_expected_bytes() {
    let on_disk = embedded_policy_text();
    assert_eq!(sha256_hex(on_disk.as_bytes()), POLICY_FILE_SHA256);
    assert_eq!(sha256_hex(POLICY_BYTES), POLICY_FILE_SHA256);
    assert_eq!(POLICY_BYTES, on_disk.as_bytes());
    let policy = load_policy(POLICY_BYTES).expect("the pinned bytes load");
    assert_eq!(policy.policy_id, "atm-ci-two-runner-v1");
    assert_eq!(policy.required_jobs, vec!["windows-x64".to_string(), "macos-arm64".to_string()]);
    assert!(!policy.allow_extra_jobs);
    assert_eq!(policy.target_repo_id.as_deref(), Some("1413039335"));
    assert_eq!(policy.target_workflow_path.as_deref(), Some(".github/workflows/ci.yml"));
}

#[test]
fn the_digest_inside_the_policy_file_is_not_a_trust_anchor() {
    let policy = load_policy(POLICY_BYTES).unwrap();
    assert_ne!(policy.policy_digest, POLICY_FILE_SHA256, "the self-declared digest is a different value");
    // Changing ONLY the self-declared digest field changes the file's real digest, so it is refused.
    let text = embedded_policy_text();
    let forged = text.replace(&policy.policy_digest, &"0".repeat(64));
    assert_ne!(forged, text);
    assert_eq!(load_policy(forged.as_bytes()), Err(ErrorClass::PolicyIntegrity));
    // A policy whose self-declared digest is honestly recomputed by ECE for different content is refused too.
    let other = TrustedCheckPolicy::new(
        "atm-ci-two-runner-v1",
        vec!["windows-x64".to_string()],
        Some("1413039335".into()),
        Some(WORKFLOW_PATH.into()),
    );
    let bytes = serde_json::to_vec_pretty(&other).unwrap();
    assert_eq!(load_policy(&bytes), Err(ErrorClass::PolicyIntegrity));
}

#[test]
fn any_mutation_of_the_policy_bytes_is_refused_before_anything_is_evaluated() {
    let text = embedded_policy_text();
    let mutations: Vec<(&str, String)> = vec![
        ("a required job renamed", text.replace("windows-x64", "windows-x65")),
        ("a required job removed", text.replace("    \"windows-x64\",\n", "")),
        ("extra jobs allowed", text.replace("\"allow_extra_jobs\": false", "\"allow_extra_jobs\": true")),
        ("another repository id", text.replace("1413039335", "1432704795")),
        ("another workflow", text.replace(".github/workflows/ci.yml", ".github/workflows/other.yml")),
        ("another policy id", text.replace("atm-ci-two-runner-v1", "atm-ci-two-runner-v2")),
        ("a trailing newline", format!("{text}\n")),
        ("CRLF line endings", text.replace('\n', "\r\n")),
        ("a leading space", format!(" {text}")),
        ("truncated", text[..text.len() - 3].to_string()),
        ("empty", String::new()),
        ("not JSON", "policy".to_string()),
    ];
    for (what, mutated) in mutations {
        assert_ne!(mutated, text, "{what}: the mutation must change the bytes");
        assert_eq!(load_policy(mutated.as_bytes()), Err(ErrorClass::PolicyIntegrity), "{what}");
    }
}

#[test]
fn a_policy_that_says_something_else_is_refused_even_if_it_were_the_pinned_bytes() {
    // Defence in depth: the parsed policy must also say what this module is written for.
    let good = load_policy(POLICY_BYTES).unwrap();
    assert!(policy_says_what_this_module_expects(&good));
    let mut p = good.clone();
    p.required_jobs.pop();
    assert!(!policy_says_what_this_module_expects(&p));
    let mut p = good.clone();
    p.required_jobs.reverse();
    assert!(!policy_says_what_this_module_expects(&p));
    let mut p = good.clone();
    p.allow_extra_jobs = true;
    assert!(!policy_says_what_this_module_expects(&p));
    let mut p = good.clone();
    p.target_repo_id = None;
    assert!(!policy_says_what_this_module_expects(&p));
    let mut p = good.clone();
    p.target_workflow_path = Some("x".into());
    assert!(!policy_says_what_this_module_expects(&p));
    let mut p = good;
    p.policy_id = "other".into();
    assert!(!policy_says_what_this_module_expects(&p));
}

#[test]
fn a_weaker_operator_policy_handed_to_the_core_cannot_hide_a_failure_that_ece_finds() {
    // The core takes whatever policy it is given (only the tests can); ECE still applies the target scope it is told.
    let store = SchemaStore::from_embedded().unwrap();
    let mut weak = load_policy(POLICY_BYTES).unwrap();
    weak.target_workflow_path = None;
    weak.target_repo_id = None;
    let mut p = healthy();
    let mut r = run(COMMIT, 1);
    r.path = Some(".github/workflows/other.yml".to_string());
    p.run_result = Some(Ok(r));
    // The request itself still carries the expected workflow, so ECE reports the mismatch regardless of the weak policy.
    let report = assess_with(&p, &request(), &weak, &store);
    assert_fails(&report, Outcome::AssessedFail, BlockingClass::WorkflowMismatch);
}

// ------------------------------------------------------------------------------------------------------------------
// Provider failures and artifacts
// ------------------------------------------------------------------------------------------------------------------

fn provider_errors() -> Vec<GitHubClientError> {
    vec![
        GitHubClientError::NotFound { resource: "run".into() },
        GitHubClientError::RateLimited { reset_at: Some(1), message: "checkpoint_accepted".into() },
        GitHubClientError::Unauthorized("token".into()),
        GitHubClientError::HttpError { status: 500, message: "boom".into(), rate_limit_reset: None },
        GitHubClientError::Timeout("t".into()),
        GitHubClientError::NetworkError("n".into()),
        GitHubClientError::InvalidJson("j".into()),
        GitHubClientError::SizeLimitExceeded(1),
        GitHubClientError::EmptyResponse,
        GitHubClientError::IncompletePagination { retrieved: 1, total_count: 2, page_limit: 1 },
    ]
}

#[test]
fn every_provider_error_on_a_required_lookup_is_insufficient_evidence_and_never_a_pass() {
    for error in provider_errors() {
        for which in ["repo", "run", "jobs"] {
            let mut p = healthy();
            match which {
                "repo" => p.repo_result = Some(Err(error.clone())),
                "run" => p.run_result = Some(Err(error.clone())),
                _ => p.jobs_result = Some(Err(error.clone())),
            }
            let r = assess_ci_evidence(&p, &request());
            assert_never_accepted(&r);
            assert_eq!(r.outcome, Outcome::InsufficientEvidence, "{which} {error:?}");
            assert_eq!(r.error, Some(ErrorClass::ProviderFailure), "{which} {error:?}");
            assert_eq!(r.ci, CiResult::UnverifiedRun);
            assert_eq!(r.review, ReviewState::NotEvaluated);
            assert_eq!(r.freshness, Freshness::NotEvaluated);
            assert!(r.blocking.is_empty(), "a provider failure is an error class, not a verdict about the checkpoint: {r:?}");
            assert!(!r.machine_ci_verified());
            // The provider's message (here an injected acceptance string) is not carried into the result.
            assert!(!format!("{r:?}").contains("checkpoint_accepted"));
        }
    }
}

fn artifact(expired: bool) -> GitHubArtifact {
    GitHubArtifact {
        id: 77,
        name: "evidence".to_string(),
        size_in_bytes: 10,
        archive_download_url: None,
        expired,
        created_at: None,
        expires_at: None,
        digest: None,
    }
}

fn declared_artifact() -> DeclaredArtifactSpec {
    DeclaredArtifactSpec { artifact_name: "evidence".to_string(), artifact_id: Some(77), expected_digest: None, verify_bytes: false }
}

#[test]
fn declared_artifacts_that_are_expired_missing_or_unreachable_fail() {
    // The public request declares no artifact (a checkpoint cannot ask this module to trust one). The core can, and ECE then
    // judges availability, so the fail-closed behaviour is proven through the core with the same policy and schemas.
    let store = SchemaStore::from_embedded().unwrap();
    let policy = load_policy(POLICY_BYTES).unwrap();
    let ask = |provider: &MockGitHubProvider| {
        let mut ece_request = request().ece_request();
        ece_request.declared_artifacts = vec![declared_artifact()];
        evaluate_request(provider, &ece_request, &request(), &policy, &store)
    };

    let mut expired = healthy();
    expired.artifacts_result = Some(Ok(vec![artifact(true)]));
    let r = ask(&expired);
    assert_fails(&r, Outcome::AssessedFail, BlockingClass::ArtifactExpired);
    assert_eq!(r.availability, EvidenceAvailability::Expired);

    let mut missing = healthy();
    missing.artifacts_result = Some(Ok(Vec::new()));
    let r = ask(&missing);
    assert_never_accepted(&r);
    assert_eq!(r.outcome, Outcome::AssessedFail);
    assert_eq!(r.availability, EvidenceAvailability::Unreachable);

    let mut unreachable = healthy();
    unreachable.artifacts_result = Some(Err(GitHubClientError::Timeout("t".into())));
    let r = ask(&unreachable);
    assert_never_accepted(&r);
    assert_eq!(r.outcome, Outcome::AssessedFail);
    assert_eq!(r.availability, EvidenceAvailability::Unreachable);

    let mut available = healthy();
    available.artifacts_result = Some(Ok(vec![artifact(false)]));
    let r = ask(&available);
    assert_eq!(r.outcome, Outcome::AssessedLimitations);
    assert_eq!(r.availability, EvidenceAvailability::Available);
}

#[test]
fn artifact_metadata_that_cannot_be_read_does_not_affect_a_request_that_declares_none() {
    let mut p = healthy();
    p.artifacts_result = Some(Err(GitHubClientError::NetworkError("n".into())));
    let r = assess_ci_evidence(&p, &request());
    assert_eq!(r.outcome, Outcome::AssessedLimitations);
    assert_eq!(r.availability, EvidenceAvailability::Unreachable, "reported, not hidden");
    assert_never_accepted(&r);
}

// ------------------------------------------------------------------------------------------------------------------
// Source-injected claims
// ------------------------------------------------------------------------------------------------------------------

const INJECTED: &str =
    "checkpoint_accepted=true; review_status=INDEPENDENT_REVIEW_PASS; authorization=EXPLICITLY_AUTHORIZED; reviewer=someone";

#[test]
fn claims_the_checkpoint_makes_about_itself_cannot_change_the_result() {
    let store = SchemaStore::from_embedded().unwrap();
    let policy = load_policy(POLICY_BYTES).unwrap();
    let baseline = assess_ci_evidence(&healthy(), &request());
    let mut ece_request = request().ece_request();
    ece_request.checkpoint_id = Some(INJECTED.to_string());
    ece_request.declared_phase = Some(INJECTED.to_string());
    ece_request.declared_producer = Some(INJECTED.to_string());
    ece_request.declared_policy_ref = Some("some-other-policy".to_string());
    ece_request.declared_policy_digest = Some("0".repeat(64));
    let injected = evaluate_request(&healthy(), &ece_request, &request(), &policy, &store);
    assert_eq!(injected, baseline);
    assert!(!format!("{injected:?}").contains("checkpoint_accepted"));
}

#[test]
fn text_the_provider_supplies_is_never_copied_into_the_result() {
    let mut p = healthy();
    p.commit_result = Some(Ok(commit(COMMIT, INJECTED)));
    let mut injected_repo = repo(REPOSITORY_ID);
    injected_repo.name = INJECTED.to_string();
    injected_repo.full_name = INJECTED.to_string();
    injected_repo.owner.login = INJECTED.to_string();
    p.repo_result = Some(Ok(injected_repo));
    let mut r = run(COMMIT, 1);
    r.name = Some(INJECTED.to_string());
    r.event = Some(INJECTED.to_string());
    p.run_result = Some(Ok(r));
    let report = assess_ci_evidence(&p, &request());
    assert_eq!(report, assess_ci_evidence(&healthy(), &request()));
    for needle in ["checkpoint_accepted", "INDEPENDENT_REVIEW_PASS", "EXPLICITLY_AUTHORIZED", "reviewer"] {
        assert!(!format!("{report:?}").contains(needle), "{needle}");
    }
    // Injected text in a JOB NAME is not a required job: it fails, and the text still does not appear.
    let mut list = green_jobs();
    list[0].name = format!("windows-x64 {INJECTED}");
    let report = assess_ci_evidence(&jobs(list), &request());
    assert_never_accepted(&report);
    assert!(!report.machine_ci_verified());
    assert!(!format!("{report:?}").contains("checkpoint_accepted"));
}

// ------------------------------------------------------------------------------------------------------------------
// The projection is strict (unsupported schema, malformed or acceptance-like assessments)
// ------------------------------------------------------------------------------------------------------------------

fn real_assessment() -> (Value, i32) {
    let store = SchemaStore::from_embedded().unwrap();
    let policy = load_policy(POLICY_BYTES).unwrap();
    let out = evaluate_github_evidence_with_store(&healthy(), &request().ece_request(), &policy, &store, None).unwrap();
    (out.assessment, out.exit_code)
}

fn project_of(assessment: &Value, exit_code: i32) -> Result<CiEvidenceReport, ErrorClass> {
    project(assessment, exit_code, &request(), &load_policy(POLICY_BYTES).unwrap())
}

fn set(value: &mut Value, path: &[&str], new: Value) {
    let (last, parents) = path.split_last().unwrap();
    let mut cursor = value;
    for key in parents {
        cursor = cursor.get_mut(*key).unwrap();
    }
    cursor.as_object_mut().unwrap().insert((*last).to_string(), new);
}

#[test]
fn the_real_assessment_projects_and_every_acceptance_like_mutation_of_it_is_refused() {
    let (good, exit) = real_assessment();
    assert!(project_of(&good, exit).unwrap().machine_ci_verified());

    let cases: Vec<(&str, Vec<&str>, Value, ErrorClass)> = vec![
        ("unsupported schema version", vec!["schema_version"], json!("9.9.9"), ErrorClass::UnsupportedSchemaVersion),
        ("schema version missing type", vec!["schema_version"], json!(1), ErrorClass::MalformedAssessment),
        ("review passed", vec!["dimensions", "review_status", "status"], json!("INDEPENDENT_REVIEW_PASS"), ErrorClass::MalformedAssessment),
        ("review self reported", vec!["dimensions", "review_status", "status"], json!("SELF_REPORT_ONLY"), ErrorClass::MalformedAssessment),
        (
            "explicitly authorized",
            vec!["dimensions", "authorization", "status"],
            json!("EXPLICITLY_AUTHORIZED"),
            ErrorClass::MalformedAssessment,
        ),
        ("human authorized flag", vec!["claims_breakdown", "human_authorized", "authorized"], json!(true), ErrorClass::MalformedAssessment),
        (
            "review verdict pass",
            vec!["claims_breakdown", "independently_reviewed", "review_verdict"],
            json!("PASS"),
            ErrorClass::MalformedAssessment,
        ),
        ("current head", vec!["dimensions", "freshness", "status"], json!("CURRENT_HEAD"), ErrorClass::MalformedAssessment),
        (
            "provisional pass verdict",
            vec!["derived_summary", "verdict"],
            json!("ASSESSED_PROVISIONAL_PASS"),
            ErrorClass::MalformedAssessment,
        ),
        ("unknown ci status", vec!["dimensions", "ci_execution", "status"], json!("SUPER_PASS"), ErrorClass::MalformedAssessment),
        ("ci not applicable", vec!["dimensions", "ci_execution", "status"], json!("NOT_APPLICABLE"), ErrorClass::MalformedAssessment),
        ("unknown availability", vec!["dimensions", "evidence_availability", "status"], json!("PERFECT"), ErrorClass::MalformedAssessment),
        ("synthetic mock", vec!["evaluator_auth", "is_synthetic_mock"], json!(true), ErrorClass::MalformedAssessment),
        ("another engine", vec!["engine", "name"], json!("Another-Engine"), ErrorClass::MalformedAssessment),
        ("another repository", vec!["evaluated_target", "repository_id"], json!("1432704795"), ErrorClass::BindingMismatch),
        ("another commit", vec!["evaluated_target", "target_commit_sha"], json!(NEWER_COMMIT), ErrorClass::BindingMismatch),
        ("another owner", vec!["evaluated_target", "repository_owner"], json!("someone-else"), ErrorClass::BindingMismatch),
        ("wrong expected job count", vec!["dimensions", "ci_execution", "expected_jobs_count"], json!(1), ErrorClass::MalformedAssessment),
        ("blocking reasons not an array", vec!["derived_summary", "blocking_reasons"], json!("none"), ErrorClass::MalformedAssessment),
        ("exit code disagrees", vec!["exit_code"], json!(2), ErrorClass::MalformedAssessment),
    ];
    for (what, path, value, expected) in cases {
        let mut mutated = good.clone();
        set(&mut mutated, &path, value);
        assert_eq!(project_of(&mutated, exit).err(), Some(expected), "{what}");
    }

    // A limitations verdict that contradicts its own CI dimension or carries a blocking reason is refused.
    let mut contradiction = good.clone();
    set(&mut contradiction, &["dimensions", "ci_execution", "status"], json!("FAILED"));
    assert_eq!(project_of(&contradiction, exit).err(), Some(ErrorClass::MalformedAssessment));
    let mut with_reason = good.clone();
    set(&mut with_reason, &["derived_summary", "blocking_reasons"], json!(["x"]));
    assert_eq!(project_of(&with_reason, exit).err(), Some(ErrorClass::MalformedAssessment));
    // The verdict and the exit code must agree.
    assert_eq!(project_of(&good, 2).err(), Some(ErrorClass::MalformedAssessment));
    assert_eq!(project_of(&good, 0).err(), Some(ErrorClass::MalformedAssessment));
    assert_eq!(project_of(&good, 4).err(), Some(ErrorClass::MalformedAssessment));
}

#[test]
fn a_source_declared_acceptance_key_anywhere_in_an_assessment_is_refused() {
    let (good, exit) = real_assessment();
    for (where_, path) in
        [("top level", vec![]), ("derived summary", vec!["derived_summary"]), ("claims", vec!["claims_breakdown", "source_declared"])]
    {
        let mut mutated = good.clone();
        let mut cursor = &mut mutated;
        for key in &path {
            cursor = cursor.get_mut(*key).unwrap();
        }
        cursor.as_object_mut().unwrap().insert("checkpoint_accepted".to_string(), json!(true));
        assert_eq!(project_of(&mutated, exit).err(), Some(ErrorClass::MalformedAssessment), "{where_}");
    }
    // ... and even `false`: the key itself is not part of any assessment this module understands.
    let mut mutated = good;
    mutated.as_object_mut().unwrap().insert("checkpoint_accepted".to_string(), json!(false));
    assert_eq!(project_of(&mutated, exit).err(), Some(ErrorClass::MalformedAssessment));
}

#[test]
fn malformed_assessments_are_refused() {
    let (good, exit) = real_assessment();
    for what in ["null", "array", "string", "number", "empty object"] {
        let value = match what {
            "null" => Value::Null,
            "array" => json!([]),
            "string" => json!("ASSESSED_LIMITATIONS"),
            "number" => json!(1),
            _ => json!({}),
        };
        assert!(project_of(&value, exit).is_err(), "{what}");
    }
    for key in ["dimensions", "derived_summary", "claims_breakdown", "evaluated_target", "engine", "evaluator_auth", "exit_code"] {
        let mut mutated = good.clone();
        mutated.as_object_mut().unwrap().remove(key);
        assert_eq!(project_of(&mutated, exit).err(), Some(ErrorClass::MalformedAssessment), "missing {key}");
    }
}

#[test]
fn an_assessment_that_fails_ece_s_own_output_schema_is_not_produced_and_the_result_is_an_error_class() {
    // A store whose assessment schema no ECE assessment can satisfy: ECE refuses to emit, and so does this module.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("checkpoint-input.schema.json"), ece_lib::EMBEDDED_INPUT_SCHEMA).unwrap();
    std::fs::write(dir.path().join("checkpoint-envelope.schema.json"), ece_lib::EMBEDDED_ENVELOPE_SCHEMA).unwrap();
    std::fs::write(
        dir.path().join("assessment-output.schema.json"),
        r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","required":["a_field_ece_never_emits"]}"#,
    )
    .unwrap();
    let store = SchemaStore::load(dir.path()).unwrap();
    let policy = load_policy(POLICY_BYTES).unwrap();
    let r = assess_with(&healthy(), &request(), &policy, &store);
    assert_never_accepted(&r);
    assert_eq!(r.outcome, Outcome::NotAssessed);
    assert_eq!(r.error, Some(ErrorClass::OutputSchemaViolation));
    assert!(!r.machine_ci_verified());
}

// ------------------------------------------------------------------------------------------------------------------
// The request
// ------------------------------------------------------------------------------------------------------------------

#[test]
fn only_a_fully_specified_request_can_be_built() {
    assert!(CiEvidenceRequest::new(COMMIT, 1, 1).is_ok());
    for bad in [
        "",
        "dd37b04",
        &COMMIT.to_uppercase(),
        &format!("{COMMIT}0"),
        "zz37b04f730569c3a8ad1b929ee6b80052ef22e2",
        " dd37b04f730569c3a8ad1b929ee6b80052ef22e",
    ] {
        assert_eq!(CiEvidenceRequest::new(bad, 1, 1), Err(RequestError::CommitShaNotFortyLowercaseHex), "{bad:?}");
    }
    assert_eq!(CiEvidenceRequest::new(COMMIT, 0, 1), Err(RequestError::RunIdZero));
    assert_eq!(CiEvidenceRequest::new(COMMIT, 1, 0), Err(RequestError::RunAttemptZero));
}

// ------------------------------------------------------------------------------------------------------------------
// No new network, credential or process capability; the exact dependency
// ------------------------------------------------------------------------------------------------------------------

fn repo_file(relative: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)).unwrap().replace("\r\n", "\n")
}

#[test]
fn ece_is_pinned_to_one_full_revision_with_default_features_off() {
    let manifest = repo_file("Cargo.toml");
    let line = manifest.lines().find(|l| l.starts_with("ece_lib = ")).expect("the ECE dependency line");
    assert!(line.contains("package = \"ece\""), "{line}");
    assert!(line.contains("git = \"https://github.com/ErenTahiroglu/Evidence-Checkpoint-Engine\""), "{line}");
    assert!(line.contains(&format!("rev = \"{ECE_REVISION}\"")), "{line}");
    assert!(line.contains("default-features = false"), "{line}");
    assert!(!line.contains("features = ["), "no ECE feature may be switched on: {line}");
    assert_eq!(ECE_REVISION.len(), 40);
    assert!(ECE_REVISION.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
}

/// The text of one `[[package]]` block of Cargo.lock.
fn lock_block(lock: &str, name: &str) -> Option<String> {
    lock.split("[[package]]\n").find(|b| b.starts_with(&format!("name = \"{name}\"\n"))).map(str::to_string)
}

#[test]
fn the_lockfile_resolves_ece_to_that_revision_and_brings_in_no_http_stack() {
    let lock = repo_file("Cargo.lock");
    let ece = lock_block(&lock, "ece").expect("ece in Cargo.lock");
    assert!(
        ece.contains(&format!(
            "source = \"git+https://github.com/ErenTahiroglu/Evidence-Checkpoint-Engine?rev={ECE_REVISION}#{ECE_REVISION}\""
        )),
        "{ece}"
    );
    for forbidden in ["reqwest", "hyper", "rustls", "native-tls", "openssl", "tokio", "h2", "ring", "socket2", "mio", "indexmap"] {
        assert!(!ece.contains(&format!("\"{forbidden}")), "ECE must not depend on {forbidden}: {ece}");
    }
    // ECE's own HTTP client dependency (reqwest 0.12) is absent from the whole graph. Tauri's reqwest is a different, pre-existing one.
    assert!(!lock.contains("name = \"reqwest\"\nversion = \"0.12"), "reqwest 0.12 must not be in Cargo.lock");
}

#[test]
fn the_module_contains_no_network_credential_filesystem_or_process_code() {
    let source = include_str!("ece_ci.rs");
    let forbidden = [
        ["std::", "net"].concat(),
        ["std::", "env"].concat(),
        ["std::", "process"].concat(),
        ["std::", "fs"].concat(),
        ["req", "west"].concat(),
        ["GITHUB", "_TOKEN"].concat(),
        ["GH", "_TOKEN"].concat(),
        ["Http", "GitHubClient"].concat(),
        ["new_un", "authenticated"].concat(),
        ["Command", "::new"].concat(),
        ["Tcp", "Stream"].concat(),
        ["tauri", "::"].concat(),
        ["rusq", "lite"].concat(),
    ];
    // Only the code before the end of the file is production code; this module has no inline test module.
    for needle in &forbidden {
        assert!(!source.contains(needle.as_str()), "ece_ci.rs mentions {needle}");
    }
    // The only way in for evidence is the caller's provider, and the only schemas are ECE's embedded ones.
    assert!(source.contains("SchemaStore::from_embedded()"));
    assert!(source.contains("evaluate_github_evidence_with_store"));
}

#[test]
fn no_command_ipc_capability_or_ui_reaches_the_module() {
    let lib = repo_file("src/lib.rs");
    let commands = repo_file("src/commands.rs");
    assert!(!commands.contains("ece_ci"), "no Tauri command may reach ece_ci");
    assert!(!lib.replace("mod ece_ci;", "").contains("ece_ci::"), "lib.rs registers nothing from ece_ci");
    let capabilities = repo_file("capabilities/default.json");
    assert!(!capabilities.contains("ece"), "no capability mentions ECE");
    for other in ["src/selfcheck.rs", "src/health.rs", "src/state.rs", "src/main.rs"] {
        assert!(!repo_file(other).contains("ece_ci"), "{other}");
    }
}
