//! AGENT-02 (Issue #53): six more read-only project surfaces (`codex-vscode`, `cline-cli`, `cline-vscode`, `cline-jetbrains`, `kiro-cli`, `kiro-ide`) and migration 11.
//! Everything runs on in-memory or temporary databases; nothing is read from or written to a real application database, no agent is started or contacted, and no
//! network or process access happens here.

use crate::{audit, catalog, db, discovery, plan, projects};
use rusqlite::Connection;

const WIN: &str = "windows-x64";
const MAC: &str = "macos-arm64";
const OLD: [&str; 5] = ["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "codex-cli", "local-cli"];
const NEW: [&str; 6] = ["codex-vscode", "cline-cli", "cline-vscode", "cline-jetbrains", "kiro-cli", "kiro-ide"];

fn n(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

/// Every row of a table as text, in rowid order, so that "unchanged" means byte-for-byte unchanged.
fn rows(conn: &Connection, sql: &str) -> Vec<String> {
    let mut stmt = conn.prepare(sql).unwrap();
    let cols = stmt.column_count();
    stmt.query_map([], |r| {
        let mut parts = Vec::new();
        for i in 0..cols {
            parts.push(format!("{:?}", r.get_ref(i).unwrap()));
        }
        Ok(parts.join("|"))
    })
    .unwrap()
    .map(|r| r.unwrap())
    .collect()
}

/// (type, name) of every schema object except the rebuilt table's own scratch name, sorted.
fn schema_objects(conn: &Connection) -> Vec<String> {
    rows(conn, "SELECT type, name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name")
}

/// A database as the previous build (schema 10) left it, with real decisions on every old surface including `codex-cli`.
fn v10_database_with_decisions() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    db::migrate_with(&mut conn, &db::migrations_prefix(10)).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), 10);
    catalog::seed_all(&conn).unwrap();
    projects::add_reference(&conn, "proje-bir", "Benim sitem", "kisisel not").unwrap();
    projects::add_reference(&conn, "proje-iki", "Ikinci", "").unwrap();
    let set = |alias: &str, entry: &str, surface: &str, platform: &str, decision: &str, days: Option<i64>| {
        projects::set_preference(&conn, alias, entry, surface, platform, decision, days).unwrap();
    };
    set("proje-bir", "agent-browser", "local-cli", WIN, "accepted", None);
    set("proje-bir", "gitleaks", "local-cli", WIN, "rejected", None);
    set("proje-bir", "microsoft-playwright-mcp", "claude-code-terminal", WIN, "postponed", Some(5));
    set("proje-bir", "microsoft-playwright-mcp", "antigravity-ide", MAC, "rejected", None);
    set("proje-bir", "anthropic-frontend-design", "claude-desktop-code", MAC, "postponed", Some(9));
    set("proje-bir", "microsoft-playwright-mcp", "codex-cli", WIN, "postponed", Some(3));
    set("proje-bir", "apify-mcp-server", "codex-cli", MAC, "rejected", None);
    set("proje-iki", "agent-browser", "local-cli", MAC, "accepted", None);
    conn
}

const INSERT: &str = "INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint)
                      SELECT project_id, 'gitleaks', ?1, 'macos-arm64', 'rejected', 'x', 'x', 'x' FROM project_registry LIMIT 1";

