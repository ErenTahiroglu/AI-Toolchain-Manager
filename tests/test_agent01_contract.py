"""AGENT-01 (Issue #31) contract, no running app needed.

Codex CLI becomes a selectable project target next to Claude Code and Antigravity; every other client is covered by one manual guide, not by an adapter.

* The surface lists agree everywhere (Rust allowlist, TypeScript list, host registry, migration 0010's CHECK), in the same order.
* The install-plan layer, the Tauri capability and the Rust command list gain NOTHING; no catalog data file carries a Codex evidence row.
* Migration 0010 is a pure table rebuild: every column is copied, only the surface list of one CHECK changes. It is also run here with Python's sqlite3 (foreign keys ON)
  against a real schema built from migrations 1-9, so the row-preservation claim does not rest on the Rust test binary alone.
* The five documents exist and keep the boundaries; the production registry of reviewed declarations held NO entry at AGENT-01 (C5: NO QUALIFIED DECLARATION); CONFIG-02A (Issue #38) added exactly one, checked in test_config02a_contract.py.
"""
import json
import pathlib
import re
import sqlite3
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
SURFACES = ["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "codex-cli", "local-cli"]
HISTORIC = [s for s in SURFACES if s != "codex-cli"]


def read(rel):
    return (ROOT / rel).read_text(encoding="utf-8")


def quoted_list(text):
    return re.findall(r"""['"]([^'"]+)['"]""", text)


def rust_list(source, name):
    m = re.search(r"pub const %s: &\[&str\] =\s*&\[([^\]]*)\]" % name, source)
    assert m, name
    return quoted_list(m.group(1))


class SurfaceListsAgree(unittest.TestCase):
    def test_rust_typescript_registry_and_migration_lists_are_identical(self):
        self.assertEqual(rust_list(read("src-tauri/src/projects.rs"), "SURFACES"), SURFACES)
        m = re.search(r"export const TARGET_SURFACES = \[([^\]]*)\] as const", read("src/workspace.ts"))
        self.assertEqual(quoted_list(m.group(1)), SURFACES)
        self.assertEqual(re.findall(r'^\s+agentSurface: "([^"]+)"', read("src/agentHosts.ts"), re.M), SURFACES)
        sql = read("src-tauri/migrations/0010_codex_cli_surface.sql")
        m = re.search(r"agent_surface\s+TEXT NOT NULL CHECK \(agent_surface IN \(([^)]*)\)\)", sql)
        self.assertEqual(quoted_list(m.group(1)), SURFACES)

    def test_the_install_plan_layer_does_not_know_codex(self):
        plan = rust_list(read("src-tauri/src/plan.rs"), "SURFACES")
        self.assertNotIn("codex-cli", plan)
        self.assertEqual(plan, ["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "antigravity-cli", "antigravity-2", "local-cli"])
        self.assertNotIn("codex", read("src-tauri/src/recipe.rs").lower().replace("encode", ""))
        for rel in ("schemas/install-recipe.schema.json", "schemas/reviewed-recipe.schema.json", "schemas/audit-event.schema.json"):
            self.assertNotIn("codex", read(rel).lower(), rel)

    def test_no_catalog_data_file_or_schema_carries_a_codex_evidence_row(self):
        # the word "codex" may appear in a publisher's own description (e.g. a purpose text); an evidence ROW or schema value for the surface may not
        for rel in ("data/catalog-seed.json", "data/catalog-candidates-1b.json", "data/catalog-candidates-c01.json", "data/catalog-candidates-c02.json", "data/catalog-candidates-c03.json", "schemas/catalog-entry.schema.json"):
            text = read(rel)
            self.assertNotIn("codex-cli", text, rel)
            self.assertIsNone(re.search(r'"agentSurface"\s*:\s*"codex', text, re.I), rel)
            self.assertIsNone(re.search(r"codex", text.split('"records"')[0] if '"records"' in text else text, re.I) if rel.endswith("schema.json") else None, rel)

    def test_the_historic_candidate_identities_are_all_still_present(self):
        ids = []
        for rel in ("data/catalog-seed.json", "data/catalog-candidates-1b.json", "data/catalog-candidates-c01.json", "data/catalog-candidates-c02.json", "data/catalog-candidates-c03.json"):
            for r in json.loads(read(rel))["records"]:
                ids.append(r.get("toolId") or r["record"]["toolId"])
        self.assertEqual((len(ids), len(set(ids))), (22, 22))
        self.assertEqual((9, 4, 3, 2, 4), tuple(len(json.loads(read("data/%s.json" % f))["records"]) for f in ("catalog-seed", "catalog-candidates-1b", "catalog-candidates-c01", "catalog-candidates-c02", "catalog-candidates-c03")))


class CapabilityAndCommandsUnchanged(unittest.TestCase):
    def test_the_capability_grants_no_filesystem_shell_network_or_codex_permission(self):
        permissions = json.loads(read("src-tauri/capabilities/default.json"))["permissions"]
        self.assertGreater(len(permissions), 20)
        for p in permissions:
            self.assertIsNone(re.search(r"fs|shell|http|opener|dialog|clipboard|process|os:|path|window", p, re.I), p)
            self.assertIsNone(re.search(r"config|mcp|codex|write-file|apply", p, re.I), p)

    def test_no_rust_command_for_codex_or_configuration_was_added(self):
        for rel in ("src-tauri/src/commands.rs", "src-tauri/src/lib.rs"):
            self.assertNotRegex(read(rel), r"(?i)fn\s+\w*(codex|mcp_config|config_preview|config_plan|write_config|apply_config|toml)\w*")

    def test_no_dependency_was_added(self):
        pkg = json.loads(read("package.json"))
        names = set(pkg.get("dependencies", {})) | set(pkg.get("devDependencies", {}))
        self.assertFalse([n for n in names if "toml" in n.lower() or "codex" in n.lower()], names)
        cargo = read("src-tauri/Cargo.toml").lower()
        self.assertNotRegex(cargo, r"(?m)^\s*(toml|toml_edit|basic-toml)\b")

    def test_the_new_typescript_sources_are_pure(self):
        forbidden = r"invoke\(|@tauri-apps|\bapi\.|fetch\(|XMLHttpRequest|WebSocket|navigator\.clipboard|localStorage|sessionStorage|indexedDB|setTimeout|setInterval|createObjectURL|FileReader|showSaveFilePicker|showOpenFilePicker|window\.open|child_process|require\("
        for rel in ("src/agentHosts.ts", "src/agentGuide.ts", "src/components/AgentGuide.tsx", "src/configPlan.ts", "src/configDeclarations.ts", "src/components/ConfigPreview.tsx"):
            self.assertIsNone(re.search(forbidden, read(rel)), rel)

    def test_the_toml_surface_is_never_given_to_the_json_parser(self):
        plan = read("src/configPlan.ts")
        i = plan.index("// 4b. a TOML target stops here")
        j = plan.index("// 5. the current configuration")
        self.assertLess(i, j)
        self.assertIn("return done(\"TOML_PREVIEW_NOT_IMPLEMENTED\")", plan[i:j])
        self.assertNotIn("parseConfigText(", plan[plan.index("export function planConfigPreview"):i])
        self.assertNotRegex(read("src/components/ConfigPreview.tsx"), r"toml.*parseConfigText|parseConfigText.*toml")


class MigrationTen(unittest.TestCase):
    def setUp(self):
        self.sql = read("src-tauri/migrations/0010_codex_cli_surface.sql")

    def table_columns(self, sql, table):
        m = re.search(r"CREATE TABLE %s \((.*?)\n\);" % re.escape(table), sql, re.S)
        self.assertIsNotNone(m, table)
        cols = []
        for line in m.group(1).split("\n"):
            line = line.strip()
            if not line or line.startswith("--") or line.startswith("PRIMARY KEY"):
                continue
            cols.append(re.match(r"(\w+)\s+(TEXT|INTEGER)", line).group(1, 2))
        return cols

    def test_the_rebuilt_table_has_exactly_the_old_columns_and_primary_key(self):
        old = self.table_columns(read("src-tauri/migrations/0002_catalog_registry.sql"), "project_preferences")
        new = self.table_columns(self.sql, "project_preferences_v10")
        self.assertEqual(old, new)
        self.assertEqual(len(new), 12)
        self.assertIn("PRIMARY KEY (project_id, entry_id, agent_surface, platform_target)", self.sql)
        # every column is copied, in order, by the INSERT ... SELECT
        names = [c[0] for c in new]
        ins = re.search(r"INSERT INTO project_preferences_v10\s*\((.*?)\)\s*SELECT (.*?)\s*FROM project_preferences", self.sql, re.S)
        self.assertEqual([x.strip() for x in ins.group(1).split(",")], names)
        self.assertEqual([x.strip() for x in ins.group(2).split(",")], names)

    def test_only_the_surface_check_differs_from_the_original_ddl(self):
        old = read("src-tauri/migrations/0002_catalog_registry.sql")
        old_ddl = re.search(r"CREATE TABLE project_preferences \((.*?)\n\);", old, re.S).group(1)
        new_ddl = re.search(r"CREATE TABLE project_preferences_v10 \((.*?)\n\);", self.sql, re.S).group(1)
        strip = lambda t: re.sub(r"\s+", " ", re.sub(r"--[^\n]*", "", t)).strip()
        self.assertEqual(strip(new_ddl), strip(old_ddl).replace("'antigravity-ide', 'local-cli'", "'antigravity-ide', 'codex-cli', 'local-cli'"))

    def test_it_touches_nothing_else(self):
        statements = re.findall(r"^(CREATE TABLE \w+|DROP TABLE \w+|ALTER TABLE \w+ RENAME TO \w+|INSERT INTO \w+|CREATE (?:UNIQUE )?INDEX|CREATE TRIGGER|UPDATE|DELETE)", self.sql, re.M)
        self.assertEqual(statements, ["CREATE TABLE project_preferences_v10", "INSERT INTO project_preferences_v10", "DROP TABLE project_preferences", "ALTER TABLE project_preferences_v10 RENAME TO project_preferences"])
        self.assertNotRegex(self.sql, r"(?i)\b(UPDATE|DELETE)\b\s+(FROM\s+)?\w")

    def test_the_migration_is_registered_last_and_documented(self):
        db = read("src-tauri/src/db.rs")
        block = db[db.index("const MIGRATIONS: &[Migration] = &["):]
        block = block[: block.index("];")]
        files = re.findall(r'include_str!\("\.\./migrations/([^"]+)"\)', block)
        self.assertEqual(files[-1], "0010_codex_cli_surface.sql")
        self.assertEqual(len(files), 7)  # migrations 2, 5 and 7 are Rust steps that include their own SQL
        self.assertEqual(block.count("Migration::"), 10)  # ten migrations in total: this phase added exactly one
        self.assertIn("10 = AGENT-01", db)

    def build_v9(self):
        con = sqlite3.connect(":memory:")
        con.execute("PRAGMA foreign_keys = ON")
        for name in ("0001_init", "0002_catalog_registry", "0003_scoped_suggestion_log", "0004_discovery", "0005_discovery_privacy", "0006_passive_provenance", "0007_identity_taxonomy", "0008_artifact_provenance", "0009_managed_roots"):
            con.executescript(read("src-tauri/migrations/%s.sql" % name))
        con.execute("PRAGMA foreign_keys = ON")
        return con

    def test_run_against_a_real_v9_schema_every_row_survives_unchanged_and_only_codex_is_newly_accepted(self):
        con = self.build_v9()
        now = "2026-10-25T00:00:00Z"
        for eid in ("tool-a", "tool-b"):
            con.execute(
                "INSERT INTO catalog_entries (entry_id, kind, origin, display_name, purpose, source_key, repo_url, review_state, metadata_state, execution_policy, license_state, enabled, revision, capability_fingerprint, record_json, added_at, updated_at) "
                "VALUES (?, 'executable_tool', 'seed', ?, 'p', ?, 'https://github.com/x/y', 'metadata-reviewed', 'documented-snapshot', 'manual-only', 'known', 1, 'r1', ?, '{}', ?, ?)",
                (eid, eid, "key-" + eid, "f" * 64, now, now),
            )
        con.execute("INSERT INTO project_registry (project_id, alias, display_name, kind, created_at) VALUES ('p1', 'proje-bir', 'Bir', 'reference', ?)", (now,))
        con.execute("INSERT INTO project_registry (project_id, alias, display_name, kind, created_at) VALUES ('p2', 'proje-iki', 'Iki', 'reference', ?)", (now,))
        rows = []
        for i, surface in enumerate(HISTORIC):
            for platform, decision in (("windows-x64", "accepted"), ("macos-arm64", "postponed")):
                rows.append(("p1" if i % 2 == 0 else "p2", "tool-a" if platform == "windows-x64" else "tool-b", surface, platform, decision, "2026-10-%02dT00:00:00Z" % (10 + i), "r1", "fp%d" % i, now if decision == "postponed" else None))
        for r in rows:
            con.execute("INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint, remind_at) VALUES (?,?,?,?,?,?,?,?,?)", r)
        snapshot = lambda: con.execute("SELECT * FROM project_preferences ORDER BY rowid").fetchall()
        before = snapshot()
        self.assertEqual(len(before), 8)
        with self.assertRaises(sqlite3.IntegrityError):  # the old table cannot hold a Codex decision
            con.execute("INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint) VALUES ('p1','tool-b','codex-cli','windows-x64','rejected','x','x','x')")

        con.executescript(self.sql)

        self.assertEqual(snapshot(), before, "every row is byte-for-byte unchanged and in the same order")
        con.execute("INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint) VALUES ('p1','tool-b','codex-cli','windows-x64','rejected','x','x','x')")
        for bad_surface in ("gemini-cli", "codex", "Codex-CLI", "deepseek", ""):
            with self.assertRaises(sqlite3.IntegrityError, msg=bad_surface):
                con.execute("INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint) VALUES ('p2','tool-b',?,'windows-x64','rejected','x','x','x')", (bad_surface,))
        with self.assertRaises(sqlite3.IntegrityError):  # primary key
            con.execute("INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint) VALUES ('p1','tool-b','codex-cli','windows-x64','accepted','x','x','x')")
        with self.assertRaises(sqlite3.IntegrityError):  # foreign keys still enforced
            con.execute("INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint) VALUES ('nope','tool-b','codex-cli','macos-arm64','rejected','x','x','x')")
        with self.assertRaises(sqlite3.IntegrityError):  # a decision value that is not a wish
            con.execute("INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint) VALUES ('p1','tool-a','codex-cli','macos-arm64','installed','x','x','x')")
        with self.assertRaises(sqlite3.IntegrityError):  # actual state still cannot claim an install
            con.execute("UPDATE project_preferences SET actual_install_state = 'installed' WHERE rowid = 1")
        self.assertEqual(con.execute("SELECT count(*) FROM sqlite_master WHERE name LIKE 'project_preferences%'").fetchone()[0], 1)
        self.assertEqual(con.execute("PRAGMA foreign_key_check").fetchall(), [])


class RegistryAndDocuments(unittest.TestCase):
    DOCS = ("HOST_PROVIDER_PROTOCOL_MATRIX", "CODEX_SOURCE_AND_SCOPE", "COMMON_AGENT_GUIDE_TR", "IMPLEMENTATION_AND_TESTS", "NEXT_PRODUCT_HANDOFF")

    def test_the_production_registry_has_no_codex_declaration(self):
        # AGENT-01 (C5): no Codex declaration exists; CONFIG-02A added one Claude Code entry and nothing else
        src = read("src/configDeclarations.ts")
        self.assertEqual(re.findall(r'hostId: "([^"]+)"', src), ["claude-code-project-mcp"])
        self.assertNotIn("codex", src.split("export const REVIEWED_DECLARATIONS")[1].lower())

    def test_the_five_documents_exist_and_keep_the_boundaries(self):
        docs = {n: read("docs/agent-01/%s.md" % n) for n in self.DOCS}
        joined = "\n".join(docs.values())
        for needle in ("REAL_CONFIG_WRITE_NO_GO", "REAL_INSTALLATION_NO_GO", "#14", "#25", "#29", "#31", "NO QUALIFIED DECLARATION", "TOML_PREVIEW_NOT_IMPLEMENTED", "MANUAL_VERIFY"):
            self.assertIn(needle, joined, needle)
        self.assertRegex(docs["CODEX_SOURCE_AND_SCOPE"], r"(?i)trusted")
        self.assertIn(".codex/config.toml", docs["CODEX_SOURCE_AND_SCOPE"])
        self.assertIn("Grok", docs["HOST_PROVIDER_PROTOCOL_MATRIX"])
        self.assertIn("Groq", docs["HOST_PROVIDER_PROTOCOL_MATRIX"])
        self.assertIn("Gemini CLI", docs["HOST_PROVIDER_PROTOCOL_MATRIX"])
        self.assertNotRegex(joined, r"(?i)issue #14 (is )?(closed|resolved|satisfied)")

    def test_the_new_publisher_sources_are_registered_with_their_limits(self):
        register = read("docs/RESEARCH-SOURCES.md")
        for rid in ("R48", "R49", "R50", "R51", "R52", "R53", "R54"):
            self.assertTrue(any(line.startswith("| %s |" % rid) for line in register.splitlines()), rid)
        row = next(l for l in register.splitlines() if l.startswith("| R51 |"))
        self.assertIn("Page Not Found", row)
        self.assertIn("pipe-to-shell", row)
        row = next(l for l in register.splitlines() if l.startswith("| R54 |"))
        self.assertIn("1.64.0-alpha-1790635538000", row)
        self.assertIn("_hasShrinkwrap", row)


if __name__ == "__main__":
    unittest.main()
