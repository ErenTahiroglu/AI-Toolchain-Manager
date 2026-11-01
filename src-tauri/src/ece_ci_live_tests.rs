//! ATM-GOV-02 P2B: focused tests of the live command's backend half (`ece_ci_live`) and of the P2B guard in `ece_ci`.
//!
//! No test here contacts GitHub or any external host. The real anonymous client is exercised only against a loopback listener on
//! 127.0.0.1 that this file starts, and everything else uses in-memory providers. The P2A tests in `ece_ci_tests` are untouched
//! apart from the assertions P2B deliberately changes (revision, feature, "one command reaches the module").

use crate::ece_ci::*;
use crate::ece_ci_live::*;
use ece_lib::github::models::{
    GitHubArtifact, GitHubCommit, GitHubCommitDetails, GitHubJob, GitHubRepository, GitHubRunRepo, GitHubUser, GitHubWorkflowRun,
};
use ece_lib::github::{GitHubClientError, GitHubProvider, HttpGitHubClient, MockGitHubProvider};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

const COMMIT: &str = "dd37b04f730569c3a8ad1b929ee6b80052ef22e2";
const RUN_ID: u64 = 41_217_731_446;

// ---------- synthetic evidence ----------

fn args(sha: &str, run_id: u64, attempt: u32) -> AssessAtmCheckpointRequest {
    AssessAtmCheckpointRequest { commit_sha: sha.to_string(), run_id, run_attempt: attempt }
}