#[test]
fn the_allowlists_are_the_old_five_plus_the_six_approved_ids_and_the_install_plan_list_is_untouched() {
    assert_eq!(
        projects::SURFACES,
        [
            "claude-code-terminal",
            "claude-desktop-code",
            "antigravity-ide",
            "codex-cli",
            "codex-vscode",
            "cline-cli",
            "cline-vscode",
            "cline-jetbrains",
            "kiro-cli",
            "kiro-ide",
            "local-cli"
        ]
    );
    for s in OLD.iter().chain(NEW.iter()) {
        assert!(projects::SURFACES.contains(s), "{s}");
    }
    assert_eq!(
        plan::SURFACES,
        ["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "antigravity-cli", "antigravity-2", "local-cli"]
    );
    for s in NEW {
        assert!(!plan::SURFACES.contains(&s), "{s} must not be an installer target");
    }
}

#[test]
fn migration_eleven_preserves_every_row_and_codex_cli_decision_byte_for_byte_and_only_widens_the_check() {
    let mut conn = v10_database_with_decisions();
    let prefs_before = rows(&conn, "SELECT * FROM project_preferences ORDER BY rowid");
    let codex_before = rows(&conn, "SELECT * FROM project_preferences WHERE agent_surface = 'codex-cli' ORDER BY rowid");
    let history_before = rows(&conn, "SELECT * FROM preference_history ORDER BY id");
    let audit_before = rows(&conn, "SELECT * FROM audit_events ORDER BY id");
    let discovery_before = rows(&conn, "SELECT * FROM discovery_observations ORDER BY id");
    let entries_before = rows(&conn, "SELECT entry_id, record_json FROM catalog_entries ORDER BY entry_id");
    let registry_before = rows(&conn, "SELECT * FROM project_registry ORDER BY rowid");
    let objects_before = schema_objects(&conn);
    let ddl_before: Vec<String> =
        rows(&conn, "SELECT name, sql FROM sqlite_master WHERE type = 'table' AND name NOT IN ('project_preferences') ORDER BY name");
    assert_eq!(codex_before.len(), 2);
    assert!(prefs_before.len() >= 8, "the eight decisions made here plus the application's own samples");
    // Before the migration the old table cannot hold any of the new ids.
    for s in NEW {
        assert!(conn.execute(INSERT, [s]).is_err(), "{s} must be refused by the schema-10 CHECK");
    }

    db::migrate(&mut conn).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), 11);
    assert_eq!(db::LATEST_VERSION, 11);

    assert_eq!(
        rows(&conn, "SELECT * FROM project_preferences ORDER BY rowid"),
        prefs_before,
        "every column of every row is unchanged and in the same order"
    );
    assert_eq!(
        rows(&conn, "SELECT * FROM project_preferences WHERE agent_surface = 'codex-cli' ORDER BY rowid"),
        codex_before,
        "Codex CLI decisions are unchanged"
    );
    assert_eq!(rows(&conn, "SELECT * FROM preference_history ORDER BY id"), history_before, "history is untouched");
    assert_eq!(rows(&conn, "SELECT * FROM audit_events ORDER BY id"), audit_before);
    assert_eq!(rows(&conn, "SELECT * FROM discovery_observations ORDER BY id"), discovery_before);
    assert_eq!(rows(&conn, "SELECT entry_id, record_json FROM catalog_entries ORDER BY entry_id"), entries_before);
    assert_eq!(rows(&conn, "SELECT * FROM project_registry ORDER BY rowid"), registry_before);
    assert_eq!(
        rows(&conn, "SELECT name, sql FROM sqlite_master WHERE type = 'table' AND name NOT IN ('project_preferences') ORDER BY name"),
        ddl_before,
        "no other table's definition changed"
    );
    assert_eq!(schema_objects(&conn), objects_before, "no table, index, trigger or view was added or removed");
    assert_eq!(n(&conn, "SELECT count(*) FROM sqlite_master WHERE name LIKE 'project_preferences%'"), 1, "no scratch table is left behind");
    // No row was created for any new surface.
    for s in NEW {
        assert_eq!(n(&conn, &format!("SELECT count(*) FROM project_preferences WHERE agent_surface = '{s}'")), 0, "{s}");
    }
    // The columns, primary key and the other CHECKs are the same: compare the DDL text without the surface list and the table name.
    let ddl: String = conn.query_row("SELECT sql FROM sqlite_master WHERE name = 'project_preferences'", [], |r| r.get(0)).unwrap();
    assert!(ddl.contains("PRIMARY KEY (project_id, entry_id, agent_surface, platform_target)"));
    assert!(
        ddl.contains("actual_install_state = 'not_managed'")
            && ddl.contains("actual_verification_state = 'unknown'")
            && ddl.contains("observed_scope = 'unknown'")
    );
}

