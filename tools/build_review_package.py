#!/usr/bin/env python3
"""Assembles docs/phase-3d3b/review-package.json (Phase 3D-3B). Offline, read-only apart from the one output file; never opens the jq executable.

  python tools/build_review_package.py --candidate-commit <sha> --ci <ci.json> [--dev-report <staging-report.json>]

`--candidate-commit` is the commit whose tree every recorded hash describes and on which the CI run named in `--ci` ran. The package itself is
committed AFTER it, in a commit that may change only files under docs/phase-3d3b/ (tools/verify_review_package.py checks that).

`--ci` is a small JSON file written by whoever queried the CI: {"runId", "headSha", "status", "conclusion", "jobs": [{"name", "conclusion",
"failedOrSkippedSteps": [...]}]}. Nothing here invents a reviewer, a signature, a review date or a passed independent check.
"""
import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "phase-3d3b" / "review-package.json"
WIN_SHA = "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627"

ARTIFACTS = [
    ("candidate recipe (revision 2, review state draft)", "docs/phase-3d3/jq-1.8.2-windows-x64.recipe.candidate.json", True),
    ("independently computed recipe digest", "docs/phase-3d3b/evidence/candidate-recipe-digest.json", True),
    ("exact authenticated-reference entry (what the activation would add)", "docs/phase-3d3b/evidence/reference-entry.json", True),
    ("retained source-attestation record", "docs/phase-3d3/evidence/jq-1.8.2-windows-x64.source-attestation.json", True),
    ("local-use licence record (R34)", "docs/phase-3d3/evidence/jq-1.8.2-windows-x64.local-use-license.json", True),
    ("written licence review record", "docs/phase-3d3/LICENSE-LOCAL-USE-REVIEW.md", False),
    ("notice manifest", "docs/phase-3d2/notices/jq-1.8.2-windows-x64.notices-manifest.json", True),
    ("notice bundle (verbatim texts)", "docs/phase-3d2/notices/THIRD-PARTY-NOTICES.jq-1.8.2.windows-x64.txt", False),
    ("staging fixture identity", "docs/phase-3d3b/staging-fixture.json", True),
    ("staging manifest (requirement -> exact test identifiers)", "docs/phase-3d3b/staging-manifest.json", True),
    ("the reviewed-recipe schema", "schemas/reviewed-recipe.schema.json", True),
]


