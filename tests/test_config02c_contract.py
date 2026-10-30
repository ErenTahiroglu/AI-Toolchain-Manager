"""CONFIG-02C (Issue #40) contract, no running app needed. A DESIGN phase for one user-authorised read of a project `.mcp.json`:

* the three documents exist, keep READ_NOT_GRANTED, and use ONE closed status vocabulary,
* the mapping table covers every FsError code of the handle-relative layer,
* the only production source files that name `.mcp.json` are the ones found by the design inspection (so a new reader cannot be added silently),
* nothing about the application's reach changed (no new command, permission, dependency, plugin, migration or frontend import),
* the owner's response block is still blank.

This is a text/structure guard. It cannot show any behaviour of a reader: none exists. Whoever implements the read (after an explicit owner decision) must
update this file deliberately, which is the point.
"""
import json
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
SRC = ROOT / "src-tauri" / "src"
DOCS = ROOT / "docs" / "config-02c"

CONTRACT = "READ_ONLY_CONTRACT.md"
PLAN = "THREAT_AND_PRIVACY_TEST_PLAN.md"
REQUEST = "REAL_READ_AUTHORIZATION_REQUEST.md"

# The closed list the backend may emit. `not_requested` is a UI-only state and is deliberately not in it.
STATUSES = [
    "root_not_found", "absent", "present_summarized", "present_metadata_only", "malformed", "denied",
    "unsafe_path", "unsupported_fs", "too_large", "changed_during_read", "io_error",
]


def read(rel):
    return (ROOT / rel).read_text(encoding="utf-8")


def doc(name):
    return (DOCS / name).read_text(encoding="utf-8")


class Documents(unittest.TestCase):
    def test_all_three_exist_and_are_not_a_grant(self):
        for name in (CONTRACT, PLAN, REQUEST):
            text = doc(name)
            self.assertIn("READ_NOT_GRANTED", text, name)
            self.assertNotIn("READ_GRANTED", text, name)
            self.assertNotIn("CONFIG_02C_IMPLEMENTED", text, name)
        self.assertIn("CONFIG_02C_DESIGN_READY_FOR_INDEPENDENT_REVIEW", doc(CONTRACT))

    def test_the_owner_response_block_is_blank(self):
        text = doc(REQUEST)
        self.assertIsNone(re.search(r"\[[xX]\]", text), "a box in the owner response is ticked: that is the owner's act, in the Issue and in this file")
        self.assertIn("Status: READ_NOT_GRANTED", text)

    def test_one_closed_status_vocabulary(self):
        contract = doc(CONTRACT)
        start = contract.index("### 3.3")
        end = contract.index("The mapping from the existing `FsError`")
        rows = re.findall(r"^\| `([a-z_]+)` \|", contract[start:end], re.M)
        self.assertEqual(rows, STATUSES)
        self.assertNotIn("`not_requested`", contract[start:end].split("| Status |")[1], "not_requested is UI-only and must not be a backend row")
        request = doc(REQUEST)
        for s in STATUSES:
            self.assertIn(f"`{s}`", request, s)
        self.assertIn("closed list of 11", request)
        # no invented status in the test plan: every backticked status-looking token it uses is in the list
        plan = doc(PLAN)
        for token in re.findall(r"`(root_not_found|absent|present_\w+|malformed|denied|unsafe_path|unsupported_fs|too_large|changed_during_read|io_error)`", plan):
            self.assertIn(token, STATUSES, token)

    def test_every_fs_error_code_has_a_row_in_the_mapping(self):
        codes = re.findall(r'FsError::\w+ => "([a-z_]+)"', read("src-tauri/src/managed_fs.rs"))
        self.assertGreaterEqual(len(codes), 15)
        contract = doc(CONTRACT)
        for code in codes:
            self.assertTrue(
                re.search(r"^\| `%s` \|" % re.escape(code), contract, re.M), f"FsError code {code!r} has no row in the contract's mapping table"
            )

    def test_the_skip_visibility_condition_is_stated(self):
        plan = doc(PLAN)
        self.assertIn("CONFIG02C_SKIP", plan)
        self.assertIn("CONFIG02C_RESULTS", plan)
        self.assertIn("BLOCKED", plan)

    def test_the_turkish_gate_text_names_the_file_the_limit_and_the_no_button(self):
        request = doc(REQUEST)
        for needle in (".mcp.json", "1.048.576", "Hayır, metni yapıştırarak devam edeceğim", "Temizle", "kaydedilmez", "İnternete bağlanılmaz"):
            self.assertIn(needle, request, needle)


