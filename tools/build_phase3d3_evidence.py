#!/usr/bin/env python3
"""Phase 3D-3: assembles the three data records of the jq 1.8.2 / Windows x64 pilot candidate from files that are ALREADY in the repository.

Offline and read-only: it opens no network connection, starts no program and never opens the retained jq executable (it only hashes the
text records that were kept in the repository). Output files (all under docs/phase-3d3/):

  evidence/jq-1.8.2-windows-x64.source-attestation.json   the SourceAttestation record (src-tauri/src/attestation.rs)
  evidence/jq-1.8.2-windows-x64.local-use-license.json    the LocalUseLicense record
  jq-1.8.2-windows-x64.recipe.candidate.json              the candidate recipe (review state: draft; NOT registered anywhere)

Run again after changing any input; tests/test_phase3d3_contract.py recomputes every hash and compares.
"""
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "phase-3d3"
WIN_SHA = "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627"
COMMIT = "34f7186b86743a083a589741b6cea95293524108"
SESSION = "docs/phase-3d0/evidence/request-a-session-20261018"
E0 = "docs/phase-3d0/evidence"
RETAINED = [
    f"{E0}/jq-1.8.2-windows-amd64.attestations.json",
    f"{E0}/report-windows-amd64.json",
    f"{E0}/jq-1.8.2-sha256sum.txt",
    f"{SESSION}/review-v2.json",
    f"{SESSION}/jq-windows-amd64.exe.gh.json",
    f"{SESSION}/jq-windows-amd64.exe.gpg.status",
    f"{SESSION}/jq-windows-amd64.exe.custom.json",
    f"{SESSION}/authenticode-windows.json",
    f"{SESSION}/static-header-facts.json",
    f"{SESSION}/network-incident.json",
]
NOTICE_BUNDLE = "docs/phase-3d2/notices/THIRD-PARTY-NOTICES.jq-1.8.2.windows-x64.txt"
REVIEW_RECORD = "docs/phase-3d3/LICENSE-LOCAL-USE-REVIEW.md"
SPDX = "LicenseRef-jq-1.8.2-win-x64"


def sha(rel):
    # the repository stores text as LF (.gitattributes), whatever a working copy looks like: the recorded hash is of the committed form
    return hashlib.sha256((ROOT / rel).read_bytes().replace(bytes([13, 10]), bytes([10]))).hexdigest()


def write(path, obj):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")