#[test]
fn after_migration_eleven_every_old_and_new_id_is_accepted_and_every_other_spelling_is_refused() {
    let mut conn = v10_database_with_decisions();
    db::migrate(&mut conn).unwrap();
    // ONE row per surface; the unique key is (project, entry, surface, platform), and each uses a fresh surface.
    for s in OLD.iter().chain(NEW.iter()) {
        conn.execute(INSERT, [s]).unwrap_or_else(|e| panic!("{s}: {e}"));
    }
    for bad in [
        "codex-ide",
        "codex-desktop",
        "codex-desktop-experience",
        "codex-app",
        "cline-desktop",
        "cline",
        "kiro",
        "Kiro-IDE",
        "CLINE-CLI",
        "Codex-VSCode",
        "codex_vscode",
        "cline-vs-code",
        "kiro-ide ",
        " kiro-cli",
        "kiro-%",
        "cline-*",
        "vscode",
        "cursor",
        "windsurf",
        "deepseek",
        "groq",
        "grok",
        "gemini-cli",
        "antigravity-cli",
        "antigravity-2",
        "",
    ] {
        assert!(conn.execute(INSERT, [bad]).is_err(), "{bad:?} must be refused by the exact CHECK");
    }
    assert_eq!(rows(&conn, "PRAGMA foreign_key_check").len(), 0, "no foreign-key violation");
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
    assert_eq!(integrity, "ok");
}

#[test]
fn a_failing_migration_eleven_rolls_back_atomically_and_leaves_the_version_and_the_old_table_untouched() {
    let mut conn = v10_database_with_decisions();
    let before = rows(&conn, "SELECT * FROM project_preferences ORDER BY rowid");
    let objects = schema_objects(&conn);
    fn broken(tx: &rusqlite::Transaction) -> crate::error::AppResult<()> {
        tx.execute_batch(include_str!("../migrations/0011_agent02_surfaces.sql"))?;
        // the rebuild really happened inside the transaction ...
        let wide: i64 =
            tx.query_row("SELECT count(*) FROM sqlite_master WHERE name = 'project_preferences' AND sql LIKE '%kiro-ide%'", [], |r| {
                r.get(0)
            })?;
        assert_eq!(wide, 1);
        // ... and then the step fails
        Err(crate::error::AppError::new("INJECTED_FAILURE", "crash after the DDL"))
    }
    let mut list = db::migrations_prefix(10);
    list.push(db::Migration::Rust(broken));
    assert_eq!(db::migrate_with(&mut conn, &list).unwrap_err().code, "INJECTED_FAILURE");
    assert_eq!(db::user_version(&conn).unwrap(), 10, "schema version unchanged after the rollback");
    assert_eq!(rows(&conn, "SELECT * FROM project_preferences ORDER BY rowid"), before, "the old table is exactly as it was");
    assert_eq!(schema_objects(&conn), objects, "no scratch table survived");
    assert!(conn.execute(INSERT, ["kiro-ide"]).is_err(), "the old, narrower CHECK is still in force");

    // The real migration still applies cleanly afterwards, and re-running it changes nothing.
    db::migrate(&mut conn).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), 11);
    assert_eq!(rows(&conn, "SELECT * FROM project_preferences ORDER BY rowid"), before);
    db::migrate(&mut conn).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), 11, "an already migrated database is not migrated again");
    assert_eq!(rows(&conn, "SELECT * FROM project_preferences ORDER BY rowid"), before);
}

