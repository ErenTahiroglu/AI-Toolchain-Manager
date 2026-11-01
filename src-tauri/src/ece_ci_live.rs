//! ATM-GOV-02 P2B: the user-initiated, read-only, live GitHub CI evidence command (backend half).
//!
//! WHAT THIS ADDS. One command, `assess_atm_checkpoint`, that a person triggers with an explicit button. It validates a commit SHA,
//! a run ID and an attempt number, builds ECE's ANONYMOUS GitHub client (`HttpGitHubClient::new_unauthenticated()`, the only
//! constructor named anywhere in ATM's call path) and hands it to `ece_ci::assess_ci_evidence_latest_attempt_only`. The fixed
//! target (repository, numeric ID, workflow, required jobs, policy) is never an input.
//!
//! WHAT THIS DOES NOT CLAIM.
//! * This build CONTAINS a network-capable TLS stack and ECE's HTTP client (the `github-http` feature also compiles ECE's legacy,
//!   token-reading constructors). The ATM call path does not use them; nothing here proves they are absent from the binary.
//! * The HTTP library may follow the operating system's or the environment's proxy configuration. The anonymous constructor
//!   enforces the request target and a redirect policy (scheme, host and port), NOT a physical network-egress guarantee.
//! * Unauthenticated GitHub API access is rate limited (60 requests per hour per IP address, per GitHub's documentation). A pass
//!   typically needs four requests (repository, commit, run and one page of jobs). More requests occur when the jobs list spans several
//!   pages (the accepted client allows up to ten) or when a same-origin redirect is followed; ATM enforces no absolute maximum.
//! * A result is a historical machine observation. It is not checkpoint acceptance, not a review and not an authorization.
//! * Dropping the awaiting future does not stop a blocking request; the single-flight token is released only when the work ends.

use crate::ece_ci::{assess_ci_evidence_latest_attempt_only, not_assessed, CiEvidenceReport, CiEvidenceRequest, ErrorClass, RequestError};
use crate::error::AppError;
use ece_lib::github::{GitHubClientError, GitHubProvider, HttpGitHubClient};
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// JavaScript numbers are exact only up to 2^53 - 1; a larger run ID cannot have come from the interface unchanged.
pub const MAX_SAFE_RUN_ID: u64 = 9_007_199_254_740_991;
/// Same bound as the pasted-checkpoint viewer (`LIMITS.maxAttempt`).
pub const MAX_RUN_ATTEMPT: u32 = 1000;

/// The whole input of the command. Unknown fields are rejected, so a repository, URL, token, policy or job list cannot be smuggled in.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssessAtmCheckpointRequest {
    pub commit_sha: String,
    pub run_id: u64,
    pub run_attempt: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    CommitSha,
    RunId,
    RunAttempt,
}

impl InputError {
    pub fn app_error(self) -> AppError {
        match self {
            InputError::CommitSha => AppError::new("CI_ASSESS_INVALID_COMMIT", "Tam, küçük harfli, 40 karakterlik bir commit SHA girin."),
            InputError::RunId => AppError::new("CI_ASSESS_INVALID_RUN_ID", "Çalıştırma kimliği pozitif ve güvenli bir tam sayı olmalı."),
            InputError::RunAttempt => AppError::new("CI_ASSESS_INVALID_ATTEMPT", "Deneme numarası 1 ile 1000 arasında olmalı."),
        }
    }
}

/// Validates BEFORE any client exists and before any network activity can start.
pub fn validate(request: &AssessAtmCheckpointRequest) -> Result<CiEvidenceRequest, InputError> {
    if request.run_id > MAX_SAFE_RUN_ID {
        return Err(InputError::RunId);
    }
    if request.run_attempt > MAX_RUN_ATTEMPT {
        return Err(InputError::RunAttempt);
    }
    CiEvidenceRequest::new(&request.commit_sha, request.run_id, request.run_attempt).map_err(|e| match e {
        RequestError::CommitShaNotFortyLowercaseHex => InputError::CommitSha,
        RequestError::RunIdZero => InputError::RunId,
        RequestError::RunAttemptZero => InputError::RunAttempt,
    })
}

/// At most one assessment at a time. The token is moved INTO the blocking work and released when that work ends, not when a caller
/// stops waiting.
pub struct InFlight(Arc<AtomicBool>);

impl InFlight {
    pub fn try_acquire(flag: &Arc<AtomicBool>) -> Option<InFlight> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).ok().map(|_| InFlight(Arc::clone(flag)))
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// The ONLY place in the ATM call path that names a GitHub client constructor, and the only one it may name: the anonymous one.
/// It reads no token variable, sends no `Authorization` header and takes no URL.
pub fn anonymous_provider() -> Result<Box<dyn GitHubProvider>, GitHubClientError> {
    HttpGitHubClient::new_unauthenticated().map(|client| Box::new(client) as Box<dyn GitHubProvider>)
}

/// Builds the provider with `make_provider` (production: [`anonymous_provider`]) and assesses. A client that cannot be built is a
/// closed error class, not a pass and not a CI failure.
pub fn assess_live<F>(request: &CiEvidenceRequest, make_provider: F) -> CiEvidenceReport
where
    F: FnOnce() -> Result<Box<dyn GitHubProvider>, GitHubClientError>,
{
    match make_provider() {
        Ok(provider) => assess_ci_evidence_latest_attempt_only(provider.as_ref(), request),
        Err(_) => not_assessed(request, ErrorClass::ClientInitFailure),
    }
}

/// Validation then assessment, as the command does it, with the provider factory injectable for tests.
#[cfg(test)]
pub fn assess_checked<F>(request: &AssessAtmCheckpointRequest, make_provider: F) -> Result<CiEvidenceReport, InputError>
where
    F: FnOnce() -> Result<Box<dyn GitHubProvider>, GitHubClientError>,
{
    let validated = validate(request)?;
    Ok(assess_live(&validated, make_provider))
}
