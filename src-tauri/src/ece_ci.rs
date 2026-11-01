//! ATM-GOV-02 P2A: the offline consumer of the Evidence-Checkpoint-Engine (ECE) verifier.
//!
//! WHAT THIS IS. A closed, bounded projection of ECE's assessment of ONE historical GitHub Actions run of this repository.
//! It calls the real ECE evaluator through its library interface (`SchemaStore::from_embedded()` and
//! `evaluate_github_evidence_with_store()`), against a `GitHubProvider` the CALLER supplies, with the operator-owned policy
//! `.github/ece/atm-ci-two-runner-v1.json`. The decision algorithm lives in ECE; nothing here reimplements it.
//!
//! WHAT THIS IS NOT. THIS module makes no network request, builds no HTTP client, reads no token or environment variable, spawns no
//! process and touches no file and no database. The live provider is constructed in `ece_ci_live.rs` (P2B), by one user-initiated
//! command, as the ANONYMOUS client only. The manually pasted Checkpoint Center viewer is unchanged and remains UNTRUSTED.
//!
//! P2B ADDITIONS (this module still decides nothing about CI itself): `assess_ci_evidence_latest_attempt_only` wraps the caller's
//! provider in a guard that (a) refuses a run whose LATEST attempt is not the requested one, because ECE reads run-level facts
//! from the unscoped run endpoint and jobs from the requested attempt, which would mix two attempts after a re-run; (b) fails
//! closed when the names the provider returns disagree with the fixed target; (c) classifies a provider/network failure as such,
//! never as an observed CI failure; (d) never asks the provider for artifacts (the request declares none).
//!
//! WHAT THE RESULT MEANS (the vocabulary is ECE's, kept exactly):
//! * `CiResult::VerifiedPass`     the defined machine CI checks (both required jobs) passed for that run attempt.
//! * `Outcome::AssessedLimitations` the checkpoint remains LIMITED; it is the best outcome a machine can reach here.
//! * `ReviewState::Unreviewed`    no independent human review was verified.
//! * `AuthorizationState::AuthorizationPending` no human authorization was verified.
//! * `Freshness::HistoricalBaseline` the run is a past observation; it does NOT certify the current HEAD.
//!
//! There is deliberately no field, variant or helper that means "accepted": nothing a machine produces can claim it.
//!
//! TRUST BOUNDARY. The policy bytes are embedded at build time and checked against an INDEPENDENT expected SHA-256 on every
//! call. The `policy_digest` field inside the file is self-declared and is NOT a trust anchor. The result copies no JSON, URL,
//! path or free text from ECE or from the provider: every field is a closed enum, a number, or a constant of this module.
#![allow(dead_code)] // the P2A entry point `assess_ci_evidence` keeps its tests but has no production caller; P2B uses the latest-attempt-only entry

use ece_lib::github::models::{GitHubArtifact, GitHubCommit, GitHubJob, GitHubRepository, GitHubWorkflowRun};
use ece_lib::github::{evaluate_github_evidence_with_store, GitHubClientError, GitHubProvider, TargetEvidenceRequest, TrustedCheckPolicy};
use ece_lib::SchemaStore;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Mutex;

/// The full, immutable ECE revision this build depends on (Cargo.toml pins the same value; a test compares them).
pub const ECE_REVISION: &str = "9f4e53e0a944a7681fb49957f0d5e30597c3148a";
/// The independently expected SHA-256 of the operator-owned policy file. Not read from the file itself.
pub const POLICY_FILE_SHA256: &str = "f523eda2bca9d2512dc372fee476bb9dc094d3cebd29b6be27ce2a38df2e1c1f";
pub const POLICY_ID: &str = "atm-ci-two-runner-v1";
pub const REPOSITORY_OWNER: &str = "ErenTahiroglu";
pub const REPOSITORY_NAME: &str = "AI-Toolchain-Manager";
pub const REPOSITORY_FULL_NAME: &str = "ErenTahiroglu/AI-Toolchain-Manager";
pub const REPOSITORY_ID: u64 = 1_413_039_335;
pub const WORKFLOW_PATH: &str = ".github/workflows/ci.yml";
pub const REQUIRED_JOBS: [&str; 2] = ["windows-x64", "macos-arm64"];

/// The operator-owned policy, embedded at build time (so there is no runtime file read) and verified on every use.
pub(crate) const POLICY_BYTES: &[u8] = include_bytes!("../../.github/ece/atm-ci-two-runner-v1.json");
/// The only assessment schema version this projection understands.
const SUPPORTED_ASSESSMENT_SCHEMA: &str = "0.1.0-draft";
/// The version of the closed report that crosses the IPC boundary.
pub const REPORT_SCHEMA_VERSION: &str = "atm.ece-ci-assessment.v1";

