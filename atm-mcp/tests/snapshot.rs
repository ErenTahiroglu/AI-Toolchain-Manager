//! S2-C0 tests for the pure snapshot validator. Every fixture is a synthetic string built in memory: no file, no network, no clock.
//! The shapes follow `src-tauri/src/ece_ci.rs` at baseline 0fa04d8d. Fixtures marked CONSTRUCTED follow a source rule and are not a
//! recorded real output. Nothing here is an ECE verification and none of it states that a snapshot writer was authenticated.

use atm_mcp::snapshot::{
    evaluate_snapshot, Authorization, CheckpointRequest, Ci, MachineCiSummary, Outcome, Reason, Reported, Review, Verdict, MAX_SNAPSHOT_BYTES, SNAPSHOT_SCHEMA,
    SOURCE_LABEL,
};
use atm_mcp::tools::{self, ECE_REVISION, POLICY_FILE_SHA256, POLICY_ID, REPOSITORY_FULL_NAME, REPOSITORY_ID, WORKFLOW_PATH};
use serde_json::{json, Value};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const OTHER_SHA: &str = "fedcba9876543210fedcba9876543210fedcba98";
const RUN: u64 = 1_234_567_890;
const RECORDED: &str = "2026-11-22T12:00:00Z";
/// Independent anchor: 2026-01-01T00:00:00Z is 1_767_225_600 (2024-01-01 is 1_704_067_200, then 366 + 365 days); November 22 is day 325 of 2026.
const RECORDED_UNIX: i64 = 1_795_348_800;
const NOW: i64 = RECORDED_UNIX + 5;

const L3: [&str; 3] = ["NO_INDEPENDENT_REVIEW", "NO_HUMAN_AUTHORIZATION", "NOT_CURRENT_HEAD_ATTESTATION"];
const FIVE: [&str; 5] =
    ["NO_INDEPENDENT_REVIEW", "NO_HUMAN_AUTHORIZATION", "NOT_CURRENT_HEAD_ATTESTATION", "COMMIT_SIGNATURE_NOT_VERIFIED", "HOSTED_RUNNERS_ONLY"];
const SIX: [&str; 6] = [
    "NO_INDEPENDENT_REVIEW",
    "NO_HUMAN_AUTHORIZATION",
    "NOT_CURRENT_HEAD_ATTESTATION",
    "STANDALONE_RUN_ASSESSMENT",
    "COMMIT_SIGNATURE_NOT_VERIFIED",
    "HOSTED_RUNNERS_ONLY",
];

// ---- builders ---------------------------------------------------------------------------------------------------------------------

fn request() -> CheckpointRequest {
    CheckpointRequest::new(SHA, RUN, 1).expect("valid request")
}

fn provenance() -> Value {
    json!({
        "eceRevision": ECE_REVISION, "policyId": POLICY_ID, "policyFileSha256": POLICY_FILE_SHA256, "repository": REPOSITORY_FULL_NAME,
        "repositoryId": REPOSITORY_ID, "workflowPath": WORKFLOW_PATH, "commitSha": SHA, "runId": RUN, "runAttempt": 1
    })
}

/// `f` is [outcome, ci, availability, review, freshness].
fn rep(f: [&str; 5], limitations: &[&str], blocking: &[&str], error: Option<&str>, provider_failure: Option<&str>) -> Value {
    json!({
        "schemaVersion": "atm.ece-ci-assessment.v1", "provenance": provenance(), "outcome": f[0], "ci": f[1], "availability": f[2],
        "review": f[3], "authorization": "AUTHORIZATION_PENDING", "freshness": f[4], "limitations": limitations, "blocking": blocking,
        "error": error, "providerFailure": provider_failure
    })
}

fn snapshot(report: Value) -> Value {
    json!({"schema": SNAPSHOT_SCHEMA, "source": SOURCE_LABEL, "recordedAt": RECORDED, "report": report})
}

fn pass() -> Value {
    rep(["ASSESSED_LIMITATIONS", "VERIFIED_PASS", "NOT_PROVIDED", "UNREVIEWED", "HISTORICAL_BASELINE"], &SIX, &[], None, None)
}

fn base() -> Value {
    snapshot(pass())
}

fn not_assessed(error: &str) -> Value {
    rep(["NOT_ASSESSED", "NOT_EVALUATED", "NOT_EVALUATED", "NOT_EVALUATED", "NOT_EVALUATED"], &L3, &[], Some(error), None)
}

fn provider_failure(kind: Option<&str>) -> Value {
    rep(["INSUFFICIENT_EVIDENCE", "UNVERIFIED_RUN", "UNREACHABLE", "NOT_EVALUATED", "NOT_EVALUATED"], &L3, &[], Some("PROVIDER_FAILURE"), kind)
}

fn assessed(outcome: &str, ci: &str, availability: &str, limitations: &[&str], blocking: &[&str]) -> Value {
    rep([outcome, ci, availability, "UNREVIEWED", "HISTORICAL_BASELINE"], limitations, blocking, None, None)
}

fn bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).expect("serialize fixture")
}

fn eval(v: &Value) -> Verdict {
    evaluate_snapshot(Some(bytes(v).as_slice()), &request(), NOW)
}

fn eval_text(text: &str) -> Verdict {
    evaluate_snapshot(Some(text.as_bytes()), &request(), NOW)
}

fn reason(v: Verdict) -> Reason {
    match v {
        Verdict::NotAssessed(r) => r,
        other => panic!("expected NotAssessed, got {other:?}"),
    }
}

fn reported(v: Verdict) -> Reported {
    match v {
        Verdict::DesktopReported(r) => *r,
        other => panic!("expected DesktopReported, got {other:?}"),
    }
}

fn set(v: &mut Value, path: &[&str], new: Value) {
    let (last, parents) = path.split_last().expect("path");
    let mut cursor = v;
    for key in parents {
        cursor = cursor.get_mut(*key).expect("parent exists");
    }
    cursor.as_object_mut().expect("object").insert((*last).to_string(), new);
}

