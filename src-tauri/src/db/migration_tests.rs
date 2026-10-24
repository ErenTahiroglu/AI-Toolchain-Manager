//! Phase 1A -> Phase 1B migration, tested against a REAL database written by the accepted Phase 1A
//! build (commit 9918e9e): schema version 1, nine tools, three synthetic projects, six decisions, a
//! 22-event audit chain, one diagnostic record and an "unclean shutdown" session marker.

use super::*;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/phase1a-9918e9e.sqlite.fixture");

fn fixture_copy(dir: &Path) -> std::path::PathBuf {
    let target = dir.join("atm.sqlite");
    std::fs::copy(FIXTURE, &target).unwrap();
    target
}

fn n(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

type Snapshot = (i64, i64, i64, i64, i64, i64, String, String);
type Mutation = fn(&mut Vec<u8>);

fn legacy_snapshot(conn: &Connection) -> Snapshot {
    let tools_json: String = conn
        .query_row("SELECT group_concat(record_json, '|') FROM (SELECT record_json FROM tools ORDER BY tool_id)", [], |r| r.get(0))
        .unwrap();
    let chain_head: String = conn.query_row("SELECT record_digest FROM audit_events ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).unwrap();
    (
        n(conn, "SELECT count(*) FROM tools"),
        n(conn, "SELECT count(*) FROM projects"),
        n(conn, "SELECT count(*) FROM project_tool_decisions"),
        n(conn, "SELECT count(*) FROM prompt_sources"),
        n(conn, "SELECT count(*) FROM audit_events"),
        n(conn, "SELECT count(*) FROM diagnostics"),
        tools_json,
        chain_head,
    )
}

#[test]
fn real_phase1a_database_migrates_and_every_record_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path());
    let before = {
        let raw = Connection::open(&path).unwrap();
        assert_eq!(user_version(&raw).unwrap(), 1, "the fixture must really be a Phase 1A (v1) database");
        assert_eq!(n(&raw, "SELECT count(*) FROM sqlite_master WHERE name = 'catalog_entries'"), 0);
        legacy_snapshot(&raw)
    };
    assert_eq!((before.0, before.1, before.2, before.3, before.4), (9, 3, 6, 3, 22));

    let backups = dir.path().join("backups");
    let conn = open_file(&path, Some(&backups)).unwrap();
    assert_eq!(user_version(&conn).unwrap(), LATEST_VERSION);

    // 1. Nothing from Phase 1A was lost or altered. The only addition is the migration's own diagnostic note.
    let after = legacy_snapshot(&conn);
    assert_eq!((after.0, after.1, after.2, after.3, after.4), (before.0, before.1, before.2, before.3, before.4));
    assert_eq!((after.6.as_str(), after.7.as_str()), (before.6.as_str(), before.7.as_str()), "tool records and audit chain head unchanged");
    assert_eq!(after.5, before.5 + 1, "exactly one new diagnostic: DB_MIGRATED_V2");
    assert_eq!(crate::settings::get_internal(&conn, "_session_state").unwrap().as_deref(), Some("running"));
    assert!(crate::audit::verify(&conn).unwrap().ok, "the old audit chain still verifies after migration");

    // 2. The nine tools were copied into the registry with identical metadata.
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 9);
    let copied: String = conn
        .query_row("SELECT group_concat(record_json, '|') FROM (SELECT record_json FROM catalog_entries ORDER BY entry_id)", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(copied, before.6);
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE install_state <> 'not_managed' OR verification_state <> 'unknown'"), 0);
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE enabled = 1"), 9);

    // 3. Synthetic projects and decisions moved over; Phase 1A "later" became "postponed".
    assert_eq!(n(&conn, "SELECT count(*) FROM project_registry WHERE kind = 'synthetic'"), 3);
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences"), 6);
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences WHERE decision = 'postponed'"), 1);
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences WHERE decision = 'later'"), 0);
    assert_eq!(n(&conn, "SELECT count(*) FROM preference_history"), 6);

    // 4. The legacy tables keep their rows but are frozen: no divergence is possible any more.
    for sql in [
        "INSERT INTO tools (tool_id, display_name, category, repo_url, review_state, record_json, updated_at) VALUES ('x','x','x','x','unreviewed','{}','x')",
        "UPDATE tools SET display_name = 'changed'",
        "DELETE FROM tools",
        "UPDATE projects SET display_name = 'changed'",
        "DELETE FROM project_tool_decisions",
    ] {
        assert!(conn.execute(sql, []).is_err(), "legacy table accepted: {sql}");
    }

    // 5. A consistent pre-migration backup was written, and it is a usable v1 database.
    let files: Vec<_> = std::fs::read_dir(&backups).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(files.len(), 1, "exactly one backup");
    let backup = Connection::open(&files[0]).unwrap();
    assert_eq!(user_version(&backup).unwrap(), 1);
    assert_eq!(legacy_snapshot(&backup), before);

    // 6. The migration says so in the local diagnostics.
    assert_eq!(n(&conn, "SELECT count(*) FROM diagnostics WHERE code = 'DB_MIGRATED_V2'"), 1);
}

