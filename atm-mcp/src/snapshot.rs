//! S2-C0: a pure, fail-closed validator for ONE desktop-reported ECE checkpoint snapshot.
//!
//! It takes bytes, a request and a clock value and returns a verdict. It reads no file, no environment variable, no clock and no network,
//! it registers no tool and it has no wire output. `atm_checkpoint_summary_v1` is unchanged.
//!
//! ORDER OF DEFECTS. The checks run in the order of the `Reason` list. A snapshot with several defects gets the reason of the FIRST check
//! that fails, and later defects are not reported. Inside the typed parse, serde stops at its first shape error; every such error is
//! `ShapeInvalid`. A caller must not read the reason as a complete list of what is wrong.
//!
//! WHAT A `DesktopReported` VERDICT MEANS. Only that the bytes have the closed shape of one ECE report, that the report is consistent with
//! itself and with the pinned target, that it is about exactly the requested commit, run and attempt, and that it is fresh. It does NOT mean
//! that the writer was authenticated: a same-user local file can be forged, so the source label is always `DESKTOP_REPORTED_UNATTESTED`.
//! It is never a cryptographic source proof, an independent acceptance or a human authorization. The validator copies the report; it
//! never raises any value of it.
//!
//! The value sets and rules below follow `src-tauri/src/ece_ci.rs` (baseline 0fa04d8d): `not_assessed`, `provider_unavailable`,
//! the provider-failure branch of `project` and the assessed branch of `project`.

use crate::guard::{check_frame, Reject};
use crate::tools::{ECE_REVISION, MAX_RUN_ATTEMPT, MAX_SAFE_RUN_ID, POLICY_FILE_SHA256, POLICY_ID, REPOSITORY_FULL_NAME, REPOSITORY_ID, WORKFLOW_PATH};
use serde::Deserialize;
use serde_json::Value;

pub const SNAPSHOT_SCHEMA: &str = "atm.ece-checkpoint-snapshot.v1";
pub const SOURCE_LABEL: &str = "DESKTOP_REPORTED_UNATTESTED";
pub const REPORT_SCHEMA_VERSION: &str = "atm.ece-ci-assessment.v1";
pub const MAX_SNAPSHOT_BYTES: usize = 4096;
/// Deepest JSON nesting the snapshot may have. The real shape needs 3 levels; 4 is the contract limit.
pub const MAX_SNAPSHOT_DEPTH: usize = 4;
/// A snapshot older than this many seconds is stale. The boundary itself is still fresh.
pub const MAX_AGE_SECONDS: u64 = 3600;
pub const MAX_LIMITATIONS: usize = 6;
pub const MAX_BLOCKING: usize = 10;
const MIN_YEAR: i64 = 2026;
const MAX_YEAR: i64 = 2100;

const ROOT_KEYS: [&str; 4] = ["schema", "source", "recordedAt", "report"];
const REPORT_KEYS: [&str; 12] = [
    "schemaVersion",
    "provenance",
    "outcome",
    "ci",
    "availability",
    "review",
    "authorization",
    "freshness",
    "limitations",
    "blocking",
    "error",
    "providerFailure",
];
const PROVENANCE_KEYS: [&str; 9] =
    ["eceRevision", "policyId", "policyFileSha256", "repository", "repositoryId", "workflowPath", "commitSha", "runId", "runAttempt"];