fn remove(v: &mut Value, path: &[&str]) {
    let (last, parents) = path.split_last().expect("path");
    let mut cursor = v;
    for key in parents {
        cursor = cursor.get_mut(*key).expect("parent exists");
    }
    cursor.as_object_mut().expect("object").remove(*last);
}

fn mutated(path: &[&str], new: Value) -> Value {
    let mut v = base();
    set(&mut v, path, new);
    v
}

fn with_report(report: Value) -> Value {
    snapshot(report)
}

#[track_caller]
fn expect_reason(v: &Value, want: Reason, what: &str) {
    assert_eq!(reason(eval(v)), want, "{what}");
}

// ---- valid snapshots are reported unchanged ---------------------------------------------------------------------------------------

struct Valid {
    name: &'static str,
    report: Value,
    outcome: Outcome,
    ci: Ci,
    summary: MachineCiSummary,
}

fn valid(name: &'static str, report: Value, outcome: Outcome, ci: Ci, summary: MachineCiSummary) -> Valid {
    Valid { name, report, outcome, ci, summary }
}

/// A passing run, a failed run, a policy failure, a provider failure and a not-assessed report are all VALID information. The typed
/// expectations compare whole enum values, never substrings: `UNVERIFIED_RUN` is a different value from `VERIFIED_PASS`.
/// `DesktopReported` says nothing about the writer.
fn valid_cases() -> Vec<Valid> {
    use MachineCiSummary::{NotReportedPass as No, ReportedPass as Yes};
    use Outcome::{AssessedFail as Fail, AssessedLimitations as Lim, InsufficientEvidence as Insuff, NotAssessed as Nax};
    let mut cases = vec![
        valid("V1 healthy pass, six limitations", pass(), Lim, Ci::VerifiedPass, Yes),
        valid("V1b pass with unreachable artifact metadata", assessed("ASSESSED_LIMITATIONS", "VERIFIED_PASS", "UNREACHABLE", &SIX, &[]), Lim, Ci::VerifiedPass, Yes),
        valid(
            "V1c pass, availability available, five limitations (CONSTRUCTED)",
            assessed("ASSESSED_LIMITATIONS", "VERIFIED_PASS", "AVAILABLE", &FIVE, &[]),
            Lim,
            Ci::VerifiedPass,
            Yes,
        ),
        valid(
            "V2 extra job: ci VERIFIED_PASS but outcome ASSESSED_FAIL",
            assessed("ASSESSED_FAIL", "VERIFIED_PASS", "NOT_PROVIDED", &SIX, &["UNEXPECTED_JOB_SET"]),
            Fail,
            Ci::VerifiedPass,
            No,
        ),
        valid(
            "V2a extra job together with another class",
            assessed("ASSESSED_FAIL", "VERIFIED_PASS", "NOT_PROVIDED", &SIX, &["COMMIT_NOT_VERIFIED", "UNEXPECTED_JOB_SET"]),
            Fail,
            Ci::VerifiedPass,
            No,
        ),
        valid(
            "V2b wrong commit: ci VERIFIED_PASS but outcome ASSESSED_FAIL",
            assessed("ASSESSED_FAIL", "VERIFIED_PASS", "NOT_PROVIDED", &SIX, &["COMMIT_NOT_VERIFIED"]),
            Fail,
            Ci::VerifiedPass,
            No,
        ),
        valid("V2c wrong workflow", assessed("ASSESSED_FAIL", "VERIFIED_PASS", "NOT_PROVIDED", &SIX, &["WORKFLOW_MISMATCH"]), Fail, Ci::VerifiedPass, No),
        valid("V3 failed jobs", assessed("ASSESSED_FAIL", "FAILED", "NOT_PROVIDED", &SIX, &["REQUIRED_JOBS_FAILED_OR_MISSING"]), Fail, Ci::Failed, No),
        valid(
            "V3b skipped job",
            assessed("ASSESSED_FAIL", "INCOMPLETE_SKIPPED", "NOT_PROVIDED", &SIX, &["REQUIRED_JOBS_SKIPPED_OR_CANCELLED"]),
            Fail,
            Ci::IncompleteSkipped,
            No,
        ),
        valid(
            "V3c unfinished run",
            assessed("ASSESSED_FAIL", "UNVERIFIED_RUN", "NOT_PROVIDED", &SIX, &["RUN_OR_JOBS_NOT_COMPLETED"]),
            Fail,
            Ci::UnverifiedRun,
            No,
        ),
        valid(
            "V3d attempt mismatch",
            assessed("ASSESSED_FAIL", "ATTEMPT_MISMATCH", "NOT_PROVIDED", &SIX, &["RUN_ATTEMPT_MISMATCH"]),
            Fail,
            Ci::AttemptMismatch,
            No,
        ),
        valid("V3e expired artifact", assessed("ASSESSED_FAIL", "VERIFIED_PASS", "EXPIRED", &FIVE, &["ARTIFACT_EXPIRED"]), Fail, Ci::VerifiedPass, No),
        valid(
            "V3f two classes",
            assessed("ASSESSED_FAIL", "FAILED", "NOT_PROVIDED", &SIX, &["REQUIRED_JOBS_FAILED_OR_MISSING", "COMMIT_NOT_VERIFIED"]),
            Fail,
            Ci::Failed,
            No,
        ),
        valid("V4 provider failure with a kind", provider_failure(Some("RATE_LIMITED")), Insuff, Ci::UnverifiedRun, No),
        valid("V4b provider failure without a kind", provider_failure(None), Insuff, Ci::UnverifiedRun, No),
        valid(
            "V6 insufficient evidence in the assessed shape (CONSTRUCTED)",
            assessed("INSUFFICIENT_EVIDENCE", "UNVERIFIED_RUN", "NOT_PROVIDED", &SIX, &["RUN_OR_JOBS_NOT_COMPLETED"]),
            Insuff,
            Ci::UnverifiedRun,
            No,
        ),
        valid(
            "V6b insufficient evidence, empty blocking, ci VERIFIED_PASS: no source rule forbids it (CONSTRUCTED)",
            assessed("INSUFFICIENT_EVIDENCE", "VERIFIED_PASS", "NOT_PROVIDED", &SIX, &[]),
            Insuff,
            Ci::VerifiedPass,
            No,
        ),
    ];
    for error in [
        "POLICY_INTEGRITY",
        "SCHEMA_UNAVAILABLE",
        "OUTPUT_SCHEMA_VIOLATION",
        "EVALUATOR_FAILURE",
        "UNSUPPORTED_SCHEMA_VERSION",
        "MALFORMED_ASSESSMENT",
        "BINDING_MISMATCH",
        "ATTEMPT_NOT_LATEST",
        "PROVIDER_IDENTITY_INCONSISTENT",
        "CLIENT_INIT_FAILURE",
    ] {
        cases.push(valid("V5 not assessed", not_assessed(error), Nax, Ci::NotEvaluated, No));
    }
    for kind in [
        "RATE_LIMITED",
        "NETWORK_UNAVAILABLE",
        "TIMEOUT",
        "FORBIDDEN",
        "NOT_FOUND",
        "HTTP_ERROR",
        "INVALID_RESPONSE",
        "RESPONSE_TOO_LARGE",
        "INCOMPLETE_RESPONSE",
    ] {
        cases.push(valid("V4 every provider failure kind", provider_failure(Some(kind)), Insuff, Ci::UnverifiedRun, No));
    }
    cases
}