// ---------------------------------------------------------------------------------------------------------------------
// The closed result
// ---------------------------------------------------------------------------------------------------------------------

/// ECE's overall verdict. `NotAssessed` is this module's own value for "no assessment was produced" (see [`ErrorClass`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Outcome {
    AssessedLimitations,
    AssessedFail,
    InsufficientEvidence,
    NotAssessed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CiResult {
    VerifiedPass,
    Failed,
    IncompleteSkipped,
    AttemptMismatch,
    UnverifiedRun,
    NotEvaluated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceAvailability {
    NotProvided,
    Available,
    PartiallyAvailable,
    Expired,
    Unreachable,
    NotEvaluated,
}

/// Only the two states a machine can produce. A "reviewed" state is not representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewState {
    Unreviewed,
    NotEvaluated,
}

/// The single state P2A can report. "Authorized" is not representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorizationState {
    AuthorizationPending,
}

/// A past observation, never a statement about the current HEAD (`CurrentHead` is not representable), or "not evaluated" when
/// no assessment of the run was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Freshness {
    HistoricalBaseline,
    NotEvaluated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Limitation {
    NoIndependentReview,
    NoHumanAuthorization,
    NotCurrentHeadAttestation,
    StandaloneRunAssessment,
    CommitSignatureNotVerified,
    HostedRunnersOnly,
}

/// Why the outcome is not a pass. Derived from ECE's structured fields; the few reasons ECE only states in prose are matched by
/// their fixed prefix and never copied. Anything unrecognised becomes `Unclassified`, which can only ever lower the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BlockingClass {
    RepositoryIdentityMismatch,
    CommitNotVerified,
    WorkflowMismatch,
    RunAttemptMismatch,
    RequiredJobsFailedOrMissing,
    RequiredJobsSkippedOrCancelled,
    RunOrJobsNotCompleted,
    ArtifactExpired,
    /// The run reports jobs beyond the policy's required set while the policy forbids extra jobs (`allow_extra_jobs: false`).
    /// ECE does not enforce that policy field itself, so this module does, conservatively.
    UnexpectedJobSet,
    Unclassified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorClass {
    /// The embedded policy bytes are not the independently expected bytes, or do not say what this module is written for.
    PolicyIntegrity,
    /// ECE's embedded schemas could not be compiled.
    SchemaUnavailable,
    /// ECE refused to emit an assessment because it failed ECE's own output schema (fail closed).
    OutputSchemaViolation,
    /// ECE returned another error.
    EvaluatorFailure,
    /// The provider could not supply the evidence (ECE assessed it as INSUFFICIENT_EVIDENCE).
    ProviderFailure,
    /// The assessment is not of the supported schema version.
    UnsupportedSchemaVersion,
    /// The assessment is not the closed shape this projection accepts, or contradicts itself.
    MalformedAssessment,
    /// The assessment is about a different target than the one asked for.
    BindingMismatch,
    /// P2B: the run's latest attempt is not the requested attempt (superseded by a re-run, or not existing yet). Never substituted.
    AttemptNotLatest,
    /// P2B: the names the provider returned disagree with the fixed target although the numeric ID may match. Fails closed.
    ProviderIdentityInconsistent,
    /// P2B: the live client could not be constructed.
    ClientInitFailure,
}

/// WHY the provider could not supply the evidence. A network or provider failure is never an observed CI failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProviderFailureKind {
    RateLimited,
    NetworkUnavailable,
    Timeout,
    /// HTTP 401 or 403 that is not a rate limit.
    Forbidden,
    NotFound,
    /// Any other HTTP error status.
    HttpError,
    InvalidResponse,
    ResponseTooLarge,
    IncompleteResponse,
}

impl ProviderFailureKind {
    pub(crate) fn from_client_error(e: &GitHubClientError) -> Self {
        match e {
            GitHubClientError::RateLimited { .. } => Self::RateLimited,
            GitHubClientError::NotFound { .. } => Self::NotFound,
            GitHubClientError::Unauthorized(_) => Self::Forbidden,
            GitHubClientError::HttpError { .. } => Self::HttpError,
            GitHubClientError::Timeout(_) => Self::Timeout,
            GitHubClientError::NetworkError(_) => Self::NetworkUnavailable,
            GitHubClientError::InvalidJson(_) | GitHubClientError::EmptyResponse => Self::InvalidResponse,
            GitHubClientError::SizeLimitExceeded(_) => Self::ResponseTooLarge,
            GitHubClientError::IncompletePagination { .. } => Self::IncompleteResponse,
        }
    }
}