fn request(attempt: u32) -> CiEvidenceRequest {
    CiEvidenceRequest::new(COMMIT, RUN_ID, attempt).unwrap()
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

fn run(attempt: u32) -> GitHubWorkflowRun {
    GitHubWorkflowRun {
        id: RUN_ID,
        name: Some("CI".to_string()),
        head_sha: COMMIT.to_string(),
        path: Some(WORKFLOW_PATH.to_string()),
        run_attempt: attempt,
        event: Some("push".to_string()),
        status: "completed".to_string(),
        conclusion: Some("success".to_string()),
        repository: Some(GitHubRunRepo { id: REPOSITORY_ID, full_name: REPOSITORY_FULL_NAME.to_string() }),
    }
}

fn job(id: u64, name: &str, conclusion: &str) -> GitHubJob {
    GitHubJob {
        id,
        name: name.to_string(),
        status: "completed".to_string(),
        conclusion: Some(conclusion.to_string()),
        started_at: None,
        completed_at: None,
    }
}

fn green() -> Vec<GitHubJob> {
    vec![job(1, "windows-x64", "success"), job(2, "macos-arm64", "success")]
}

fn healthy(latest_attempt: u32) -> MockGitHubProvider {
    let mut p = MockGitHubProvider::new();
    p.repo_result = Some(Ok(repo(REPOSITORY_ID)));
    p.commit_result =
        Some(Ok(GitHubCommit { sha: COMMIT.to_string(), commit: GitHubCommitDetails { message: Some("m".into()), author: None } }));
    p.run_result = Some(Ok(run(latest_attempt)));
    p.jobs_result = Some(Ok(green()));
    p.artifacts_result = Some(Ok(Vec::new()));
    p
}

/// Counts what the code under test asks of the provider, then delegates to the in-memory one.
struct Counting {
    inner: MockGitHubProvider,
    repo_calls: AtomicUsize,
    jobs_calls: AtomicUsize,
    artifact_calls: AtomicUsize,
    download_calls: AtomicUsize,
}

impl Counting {
    fn new(inner: MockGitHubProvider) -> Self {
        Self {
            inner,
            repo_calls: AtomicUsize::new(0),
            jobs_calls: AtomicUsize::new(0),
            artifact_calls: AtomicUsize::new(0),
            download_calls: AtomicUsize::new(0),
        }
    }
}

impl GitHubProvider for Counting {
    fn get_repository(&self, o: &str, r: &str) -> Result<GitHubRepository, GitHubClientError> {
        self.repo_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.get_repository(o, r)
    }
    fn get_commit(&self, o: &str, r: &str, s: &str) -> Result<GitHubCommit, GitHubClientError> {
        self.inner.get_commit(o, r, s)
    }
    fn get_workflow_run(&self, o: &str, r: &str, id: u64) -> Result<GitHubWorkflowRun, GitHubClientError> {
        self.inner.get_workflow_run(o, r, id)
    }
    fn get_attempt_jobs(&self, o: &str, r: &str, id: u64, a: u32) -> Result<Vec<GitHubJob>, GitHubClientError> {
        self.jobs_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.get_attempt_jobs(o, r, id, a)
    }
    fn get_artifacts(&self, o: &str, r: &str, id: u64) -> Result<Vec<GitHubArtifact>, GitHubClientError> {
        self.artifact_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.get_artifacts(o, r, id)
    }
    fn download_artifact_digest(&self, o: &str, r: &str, id: u64, max: usize) -> Result<String, GitHubClientError> {
        self.download_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.download_artifact_digest(o, r, id, max)
    }
}

fn never_a_pass(r: &CiEvidenceReport) {
    assert!(!r.machine_ci_verified(), "{r:?}");
    assert!(matches!(r.review, ReviewState::Unreviewed | ReviewState::NotEvaluated), "{r:?}");
    assert_eq!(r.authorization, AuthorizationState::AuthorizationPending, "{r:?}");
}

// ------------------------------------------------------------------------------------------------------------------
// 6. Invalid input is rejected before any client exists
// ------------------------------------------------------------------------------------------------------------------

#[test]
fn invalid_input_is_rejected_before_a_provider_is_ever_built() {
    let built = AtomicUsize::new(0);
    let factory = || -> Result<Box<dyn GitHubProvider>, GitHubClientError> {
        built.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(healthy(1)))
    };
    let upper = COMMIT.to_uppercase();
    let cases: Vec<(&str, AssessAtmCheckpointRequest, InputError)> = vec![
        ("empty sha", args("", RUN_ID, 1), InputError::CommitSha),
        ("short sha", args("dd37b04", RUN_ID, 1), InputError::CommitSha),
        ("upper-case sha", args(&upper, RUN_ID, 1), InputError::CommitSha),
        ("41 chars", args(&format!("{COMMIT}0"), RUN_ID, 1), InputError::CommitSha),
        ("non-hex", args("zz37b04f730569c3a8ad1b929ee6b80052ef22e2", RUN_ID, 1), InputError::CommitSha),
        ("branch name", args("development", RUN_ID, 1), InputError::CommitSha),
        ("whitespace", args(&format!(" {}", &COMMIT[1..]), RUN_ID, 1), InputError::CommitSha),
        ("path-like", args("../../repos/x/commits/abc", RUN_ID, 1), InputError::CommitSha),
        ("run id 0", args(COMMIT, 0, 1), InputError::RunId),
        ("run id above 2^53-1", args(COMMIT, MAX_SAFE_RUN_ID + 1, 1), InputError::RunId),
        ("run id u64::MAX", args(COMMIT, u64::MAX, 1), InputError::RunId),
        ("attempt 0", args(COMMIT, RUN_ID, 0), InputError::RunAttempt),
        ("attempt 1001", args(COMMIT, RUN_ID, MAX_RUN_ATTEMPT + 1), InputError::RunAttempt),
        ("attempt u32::MAX", args(COMMIT, RUN_ID, u32::MAX), InputError::RunAttempt),
    ];
    for (what, input, expected) in cases {
        let err = assess_checked(&input, factory).err();
        assert_eq!(err, Some(expected), "{what}");
    }
    assert_eq!(built.load(Ordering::SeqCst), 0, "a provider was built for invalid input");
    // The boundary values themselves are accepted, and then the provider is built exactly once.
    assert!(validate(&args(COMMIT, MAX_SAFE_RUN_ID, MAX_RUN_ATTEMPT)).is_ok());
    let report = assess_checked(&args(COMMIT, RUN_ID, 1), factory).unwrap();
    assert_eq!(built.load(Ordering::SeqCst), 1);
    assert!(report.machine_ci_verified());
}