#[test]
fn a_file_database_is_backed_up_then_migrated_reopened_without_repeating_and_a_newer_database_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atm.sqlite");
    let backups = dir.path().join("backups");
    {
        let mut conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        db::migrate_with(&mut conn, &db::migrations_prefix(10)).unwrap();
        catalog::seed_all(&conn).unwrap();
        projects::add_reference(&conn, "proje-bir", "Benim sitem", "").unwrap();
        projects::set_preference(&conn, "proje-bir", "microsoft-playwright-mcp", "codex-cli", WIN, "postponed", Some(3)).unwrap();
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())).ok();
    }
    let conn = db::open_file(&path, Some(&backups)).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), 11);
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences WHERE agent_surface = 'codex-cli' AND decision = 'postponed'"), 1);
    let snapshots: Vec<_> = std::fs::read_dir(&backups).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(snapshots.len(), 1, "one pre-migration snapshot");
    assert!(snapshots[0].starts_with("atm-v10-"), "{snapshots:?}");
    drop(conn);
    // opening it again does not migrate or snapshot again
    let again = db::open_file(&path, Some(&backups)).unwrap();
    assert_eq!(db::user_version(&again).unwrap(), 11);
    assert_eq!(std::fs::read_dir(&backups).unwrap().count(), 1);
    drop(again);

    // A database written by a NEWER build (schema 12) is refused and not modified: an older reader never opens a newer schema.
    let future = dir.path().join("future.sqlite");
    {
        let c = Connection::open(&future).unwrap();
        c.execute_batch("CREATE TABLE marker(x); PRAGMA user_version = 12;").unwrap();
        c.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())).ok();
    }
    let bytes = std::fs::read(&future).unwrap();
    assert_eq!(db::open_file(&future, None).unwrap_err().code, "DB_SCHEMA_TOO_NEW");
    assert_eq!(std::fs::read(&future).unwrap(), bytes);
}

#[test]
fn a_fresh_database_has_the_wide_check_and_the_backend_stores_a_preference_per_surface_without_leaking_to_a_sibling() {
    let conn = db::open_memory().unwrap();
    catalog::seed_all(&conn).unwrap();
    projects::add_reference(&conn, "proje-bir", "Benim sitem", "").unwrap();
    let baseline = n(&conn, "SELECT count(*) FROM project_preferences");
    for (i, s) in NEW.iter().enumerate() {
        let stored = projects::set_preference(&conn, "proje-bir", "microsoft-playwright-mcp", s, WIN, "rejected", None).unwrap();
        assert_eq!((stored.agent_surface.as_str(), stored.decision.as_str()), (*s, "rejected"));
        assert_eq!(stored.compatibility, "unknown", "no publisher evidence row exists for {s}: unknown stays unknown");
        assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences"), baseline + i as i64 + 1);
    }
    // each new surface holds exactly its own one row (the sibling relationships are exercised, with isolated fixtures, in the two tests below)
    for s in NEW {
        assert_eq!(n(&conn, &format!("SELECT count(*) FROM project_preferences WHERE agent_surface = '{s}'")), 1, "{s}");
    }
    assert!(audit::verify(&conn).unwrap().ok);
}

/// Sibling surfaces of one client family; every ordered pair (a, b) with a != b is exercised.
const FAMILIES: [&[&str]; 3] =
    [&["codex-cli", "codex-vscode"], &["cline-cli", "cline-vscode", "cline-jetbrains"], &["kiro-cli", "kiro-ide"]];

fn sibling_pairs() -> Vec<(&'static str, &'static str)> {
    let mut pairs = Vec::new();
    for family in FAMILIES {
        for a in family {
            for b in family {
                if a != b {
                    pairs.push((*a, *b));
                }
            }
        }
    }
    assert_eq!(pairs.len(), 2 + 6 + 2);
    pairs
}

/// Alias of the test project. It differs from the sample projects that `catalog::seed_all` creates (`ornek-*`), which already hold demonstration preferences.
const ALIAS: &str = "agent02-deneme";
const ENTRY: &str = "microsoft-playwright-mcp";