#[test]
fn every_valid_report_shape_is_returned_unchanged_and_never_raised() {
    for case in valid_cases() {
        let name = case.name;
        let r = reported(eval(&with_report(case.report.clone())));
        assert_eq!(r.age_seconds, 5, "{name}");
        assert_eq!(r.recorded_at, RECORDED, "{name}");
        assert_eq!(r.machine_ci_summary, case.summary, "{name}");
        // Whole typed values, so the copy cannot be a substring match: UNVERIFIED_RUN is not VERIFIED_PASS.
        assert_eq!(r.report.outcome, case.outcome, "{name}");
        assert_eq!(r.report.ci, case.ci, "{name}");
        assert_eq!(r.report.authorization, Authorization::AuthorizationPending, "{name}");
        let want_review = if case.report["review"] == "UNREVIEWED" { Review::Unreviewed } else { Review::NotEvaluated };
        assert_eq!(r.report.review, want_review, "{name}");
        assert_eq!(r.report.limitations.len(), case.report["limitations"].as_array().map_or(usize::MAX, Vec::len), "{name}");
        assert_eq!(r.report.blocking.len(), case.report["blocking"].as_array().map_or(usize::MAX, Vec::len), "{name}");
        assert_eq!(r.report.error.is_some(), !case.report["error"].is_null(), "{name}");
        assert_eq!(r.report.provider_failure.is_some(), !case.report["providerFailure"].is_null(), "{name}");
        assert_eq!(r.report.provenance.commit_sha, SHA, "{name}");
        assert_eq!((r.report.provenance.run_id, r.report.provenance.run_attempt), (RUN, 1), "{name}");
    }
}

#[test]
fn a_reported_verified_pass_dimension_is_not_a_verified_result() {
    // ECE's `VERIFIED_PASS` is one reported dimension. A policy failure keeps it and is still not a pass.
    let policy_failure = reported(eval(&with_report(assessed("ASSESSED_FAIL", "VERIFIED_PASS", "NOT_PROVIDED", &SIX, &["UNEXPECTED_JOB_SET"]))));
    assert_eq!(policy_failure.report.ci, Ci::VerifiedPass);
    assert_eq!(policy_failure.report.outcome, Outcome::AssessedFail);
    assert_eq!(policy_failure.machine_ci_summary, MachineCiSummary::NotReportedPass);
    // `UNVERIFIED_RUN` is its own value and never reads as `VERIFIED_PASS`.
    let unfinished = reported(eval(&with_report(assessed("ASSESSED_FAIL", "UNVERIFIED_RUN", "NOT_PROVIDED", &SIX, &["RUN_OR_JOBS_NOT_COMPLETED"]))));
    assert_eq!(unfinished.report.ci, Ci::UnverifiedRun);
    assert_ne!(unfinished.report.ci, Ci::VerifiedPass);
    let passes = valid_cases().into_iter().filter(|c| c.summary == MachineCiSummary::ReportedPass).count();
    assert_eq!(passes, 3, "V1, V1b and V1c only");
    for case in valid_cases() {
        if case.summary == MachineCiSummary::ReportedPass {
            assert_eq!((case.outcome, case.ci), (Outcome::AssessedLimitations, Ci::VerifiedPass), "{}", case.name);
        }
    }
}

// ---- absent, oversized, malformed -------------------------------------------------------------------------------------------------

#[test]
fn no_snapshot_and_oversized_input_are_refused_first() {
    assert_eq!(reason(evaluate_snapshot(None, &request(), NOW)), Reason::NoSnapshot);
    let big = vec![b' '; MAX_SNAPSHOT_BYTES + 1];
    assert_eq!(reason(evaluate_snapshot(Some(big.as_slice()), &request(), NOW)), Reason::TooLarge);
    // The cap is checked before any parse: a huge garbage input is "too large", not "not JSON".
    let huge = vec![b'x'; 1_000_000];
    assert_eq!(reason(evaluate_snapshot(Some(huge.as_slice()), &request(), NOW)), Reason::TooLarge);
    assert!(bytes(&base()).len() < MAX_SNAPSHOT_BYTES / 2, "a real snapshot is far below the cap");
}

#[test]
fn malformed_json_is_refused() {
    assert_eq!(reason(evaluate_snapshot(Some(&b"\xff\xfe"[..]), &request(), NOW)), Reason::NotJson);
    let trailing = format!("{} {{}}", base());
    assert_eq!(reason(eval_text(&trailing)), Reason::NotJson);
    let truncated = base().to_string();
    assert_eq!(reason(eval_text(truncated.get(..truncated.len() - 1).expect("prefix"))), Reason::NotJson);
    for text in ["", " ", "{", "}", "garbage"] {
        assert_eq!(reason(eval_text(text)), Reason::NotJson, "{text:?}");
    }
    for text in ["[1]", "null", "123", "true", "\"x\"", "[]", "{}"] {
        let r = reason(eval_text(text));
        assert!(matches!(r, Reason::NotJson | Reason::ShapeInvalid), "{text:?} gave {r:?}");
    }
}