def main():
    listing = f"{SESSION}/session-sha256.txt"
    if f"{WIN_SHA}" not in (ROOT / listing).read_text(encoding="utf-8"):
        sys.exit("the retained session listing does not list the jq digest")
    attestation = {
        "schemaVersion": 1,
        "evidenceId": "jq-1-8-2-windows-x64-request-a",
        "toolId": "jq",
        "version": "1.8.2",
        "platformTarget": "windows-x64",
        "artifact": {"fileName": "jq-windows-amd64.exe", "sizeBytes": 1035264, "sha256": WIN_SHA, "format": "pe-x64"},
        "repository": {"slug": "jqlang/jq", "repositoryId": 5101141, "ownerId": 104800540},
        "source": {"tag": "jq-1.8.2", "commit": COMMIT},
        "workflow": {
            "path": ".github/workflows/ci.yml",
            "gitRef": "refs/tags/jq-1.8.2",
            "issuer": "https://token.actions.githubusercontent.com",
            "runnerEnvironment": "github-hosted",
            "trigger": "push",
            "runUrl": "https://github.com/jqlang/jq/actions/runs/27872779544/attempts/1",
            "transparencyLogIndex": 1884519155,
        },
        "verification": {
            "methods": [
                "gh attestation verify (official verifier, strict identity policy, online and byte-exact offline replay)",
                "tools/verify_sigstore_bundle.py + tools/review_jq_verification.py (supporting verifier, 151 checks over the retained session)",
                "gpg detached signature, isolated keyring, pinned fingerprint",
            ],
            "sessionId": "jq-review-20261018T2240Z",
            "checksTotal": 151,
            "checksFailed": 0,
            "officialVerifierPassed": True,
            "exactBytesVerified": True,
            "gpgFingerprint": "93079A9511EFC0B7D6CDBFB5B0DA60FB454BAF18",
            "gpgSignatureValid": True,
            "networkPolicyCompliant": False,
            "networkIncident": "the first online `gh attestation verify` contacted tmaproduction.blob.core.windows.net, a host outside the approved list; recorded permanently in network-incident.json and NOT accepted retroactively",
            "offlineReplayContactedHosts": 0,
            "acceptance": "Issue #19 closure: the cryptographic evidence is conditionally accepted; the network contact is not. Effective in this application only if a reviewed change compiles this record in.",
        },
        "retainedEvidence": [{"path": p, "sha256": sha(p)} for p in RETAINED],
        "retainedSessionListingSha256": sha(listing),
    }
    write(OUT / "evidence" / "jq-1.8.2-windows-x64.source-attestation.json", attestation)

    notice_sha = sha(NOTICE_BUNDLE)
    conditions = [
        {"id": "C1", "text": "The project does not host, mirror, bundle or re-sign the executable.", "satisfied_by": "No jq byte is in the repository (tests/test_phase3d3_contract.py hashes every tracked file); the application ships no jq; the file is the user's own retained copy of the upstream release asset; the manager only writes it into the user's own manager folder."},
        {"id": "C2", "text": "Third-party notices are available to the person inside the application; the 'next to the installed version' half is replaced.", "satisfied_by": "Application-presented, digest-bound complete notice bundle (docs/phase-3d2/NOTICES-DECISION.md); amendment of C2 instructed for this local-use pilot only."},
        {"id": "C3", "text": "Q1-Q3 are evaluated in writing.", "satisfied_by": "docs/phase-3d3/LICENSE-LOCAL-USE-REVIEW.md section 3: none of them blocks a local, non-redistributing copy; all stay open for redistribution."},
        {"id": "C4", "text": "Any move toward redistribution re-opens the review.", "satisfied_by": "The recipe state is accepted_local_use, which never satisfies redistribution_permitted(); the record is bound to one file digest and one notice text."},
        {"id": "C5", "text": "license.state stays unknown until C1-C3 are recorded.", "satisfied_by": "C1-C3 are recorded here; the state becomes accepted_local_use, never accepted."},
    ]
    questions = [
        {"id": "Q1", "text": "Do Oniguruma's compiled Unicode 16.0 tables count as 'Data Files or Software' under Unicode License V3?", "blocks_local_use": False, "blocks_redistribution": True},
        {"id": "Q2", "text": "Which mingw-w64 runtime parts are linked (stripped executable)?", "blocks_local_use": False, "blocks_redistribution": True},
        {"id": "Q3", "text": "Is libgcc linked, and does the upstream build meet every detail of the GCC Runtime Library Exception? (libssp is linked.)", "blocks_local_use": False, "blocks_redistribution": True},
    ]
    local_use = {
        "schemaVersion": 1,
        "evidenceId": "R34",
        "scope": "local_use_no_redistribution",
        "decision": "LICENSE_PASS_LOCAL_USE",
        "toolId": "jq",
        "version": "1.8.2",
        "platformTarget": "windows-x64",
        "artifactSha256": WIN_SHA,
        "spdxExpression": SPDX,
        "noticeBundleSha256": notice_sha,
        "reviewRecordPath": REVIEW_RECORD,
        "reviewRecordSha256": sha(REVIEW_RECORD),
        "conditions": [{"id": c["id"], "text": c["text"], "satisfiedBy": c["satisfied_by"]} for c in conditions],
        "openQuestions": [{"id": q["id"], "text": q["text"], "blocksLocalUse": q["blocks_local_use"], "blocksRedistribution": q["blocks_redistribution"]} for q in questions],
    }
    write(OUT / "evidence" / "jq-1.8.2-windows-x64.local-use-license.json", local_use)

    draft = json.loads((ROOT / "docs/phase-3d1/jq-1.8.2-windows-x64.recipe.draft.json").read_text(encoding="utf-8"))
    r = draft["recipe"]
    r["revision"] = 2
    r["recipeId"] = "jq-1.8.2-windows-x64"
    r["source"]["sourceAttested"] = True
    r["authentication"] = [
        {"kind": "authenticated_reference_digest", "evidenceId": "request-a-session-20261018"},
        {"kind": "source_attestation_evidence", "evidenceId": attestation["evidenceId"]},
    ]
    r["license"] = {"state": "accepted_local_use", "spdxId": SPDX, "evidenceId": "R34"}
    r["permissions"] = ["current_user_account", "write_manager_folders"]
    r["network"] = []
    r["filesystemChanges"] = [
        {"area": "manager_staging", "kind": "create"},
        {"area": "manager_versions", "kind": "create"},
        {"area": "manager_versions", "kind": "remove"},
        {"area": "manager_state", "kind": "replace"},
        {"area": "manager_state", "kind": "remove"},
    ]
    draft["review"] = {"state": "draft", "reviewerId": "none-yet", "reviewedRecipeDigest": "", "stagingEvidence": []}
    write(OUT / "jq-1.8.2-windows-x64.recipe.candidate.json", draft)
    print("ok", notice_sha[:12])


if __name__ == "__main__":
    main()