/// Where the assessment came from and what it is bound to. All constants of this module or validated request values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    pub ece_revision: &'static str,
    pub policy_id: &'static str,
    pub policy_file_sha256: &'static str,
    pub repository: &'static str,
    pub repository_id: u64,
    pub workflow_path: &'static str,
    pub commit_sha: String,
    pub run_id: u64,
    pub run_attempt: u32,
}

/// The bounded result. Nothing in it is attacker-controlled free text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CiEvidenceReport {
    pub schema_version: &'static str,
    pub provenance: Provenance,
    pub outcome: Outcome,
    pub ci: CiResult,
    pub availability: EvidenceAvailability,
    pub review: ReviewState,
    pub authorization: AuthorizationState,
    pub freshness: Freshness,
    pub limitations: Vec<Limitation>,
    pub blocking: Vec<BlockingClass>,
    pub error: Option<ErrorClass>,
    /// Set only when `error` is `ProviderFailure`: the closed reason the evidence could not be obtained.
    pub provider_failure: Option<ProviderFailureKind>,
}

impl CiEvidenceReport {
    /// True only when the defined machine CI checks passed and nothing blocks. It is NOT acceptance of anything.
    pub fn machine_ci_verified(&self) -> bool {
        self.outcome == Outcome::AssessedLimitations
            && self.ci == CiResult::VerifiedPass
            && self.blocking.is_empty()
            && self.error.is_none()
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The request
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestError {
    CommitShaNotFortyLowercaseHex,
    RunIdZero,
    RunAttemptZero,
}

/// What to assess: one exact commit, one run, one attempt, of the fixed repository and workflow. Validated at construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CiEvidenceRequest {
    commit_sha: String,
    run_id: u64,
    run_attempt: u32,
}

impl CiEvidenceRequest {
    pub fn new(commit_sha: &str, run_id: u64, run_attempt: u32) -> Result<Self, RequestError> {
        if commit_sha.len() != 40 || !commit_sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err(RequestError::CommitShaNotFortyLowercaseHex);
        }
        if run_id == 0 {
            return Err(RequestError::RunIdZero);
        }
        if run_attempt == 0 {
            return Err(RequestError::RunAttemptZero);
        }
        Ok(Self { commit_sha: commit_sha.to_string(), run_id, run_attempt })
    }

    fn provenance(&self) -> Provenance {
        Provenance {
            ece_revision: ECE_REVISION,
            policy_id: POLICY_ID,
            policy_file_sha256: POLICY_FILE_SHA256,
            repository: REPOSITORY_FULL_NAME,
            repository_id: REPOSITORY_ID,
            workflow_path: WORKFLOW_PATH,
            commit_sha: self.commit_sha.clone(),
            run_id: self.run_id,
            run_attempt: self.run_attempt,
        }
    }

