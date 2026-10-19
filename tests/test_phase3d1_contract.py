"""Phase 3D-1: independent checks of the licence-review evidence, the DRAFT jq recipe, the documents' honesty, and the self-check probe.

Nothing here downloads, installs, starts or enrols anything; it reads files that are in the repository (and, if provided, the
headless self-check output).
"""

import hashlib
import json
import os
import pathlib
import re
import unittest
from urllib.parse import unquote

ROOT = pathlib.Path(__file__).resolve().parents[1]
D1 = ROOT / "docs" / "phase-3d1"
LIC = D1 / "evidence" / "license"
SELFCHECK = os.environ.get("ATM_SELFCHECK_JSON")
WIN_SHA = "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627"


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


class LicenceEvidence(unittest.TestCase):
    def test_every_stored_file_matches_its_manifest_hash_and_carries_no_personal_path(self):
        man = json.loads((LIC / "evidence-manifest.json").read_text(encoding="utf-8"))
        names = {f["file"] for f in man["files"]}
        self.assertGreaterEqual(len(names), 20)
        for f in man["files"]:
            p = LIC / f["file"]
            self.assertTrue(p.is_file(), f["file"])
            self.assertEqual(sha(p), f["sha256"], f["file"])
            self.assertEqual(p.stat().st_size, f["bytes"], f["file"])
            text = p.read_text(encoding="utf-8", errors="replace")
            for needle in ["C:\\Users", "/Users/", "erent", "AppData"]:
                self.assertNotIn(needle, text, f"{needle} in {f['file']}")
        for must in ["jq-COPYING.txt", "onig-COPYING.txt", "winpthreads-COPYING.txt", "mingw-w64-COPYING.MinGW-w64-runtime.txt", "gcc-COPYING.RUNTIME.txt", "pe-static.json", "jq-src-notice-scan.json"]:
            self.assertIn(must, names)

    def test_the_research_used_one_host_and_the_guard_saw_no_incident(self):
        for i in range(1, 5):
            rep = json.loads((LIC / f"guard-report-{i}.json").read_text(encoding="utf-8"))
            self.assertEqual([h["host"] for h in rep["contactedHosts"]], ["api.github.com"], i)
            self.assertEqual(rep["incidents"], [], i)
            self.assertTrue(rep["guard"]["used"])
            self.assertEqual(rep["guard"]["allowedHosts"], ["api.github.com"])

    def test_the_static_analysis_supports_what_the_review_says_about_the_binary(self):
        pe = json.loads((LIC / "pe-static.json").read_text(encoding="utf-8"))
        self.assertEqual(pe["file"], "jq-windows-amd64.exe")
        self.assertEqual(pe["bytes"], 1035264)
        dlls = set(pe["importedDlls"])
        self.assertTrue(all(d.lower().startswith("api-ms-win-crt-") or d in ("KERNEL32.dll", "SHLWAPI.dll") for d in dlls), dlls)
        self.assertFalse(any("pthread" in d.lower() or "gcc" in d.lower() for d in dlls), "winpthreads and libgcc are static, not DLLs")
        text = "\n".join(pe["stringsMatchingLicenseOrToolchainKeywords"])
        for needle in ["Built by MSYS2 project", "Mingw-w64 runtime failure", "winpthreads/src/rwlock.c", "ONIG-MONITOR", "decNumber 3.68", "--enable-all-static"]:
            self.assertIn(needle, text, needle)
        self.assertFalse(pe["hasDelayImports"])

    def test_the_draft_notice_bundle_quotes_the_stored_texts_and_their_hashes(self):
        draft = (D1 / "THIRD-PARTY-NOTICES.jq-1.8.2.windows-x64.DRAFT.txt").read_text(encoding="utf-8")
        self.assertIn("DRAFT", draft.splitlines()[0] + draft.splitlines()[2])
        self.assertIn("NOT legal advice", draft)
        for fname in ["jq-COPYING.txt", "onig-COPYING.txt", "mingw-w64-COPYING.txt", "mingw-w64-COPYING.MinGW-w64-runtime.txt", "winpthreads-COPYING.txt", "gcc-COPYING.RUNTIME.txt"]:
            self.assertIn(f"SHA-256 {sha(LIC / fname)}", draft, fname)
            body = (LIC / fname).read_text(encoding="utf-8").strip()
            self.assertIn(body, draft, f"{fname} is included verbatim")

    def test_the_decision_is_conditional_and_names_the_open_questions(self):
        text = (D1 / "LICENSE-REVIEW-JQ-1.8.2-WINDOWS.md").read_text(encoding="utf-8")
        self.assertIn("**Decision: `LICENSE_CONDITIONAL`.**", text)
        self.assertNotIn("**Decision: `LICENSE_PASS`", text)
        for needle in ["not legal advice", "Q1", "Q2", "Q3", "Q4", "Q5", "Oniguruma", "winpthreads", "libgcc", "GCC Runtime Library Exception", "Bison", "decNumber", "dtoa", "Heimdal", "NetBSD", "not exhaustive"]:
            self.assertIn(needle, text, needle)