#[test]
fn json_nested_deeper_than_the_contract_is_refused() {
    let mut v = base();
    set(&mut v, &["report", "limitations"], json!([[[[[]]]]]));
    expect_reason(&v, Reason::ShapeInvalid, "five levels");
    let mut w = base();
    set(&mut w, &["report", "limitations"], json!([[]]));
    expect_reason(&w, Reason::ShapeInvalid, "a nested list is not a limitation");
}

#[test]
fn duplicate_keys_are_refused_before_any_parse_keeps_only_one() {
    let text = base().to_string();
    let top = text.replacen("\"source\":\"DESKTOP_REPORTED_UNATTESTED\"", "\"source\":\"DESKTOP_REPORTED_UNATTESTED\",\"source\":\"DESKTOP_REPORTED_UNATTESTED\"", 1);
    assert_ne!(top, text, "the replacement must apply");
    assert_eq!(reason(eval_text(&top)), Reason::DuplicateKey, "top level, identical values");
    let inner = text.replacen("\"authorization\":\"AUTHORIZATION_PENDING\"", "\"authorization\":\"AUTHORIZATION_PENDING\",\"authorization\":\"AUTHORIZATION_PENDING\"", 1);
    assert_ne!(inner, text);
    assert_eq!(reason(eval_text(&inner)), Reason::DuplicateKey, "report level");
    let prov = text.replacen("\"runAttempt\":1", "\"runAttempt\":1,\"runAttempt\":1", 1);
    assert_ne!(prov, text);
    assert_eq!(reason(eval_text(&prov)), Reason::DuplicateKey, "provenance level");
    // The same key spelled with an escape (built at run time) is still a duplicate after unescaping.
    let bs = char::from(92u8);
    let escaped = text.replacen('{', &format!("{{\"s{bs}u006furce\":\"X\","), 1);
    assert_ne!(escaped, text);
    assert_eq!(reason(eval_text(&escaped)), Reason::DuplicateKey, "escaped spelling");
}

#[test]
fn unknown_and_missing_keys_are_refused_at_every_level() {
    for path in [&["extra"][..], &["report", "extra"], &["report", "provenance", "extra"]] {
        expect_reason(&mutated(path, json!(1)), Reason::ShapeInvalid, "unknown key");
    }
    for path in [
        &["schema"][..],
        &["source"],
        &["recordedAt"],
        &["report"],
        &["report", "schemaVersion"],
        &["report", "provenance"],
        &["report", "outcome"],
        &["report", "error"],
        &["report", "providerFailure"],
        &["report", "limitations"],
        &["report", "provenance", "runId"],
        &["report", "provenance", "commitSha"],
    ] {
        let mut v = base();
        remove(&mut v, path);
        expect_reason(&v, Reason::ShapeInvalid, &format!("missing {path:?}"));
    }
}

#[test]
fn wrong_types_unknown_enum_values_and_list_limits_are_refused() {
    let cases: [(&[&str], Value); 19] = [
        (&["report", "provenance", "runId"], json!("1234567890")),
        (&["report", "provenance", "runId"], json!(1.5)),
        (&["report", "provenance", "runAttempt"], json!(4_294_967_296_u64)),
        (&["report", "provenance", "repositoryId"], json!("1413039335")),
        (&["report", "outcome"], json!("assessed_limitations")),
        (&["report", "outcome"], json!("ASSESSED_PROVISIONAL_PASS")),
        (&["report", "ci"], json!("SUPER_PASS")),
        (&["report", "availability"], json!("PERFECT")),
        (&["report", "review"], json!("INDEPENDENT_REVIEW_PASS")),
        (&["report", "authorization"], json!("EXPLICITLY_AUTHORIZED")),
        (&["report", "freshness"], json!("CURRENT_HEAD")),
        (&["report", "error"], json!("TEAPOT")),
        (&["report", "providerFailure"], json!("TEAPOT")),
        (&["report", "limitations"], json!("NO_INDEPENDENT_REVIEW")),
        (&["report", "limitations"], json!(["NO_INDEPENDENT_REVIEW", "NO_INDEPENDENT_REVIEW"])),
        (&["report", "limitations"], json!(["ACCEPTED"])),
        (&["report", "blocking"], json!(["COMMIT_NOT_VERIFIED", "COMMIT_NOT_VERIFIED"])),
        (&["report", "blocking"], json!(["NOT_A_CLASS"])),
        (&["recordedAt"], json!(1_795_348_800_u64)),
    ];
    for (path, value) in cases {
        expect_reason(&mutated(path, value.clone()), Reason::ShapeInvalid, &format!("{path:?} = {value}"));
    }
    // More than six limitations or ten blocking classes can only be duplicates, which are refused too.
    let seven = ["NO_INDEPENDENT_REVIEW"; 7];
    expect_reason(&mutated(&["report", "limitations"], json!(seven)), Reason::ShapeInvalid, "seven limitations");
    let eleven = ["UNCLASSIFIED"; 11];
    expect_reason(&mutated(&["report", "blocking"], json!(eleven)), Reason::ShapeInvalid, "eleven blocking classes");
}

// ---- identity, request, time ------------------------------------------------------------------------------------------------------

#[test]
fn another_schema_source_or_target_is_refused() {
    expect_reason(&mutated(&["schema"], json!("atm.ece-checkpoint-snapshot.v2")), Reason::SchemaUnsupported, "snapshot schema");
    expect_reason(&mutated(&["report", "schemaVersion"], json!("atm.ece-ci-assessment.v2")), Reason::SchemaUnsupported, "report schema");
    for source in ["DESKTOP_ATTESTED", "ECE_VERIFIED", "desktop_reported_unattested", ""] {
        expect_reason(&mutated(&["source"], json!(source)), Reason::SourceUnsupported, source);
    }
    let zero40 = "0".repeat(40);
    let zero64 = "0".repeat(64);
    let cases: [(&str, Value); 6] = [
        ("eceRevision", json!(zero40)),
        ("policyId", json!("atm-ci-other-v1")),
        ("policyFileSha256", json!(zero64)),
        ("repository", json!("ErenTahiroglu/Evidence-Checkpoint-Engine")),
        ("repositoryId", json!(REPOSITORY_ID + 1)),
        ("workflowPath", json!(".github/workflows/mcp60-s1-windows-once.yml")),
    ];
    for (key, value) in cases {
        expect_reason(&mutated(&["report", "provenance", key], value), Reason::TargetMismatch, key);
    }
}

