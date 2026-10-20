"""Phase 3D-4 (B2) contract: the independent B1 decision bound to the exact recipe, and a pilot configuration that stays closed.

Offline, repository files only. Nothing here downloads, installs, opens or runs jq, and nothing here fabricates a review: every value that
claims to come from the reviewer is checked to occur verbatim in the capture of the reviewer's own comment (Issue #23, comment 6299270819).
"""
import hashlib
import json
import os
import re
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src-tauri" / "src"
D4 = ROOT / "docs" / "phase-3d4"
D3 = ROOT / "docs" / "phase-3d3"
D3B = ROOT / "docs" / "phase-3d3b"

WIN_SHA = "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627"
RECIPE_DIGEST = "9c7a020d65d233e86577464a06c32b8f08fd4fd8403110e68fd551b674f7cca1"
ACCEPTED_SHA = "20106944977c96195e84b4d3cdcf1a1743877e35"


def text(p):
    return Path(p).read_text(encoding="utf-8").replace("\r\n", "\n")


def jl(p):
    return json.loads(text(p))


def canonical(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def sha(b):
    return hashlib.sha256(b).hexdigest()


def production(name):
    return text(SRC / name).split("#[cfg(test)]\n#[path")[0]


class BindingRecord(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.b = jl(D4 / "b1-review-binding.json")
        cls.capture_bytes = (D4 / "evidence" / "issue-23-comment-6299270819.md").read_bytes()
        cls.capture = cls.capture_bytes.decode("utf-8")
        cls.cand = jl(D3 / "jq-1.8.2-windows-x64.recipe.candidate.json")

    def test_the_source_is_the_real_recorded_comment_and_its_capture_is_pinned(self):
        s = self.b["source"]
        self.assertEqual((s["kind"], s["issue"], s["commentId"]), ("github_issue_comment", 23, 6299270819))
        self.assertEqual(s["url"], "https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/23#issuecomment-6299270819")
        self.assertEqual(sha(self.capture_bytes), s["snapshotSha256"])
        self.assertNotIn(b"\r", self.capture_bytes, "the capture is committed with LF only, so its hash is the same on every checkout")
        self.assertEqual(s["snapshotPath"], "docs/phase-3d4/evidence/issue-23-comment-6299270819.md")
        # the developer's own pin is labelled as such, never as a value the reviewer published
        self.assertIn("DEVELOPER", s["snapshotNote"])
        self.assertTrue(self.capture.startswith("# Independent Red Team B1 decision"))

    def test_every_hash_and_sha_the_binding_attributes_to_the_reviewer_occurs_in_the_capture(self):
        p = self.b["reviewerPublished"]
        a = self.b["accepted"]
        for v in (a["finalSha"], a["recipeDigest"], a["artifactSha256"], p["examinedZipSha256"], p["examinedFinalTree"], p["examinedCandidateTree"],
                  p["decisionFileSha256"], p["sourceReportSha256"], p["checkResultsSha256"]):
            self.assertIn(v, self.capture, v)
        # and the capture contains no 64-hex value that the binding does not know about (nothing silently dropped)
        known = {a["recipeDigest"], a["artifactSha256"], p["examinedZipSha256"], p["decisionFileSha256"], p["sourceReportSha256"], p["checkResultsSha256"]}
        self.assertEqual(set(re.findall(r"\b[0-9a-f]{64}\b", self.capture)), known)

    def test_the_decision_says_what_the_binding_relies_on(self):
        for phrase in ("B1 LIMITED-PILOT ACCEPTED", "does NOT mean full independently executed Rust/frontend staging", "NOT independently run by ChatGPT",
                       "B2 GO (implementation and synthetic testing only)", "B3 real installation NO-GO", "Do not fabricate reviewer identities",
                       "mechanically binding this reviewer-issued decision to the exact candidate recipe in a separate B2 commit"):
            self.assertIn(phrase, self.capture, phrase)

    def test_the_candidate_recipe_is_unchanged_and_its_digest_is_the_one_the_reviewer_accepted(self):
        digest = sha(canonical(self.cand["recipe"]).encode("utf-8"))
        self.assertEqual(digest, RECIPE_DIGEST)
        self.assertEqual(self.b["accepted"]["recipeDigest"], RECIPE_DIGEST)
        self.assertEqual(jl(D3B / "evidence" / "candidate-recipe-digest.json")["recipeDigest"], RECIPE_DIGEST)
        # the candidate FILE is byte-for-byte what the review package pinned (so the independent reviewer's file hash still applies)
        pkg = jl(D3B / "review-package.json")
        pinned = [x for x in pkg["artifacts"] if x["path"] == "docs/phase-3d3/jq-1.8.2-windows-x64.recipe.candidate.json"][0]
        self.assertEqual(sha(text(D3 / "jq-1.8.2-windows-x64.recipe.candidate.json").encode("utf-8")), pinned["sha256"])
        rv = self.cand["review"]
        self.assertEqual((rv["state"], rv["reviewerId"], rv["reviewedRecipeDigest"], rv["stagingEvidence"]), ("draft", "none-yet", "", []),
                         "the reviewed state lives only in the binding; the candidate file was not edited")
        # it is still the content the accepted commit had
        old = subprocess.run(["git", "show", f"{ACCEPTED_SHA}:docs/phase-3d3/jq-1.8.2-windows-x64.recipe.candidate.json"], cwd=ROOT, capture_output=True)
        if old.returncode == 0:
            self.assertEqual(json.loads(old.stdout.decode("utf-8").replace("\r\n", "\n")), self.cand)

    def test_the_accepted_facts_equal_the_recipe_the_reference_and_the_target(self):
        a, r = self.b["accepted"], self.cand["recipe"]
        self.assertEqual((a["recipeId"], a["revision"], a["toolId"], a["version"], a["platformTarget"]),
                         (r["recipeId"], r["revision"], r["toolId"], r["version"], r["target"]["platformTarget"]))
        self.assertEqual((a["fileName"], a["sizeBytes"], a["artifactSha256"], a["origin"]),
                         (r["artifact"]["fileName"], r["artifact"]["sizeBytes"], r["artifact"]["sha256"], r["artifact"]["origin"]))
        self.assertEqual((a["fileName"], a["sizeBytes"], a["artifactSha256"]), ("jq-windows-amd64.exe", 1035264, WIN_SHA))
        self.assertEqual(a["finalSha"], ACCEPTED_SHA)
        ref = jl(D3B / "evidence" / "reference-entry.json")
        self.assertEqual((ref["entryId"], ref["version"], ref["platformTarget"], ref["sha256"]), ("jq", "1.8.2", "windows-x64", WIN_SHA))

    def test_the_review_block_is_exactly_the_reviewers_own_evidence_and_nothing_a_developer_made(self):
        rv = self.b["review"]
        p = self.b["reviewerPublished"]
        self.assertEqual((rv["state"], rv["reviewedRecipeDigest"], rv["reviewerId"]), ("reviewed", RECIPE_DIGEST, self.b["reviewer"]["id"]))
        self.assertEqual(sorted(e["evidenceDigest"] for e in rv["stagingEvidence"]), sorted([p["checkResultsSha256"], p["sourceReportSha256"]]))
        for e in rv["stagingEvidence"]:
            self.assertEqual((e["state"], e["independent"]), ("passed", True))
            self.assertIn(e["evidenceDigest"], self.capture)
        self.assertEqual(self.b["reviewer"]["name"], "ChatGPT")
        self.assertIn("ChatGPT", self.capture)
        self.assertIn("none", self.b["reviewer"]["signature"].lower())
        self.assertNotIn("e" * 64, text(D4 / "b1-review-binding.json"))

    def test_nothing_is_overclaimed(self):
        b = self.b
        self.assertIs(b["supportingCi"]["independent"], False)
        self.assertEqual(b["supportingCi"]["headSha"], ACCEPTED_SHA)
        self.assertEqual(b["supportingCi"]["runId"], 39344235717)
        self.assertEqual(sorted(b["notIndependentlyExecuted"]), ["frontend-staging-checks-2", "rust-tests-48", "windows-gui"])
        joined = " ".join(b["limitations"] + b["scope"])
        for needle in ("did NOT run the 48 Rust tests", "Issue #14", "Request A unauthorized network contact remains FAIL", "not redistribution approval",
                       "B3 real installation: NO-GO", "no execution (not even --version)"):
            self.assertIn(needle, joined, needle)
        self.assertIn("were NOT available to the developer", b["reviewerPublished"]["note"])
        # the CI jobs are recorded as supporting evidence only
        self.assertEqual({j["name"]: (j["conclusion"], j["successfulSteps"]) for j in b["supportingCi"]["jobs"]}, {"windows-x64": ("success", 23), "macos-arm64": ("success", 23)})

    def test_the_developer_checks_record_what_could_actually_be_verified(self):
        ids = {c["id"] for c in self.b["developerChecks"]}
        self.assertEqual(ids, {"comment-retrieved", "final-tree", "candidate-tree", "recipe-digest", "ci-run", "fixture"})
        for t in (self.b["reviewerPublished"]["examinedFinalTree"], self.b["reviewerPublished"]["examinedCandidateTree"]):
            self.assertRegex(t, r"^[0-9a-f]{40}$")
        # the claimed trees are reproducible offline from this repository's own history
        final = subprocess.run(["git", "rev-parse", f"{ACCEPTED_SHA}^{{tree}}"], cwd=ROOT, capture_output=True, text=True)
        if final.returncode == 0:
            self.assertEqual(final.stdout.strip(), self.b["reviewerPublished"]["examinedFinalTree"])


class TheReleaseStaysClosed(unittest.TestCase):
    def test_the_production_lists_are_empty_and_nothing_names_jq(self):
        self.assertEqual(text(SRC / "recipe.rs").count('pub const BUILT_IN_RECIPES_JSON: &str = "[]";'), 1)
        self.assertIn('pub const BUILT_IN_EVIDENCE_JSON: &str = r#"{"attestations":[],"localUseLicenses":[]}"#;', text(SRC / "attestation.rs"))
        self.assertIn("pub const BUILT_IN_AUTHENTICATED: &[BuiltIn] = &[];", text(SRC / "discovery" / "reference.rs"))
        flow = production("install_flow.rs")
        self.assertIn("evidence: Evidence::production(),\n            pilot_gate: Vec::new(),", flow)
        for p in SRC.glob("*.rs"):
            self.assertNotIn(WIN_SHA, text(p), f"{p.name}: the jq digest is never a literal in Rust (the pilot reads it from the committed data files)")
            self.assertNotIn(WIN_SHA[:8], text(p), p.name)
        for p in (ROOT / "data").glob("*.json"):
            self.assertNotIn(WIN_SHA, text(p), p.name)
            self.assertNotIn('"jq"', text(p), f"{p.name}: the default catalog has no jq row; only the jq-pilot build seeds one")

    def test_the_pilot_unlock_is_one_constant_and_it_is_off(self):
        pilot = text(SRC / "jq_pilot.rs")
        code = "\n".join(l for l in pilot.splitlines() if not l.strip().startswith("//"))
        # None at B2; from the Phase 3D-5 activation candidate on: exactly the reviewed recipe digest (and the allowance stays inactive: see test_phase3d5_contract)
        self.assertRegex(code, r'pub const B3_UNLOCK: Option<&str> = (None|Some\("9c7a020d65d233e86577464a06c32b8f08fd4fd8403110e68fd551b674f7cca1"\));')
        self.assertEqual(code.count("pub const B3_UNLOCK:"), 1)
        self.assertEqual(sum(1 for l in code.splitlines() if "B3_UNLOCK" in l), 2, "declaration + the one use that builds the application's supply")
        for needle in ("std::env", "var_os", "std::process", "Command::new", "std::net", "TcpStream", "reqwest", "set_var", "winreg", "std::fs", "allow_real", "force_install"):
            self.assertNotIn(needle, code, needle)

    def test_the_pilot_feature_is_off_by_default_windows_only_and_never_in_the_release_build(self):
        cargo = text(ROOT / "src-tauri" / "Cargo.toml")
        self.assertIn("\njq-pilot = []\n", cargo)
        self.assertNotRegex(cargo, r"(?m)^default\s*=")
        lib = text(SRC / "lib.rs")
        self.assertIn('#[cfg(all(feature = "jq-pilot", not(windows)))]\ncompile_error!', lib)
        self.assertIn('#[cfg(all(feature = "jq-pilot", feature = "synthetic-pilot"))]\ncompile_error!', lib)
        ci = text(ROOT / ".github" / "workflows" / "ci.yml")
        for line in ci.splitlines():
            if "tauri build" in line:
                self.assertNotIn("pilot", line, "the release build never carries a pilot feature")
        self.assertIn("--features jq-pilot", ci)
        # the Windows-only configuration is exercised only on the Windows job; the macOS job proves the refusal
        win_step = ci[ci.index("jq-pilot feature configuration"):]
        self.assertIn("if: matrix.name == 'windows-x64'", win_step.split("- name:")[0])
        mac_step = ci[ci.index("jq-pilot feature is refused on macOS"):]
        self.assertIn("if: matrix.name == 'macos-arm64'", mac_step.split("- name:")[0])

    def test_the_pilot_is_wired_in_exactly_one_place_behind_the_feature(self):
        flow = production("install_flow.rs")
        self.assertEqual(flow.count("jq_pilot::sources_for_app"), 1)
        self.assertEqual(flow.count("jq_pilot::seed_catalog"), 1)
        self.assertIn('#[cfg(any(test, feature = "jq-pilot"))]\n#[path = "jq_pilot.rs"]\npub mod jq_pilot;', flow)
        for name in ("commands.rs", "state.rs", "lib.rs", "settings.rs", "main.rs"):
            body = text(SRC / name)
            self.assertNotIn("jq_pilot", body.replace("jq-pilot", ""), name)
            self.assertNotIn("RetainedFileProvider", body, name)
        # the acting adapter still exists only for tests and the two explicit pilot features, and production stays closed
        adapter = production("adapter.rs")
        self.assertIn('#[cfg(any(test, feature = "synthetic-pilot", feature = "jq-pilot"))]\n    pub fn mvp_synthetic', adapter)
        self.assertIn("Mode::Production => Err(TxnError::Refused(\"production_adapter_not_enabled\"))", adapter)
        self.assertEqual(adapter.count("mode: Mode::MvpSynthetic"), 1)
        self.assertIn('#[cfg(not(any(test, feature = "synthetic-pilot", feature = "jq-pilot")))]\n    fn adapter<', flow)
        self.assertIn('Err("production_adapter_not_enabled")', flow)

    def test_no_windows_hello_no_process_no_network_in_the_pilot_module(self):
        body = "\n".join(l for l in production("jq_pilot.rs").splitlines() if not l.strip().startswith("//"))
        for needle in ("KeyCredential", "os_hello", "os_approval", "ApprovalStore", "cng_verify", "device_trust"):
            self.assertNotIn(needle, body, needle)

    def test_no_jq_executable_is_tracked_in_the_repository(self):
        out = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True).stdout.splitlines()
        for rel in out:
            p = ROOT / rel
            if p.is_file() and p.stat().st_size == 1035264:
                self.assertNotEqual(sha(p.read_bytes()), WIN_SHA, f"{rel} IS the jq executable")


SELFCHECK = os.environ.get("ATM_SELFCHECK_JSON")


@unittest.skipUnless(SELFCHECK and Path(SELFCHECK).is_file(), "ATM_SELFCHECK_JSON not provided; Rust self-check output not available")
class TheBuiltBinaryCannotInstallAnything(unittest.TestCase):
    """CI runs the headless self-check of the DEFAULT release build; its own report says what that build could install."""

    def test_the_default_build_holds_no_recipe_reference_evidence_or_enablement_and_can_install_nothing(self):
        probe = json.loads(Path(SELFCHECK).read_text(encoding="utf-8"))["installSupplyProbe"]
        self.assertEqual(probe["pilotFeatures"], {"jqPilot": False, "syntheticPilot": False})
        for key in ("realRecipes", "authenticatedReferences", "enablementEntries", "evidenceRecords", "noticeBundles", "entriesThatCanBeInstalledOrRemoved"):
            self.assertEqual(probe[key], 0, key)
        self.assertGreaterEqual(probe["catalogEntries"], 13)
        self.assertIs(probe["everyEntryVerificationIncomplete"], True)
        self.assertNotIn("jq", json.dumps(probe).lower().replace("jqpilot", ""))


class StagingManifest(unittest.TestCase):
    def test_every_identifier_of_the_b2_manifest_exists_and_every_b2_test_is_listed(self):
        m = jl(D4 / "b2-staging-manifest.json")
        listed = [t for r in m["requirements"] for t in r["cargo"]]
        src = text(SRC / "install_flow_b2_tests.rs")
        defined = ["install_flow::tests::" + n for n in re.findall(r"#\[test\]\s*fn (b2_\w+)\(", src)]
        self.assertEqual(sorted(listed), sorted(defined))
        self.assertEqual(len(set(listed)), len(listed))
        self.assertIn("none of them is independent evidence by itself", m["purpose"])
        self.assertEqual(m["python"], ["tests.test_phase3d4_contract"])


class Documents(unittest.TestCase):
    def test_the_phase_document_states_the_gates_and_the_limits(self):
        t = text(ROOT / "docs" / "PHASE-3D4.md")
        for needle in ("B2", "9c7a020d65d233e86577464a06c32b8f08fd4fd8403110e68fd551b674f7cca1", "B3_UNLOCK", "jq-pilot", "Issue #14", "NOT independently run",
                       "not Windows identity authentication", "artifact.bin"):
            self.assertIn(needle, t, needle)

    def test_the_b3_request_is_a_turkish_proposal_and_not_an_authorization(self):
        t = text(D4 / "B3-AUTHORIZATION-REQUEST.tr.md")
        for needle in ("TASLAK", "henüz", "yetki VERİLMEDİ", "çalıştırılmadan", "PATH", "Issue #14"):
            self.assertIn(needle, t, needle)


if __name__ == "__main__":
    unittest.main()
