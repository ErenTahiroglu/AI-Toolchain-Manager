//! Phase 1B closure tests for the two independent Red Team findings (GitHub Issue #3):
//! RT-1B-01 project-scoped suggestion de-duplication and RT-1B-02 immutable classification of restored entries.

use crate::{
    audit, catalog, db,
    health::{self, AutostartState, HealthContext, PreviousShutdown, TrayState},
    projects,
    recommend::{self, Evaluation, TaskInput, FRONTEND_DESIGN},
};
use rusqlite::Connection;
use std::collections::BTreeSet;

const THREE_HITS: &str = "Design a React component, test it in the browser, and find a reusable skill for charts";

fn setup() -> Connection {
    let conn = db::open_memory().unwrap();
    catalog::seed_all(&conn).unwrap();
    projects::add_reference(&conn, "proje-a", "Proje A", "").unwrap();
    projects::add_reference(&conn, "proje-b", "Proje B", "").unwrap();
    conn
}

fn input(project: &str, task_id: &str, text: &str) -> TaskInput {
    TaskInput {
        project_alias: project.into(),
        task_id: task_id.into(),
        task_text: text.into(),
        evidence_files: vec!["Chart.tsx".into()],
        agent_surface: "claude-code-terminal".into(),
        platform_target: "windows-x64".into(),
        phase: None,
        availability: vec![],
    }
}

fn shown(e: &Evaluation) -> BTreeSet<String> {
    e.recommendations.iter().map(|r| r.entry_id.clone()).collect()
}

fn reason(e: &Evaluation, entry: &str) -> Option<&'static str> {
    e.suppressed.iter().find(|s| s.entry_id == entry).map(|s| s.reason_code)
}

fn n(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn updated(conn: &Connection, entry_id: &str) -> String {
    conn.query_row("SELECT updated_at FROM catalog_entries WHERE entry_id = ?1", [entry_id], |r| r.get(0)).unwrap()
}

fn ctx() -> HealthContext {
    HealthContext { tray: TrayState::NotAttempted, autostart: AutostartState::NotQueried, previous_shutdown: PreviousShutdown::FirstRun }
}

fn health_status(conn: &Connection, id: &str) -> &'static str {
    health::run(conn, &ctx()).checks.iter().find(|c| c.id == id).unwrap().status
}

// ------------------------------------------------------------------------------------------------
// RT-1B-01: project-scoped de-duplication
// ------------------------------------------------------------------------------------------------

#[test]
fn the_same_task_id_in_two_projects_is_two_independent_tasks() {
    let conn = setup();
    let text = "Design a React component";
    let a = recommend::evaluate(&conn, &input("proje-a", "ortak-gorev", text)).unwrap();
    assert_eq!(shown(&a), BTreeSet::from([FRONTEND_DESIGN.to_string()]));
    // Before the fix, project A's log silently suppressed this one.
    let b = recommend::evaluate(&conn, &input("proje-b", "ortak-gorev", text)).unwrap();
    assert_eq!(
        shown(&b),
        BTreeSet::from([FRONTEND_DESIGN.to_string()]),
        "project B must not inherit project A's state: {:?}",
        b.suppressed
    );
    // Each project is still told only once.
    assert_eq!(
        reason(&recommend::evaluate(&conn, &input("proje-a", "ortak-gorev", text)).unwrap(), FRONTEND_DESIGN),
        Some("already-shown-this-task")
    );
    assert_eq!(
        reason(&recommend::evaluate(&conn, &input("proje-b", "ortak-gorev", text)).unwrap(), FRONTEND_DESIGN),
        Some("already-shown-this-task")
    );
    assert_eq!(n(&conn, "SELECT count(*) FROM suggestion_log WHERE task_id = 'ortak-gorev'"), 2);
    assert_eq!(n(&conn, "SELECT count(DISTINCT project_id) FROM suggestion_log WHERE task_id = 'ortak-gorev'"), 2);
}