/// Every case gets its own in-memory database. `catalog::seed_all` writes sample projects with their own preferences and history rows, so nothing here assumes
/// that the global tables are empty. The test project is new, its own preferences are checked by exact project id, and global totals are compared with a recorded baseline.
struct Fixture {
    conn: Connection,
    project_id: String,
    /// Snapshot of every preference row that does NOT belong to the test project (the seeded samples).
    others_before: Vec<String>,
    prefs_before: i64,
    history_before: i64,
    audit_before: usize,
}

fn isolated_fixture() -> Fixture {
    let conn = db::open_memory().unwrap();
    catalog::seed_all(&conn).unwrap();
    projects::add_reference(&conn, ALIAS, "Agent02 deneme", "").unwrap();
    let project_id: String = conn.query_row("SELECT project_id FROM project_registry WHERE alias = ?1", [ALIAS], |r| r.get(0)).unwrap();
    let fixture = Fixture {
        others_before: rows(&conn, &format!("SELECT * FROM project_preferences WHERE project_id <> '{project_id}' ORDER BY rowid")),
        prefs_before: n(&conn, "SELECT count(*) FROM project_preferences"),
        history_before: n(&conn, "SELECT count(*) FROM preference_history"),
        audit_before: audit::list_ascending(&conn).unwrap().len(),
        project_id,
        conn,
    };
    assert_eq!(fixture.project_prefs(), 0, "the new project starts without any preference");
    assert_eq!(n(&fixture.conn, &format!("SELECT count(*) FROM preference_history WHERE project_id = '{}'", fixture.project_id)), 0);
    assert_eq!(fixture.history_total(), fixture.history_before);
    assert_eq!(fixture.audit_events().len(), fixture.audit_before);
    fixture
}

impl Fixture {
    fn project_prefs(&self) -> i64 {
        n(&self.conn, &format!("SELECT count(*) FROM project_preferences WHERE project_id = '{}'", self.project_id))
    }

    /// All rows of the test project, as text.
    fn project_rows(&self) -> Vec<String> {
        rows(&self.conn, &format!("SELECT * FROM project_preferences WHERE project_id = '{}' ORDER BY rowid", self.project_id))
    }

    /// The rows of one exact composite key (project, entry, surface, platform), as text.
    fn key_rows(&self, surface: &str) -> Vec<String> {
        rows(
            &self.conn,
            &format!(
                "SELECT * FROM project_preferences WHERE project_id = '{}' AND entry_id = '{ENTRY}' AND agent_surface = '{surface}' AND platform_target = '{WIN}' ORDER BY rowid",
                self.project_id
            ),
        )
    }

    fn history_rows(&self, action: &str, surface: &str) -> i64 {
        n(
            &self.conn,
            &format!(
                "SELECT count(*) FROM preference_history WHERE project_id = '{}' AND entry_id = '{ENTRY}' AND agent_surface = '{surface}' AND platform_target = '{WIN}' AND action = '{action}'",
                self.project_id
            ),
        )
    }

    fn history_total(&self) -> i64 {
        n(&self.conn, "SELECT count(*) FROM preference_history")
    }

    fn audit_events(&self) -> Vec<serde_json::Value> {
        audit::list_ascending(&self.conn).unwrap()
    }

    fn assert_seeded_samples_untouched(&self, why: &str) {
        let now = rows(&self.conn, &format!("SELECT * FROM project_preferences WHERE project_id <> '{}' ORDER BY rowid", self.project_id));
        assert_eq!(now, self.others_before, "{why}: the preferences of the sample projects are unchanged");
    }
}