class DraftRecipe(unittest.TestCase):
    def setUp(self):
        self.d = json.loads((D1 / "jq-1.8.2-windows-x64.recipe.draft.json").read_text(encoding="utf-8"))
        self.r = self.d["recipe"]

    def test_it_pins_the_verified_file_and_is_a_draft_with_no_review(self):
        self.assertEqual((self.r["toolId"], self.r["version"], self.r["target"]["platformTarget"]), ("jq", "1.8.2", "windows-x64"))
        self.assertEqual((self.r["artifact"]["sha256"], self.r["artifact"]["sizeBytes"], self.r["artifact"]["format"]), (WIN_SHA, 1035264, "pe-x64"))
        self.assertEqual(self.r["steps"][1]["expectedSha256"], WIN_SHA)
        self.assertEqual(self.d["review"]["state"], "draft")
        self.assertEqual(self.d["review"]["stagingEvidence"], [])
        self.assertEqual(self.r["license"]["state"], "unknown")
        self.assertFalse(self.r["source"]["sourceAttested"])
        self.assertEqual(self.r["lifecycleActions"], ["install", "uninstall"])

    def test_the_sha_matches_the_request_a_evidence_and_the_review_pin(self):
        rv = (ROOT / "tools" / "review_jq_verification.py").read_text(encoding="utf-8")
        self.assertIn(WIN_SHA, rv)
        listing = (ROOT / "docs" / "phase-3d0" / "evidence" / "pinned-f0dabad36a5fdb37393829e6bd4c465d70c40a3a" / "sha256sum.txt").read_text(encoding="utf-8")
        self.assertRegex(listing, rf"(?m)^{WIN_SHA}\s+\*?jq-windows-amd64\.exe\s*$")

    def test_nothing_in_production_code_or_data_refers_to_the_draft_or_activates_it(self):
        src = ROOT / "src-tauri" / "src"
        for p in src.glob("*.rs"):
            if p.name.endswith("_tests.rs"):
                continue
            text = p.read_text(encoding="utf-8")
            self.assertNotIn("recipe.draft.json", text, p.name)
            if p.name in ("recipe.rs", "plan.rs", "adapter.rs", "lifecycle.rs", "commands.rs", "state.rs", "lib.rs", "catalog.rs"):
                self.assertNotIn("phase-3d1", text, p.name)  # a comment elsewhere may point at the documents; the engine never reads them
        recipe = (src / "recipe.rs").read_text(encoding="utf-8")
        self.assertIn('pub const BUILT_IN_RECIPES_JSON: &str = "[]";', recipe)
        adapter = (src / "adapter.rs").read_text(encoding="utf-8")
        self.assertIn("production_adapter_not_enabled", adapter)
        for p in (ROOT / "data").glob("*.json"):
            self.assertNotIn(WIN_SHA, p.read_text(encoding="utf-8"), f"{p.name}: no reference digest for jq is entered anywhere")


