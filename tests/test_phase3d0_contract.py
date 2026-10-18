"""Phase 3D-0: independent checks of the jq provenance evidence and of the real-device acceptance probe.

* The attestation tests run `tools/verify_sigstore_bundle.py` (no network, no execution of anything) on the stored PUBLIC metadata of
  the jq 1.8.2 release attestations, and then on deliberately damaged copies: every damage must be detected.
* The probe tests read the headless self-check output (`ATM_SELFCHECK_JSON`); without it they are skipped.
Nothing here downloads or runs jq or any other third-party program.
"""

import base64
import copy
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
EVID = ROOT / "docs" / "phase-3d0" / "evidence"
TOOL = ROOT / "tools" / "verify_sigstore_bundle.py"
SELFCHECK = os.environ.get("ATM_SELFCHECK_JSON")

WIN = ("jq-windows-amd64.exe", "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627", "jq-1.8.2-windows-amd64.attestations.json")
MAC = ("jq-macos-arm64", "2d75340ba57a4b4b4c8708a21c2dc8e958a48aaa8bba13b27f77f6e4c0eca07e", "jq-1.8.2-macos-arm64.attestations.json")
COMMIT = "34f7186b86743a083a589741b6cea95293524108"


def verify(bundle, name, sha, trusted=None, ref="refs/tags/jq-1.8.2", commit=COMMIT, repo="https://github.com/jqlang/jq", path=".github/workflows/ci.yml"):
    cmd = [
        sys.executable, "-I", str(TOOL), "--bundle", str(bundle), "--trusted-root", str(trusted or EVID / "sigstore-trusted-root.jsonl"),
        "--subject-name", name, "--subject-sha256", sha, "--repo-uri", repo, "--workflow-path", path, "--ref", ref, "--commit", commit,
        "--repo-id", "5101141", "--owner-id", "104800540",
    ]
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    out = json.loads(p.stdout) if p.stdout.strip().startswith("{") else None
    return p.returncode, out, p.stderr