    /// ECE's request: exactly the fixed binding, and NO source-declared claim (phase, producer, policy reference or digest,
    /// artifacts). Nothing a checkpoint says about itself is passed to the evaluator.
    pub(crate) fn ece_request(&self) -> TargetEvidenceRequest {
        TargetEvidenceRequest {
            owner: REPOSITORY_OWNER.to_string(),
            repo: REPOSITORY_NAME.to_string(),
            expected_repo_id: REPOSITORY_ID.to_string(),
            commit_sha: self.commit_sha.clone(),
            run_id: self.run_id,
            run_attempt: self.run_attempt,
            expected_workflow_path: Some(WORKFLOW_PATH.to_string()),
            checkpoint_id: None,
            declared_phase: None,
            declared_producer: None,
            declared_policy_ref: None,
            declared_policy_digest: None,
            declared_artifacts: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The operator-owned policy
// ---------------------------------------------------------------------------------------------------------------------

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Verifies `bytes` against the independent expected SHA-256 and only then parses them. The file's own `policy_digest` is
/// never consulted for trust. The parsed policy must additionally say exactly what this module is written for.
pub(crate) fn load_policy(bytes: &[u8]) -> Result<TrustedCheckPolicy, ErrorClass> {
    if sha256_hex(bytes) != POLICY_FILE_SHA256 {
        return Err(ErrorClass::PolicyIntegrity);
    }
    let policy: TrustedCheckPolicy = serde_json::from_slice(bytes).map_err(|_| ErrorClass::PolicyIntegrity)?;
    if policy_says_what_this_module_expects(&policy) {
        Ok(policy)
    } else {
        Err(ErrorClass::PolicyIntegrity)
    }
}

/// Defence in depth on top of the byte check: the parsed policy must say exactly what this module is written for.
pub(crate) fn policy_says_what_this_module_expects(policy: &TrustedCheckPolicy) -> bool {
    policy.policy_id == POLICY_ID
        && policy.required_jobs.iter().map(String::as_str).eq(REQUIRED_JOBS)
        && !policy.allow_extra_jobs
        && policy.target_repo_id.as_deref() == Some("1413039335")
        && policy.target_workflow_path.as_deref() == Some(WORKFLOW_PATH)
}

// ---------------------------------------------------------------------------------------------------------------------
// Assessment
// ---------------------------------------------------------------------------------------------------------------------

/// Assesses one historical run with the real ECE evaluator against the caller's provider. Never panics; every failure is a
/// closed [`ErrorClass`] and never a pass.
pub fn assess_ci_evidence(provider: &dyn GitHubProvider, request: &CiEvidenceRequest) -> CiEvidenceReport {
    let policy = match load_policy(POLICY_BYTES) {
        Ok(p) => p,
        Err(e) => return not_assessed(request, e),
    };
    let store = match SchemaStore::from_embedded() {
        Ok(s) => s,
        Err(_) => return not_assessed(request, ErrorClass::SchemaUnavailable),
    };
    assess_with(provider, request, &policy, &store)
}

/// The shared core. The tests use it with a mutated policy or a different schema store to prove the fail-closed paths.
pub(crate) fn assess_with(
    provider: &dyn GitHubProvider,
    request: &CiEvidenceRequest,
    policy: &TrustedCheckPolicy,
    store: &SchemaStore,
) -> CiEvidenceReport {
    evaluate_request(provider, &request.ece_request(), request, policy, store)
}

pub(crate) fn evaluate_request(
    provider: &dyn GitHubProvider,
    ece_request: &TargetEvidenceRequest,
    request: &CiEvidenceRequest,
    policy: &TrustedCheckPolicy,
    store: &SchemaStore,
) -> CiEvidenceReport {
    match evaluate_github_evidence_with_store(provider, ece_request, policy, store, None) {
        Ok(output) => match project(&output.assessment, output.exit_code, request, policy) {
            Ok(report) => report,
            Err(e) => not_assessed(request, e),
        },
        Err(ece_lib::VerifierError::OutputSchemaViolation { .. }) => not_assessed(request, ErrorClass::OutputSchemaViolation),
        Err(_) => not_assessed(request, ErrorClass::EvaluatorFailure),
    }
}

pub(crate) fn not_assessed(request: &CiEvidenceRequest, error: ErrorClass) -> CiEvidenceReport {
    CiEvidenceReport {
        schema_version: REPORT_SCHEMA_VERSION,
        provenance: request.provenance(),
        outcome: Outcome::NotAssessed,
        ci: CiResult::NotEvaluated,
        availability: EvidenceAvailability::NotEvaluated,
        review: ReviewState::NotEvaluated,
        authorization: AuthorizationState::AuthorizationPending,
        freshness: Freshness::NotEvaluated,
        limitations: vec![Limitation::NoIndependentReview, Limitation::NoHumanAuthorization, Limitation::NotCurrentHeadAttestation],
        blocking: Vec::new(),
        error: Some(error),
        provider_failure: None,
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// P2B: the live-evidence guard. ECE is NOT modified; this wraps the provider ECE is given and judges what it saw.
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Default)]
struct Seen {
    /// The `run_attempt` the run endpoint reported: the run's LATEST attempt.
    latest_attempt: Option<u32>,
    /// A name the provider returned disagrees with the fixed target.
    identity_inconsistent: bool,
    /// The first provider failure that is not a legitimate "does not exist" answer about the commit.
    provider_failure: Option<ProviderFailureKind>,
    /// The provider was asked for the jobs of an attempt other than the run's latest one (it was not called).
    jobs_skipped: bool,
}

/// Records what the real provider returns and enforces three rules before ECE can reach a pass:
/// 1. run-level facts and jobs must describe the SAME attempt (the requested one is the latest), otherwise nothing is assessed;
/// 2. the names the provider reports must be the fixed target's (ECE binds the numeric ID; the names are checked here);
/// 3. a provider failure is reported as a provider failure, even when ECE would have turned it into a blocking reason.
///
/// Artifacts are never requested (the request declares none), so a pass depends on no artifact bytes and no extra request is spent.
pub(crate) struct AttemptGuardedProvider<'a> {
    inner: &'a dyn GitHubProvider,
    requested_attempt: u32,
    seen: Mutex<Seen>,
}

impl<'a> AttemptGuardedProvider<'a> {
    pub(crate) fn new(inner: &'a dyn GitHubProvider, requested_attempt: u32) -> Self {
        Self { inner, requested_attempt, seen: Mutex::new(Seen::default()) }
    }

    fn note(&self, f: impl FnOnce(&mut Seen)) {
        if let Ok(mut seen) = self.seen.lock() {
            f(&mut seen);
        }
    }

    fn fail(&self, error: &GitHubClientError) {
        self.note(|seen| {
            if seen.provider_failure.is_none() {
                seen.provider_failure = Some(ProviderFailureKind::from_client_error(error));
            }
        });
    }
}

impl GitHubProvider for AttemptGuardedProvider<'_> {
    fn get_repository(&self, owner: &str, repo: &str) -> Result<GitHubRepository, GitHubClientError> {
        let result = self.inner.get_repository(owner, repo);
        match &result {
            Ok(r) => {
                if r.full_name != REPOSITORY_FULL_NAME || r.name != REPOSITORY_NAME || r.owner.login != REPOSITORY_OWNER {
                    self.note(|seen| seen.identity_inconsistent = true);
                }
            }
            Err(e) => self.fail(e),
        }
        result
    }

    fn get_commit(&self, owner: &str, repo: &str, commit_sha: &str) -> Result<GitHubCommit, GitHubClientError> {
        let result = self.inner.get_commit(owner, repo, commit_sha);
        // "No such commit in this repository" is a legitimate observation about the target; every other error is not.
        if let Err(e) = &result {
            if !matches!(e, GitHubClientError::NotFound { .. }) {
                self.fail(e);
            }
        }
        result
    }

    fn get_workflow_run(&self, owner: &str, repo: &str, run_id: u64) -> Result<GitHubWorkflowRun, GitHubClientError> {
        let result = self.inner.get_workflow_run(owner, repo, run_id);
        match &result {
            Ok(run) => {
                let inconsistent = matches!(&run.repository, Some(r) if r.full_name != REPOSITORY_FULL_NAME);
                self.note(|seen| {
                    seen.latest_attempt = Some(run.run_attempt);
                    if inconsistent {
                        seen.identity_inconsistent = true;
                    }
                });
            }
            Err(e) => self.fail(e),
        }
        result
    }

    fn get_attempt_jobs(&self, owner: &str, repo: &str, run_id: u64, attempt: u32) -> Result<Vec<GitHubJob>, GitHubClientError> {
        let latest = self.seen.lock().ok().and_then(|seen| seen.latest_attempt);
        if matches!(latest, Some(l) if l != self.requested_attempt) {
            // The result will be refused as AttemptNotLatest whatever the jobs say: do not spend a request on them.
            self.note(|seen| seen.jobs_skipped = true);
            return Ok(Vec::new());
        }
        let result = self.inner.get_attempt_jobs(owner, repo, run_id, attempt);
        if let Err(e) = &result {
            self.fail(e);
        }
        result
    }

    fn get_artifacts(&self, _owner: &str, _repo: &str, _run_id: u64) -> Result<Vec<GitHubArtifact>, GitHubClientError> {
        Ok(Vec::new())
    }

    fn download_artifact_digest(
        &self,
        _owner: &str,
        _repo: &str,
        _artifact_id: u64,
        _max_bytes: usize,
    ) -> Result<String, GitHubClientError> {
        // Never reachable (no artifact is declared) and never a download: refuse rather than reach the network.
        Err(GitHubClientError::NotFound { resource: "artifact download is not part of this assessment".to_string() })
    }
}

fn provider_unavailable(request: &CiEvidenceRequest, kind: ProviderFailureKind) -> CiEvidenceReport {
    CiEvidenceReport {
        schema_version: REPORT_SCHEMA_VERSION,
        provenance: request.provenance(),
        outcome: Outcome::InsufficientEvidence,
        ci: CiResult::UnverifiedRun,
        availability: EvidenceAvailability::Unreachable,
        review: ReviewState::NotEvaluated,
        authorization: AuthorizationState::AuthorizationPending,
        freshness: Freshness::NotEvaluated,
        limitations: vec![Limitation::NoIndependentReview, Limitation::NoHumanAuthorization, Limitation::NotCurrentHeadAttestation],
        blocking: Vec::new(),
        error: Some(ErrorClass::ProviderFailure),
        provider_failure: Some(kind),
    }
}

/// The P2B entry point: [`assess_ci_evidence`] behind the [`AttemptGuardedProvider`]. A pass is possible only when the requested
/// attempt is the run's latest attempt; a superseded (or not yet existing) attempt is `NotAssessed` / `AttemptNotLatest`.
pub fn assess_ci_evidence_latest_attempt_only(provider: &dyn GitHubProvider, request: &CiEvidenceRequest) -> CiEvidenceReport {
    let policy = match load_policy(POLICY_BYTES) {
        Ok(p) => p,
        Err(e) => return not_assessed(request, e),
    };
    let store = match SchemaStore::from_embedded() {
        Ok(s) => s,
        Err(_) => return not_assessed(request, ErrorClass::SchemaUnavailable),
    };
    assess_guarded_with(provider, request, &policy, &store)
}

pub(crate) fn assess_guarded_with(
    provider: &dyn GitHubProvider,
    request: &CiEvidenceRequest,
    policy: &TrustedCheckPolicy,
    store: &SchemaStore,
) -> CiEvidenceReport {
    let guard = AttemptGuardedProvider::new(provider, request.run_attempt);
    let report = assess_with(&guard, request, policy, store);
    let Ok(seen) = guard.seen.lock() else {
        return not_assessed(request, ErrorClass::EvaluatorFailure);
    };
    // A policy/schema/evaluator problem happened before or without any usable evidence: keep it as it is.
    if report.outcome == Outcome::NotAssessed {
        return report;
    }
    // 1. Provider failure beats everything: it is not an observation about the checkpoint.
    if let Some(kind) = seen.provider_failure {
        return provider_unavailable(request, kind);
    }
    // 2. The names the provider reported disagree with the fixed target.
    if seen.identity_inconsistent {
        return not_assessed(request, ErrorClass::ProviderIdentityInconsistent);
    }
    // 3. The run-level facts describe the latest attempt; they may only be used for that attempt.
    if matches!(seen.latest_attempt, Some(latest) if latest != request.run_attempt) {
        return not_assessed(request, ErrorClass::AttemptNotLatest);
    }
    report
}

// ---------------------------------------------------------------------------------------------------------------------
// The projection: ECE's JSON assessment -> the closed result. Strict: an unknown value, a missing field, an acceptance-like
// value or a self-contradiction is an error, never a pass.
// ---------------------------------------------------------------------------------------------------------------------

fn at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |v, key| v.get(*key))
}