#[test]
fn the_request_has_exactly_three_fields_and_refuses_anything_else() {
    let ok = r#"{"commitSha":"dd37b04f730569c3a8ad1b929ee6b80052ef22e2","runId":41217731446,"runAttempt":1}"#;
    assert!(serde_json::from_str::<AssessAtmCheckpointRequest>(ok).is_ok());
    for (what, extra) in [
        ("repository", r#""repository":"someone/else""#),
        ("owner", r#""owner":"someone""#),
        ("url", r#""url":"https://example.invalid/""#),
        ("base url", r#""baseUrl":"http://127.0.0.1:1""#),
        ("token", r#""token":"ghp_x""#),
        ("policy", r#""policy":{"required_jobs":[]}"#),
        ("jobs", r#""requiredJobs":["a"]"#),
        ("workflow", r#""workflowPath":".github/workflows/other.yml""#),
        ("acknowledged", r#""acknowledged":true"#),
    ] {
        let text = ok.replace('}', &format!(",{extra}}}"));
        assert!(serde_json::from_str::<AssessAtmCheckpointRequest>(&text).is_err(), "{what} must be refused");
    }
    // snake_case spellings and missing fields are not accepted either.
    assert!(serde_json::from_str::<AssessAtmCheckpointRequest>(r#"{"commit_sha":"x","run_id":1,"run_attempt":1}"#).is_err());
    assert!(serde_json::from_str::<AssessAtmCheckpointRequest>(r#"{"commitSha":"x","runId":1}"#).is_err());
}

#[test]
fn a_client_that_cannot_be_built_is_a_closed_error_not_a_pass_and_not_a_ci_failure() {
    let report = assess_live(&request(1), || Err(GitHubClientError::NetworkError("tls init".into())));
    never_a_pass(&report);
    assert_eq!(report.outcome, Outcome::NotAssessed);
    assert_eq!(report.error, Some(ErrorClass::ClientInitFailure));
    assert!(report.blocking.is_empty());
    assert_eq!(report.ci, CiResult::NotEvaluated);
}

// ------------------------------------------------------------------------------------------------------------------
// 7. One assessment at a time
// ------------------------------------------------------------------------------------------------------------------

#[test]
fn only_one_assessment_runs_at_a_time_and_the_token_outlives_a_caller_that_stops_waiting() {
    let flag = Arc::new(AtomicBool::new(false));
    let first = InFlight::try_acquire(&flag).expect("the first request is admitted");
    assert!(InFlight::try_acquire(&flag).is_none(), "a second request while one is pending is refused");
    drop(first);
    let again = InFlight::try_acquire(&flag).expect("released after the work ends");
    // The token moves into the blocking work: the caller (an awaiting future) dropping does not release it, the work ending does.
    let flag2 = Arc::clone(&flag);
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (finish_tx, finish_rx) = std::sync::mpsc::channel::<()>();
    let worker = std::thread::spawn(move || {
        let _held = again;
        started_tx.send(()).unwrap();
        finish_rx.recv().unwrap();
    });
    started_rx.recv().unwrap();
    assert!(InFlight::try_acquire(&flag2).is_none(), "still busy while the blocking work runs");
    finish_tx.send(()).unwrap();
    worker.join().unwrap();
    assert!(InFlight::try_acquire(&flag2).is_some(), "free again only after the work ended");
}

// ------------------------------------------------------------------------------------------------------------------
// 5/9/F. The requested attempt must be the run's latest attempt
// ------------------------------------------------------------------------------------------------------------------

#[test]
fn a_pass_needs_the_requested_attempt_to_be_the_latest_attempt() {
    let ok = assess_ci_evidence_latest_attempt_only(&healthy(1), &request(1));
    assert!(ok.machine_ci_verified(), "{ok:?}");
    let ok2 = assess_ci_evidence_latest_attempt_only(&healthy(2), &request(2));
    assert!(ok2.machine_ci_verified(), "{ok2:?}");
}

#[test]
fn a_superseded_attempt_is_refused_and_never_mixed_with_the_latest_attempt() {
    // Attempt 1 was re-run: the run endpoint now describes attempt 2 (green, success) while attempt 1's own jobs are red.
    let mut red_jobs_old_attempt = healthy(2);
    red_jobs_old_attempt.jobs_result = Some(Ok(vec![job(1, "windows-x64", "failure"), job(2, "macos-arm64", "success")]));
    let counting = Counting::new(red_jobs_old_attempt);
    let r = assess_ci_evidence_latest_attempt_only(&counting, &request(1));
    never_a_pass(&r);
    assert_eq!((r.outcome, r.error), (Outcome::NotAssessed, Some(ErrorClass::AttemptNotLatest)), "{r:?}");
    assert_eq!(r.ci, CiResult::NotEvaluated, "no CI verdict is invented for either attempt");
    assert!(r.blocking.is_empty());
    assert_eq!(counting.jobs_calls.load(Ordering::SeqCst), 0, "the old attempt's jobs are not even requested");

    // The opposite mix: attempt 1 jobs green, but the latest attempt failed overall. Still no pass, still not a CI failure verdict.
    let mut green_old_red_latest = healthy(2);
    let mut latest = run(2);
    latest.conclusion = Some("failure".to_string());
    green_old_red_latest.run_result = Some(Ok(latest));
    let r = assess_ci_evidence_latest_attempt_only(&green_old_red_latest, &request(1));
    never_a_pass(&r);
    assert_eq!(r.error, Some(ErrorClass::AttemptNotLatest), "{r:?}");

    // The same evidence through the unguarded P2A entry point WOULD have produced the mixed answer; that is why P2B does not use it.
    let unguarded = assess_ci_evidence(&healthy(2), &request(1));
    assert_eq!(unguarded.outcome, Outcome::AssessedLimitations, "documents the P2A behaviour the guard replaces");
}

#[test]
fn an_attempt_that_does_not_exist_yet_is_refused_too() {
    let r = assess_ci_evidence_latest_attempt_only(&healthy(2), &request(3));
    never_a_pass(&r);
    assert_eq!((r.outcome, r.error), (Outcome::NotAssessed, Some(ErrorClass::AttemptNotLatest)), "{r:?}");
}

// ------------------------------------------------------------------------------------------------------------------
// Identity: ECE binds the numeric ID; the names are checked here
// ------------------------------------------------------------------------------------------------------------------

#[test]
fn a_provider_whose_names_disagree_with_the_fixed_target_fails_closed() {
    type Mutate = fn(&mut MockGitHubProvider);
    let cases: Vec<(&str, Mutate)> = vec![
        ("repository full name", |p| {
            let mut r = repo(REPOSITORY_ID);
            r.full_name = "ErenTahiroglu/Another-Repo".to_string();
            p.repo_result = Some(Ok(r));
        }),
        ("repository name", |p| {
            let mut r = repo(REPOSITORY_ID);
            r.name = "ai-toolchain-manager".to_string();
            p.repo_result = Some(Ok(r));
        }),
        ("owner login", |p| {
            let mut r = repo(REPOSITORY_ID);
            r.owner.login = "someone-else".to_string();
            p.repo_result = Some(Ok(r));
        }),
        ("run repository full name", |p| {
            let mut r = run(1);
            r.repository = Some(GitHubRunRepo { id: REPOSITORY_ID, full_name: "ErenTahiroglu/Fork".to_string() });
            p.run_result = Some(Ok(r));
        }),
    ];
    for (what, mutate) in cases {
        let mut p = healthy(1);
        mutate(&mut p);
        let r = assess_ci_evidence_latest_attempt_only(&p, &request(1));
        never_a_pass(&r);
        assert_eq!(r.error, Some(ErrorClass::ProviderIdentityInconsistent), "{what}: {r:?}");
        assert_eq!(r.outcome, Outcome::NotAssessed, "{what}");
    }
}

#[test]
fn a_wrong_numeric_id_commit_run_or_attempt_cannot_yield_a_machine_pass() {
    type Mutate = fn(&mut MockGitHubProvider);
    let cases: Vec<(&str, Mutate)> = vec![
        ("another numeric repository id", |p| p.repo_result = Some(Ok(repo(1_432_704_795)))),
        ("commit lookup returns another sha", |p| {
            p.commit_result = Some(Ok(GitHubCommit {
                sha: "0123456789abcdef0123456789abcdef01234567".into(),
                commit: GitHubCommitDetails { message: None, author: None },
            }))
        }),
        ("run belongs to another head sha", |p| {
            let mut r = run(1);
            r.head_sha = "0123456789abcdef0123456789abcdef01234567".to_string();
            p.run_result = Some(Ok(r));
        }),
        ("another run id returned", |p| {
            let mut r = run(1);
            r.id = RUN_ID + 1;
            p.run_result = Some(Ok(r));
        }),
        ("another workflow", |p| {
            let mut r = run(1);
            r.path = Some(".github/workflows/other.yml".to_string());
            p.run_result = Some(Ok(r));
        }),
        ("run from another repository id", |p| {
            let mut r = run(1);
            r.repository = Some(GitHubRunRepo { id: 1_432_704_795, full_name: REPOSITORY_FULL_NAME.to_string() });
            p.run_result = Some(Ok(r));
        }),
        ("run not completed", |p| {
            let mut r = run(1);
            r.status = "in_progress".into();
            r.conclusion = None;
            p.run_result = Some(Ok(r));
        }),
    ];
    for (what, mutate) in cases {
        let mut p = healthy(1);
        mutate(&mut p);
        let r = assess_ci_evidence_latest_attempt_only(&p, &request(1));
        never_a_pass(&r);
        assert_ne!(r.outcome, Outcome::AssessedLimitations, "{what}: {r:?}");
    }
}

#[test]
fn failed_missing_skipped_cancelled_extra_and_duplicate_required_jobs_never_become_verified_pass() {
    let cases: Vec<(&str, Vec<GitHubJob>)> = vec![
        ("failed", vec![job(1, "windows-x64", "failure"), job(2, "macos-arm64", "success")]),
        ("timed out", vec![job(1, "windows-x64", "success"), job(2, "macos-arm64", "timed_out")]),
        ("cancelled", vec![job(1, "windows-x64", "cancelled"), job(2, "macos-arm64", "success")]),
        ("skipped", vec![job(1, "windows-x64", "success"), job(2, "macos-arm64", "skipped")]),
        ("one missing", vec![job(1, "windows-x64", "success")]),
        ("none", Vec::new()),
        ("extra job", vec![job(1, "windows-x64", "success"), job(2, "macos-arm64", "success"), job(3, "extra", "success")]),
        ("duplicate", vec![job(1, "windows-x64", "success"), job(3, "windows-x64", "success"), job(2, "macos-arm64", "success")]),
        (
            "contradictory duplicate",
            vec![job(1, "macos-arm64", "failure"), job(3, "macos-arm64", "success"), job(2, "windows-x64", "success")],
        ),
        ("renamed", vec![job(1, "Windows x64", "success"), job(2, "macos-arm64", "success")]),
    ];
    for (what, list) in cases {
        let mut p = healthy(1);
        p.jobs_result = Some(Ok(list));
        let r = assess_ci_evidence_latest_attempt_only(&p, &request(1));
        never_a_pass(&r);
        if what == "extra job" {
            // ECE's own dimension stays VERIFIED_PASS (both required jobs passed); the policy forbids extra jobs, so the outcome fails.
            assert_eq!(r.outcome, Outcome::AssessedFail, "{r:?}");
            assert!(r.blocking.contains(&BlockingClass::UnexpectedJobSet), "{r:?}");
        } else {
            assert_ne!(r.ci, CiResult::VerifiedPass, "{what}: {r:?}");
        }
        assert!(r.error.is_none(), "{what} is an observation about the run, not a provider error: {r:?}");
    }
}

// ------------------------------------------------------------------------------------------------------------------
// 11. Provider and network errors stay unavailable; they are never an observed CI failure
// ------------------------------------------------------------------------------------------------------------------

fn client_errors() -> Vec<(GitHubClientError, ProviderFailureKind)> {
    vec![
        (GitHubClientError::RateLimited { reset_at: Some(1_900_000_000), message: "limit".into() }, ProviderFailureKind::RateLimited),
        (GitHubClientError::NotFound { resource: "https://api.github.com/x".into() }, ProviderFailureKind::NotFound),
        (GitHubClientError::Unauthorized("403".into()), ProviderFailureKind::Forbidden),
        (
            GitHubClientError::HttpError { status: 502, message: "bad gateway".into(), rate_limit_reset: None },
            ProviderFailureKind::HttpError,
        ),
        (GitHubClientError::Timeout("t".into()), ProviderFailureKind::Timeout),
        (GitHubClientError::NetworkError("dns".into()), ProviderFailureKind::NetworkUnavailable),
        (GitHubClientError::InvalidJson("j".into()), ProviderFailureKind::InvalidResponse),
        (GitHubClientError::EmptyResponse, ProviderFailureKind::InvalidResponse),
        (GitHubClientError::SizeLimitExceeded(5), ProviderFailureKind::ResponseTooLarge),
        (GitHubClientError::IncompletePagination { retrieved: 1, total_count: 2, page_limit: 1 }, ProviderFailureKind::IncompleteResponse),
    ]
}

#[test]
fn a_provider_error_on_any_lookup_is_insufficient_evidence_with_its_closed_kind() {
    for (error, kind) in client_errors() {
        for stage in ["repository", "run", "jobs", "commit"] {
            // A 404 about the COMMIT is a legitimate answer ("no such commit"); it is covered separately below.
            if stage == "commit" && matches!(error, GitHubClientError::NotFound { .. }) {
                continue;
            }
            let mut p = healthy(1);
            match stage {
                "repository" => p.repo_result = Some(Err(error.clone())),
                "run" => p.run_result = Some(Err(error.clone())),
                "jobs" => p.jobs_result = Some(Err(error.clone())),
                _ => p.commit_result = Some(Err(error.clone())),
            }
            let r = assess_ci_evidence_latest_attempt_only(&p, &request(1));
            never_a_pass(&r);
            let what = format!("{stage} {error:?}");
            assert_eq!(r.outcome, Outcome::InsufficientEvidence, "{what}: {r:?}");
            assert_eq!(r.error, Some(ErrorClass::ProviderFailure), "{what}");
            assert_eq!(r.provider_failure, Some(kind), "{what}");
            assert_eq!(r.ci, CiResult::UnverifiedRun, "{what}");
            assert!(r.blocking.is_empty(), "{what}: a provider failure must not read as an observed CI failure: {r:?}");
            assert_ne!(r.outcome, Outcome::AssessedFail, "{what}");
            assert_eq!(r.freshness, Freshness::NotEvaluated);
        }
    }
}

#[test]
fn a_missing_commit_is_an_observation_but_an_unreachable_commit_lookup_is_not() {
    let mut missing = healthy(1);
    missing.commit_result = Some(Err(GitHubClientError::NotFound { resource: "commit".into() }));
    let r = assess_ci_evidence_latest_attempt_only(&missing, &request(1));
    never_a_pass(&r);
    assert_eq!(r.outcome, Outcome::AssessedFail);
    assert!(r.blocking.contains(&BlockingClass::CommitNotVerified), "{r:?}");
    assert!(r.error.is_none() && r.provider_failure.is_none());
}

#[test]
fn artifacts_are_never_requested_and_nothing_is_ever_downloaded() {
    let mut inner = healthy(1);
    inner.artifacts_result = Some(Err(GitHubClientError::NetworkError("would have failed".into())));
    let counting = Counting::new(inner);
    let r = assess_ci_evidence_latest_attempt_only(&counting, &request(1));
    assert!(r.machine_ci_verified(), "{r:?}");
    assert_eq!(r.availability, EvidenceAvailability::NotProvided);
    assert_eq!(counting.artifact_calls.load(Ordering::SeqCst), 0);
    assert_eq!(counting.download_calls.load(Ordering::SeqCst), 0);
    assert_eq!(counting.repo_calls.load(Ordering::SeqCst), 1);
    assert_eq!(counting.jobs_calls.load(Ordering::SeqCst), 1);
}

// ------------------------------------------------------------------------------------------------------------------
// The closed report that crosses the IPC boundary
// ------------------------------------------------------------------------------------------------------------------

#[test]
fn the_serialized_report_is_closed_and_never_says_accepted() {
    let r = assess_ci_evidence_latest_attempt_only(&healthy(1), &request(1));
    let v = serde_json::to_value(&r).unwrap();
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "authorization",
            "availability",
            "blocking",
            "ci",
            "error",
            "freshness",
            "limitations",
            "outcome",
            "provenance",
            "providerFailure",
            "review",
            "schemaVersion"
        ]
    );
    assert_eq!(v["schemaVersion"], "atm.ece-ci-assessment.v1");
    assert_eq!(v["outcome"], "ASSESSED_LIMITATIONS");
    assert_eq!(v["ci"], "VERIFIED_PASS");
    assert_eq!(v["review"], "UNREVIEWED");
    assert_eq!(v["authorization"], "AUTHORIZATION_PENDING");
    assert_eq!(v["freshness"], "HISTORICAL_BASELINE");
    assert_eq!(v["error"], serde_json::Value::Null);
    let p = &v["provenance"];
    assert_eq!(p["eceRevision"], "9f4e53e0a944a7681fb49957f0d5e30597c3148a");
    assert_eq!(p["policyId"], "atm-ci-two-runner-v1");
    assert_eq!(p["policyFileSha256"], "f523eda2bca9d2512dc372fee476bb9dc094d3cebd29b6be27ce2a38df2e1c1f");
    assert_eq!(p["repositoryId"], 1_413_039_335_u64);
    assert_eq!(p["workflowPath"], ".github/workflows/ci.yml");
    assert_eq!((p["commitSha"].as_str(), p["runId"].as_u64(), p["runAttempt"].as_u64()), (Some(COMMIT), Some(RUN_ID), Some(1)));
    let text = v.to_string().to_lowercase();
    for banned in ["accepted", "approved", "authorized\"", "checkpoint_accepted", "http", "token", "password"] {
        assert!(!text.contains(banned), "the closed report must not contain {banned:?}: {text}");
    }
    // Every string value is an upper-case enum token, a fixed constant or the validated request echo.
    fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            serde_json::Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut strings = Vec::new();
    walk(&v, &mut strings);
    for s in strings {
        let enum_like = s.chars().all(|c| c.is_ascii_uppercase() || c == '_');
        let fixed = [REPOSITORY_FULL_NAME, WORKFLOW_PATH, POLICY_ID, POLICY_FILE_SHA256, ECE_REVISION, COMMIT, REPORT_SCHEMA_VERSION];
        assert!(enum_like || fixed.contains(&s.as_str()), "unexpected free text in the report: {s:?}");
    }
}

#[test]
fn a_provider_failure_report_serializes_its_closed_kind() {
    let mut p = healthy(1);
    p.run_result = Some(Err(GitHubClientError::RateLimited { reset_at: Some(1), message: "ghp_secret_not_to_be_copied".into() }));
    let r = assess_ci_evidence_latest_attempt_only(&p, &request(1));
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["error"], "PROVIDER_FAILURE");
    assert_eq!(v["providerFailure"], "RATE_LIMITED");
    assert_eq!(v["outcome"], "INSUFFICIENT_EVIDENCE");
    assert!(!v.to_string().contains("ghp_secret"));
}

// ------------------------------------------------------------------------------------------------------------------
// 4/12/J. The REAL anonymous client, against a loopback listener only
// ------------------------------------------------------------------------------------------------------------------

/// A one-connection HTTP/1.1 server on 127.0.0.1 that records the request head and answers with `respond(head)`.
fn loopback(respond: impl Fn(&str) -> String + Send + 'static) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut head = Vec::new();
        let mut buf = [0u8; 1024];
        while !head.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut buf).expect("read");
            if n == 0 {
                break;
            }
            head.extend_from_slice(&buf[..n]);
        }
        let head = String::from_utf8_lossy(&head).into_owned();
        stream.write_all(respond(&head).as_bytes()).expect("write");
        stream.flush().ok();
        head
    });
    (base, handle)
}

const REPO_JSON: &str = r#"{"id":1413039335,"name":"AI-Toolchain-Manager","full_name":"ErenTahiroglu/AI-Toolchain-Manager","owner":{"login":"ErenTahiroglu","id":149682795},"default_branch":"main"}"#;

fn json_response(body: &str) -> String {
    format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

#[test]
fn ambient_github_tokens_never_become_an_authorization_header_in_the_production_execution_context() {
    // The same execution context as the command: a tokio blocking-pool thread (creating, using and dropping the blocking client there).
    let (base, server) = loopback(|_| json_response(REPO_JSON));
    std::env::set_var("GITHUB_TOKEN", "ambient-token-aaaa1111");
    std::env::set_var("GH_TOKEN", "ambient-token-bbbb2222");
    let result = tauri::async_runtime::block_on(async move {
        tauri::async_runtime::spawn_blocking(move || {
            // Test-only: the production path cannot name a base URL; this proves the anonymous behaviour against loopback.
            let client = HttpGitHubClient::new_unauthenticated_with_base_url(base).expect("anonymous client");
            client.get_repository(REPOSITORY_OWNER, REPOSITORY_NAME)
        })
        .await
    });
    std::env::remove_var("GITHUB_TOKEN");
    std::env::remove_var("GH_TOKEN");
    let head = server.join().unwrap().to_lowercase();
    let fetched = result.expect("the blocking task finished").expect("loopback answered");
    assert_eq!(fetched.id, REPOSITORY_ID);
    assert!(head.starts_with("get /repos/erentahiroglu/ai-toolchain-manager http/1.1"), "{head}");
    assert!(!head.contains("authorization"), "an Authorization header was sent: {head}");
    assert!(!head.contains("ambient-token"), "an ambient token was sent: {head}");
    assert!(!head.contains("cookie"), "{head}");
    assert!(head.contains("user-agent:") && head.contains("x-github-api-version: 2022-11-28"), "{head}");
}

#[test]
fn a_redirect_to_another_scheme_or_host_is_refused_by_the_anonymous_client_and_reported_as_a_network_failure() {
    for location in ["https://127.0.0.1:1/escaped", "http://127.0.0.2:1/escaped", "http://example.invalid/escaped"] {
        let target = location.to_string();
        let (base, server) =
            loopback(move |_| format!("HTTP/1.1 302 Found\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"));
        let client = HttpGitHubClient::new_unauthenticated_with_base_url(base).expect("anonymous client");
        let err = client.get_repository(REPOSITORY_OWNER, REPOSITORY_NAME).expect_err("the redirect must not be followed");
        server.join().unwrap();
        assert_eq!(ProviderFailureKind::from_client_error(&err), ProviderFailureKind::NetworkUnavailable, "{location}: {err:?}");
    }
}

// ------------------------------------------------------------------------------------------------------------------
// 4/5/15. Static boundary: one constructor, one command, no startup or UI-free network
// ------------------------------------------------------------------------------------------------------------------

fn repo_file(relative: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)).unwrap().replace("\r\n", "\n")
}

fn production_sources() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                stack.push(path);
            } else if name.ends_with(".rs") && !name.ends_with("_tests.rs") && !name.starts_with("tests") {
                let text = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
                // Production code only: everything before the first unit-test module.
                let production = text.split("#[cfg(test)]").next().unwrap();
                // Comments are documentation, not behaviour: drop whole-line comments before scanning.
                let code: Vec<&str> = production.lines().filter(|l| !l.trim_start().starts_with("//")).collect();
                out.push((name, code.join("\n")));
            }
        }
    }
    out
}

#[test]
fn only_the_anonymous_constructor_is_named_and_only_in_the_live_module() {
    let mut users = Vec::new();
    for (name, text) in production_sources() {
        if text.contains("HttpGitHubClient") {
            users.push(name.clone());
            // The legacy and base-URL constructors must not be named anywhere in production code.
            for forbidden in
                ["HttpGitHubClient::new(", "new_with_base_url", "new_unauthenticated_with_base_url", "GITHUB_TOKEN", "GH_TOKEN", "bearer"]
            {
                assert!(!text.contains(forbidden), "{name} names {forbidden}");
            }
            assert_eq!(text.matches("HttpGitHubClient::new_unauthenticated()").count(), 1, "{name}");
        }
    }
    assert_eq!(users, vec!["ece_ci_live.rs".to_string()], "HttpGitHubClient may be named only in ece_ci_live.rs");
}

#[test]
fn nothing_but_the_one_command_reaches_the_live_provider_so_nothing_runs_at_startup() {
    for (name, text) in production_sources() {
        if name == "ece_ci_live.rs" || name == "commands.rs" {
            continue;
        }
        for needle in ["anonymous_provider", "assess_live", "assess_checked", "assess_ci_evidence_latest_attempt_only"] {
            // `ece_ci.rs` defines the latest-attempt-only entry; no other file may call any of them.
            if name == "ece_ci.rs" && needle == "assess_ci_evidence_latest_attempt_only" {
                continue;
            }
            assert!(!text.contains(needle), "{name} reaches {needle}");
        }
    }
    let commands = repo_file("src/commands.rs");
    assert_eq!(commands.matches("ece_ci_live::anonymous_provider").count(), 1);
    let lib = repo_file("src/lib.rs");
    let setup = lib.split("fn setup").nth(1).unwrap().split("pub fn run").next().unwrap();
    for needle in ["ece_ci", "assess_atm_checkpoint", "assess_live", "anonymous_provider", "HttpGitHubClient"] {
        assert!(!setup.contains(needle), "startup reaches {needle}");
    }
}

#[test]
fn the_capability_gains_exactly_one_permission_and_it_is_the_named_command() {
    let caps: serde_json::Value = serde_json::from_str(&repo_file("capabilities/default.json")).unwrap();
    let permissions: Vec<&str> = caps["permissions"].as_array().unwrap().iter().map(|p| p.as_str().unwrap()).collect();
    assert_eq!(permissions.len(), 38, "37 earlier permissions plus exactly one");
    let new: Vec<&&str> = permissions.iter().filter(|p| p.contains("assess")).collect();
    assert_eq!(new, [&"allow-assess-atm-checkpoint"]);
    for p in &permissions {
        assert!(!p.contains("http") && !p.contains("fs") && !p.contains("shell") && !p.contains("process") && !p.contains("sql"), "{p}");
    }
    let build = repo_file("build.rs");
    assert_eq!(build.matches("\"assess_atm_checkpoint\"").count(), 1);
    assert_eq!(repo_file("src/commands.rs").matches("#[tauri::command]").count(), 38);
    // No network permission for the WebView: the policy stays "ipc only".
    let conf = repo_file("tauri.conf.json");
    assert!(conf.contains("connect-src ipc: http://ipc.localhost;"));
}