#[test]
fn a_snapshot_for_another_commit_run_or_attempt_is_refused() {
    expect_reason(&mutated(&["report", "provenance", "commitSha"], json!(OTHER_SHA)), Reason::RequestMismatch, "commit");
    expect_reason(&mutated(&["report", "provenance", "commitSha"], json!(SHA.to_uppercase())), Reason::RequestMismatch, "uppercase commit");
    expect_reason(&mutated(&["report", "provenance", "runId"], json!(RUN + 1)), Reason::RequestMismatch, "run");
    expect_reason(&mutated(&["report", "provenance", "runAttempt"], json!(2)), Reason::RequestMismatch, "attempt");
    // The same snapshot is fine for the matching request and refused for another one.
    let other = CheckpointRequest::new(OTHER_SHA, RUN, 1).expect("valid request");
    assert_eq!(reason(evaluate_snapshot(Some(bytes(&base()).as_slice()), &other, NOW)), Reason::RequestMismatch);
}

#[test]
fn the_request_is_validated_like_the_v1_tool() {
    assert!(CheckpointRequest::new(SHA, 1, 1).is_some());
    assert!(CheckpointRequest::new(SHA, 9_007_199_254_740_991, 1000).is_some());
    for (sha, run, attempt) in [
        (OTHER_SHA.to_uppercase(), 1_u64, 1_u64),
        ("abc".to_string(), 1, 1),
        (format!("{SHA}0"), 1, 1),
        (SHA.to_string(), 0, 1),
        (SHA.to_string(), 9_007_199_254_740_992, 1),
        (SHA.to_string(), 1, 0),
        (SHA.to_string(), 1, 1001),
        (SHA.to_string(), 1, u64::MAX),
    ] {
        assert!(CheckpointRequest::new(&sha, run, attempt).is_none(), "{sha} {run} {attempt}");
    }
}

#[test]
fn a_recorded_time_must_be_exact_utc_and_inside_the_calendar() {
    for bad in [
        "2026-02-29T00:00:00Z",
        "2100-02-29T00:00:00Z",
        "2025-12-31T23:59:59Z",
        "2101-01-01T00:00:00Z",
        "2026-04-31T00:00:00Z",
        "2026-13-01T00:00:00Z",
        "2026-01-01T24:00:00Z",
        "2026-01-01T00:00:60Z",
        "2026-11-22T12:00:00",
        "2026-11-22T12:00:00+00:00",
        "2026-11-22T12:00:00.5Z",
        "2026-11-22 12:00:00Z",
        "2026-11-22t12:00:00z",
        "2026-1-22T12:00:00Z",
        "",
    ] {
        expect_reason(&mutated(&["recordedAt"], json!(bad)), Reason::TimeInvalid, bad);
    }
}

#[test]
fn freshness_is_sixty_minutes_from_the_recorded_time_and_fails_closed() {
    let snap = bytes(&base());
    let at = |now: i64| evaluate_snapshot(Some(snap.as_slice()), &request(), now);
    assert_eq!(reported(at(RECORDED_UNIX)).age_seconds, 0);
    assert_eq!(reported(at(RECORDED_UNIX + 3600)).age_seconds, 3600, "the boundary is still fresh");
    assert_eq!(reason(at(RECORDED_UNIX + 3601)), Reason::Stale);
    assert_eq!(reason(at(RECORDED_UNIX - 1)), Reason::Future);
    assert_eq!(reason(at(0)), Reason::Future);
    assert_eq!(reason(at(-1)), Reason::Future);
    // Extreme clocks: no overflow and no panic.
    assert_eq!(reason(at(i64::MIN)), Reason::TimeInvalid);
    assert_eq!(reason(at(i64::MAX)), Reason::Stale);
    // Independent anchor for the date arithmetic.
    let jan = bytes(&mutated(&["recordedAt"], json!("2026-01-01T00:00:00Z")));
    assert_eq!(reported(evaluate_snapshot(Some(jan.as_slice()), &request(), 1_767_225_600 + 10)).age_seconds, 10);
}

// ---- consistency ------------------------------------------------------------------------------------------------------------------

