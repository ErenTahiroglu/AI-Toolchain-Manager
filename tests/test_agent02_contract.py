"""AGENT-02 (Issue #53) contract, no running app needed.

Six more read-only project surfaces (codex-vscode, cline-cli, cline-vscode, cline-jetbrains, kiro-cli, kiro-ide) next to the existing five.

* The surface lists agree everywhere (Rust allowlist, TypeScript list, host registry, migration 0011's CHECK, catalog-entry schema), in the same order.
* The install-plan layer, the discovery CHECK, the other schemas and the Tauri capability gain NOTHING.
* Migration 0011 is a pure table rebuild. It is run here with Python's sqlite3 (foreign keys ON) against a real schema built from migrations 1-10, including a
  deliberately failing, rolled-back run, so the row-preservation and atomicity claims do not rest on the Rust test binary alone (which could not be built on the
  authoring machine; see docs/agent-02/TEST_PLAN_AND_RESULTS.md).
"""
import json
import pathlib
import re
import sqlite3
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
OLD = ["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "codex-cli", "local-cli"]
NEW = ["codex-vscode", "cline-cli", "cline-vscode", "cline-jetbrains", "kiro-cli", "kiro-ide"]
ALL = ["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "codex-cli", "codex-vscode", "cline-cli", "cline-vscode", "cline-jetbrains", "kiro-cli", "kiro-ide", "local-cli"]
MIGRATIONS_1_TO_10 = ("0001_init", "0002_catalog_registry", "0003_scoped_suggestion_log", "0004_discovery", "0005_discovery_privacy", "0006_passive_provenance", "0007_identity_taxonomy", "0008_artifact_provenance", "0009_managed_roots", "0010_codex_cli_surface")
INSERT = ("INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint) "
          "VALUES (?, ?, ?, ?, ?, 'x', 'x', 'x')")


def read(rel):
    return (ROOT / rel).read_text(encoding="utf-8")


def quoted_list(text):
    return re.findall(r"""['"]([^'"]+)['"]""", text)


def rust_list(source, name):
    m = re.search(r"pub const %s: &\[&str\] =\s*&\[([^\]]*)\]" % name, source)
    assert m, name
    return quoted_list(m.group(1))


def statements(sql):
    """Split a migration file into statements (comment lines removed; the files contain no semicolon inside a literal)."""
    body = "\n".join(l for l in sql.splitlines() if not l.strip().startswith("--"))
    return [s.strip() + ";" for s in body.split(";") if s.strip()]


def build_v10():
    con = sqlite3.connect(":memory:", isolation_level=None)
    for name in MIGRATIONS_1_TO_10:
        con.executescript(read("src-tauri/migrations/%s.sql" % name))
    con.execute("PRAGMA foreign_keys = ON")
    return con


def seed(con):
    now = "2026-11-14T00:00:00Z"
    for eid in ("tool-a", "tool-b"):
        con.execute(
            "INSERT INTO catalog_entries (entry_id, kind, origin, display_name, purpose, source_key, repo_url, review_state, metadata_state, execution_policy, license_state, enabled, revision, capability_fingerprint, record_json, added_at, updated_at) "
            "VALUES (?, 'executable_tool', 'seed', ?, 'p', ?, 'https://github.com/x/y', 'metadata-reviewed', 'documented-snapshot', 'manual-only', 'known', 1, 'r1', ?, '{}', ?, ?)",
            (eid, eid, "key-" + eid, "f" * 64, now, now),
        )
    con.execute("INSERT INTO project_registry (project_id, alias, display_name, kind, created_at) VALUES ('p1', 'proje-bir', 'Bir', 'reference', ?)", (now,))
    con.execute("INSERT INTO project_registry (project_id, alias, display_name, kind, created_at) VALUES ('p2', 'proje-iki', 'Iki', 'reference', ?)", (now,))
    n = 0
    for surface in OLD:  # every old surface, including codex-cli, both platforms
        for platform, decision in (("windows-x64", "accepted"), ("macos-arm64", "postponed")):
            con.execute(
                "INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint, remind_at) VALUES (?,?,?,?,?,?,?,?,?)",
                ("p1" if n % 2 == 0 else "p2", "tool-a" if platform == "windows-x64" else "tool-b", surface, platform, decision, "2026-10-%02dT00:00:00Z" % (10 + n), "r1", "fp%d" % n, now if decision == "postponed" else None),
            )
            n += 1
    return n


def objects(con):
    return sorted(con.execute("SELECT type, name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'").fetchall())


class SurfaceListsAgree(unittest.TestCase):
    def test_rust_typescript_registry_migration_and_schema_lists_are_identical(self):
        self.assertEqual(rust_list(read("src-tauri/src/projects.rs"), "SURFACES"), ALL)
        m = re.search(r"export const TARGET_SURFACES = \[([^\]]*)\] as const", read("src/workspace.ts"))
        self.assertEqual(quoted_list(m.group(1)), ALL)
        self.assertEqual(re.findall(r'^\s+agentSurface: "([^"]+)"', read("src/agentHosts.ts"), re.M), ALL)
        m = re.search(r"agent_surface\s+TEXT NOT NULL CHECK \(agent_surface IN \(([^)]*)\)\)", read("src-tauri/migrations/0011_agent02_surfaces.sql"))
        self.assertEqual(quoted_list(m.group(1)), ALL)
        schema = json.loads(read("schemas/catalog-entry.schema.json"))
        found = []

        def walk(o):
            if isinstance(o, dict):
                if isinstance(o.get("agentSurface"), dict) and "enum" in o["agentSurface"]:
                    found.append(o["agentSurface"]["enum"])
                for v in o.values():
                    walk(v)
            elif isinstance(o, list):
                for v in o:
                    walk(v)

        walk(schema)
        self.assertEqual(found, [["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "antigravity-cli", "antigravity-2", "codex-cli", "codex-vscode", "cline-cli", "cline-vscode", "cline-jetbrains", "kiro-cli", "kiro-ide", "local-cli", None]])

    def test_the_install_plan_discovery_and_other_schemas_gain_nothing(self):
        plan = rust_list(read("src-tauri/src/plan.rs"), "SURFACES")
        self.assertEqual(plan, ["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "antigravity-cli", "antigravity-2", "local-cli"])
        for rel in ("src-tauri/src/plan.rs", "src-tauri/src/recipe.rs", "src-tauri/src/config_txn.rs", "src-tauri/src/managed_fs_mcp.rs", "src-tauri/src/install_flow.rs", "src-tauri/src/discovery/detect.rs", "src-tauri/src/discovery/store.rs", "src-tauri/migrations/0004_discovery.sql"):
            text = read(rel)
            for s in NEW:
                self.assertNotIn(s, text, "%s names %s" % (rel, s))
        for rel in ("schemas/install-recipe.schema.json", "schemas/reviewed-recipe.schema.json", "schemas/project-policy.schema.json", "schemas/sync-envelope.schema.json", "schemas/audit-event.schema.json"):
            text = read(rel)
            for s in NEW:
                self.assertNotIn(s, text, "%s names %s" % (rel, s))

    def test_no_desktop_alias_or_provider_became_a_surface_and_no_data_file_carries_new_evidence(self):
        for bad in ("codex-ide", "codex-desktop", "codex-app", "cline-desktop"):
            for rel in ("src/workspace.ts", "src-tauri/src/projects.rs", "src-tauri/migrations/0011_agent02_surfaces.sql", "src/agentHosts.ts"):
                self.assertNotIn('"%s"' % bad, read(rel), rel)
                self.assertNotIn("'%s'" % bad, read(rel), rel)
        for rel in ("data/catalog-seed.json", "data/catalog-candidates-1b.json", "data/catalog-candidates-c01.json", "data/catalog-candidates-c02.json", "data/catalog-candidates-c03.json", "data/catalog-candidates-c04.json"):
            text = read(rel)
            for s in NEW + ["codex-cli"]:
                self.assertNotIn('"agentSurface": "%s"' % s, text, rel)
                self.assertNotIn('"agentSurface":"%s"' % s, text, rel)

    def test_the_capability_and_command_surface_is_unchanged(self):
        import subprocess
        diff = subprocess.run(["git", "diff", "HEAD", "--", "src-tauri/capabilities", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "package.json", "package-lock.json", ".github", "tools/check_action_pins.py", "tools/action_pins_allowlist.json"], cwd=ROOT, capture_output=True, text=True, check=False).stdout
        self.assertEqual(diff, "", "capabilities, Tauri config, dependencies, workflows and the pin guard are unchanged")
        lib = read("src-tauri/src/lib.rs")
        self.assertNotRegex(lib, r"(?i)fn\s+\w*(cline|kiro|codex_vscode)\w*")
        head = subprocess.run(["git", "show", "HEAD:src-tauri/src/lib.rs"], cwd=ROOT, capture_output=True, text=True, check=False).stdout
        handlers = lambda t: re.search(r"generate_handler!\[(.*?)\]\)", t, re.S).group(1)
        self.assertEqual(handlers(lib), handlers(head), "no Tauri command was added or removed")

    def test_no_dependency_was_added(self):
        self.assertNotRegex(read("src-tauri/Cargo.toml"), r"(?i)\b(cline|kiro|toml)\b\s*=")
        names = set(json.loads(read("package.json")).get("dependencies", {})) | set(json.loads(read("package.json")).get("devDependencies", {}))
        self.assertFalse([n for n in names if re.search(r"cline|kiro|toml", n, re.I)], names)


class MigrationEleven(unittest.TestCase):
    def setUp(self):
        self.sql = read("src-tauri/migrations/0011_agent02_surfaces.sql")

    def test_it_is_a_pure_rebuild_that_touches_nothing_else(self):
        found = re.findall(r"^(CREATE TABLE \w+|DROP TABLE \w+|ALTER TABLE \w+ RENAME TO \w+|INSERT INTO \w+|CREATE (?:UNIQUE )?INDEX|CREATE TRIGGER|CREATE VIEW|UPDATE|DELETE|PRAGMA)", self.sql, re.M)
        self.assertEqual(found, ["CREATE TABLE project_preferences_v11", "INSERT INTO project_preferences_v11", "DROP TABLE project_preferences", "ALTER TABLE project_preferences_v11 RENAME TO project_preferences"])
        for table in ("preference_history", "discovery_observations", "audit_events", "installation"):
            self.assertNotRegex(re.sub(r"--[^\n]*", "", self.sql), r"(?i)\b%s\b" % table)

    def test_only_the_surface_check_differs_from_migration_ten(self):
        ten = read("src-tauri/migrations/0010_codex_cli_surface.sql")
        strip = lambda t: re.sub(r"\s+", " ", re.sub(r"--[^\n]*", "", t)).strip()
        old_ddl = re.search(r"CREATE TABLE project_preferences_v10 \((.*?)\n\);", ten, re.S).group(1)
        new_ddl = re.search(r"CREATE TABLE project_preferences_v11 \((.*?)\n\);", self.sql, re.S).group(1)
        self.assertEqual(strip(new_ddl), strip(old_ddl).replace("'codex-cli', 'local-cli'", "'codex-cli', 'codex-vscode', 'cline-cli', 'cline-vscode', 'cline-jetbrains', 'kiro-cli', 'kiro-ide', 'local-cli'"))
        names = [m.group(1) for m in re.finditer(r"^\s{4}(\w+)\s+(?:TEXT|INTEGER)", new_ddl, re.M)]
        ins = re.search(r"INSERT INTO project_preferences_v11\s*\((.*?)\)\s*SELECT (.*?)\s*FROM project_preferences", self.sql, re.S)
        self.assertEqual([x.strip() for x in ins.group(1).split(",")], names)
        self.assertEqual([x.strip() for x in ins.group(2).split(",")], names)
        self.assertEqual(len(names), 12)

    def test_it_is_exact_allowlist_not_a_pattern(self):
        check = re.search(r"agent_surface\s+TEXT NOT NULL CHECK \(([^\n]*)\),\n", self.sql).group(1)
        self.assertNotRegex(check, r"(?i)\b(LIKE|GLOB|REGEXP|MATCH)\b")

    def build_and_seed(self):
        con = build_v10()
        seed(con)
        return con

    def test_run_against_a_real_v10_schema_every_row_survives_and_only_the_six_ids_are_newly_accepted(self):
        con = self.build_and_seed()
        snapshot = lambda: con.execute("SELECT * FROM project_preferences ORDER BY rowid").fetchall()
        before, codex_before = snapshot(), con.execute("SELECT * FROM project_preferences WHERE agent_surface = 'codex-cli' ORDER BY rowid").fetchall()
        objs_before = objects(con)
        other_ddl = con.execute("SELECT name, sql FROM sqlite_master WHERE type='table' AND name <> 'project_preferences' ORDER BY name").fetchall()
        self.assertEqual(len(before), 10)
        self.assertEqual(len(codex_before), 2)
        for s in NEW:  # the old table cannot hold any new id
            with self.assertRaises(sqlite3.IntegrityError, msg=s):
                con.execute(INSERT, ("p1", "tool-b", s, "windows-x64", "rejected"))

        con.executescript(self.sql)

        self.assertEqual(snapshot(), before, "every row of every column is unchanged and in the same order")
        self.assertEqual(con.execute("SELECT * FROM project_preferences WHERE agent_surface = 'codex-cli' ORDER BY rowid").fetchall(), codex_before)
        self.assertEqual(objects(con), objs_before, "no table, index, trigger or view added or removed")
        self.assertEqual(con.execute("SELECT name, sql FROM sqlite_master WHERE type='table' AND name <> 'project_preferences' ORDER BY name").fetchall(), other_ddl, "no other table changed")
        for s in NEW:
            self.assertEqual(con.execute("SELECT count(*) FROM project_preferences WHERE agent_surface = ?", (s,)).fetchone()[0], 0, "no row created for " + s)
        # every old and every new id is now accepted (a distinct (project, entry, platform) per id)
        for i, s in enumerate(NEW):
            con.execute(INSERT, ("p2", "tool-a" if i % 2 else "tool-b", s, "windows-x64" if i < 3 else "macos-arm64", "rejected"))
        for bad in ("gemini-cli", "codex", "Codex-CLI", "CLINE-CLI", "Kiro-IDE", "codex-ide", "codex-desktop", "codex-app", "cline-desktop", "cline", "kiro", "cursor", "windsurf", "deepseek", "groq", "grok", "kiro-ide ", " kiro-cli", "kiro-%", "cline-*", "kiro‑ide", "", "antigravity-cli", "antigravity-2"):
            with self.assertRaises(sqlite3.IntegrityError, msg=repr(bad)):
                con.execute(INSERT, ("p1", "tool-b", bad, "macos-arm64", "rejected"))
        with self.assertRaises(sqlite3.IntegrityError):  # primary key
            con.execute(INSERT, ("p2", "tool-b", "codex-vscode", "windows-x64", "accepted"))
        with self.assertRaises(sqlite3.IntegrityError):  # foreign keys still enforced
            con.execute(INSERT, ("nope", "tool-b", "kiro-ide", "macos-arm64", "rejected"))
        with self.assertRaises(sqlite3.IntegrityError):  # a decision value that is not a wish
            con.execute(INSERT, ("p1", "tool-a", "kiro-cli", "macos-arm64", "installed"))
        with self.assertRaises(sqlite3.IntegrityError):  # platform still exact
            con.execute(INSERT, ("p1", "tool-a", "kiro-cli", "linux-x64", "rejected"))
        with self.assertRaises(sqlite3.IntegrityError):  # actual state still cannot claim an install
            con.execute("UPDATE project_preferences SET actual_install_state = 'installed' WHERE rowid = 1")
        self.assertEqual(con.execute("SELECT count(*) FROM sqlite_master WHERE name LIKE 'project_preferences%'").fetchone()[0], 1)
        self.assertEqual(con.execute("PRAGMA foreign_key_check").fetchall(), [])
        self.assertEqual(con.execute("PRAGMA integrity_check").fetchall(), [("ok",)])

    def test_discovery_and_history_constraints_are_untouched(self):
        con = self.build_and_seed()
        ddl = {n: s for n, s in con.execute("SELECT name, sql FROM sqlite_master WHERE type='table'")}
        con.executescript(self.sql)
        after = {n: s for n, s in con.execute("SELECT name, sql FROM sqlite_master WHERE type='table'")}
        self.assertEqual(after["discovery_observations"], ddl["discovery_observations"])
        self.assertEqual(after["preference_history"], ddl["preference_history"])
        self.assertIn("agent_surface IN ('claude-code-terminal', 'claude-desktop-code', 'antigravity-ide', 'local-cli')", after["discovery_observations"])
        for s in NEW:
            self.assertNotIn(s, after["discovery_observations"])

    def test_a_failure_after_the_rebuild_rolls_everything_back(self):
        con = self.build_and_seed()
        before = con.execute("SELECT * FROM project_preferences ORDER BY rowid").fetchall()
        objs = objects(con)
        version = con.execute("PRAGMA user_version").fetchone()[0]
        con.execute("BEGIN")
        for stmt in statements(self.sql):
            con.execute(stmt)
        self.assertIn("kiro-ide", con.execute("SELECT sql FROM sqlite_master WHERE name = 'project_preferences'").fetchone()[0])  # the rebuild happened inside the transaction
        con.execute("ROLLBACK")  # the injected failure
        self.assertEqual(con.execute("PRAGMA user_version").fetchone()[0], version, "the version was not bumped")
        self.assertEqual(con.execute("SELECT * FROM project_preferences ORDER BY rowid").fetchall(), before)
        self.assertEqual(objects(con), objs, "no scratch table survived")
        with self.assertRaises(sqlite3.IntegrityError):
            con.execute(INSERT, ("p1", "tool-b", "kiro-ide", "macos-arm64", "rejected"))
        con.executescript(self.sql)  # and the real run still succeeds afterwards
        self.assertEqual(con.execute("SELECT * FROM project_preferences ORDER BY rowid").fetchall(), before)

    def test_the_migration_is_registered_last_and_documented(self):
        db = read("src-tauri/src/db.rs")
        block = db[db.index("const MIGRATIONS: &[Migration] = &["):]
        block = block[: block.index("];")]
        files = re.findall(r'include_str!\("\.\./migrations/([^"]+)"\)', block)
        self.assertEqual(files[-1], "0011_agent02_surfaces.sql")
        self.assertEqual(block.count("Migration::"), 11)
        self.assertEqual(sorted(p.name for p in (ROOT / "src-tauri" / "migrations").iterdir() if p.name[:4].isdigit())[-1], "0011_agent02_surfaces.sql")

    def test_migrations_one_to_ten_are_byte_identical_to_the_baseline(self):
        import subprocess
        out = subprocess.run(["git", "diff", "--stat", "HEAD", "--", "src-tauri/migrations"], cwd=ROOT, capture_output=True, text=True, check=False).stdout
        self.assertNotRegex(out, r"migrations/00(0[1-9]|10)_")


class Documents(unittest.TestCase):
    DOCS = ("SOURCE_EVIDENCE", "SURFACE_MATRIX", "MIGRATION_AND_SECURITY", "TEST_PLAN_AND_RESULTS")

    def test_the_documents_exist_and_keep_the_boundaries(self):
        docs = {n: read("docs/agent-02/%s.md" % n) for n in self.DOCS}
        joined = "\n".join(docs.values())
        for needle in ("#53", "#54", "R58", "R59", "R60", "2026-11-14", "not-assessed", "none-documented", "UNSUPPORTED_SURFACE", "TOML_PREVIEW_NOT_IMPLEMENTED", "0011_agent02_surfaces", "BLOCKED"):
            self.assertIn(needle, joined, needle)
        self.assertNotRegex(joined, r"(?i)independently (accepted|tested|verified)\b(?!.*not)")

    def test_the_register_rows_exist_with_their_limits(self):
        lines = read("docs/RESEARCH-SOURCES.md").splitlines()
        for rid in ("R58", "R59", "R60"):
            self.assertEqual(sum(1 for l in lines if l.startswith("| %s |" % rid)), 1, rid)
        ids = [re.match(r"\| (R\d+) \|", l).group(1) for l in lines if re.match(r"\| R\d+ \|", l)]
        # R23-R25 are historic duplicates that existed before this phase (historical evidence is not rewritten); AGENT-02 added no collision
        self.assertEqual(sorted({i for i in ids if ids.count(i) > 1}), ["R23", "R24", "R25"])
        self.assertIn("Limit:", next(l for l in lines if l.startswith("| R58 |")))
        self.assertIn("Limit:", next(l for l in lines if l.startswith("| R59 |")))
        self.assertIn("does not authorize", next(l for l in lines if l.startswith("| R60 |")))


if __name__ == "__main__":
    unittest.main()
