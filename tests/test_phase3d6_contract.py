"""Phase 3D-6 contract: the one-installation allowance is ARMED for the ONE data directory the user authorized (a digest, never a path).

Offline, repository files only. Nothing here downloads, installs, opens or runs jq, and nothing here names the directory: the repository carries only
the digest, which is a scope identifier (not a secret, not an authentication credential).
"""
import os
import re
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src-tauri" / "src"
D6 = ROOT / "docs" / "phase-3d6"
sys.path.insert(0, str(ROOT / "tools"))
import pilot_data_dir_digest  # noqa: E402

RECIPE_DIGEST = "9c7a020d65d233e86577464a06c32b8f08fd4fd8403110e68fd551b674f7cca1"
ARMED_DIR_DIGEST = "158c279e2151088c91783a5d788cc5877cc417cb23e0bff69dbeade99a7ee8a5"
WIN_SHA = "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627"


def text(p):
    return Path(p).read_text(encoding="utf-8").replace("\r\n", "\n")


def code_only(name):
    return "\n".join(l for l in text(SRC / name).split("#[cfg(test)]\n#[path")[0].splitlines() if not l.strip().startswith("//"))


class TheArming(unittest.TestCase):
    def test_exactly_the_two_constants_have_their_final_values(self):
        pilot = code_only("jq_pilot.rs")
        self.assertIn(f'pub const B3_UNLOCK: Option<&str> = Some("{RECIPE_DIGEST}");', pilot, "the unlock names the reviewed recipe digest, unchanged")
        self.assertIn(f'pub const B3_ARMED_DATA_DIR_SHA256: Option<&str> = Some("{ARMED_DIR_DIGEST}");', pilot, "armed for the ONE authorized directory")
        self.assertEqual(sum(1 for l in pilot.splitlines() if "B3_UNLOCK" in l), 2)
        self.assertEqual(sum(1 for l in pilot.splitlines() if "B3_ARMED_DATA_DIR_SHA256" in l), 2, "declaration + the one use in sources_for_app")

    def test_the_digest_is_the_python_tool_s_for_a_path_and_the_normalisation_is_the_documented_one(self):
        # a made-up path (the real one is never committed): separators, case and a trailing separator do not matter; the prefix does
        base = pilot_data_dir_digest.data_dir_digest(r"c:\somebody\pilot-folder")
        self.assertEqual(pilot_data_dir_digest.data_dir_digest("C:/Somebody/Pilot-Folder/"), base)
        self.assertNotEqual(pilot_data_dir_digest.data_dir_digest(r"c:\somebody\pilot-folder2"), base)
        rust = code_only("pilot_allowance.rs")
        self.assertIn(r""".replace('/', "\\").to_lowercase()""", rust)
        self.assertIn("trim_end_matches('" + "\\" * 2 + "')", rust)
        self.assertIn("atm-pilot-data-dir-v1|", rust)
        # LOCAL check: the directory the user named, supplied by the person running the test; the repository does not know it
        named = os.environ.get("ATM_B3_DIR_CHECK")
        if named:
            self.assertEqual(pilot_data_dir_digest.data_dir_digest(named), ARMED_DIR_DIGEST)

    def test_no_file_of_the_repository_contains_the_authorized_directory_in_plaintext(self):
        # assembled here so that this file does not contain what it searches for
        leaf = "AI-Toolchain-Manager-" + "jq-" + "pilot"
        hits = []
        for p in subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True).stdout.splitlines():
            f = ROOT / p
            if f.is_file() and f.stat().st_size < 4_000_000:
                try:
                    body = f.read_text(encoding="utf-8").lower()
                except UnicodeDecodeError:
                    continue
                if leaf.lower() in body:
                    hits.append(p)
        self.assertEqual(hits, [])

    def test_the_armed_pilot_is_still_one_installation_bound_to_the_reviewed_files_and_no_artifact_digest_is_a_rust_literal(self):
        for p in SRC.glob("*.rs"):
            self.assertNotIn(WIN_SHA, text(p), p.name)
            self.assertNotIn(WIN_SHA[:8], text(p), p.name)
        allowance = code_only("pilot_allowance.rs")
        self.assertEqual(allowance.count("INSERT INTO settings"), 1)
        self.assertEqual(allowance.count("UPDATE settings"), 1)
        self.assertNotIn("DELETE", allowance)

    def test_the_pilot_feature_stays_non_default_windows_only_and_out_of_every_release_build(self):
        cargo = text(ROOT / "src-tauri" / "Cargo.toml")
        self.assertRegex(cargo, r"(?m)^jq-pilot = \[\]$")
        self.assertNotRegex(cargo, r"(?m)^default\s*=")
        lib = text(SRC / "lib.rs")
        self.assertIn('#[cfg(all(feature = "jq-pilot", not(windows)))]\ncompile_error!', lib)
        self.assertIn('#[cfg(all(feature = "jq-pilot", feature = "synthetic-pilot"))]\ncompile_error!', lib)
        ci = text(ROOT / ".github" / "workflows" / "ci.yml")
        for line in ci.splitlines():
            if "tauri build" in line:
                self.assertNotIn("pilot", line)
        flow = text(SRC / "install_flow.rs").split("#[cfg(test)]\n#[path")[0]
        self.assertIn("one_time: None,", flow, "the release supply carries no one-time rule")

    def test_the_manifest_lists_the_arming_tests(self):
        import json

        m = json.loads(text(ROOT / "docs" / "phase-3d5" / "b3-staging-manifest.json"))
        ids = {r["id"]: r["cargo"] for r in m["requirements"]}
        self.assertEqual(len(ids["B3-8"]), 3)
        for name in ids["B3-8"]:
            self.assertIn(name.split("::")[-1], text(SRC / "install_flow_b3_tests.rs"))


class TheDocuments(unittest.TestCase):
    def test_the_phase_document_states_the_decision_the_limits_and_that_nothing_was_installed(self):
        t = text(ROOT / "docs" / "PHASE-3D6.md")
        for needle in ("Issue #24", "Issue #14", ARMED_DIR_DIGEST, "NOT installed", "NOT executed", "not been opened", "final independent", "scope identifier",
                       "not a secret", "NOT independently run", "needs_attention"):
            self.assertIn(needle, t, needle)

    def test_the_remaining_steps_are_preparation_and_not_permission(self):
        r = text(D6 / "REMAINING-STEPS.tr.md")
        for needle in ("izin DEĞİLDİR", "son bağımsız Red Team", "onay düğmesine yalnızca siz basarsınız", "jq-pilot-input", "çalıştırılmaz", "otomatik temizlik"):
            self.assertIn(needle, r, needle)


if __name__ == "__main__":
    unittest.main()