#[test]
fn every_inconsistent_combination_is_refused() {
    let lim = ["ASSESSED_LIMITATIONS", "VERIFIED_PASS", "NOT_PROVIDED", "UNREVIEWED", "HISTORICAL_BASELINE"];
    let fail = |ci: &'static str| ["ASSESSED_FAIL", ci, "NOT_PROVIDED", "UNREVIEWED", "HISTORICAL_BASELINE"];
    let nax = ["NOT_ASSESSED", "NOT_EVALUATED", "NOT_EVALUATED", "NOT_EVALUATED", "NOT_EVALUATED"];
    let prov = ["INSUFFICIENT_EVIDENCE", "UNVERIFIED_RUN", "UNREACHABLE", "NOT_EVALUATED", "NOT_EVALUATED"];
    let missing_one = ["NO_INDEPENDENT_REVIEW", "NOT_CURRENT_HEAD_ATTESTATION", "COMMIT_SIGNATURE_NOT_VERIFIED", "HOSTED_RUNNERS_ONLY"];
    let swapped = [
        "NO_HUMAN_AUTHORIZATION",
        "NO_INDEPENDENT_REVIEW",
        "NOT_CURRENT_HEAD_ATTESTATION",
        "STANDALONE_RUN_ASSESSMENT",
        "COMMIT_SIGNATURE_NOT_VERIFIED",
        "HOSTED_RUNNERS_ONLY",
    ];
    let cases = [
        ("a required limitation is missing", rep(lim, &missing_one, &[], None, None)),
        ("limitations in another order", rep(lim, &swapped, &[], None, None)),
        ("limitations only the required three in an assessed report", rep(lim, &L3, &[], None, None)),
        ("limitations outcome with a blocking class", rep(lim, &SIX, &["REPOSITORY_IDENTITY_MISMATCH"], None, None)),
        ("limitations outcome with a failed ci", rep(["ASSESSED_LIMITATIONS", "FAILED", "NOT_PROVIDED", "UNREVIEWED", "HISTORICAL_BASELINE"], &SIX, &["REQUIRED_JOBS_FAILED_OR_MISSING"], None, None)),
        ("failed outcome with no blocking class", rep(fail("VERIFIED_PASS"), &SIX, &[], None, None)),
        ("ci FAILED without its class", rep(fail("FAILED"), &SIX, &["COMMIT_NOT_VERIFIED"], None, None)),
        ("ci INCOMPLETE_SKIPPED without its class", rep(fail("INCOMPLETE_SKIPPED"), &SIX, &["COMMIT_NOT_VERIFIED"], None, None)),
        ("ci ATTEMPT_MISMATCH without its class", rep(fail("ATTEMPT_MISMATCH"), &SIX, &["COMMIT_NOT_VERIFIED"], None, None)),
        ("ci UNVERIFIED_RUN without its class", rep(fail("UNVERIFIED_RUN"), &SIX, &["COMMIT_NOT_VERIFIED"], None, None)),
        (
            "expired availability without its class",
            rep(["ASSESSED_FAIL", "VERIFIED_PASS", "EXPIRED", "UNREVIEWED", "HISTORICAL_BASELINE"], &SIX, &["COMMIT_NOT_VERIFIED"], None, None),
        ),
        ("assessed report with review not evaluated", rep(["ASSESSED_FAIL", "VERIFIED_PASS", "NOT_PROVIDED", "NOT_EVALUATED", "HISTORICAL_BASELINE"], &SIX, &["COMMIT_NOT_VERIFIED"], None, None)),
        ("assessed report with freshness not evaluated", rep(["ASSESSED_FAIL", "VERIFIED_PASS", "NOT_PROVIDED", "UNREVIEWED", "NOT_EVALUATED"], &SIX, &["COMMIT_NOT_VERIFIED"], None, None)),
        ("assessed report with ci not evaluated", rep(["ASSESSED_FAIL", "NOT_EVALUATED", "NOT_PROVIDED", "UNREVIEWED", "HISTORICAL_BASELINE"], &SIX, &["COMMIT_NOT_VERIFIED"], None, None)),
        ("assessed report with availability not evaluated", rep(["ASSESSED_FAIL", "VERIFIED_PASS", "NOT_EVALUATED", "UNREVIEWED", "HISTORICAL_BASELINE"], &SIX, &["COMMIT_NOT_VERIFIED"], None, None)),
        ("assessed report with a provider failure kind and no error", rep(lim, &SIX, &[], None, Some("TIMEOUT"))),
        ("limitations outcome with an error", rep(lim, &SIX, &[], Some("MALFORMED_ASSESSMENT"), None)),
        ("insufficient evidence with a non-provider error", rep(["INSUFFICIENT_EVIDENCE", "UNVERIFIED_RUN", "UNREACHABLE", "NOT_EVALUATED", "NOT_EVALUATED"], &L3, &[], Some("BINDING_MISMATCH"), None)),
        ("not assessed without an error", rep(nax, &L3, &[], None, None)),
        ("not assessed with the provider-failure error", rep(nax, &L3, &[], Some("PROVIDER_FAILURE"), None)),
        ("not assessed with a blocking class", rep(nax, &L3, &["COMMIT_NOT_VERIFIED"], Some("EVALUATOR_FAILURE"), None)),
        ("not assessed with a failure kind", rep(nax, &L3, &[], Some("EVALUATOR_FAILURE"), Some("TIMEOUT"))),
        ("not assessed with more limitations", rep(nax, &SIX, &[], Some("EVALUATOR_FAILURE"), None)),
        ("not assessed with a ci value", rep(["NOT_ASSESSED", "FAILED", "NOT_EVALUATED", "NOT_EVALUATED", "NOT_EVALUATED"], &L3, &[], Some("EVALUATOR_FAILURE"), None)),
        ("provider failure with the wrong ci", rep(["INSUFFICIENT_EVIDENCE", "FAILED", "UNREACHABLE", "NOT_EVALUATED", "NOT_EVALUATED"], &L3, &[], Some("PROVIDER_FAILURE"), None)),
        ("provider failure with the wrong availability", rep(["INSUFFICIENT_EVIDENCE", "UNVERIFIED_RUN", "AVAILABLE", "NOT_EVALUATED", "NOT_EVALUATED"], &L3, &[], Some("PROVIDER_FAILURE"), None)),
        ("provider failure with a blocking class", rep(prov, &L3, &["RUN_OR_JOBS_NOT_COMPLETED"], Some("PROVIDER_FAILURE"), None)),
        ("provider failure with more limitations", rep(prov, &SIX, &[], Some("PROVIDER_FAILURE"), None)),
        ("provider failure with the wrong outcome", rep(["ASSESSED_FAIL", "UNVERIFIED_RUN", "UNREACHABLE", "NOT_EVALUATED", "NOT_EVALUATED"], &L3, &[], Some("PROVIDER_FAILURE"), None)),
        ("failed ci with the extra-job class", rep(["ASSESSED_FAIL", "FAILED", "NOT_PROVIDED", "UNREVIEWED", "HISTORICAL_BASELINE"], &SIX, &["REQUIRED_JOBS_FAILED_OR_MISSING", "UNEXPECTED_JOB_SET"], None, None)),
        ("limitations outcome with the extra-job class", rep(lim, &SIX, &["UNEXPECTED_JOB_SET"], None, None)),
    ];
    for (what, report) in cases {
        expect_reason(&with_report(report), Reason::Inconsistent, what);
    }
}