#[test]
fn the_two_suggestion_cap_is_per_project_and_task_not_shared() {
    let conn = setup();
    let a = recommend::evaluate(&conn, &input("proje-a", "t", THREE_HITS)).unwrap();
    assert_eq!(a.recommendations.len(), recommend::MAX_PER_TASK);
    assert!(a.suppressed.iter().any(|s| s.reason_code == "per-task-limit"));
    // Project B's first evaluation is not affected by A having used up "its" cap under the same task id.
    let b = recommend::evaluate(&conn, &input("proje-b", "t", THREE_HITS)).unwrap();
    assert_eq!(b.recommendations.len(), recommend::MAX_PER_TASK);
    assert_eq!(shown(&a), shown(&b), "same task text, same rules, same answer in each project");
    // And neither project can exceed the cap on repeated calls.
    for _ in 0..3 {
        assert!(recommend::evaluate(&conn, &input("proje-a", "t", THREE_HITS)).unwrap().recommendations.is_empty());
        assert!(recommend::evaluate(&conn, &input("proje-b", "t", THREE_HITS)).unwrap().recommendations.is_empty());
    }
    assert_eq!(n(&conn, "SELECT count(*) FROM suggestion_log WHERE task_id = 't'"), 2 * recommend::MAX_PER_TASK as i64);
}

#[test]
fn the_one_late_suggestion_rule_is_per_project() {
    let conn = setup();
    let late = |project: &str, text: &str| {
        let mut i = input(project, "t-late", text);
        i.phase = Some("late".into());
        recommend::evaluate(&conn, &i).unwrap()
    };
    assert_eq!(late("proje-a", "Now design the React button component").recommendations.len(), 1);
    assert_eq!(late("proje-b", "Now design the React button component").recommendations.len(), 1, "B's one late suggestion is its own");
    let a_again = late("proje-a", "Also test this in the browser");
    assert!(a_again.recommendations.is_empty());
    assert_eq!(reason(&a_again, "agent-browser"), Some("late-limit"));
}

#[test]
fn no_duplicate_within_one_project_and_task_even_after_many_calls() {
    let conn = setup();
    for _ in 0..6 {
        recommend::evaluate(&conn, &input("proje-a", "t", "Design a React component")).unwrap();
    }
    assert_eq!(n(&conn, "SELECT count(*) FROM suggestion_log WHERE task_id = 't'"), 1);
}

#[test]
fn the_database_key_allows_two_projects_but_refuses_a_duplicate_within_one() {
    let conn = setup();
    let ids: Vec<String> = conn
        .prepare("SELECT project_id FROM project_registry WHERE alias IN ('proje-a','proje-b') ORDER BY alias")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let insert = |project: &str| {
        conn.execute(
            "INSERT INTO suggestion_log (project_id, task_id, entry_id, phase, shown_at) VALUES (?1, 't', 'strix', 'start', 'x')",
            [project],
        )
    };
    assert!(insert(&ids[0]).is_ok());
    assert!(insert(&ids[1]).is_ok(), "same task and entry in another project is legal");
    assert!(insert(&ids[0]).is_err(), "same project, task and entry is a duplicate");
    assert!(conn
        .execute("INSERT INTO suggestion_log (project_id, task_id, entry_id, phase, shown_at) VALUES ('p', '', 'x', 'start', 'x')", [])
        .is_err());
    assert!(conn
        .execute("INSERT INTO suggestion_log (project_id, task_id, entry_id, phase, shown_at) VALUES ('p', 't', 'x', 'whenever', 'x')", [])
        .is_err());
    assert!(
        conn.execute("INSERT INTO suggestion_log (task_id, entry_id, phase, shown_at) VALUES ('t', 'x', 'start', 'x')", []).is_err(),
        "project is mandatory"
    );
}

#[test]
fn pruning_is_per_project_so_a_busy_project_cannot_evict_another_projects_state() {
    let conn = setup();
    let id = |alias: &str| -> String {
        conn.query_row("SELECT project_id FROM project_registry WHERE alias = ?1", [alias], |r| r.get(0)).unwrap()
    };
    let (a, b) = (id("proje-a"), id("proje-b"));
    for i in 0..650 {
        conn.execute(
            "INSERT INTO suggestion_log (project_id, task_id, entry_id, phase, shown_at) VALUES (?1, ?2, 'strix', 'start', 'x')",
            rusqlite::params![a, format!("old-{i}")],
        )
        .unwrap();
    }
    for i in 0..3 {
        conn.execute(
            "INSERT INTO suggestion_log (project_id, task_id, entry_id, phase, shown_at) VALUES (?1, ?2, 'strix', 'start', 'x')",
            rusqlite::params![b, format!("keep-{i}")],
        )
        .unwrap();
    }
    recommend::evaluate(&conn, &input("proje-a", "fresh", "Design a React component")).unwrap();
    let count =
        |p: &str| -> i64 { conn.query_row("SELECT count(*) FROM suggestion_log WHERE project_id = ?1", [p], |r| r.get(0)).unwrap() };
    assert!(count(&a) <= 502, "project A is bounded: {}", count(&a));
    assert_eq!(count(&b), 3, "project B's state is untouched");
    assert_eq!(reason(&recommend::evaluate(&conn, &input("proje-b", "keep-1", "Design a React component")).unwrap(), "strix"), None);
}