class NothingNewIsReachable(unittest.TestCase):
    def test_only_the_known_production_files_name_the_target(self):
        found = set()
        for path in SRC.rglob("*.rs"):
            if "tests" in path.name:
                continue
            if ".mcp.json" in path.read_text(encoding="utf-8"):
                found.add(path.relative_to(SRC).as_posix())
        # config_txn.rs and managed_fs_mcp.rs are compiled for tests only (checked by test_config02b_contract); discovery/* is the existing scan;
        # discovery/store.rs only names the file inside a Turkish sentence.
        self.assertEqual(
            found,
            {"config_txn.rs", "managed_fs_mcp.rs", "discovery/detect.rs", "discovery/store.rs", "discovery_probe.rs"},
            "a new production file now names .mcp.json: that is a new reader and needs the owner decision of docs/config-02c/REAL_READ_AUTHORIZATION_REQUEST.md",
        )

    def test_no_read_command_exists(self):
        for path in SRC.rglob("*.rs"):
            self.assertNotIn("read_project_mcp", path.read_text(encoding="utf-8"), str(path))
        self.assertEqual(read("src-tauri/src/commands.rs").count("#[tauri::command]"), 36)
        for p in json.loads(read("src-tauri/capabilities/default.json"))["permissions"]:
            self.assertNotIn("mcp", p, p)
            self.assertIsNone(re.search(r"fs|shell|http|opener|dialog|clipboard|process|os:|path", p, re.I), p)

    def test_no_dialog_or_fs_plugin_dependency_and_no_new_migration(self):
        forbidden = re.compile(r"plugin-dialog|plugin-fs|plugin_dialog|plugin_fs|\brfd\b", re.I)
        for rel in ("src-tauri/Cargo.toml", "package.json"):
            self.assertIsNone(forbidden.search(read(rel)), rel)
        for path in (ROOT / "src").rglob("*.ts*"):
            self.assertIsNone(forbidden.search(path.read_text(encoding="utf-8")), str(path))
        migrations = sorted(p.name for p in (ROOT / "src-tauri" / "migrations").glob("*.sql"))
        self.assertEqual((len(migrations), migrations[-1]), (10, "0010_codex_cli_surface.sql"))
        cargo = read("src-tauri/Cargo.toml")
        deps = re.search(r"\[dependencies\]\r?\n(.*?)\r?\n\r?\n", cargo, re.S).group(1)
        self.assertEqual(
            sorted(re.findall(r"^([a-z0-9_-]+) =", deps, re.M)),
            sorted(["tauri", "tauri-plugin-single-instance", "tauri-plugin-autostart", "rusqlite", "serde", "serde_json", "sha2", "uuid", "chrono", "regex"]),
        )

    def test_the_existing_path_accepting_command_is_still_the_only_one(self):
        # Finding F2: exactly one command takes a project directory, and it is the existing discovery scan.
        taking_dirs = [p.relative_to(SRC).as_posix() for p in SRC.rglob("*.rs") if "tests" not in p.name and re.search(r"pub directory: String", p.read_text(encoding="utf-8"))]
        self.assertEqual(taking_dirs, ["discovery/mod.rs"])


if __name__ == "__main__":
    unittest.main()