#[test]
fn rules_that_ece_refutes_are_not_enforced() {
    // ci VERIFIED_PASS with a failed outcome (ece_ci_tests.rs 291-298) and unreachable metadata with a limitations outcome (507-515).
    assert!(matches!(eval(&with_report(assessed("ASSESSED_FAIL", "VERIFIED_PASS", "NOT_PROVIDED", &SIX, &["UNEXPECTED_JOB_SET"]))), Verdict::DesktopReported(_)));
    assert!(matches!(eval(&with_report(assessed("ASSESSED_LIMITATIONS", "VERIFIED_PASS", "UNREACHABLE", &SIX, &[]))), Verdict::DesktopReported(_)));
    // error PROVIDER_FAILURE with no failure kind (ece_ci.rs 687-708).
    assert!(matches!(eval(&with_report(provider_failure(None))), Verdict::DesktopReported(_)));
}

// ---- F2: enum fields are JSON strings only ---------------------------------------------------------------------------------------

#[test]
fn enum_fields_refuse_object_array_number_and_boolean_forms() {
    // serde would read {"VERIFIED_PASS":null} as a unit variant; the validator must not.
    let tagged = [
        ("outcome", json!({"ASSESSED_FAIL": null})),
        ("ci", json!({"VERIFIED_PASS": null})),
        ("availability", json!({"NOT_PROVIDED": null})),
        ("review", json!({"UNREVIEWED": null})),
        ("authorization", json!({"AUTHORIZATION_PENDING": null})),
        ("freshness", json!({"HISTORICAL_BASELINE": null})),
    ];
    for (key, value) in tagged {
        expect_reason(&mutated(&["report", key], value), Reason::ShapeInvalid, key);
    }
    for key in ["outcome", "ci", "availability", "review", "authorization", "freshness"] {
        for other in [json!(["X"]), json!(1), json!(true), json!(null), json!({})] {
            expect_reason(&mutated(&["report", key], other.clone()), Reason::ShapeInvalid, &format!("{key} = {other}"));
        }
    }
}

#[test]
fn enum_lists_hold_only_strings() {
    for key in ["limitations", "blocking"] {
        let first = if key == "limitations" { "NO_INDEPENDENT_REVIEW" } else { "COMMIT_NOT_VERIFIED" };
        let mut tagged = serde_json::Map::new();
        tagged.insert(first.to_string(), Value::Null);
        for item in [Value::Object(tagged), json!([first]), json!(1), json!(null), json!(false)] {
            expect_reason(&mutated(&["report", key], json!([item.clone()])), Reason::ShapeInvalid, &format!("{key} item {item}"));
        }
    }
    // A tagged item hidden among valid strings is refused too.
    let mut mixed = SIX.iter().map(|s| json!(s)).collect::<Vec<Value>>();
    mixed[0] = json!({"NO_INDEPENDENT_REVIEW": null});
    expect_reason(&mutated(&["report", "limitations"], json!(mixed)), Reason::ShapeInvalid, "tagged item among strings");
}

#[test]
fn nullable_enum_fields_accept_a_string_or_null_and_nothing_else() {
    // The same fixtures are valid with a string or with null ...
    assert!(matches!(eval(&with_report(provider_failure(Some("RATE_LIMITED")))), Verdict::DesktopReported(_)));
    assert!(matches!(eval(&with_report(provider_failure(None))), Verdict::DesktopReported(_)));
    assert!(matches!(eval(&base()), Verdict::DesktopReported(_)), "error and providerFailure null");
    // ... and refused in every other JSON form, in a failure-kind fixture and in a plain one.
    for key in ["error", "providerFailure"] {
        for other in [json!({"PROVIDER_FAILURE": null}), json!(["PROVIDER_FAILURE"]), json!(1), json!(true), json!({}), json!("")] {
            let mut failing = with_report(provider_failure(Some("RATE_LIMITED")));
            set(&mut failing, &["report", key], other.clone());
            expect_reason(&failing, Reason::ShapeInvalid, &format!("failure fixture {key} = {other}"));
            expect_reason(&mutated(&["report", key], other.clone()), Reason::ShapeInvalid, &format!("plain fixture {key} = {other}"));
        }
    }
}

// ---- F3: a request exists only through its validating constructor ------------------------------------------------------------------

#[test]
fn a_request_exists_only_through_its_validating_constructor() {
    // The fields are private, so a struct literal cannot be written outside the module; the only way in is `new`.
    let ok = CheckpointRequest::new(SHA, RUN, 7).expect("valid request");
    assert_eq!((ok.commit_sha(), ok.run_id(), ok.run_attempt()), (SHA, RUN, 7));
    // Every invalid value gives no request, so no verdict can ever be computed for it.
    for (sha, run, attempt) in [
        ("", RUN, 1_u64),
        ("0123456789ABCDEF0123456789abcdef01234567", RUN, 1),
        ("0123456789abcdef0123456789abcdef0123456", RUN, 1),
        ("0123456789abcdef0123456789abcdef012345678", RUN, 1),
        ("g123456789abcdef0123456789abcdef01234567", RUN, 1),
        (SHA, 0, 1),
        (SHA, 9_007_199_254_740_992, 1),
        (SHA, RUN, 0),
        (SHA, RUN, 1001),
        (SHA, RUN, 4_294_967_297),
    ] {
        assert!(CheckpointRequest::new(sha, run, attempt).is_none(), "{sha:?} {run} {attempt}");
    }
}

// ---- F4: UNEXPECTED_JOB_SET needs a passing ci dimension and a failed outcome --------------------------------------------------------

