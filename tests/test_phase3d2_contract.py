"""Phase 3D-2 contract: the MVP installation workflow, the notices package and the gates that must not move.

Offline, reads repository files only. Nothing here downloads, installs or runs anything.
"""
import hashlib
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src-tauri" / "src"
D2 = ROOT / "docs" / "phase-3d2"
EVIDENCE = ROOT / "docs" / "phase-3d1" / "evidence" / "license"
WIN_SHA = "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627"


def text(path):
    return Path(path).read_text(encoding="utf-8").replace("\r\n", "\n")


def production(name):
    return text(SRC / name).split("#[cfg(test)]\n#[path")[0]


class NoticesPackage(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest = json.loads(text(D2 / "notices" / "jq-1.8.2-windows-x64.notices-manifest.json"))
        cls.bundle_bytes = (D2 / "notices" / cls.manifest["bundleFile"]).read_bytes()
        cls.bundle = cls.bundle_bytes.decode("utf-8")

    def test_the_bundle_matches_its_manifest_and_is_not_crlf(self):
        self.assertEqual(hashlib.sha256(self.bundle_bytes).hexdigest(), self.manifest["bundleSha256"])
        self.assertEqual(len(self.bundle_bytes), self.manifest["bundleBytes"])
        self.assertNotIn(b"\r\n", self.bundle_bytes)

    def test_every_component_text_is_present_verbatim_and_its_evidence_hash_holds(self):
        ids = {c["id"] for c in self.manifest["components"]}
        self.assertTrue({"jq", "oniguruma", "unicode", "mingw-w64", "mingw-w64-runtime-notices", "winpthreads", "gcc-runtime-exception"} <= ids)
        for c in self.manifest["components"]:
            path = ROOT / c["evidenceFile"]
            raw = path.read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(), c["evidenceSha256"], c["id"])
            body = raw.decode("utf-8").replace("\r\n", "\n").strip()
            self.assertIn(body, self.bundle, f"{c['id']}: the verbatim text is part of the bundle")
            self.assertIn(c["evidenceSha256"], self.bundle, f"{c['id']}: its section header names the hash")

    def test_the_decision_is_conditional_not_bundled_not_a_sidecar_and_the_open_points_are_listed(self):
        m = self.manifest
        self.assertEqual((m["licenseDecision"], m["recipeLicenseState"]), ("LICENSE_CONDITIONAL", "unknown"))
        self.assertTrue(m["notBundledWithInstaller"])
        self.assertFalse(m["writtenIntoToolFolder"])
        self.assertEqual({u["id"] for u in m["unresolved"]}, {"Q1", "Q2", "Q3", "C1-C5"})
        self.assertTrue(all(u["status"] == "open" for u in m["unresolved"]))
        self.assertIn("STATUS: PROPOSED", self.bundle)
        self.assertIn("NOT accepted", self.bundle)

    def test_the_draft_recipe_still_says_the_licence_is_unknown_and_nothing_activates_it(self):
        draft = json.loads(text(ROOT / "docs" / "phase-3d1" / "jq-1.8.2-windows-x64.recipe.draft.json"))
        self.assertEqual(draft["recipe"]["license"]["state"], "unknown")
        self.assertEqual(draft["review"]["state"], "draft")
        self.assertEqual(draft["recipe"]["artifact"]["sha256"], WIN_SHA)
        self.assertEqual(draft["recipe"]["artifact"]["sizeBytes"], 1035264)
        self.assertEqual(text(SRC / "recipe.rs").count('pub const BUILT_IN_RECIPES_JSON: &str = "[]";'), 1)
        for p in SRC.glob("*.rs"):
            self.assertNotIn(WIN_SHA, text(p), f"{p.name}: the jq digest is never a literal in Rust")
        for p in (ROOT / "data").glob("*.json"):
            self.assertNotIn(WIN_SHA, text(p), p.name)