def canonical(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def lf(b):
    return b.replace(bytes([13, 10]), bytes([10]))


def sha(b):
    return hashlib.sha256(b).hexdigest()


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--candidate-commit", required=True)
    ap.add_argument("--ci", required=True)
    ap.add_argument("--dev-report")
    a = ap.parse_args()
    ci = json.loads(Path(a.ci).read_text(encoding="utf-8"))
    if ci["headSha"] != a.candidate_commit:
        sys.exit("the CI file is for another commit than the candidate commit")
    arts = []
    for title, rel, is_json in ARTIFACTS:
        b = lf((ROOT / rel).read_bytes())
        e = {"title": title, "path": rel, "sha256": sha(b)}
        if is_json:
            e["canonicalSha256"] = sha(canonical(json.loads(b)).encode("utf-8"))
        arts.append(e)
    cand = json.loads((ROOT / "docs/phase-3d3/jq-1.8.2-windows-x64.recipe.candidate.json").read_text(encoding="utf-8"))
    att = json.loads((ROOT / "docs/phase-3d3/evidence/jq-1.8.2-windows-x64.source-attestation.json").read_text(encoding="utf-8"))
    pkg = {
        "schemaVersion": 1,
        "purpose": "Everything an independent reviewer needs to review the jq 1.8.2 / Windows x64 candidate recipe (Issue #23, blocker B1) and to reproduce its staging checks on synthetic fixture bytes. It records evidence and its CLASS; it contains no reviewer identity, signature, review date or independent approval, and does not mark anything reviewed.",
        "candidateCommit": a.candidate_commit,
        "packageRule": "The package is committed after the candidate commit and may change only files under docs/phase-3d3b/ (tools/verify_review_package.py enforces it).",
        "target": {"tool": "jq", "version": "1.8.2", "platform": "windows-x64", "file": "jq-windows-amd64.exe", "sizeBytes": 1035264, "sha256": WIN_SHA},
        "recipe": {
            "recipeId": cand["recipe"]["recipeId"],
            "revision": cand["recipe"]["revision"],
            "recipeDigest": sha(canonical(cand["recipe"]).encode("utf-8")),
            "digestRule": "SHA-256 of the canonical JSON (sorted keys, no spaces, UTF-8) of the `recipe` object; the `review` block is not part of it",
            "offlineSourceContract": {
                "artifactOrigin": cand["recipe"]["artifact"].get("origin"),
                "network": cand["recipe"]["network"],
                "permissions": cand["recipe"]["permissions"],
                "firstStep": cand["recipe"]["steps"][0],
                "decision": "The first step keeps the kind `fetch-release-artifact` as a LOGICAL acquisition step. The recipe declares `artifact.origin: local_retained_file` (digest-bound, validator-enforced: no network list, no download permission), the plan and the preview say the bytes come from one local retained file and that nothing is downloaded, and no code interprets recipe steps as a command (see staging check S11).",
            },
        },
        "reviewState": {
            "recipeReviewState": cand["review"]["state"],
            "reviewerId": cand["review"]["reviewerId"],
            "stagingEvidence": cand["review"]["stagingEvidence"],
            "note": "Left exactly as authored: `draft`. Only an independent reviewer may set `reviewed`, over the digest above, and only independent staging evidence may be recorded as `independent: true`.",
        },
        "artifacts": arts,
        "retainedEvidence": att["retainedEvidence"],
        "retainedSessionListingSha256": att["retainedSessionListingSha256"],
        "staging": {
            "manifest": "docs/phase-3d3b/staging-manifest.json",
            "runner": "tools/run_staging.py",
            "fixture": "docs/phase-3d3b/staging-fixture.json",
            "reproduce": [
                "python tools/verify_review_package.py",
                "python tools/staging_fixture.py print",
                "python tools/run_staging.py --report my-staging-report.json",
            ],
        },
        "ci": {
            "runId": ci["runId"],
            "headSha": ci["headSha"],
            "status": ci["status"],
            "conclusion": ci["conclusion"],
            "jobs": ci["jobs"],
            "whatItIs": "CI-generated SUPPORTING evidence: the jobs built and tested this exact commit on a clean Windows and a clean macOS image, and ran tools/run_staging.py (exit status 0 = every named check passed). It does not make the developer-authored assertions independent.",
        },
        "evidenceClasses": {
            "1_developerGenerated": [
                "the candidate recipe, the reference entry, the attestation record, the licence record and review text, the notice manifest and bundle (assembled by the author from retained primary texts)",
                "all staging tests named in the staging manifest, the fixture and its independent rebuild, the review tools",
                "the local run of tools/run_staging.py (devRun) and the real-window runs in docs/phase-3d3/evidence/gui and docs/phase-3d3b/evidence/gui",
            ],
            "2_ciGenerated": ["the exact-commit Windows x64 and macOS ARM64 jobs recorded under `ci` (pass/fail of steps; per-test results are not read from the logs)"],
            "3_independentlyReviewedByChatGPT": [
                "Request A cryptographic evidence of jq 1.8.2: recorded in the repository as conditionally accepted at the Issue #19 closure (the network contact of its first online run is NOT accepted); this package does not re-verify the cryptography",
                "the instruction to replace the sidecar notice requirement by the application-presented, digest-bound notice manifest for the local-use pilot only: recorded as given in Phase 3D-3",
                "NOTHING ELSE. No reviewer has yet reviewed this recipe, its staging evidence, the licence record or the reference entry.",
            ],
            "4_awaitingIndependentReview": [
                "the candidate recipe itself (review state, over the digest above)",
                "independent staging evidence (the developer-authored tests are not it)",
                "the local-use licence record and its single legal assumption",
                "the source-attestation record and the reference entry",
                "completeness of the notice bundle",
                "the real-window run reproduced on a machine the author does not control",
                "the activation diff (B2), the user's authorisation (B3)",
            ],
        },
        "knownLimitations": [
            "Developer-authored tests prove the code matches the author's own understanding; they are not scientifically independent.",
            "The Request A verification (151 checks, official verifier, GPG) is recorded, and the hashes of its retained records are checked here; the cryptography is NOT redone in this package. A reviewer can replay it offline from the retained session (docs/phase-3d0/evidence/request-a-session-20261018/closure-replay).",
            "The first online run of Request A contacted a host outside the approved list (recorded in the attestation record; not repeated; the offline replay contacted none).",
            "The fixture is a made-up stand-in with a minimal native header. The real file has never been passed through check_artifact_for in any test (it was not opened in this or the previous phase); its PE/x64 format, size and digest are checked at the moment of a real preview.",
            "No operating-system-level network isolation is available without administrator rights. The offline claim rests on code evidence (no network client in the application, the workflow never reads recipe steps, the window can reach only its own IPC, no http/shell/opener/fs permission) and on the recipe/plan/preview contract.",
            "The licence decision is an engineering evaluation, not legal advice, and rests on one stated legal assumption.",
            "The installed file will be named `artifact.bin` (the adapter's fixed name), and no `jq` catalog entry exists yet (the recipe's tool id must equal a catalog entry id): both belong to the activation change.",
            "The Phase 3D-0 guard that forbids the jq digest as a literal in Rust sources will have to be replaced by an exact-one-entry guard in the activation change.",
            "One intermittent test failure was seen once on a fresh binary in Phase 3D-3 and is unexplained (docs/phase-3d3/INTERMITTENT-TEST-NOTE.md).",
            "Smart App Control may block a freshly built test binary; such a run is BLOCKED / NOT RUN and is never counted as passed (docs/phase-3d3/SMART-APP-CONTROL-RECORD.md).",
            "The macOS runner is not a Mac desktop; the candidate is Windows x64 only. Issue #14 stays open.",
        ],
    }
    if a.dev_report:
        rp = Path(a.dev_report)
        d = json.loads(rp.read_text(encoding="utf-8"))
        pkg["devRun"] = {
            "report": str(rp.relative_to(ROOT)).replace("\\", "/"),
            "sha256": sha(lf(rp.read_bytes())),
            "commit": d["commit"],
            "workingTreeClean": d["workingTreeClean"],
            "tools": d["tools"],
            "counts": d["counts"],
            "allPassed": d["allPassed"],
            "evidenceClass": "developer-generated",
        }
    OUT.write_text(json.dumps(pkg, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")
    print("written", OUT.relative_to(ROOT))


if __name__ == "__main__":
    main()