#[test]
fn unexpected_job_set_needs_verified_pass_and_a_failed_outcome() {
    let bad = [
        ("FAILED with UNEXPECTED_JOB_SET", assessed("ASSESSED_FAIL", "FAILED", "NOT_PROVIDED", &SIX, &["REQUIRED_JOBS_FAILED_OR_MISSING", "UNEXPECTED_JOB_SET"])),
        ("UNVERIFIED_RUN with UNEXPECTED_JOB_SET", assessed("ASSESSED_FAIL", "UNVERIFIED_RUN", "NOT_PROVIDED", &SIX, &["RUN_OR_JOBS_NOT_COMPLETED", "UNEXPECTED_JOB_SET"])),
        ("limitations outcome with UNEXPECTED_JOB_SET", assessed("ASSESSED_LIMITATIONS", "VERIFIED_PASS", "NOT_PROVIDED", &SIX, &["UNEXPECTED_JOB_SET"])),
        ("insufficient evidence with UNEXPECTED_JOB_SET", assessed("INSUFFICIENT_EVIDENCE", "VERIFIED_PASS", "NOT_PROVIDED", &SIX, &["UNEXPECTED_JOB_SET"])),
    ];
    for (what, report) in bad {
        expect_reason(&with_report(report), Reason::Inconsistent, what);
    }
    // The real combination stays valid (ece_ci_tests.rs 291-298).
    let good = reported(eval(&with_report(assessed("ASSESSED_FAIL", "VERIFIED_PASS", "NOT_PROVIDED", &SIX, &["UNEXPECTED_JOB_SET"]))));
    assert_eq!((good.report.outcome, good.report.ci), (Outcome::AssessedFail, Ci::VerifiedPass));
}

// ---- P3: the first failing check wins; the size boundary is exact -----------------------------------------------------------------------

#[test]
fn several_defects_give_the_reason_of_the_first_failing_check() {
    // Defects, in check order: schema, source, target, request, time, consistency. Fixing one at a time reveals the next.
    let build = |defects: [bool; 6]| -> (Value, i64) {
        let mut v = base();
        if defects[0] {
            set(&mut v, &["schema"], json!("atm.ece-checkpoint-snapshot.v2"));
        }
        if defects[1] {
            set(&mut v, &["source"], json!("DESKTOP_ATTESTED"));
        }
        if defects[2] {
            set(&mut v, &["report", "provenance", "policyId"], json!("atm-ci-other-v1"));
        }
        if defects[3] {
            set(&mut v, &["report", "provenance", "commitSha"], json!(OTHER_SHA));
        }
        if defects[5] {
            set(&mut v, &["report", "blocking"], json!(["COMMIT_NOT_VERIFIED"]));
        }
        (v, if defects[4] { RECORDED_UNIX + 3601 } else { NOW })
    };
    let order = [Reason::SchemaUnsupported, Reason::SourceUnsupported, Reason::TargetMismatch, Reason::RequestMismatch, Reason::Stale, Reason::Inconsistent];
    for (first, want) in order.iter().enumerate() {
        let defects: [bool; 6] = std::array::from_fn(|i| i >= first);
        let (v, now) = build(defects);
        let got = reason(evaluate_snapshot(Some(bytes(&v).as_slice()), &request(), now));
        assert_eq!(got, *want, "defects from check {first} on");
    }
    let (clean, now) = build([false; 6]);
    assert!(matches!(evaluate_snapshot(Some(bytes(&clean).as_slice()), &request(), now), Verdict::DesktopReported(_)));
    // Two shape defects (an unknown key and a wrong type) give the one reason for the shape stage, whichever serde meets first.
    let mut shape = build([true; 6]).0;
    set(&mut shape, &["extra"], json!(1));
    set(&mut shape, &["report", "outcome"], json!(5));
    assert_eq!(reason(eval(&shape)), Reason::ShapeInvalid);
    // A malformed byte stream beats every later defect.
    let text = format!("{} x", build([true; 6]).0);
    assert_eq!(reason(eval_text(&text)), Reason::NotJson);
}

#[test]
fn input_of_exactly_4096_bytes_is_accepted_and_one_more_byte_is_refused() {
    let mut exact = bytes(&base());
    assert!(exact.len() < MAX_SNAPSHOT_BYTES);
    // Trailing JSON whitespace is allowed, so this is the same valid snapshot at the exact cap.
    exact.resize(MAX_SNAPSHOT_BYTES, b' ');
    assert_eq!(exact.len(), 4096);
    assert!(matches!(evaluate_snapshot(Some(exact.as_slice()), &request(), NOW), Verdict::DesktopReported(_)));
    exact.push(b' ');
    assert_eq!(exact.len(), 4097);
    assert_eq!(reason(evaluate_snapshot(Some(exact.as_slice()), &request(), NOW)), Reason::TooLarge);
}

// ---- robustness and the unchanged S1 behavior ------------------------------------------------------------------------------------

#[test]
fn no_prefix_and_no_single_byte_change_of_a_valid_snapshot_panics_or_is_wrongly_reported() {
    let good = bytes(&base());
    for n in 0..good.len() {
        let prefix = good.get(..n).expect("prefix");
        assert!(matches!(evaluate_snapshot(Some(prefix), &request(), NOW), Verdict::NotAssessed(_)), "prefix of {n} bytes");
    }
    for i in 0..good.len() {
        let mut changed = good.clone();
        *changed.get_mut(i).expect("in range") = b'x';
        // Any answer is fine as long as it does not panic; a changed snapshot may not claim a pass for another request.
        if let Verdict::DesktopReported(r) = evaluate_snapshot(Some(changed.as_slice()), &request(), NOW) {
            assert_eq!(r.report.provenance.commit_sha, SHA);
            assert_eq!(r.report.provenance.run_id, RUN);
        }
    }
    for now in [i64::MIN, -1, 0, RECORDED_UNIX, i64::MAX] {
        let _ = evaluate_snapshot(Some(good.as_slice()), &request(), now);
    }
}

#[test]
fn the_v1_checkpoint_tool_and_the_tool_list_are_unchanged() {
    assert_eq!(tools::TOOL_NAMES, ["atm_catalog_search_v1", "atm_catalog_get_v1", "atm_runtime_status_v1", "atm_checkpoint_summary_v1"]);
    assert_eq!(tools::definitions().len(), 4);
    let args = json!({"commitSha": SHA, "runId": 5, "runAttempt": 1});
    let map = args.as_object().expect("object");
    match tools::dispatch(tools::T_CHECKPOINT, map) {
        Some(Ok((v, _))) => {
            assert_eq!(v["schema"], "atm.mcp.checkpoint_summary.v1");
            assert_eq!(v["outcome"], "NOT_ASSESSED");
            assert_eq!(v["error"], "LIVE_VERIFICATION_NOT_AVAILABLE_IN_MCP_V1");
        }
        _ => panic!("the v1 checkpoint tool changed"),
    }
}