class TheGatesThatMustNotMove(unittest.TestCase):
    def test_the_acting_adapter_exists_only_for_tests_and_the_pilot_feature(self):
        adapter = production("adapter.rs")
        self.assertIn('#[cfg(any(test, feature = "synthetic-pilot"))]\n    pub fn mvp_synthetic', adapter)
        self.assertEqual(adapter.count("mode: Mode::MvpSynthetic"), 1)
        self.assertEqual(adapter.count("mode: Mode::TestSynthetic"), 1)
        self.assertIn("#[cfg(test)]\n    pub fn test_mode", adapter)
        self.assertIn("production_adapter_not_enabled", adapter)
        flow = production("install_flow.rs")
        self.assertEqual(flow.count("Adapter::mvp_synthetic"), 1)
        self.assertIn('#[cfg(not(any(test, feature = "synthetic-pilot")))]\n    fn adapter<', flow)
        self.assertIn('Err("production_adapter_not_enabled")', flow)
        self.assertIn('#[cfg(any(test, feature = "synthetic-pilot"))]\n#[path = "pilot_supply.rs"]', flow)

    def test_the_feature_is_off_by_default_and_release_ci_never_enables_it(self):
        cargo = text(ROOT / "src-tauri" / "Cargo.toml")
        self.assertIn("synthetic-pilot = []", cargo)
        self.assertNotRegex(cargo, r"(?m)^default\s*=")
        ci = text(ROOT / ".github" / "workflows" / "ci.yml")
        for line in ci.splitlines():
            if "tauri build" in line:
                self.assertNotIn("synthetic-pilot", line, "the release build must not carry the synthetic supply")

    def test_every_recipe_must_be_synthetic_and_the_production_policy_accepts_no_actor(self):
        self.assertIn('!r.synthetic || !r.tool_id.starts_with("synthetic-")', text(SRC / "lifecycle.rs"))
        approval = production("approval.rs")
        self.assertIn("ApprovalPolicy { allow_synthetic: false, allow_in_app: false }", approval)

    def test_windows_hello_is_not_part_of_the_workflow(self):
        for name in ["install_flow.rs", "consent.rs", "pilot_supply.rs", "commands.rs", "state.rs", "lib.rs"]:
            body = "\n".join(l for l in production(name).splitlines() if not l.strip().startswith(("//", "mod ")))
            for needle in ["KeyCredential", "WindowsHello", "os_hello", "os_approval", "ApprovalStore", "approval_store", "cng_verify", "device_trust"]:
                self.assertNotIn(needle, body, f"{needle} in {name}")

    def test_the_command_allow_list_is_identical_in_build_capabilities_and_health(self):
        build = text(ROOT / "src-tauri" / "build.rs")
        commands = re.findall(r'"([a-z_]+)",', build.split("COMMANDS")[1].split("];")[0])
        caps = json.loads(text(ROOT / "src-tauri" / "capabilities" / "default.json"))["permissions"]
        self.assertEqual(sorted("allow-" + c.replace("_", "-") for c in commands), sorted(caps))
        health = text(SRC / "health.rs")
        for c in caps:
            self.assertIn(f'"{c}"', health)
        new = {"list_install_states", "prepare_install", "cancel_install", "confirm_install", "get_installed_details", "recover_installations"}
        self.assertTrue(new <= set(commands))
        # none of the installation commands can take a path, a URL, a command line or bytes (the older offline `preview_github_url` only
        # validates link text and fetches nothing, so it is not part of this check)
        for c in new:
            for bad in ("path", "url", "command", "bytes", "script"):
                self.assertNotIn(bad, c)
        install_section = text(SRC / "commands.rs").split("pub fn list_install_states")[1].split("pub fn list_inventory")[0]
        for line in install_section.splitlines():
            if "pub fn " in line or "pub async fn " in line:
                for bad in ("path:", "url:", "command:", "bytes:", "sha256:", "script:"):
                    self.assertNotIn(bad, line)


class Documents(unittest.TestCase):
    NAMES = [
        "MVP-CONSENT-MODEL.md", "NOTICES-DECISION.md", "JQ-PILOT-TRANSITION.md", "ISSUE-14-LIMITED-PILOT-ASSESSMENT.md",
        "GUARD-TEST-CHANGES.md", "GUI-EVIDENCE.md",
    ]

    def test_documents_exist_and_their_links_resolve(self):
        link = re.compile(r"\]\(([^)#]+)(?:#[^)]*)?\)")
        files = [ROOT / "docs" / "PHASE-3D2.md"] + [D2 / n for n in self.NAMES]
        for f in files:
            self.assertTrue(f.exists(), f)
            for target in link.findall(text(f)):
                if target.startswith(("http://", "https://", "mailto:")):
                    continue
                self.assertTrue((f.parent / target).resolve().exists(), f"{f.name}: {target}")

    def test_the_consent_document_never_calls_the_confirmation_an_authentication(self):
        body = text(D2 / "MVP-CONSENT-MODEL.md")
        self.assertIn("not authentication", body.lower())
        self.assertIn("same Windows account", body)
        self.assertNotRegex(body, r"(?i)proves? (that )?(a )?(person|user) was present")

    def test_issue_14_stays_open_in_every_document_that_mentions_it(self):
        body = text(D2 / "ISSUE-14-LIMITED-PILOT-ASSESSMENT.md")
        self.assertIn("Issue #14 stays open", body)
        self.assertNotRegex(body, r"(?i)issue #14 (is )?(closed|resolved|satisfied)")
        self.assertIn("establishes nothing about other computers", body)

    def test_the_transition_document_lists_five_blockers_including_the_source_attestation(self):
        body = text(D2 / "JQ-PILOT-TRANSITION.md")
        for code in ["artifact_not_authenticated", "license_not_accepted", "not_reviewed", "staging_evidence", "source_origin_unverified"]:
            self.assertIn(code, body)

    def test_the_gui_driver_and_wrapper_are_in_the_repository(self):
        self.assertTrue((ROOT / "tools" / "gui_install_flow.mjs").exists())
        ps = text(ROOT / "tools" / "gui_install_flow.ps1")
        self.assertIn("ATM_DATA_DIR", ps)
        self.assertNotIn("Set-MpPreference", ps)
        self.assertNotIn("Add-MpPreference", ps)


if __name__ == "__main__":
    unittest.main()
