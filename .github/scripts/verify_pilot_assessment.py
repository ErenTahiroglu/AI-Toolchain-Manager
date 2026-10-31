#!/usr/bin/env python3
"""
verify_pilot_assessment.py — Strict, truthful parser and gate validator for
Evidence-Checkpoint-Engine (ECE) assessments emitted during the ATM-GOV-02 pilot.

Enforces:
- Schema conformance
- Exit code alignment (process exit code == JSON exit_code)
- Identity binding (numeric repo ID, target commit SHA, workflow path)
- Six-dimensional evidence status
- Explicit separation of machine CI evidence from human authorization
"""

import argparse
import json
import os
import sys


def parse_args():
    parser = argparse.ArgumentParser(description="Validate ECE assessment output.")
    parser.add_argument("--mode", required=True, choices=["positive", "n1_wrong_repo", "n2_wrong_sha", "n3_failed_ci"])
    parser.add_argument("--assessment", required=True, help="Path to assessment JSON file")
    parser.add_argument("--cli-rc", required=True, type=int, help="Actual CLI process return code")
    parser.add_argument("--expected-repo", default="1413039335")
    parser.add_argument("--expected-commit", default=None)
    parser.add_argument("--caller-sha", default=None)
    parser.add_argument("--summary-file", default=None, help="Path to write markdown summary (e.g. GITHUB_STEP_SUMMARY)")
    return parser.parse_args()


def main():
    args = parse_args()

    if not os.path.isfile(args.assessment):
        print(f"ERROR: Assessment file not found: {args.assessment}", file=sys.stderr)
        sys.exit(1)

    try:
        with open(args.assessment, "r", encoding="utf-8") as f:
            doc = json.load(f)
    except Exception as e:
        print(f"ERROR: Failed to parse assessment JSON: {e}", file=sys.stderr)
        sys.exit(1)

    # Core structural checks common to all valid ECE outputs
    assert doc.get("schema_version") == "0.1.0-draft", f"Unexpected schema_version: {doc.get('schema_version')}"
    assert doc.get("engine", {}).get("name") == "Evidence-Checkpoint-Engine", f"Unexpected engine: {doc.get('engine')}"
    assert doc.get("evaluator_auth", {}).get("is_synthetic_mock") is False, "Output marked as synthetic mock!"
    assert "dimensions" in doc, "Missing dimensions in assessment"
    assert "derived_summary" in doc, "Missing derived_summary in assessment"
    assert "claims_breakdown" in doc, "Missing claims_breakdown in assessment"

    json_exit_code = doc.get("exit_code")
    assert args.cli_rc == json_exit_code, f"CLI return code ({args.cli_rc}) != JSON exit_code ({json_exit_code})"

    if args.mode == "positive":
        # Positive historical baseline verification
        assert args.cli_rc == 1, f"Expected exit code 1 (ASSESSED_LIMITATIONS), got {args.cli_rc}"
        assert doc["derived_summary"]["verdict"] == "ASSESSED_LIMITATIONS", f"Unexpected verdict: {doc['derived_summary']['verdict']}"

        # Target verification
        target = doc.get("evaluated_target", {})
        assert str(target.get("repository_id")) == str(args.expected_repo), f"Repo ID mismatch: {target.get('repository_id')} != {args.expected_repo}"
        if args.expected_commit:
            assert target.get("target_commit_sha", "").lower() == args.expected_commit.lower(), f"Commit SHA mismatch: {target.get('target_commit_sha')} != {args.expected_commit}"

        # Machine assessment breakdown
        machine = doc.get("claims_breakdown", {}).get("machine_assessed", {})
        assert machine.get("sha_verified") is True, "sha_verified is not True"
        assert machine.get("repo_id_matched") is True, "repo_id_matched is not True"

        # Dimensions: CI execution
        ci = doc["dimensions"].get("ci_execution", {})
        assert ci.get("status") == "VERIFIED_PASS", f"CI status is not VERIFIED_PASS: {ci.get('status')}"
        assert ci.get("expected_jobs_count") == 2, f"Expected jobs count != 2: {ci.get('expected_jobs_count')}"
        assert ci.get("observed_jobs_count") == 2, f"Observed jobs count != 2: {ci.get('observed_jobs_count')}"
        assert ci.get("unresolved_jobs") == [], f"Unresolved jobs not empty: {ci.get('unresolved_jobs')}"

        # Dimensions: Review & Authorization
        rev = doc["dimensions"].get("review_status", {})
        assert rev.get("status") == "UNREVIEWED", f"Unexpected review status: {rev.get('status')}"
        auth = doc["dimensions"].get("authorization", {})
        assert auth.get("status") == "AUTHORIZATION_PENDING", f"Unexpected authorization status: {auth.get('status')}"
        human_auth = doc.get("claims_breakdown", {}).get("human_authorized", {})
        assert human_auth.get("authorized") is False, "human_authorized is True (must be False in observation pilot)!"

        # Freshness
        freshness = doc["dimensions"].get("freshness", {})
        assert freshness.get("status") == "HISTORICAL_BASELINE", f"Unexpected freshness status: {freshness.get('status')}"

        # Verify historical vs caller HEAD distinction
        if args.caller_sha and args.expected_commit:
            assert args.expected_commit.lower() != args.caller_sha.lower(), "Target commit unexpectedly matches caller HEAD! Must be distinct historical baseline."

        print(f"POSITIVE VERIFICATION PASSED: Run independently evaluated as VERIFIED_PASS under operator policy.")
        print(f"Verdict: {doc['derived_summary']['verdict']} (Exit Code {args.cli_rc})")
        print(f"Human authorization: {human_auth.get('authorized')} (Full checkpoint accepted: False)")

        if args.summary_file:
            write_step_summary(args.summary_file, doc, args)

    elif args.mode == "n1_wrong_repo":
        assert args.cli_rc == 2, f"Expected exit code 2 (ASSESSED_FAIL), got {args.cli_rc}"
        assert doc["derived_summary"]["verdict"] == "ASSESSED_FAIL", f"Expected ASSESSED_FAIL, got {doc['derived_summary']['verdict']}"
        machine = doc.get("claims_breakdown", {}).get("machine_assessed", {})
        assert machine.get("repo_id_matched") is False, "repo_id_matched should be False for wrong repo ID"
        print("N1 NEGATIVE CONTROL PASSED: Wrong repository ID rejected with ASSESSED_FAIL.")

    elif args.mode == "n2_wrong_sha":
        assert args.cli_rc == 2, f"Expected exit code 2 (ASSESSED_FAIL), got {args.cli_rc}"
        assert doc["derived_summary"]["verdict"] == "ASSESSED_FAIL", f"Expected ASSESSED_FAIL, got {doc['derived_summary']['verdict']}"
        machine = doc.get("claims_breakdown", {}).get("machine_assessed", {})
        assert machine.get("sha_verified") is False, "sha_verified should be False for wrong commit SHA"
        print("N2 NEGATIVE CONTROL PASSED: Wrong commit SHA rejected with ASSESSED_FAIL.")

    elif args.mode == "n3_failed_ci":
        assert args.cli_rc == 2, f"Expected exit code 2 (ASSESSED_FAIL), got {args.cli_rc}"
        assert doc["derived_summary"]["verdict"] == "ASSESSED_FAIL", f"Expected ASSESSED_FAIL, got {doc['derived_summary']['verdict']}"
        ci = doc["dimensions"].get("ci_execution", {})
        assert ci.get("status") == "FAILED", f"Expected CI status FAILED, got {ci.get('status')}"
        assert ci.get("status") != "VERIFIED_PASS", "Failed CI must never produce VERIFIED_PASS"
        print("N3 NEGATIVE CONTROL PASSED: Historical failed Windows CI correctly evaluated as FAILED / ASSESSED_FAIL.")