/// Case A: the sibling B has no preference, so resetting B reports PREFERENCE_NOT_FOUND and changes nothing, including A.
#[test]
fn resetting_a_sibling_surface_that_has_no_preference_is_refused_and_leaves_the_other_surface_untouched() {
    for (a, b) in sibling_pairs() {
        let f = isolated_fixture();
        projects::set_preference(&f.conn, ALIAS, ENTRY, a, WIN, "postponed", Some(4)).unwrap();
        assert!(f.key_rows(b).is_empty(), "{a}/{b}: B is absent under the exact composite key");
        assert!(projects::stored_preference(&f.conn, ALIAS, ENTRY, b, WIN).unwrap().is_none());
        assert_eq!(f.project_prefs(), 1, "{a}/{b}: the project holds only A");
        assert_eq!(f.key_rows(a).len(), 1);
        assert_eq!(n(&f.conn, "SELECT count(*) FROM project_preferences"), f.prefs_before + 1, "{a}/{b}: baseline + A");

        // state after the setup and before the failing reset
        let a_before = f.key_rows(a);
        let project_before = f.project_rows();
        let history_before = f.history_total();
        let events_before = f.audit_events();

        let err = projects::reset_preference(&f.conn, ALIAS, ENTRY, b, WIN).unwrap_err();
        assert_eq!(err.code, "PREFERENCE_NOT_FOUND", "{a}/{b}");

        assert_eq!(f.key_rows(a), a_before, "{a}/{b}: A is byte-identical");
        assert_eq!(f.project_rows(), project_before, "{a}/{b}: the rows of the project are unchanged by the failed reset");
        assert_eq!(
            n(&f.conn, "SELECT count(*) FROM project_preferences"),
            f.prefs_before + 1,
            "{a}/{b}: the global count is unchanged by the failed reset"
        );
        assert_eq!(f.history_total(), history_before, "{a}/{b}: no history row for a failed reset");
        assert_eq!(f.history_rows("reset", a) + f.history_rows("reset", b), 0, "{a}/{b}: no reset history at all");
        assert_eq!(f.audit_events(), events_before, "{a}/{b}: no audit event for a failed reset");
        f.assert_seeded_samples_untouched(&format!("{a}/{b}"));
        assert!(audit::verify(&f.conn).unwrap().ok);
    }
}