#[test]
fn concurrent_evaluations_from_two_connections_never_exceed_the_cap_or_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atm.sqlite");
    {
        let conn = db::open_file(&path, None).unwrap();
        catalog::seed_all(&conn).unwrap();
        projects::add_reference(&conn, "proje-a", "Proje A", "").unwrap();
    }
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let path = path.clone();
            std::thread::spawn(move || {
                let conn = db::open_file(&path, None).unwrap();
                (0..3)
                    .map(|_| recommend::evaluate(&conn, &input("proje-a", "race", THREE_HITS)).unwrap().recommendations.len())
                    .sum::<usize>()
            })
        })
        .collect();
    let total: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
    assert_eq!(total, recommend::MAX_PER_TASK, "exactly the cap was shown across all concurrent callers");
    let conn = db::open_file(&path, None).unwrap();
    assert_eq!(n(&conn, "SELECT count(*) FROM suggestion_log WHERE task_id = 'race'"), recommend::MAX_PER_TASK as i64);
    assert_eq!(n(&conn, "SELECT count(DISTINCT entry_id) FROM suggestion_log WHERE task_id = 'race'"), recommend::MAX_PER_TASK as i64);
}

// ---- migration v2 -> v3, against a REAL database written by the Phase 1B build c921932 ----------

const V2_FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/phase1b-c921932.sqlite.fixture");

fn rows(conn: &Connection, sql: &str) -> Vec<Vec<String>> {
    let mut stmt = conn.prepare(sql).unwrap();
    let cols = stmt.column_count();
    stmt.query_map([], |r| {
        (0..cols).map(|i| r.get::<_, rusqlite::types::Value>(i).map(|v| format!("{v:?}"))).collect::<Result<Vec<_>, _>>()
    })
    .unwrap()
    .map(|r| r.unwrap())
    .collect()
}

fn pk_columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn.prepare(&format!("SELECT name, pk FROM pragma_table_info('{table}') WHERE pk > 0 ORDER BY pk")).unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0)).unwrap().map(|r| r.unwrap()).collect()
}

#[test]
fn real_v2_database_migrates_to_v3_keeping_every_record_and_every_user_decision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atm.sqlite");
    std::fs::copy(V2_FIXTURE, &path).unwrap();
    let tables = [
        "catalog_entries",
        "project_registry",
        "project_preferences",
        "preference_history",
        "audit_events",
        "diagnostics",
        "settings",
        "skill_index",
    ];
    let (before, log_before, vsn) = {
        let raw = Connection::open(&path).unwrap();
        assert_eq!(db::user_version(&raw).unwrap(), 2, "the fixture must be a genuine v2 database");
        assert_eq!(pk_columns(&raw, "suggestion_log"), ["task_id", "entry_id"], "v2 carries the vulnerable key");
        let snap: Vec<_> = tables.iter().map(|t| rows(&raw, &format!("SELECT * FROM {t} ORDER BY 1, 2"))).collect();
        (snap, rows(&raw, "SELECT project_id, task_id, entry_id, phase, shown_at FROM suggestion_log ORDER BY 1, 2, 3"), 2)
    };
    assert!(!log_before.is_empty(), "the fixture holds real suggestion records to preserve");
    assert!(before[2].len() >= 7, "user decisions exist in the fixture");

    let conn = db::open_file(&path, Some(&dir.path().join("backups"))).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
    assert!(db::LATEST_VERSION > vsn);
    assert_eq!(pk_columns(&conn, "suggestion_log"), ["project_id", "task_id", "entry_id"]);
    assert_eq!(
        rows(&conn, "SELECT project_id, task_id, entry_id, phase, shown_at FROM suggestion_log ORDER BY 1, 2, 3"),
        log_before,
        "suggestion records preserved"
    );
    for (i, t) in tables.iter().enumerate() {
        assert_eq!(rows(&conn, &format!("SELECT * FROM {t} ORDER BY 1, 2")), before[i], "{t} changed during migration 3");
    }
    assert!(audit::verify(&conn).unwrap().ok, "the audit chain still verifies");
    let backups: Vec<_> = std::fs::read_dir(dir.path().join("backups")).unwrap().collect();
    assert_eq!(backups.len(), 1, "a consistent pre-migration backup exists");
    // The new rule is live after migration: the same task id in another project is independent.
    catalog::seed_all(&conn).unwrap();
    let aliases: Vec<String> = projects::list_projects(&conn).unwrap().into_iter().map(|p| p.alias).collect();
    assert!(aliases.contains(&"benim-sitem".to_string()));
    let first = recommend::evaluate(&conn, &input("ornek-web-sitesi", "gorev-mvcfewze-fa3xp", "Design a React component")).unwrap();
    assert_eq!(
        shown(&first),
        BTreeSet::from([FRONTEND_DESIGN.to_string()]),
        "the old task id, now used in another project, is not suppressed"
    );
    drop(conn);
    // Repeated start: nothing changes, no second backup.
    let conn = db::open_file(&path, Some(&dir.path().join("backups"))).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
    assert_eq!(std::fs::read_dir(dir.path().join("backups")).unwrap().count(), 1);
}