def write_step_summary(path, doc, args):
    try:
        with open(path, "a", encoding="utf-8") as f:
            f.write("## ECE × ATM — CI Evidence Pilot Assessment Summary\n\n")
            f.write("| Dimension | Machine-Assessed Status | Authority Tier |\n")
            f.write("|---|---|---|\n")
            f.write(f"| **Engine** | `{doc.get('engine', {}).get('name')} v{doc.get('engine', {}).get('version')}` | `MACHINE_ASSESSED` |\n")
            f.write(f"| **Target Repository ID** | `{doc.get('evaluated_target', {}).get('repository_id')}` (ErenTahiroglu/AI-Toolchain-Manager) | `MACHINE_ASSESSED` |\n")
            f.write(f"| **Target Commit SHA** | `{doc.get('evaluated_target', {}).get('target_commit_sha')}` | `MACHINE_ASSESSED` |\n")
            f.write(f"| **Workflow Caller HEAD** | `{args.caller_sha}` | `MACHINE_OBSERVED` |\n")
            f.write(f"| **CI Execution** | `{doc['dimensions']['ci_execution']['status']}` (2/2 jobs: windows-x64, macos-arm64) | `MACHINE_ASSESSED` |\n")
            f.write(f"| **Evidence Integrity** | `{doc['dimensions']['evidence_integrity']['status']}` | `MACHINE_ASSESSED` |\n")
            f.write(f"| **Review Status** | `{doc['dimensions']['review_status']['status']}` | `INDEPENDENTLY_REVIEWED (out-of-scope)` |\n")
            f.write(f"| **Authorization** | `{doc['dimensions']['authorization']['status']}` | `HUMAN_AUTHORIZED (pending)` |\n")
            f.write(f"| **Freshness** | `{doc['dimensions']['freshness']['status']}` (historical baseline != live HEAD) | `MACHINE_ASSESSED` |\n")
            f.write(f"| **Derived Verdict** | `{doc['derived_summary']['verdict']}` (exit code {doc['exit_code']}) | `DERIVED_SUMMARY` |\n")
            f.write(f"| **Checkpoint Accepted** | `false` | `EXPLICIT_CONSTRAINT` |\n\n")
            f.write("> [!NOTE]\n")
            f.write("> **Pilot Observation Contract:** `ci_execution.status = VERIFIED_PASS` proves machine CI completion on the exact historical target run only. It does not confer code correctness, independent review sign-off, live branch HEAD approval, or human owner authorization.\n\n")
    except Exception as e:
        print(f"WARNING: Failed to write step summary: {e}", file=sys.stderr)


if __name__ == "__main__":
    main()