/// Case B: both A and B hold distinct preferences. Resetting B succeeds, removes only B and leaves A byte-for-byte as it was. It records one B-specific history row
/// and one `reset_rejection` audit event whose fields follow the production contract in `projects::reset_preference`.
#[test]
fn resetting_an_existing_sibling_preference_succeeds_removes_only_that_surface_and_keeps_the_other_intact() {
    for (a, b) in sibling_pairs() {
        let f = isolated_fixture();
        let stored_a = projects::set_preference(&f.conn, ALIAS, ENTRY, a, WIN, "postponed", Some(4)).unwrap();
        let stored_b = projects::set_preference(&f.conn, ALIAS, ENTRY, b, WIN, "rejected", None).unwrap();
        assert_eq!((stored_a.agent_surface.as_str(), stored_a.decision.as_str()), (a, "postponed"));
        assert!(stored_a.remind_at.is_some(), "{a}/{b}: A carries a reminder");
        assert_eq!((stored_b.agent_surface.as_str(), stored_b.decision.as_str()), (b, "rejected"));
        assert_eq!(f.project_prefs(), 2, "{a}/{b}: exactly A and B in the project");
        assert_eq!(f.key_rows(a).len(), 1);
        assert_eq!(f.key_rows(b).len(), 1);
        assert_eq!(n(&f.conn, "SELECT count(*) FROM project_preferences"), f.prefs_before + 2, "{a}/{b}: baseline + 2");

        let a_before = f.key_rows(a);
        let history_before = f.history_total();
        let events_before = f.audit_events();
        assert_eq!(f.history_rows("reset", b), 0);

        projects::reset_preference(&f.conn, ALIAS, ENTRY, b, WIN).unwrap();

        assert!(f.key_rows(b).is_empty(), "{a}/{b}: B is removed");
        assert!(projects::stored_preference(&f.conn, ALIAS, ENTRY, b, WIN).unwrap().is_none());
        assert_eq!(f.key_rows(a), a_before, "{a}/{b}: A (decision, timestamps, reminder, fingerprint, install state) is byte-identical");
        let kept = projects::stored_preference(&f.conn, ALIAS, ENTRY, a, WIN).unwrap().unwrap();
        assert_eq!(kept.decision, "postponed");
        assert!(kept.remind_at.is_some());
        assert_eq!(f.project_prefs(), 1, "{a}/{b}: only A is left in the project");
        assert_eq!(n(&f.conn, "SELECT count(*) FROM project_preferences"), f.prefs_before + 1, "{a}/{b}: baseline + 2 - 1");
        f.assert_seeded_samples_untouched(&format!("{a}/{b}"));

        // history: exactly one new row, and it is the reset of B (never of A)
        assert_eq!(f.history_total(), history_before + 1, "{a}/{b}");
        assert_eq!(f.history_rows("reset", b), 1, "{a}/{b}: one reset row for B");
        assert_eq!(f.history_rows("reset", a), 0, "{a}/{b}: no reset row for A");

        // audit: exactly one new event, found relative to the baseline, with the fields of the production contract
        let events = f.audit_events();
        assert_eq!(events.len(), events_before.len() + 1, "{a}/{b}: one new audit event");
        assert_eq!(&events[..events_before.len()], &events_before[..], "{a}/{b}: earlier events are unchanged");
        let e = &events[events_before.len()];
        assert_eq!(e["eventType"], "reset_rejection", "{a}/{b}");
        assert_eq!(e["outcome"], "passed", "{a}/{b}");
        assert_eq!(e["actorType"], "user", "{a}/{b}");
        assert_eq!(e["toolId"], ENTRY, "{a}/{b}");
        assert_eq!(e["projectAlias"], ALIAS, "{a}/{b}");
        // `projects::audit_target` maps only claude-code-terminal and antigravity-ide. Every AGENT-02 surface is therefore "unknown" (the mapping gained no new surface).
        assert_eq!(e["targetId"], "unknown", "{a}/{b}");
        let report = audit::verify(&f.conn).unwrap();
        assert!(report.ok, "{a}/{b}: {:?}", report.problem);
        assert_eq!(report.count as usize, events.len());

        // the repeated reset is the documented missing-preference error and changes nothing
        let again = projects::reset_preference(&f.conn, ALIAS, ENTRY, b, WIN).unwrap_err();
        assert_eq!(again.code, "PREFERENCE_NOT_FOUND", "{a}/{b}");
        assert_eq!(f.audit_events().len(), events.len(), "{a}/{b}: the failed second reset adds no audit event");
        assert_eq!(f.history_total(), history_before + 1, "{a}/{b}: and no history row");
        assert_eq!(f.key_rows(a), a_before);
        assert!(audit::verify(&f.conn).unwrap().ok);
    }
}

#[test]
fn acceptance_for_an_unknown_compatibility_pair_is_refused_on_every_new_surface() {
    let conn = db::open_memory().unwrap();
    catalog::seed_all(&conn).unwrap();
    projects::add_reference(&conn, "proje-bir", "Benim sitem", "").unwrap();
    let baseline = n(&conn, "SELECT count(*) FROM project_preferences");
    let entries = catalog::list_catalog(&conn).unwrap();
    assert_eq!(entries.len(), catalog::SEED_ENTRY_COUNT);
    for s in NEW {
        for e in &entries {
            for platform in [WIN, MAC] {
                assert_eq!(
                    catalog::compatibility(e, platform, s),
                    "unknown",
                    "{} on {platform}/{s}: no evidence row may be implied",
                    e.entry_id
                );
                let err = projects::set_preference(&conn, "proje-bir", &e.entry_id, s, platform, "accepted", None).unwrap_err();
                assert!(
                    ["PREFERENCE_NOT_ACTIVATABLE", "ACTIVATION_BLOCKED", "ENTRY_DISABLED"].contains(&err.code),
                    "{} on {platform}/{s}: accepted must be refused, got {}",
                    e.entry_id,
                    err.code
                );
            }
        }
    }
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences"), baseline, "nothing stored");
}