#[test]
fn a_failure_inside_migration_3_rolls_back_and_keeps_the_old_log() {
    fn broken(tx: &rusqlite::Transaction) -> crate::error::AppResult<()> {
        tx.execute_batch(include_str!("../migrations/0003_scoped_suggestion_log.sql"))?;
        Err(crate::error::AppError::new("INJECTED_FAILURE", "crash after the table swap"))
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atm.sqlite");
    std::fs::copy(V2_FIXTURE, &path).unwrap();
    let before = rows(&Connection::open(&path).unwrap(), "SELECT * FROM suggestion_log ORDER BY 1, 2, 3");
    let mut conn = Connection::open(&path).unwrap();
    let list = [
        db::Migration::Sql(include_str!("../migrations/0001_init.sql")),
        db::Migration::Rust(|tx| {
            // v2 is already applied in the fixture; this closure is never run because user_version is 2.
            let _ = tx;
            Ok(())
        }),
        db::Migration::Rust(broken),
    ];
    assert_eq!(db::migrate_with(&mut conn, &list).unwrap_err().code, "INJECTED_FAILURE");
    assert_eq!(db::user_version(&conn).unwrap(), 2, "version unchanged");
    assert_eq!(pk_columns(&conn, "suggestion_log"), ["task_id", "entry_id"], "old table still in place");
    assert_eq!(rows(&conn, "SELECT * FROM suggestion_log ORDER BY 1, 2, 3"), before);
    assert_eq!(n(&conn, "SELECT count(*) FROM sqlite_master WHERE name IN ('suggestion_log_v3', 'catalog_user_kind_immutable')"), 0);
}

#[test]
fn migrating_a_database_whose_old_log_has_rows_for_several_projects_keeps_all_of_them() {
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    let v2_only = [
        db::Migration::Sql(include_str!("../migrations/0001_init.sql")),
        db::Migration::Rust(|tx| {
            tx.execute_batch(include_str!("../migrations/0002_catalog_registry.sql"))?;
            crate::catalog::migrate_phase1a_data(tx)
        }),
    ];
    db::migrate_with(&mut conn, &v2_only).unwrap();
    for (project, task, entry) in [("p1", "t1", "strix"), ("p2", "t2", "strix"), ("p1", "t3", "agent-browser")] {
        conn.execute(
            "INSERT INTO suggestion_log (task_id, entry_id, project_id, phase, shown_at) VALUES (?1, ?2, ?3, 'start', 'x')",
            [task, entry, project],
        )
        .unwrap();
    }
    db::migrate(&mut conn).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
    assert_eq!(n(&conn, "SELECT count(*) FROM suggestion_log"), 3);
    assert_eq!(pk_columns(&conn, "suggestion_log"), ["project_id", "task_id", "entry_id"]);
}

// ------------------------------------------------------------------------------------------------
// RT-1B-02: restored entries keep their classification
// ------------------------------------------------------------------------------------------------

fn add_user_entry(conn: &Connection, kind: &str) -> catalog::CatalogEntry {
    catalog::add_entry(conn, "https://github.com/acme/widget", kind, "Deneme").unwrap()
}

#[test]
fn remove_and_restore_without_changing_the_kind_keeps_identity_fingerprint_and_history() {
    let conn = setup();
    let e = add_user_entry(&conn, "agent_plugin");
    projects::set_preference(&conn, "proje-a", &e.entry_id, "claude-code-terminal", "windows-x64", "rejected", None).unwrap();
    catalog::remove_entry(&conn, &e.entry_id).unwrap();
    let back = catalog::add_entry(&conn, "https://github.com/acme/widget", "agent_plugin", "Yeni not").unwrap();
    assert_eq!(back.entry_id, e.entry_id);
    assert_eq!((back.kind.as_str(), back.capability_fingerprint.as_str()), ("agent_plugin", e.capability_fingerprint.as_str()));
    assert!(back.removed_at.is_none() && !back.enabled, "restored, but switched off until the user enables it");
    assert_eq!(back.purpose, "Yeni not");
    let pref = &projects::list_projects(&conn).unwrap().into_iter().find(|p| p.alias == "proje-a").unwrap().preferences[0];
    assert_eq!(pref.decision, "rejected");
    assert!(!pref.material_change, "same classification, same fingerprint: the decision still means what it meant");
    assert_eq!(n(&conn, "SELECT count(*) FROM preference_history WHERE entry_id = 'acme-widget'"), 1);
}

#[test]
fn restore_with_a_changed_kind_is_refused_and_changes_nothing() {
    let conn = setup();
    let e = add_user_entry(&conn, "agent_plugin");
    projects::set_preference(&conn, "proje-a", &e.entry_id, "claude-code-terminal", "windows-x64", "rejected", None).unwrap();
    catalog::remove_entry(&conn, &e.entry_id).unwrap();
    let frozen = catalog::get_entry(&conn, &e.entry_id).unwrap();
    let frozen_updated = updated(&conn, &e.entry_id);
    let diag_before = n(&conn, "SELECT count(*) FROM diagnostics");
    for other in ["executable_tool", "skill_collection", "prompt_source"] {
        let err = catalog::add_entry(&conn, "https://github.com/acme/widget", other, "x").unwrap_err();
        assert_eq!(err.code, "KIND_CHANGE_NOT_ALLOWED", "{other}");
        assert!(err.message.contains("agent_plugin"), "tells the user which type it had: {}", err.message);
    }
    let after = catalog::get_entry(&conn, &e.entry_id).unwrap();
    assert!(after.removed_at.is_some(), "still removed");
    assert_eq!(
        (after.kind, after.capability_fingerprint, after.purpose, updated(&conn, &e.entry_id)),
        (frozen.kind, frozen.capability_fingerprint, frozen.purpose, frozen_updated)
    );
    assert_eq!(n(&conn, "SELECT count(*) FROM diagnostics"), diag_before, "a refused restore leaves no trace of a restore");
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE source_key = 'acme/widget'"), 1, "no second entry was created either");
    // The old decision is still exactly as it was.
    let pref = &projects::list_projects(&conn).unwrap().into_iter().find(|p| p.alias == "proje-a").unwrap().preferences[0];
    assert_eq!(pref.decision, "rejected");
    // And restoring with the original kind still works afterwards.
    assert!(catalog::add_entry(&conn, "https://github.com/acme/widget", "agent_plugin", "").is_ok());
}

#[test]
fn a_removed_seed_entry_also_cannot_be_restored_under_another_kind() {
    let conn = setup();
    catalog::remove_entry(&conn, "gitleaks").unwrap();
    let err = catalog::add_entry(&conn, "https://github.com/gitleaks/gitleaks", "agent_plugin", "").unwrap_err();
    assert_eq!(err.code, "KIND_CHANGE_NOT_ALLOWED");
    assert_eq!(catalog::get_entry(&conn, "gitleaks").unwrap().kind, "executable_tool");
    assert!(catalog::add_entry(&conn, "https://github.com/gitleaks/gitleaks", "executable_tool", "").is_ok());
}

#[test]
fn the_database_itself_refuses_an_in_place_classification_change_of_a_user_entry() {
    let conn = setup();
    let e = add_user_entry(&conn, "agent_plugin");
    assert!(conn.execute("UPDATE catalog_entries SET kind = 'executable_tool' WHERE entry_id = ?1", [&e.entry_id]).is_err());
    assert!(conn.execute("UPDATE catalog_entries SET kind = 'executable_tool'", []).is_err(), "bulk edits are refused as well");
    assert_eq!(catalog::get_entry(&conn, &e.entry_id).unwrap().kind, "agent_plugin");
    // Other columns of a user entry stay editable (for example the enable switch).
    assert!(conn.execute("UPDATE catalog_entries SET enabled = 1 WHERE entry_id = ?1", [&e.entry_id]).is_ok());
    // Seed entries are refreshed by the app's own seed step, which recomputes the fingerprint in the same statement.
    assert!(conn
        .execute("UPDATE catalog_entries SET kind = 'executable_tool' WHERE entry_id = 'ponytail' AND origin = 'seed'", [])
        .is_ok());
}

#[test]
fn health_flags_a_classification_that_drifted_away_from_its_fingerprint() {
    let conn = setup();
    let e = add_user_entry(&conn, "agent_plugin");
    assert_eq!(health_status(&conn, "catalog-invariants"), "passed");
    // Simulate a same-user edit that bypasses the trigger and leaves the old fingerprint behind.
    conn.execute_batch("DROP TRIGGER catalog_user_kind_immutable").unwrap();
    conn.execute("UPDATE catalog_entries SET kind = 'executable_tool' WHERE entry_id = ?1", [&e.entry_id]).unwrap();
    assert_eq!(health_status(&conn, "catalog-invariants"), "failed");
    let report = health::run(&conn, &ctx());
    assert_eq!(report.overall, "failed");
    assert_eq!(report.checks.iter().find(|c| c.id == "catalog-invariants").unwrap().error_code, Some("INVARIANT_VIOLATION"));
}

#[test]
fn a_previously_rejected_tool_stays_rejected_after_a_same_kind_restore_and_ordinary_version_change() {
    let conn = setup();
    let text = "Test the web checkout in a browser";
    projects::set_preference(&conn, "proje-a", "agent-browser", "local-cli", "windows-x64", "rejected", None).unwrap();
    let mut ev = input("proje-a", "r1", text);
    ev.agent_surface = "local-cli".into();
    assert_eq!(reason(&recommend::evaluate(&conn, &ev).unwrap(), "agent-browser"), Some("rejected"));
    catalog::remove_entry(&conn, "agent-browser").unwrap();
    catalog::restore_entry(&conn, "agent-browser").unwrap();
    catalog::set_enabled(&conn, "agent-browser", true).unwrap();
    conn.execute("UPDATE catalog_entries SET revision = 'v9.9.9@0123456789ab' WHERE entry_id = 'agent-browser'", []).unwrap();
    ev.task_id = "r2".into();
    assert_eq!(
        reason(&recommend::evaluate(&conn, &ev).unwrap(), "agent-browser"),
        Some("rejected"),
        "no repeated question for a restore plus a version bump"
    );
}

fn change_capabilities(conn: &Connection, entry_id: &str) {
    // Exactly what the seed step does when upstream capabilities change: new record AND recomputed fingerprint together.
    let e = catalog::get_entry(conn, entry_id).unwrap();
    let mut record = e.record.clone();
    record["networkClaims"] = serde_json::json!(["now also uploads project files to a third party"]);
    let fp = catalog::capability_fingerprint(&record, &e.kind, &e.execution_policy);
    conn.execute(
        "UPDATE catalog_entries SET record_json = ?2, capability_fingerprint = ?3 WHERE entry_id = ?1",
        rusqlite::params![entry_id, record.to_string(), fp],
    )
    .unwrap();
}

#[test]
fn a_previously_accepted_tool_is_not_asked_again_for_a_version_bump_but_is_after_a_material_change() {
    let conn = setup();
    let text = "Design a React component";
    projects::set_preference(&conn, "proje-a", FRONTEND_DESIGN, "claude-code-terminal", "windows-x64", "accepted", None).unwrap();
    assert_eq!(reason(&recommend::evaluate(&conn, &input("proje-a", "x1", text)).unwrap(), FRONTEND_DESIGN), Some("already-enabled"));
    // Remove and restore (same entry, same classification): the acceptance still stands, once the user re-enables the entry.
    catalog::remove_entry(&conn, FRONTEND_DESIGN).unwrap();
    assert_eq!(reason(&recommend::evaluate(&conn, &input("proje-a", "x2", text)).unwrap(), FRONTEND_DESIGN), Some("removed-from-catalog"));
    catalog::restore_entry(&conn, FRONTEND_DESIGN).unwrap();
    assert_eq!(
        reason(&recommend::evaluate(&conn, &input("proje-a", "x3", text)).unwrap(), FRONTEND_DESIGN),
        Some("disabled-in-catalog"),
        "restored entries start off"
    );
    catalog::set_enabled(&conn, FRONTEND_DESIGN, true).unwrap();
    // Ordinary version change: still silent.
    conn.execute("UPDATE catalog_entries SET revision = 'v2.0.0@abcdef012345' WHERE entry_id = ?1", [FRONTEND_DESIGN]).unwrap();
    assert_eq!(reason(&recommend::evaluate(&conn, &input("proje-a", "x4", text)).unwrap(), FRONTEND_DESIGN), Some("already-enabled"));
    // Meaningful capability change: the old approval is NOT reused; the user is asked again and told why.
    change_capabilities(&conn, FRONTEND_DESIGN);
    let renewed = recommend::evaluate(&conn, &input("proje-a", "x5", text)).unwrap();
    let rec = renewed.recommendations.iter().find(|r| r.entry_id == FRONTEND_DESIGN).expect("renewed review");
    assert!(rec.notes.iter().any(|n| n.contains("yeniden incelenmeli")), "{:?}", rec.notes);
    let pref = &projects::list_projects(&conn).unwrap().into_iter().find(|p| p.alias == "proje-a").unwrap().preferences[0];
    assert!(pref.material_change, "the stored approval is flagged as no longer matching the tool");
    // Approving again records the new fingerprint; from then on it is silent again.
    projects::set_preference(&conn, "proje-a", FRONTEND_DESIGN, "claude-code-terminal", "windows-x64", "accepted", None).unwrap();
    assert_eq!(reason(&recommend::evaluate(&conn, &input("proje-a", "x6", text)).unwrap(), FRONTEND_DESIGN), Some("already-enabled"));
    assert!(!projects::list_projects(&conn).unwrap().into_iter().find(|p| p.alias == "proje-a").unwrap().preferences[0].material_change);
}

#[test]
fn a_meaningful_capability_change_renews_the_review_of_a_rejection_and_a_user_entry_cannot_dodge_it_via_restore() {
    let conn = setup();
    let text = "Test the web checkout in a browser";
    projects::set_preference(&conn, "proje-a", "agent-browser", "local-cli", "windows-x64", "rejected", None).unwrap();
    change_capabilities(&conn, "agent-browser");
    let mut ev = input("proje-a", "m1", text);
    ev.agent_surface = "local-cli".into();
    let eval = recommend::evaluate(&conn, &ev).unwrap();
    let rec = eval.recommendations.iter().find(|r| r.entry_id == "agent-browser").expect("renewed review after a material change");
    assert!(rec.notes.iter().any(|n| n.contains("önemli ölçüde değişti")));
    // A user entry that was rejected cannot get a fresh fingerprint by being restored under another type.
    let e = add_user_entry(&conn, "executable_tool");
    projects::set_preference(&conn, "proje-a", &e.entry_id, "local-cli", "windows-x64", "rejected", None).unwrap();
    catalog::remove_entry(&conn, &e.entry_id).unwrap();
    assert_eq!(
        catalog::add_entry(&conn, "https://github.com/acme/widget", "agent_plugin", "").unwrap_err().code,
        "KIND_CHANGE_NOT_ALLOWED"
    );
    let pref = projects::list_projects(&conn)
        .unwrap()
        .into_iter()
        .find(|p| p.alias == "proje-a")
        .unwrap()
        .preferences
        .into_iter()
        .find(|p| p.entry_id == e.entry_id)
        .unwrap();
    assert_eq!((pref.decision.as_str(), pref.material_change), ("rejected", false));
}

#[test]
fn preview_of_a_removed_link_reports_the_kind_it_must_be_restored_with() {
    let conn = setup();
    let e = add_user_entry(&conn, "skill_collection");
    catalog::remove_entry(&conn, &e.entry_id).unwrap();
    let preview = catalog::preview_url(&conn, "https://github.com/ACME/Widget").unwrap();
    assert!(preview.ok);
    let dup = preview.duplicate_of.unwrap();
    assert!(dup.removed);
    assert_eq!(dup.kind, "skill_collection");
}