class Documents(unittest.TestCase):
    def md(self, name):
        return (D1 / name).read_text(encoding="utf-8")

    def test_links_in_the_new_documents_resolve(self):
        link = re.compile(r"\]\(([^)]+)\)")
        files = [ROOT / "docs" / "PHASE-3D1.md", *sorted(D1.glob("*.md"))]
        for f in files:
            for target in link.findall(f.read_text(encoding="utf-8")):
                target = unquote(target.split("#", 1)[0])
                if not target or "://" in target:
                    continue
                self.assertTrue((f.parent / target).exists(), f"broken link {target} in {f.relative_to(ROOT)}")

    def test_the_hello_document_states_availability_and_the_unexercised_parts_honestly(self):
        t = self.md("WINDOWS-HELLO-APPROVAL.md")
        for needle in ["api_reachable_and_hello_available", "unsigned", "never run", "Prompt parenting", "fails closed", "Option A", "Option B", "Option C", "OsReportedHardware",
                       "with the code as it is, a real Windows Hello key can never satisfy the production policy", "ENABLE-OS-APPROVAL", "DISABLE-OS-APPROVAL", "deletes the OS key first"]:
            self.assertIn(needle, t, needle)

    def test_the_request_is_a_proposal_and_names_what_it_does_not_cover(self):
        t = self.md("FIRST-INSTALL-AUTHORIZATION-REQUEST.tr.md")
        for needle in ["TEKLİFTİR", "Hiçbir yetki verilmemiştir", "Talep B", "Talep C", a_sha(), "Yeni indirme yoktur", "Hiçbir şey çalıştırılmaz", "PATH", "Ön koşullar", "Red Team", "kapalıdır"]:
            self.assertIn(needle, t, needle)

    def test_the_dry_run_document_lists_what_it_did_not_exercise(self):
        t = self.md("SYNTHETIC-DRY-RUN.md")
        for needle in ["rehearsal", "NOT exercised", "Windows Hello", "Real power loss", "second independent PC", "not a jq installation"]:
            self.assertIn(needle, t, needle)

    def test_the_issue_14_note_keeps_the_issue_open_and_does_not_equate_a_vm_with_a_second_pc(self):
        t = self.md("ISSUE-14-WINDOWS-PILOT-HARDWARE-GATE.md")
        self.assertIn("**Issue #14 stays open.**", t)
        for needle in ["H1", "H4", "H8", "never silently for all", "cannot close Issue #14", "replace H4/H5", "same host"]:
            self.assertIn(needle, t, needle)


def a_sha():
    return WIN_SHA


@unittest.skipUnless(SELFCHECK and pathlib.Path(SELFCHECK).is_file(), "ATM_SELFCHECK_JSON not provided; Rust self-check output not available")
class HelloProbe(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.data = json.loads(pathlib.Path(SELFCHECK).read_text(encoding="utf-8"))
        cls.probe = cls.data["windowsHelloProbe"]

    def test_the_probe_is_read_only_and_reports_closed_codes_only(self):
        p = self.probe
        if self.data["platform"] == "windows":
            self.assertIn(p["state"], ("api_reachable_and_hello_available", "api_reachable_hello_not_available", "api_error"))
            self.assertIs(p["readOnly"], True)
            self.assertIs(p["keyCreated"], False)
            self.assertIs(p["promptShown"], False)
            self.assertIn(p["processHasNoPackageIdentity"], (True, False, None))
            self.assertIn(p["keyCredentialManagerIsSupported"]["result"], ("ok", "error"))
        else:
            self.assertEqual(p["state"], "not_applicable_platform")
        text = json.dumps(p)
        for needle in ["C:\\", "/Users/", "/home/", "credential id", "PIN"]:
            self.assertNotIn(needle, text)


if __name__ == "__main__":
    unittest.main()
