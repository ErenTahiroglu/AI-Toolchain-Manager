"""Phase 3C: independent (non-Rust) checks of the isolated production-oriented filesystem layer and the test-mode adapter.

Nothing here runs a Rust binary, downloads or executes any tool. The tests read the repository's source and migration files and
apply the managed-roots migration to a throw-away in-memory SQLite database, so that the claims in docs/PHASE-3C.md are also
checked by something other than the code under test.
"""

import pathlib
import re
import sqlite3
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
SRC = ROOT / "src-tauri" / "src"
NEW_MODULES = ["managed_fs.rs", "managed_fs_windows.rs", "managed_fs_unix.rs", "managed_root.rs", "device_trust.rs", "adapter.rs"]


def production_text(name):
    text = (SRC / name).read_text(encoding="utf-8")
    # cut off the test module declaration and everything after it
    return re.split(r"#\[cfg\(test\)\]\n(?:#\[path|mod )", text)[0]


class SourceInvariants(unittest.TestCase):
    def test_the_new_modules_have_no_process_network_environment_or_recursive_delete_capability(self):
        needles = ["std::process", "Command::new", "std::net", "TcpStream", "reqwest", "set_var", "remove_var", "set_current_dir", "remove_dir_all"]
        for name in NEW_MODULES:
            text = production_text(name)
            for needle in needles:
                self.assertNotIn(needle, text, f"{needle} in {name}")

    def test_only_the_filesystem_layer_touches_the_filesystem_and_never_by_path_mutation(self):
        for name in ["managed_root.rs", "device_trust.rs", "adapter.rs"]:
            text = production_text(name)
            for needle in ["std::fs", "fs::remove", "fs::write", "fs::rename", "fs::create_dir", "fs::copy"]:
                self.assertNotIn(needle, text, f"{needle} in {name}")
        for name in ["managed_fs.rs", "managed_fs_windows.rs", "managed_fs_unix.rs"]:
            text = production_text(name)
            for needle in ["fs::remove_file", "fs::remove_dir", "fs::write", "fs::create_dir", "fs::rename", "fs::copy", "fs::hard_link"]:
                self.assertNotIn(needle, text, f"{needle} in {name}")

    def test_no_command_or_start_up_code_can_name_a_root_or_reach_the_adapter(self):
        for name in ["commands.rs", "state.rs"]:
            text = (SRC / name).read_text(encoding="utf-8")
            for needle in ["managed_root", "managed_fs", "AppDataAnchor", "ManagedRoot", "adapter::", "device_trust"]:
                self.assertNotIn(needle, text, f"{needle} in {name}")
        lib = (SRC / "lib.rs").read_text(encoding="utf-8")
        for line in lib.splitlines():
            stripped = line.strip()
            if stripped.startswith("mod ") or stripped.startswith("//"):
                continue
            for needle in ["managed_root", "managed_fs", "AppDataAnchor", "adapter::", "device_trust"]:
                self.assertNotIn(needle, line, f"lib.rs uses {needle}")

    def test_the_acting_adapter_constructor_exists_in_test_builds_only_and_production_is_closed(self):
        text = production_text("adapter.rs")
        self.assertEqual(text.count("mode: Mode::TestSynthetic"), 1)
        self.assertIn("#[cfg(test)]\n    pub fn test_mode", text)
        self.assertIn("production_adapter_not_enabled", text)

    def test_production_lists_stay_empty_and_no_os_key_store_exists(self):
        recipe = (SRC / "recipe.rs").read_text(encoding="utf-8")
        self.assertIn('pub const BUILT_IN_RECIPES_JSON: &str = "[]";', recipe)
        trust = production_text("device_trust.rs")
        self.assertEqual(len(re.findall(r"impl KeyStore for", trust)), 1)
        self.assertIn("impl KeyStore for FakeKeyStore", trust)
        self.assertIn("SoftwareFake", trust)

    def test_the_windows_layer_opens_everything_relative_to_a_handle_without_following_reparse_points(self):
        text = production_text("managed_fs_windows.rs")
        self.assertIn("FILE_OPEN_REPARSE_POINT", text)
        self.assertGreaterEqual(text.count("FILE_OPEN_REPARSE_POINT"), 4)
        self.assertIn("FILE_CREATE", text)
        self.assertIn("FILE_DISPOSITION_INFORMATION_EX", text)

    def test_the_unix_layer_never_uses_path_based_calls(self):
        text = production_text("managed_fs_unix.rs")
        for needle in ["libc::open(", "libc::unlink(", "libc::rename(", "libc::mkdir(", "libc::stat(", "libc::lstat("]:
            if needle == "libc::open(":
                # the single permitted path-based open is the volume root "/" that starts the descriptor-relative walk
                self.assertEqual(text.count(needle), 1)
            else:
                self.assertNotIn(needle, text)
        for needle in ["openat", "mkdirat", "unlinkat", "renameat", "fstatat", "O_NOFOLLOW"]:
            self.assertIn(needle, text)