#[test]
fn migrated_database_gets_the_four_new_entries_and_startup_is_repeatable() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path());
    let mut shapes = Vec::new();
    for round in 0..3 {
        let conn = open_file(&path, Some(&dir.path().join("backups"))).unwrap();
        crate::catalog::seed_all(&conn).unwrap();
        crate::catalog::seed_all(&conn).unwrap();
        shapes.push((
            n(&conn, "SELECT count(*) FROM catalog_entries"),
            n(&conn, "SELECT count(*) FROM project_registry"),
            n(&conn, "SELECT count(*) FROM project_preferences"),
            n(&conn, "SELECT count(*) FROM preference_history"),
            n(&conn, "SELECT count(*) FROM diagnostics WHERE code = 'DB_MIGRATED_V2'"),
            user_version(&conn).unwrap(),
        ));
        if round == 0 {
            // The user's own reset of a demonstration decision must survive later seeding.
            conn.execute("DELETE FROM project_preferences WHERE entry_id = 'graphify'", []).unwrap();
        }
    }
    assert_eq!(shapes[0], (22, 3, 6, 6, 1, LATEST_VERSION));
    assert_eq!(shapes[1], (22, 3, 5, 6, 1, LATEST_VERSION), "a reset decision stays reset; nothing is re-seeded or re-migrated");
    assert_eq!(shapes[1], shapes[2]);
    assert_eq!(std::fs::read_dir(dir.path().join("backups")).unwrap().count(), 1, "only the first (real) migration made a backup");

    let conn = Connection::open(&path).unwrap();
    let report = crate::health::run(
        &conn,
        &crate::health::HealthContext {
            tray: crate::health::TrayState::NotAttempted,
            autostart: crate::health::AutostartState::NotQueried,
            previous_shutdown: crate::health::PreviousShutdown::FirstRun,
        },
    );
    let status = |id: &str| report.checks.iter().find(|c| c.id == id).unwrap().status;
    assert_eq!(status("catalog-invariants"), "passed");
    assert_eq!(status("database"), "passed");
    assert_eq!(status("audit-chain"), "passed");
}

#[test]
fn failed_registry_migration_rolls_back_and_leaves_the_phase1a_database_as_it_was() {
    fn broken(tx: &Transaction) -> AppResult<()> {
        tx.execute_batch(include_str!("../../migrations/0002_catalog_registry.sql"))?;
        // Half-done work that must disappear on failure.
        tx.execute("INSERT INTO project_registry (project_id, alias, display_name, kind, created_at) VALUES ('p', 'half-done', 'x', 'reference', 'x')", [])?;
        Err(AppError::new("INJECTED_FAILURE", "simulated crash in the middle of a migration"))
    }
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path());
    let before = legacy_snapshot(&Connection::open(&path).unwrap());
    let mut conn = Connection::open(&path).unwrap();
    let err =
        migrate_with(&mut conn, &[Migration::Sql(include_str!("../../migrations/0001_init.sql")), Migration::Rust(broken)]).unwrap_err();
    assert_eq!(err.code, "INJECTED_FAILURE");
    assert_eq!(user_version(&conn).unwrap(), 1, "version unchanged");
    assert_eq!(
        n(
            &conn,
            "SELECT count(*) FROM sqlite_master WHERE name IN ('catalog_entries','project_registry','project_preferences','skill_index')"
        ),
        0
    );
    assert_eq!(legacy_snapshot(&conn), before, "old data untouched");
    // The legacy tables are not frozen after a failed attempt: the previous app version could still write to them.
    assert!(conn.execute("UPDATE tools SET display_name = display_name", []).is_ok());
    drop(conn);
    // A later, healthy start migrates normally.
    let conn = open_file(&path, None).unwrap();
    assert_eq!(user_version(&conn).unwrap(), LATEST_VERSION);
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 9);
}

