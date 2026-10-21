"""Phase 3D-5 contract: the one-installation allowance (Issue #24) and the B3 activation candidate.

Offline, repository files only. Nothing here downloads, installs, opens or runs jq.
"""
import hashlib
import json
import os
import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src-tauri" / "src"
D5 = ROOT / "docs" / "phase-3d5"
sys.path.insert(0, str(ROOT / "tools"))
import pilot_data_dir_digest  # noqa: E402

WIN_SHA = "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627"
RECIPE_DIGEST = "9c7a020d65d233e86577464a06c32b8f08fd4fd8403110e68fd551b674f7cca1"
# Phase 3D-6: the scope identifier (digest of the normalised path text) of the ONE isolated data directory the user authorized; the path is never committed
ARMED_DIR_DIGEST = "158c279e2151088c91783a5d788cc5877cc417cb23e0bff69dbeade99a7ee8a5"


def text(p):
    return Path(p).read_text(encoding="utf-8").replace("\r\n", "\n")


def code_only(name):
    return "\n".join(l for l in text(SRC / name).split("#[cfg(test)]\n#[path")[0].splitlines() if not l.strip().startswith("//"))


class TheAllowance(unittest.TestCase):
    def test_the_issue_the_disposition_answers_is_captured_verbatim(self):
        t = text(D5 / "evidence" / "issue-24-body.md")
        for needle in ("persistent feature unlock is not a one-time jq installation", "one exact artifact/recipe, one isolated pilot data directory and one successful install",
                       "must not automatically reopen installation", "Never rely solely on the compile-time `Some` gate or a one-use preview nonce"):
            self.assertIn(needle, t, needle)

    def test_the_module_is_a_small_state_machine_with_no_reset_and_no_credential_framework(self):
        body = code_only("pilot_allowance.rs")
        for needle in ("DELETE", "DROP", "std::fs", "std::env", "std::process", "std::net", "KeyCredential", "os_hello", "device_trust", "approval_store", "TPM", "biometric"):
            self.assertNotIn(needle, body, needle)
        self.assertEqual(body.count("INSERT INTO settings"), 1)
        self.assertEqual(body.count("UPDATE settings"), 1)
        for state in ('pub const RESERVED: &str = "reserved";', 'pub const CONSUMED: &str = "consumed";', 'pub const RELEASED: &str = "released";'):
            self.assertIn(state, body)
        # consumed is terminal: the only function that writes `released` or `reserved` refuses to start from it
        self.assertIn('if r.state != RESERVED || r.approval_id != approval_id {', body)
        self.assertIn("if r.state != RELEASED {", body)

    def test_it_is_wired_through_the_install_service_only_and_never_reachable_from_the_interface(self):
        flow = text(SRC / "install_flow.rs").split("#[cfg(test)]\n#[path")[0]
        for needle in ("pilot_allowance::reserve", "pilot_allowance::consume", "pilot_allowance::release", "pilot_allowance::evaluate", "fn reconcile_allowance"):
            self.assertIn(needle, flow, needle)
        # the reservation sits after the mandatory approval row and the opened folder, before the transaction
        apply = flow[flow.index("fn apply("):]
        self.assertLess(apply.index("pilot_allowance::reserve"), apply.index("adapter.install_with_consent"))
        self.assertLess(apply.index("let adapter = match self.adapter("), apply.index("pilot_allowance::reserve"))
        for name in ("commands.rs", "state.rs", "lib.rs", "settings.rs", "main.rs"):
            self.assertNotIn("pilot_allowance", text(SRC / name).replace("mod pilot_allowance;", ""), name)
        self.assertIn("one_time: None,", flow, "the release supply carries no one-time rule")

    def test_the_data_directory_digest_is_the_same_in_rust_and_python(self):
        self.assertEqual(pilot_data_dir_digest.data_dir_digest(r"c:\isolated\pilot-data"), "0cd7d19471ddccb4d25f06b5a8e55eaa17a4008163999a437d9141b515531e5e")
        self.assertEqual(pilot_data_dir_digest.data_dir_digest("C:/Isolated/Pilot-Data/"), pilot_data_dir_digest.data_dir_digest(r"c:\isolated\pilot-data"))
        self.assertIn("0cd7d19471ddccb4d25f06b5a8e55eaa17a4008163999a437d9141b515531e5e", text(SRC / "pilot_allowance_tests.rs"))
        rust = code_only("pilot_allowance.rs")
        self.assertIn(r""".replace('/', "\\").to_lowercase()""", rust)
        self.assertIn("trim_end_matches('" + "\\" * 2 + "')", rust)
        self.assertIn("atm-pilot-data-dir-v1|", rust)