class Attestations(unittest.TestCase):
    def test_both_target_artifacts_verify_against_the_pinned_identity_policy(self):
        for name, sha, bundle in (WIN, MAC):
            code, out, err = verify(EVID / bundle, name, sha)
            self.assertEqual(code, 0, (err, [c for c in (out or {}).get("checks", []) if not c["ok"]]))
            self.assertTrue(out["ok"])
            self.assertGreaterEqual(len(out["checks"]), 28)

    def test_a_wrong_digest_name_ref_commit_repository_or_workflow_is_refused(self):
        name, sha, bundle = WIN
        b = EVID / bundle
        cases = {
            "digest": dict(sha="0" * 64),
            "name": dict(name="jq-windows-arm64.exe"),
            "ref": dict(ref="refs/tags/jq-1.8.1"),
            "commit": dict(commit="0" * 40),
            "repo": dict(repo="https://github.com/evil/jq"),
            "workflow": dict(path=".github/workflows/other.yml"),
        }
        for what, kw in cases.items():
            code, out, _ = verify(b, kw.get("name", name), kw.get("sha", sha), ref=kw.get("ref", "refs/tags/jq-1.8.2"), commit=kw.get("commit", COMMIT),
                                  repo=kw.get("repo", "https://github.com/jqlang/jq"), path=kw.get("path", ".github/workflows/ci.yml"))
            self.assertNotEqual(code, 0, what)

    def test_every_kind_of_tampering_with_the_bundle_or_the_trust_root_is_detected(self):
        name, sha, bundle = WIN
        original = json.loads((EVID / bundle).read_text(encoding="utf-8"))

        def mutated(edit):
            d = copy.deepcopy(original)
            edit(d["attestations"][0]["bundle"])
            return d

        def flip(b64):
            raw = bytearray(base64.b64decode(b64))
            raw[len(raw) // 2] ^= 1
            return base64.b64encode(bytes(raw)).decode()

        def payload_digest(b):
            st = json.loads(base64.b64decode(b["dsseEnvelope"]["payload"]))
            st["subject"][0]["digest"]["sha256"] = "f" * 64
            b["dsseEnvelope"]["payload"] = base64.b64encode(json.dumps(st).encode()).decode()

        edits = {
            "payload changed": payload_digest,
            "signature flipped": lambda b: b["dsseEnvelope"]["signatures"][0].__setitem__("sig", flip(b["dsseEnvelope"]["signatures"][0]["sig"])),
            "certificate flipped": lambda b: b["verificationMaterial"]["certificate"].__setitem__("rawBytes", flip(b["verificationMaterial"]["certificate"]["rawBytes"])),
            "log time changed": lambda b: b["verificationMaterial"]["tlogEntries"][0].__setitem__("integratedTime", str(int(b["verificationMaterial"]["tlogEntries"][0]["integratedTime"]) + 3600)),
            "log index changed": lambda b: b["verificationMaterial"]["tlogEntries"][0].__setitem__("logIndex", str(int(b["verificationMaterial"]["tlogEntries"][0]["logIndex"]) + 1)),
            "set flipped": lambda b: b["verificationMaterial"]["tlogEntries"][0]["inclusionPromise"].__setitem__("signedEntryTimestamp", flip(b["verificationMaterial"]["tlogEntries"][0]["inclusionPromise"]["signedEntryTimestamp"])),
            "proof hash flipped": lambda b: b["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["hashes"].__setitem__(0, flip(b["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["hashes"][0])),
            "root hash flipped": lambda b: b["verificationMaterial"]["tlogEntries"][0]["inclusionProof"].__setitem__("rootHash", flip(b["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["rootHash"])),
            "log entry body changed": lambda b: b["verificationMaterial"]["tlogEntries"][0].__setitem__("canonicalizedBody", flip(b["verificationMaterial"]["tlogEntries"][0]["canonicalizedBody"])),
        }
        with tempfile.TemporaryDirectory() as tmp:
            for what, edit in edits.items():
                path = pathlib.Path(tmp) / "bundle.json"
                path.write_text(json.dumps(mutated(edit)), encoding="utf-8")
                code, out, err = verify(path, name, sha)
                self.assertNotEqual(code, 0, f"{what} was not detected")
            # a trust root without the Fulcio CAs, and one with the log key removed
            lines = [json.loads(l) for l in (EVID / "sigstore-trusted-root.jsonl").read_text(encoding="utf-8").splitlines() if l.strip()]
            for what, edit in {"no CA": lambda r: r.update(certificateAuthorities=[]), "no log": lambda r: r.update(tlogs=[])}.items():
                rs = copy.deepcopy(lines)
                for r in rs:
                    edit(r)
                path = pathlib.Path(tmp) / "root.jsonl"
                path.write_text("\n".join(json.dumps(r) for r in rs), encoding="utf-8")
                code, _, _ = verify(EVID / bundle, name, sha, trusted=path)
                self.assertNotEqual(code, 0, what)

    def test_the_attestation_covers_both_files_and_all_published_digests_agree(self):
        stmt = json.loads(base64.b64decode(json.loads((EVID / WIN[2]).read_text(encoding="utf-8"))["attestations"][0]["bundle"]["dsseEnvelope"]["payload"]))
        subjects = {s["name"]: s["digest"]["sha256"] for s in stmt["subject"]}
        self.assertEqual(subjects["jq-windows-amd64.exe"], WIN[1])
        self.assertEqual(subjects["jq-macos-arm64"], MAC[1])
        listed = dict(reversed(l.split()) for l in (EVID / "jq-1.8.2-sha256sum.txt").read_text(encoding="utf-8").splitlines() if l.strip())
        self.assertEqual(listed["jq-windows-amd64.exe"], WIN[1])
        self.assertEqual(listed["jq-macos-arm64"], MAC[1])
        # the stored trusted-root snapshot is evidence only; its digest is recorded in the evidence README
        readme = (EVID / "README.md").read_text(encoding="utf-8")
        self.assertIn(hashlib.sha256((EVID / "sigstore-trusted-root.jsonl").read_bytes()).hexdigest(), readme)

    def test_the_verifier_has_no_network_process_or_file_write_capability(self):
        text = TOOL.read_text(encoding="utf-8")
        for needle in ["import socket", "urllib", "requests", "http.client", "subprocess", "os.system", "open(path, \"w", ".write(", "shutil"]:
            self.assertNotIn(needle, text, needle)


class Documents(unittest.TestCase):
    def test_the_gate_documents_state_the_five_permissions_and_the_two_separate_decisions(self):
        text = (ROOT / "docs" / "PHASE-3D0.md").read_text(encoding="utf-8")
        for needle in ["Request A", "Actual installation (permission 4): NO-GO", "Downloading exact selected third-party artifacts", "Executing the installed tool", "No digest was entered into any reference list",
                       "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627", "2d75340ba57a4b4b4c8708a21c2dc8e958a48aaa8bba13b27f77f6e4c0eca07e"]:
            self.assertIn(needle, text)
        prov = (ROOT / "docs" / "phase-3d0" / "JQ-PROVENANCE.md").read_text(encoding="utf-8")
        for needle in ["9307 9A95 11EF C0B7 D6CD BFB5 B0DA 60FB 454B AF18", "immutable", "NOASSERTION", "Oniguruma", "no revocation", "No binary artifact was downloaded"]:
            self.assertIn(needle, prov, needle)
        for name in ["ISSUE-14-RESIDUAL-RISKS.md", "APPROVAL-READINESS.md", "DEVICE-TEST-PLAN.md"]:
            self.assertTrue((ROOT / "docs" / "phase-3d0" / name).is_file(), name)

    def test_production_lists_are_still_empty_and_no_reference_digest_was_added(self):
        self.assertIn('pub const BUILT_IN_RECIPES_JSON: &str = "[]";', (ROOT / "src-tauri" / "src" / "recipe.rs").read_text(encoding="utf-8"))
        for needle in ("a6fc67fe", "2d75340b"):
            for path in (ROOT / "src-tauri" / "src").glob("*.rs"):
                self.assertNotIn(needle, path.read_text(encoding="utf-8"), f"{needle} in {path.name}")
            self.assertNotIn(needle, (ROOT / "data" / "catalog-seed.json").read_text(encoding="utf-8"))


@unittest.skipUnless(SELFCHECK and pathlib.Path(SELFCHECK).is_file(), "ATM_SELFCHECK_JSON not provided; Rust self-check output not available")
class FsAcceptanceProbe(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.probe = json.loads(pathlib.Path(SELFCHECK).read_text(encoding="utf-8"))["fsAcceptanceProbe"]

    def results(self):
        return {c["id"]: c for c in self.probe["checks"]}

    def test_it_ran_with_no_failure_and_reports_codes_only(self):
        self.assertTrue(self.probe["ran"])
        self.assertEqual(self.probe["failed"], 0, [c for c in self.probe["checks"] if c["result"] == "fail"])
        text = json.dumps(self.probe)
        for needle in ["C:\\", "/Users/", "/home/", "/private/", "/tmp", "artificial payload", "fs-acceptance-"]:
            self.assertNotIn(needle, text)
        for c in self.probe["checks"]:
            self.assertIn(c["result"], ("pass", "observed", "not_exercised"))

    def test_the_core_guarantees_are_checked_on_this_volume(self):
        r = self.results()
        for cid in [
            "anchor_opens_without_following_links", "owner_only_root_is_created_and_verified", "files_can_be_created_and_flushed",
            "exclusive_create_refuses_an_existing_name", "identity_and_single_link_are_read_from_the_handle",
            "deletion_refuses_a_same_content_replacement", "deletion_of_the_verified_file_works",
        ]:
            self.assertEqual(r[cid]["result"], "pass", cid)

    def test_no_clobber_and_replacement_primitives_are_reported_per_volume(self):
        r = self.results()
        self.assertIn(r["no_clobber_move_works_on_this_volume"]["result"], ("pass", "observed"))
        if r["no_clobber_move_works_on_this_volume"]["result"] == "pass":
            self.assertEqual(r["no_clobber_move_refuses_an_existing_destination"]["result"], "pass")
            self.assertEqual(r["replacement_refuses_an_unknown_destination"]["result"], "pass")
        self.assertIn(self.probe["platform"], ("windows", "macos"))
        self.assertTrue(isinstance(self.probe["filesystem"], str))


if __name__ == "__main__":
    unittest.main()