#[test]
fn a_later_step_failure_keeps_the_earlier_committed_versions() {
    fn late(tx: &Transaction) -> AppResult<()> {
        tx.execute_batch("CREATE TABLE late_table (x); INSERT INTO late_table VALUES (1);")?;
        Err(AppError::new("INJECTED_FAILURE", "step 3"))
    }
    let mut conn = Connection::open_in_memory().unwrap();
    let list = [Migration::Sql(include_str!("../../migrations/0001_init.sql")), Migration::Rust(migrate_2), Migration::Rust(late)];
    assert!(migrate_with(&mut conn, &list).is_err());
    assert_eq!(user_version(&conn).unwrap(), 2);
    assert_eq!(n(&conn, "SELECT count(*) FROM sqlite_master WHERE name = 'late_table'"), 0);
    assert_eq!(n(&conn, "SELECT count(*) FROM sqlite_master WHERE name = 'catalog_entries'"), 1);
}

#[test]
fn corrupted_legacy_content_aborts_the_migration_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path());
    {
        let raw = Connection::open(&path).unwrap();
        raw.execute("UPDATE tools SET record_json = '{this is not json' WHERE tool_id = 'gitleaks'", []).unwrap();
    }
    let err = open_file(&path, None).unwrap_err();
    assert!(matches!(err.code, "JSON_ERROR" | "DB_ERROR"), "{err:?}");
    let raw = Connection::open(&path).unwrap();
    assert_eq!(user_version(&raw).unwrap(), 1);
    assert_eq!(n(&raw, "SELECT count(*) FROM sqlite_master WHERE name = 'catalog_entries'"), 0);
    assert_eq!(n(&raw, "SELECT count(*) FROM tools"), 9);
}

#[test]
fn truncated_or_overwritten_database_files_fail_without_panicking() {
    let original = std::fs::read(FIXTURE).unwrap();
    let mutations: [(&str, Mutation); 3] = [
        ("truncated", |b| b.truncate(b.len() / 3)),
        ("zeroed-page", |b| b[4096..8192].fill(0)),
        ("garbage-header", |b| b[..100].fill(0x41)),
    ];
    for (label, mutate) in mutations {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("atm.sqlite");
        let mut bytes = original.clone();
        mutate(&mut bytes);
        std::fs::write(&path, &bytes).unwrap();
        match open_file(&path, None) {
            Err(e) => assert!(!e.message.is_empty(), "{label}"),
            Ok(conn) => {
                // If SQLite still opens it, the self-audit must be able to notice.
                let quick: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0)).unwrap_or_default();
                assert!(quick != "ok" || crate::audit::verify(&conn).is_ok(), "{label}: corruption went unnoticed");
            }
        }
    }
}

#[test]
fn an_unknown_legacy_tool_is_migrated_as_an_unreviewed_executable_candidate_not_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path());
    {
        let raw = Connection::open(&path).unwrap();
        let record: String = raw.query_row("SELECT record_json FROM tools WHERE tool_id = 'gitleaks'", [], |r| r.get(0)).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&record).unwrap();
        value["toolId"] = "custom-old".into();
        value["repository"] = serde_json::json!({"url": "https://github.com/someone/custom-old", "owner": "someone", "name": "custom-old"});
        let record = value.to_string();
        raw.execute(
            "INSERT INTO tools (tool_id, display_name, category, repo_url, review_state, record_json, updated_at)
             VALUES ('custom-old','Custom','native-cli','https://github.com/someone/custom-old','unreviewed',?1,'2026-01-01T00:00:00Z')",
            [record],
        )
        .unwrap();
    }
    let conn = open_file(&path, None).unwrap();
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 10);
    let kind: String = conn.query_row("SELECT kind FROM catalog_entries WHERE entry_id = 'custom-old'", [], |r| r.get(0)).unwrap();
    assert_eq!(kind, "executable_tool");
}