class TheCandidateStaysInactive(unittest.TestCase):
    def test_the_unlock_is_none_or_exactly_the_reviewed_recipe_digest_and_the_arming_is_none(self):
        pilot = code_only("jq_pilot.rs")
        unlock = re.search(r"pub const B3_UNLOCK: Option<&str> = (None|Some\(\"([0-9a-f]{64})\"\));", pilot)
        self.assertIsNotNone(unlock)
        self.assertIn(unlock.group(2), (None, RECIPE_DIGEST), "the unlock may only ever name the reviewed recipe digest")
        armed = re.search(r"pub const B3_ARMED_DATA_DIR_SHA256: Option<&str> = (None|Some\(\"([0-9a-f]{64})\"\));", pilot)
        self.assertIsNotNone(armed, "exactly one declaration, None (inactive) or the digest of the one authorized directory")
        self.assertIn(armed.group(2), (None, ARMED_DIR_DIGEST), "the allowance may only ever be armed for the one authorized directory")
        self.assertEqual(sum(1 for l in pilot.splitlines() if "B3_UNLOCK" in l), 2)
        self.assertEqual(sum(1 for l in pilot.splitlines() if "B3_ARMED_DATA_DIR_SHA256" in l), 2)

    def test_the_default_release_and_ci_never_enable_the_pilot_and_no_jq_byte_is_in_the_tree(self):
        ci = text(ROOT / ".github" / "workflows" / "ci.yml")
        for line in ci.splitlines():
            if "tauri build" in line:
                self.assertNotIn("pilot", line)
        self.assertIn("docs/phase-3d5/b3-staging-manifest.json", ci)
        for p in SRC.glob("*.rs"):
            self.assertNotIn(WIN_SHA, text(p), p.name)
            self.assertNotIn(WIN_SHA[:8], text(p), p.name)
        cargo = text(ROOT / "src-tauri" / "Cargo.toml")
        self.assertNotRegex(cargo, r"(?m)^default\s*=")

    def test_the_manifest_lists_every_b3_identifier_once(self):
        m = json.loads(text(D5 / "b3-staging-manifest.json"))
        listed = [t for r in m["requirements"] for t in r["cargo"]]
        defined = ["install_flow::tests::" + n for n in re.findall(r"#\[test\]\s*fn (b3_\w+)\(", text(SRC / "install_flow_b3_tests.rs"))]
        defined += ["pilot_allowance::tests::" + n for n in re.findall(r"#\[test\]\s*fn (\w+)\(", text(SRC / "pilot_allowance_tests.rs"))]
        self.assertEqual(sorted(listed), sorted(defined))
        self.assertEqual(len(set(listed)), len(listed))


SELFCHECK = os.environ.get("ATM_SELFCHECK_JSON")


@unittest.skipUnless(SELFCHECK and Path(SELFCHECK).is_file(), "ATM_SELFCHECK_JSON not provided; Rust self-check output not available")
class TheBuiltBinaryCarriesNoOneTimeRule(unittest.TestCase):
    def test_the_default_build_has_no_allowance_at_all(self):
        probe = json.loads(Path(SELFCHECK).read_text(encoding="utf-8"))["installSupplyProbe"]
        self.assertEqual(probe["oneTimeAllowances"], 0)
        self.assertIs(probe["oneTimeAllowanceArmed"], False)
        self.assertEqual(probe["entriesThatCanBeInstalledOrRemoved"], 0)


class Documents(unittest.TestCase):
    def test_the_phase_document_the_checklist_and_the_request_state_the_limits(self):
        t = text(ROOT / "docs" / "PHASE-3D5.md")
        for needle in ("Issue #24", "ONE", "consumed", "B3_UNLOCK", "B3_ARMED_DATA_DIR_SHA256", "inactive", "Issue #14", "NOT independently run", "needs_attention"):
            self.assertIn(needle, t, needle)
        c = text(D5 / "PILOT-CHECKLIST.tr.md")
        for needle in ("HAZIRLIK", "izin DEĞİLDİR", "Envanter 0", "jq-pilot-input", "SHA-256", "Smart App Control", "otomatik temizlik", "kimlik doğrulaması DEĞİLDİR"):
            self.assertIn(needle, c, needle)
        r = text(D5 / "B3-AUTHORIZATION-REQUEST.tr.md")
        for needle in ("TASLAK", "yetki VERİLMEDİ", "BİR KEZ", "çalıştırılmadan", "tek seferlik", "yeniden kurulum"):
            self.assertIn(needle, r, needle)


if __name__ == "__main__":
    unittest.main()