class ManagedRootsMigration(unittest.TestCase):
    SQL = (ROOT / "src-tauri" / "migrations" / "0009_managed_roots.sql").read_text(encoding="utf-8")

    def setUp(self):
        self.db = sqlite3.connect(":memory:")
        self.db.executescript(self.SQL)

    def insert(self, root_id, computer="c", platform="windows", state="creating", volume=None, obj=None, activated=None):
        self.db.execute(
            "INSERT INTO managed_roots (root_id, computer_id, platform, volume_id, object_id, state, created_at, activated_at) VALUES (?,?,?,?,?,?,?,?)",
            (root_id, computer, platform, volume, obj, state, "now", activated),
        )

    def test_an_active_root_needs_its_recorded_identity(self):
        with self.assertRaises(sqlite3.IntegrityError):
            self.insert("a" * 32, state="active")
        self.insert("a" * 32, state="active", volume="1", obj="2", activated="now")

    def test_one_live_root_per_computer_and_retired_roots_do_not_count(self):
        self.insert("a" * 32)
        with self.assertRaises(sqlite3.IntegrityError):
            self.insert("b" * 32)
        self.db.execute("UPDATE managed_roots SET state='retired' WHERE root_id=?", ("a" * 32,))
        self.insert("b" * 32)
        self.insert("c" * 32, computer="other")

    def test_malformed_values_are_rejected(self):
        for kwargs in [
            {"root_id": "short"},
            {"root_id": "G" * 32},
            {"root_id": "d" * 32, "platform": "linux"},
            {"root_id": "d" * 32, "volume": "1x", "obj": "2"},
            {"root_id": "d" * 32, "state": "weird"},
            {"root_id": "d" * 32, "computer": ""},
        ]:
            with self.assertRaises(sqlite3.IntegrityError, msg=str(kwargs)):
                self.insert(**kwargs)


class Documents(unittest.TestCase):
    def test_the_phase_documents_exist_and_state_the_non_claims(self):
        phase = (ROOT / "docs" / "PHASE-3C.md").read_text(encoding="utf-8")
        for needle in ["Issue #14", "Issue #15", "ambiguous orphan", "No real tool was downloaded", "authenticated-reference set are empty"]:
            self.assertIn(needle, phase)
        guarantees = (ROOT / "docs" / "phase-3c" / "FILESYSTEM-GUARANTEES.md").read_text(encoding="utf-8")
        for needle in ["[simulated]", "[CI-mac]", "irreducible window", "Same-user malware"]:
            self.assertIn(needle, guarantees)
        jq = (ROOT / "docs" / "phase-3c" / "JQ-REVIEW-PACKAGE.md").read_text(encoding="utf-8")
        self.assertIn("declared, not verified", jq)
        self.assertGreaterEqual(jq.count("**not done**"), 10)
        self.assertIn("nothing was added", jq)


if __name__ == "__main__":
    unittest.main()