fn text<'a>(value: &'a Value, path: &[&str]) -> Result<&'a str, ErrorClass> {
    at(value, path).and_then(Value::as_str).ok_or(ErrorClass::MalformedAssessment)
}

fn count(value: &Value, path: &[&str]) -> Result<u64, ErrorClass> {
    at(value, path).and_then(Value::as_u64).ok_or(ErrorClass::MalformedAssessment)
}

fn flag(value: &Value, path: &[&str]) -> Result<bool, ErrorClass> {
    at(value, path).and_then(Value::as_bool).ok_or(ErrorClass::MalformedAssessment)
}

/// A key that no ECE assessment has and that would mean "accepted" if it ever appeared.
fn contains_key(value: &Value, wanted: &str) -> bool {
    match value {
        Value::Object(map) => map.keys().any(|k| k == wanted) || map.values().any(|v| contains_key(v, wanted)),
        Value::Array(items) => items.iter().any(|v| contains_key(v, wanted)),
        _ => false,
    }
}

pub(crate) fn project(
    assessment: &Value,
    exit_code: i32,
    request: &CiEvidenceRequest,
    policy: &TrustedCheckPolicy,
) -> Result<CiEvidenceReport, ErrorClass> {
    if !assessment.is_object() {
        return Err(ErrorClass::MalformedAssessment);
    }
    if text(assessment, &["schema_version"])? != SUPPORTED_ASSESSMENT_SCHEMA {
        return Err(ErrorClass::UnsupportedSchemaVersion);
    }
    if contains_key(assessment, "checkpoint_accepted") {
        return Err(ErrorClass::MalformedAssessment);
    }
    if text(assessment, &["engine", "name"])? != "Evidence-Checkpoint-Engine" || flag(assessment, &["evaluator_auth", "is_synthetic_mock"])?
    {
        return Err(ErrorClass::MalformedAssessment);
    }

    // The assessment must be about exactly the target that was asked for.
    let bound = text(assessment, &["evaluated_target", "repository_id"])? == REPOSITORY_ID.to_string()
        && text(assessment, &["evaluated_target", "repository_owner"])? == REPOSITORY_OWNER
        && text(assessment, &["evaluated_target", "repository_name"])? == REPOSITORY_NAME
        && text(assessment, &["evaluated_target", "target_commit_sha"])?.eq_ignore_ascii_case(&request.commit_sha);
    if !bound {
        return Err(ErrorClass::BindingMismatch);
    }

    let ci = match text(assessment, &["dimensions", "ci_execution", "status"])? {
        "VERIFIED_PASS" => CiResult::VerifiedPass,
        "FAILED" => CiResult::Failed,
        "INCOMPLETE_SKIPPED" => CiResult::IncompleteSkipped,
        "ATTEMPT_MISMATCH" => CiResult::AttemptMismatch,
        "UNVERIFIED_RUN" => CiResult::UnverifiedRun,
        _ => return Err(ErrorClass::MalformedAssessment),
    };
    let availability = match text(assessment, &["dimensions", "evidence_availability", "status"])? {
        "NOT_PROVIDED" => EvidenceAvailability::NotProvided,
        "AVAILABLE" => EvidenceAvailability::Available,
        "PARTIALLY_AVAILABLE" => EvidenceAvailability::PartiallyAvailable,
        "EXPIRED" => EvidenceAvailability::Expired,
        "UNREACHABLE" => EvidenceAvailability::Unreachable,
        _ => return Err(ErrorClass::MalformedAssessment),
    };
    // A reviewed, passed, self-reported or failed review is NOT representable: this build cannot have verified one.
    let review = match text(assessment, &["dimensions", "review_status", "status"])? {
        "UNREVIEWED" => ReviewState::Unreviewed,
        "NOT_EVALUATED" => ReviewState::NotEvaluated,
        _ => return Err(ErrorClass::MalformedAssessment),
    };
    if text(assessment, &["dimensions", "authorization", "status"])? != "AUTHORIZATION_PENDING"
        || flag(assessment, &["claims_breakdown", "human_authorized", "authorized"])?
        || text(assessment, &["claims_breakdown", "independently_reviewed", "review_verdict"])? != "NOT_EVALUATED"
    {
        return Err(ErrorClass::MalformedAssessment);
    }
    let freshness_status = text(assessment, &["dimensions", "freshness", "status"])?;
    let verdict = text(assessment, &["derived_summary", "verdict"])?;

    // The shape ECE emits when the PROVIDER could not supply the evidence: nothing about the run was assessed. It is an error
    // class of its own, not a verdict about the checkpoint, and none of the run-level facts below exist in it.
    let provider_failure_shape = review == ReviewState::NotEvaluated
        && availability == EvidenceAvailability::Unreachable
        && ci == CiResult::UnverifiedRun
        && freshness_status == "NOT_EVALUATED"
        && verdict == "INSUFFICIENT_EVIDENCE"
        && exit_code == 3;
    if provider_failure_shape {
        return Ok(CiEvidenceReport {
            schema_version: REPORT_SCHEMA_VERSION,
            provenance: request.provenance(),
            outcome: Outcome::InsufficientEvidence,
            ci: CiResult::UnverifiedRun,
            availability: EvidenceAvailability::Unreachable,
            review: ReviewState::NotEvaluated,
            authorization: AuthorizationState::AuthorizationPending,
            freshness: Freshness::NotEvaluated,
            limitations: vec![Limitation::NoIndependentReview, Limitation::NoHumanAuthorization, Limitation::NotCurrentHeadAttestation],
            blocking: Vec::new(),
            error: Some(ErrorClass::ProviderFailure),
            provider_failure: None,
        });
    }
    // Otherwise the run was assessed: it is a past observation, unreviewed, with nothing reviewed or authorized.
    if freshness_status != "HISTORICAL_BASELINE" || review != ReviewState::Unreviewed {
        return Err(ErrorClass::MalformedAssessment);
    }

    let mut outcome = match (verdict, exit_code) {
        ("ASSESSED_LIMITATIONS", 1) => Outcome::AssessedLimitations,
        ("ASSESSED_FAIL", 2) => Outcome::AssessedFail,
        ("INSUFFICIENT_EVIDENCE", 3) => Outcome::InsufficientEvidence,
        _ => return Err(ErrorClass::MalformedAssessment),
    };
    let reported_exit = at(assessment, &["exit_code"]).and_then(Value::as_i64).ok_or(ErrorClass::MalformedAssessment)?;
    if reported_exit != i64::from(exit_code) {
        return Err(ErrorClass::MalformedAssessment);
    }
    // A limitations outcome means "no blocking reason and the CI run is verified". Anything else contradicts itself.
    let blocking_reasons =
        at(assessment, &["derived_summary", "blocking_reasons"]).and_then(Value::as_array).ok_or(ErrorClass::MalformedAssessment)?;
    if outcome == Outcome::AssessedLimitations && (ci != CiResult::VerifiedPass || !blocking_reasons.is_empty()) {
        return Err(ErrorClass::MalformedAssessment);
    }
    if outcome != Outcome::AssessedLimitations && blocking_reasons.is_empty() && !matches!(outcome, Outcome::InsufficientEvidence) {
        return Err(ErrorClass::MalformedAssessment);
    }

    let expected_jobs = count(assessment, &["dimensions", "ci_execution", "expected_jobs_count"])?;
    let observed_jobs = count(assessment, &["dimensions", "ci_execution", "observed_jobs_count"])?;
    if expected_jobs != REQUIRED_JOBS.len() as u64 {
        return Err(ErrorClass::MalformedAssessment);
    }
    let repo_id_matched = flag(assessment, &["claims_breakdown", "machine_assessed", "repo_id_matched"])?;
    let sha_verified = flag(assessment, &["claims_breakdown", "machine_assessed", "sha_verified"])?;
    let integrity_evaluated = text(assessment, &["dimensions", "evidence_integrity", "status"])? != "NOT_EVALUATED";

    // Why it is not a pass, as closed classes.
    let mut blocking = Vec::new();
    let mut push = |class: BlockingClass| {
        if !blocking.contains(&class) {
            blocking.push(class);
        }
    };
    if !repo_id_matched {
        push(BlockingClass::RepositoryIdentityMismatch);
    }
    if !sha_verified && outcome != Outcome::InsufficientEvidence {
        push(BlockingClass::CommitNotVerified);
    }
    match ci {
        CiResult::AttemptMismatch => push(BlockingClass::RunAttemptMismatch),
        CiResult::Failed => push(BlockingClass::RequiredJobsFailedOrMissing),
        CiResult::IncompleteSkipped => push(BlockingClass::RequiredJobsSkippedOrCancelled),
        CiResult::UnverifiedRun => push(BlockingClass::RunOrJobsNotCompleted),
        CiResult::VerifiedPass | CiResult::NotEvaluated => {}
    }
    if availability == EvidenceAvailability::Expired {
        push(BlockingClass::ArtifactExpired);
    }
    let mut unexplained = false;
    for reason in blocking_reasons {
        let reason = reason.as_str().ok_or(ErrorClass::MalformedAssessment)?;
        if reason.starts_with("Workflow definition path") || reason.starts_with("Policy target workflow") {
            push(BlockingClass::WorkflowMismatch);
        } else if reason.starts_with("Repository numeric ID mismatch")
            || reason.starts_with("Commit SHA mismatch")
            || reason.starts_with("Target commit ")
            || reason.starts_with("Workflow run head_sha")
            || reason.starts_with("Run attempt mismatch")
            || reason.starts_with("Invalid run attempt")
            || reason.starts_with("Required CI jobs ")
            || reason.starts_with("Ambiguous duplicate")
            || reason.starts_with("Workflow run ")
            || reason.starts_with("Artifact ")
        {
            // already represented by a structured class above, or by the prefix families below
            if reason.starts_with("Ambiguous duplicate") {
                push(BlockingClass::RequiredJobsFailedOrMissing);
            }
        } else {
            unexplained = true;
        }
    }
    if unexplained {
        push(BlockingClass::Unclassified);
    }
    // ECE ignores the policy's `allow_extra_jobs`; this module does not.
    if !policy.allow_extra_jobs && observed_jobs != expected_jobs && ci == CiResult::VerifiedPass {
        push(BlockingClass::UnexpectedJobSet);
        outcome = Outcome::AssessedFail;
    }
    if outcome != Outcome::AssessedLimitations && blocking.is_empty() && !blocking_reasons.is_empty() {
        blocking.push(BlockingClass::Unclassified);
    }

    let mut limitations = vec![Limitation::NoIndependentReview, Limitation::NoHumanAuthorization, Limitation::NotCurrentHeadAttestation];
    if !integrity_evaluated {
        limitations.push(Limitation::StandaloneRunAssessment);
    }
    limitations.push(Limitation::CommitSignatureNotVerified);
    limitations.push(Limitation::HostedRunnersOnly);

    Ok(CiEvidenceReport {
        schema_version: REPORT_SCHEMA_VERSION,
        provenance: request.provenance(),
        outcome,
        ci,
        availability,
        review,
        authorization: AuthorizationState::AuthorizationPending,
        freshness: Freshness::HistoricalBaseline,
        limitations,
        blocking,
        error: None,
        provider_failure: None,
    })
}