#[test]
fn unlisted_surfaces_aliases_desktops_and_providers_are_refused_by_the_backend() {
    let conn = db::open_memory().unwrap();
    catalog::seed_all(&conn).unwrap();
    projects::add_reference(&conn, "proje-bir", "Benim sitem", "").unwrap();
    let baseline = n(&conn, "SELECT count(*) FROM project_preferences");
    for bad in [
        "codex-ide",
        "codex-desktop-experience",
        "codex-app",
        "cline-desktop",
        "cline",
        "kiro",
        "Cline-CLI",
        "KIRO-IDE",
        "cursor",
        "windsurf",
        "deepseek",
        "groq",
        "grok",
        "gemini-api",
        "",
    ] {
        let err = projects::set_preference(&conn, "proje-bir", "microsoft-playwright-mcp", bad, WIN, "postponed", Some(1)).unwrap_err();
        assert_eq!(err.code, "SURFACE_INVALID", "{bad:?}");
    }
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences"), baseline);
}

#[test]
fn no_new_surface_reaches_the_install_plan_layer() {
    let conn = db::open_memory().unwrap();
    catalog::seed_all(&conn).unwrap();
    let host = plan::HostFacts { platform: WIN.to_string(), elevated: Some(false) };
    for s in NEW.iter().chain(["codex-cli"].iter()) {
        let r = plan::PlanRequest {
            entry_id: "gitleaks".to_string(),
            platform_target: WIN.to_string(),
            agent_surface: s.to_string(),
            scope: "user".to_string(),
            method_id: None,
            project_alias: None,
        };
        assert_eq!(plan::plan_for(&conn, &r, &host).unwrap_err().code, "PLAN_REQUEST_INVALID", "{s}");
    }
}

#[test]
fn no_new_surface_has_a_declaration_detector_or_a_discovery_row() {
    for s in NEW {
        assert!(!discovery::detect::has_declaration_detector("codebase-memory-mcp", s), "{s}");
        assert!(!discovery::detect::has_declaration_detector("microsoft-playwright-mcp", s), "{s}");
    }
    // the discovery table keeps its own four-surface CHECK: a new surface can never be stored as an observation
    let conn = db::open_memory().unwrap();
    let ddl: String = conn.query_row("SELECT sql FROM sqlite_master WHERE name = 'discovery_observations'", [], |r| r.get(0)).unwrap();
    assert!(ddl.contains("agent_surface IN ('claude-code-terminal', 'claude-desktop-code', 'antigravity-ide', 'local-cli')"), "{ddl}");
    for s in NEW {
        assert!(!ddl.contains(s), "{s}");
    }
}

#[test]
fn the_recommendation_backend_accepts_a_new_surface_only_as_a_project_target_and_still_refuses_unknown_ones() {
    // `recommend::validate` uses projects::SURFACES: a new surface is a valid TARGET for a suggestion (no execution), anything else is refused.
    let src = include_str!("recommend.rs");
    assert!(src.contains("projects::SURFACES.contains(&input.agent_surface.as_str())"));
    assert!(!src.contains("plan::SURFACES"));
}

#[test]
fn the_new_surfaces_are_absent_from_every_executable_path() {
    // Source-level pins for the install, recipe and discovery layers (the configuration-write layers are covered behaviourally in their own test files and by tests/test_agent02_contract.py). A pin that passed because something else was missing would prove nothing, so the behavioural tests above are the primary guard.
    for (name, src) in [
        ("plan.rs", include_str!("plan.rs")),
        ("recipe.rs", include_str!("recipe.rs")),
        ("install_flow.rs", include_str!("install_flow.rs")),
        ("discovery/detect.rs", include_str!("discovery/detect.rs")),
        ("discovery/store.rs", include_str!("discovery/store.rs")),
    ] {
        for s in NEW {
            assert!(!src.contains(&format!("\"{s}\"")), "{name} must not name {s}");
        }
    }
}