// ---------------------------------------------------------------------------------------------------------------------
// Closed enums (the value sets of `ece_ci.rs`, copied by hand; a later checkpoint must prove they still match)
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Outcome {
    AssessedLimitations,
    AssessedFail,
    InsufficientEvidence,
    NotAssessed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Ci {
    VerifiedPass,
    Failed,
    IncompleteSkipped,
    AttemptMismatch,
    UnverifiedRun,
    NotEvaluated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Availability {
    NotProvided,
    Available,
    PartiallyAvailable,
    Expired,
    Unreachable,
    NotEvaluated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Review {
    Unreviewed,
    NotEvaluated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Authorization {
    AuthorizationPending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Freshness {
    HistoricalBaseline,
    NotEvaluated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Limitation {
    NoIndependentReview,
    NoHumanAuthorization,
    NotCurrentHeadAttestation,
    StandaloneRunAssessment,
    CommitSignatureNotVerified,
    HostedRunnersOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Blocking {
    RepositoryIdentityMismatch,
    CommitNotVerified,
    WorkflowMismatch,
    RunAttemptMismatch,
    RequiredJobsFailedOrMissing,
    RequiredJobsSkippedOrCancelled,
    RunOrJobsNotCompleted,
    ArtifactExpired,
    UnexpectedJobSet,
    Unclassified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorClass {
    PolicyIntegrity,
    SchemaUnavailable,
    OutputSchemaViolation,
    EvaluatorFailure,
    ProviderFailure,
    UnsupportedSchemaVersion,
    MalformedAssessment,
    BindingMismatch,
    AttemptNotLatest,
    ProviderIdentityInconsistent,
    ClientInitFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProviderFailureKind {
    RateLimited,
    NetworkUnavailable,
    Timeout,
    Forbidden,
    NotFound,
    HttpError,
    InvalidResponse,
    ResponseTooLarge,
    IncompleteResponse,
}

// ---------------------------------------------------------------------------------------------------------------------
// The input: one closed object. Unknown keys are refused by serde; missing keys are refused by the exact key-set check below.
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Provenance {
    pub ece_revision: String,
    pub policy_id: String,
    pub policy_file_sha256: String,
    pub repository: String,
    pub repository_id: u64,
    pub workflow_path: String,
    pub commit_sha: String,
    pub run_id: u64,
    pub run_attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Report {
    pub schema_version: String,
    pub provenance: Provenance,
    pub outcome: Outcome,
    pub ci: Ci,
    pub availability: Availability,
    pub review: Review,
    pub authorization: Authorization,
    pub freshness: Freshness,
    pub limitations: Vec<Limitation>,
    pub blocking: Vec<Blocking>,
    pub error: Option<ErrorClass>,
    pub provider_failure: Option<ProviderFailureKind>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Snapshot {
    schema: String,
    source: String,
    recorded_at: String,
    report: Report,
}

/// What the caller asks about. The fields are private: an instance exists only if `new` accepted its values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointRequest {
    commit_sha: String,
    run_id: u64,
    run_attempt: u32,
}

impl CheckpointRequest {
    /// The same ranges as `atm_checkpoint_summary_v1`: 40 lowercase hex characters, a safe positive run id, an attempt from 1 to 1000.
    pub fn new(commit_sha: &str, run_id: u64, run_attempt: u64) -> Option<Self> {
        let sha_ok = commit_sha.len() == 40 && commit_sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !sha_ok || !(1..=MAX_SAFE_RUN_ID).contains(&run_id) || !(1..=MAX_RUN_ATTEMPT).contains(&run_attempt) {
            return None;
        }
        Some(Self { commit_sha: commit_sha.to_owned(), run_id, run_attempt: u32::try_from(run_attempt).ok()? })
    }

    pub fn commit_sha(&self) -> &str {
        &self.commit_sha
    }

    pub fn run_id(&self) -> u64 {
        self.run_id
    }

    pub fn run_attempt(&self) -> u32 {
        self.run_attempt
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The output
// ---------------------------------------------------------------------------------------------------------------------

/// Why no report is returned. Closed. The order of the checks in `evaluate` is the order of this list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    NoSnapshot,
    TooLarge,
    NotJson,
    DuplicateKey,
    ShapeInvalid,
    SchemaUnsupported,
    SourceUnsupported,
    TargetMismatch,
    RequestMismatch,
    TimeInvalid,
    Future,
    Stale,
    Inconsistent,
}

impl Reason {
    pub fn as_str(&self) -> &'static str {
        match *self {
            Reason::NoSnapshot => "NO_SNAPSHOT",
            Reason::TooLarge => "SNAPSHOT_TOO_LARGE",
            Reason::NotJson => "SNAPSHOT_NOT_JSON",
            Reason::DuplicateKey => "SNAPSHOT_DUPLICATE_KEY",
            Reason::ShapeInvalid => "SNAPSHOT_SHAPE_INVALID",
            Reason::SchemaUnsupported => "SNAPSHOT_SCHEMA_UNSUPPORTED",
            Reason::SourceUnsupported => "SNAPSHOT_SOURCE_UNSUPPORTED",
            Reason::TargetMismatch => "SNAPSHOT_TARGET_MISMATCH",
            Reason::RequestMismatch => "SNAPSHOT_REQUEST_MISMATCH",
            Reason::TimeInvalid => "SNAPSHOT_TIME_INVALID",
            Reason::Future => "SNAPSHOT_FUTURE",
            Reason::Stale => "SNAPSHOT_STALE",
            Reason::Inconsistent => "SNAPSHOT_INCONSISTENT",
        }
    }
}

/// Mirrors `CiEvidenceReport::machine_ci_verified()` in `ece_ci.rs`: a pass only when the outcome is a limitations outcome, the CI
/// dimension passed, nothing blocks and there is no error. A policy failure whose `ci` is `VERIFIED_PASS` is `NOT_REPORTED_PASS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineCiSummary {
    ReportedPass,
    NotReportedPass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reported {
    pub age_seconds: u64,
    pub recorded_at: String,
    pub report: Report,
    pub machine_ci_summary: MachineCiSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    NotAssessed(Reason),
    DesktopReported(Box<Reported>),
}

// ---------------------------------------------------------------------------------------------------------------------
// The validator
// ---------------------------------------------------------------------------------------------------------------------

/// Pure and total: no panic path, no input other than its arguments. `now_unix` is the caller's clock in whole seconds.
pub fn evaluate_snapshot(snapshot: Option<&[u8]>, request: &CheckpointRequest, now_unix: i64) -> Verdict {
    match evaluate(snapshot, request, now_unix) {
        Ok(reported) => Verdict::DesktopReported(Box::new(reported)),
        Err(reason) => Verdict::NotAssessed(reason),
    }
}

fn evaluate(snapshot: Option<&[u8]>, request: &CheckpointRequest, now_unix: i64) -> Result<Reported, Reason> {
    let bytes = snapshot.ok_or(Reason::NoSnapshot)?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(Reason::TooLarge);
    }
    // Duplicate keys (after unescaping) are refused on the token stream, before any typed or `Value` parse can keep only one of them.
    match check_frame(bytes) {
        Ok(()) => {}
        Err(Reject::NotJson) => return Err(Reason::NotJson),
        Err(Reject::DuplicateKey) => return Err(Reason::DuplicateKey),
        Err(Reject::TooDeep) => return Err(Reason::ShapeInvalid),
    }
    // `check_frame` allows deeper nesting than this contract does.
    if nesting_depth(bytes) > MAX_SNAPSHOT_DEPTH {
        return Err(Reason::ShapeInvalid);
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| Reason::NotJson)?;
    // serde also reads an object such as {"VERIFIED_PASS":null} as a unit variant, so the JSON type of every enum field is checked first.
    let shape_ok = keys_are(&value, &ROOT_KEYS)
        && value.get("report").is_some_and(|r| {
            keys_are(r, &REPORT_KEYS) && r.get("provenance").is_some_and(|p| keys_are(p, &PROVENANCE_KEYS)) && enum_fields_are_strings(r)
        });
    if !shape_ok {
        return Err(Reason::ShapeInvalid);
    }
    let snap: Snapshot = serde_json::from_value(value).map_err(|_| Reason::ShapeInvalid)?;
    let report = snap.report;
    if report.limitations.len() > MAX_LIMITATIONS
        || report.blocking.len() > MAX_BLOCKING
        || has_duplicates(&report.limitations)
        || has_duplicates(&report.blocking)
    {
        return Err(Reason::ShapeInvalid);
    }
    if snap.schema != SNAPSHOT_SCHEMA || report.schema_version != REPORT_SCHEMA_VERSION {
        return Err(Reason::SchemaUnsupported);
    }
    if snap.source != SOURCE_LABEL {
        return Err(Reason::SourceUnsupported);
    }
    let p = &report.provenance;
    if p.ece_revision != ECE_REVISION
        || p.policy_id != POLICY_ID
        || p.policy_file_sha256 != POLICY_FILE_SHA256
        || p.repository != REPOSITORY_FULL_NAME
        || p.repository_id != REPOSITORY_ID
        || p.workflow_path != WORKFLOW_PATH
    {
        return Err(Reason::TargetMismatch);
    }
    if p.commit_sha != request.commit_sha() || p.run_id != request.run_id() || p.run_attempt != request.run_attempt() {
        return Err(Reason::RequestMismatch);
    }
    let recorded = parse_recorded_at(&snap.recorded_at).ok_or(Reason::TimeInvalid)?;
    let age = now_unix.checked_sub(recorded).ok_or(Reason::TimeInvalid)?;
    if age < 0 {
        return Err(Reason::Future);
    }
    let age_seconds = u64::try_from(age).map_err(|_| Reason::TimeInvalid)?;
    if age_seconds > MAX_AGE_SECONDS {
        return Err(Reason::Stale);
    }
    if !is_consistent(&report) {
        return Err(Reason::Inconsistent);
    }
    let machine_ci_summary = if report.outcome == Outcome::AssessedLimitations
        && report.ci == Ci::VerifiedPass
        && report.blocking.is_empty()
        && report.error.is_none()
    {
        MachineCiSummary::ReportedPass
    } else {
        MachineCiSummary::NotReportedPass
    };
    Ok(Reported { age_seconds, recorded_at: snap.recorded_at, report, machine_ci_summary })
}

/// The exact key set of an object. Keys are unique here because duplicates were refused earlier.
fn keys_are(value: &Value, expected: &[&str]) -> bool {
    match value.as_object() {
        Some(map) => map.len() == expected.len() && expected.iter().all(|k| map.contains_key(*k)),
        None => false,
    }
}

/// Enum fields are JSON strings only. The two nullable fields are a string or null. The two lists hold strings only.
fn enum_fields_are_strings(report: &Value) -> bool {
    let required = ["outcome", "ci", "availability", "review", "authorization", "freshness"];
    let nullable = ["error", "providerFailure"];
    let lists = ["limitations", "blocking"];
    required.iter().all(|k| report.get(*k).is_some_and(Value::is_string))
        && nullable.iter().all(|k| report.get(*k).is_some_and(|v| v.is_string() || v.is_null()))
        && lists.iter().all(|k| report.get(*k).and_then(Value::as_array).is_some_and(|items| items.iter().all(Value::is_string)))
}

fn has_duplicates<T: PartialEq>(items: &[T]) -> bool {
    items.iter().enumerate().any(|(i, x)| items.iter().skip(i + 1).any(|y| x == y))
}

/// The deepest nesting of `{` and `[` outside strings. The input is already known to be valid JSON.
fn nesting_depth(bytes: &[u8]) -> usize {
    let mut depth = 0usize;
    let mut deepest = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for &b in bytes {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == 0x5c {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth = depth.saturating_add(1);
                deepest = deepest.max(depth);
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

// ---------------------------------------------------------------------------------------------------------------------
// Time: `YYYY-MM-DDTHH:MM:SSZ`, UTC only, parsed by hand with checked arithmetic (no clock crate, no overflow)
// ---------------------------------------------------------------------------------------------------------------------

fn parse_recorded_at(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() != 20 {
        return None;
    }
    let separators = [(4usize, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':'), (19, b'Z')];
    if !separators.iter().all(|&(i, c)| b.get(i) == Some(&c)) {
        return None;
    }
    let number = |start: usize, len: usize| -> Option<i64> {
        let end = start.checked_add(len)?;
        let digits = b.get(start..end)?;
        digits.iter().try_fold(0i64, |acc, &d| {
            if d.is_ascii_digit() {
                acc.checked_mul(10)?.checked_add(i64::from(d - b'0'))
            } else {
                None
            }
        })
    };
    let year = number(0, 4)?;
    let month = number(5, 2)?;
    let day = number(8, 2)?;
    let hour = number(11, 2)?;
    let minute = number(14, 2)?;
    let second = number(17, 2)?;
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) || !(1..=12).contains(&month) || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => return None,
    };
    if !(1..=days_in_month).contains(&day) {
        return None;
    }
    days_from_civil(year, month, day)
        .checked_mul(86_400)?
        .checked_add(hour.checked_mul(3600)?)?
        .checked_add(minute.checked_mul(60)?)?
        .checked_add(second)
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm). The inputs are range-checked by the caller.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let year_of_era = y - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

// ---------------------------------------------------------------------------------------------------------------------
// Consistency: the four report shapes that `ece_ci.rs` can produce. Anything else is refused.
// ---------------------------------------------------------------------------------------------------------------------

const REQUIRED_LIMITATIONS: [Limitation; 3] =
    [Limitation::NoIndependentReview, Limitation::NoHumanAuthorization, Limitation::NotCurrentHeadAttestation];

fn is_consistent(r: &Report) -> bool {
    if !REQUIRED_LIMITATIONS.iter().all(|l| r.limitations.contains(l)) {
        return false;
    }
    match (r.outcome, r.error) {
        (Outcome::NotAssessed, Some(error)) => error != ErrorClass::ProviderFailure && is_not_assessed_shape(r),
        (Outcome::NotAssessed, None) => false,
        (Outcome::InsufficientEvidence, Some(ErrorClass::ProviderFailure)) => is_provider_failure_shape(r),
        (_, Some(_)) => false,
        (_, None) => is_assessed_shape(r),
    }
}

/// `not_assessed()` in `ece_ci.rs`.
fn is_not_assessed_shape(r: &Report) -> bool {
    r.ci == Ci::NotEvaluated
        && r.availability == Availability::NotEvaluated
        && r.review == Review::NotEvaluated
        && r.freshness == Freshness::NotEvaluated
        && r.limitations.as_slice() == REQUIRED_LIMITATIONS.as_slice()
        && r.blocking.is_empty()
        && r.provider_failure.is_none()
}

/// `provider_unavailable()` and the provider-failure branch of `project()`. The failure kind may be absent.
fn is_provider_failure_shape(r: &Report) -> bool {
    r.ci == Ci::UnverifiedRun
        && r.availability == Availability::Unreachable
        && r.review == Review::NotEvaluated
        && r.freshness == Freshness::NotEvaluated
        && r.limitations.as_slice() == REQUIRED_LIMITATIONS.as_slice()
        && r.blocking.is_empty()
}

/// The assessed branch of `project()`.
fn is_assessed_shape(r: &Report) -> bool {
    let five = [
        Limitation::NoIndependentReview,
        Limitation::NoHumanAuthorization,
        Limitation::NotCurrentHeadAttestation,
        Limitation::CommitSignatureNotVerified,
        Limitation::HostedRunnersOnly,
    ];
    let six = [
        Limitation::NoIndependentReview,
        Limitation::NoHumanAuthorization,
        Limitation::NotCurrentHeadAttestation,
        Limitation::StandaloneRunAssessment,
        Limitation::CommitSignatureNotVerified,
        Limitation::HostedRunnersOnly,
    ];
    let limitations_ok = r.limitations.as_slice() == five.as_slice() || r.limitations.as_slice() == six.as_slice();
    if r.provider_failure.is_some()
        || r.review != Review::Unreviewed
        || r.freshness != Freshness::HistoricalBaseline
        || r.ci == Ci::NotEvaluated
        || r.availability == Availability::NotEvaluated
        || !limitations_ok
    {
        return false;
    }
    // A limitations outcome needs a passing CI dimension and nothing that blocks.
    if r.outcome == Outcome::AssessedLimitations && (r.ci != Ci::VerifiedPass || !r.blocking.is_empty()) {
        return false;
    }
    // ece_ci.rs 794-797 adds this class only when the ci dimension passed, and then sets the outcome to failed.
    if r.blocking.contains(&Blocking::UnexpectedJobSet) && (r.ci != Ci::VerifiedPass || r.outcome != Outcome::AssessedFail) {
        return false;
    }
    // A failed outcome always names at least one blocking class.
    if r.outcome == Outcome::AssessedFail && r.blocking.is_empty() {
        return false;
    }
    let ci_class_present = match r.ci {
        Ci::Failed => r.blocking.contains(&Blocking::RequiredJobsFailedOrMissing),
        Ci::IncompleteSkipped => r.blocking.contains(&Blocking::RequiredJobsSkippedOrCancelled),
        Ci::AttemptMismatch => r.blocking.contains(&Blocking::RunAttemptMismatch),
        Ci::UnverifiedRun => r.blocking.contains(&Blocking::RunOrJobsNotCompleted),
        Ci::VerifiedPass | Ci::NotEvaluated => true,
    };
    let expired_class_present = r.availability != Availability::Expired || r.blocking.contains(&Blocking::ArtifactExpired);
    ci_class_present && expired_class_present
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CONTRACT: nesting is counted outside strings only; brackets inside a string (even after an escaped quote) do not count.
    #[test]
    fn nesting_depth_counts_containers_outside_strings_only() {
        assert_eq!(nesting_depth(b"{}"), 1);
        assert_eq!(nesting_depth(b"{\"a\":{\"b\":[1]}}"), 3);
        assert_eq!(nesting_depth(b"[[[[1]]]]"), 4);
        assert_eq!(nesting_depth(b"[[[[[1]]]]]"), 5);
        assert_eq!(nesting_depth(b"{\"a\":\"[[[[[[\"}"), 1);
        assert_eq!(nesting_depth(b"{\"a\":\"x\\\"[[[[[[\"}"), 1);
        assert_eq!(nesting_depth(b""), 0);
    }

    /// CONTRACT: the epoch anchors, the calendar and the range are exact.
    #[test]
    fn recorded_at_is_parsed_exactly() {
        assert_eq!(parse_recorded_at("2026-01-01T00:00:00Z"), Some(1_767_225_600));
        assert_eq!(parse_recorded_at("2026-11-22T12:00:00Z"), Some(1_795_348_800));
        assert!(parse_recorded_at("2028-02-29T00:00:00Z").is_some());
        assert!(parse_recorded_at("2100-12-31T23:59:59Z").is_some());
        for bad in [
            "2026-02-29T00:00:00Z",
            "2100-02-29T00:00:00Z",
            "2025-12-31T23:59:59Z",
            "2101-01-01T00:00:00Z",
            "2026-04-31T00:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-00-10T00:00:00Z",
            "2026-01-00T00:00:00Z",
            "2026-01-01T24:00:00Z",
            "2026-01-01T00:60:00Z",
            "2026-01-01T00:00:60Z",
            "2026-01-01T00:00:00",
            "2026-01-01T00:00:00+00:00",
            "2026-01-01T00:00:00.5Z",
            "2026-01-01 00:00:00Z",
            "2026-1-1T0:0:0Z",
            "+026-01-01T00:00:00Z",
            "",
        ] {
            assert_eq!(parse_recorded_at(bad), None, "{bad:?}");
        }
    }
}
