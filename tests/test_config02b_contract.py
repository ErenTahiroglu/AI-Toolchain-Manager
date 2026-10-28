"""CONFIG-02B (Issue #39) contract, no running app needed. A test-only prototype of a one-file transaction on synthetic directories:

* the prototype is compiled for tests only and is referenced from nowhere else,
* nothing about the application's reach changed (no new command / capability / migration / dependency),
* the Rust constants are the reviewed CONFIG-02A declaration and the catalog's pin,
* the non-test source touches no process, network, clock or file path API of its own,
* the documents keep the boundaries and the shared golden file has both consumers.
"""
import json
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
SRC = ROOT / "src-tauri" / "src"


def read(rel):
    return (ROOT / rel).read_text(encoding="utf-8")


def const(src, name):
    m = re.search(r'pub const %s: &str = "([^"]*)";' % name, src)
    assert m, name
    return m.group(1)


class Isolation(unittest.TestCase):
    def test_both_modules_are_declared_for_tests_only(self):
        self.assertRegex(read("src-tauri/src/lib.rs"), r"#\[cfg\(test\)\]\r?\nmod config_txn;")
        self.assertRegex(read("src-tauri/src/managed_fs.rs"), r'#\[cfg\(test\)\]\r?\n#\[path = "managed_fs_mcp\.rs"\]\r?\nmod mcp_file;')
        self.assertRegex(read("src-tauri/src/config_txn.rs"), r'#\[cfg\(test\)\]\r?\n#\[path = "config_txn_tests\.rs"\]\r?\nmod tests;')

    def test_nothing_else_refers_to_them(self):
        own = {"config_txn.rs", "config_txn_tests.rs", "managed_fs_mcp.rs", "managed_fs_mcp_tests.rs", "lib.rs", "managed_fs.rs"}
        for path in SRC.rglob("*.rs"):
            if path.name in own:
                continue
            text = path.read_text(encoding="utf-8")
            self.assertIsNone(re.search(r"\bconfig_txn\b|\bmcp_file\b|managed_fs_mcp|ProjectRoot", text), str(path))

    def test_no_command_capability_migration_or_dependency_was_added(self):
        self.assertEqual(len(re.findall(r"#\[tauri::command\]", read("src-tauri/src/commands.rs"))), 36)
        for p in json.loads(read("src-tauri/capabilities/default.json"))["permissions"]:
            self.assertIsNone(re.search(r"fs|shell|http|opener|dialog|clipboard|process|os:|path", p, re.I), p)
        migrations = sorted(p.name for p in (ROOT / "src-tauri" / "migrations").glob("*.sql"))
        self.assertEqual((len(migrations), migrations[-1]), (10, "0010_codex_cli_surface.sql"))
        cargo = read("src-tauri/Cargo.toml")
        deps = re.search(r"\[dependencies\]\r?\n(.*?)\r?\n\r?\n", cargo, re.S).group(1)
        self.assertEqual(
            sorted(re.findall(r"^([a-z0-9_-]+) =", deps, re.M)),
            sorted(["tauri", "tauri-plugin-single-instance", "tauri-plugin-autostart", "rusqlite", "serde", "serde_json", "sha2", "uuid", "chrono", "regex"]),
        )

    def test_the_non_test_prototype_sources_have_no_process_network_clock_or_path_access_of_their_own(self):
        forbidden = r"std::process|Command::new|std::net|TcpStream|reqwest|tauri::|invoke|SystemTime|Instant::now|chrono|std::env|std::fs|File::open|OpenOptions|canonicalize|read_dir"
        for name in ("config_txn.rs", "managed_fs_mcp.rs"):
            self.assertIsNone(re.search(forbidden, read(f"src-tauri/src/{name}")), name)

    def test_the_target_name_is_a_constant_and_no_function_takes_a_file_name(self):
        src = read("src-tauri/src/managed_fs_mcp.rs")
        for sig in re.findall(r"pub fn (\w+)\(([^)]*)\)", src):
            self.assertNotRegex(sig[1], r"name\s*:|file\s*:|rel\s*:|path\s*:\s*&str", sig[0])
        self.assertEqual(const(read("src-tauri/src/config_txn.rs"), "TARGET_FILE"), ".mcp.json")


class Constants(unittest.TestCase):
    def test_the_rust_constants_are_the_reviewed_declaration_and_the_catalog_pin(self):
        rust = read("src-tauri/src/config_txn.rs")
        ts = read("src/configDeclarations.ts")
        self.assertEqual(const(rust, "STANZA_TYPE"), "http")
        self.assertIn('declaration: { type: "http", url: "%s" },' % const(rust, "STANZA_URL"), ts)
        self.assertIn('entryId: "%s"' % const(rust, "ENTRY_ID"), ts)
        self.assertIn('hostId: "%s"' % const(rust, "HOST_ID"), ts)
        self.assertIn('serverName: "%s"' % const(rust, "SERVER_NAME"), ts)
        self.assertIn('reviewedAgainst: "%s"' % const(rust, "CATALOG_PIN"), ts)
        self.assertIn('repositoryUrl: "%s"' % const(rust, "REPOSITORY_URL"), ts)
        records = json.loads(read("data/catalog-candidates-c01.json"))["records"]
        rec = next(r["record"] for r in records if r["record"]["toolId"] == const(rust, "ENTRY_ID"))
        self.assertEqual(rec["sourceSnapshot"]["immutableCommit"], const(rust, "CATALOG_PIN"))
        self.assertEqual(rec["repository"]["url"], const(rust, "REPOSITORY_URL"))
        self.assertEqual(const(rust, "SURFACE"), "claude-code-terminal")

    def test_the_preview_mask_is_the_one_the_preview_uses(self):
        self.assertIn('MASK = "%s"' % const(read("src-tauri/src/config_txn.rs"), "PREVIEW_MASK"), read("src/configPlan.ts"))


class GoldenAndDocuments(unittest.TestCase):
    def test_the_golden_file_has_both_consumers(self):
        golden = json.loads(read("tests/fixtures/config-02b-golden.json"))
        self.assertGreaterEqual(len(golden["cases"]), 5)
        self.assertIn("config-02b-golden.json", read("src/configPlanGolden.test.ts"))
        self.assertIn("config-02b-golden.json", read("src-tauri/src/config_txn_tests.rs"))

    def test_the_documents_keep_the_boundaries(self):
        docs = read("docs/config-02b/CONTRACT_AND_LIMITS.md") + read("docs/config-02b/TESTS_AND_REDTEAM.md")
        for needle in (
            "REAL_CONFIG_WRITE_NO_GO", "REAL_INSTALLATION_NO_GO", "#14", "#25", "#39", "WRITE_ADAPTER_BLOCKED", "design only", "not authorised",
            "not detected", "CONFIG02B_SKIPPED", "Failure table", "State machine", "implemented in tests",
        ):
            self.assertIn(needle, docs, needle)
        self.assertNotRegex(docs, r"(?i)production[- ]ready|issue #14 (is )?(closed|resolved|satisfied)")
        self.assertIn("Yalnızca yapılandırma taslağı; uygulanmadı, bağlantı kurulmadı.", docs)


if __name__ == "__main__":
    unittest.main()
