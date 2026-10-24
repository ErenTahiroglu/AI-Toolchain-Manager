//! AGENT-01 (Issue #31): `codex-cli` is a selectable project target. Everything here runs on in-memory databases and the embedded catalog; nothing is read from
//! or written to a real file, nothing is fetched, installed or executed, and no agent (Codex or any other) is started or contacted.

use crate::{audit, catalog, db, plan, projects};
use rusqlite::Connection;

const WIN: &str = "windows-x64";
const MAC: &str = "macos-arm64";
const CODEX: &str = "codex-cli";
const OLD_SURFACES: [&str; 4] = ["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "local-cli"];

fn seeded() -> Connection {
    let conn = db::open_memory().unwrap();
    catalog::seed_all(&conn).unwrap();
    conn
}

fn n(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

/// The seeded database already holds the application's own sample decisions (synthetic projects); tests count relative to that baseline.
fn prefs(conn: &Connection) -> i64 {
    n(conn, "SELECT count(*) FROM project_preferences")
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

#[test]
fn the_surface_allowlist_is_the_four_historic_values_plus_codex_cli_and_the_install_plan_layer_is_untouched() {
    assert_eq!(projects::SURFACES, ["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "codex-cli", "local-cli"]);
    for s in OLD_SURFACES {
        assert!(projects::SURFACES.contains(&s), "{s} must stay a valid target");
    }
    assert!(!plan::SURFACES.contains(&CODEX), "AGENT-01 adds a project target, not an installer target");
    assert_eq!(plan::SURFACES.len(), 6, "the install-plan surface list is unchanged");
}

#[test]
fn a_codex_preference_is_stored_under_its_own_surface_and_never_under_another() {
    let conn = seeded();
    projects::add_reference(&conn, "proje-bir", "Benim sitem", "").unwrap();
    let baseline = prefs(&conn);
    let stored = projects::set_preference(&conn, "proje-bir", "microsoft-playwright-mcp", CODEX, WIN, "postponed", Some(3)).unwrap();
    assert_eq!((stored.agent_surface.as_str(), stored.platform_target.as_str(), stored.decision.as_str()), (CODEX, WIN, "postponed"));
    assert_eq!(stored.compatibility, "unknown", "no publisher evidence row exists for Codex: unknown stays unknown");
    assert_eq!(stored.actual_install_state, "not_managed");
    assert_eq!((stored.actual_verification_state.as_str(), stored.observed_scope.as_str()), ("unknown", "unknown"));
    // The same tool/OS under every other surface is untouched: the key has four dimensions and the fourth is the surface.
    for other in OLD_SURFACES {
        assert!(projects::stored_preference(&conn, "proje-bir", "microsoft-playwright-mcp", other, WIN).unwrap().is_none(), "{other}");
    }
    assert!(projects::stored_preference(&conn, "proje-bir", "microsoft-playwright-mcp", CODEX, MAC).unwrap().is_none());
    assert_eq!(prefs(&conn), baseline + 1);
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences WHERE agent_surface = 'codex-cli'"), 1);
}

#[test]
fn for_all_twenty_two_historic_entries_codex_compatibility_is_unknown_and_acceptance_stays_refused() {
    let conn = seeded();
    projects::add_reference(&conn, "proje-bir", "Benim sitem", "").unwrap();
    let baseline = prefs(&conn);
    let entries = catalog::list_catalog(&conn).unwrap();
    assert_eq!(entries.len(), catalog::SEED_ENTRY_COUNT);
    assert_eq!(entries.len(), 22, "the historic candidate identities are all present");
    for e in &entries {
        for platform in [WIN, MAC] {
            assert_eq!(
                catalog::compatibility(e, platform, CODEX),
                "unknown",
                "{} on {platform}: no Codex evidence row may be implied",
                e.entry_id
            );
            let err = projects::set_preference(&conn, "proje-bir", &e.entry_id, CODEX, platform, "accepted", None).unwrap_err();
            assert!(
                ["PREFERENCE_NOT_ACTIVATABLE", "ACTIVATION_BLOCKED", "ENTRY_DISABLED"].contains(&err.code),
                "{} on {platform}: accepted must be refused, got {}",
                e.entry_id,
                err.code
            );
        }
    }
    assert_eq!(prefs(&conn), baseline, "a refused wish stores nothing");
    // An unknown option can still be postponed and rejected (and reset), exactly like any other unknown cell.
    projects::set_preference(&conn, "proje-bir", "microsoft-playwright-mcp", CODEX, WIN, "postponed", Some(2)).unwrap();
    projects::set_preference(&conn, "proje-bir", "apify-mcp-server", CODEX, MAC, "rejected", None).unwrap();
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences WHERE agent_surface = 'codex-cli'"), 2);
    projects::reset_preference(&conn, "proje-bir", "apify-mcp-server", CODEX, MAC).unwrap();
    assert!(audit::verify(&conn).unwrap().ok);
}

#[test]
fn no_catalog_data_file_carries_a_codex_evidence_row() {
    for (name, json) in [
        ("seed", include_str!("../../data/catalog-seed.json")),
        ("1b", include_str!("../../data/catalog-candidates-1b.json")),
        ("c01", include_str!("../../data/catalog-candidates-c01.json")),
        ("c02", include_str!("../../data/catalog-candidates-c02.json")),
        ("c03", include_str!("../../data/catalog-candidates-c03.json")),
    ] {
        assert!(!json.contains("codex-cli"), "{name}: AGENT-01 adds no synthetic Codex support claim");
    }
}

#[test]
fn an_unlisted_surface_and_provider_brand_names_are_refused_by_the_backend() {
    let conn = seeded();
    projects::add_reference(&conn, "proje-bir", "Benim sitem", "").unwrap();
    let baseline = prefs(&conn);
    for bad in [
        "codex",
        "Codex-CLI",
        "codex-desktop",
        "codex-ide",
        "gemini-cli",
        "deepseek",
        "grok",
        "groq",
        "xai",
        "openai",
        "antigravity-cli",
        "antigravity-2",
        "claude",
        "",
    ] {
        let err = projects::set_preference(&conn, "proje-bir", "microsoft-playwright-mcp", bad, WIN, "postponed", Some(1)).unwrap_err();
        assert_eq!(err.code, "SURFACE_INVALID", "{bad:?}");
    }
    assert_eq!(prefs(&conn), baseline);
}

#[test]
fn a_codex_event_is_audited_without_an_invented_audit_target_id() {
    let conn = seeded();
    projects::add_reference(&conn, "proje-bir", "Benim sitem", "").unwrap();
    projects::set_preference(&conn, "proje-bir", "microsoft-playwright-mcp", CODEX, WIN, "rejected", None).unwrap();
    let events = audit::list_ascending(&conn).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["targetId"], "unknown", "there is no audit target id for Codex; it is not mapped onto another target");
    assert!(audit::verify(&conn).unwrap().ok);
}

/// A database as the previous build (schema 9) left it, with real decisions on every historic surface.
fn v9_database_with_decisions() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    db::migrate_with(&mut conn, &db::migrations_prefix(9)).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), 9);
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
    set("proje-iki", "agent-browser", "local-cli", MAC, "accepted", None);
    conn
}

#[test]
fn migration_ten_preserves_every_existing_decision_byte_for_byte_and_only_widens_the_surface_check() {
    let mut conn = v9_database_with_decisions();
    let prefs_before = rows(&conn, "SELECT * FROM project_preferences ORDER BY rowid");
    let history_before = rows(&conn, "SELECT * FROM preference_history ORDER BY id");
    let audit_before = rows(&conn, "SELECT * FROM audit_events ORDER BY id");
    let entries_before = rows(&conn, "SELECT entry_id, record_json FROM catalog_entries ORDER BY entry_id");
    let projects_before = rows(&conn, "SELECT * FROM project_registry ORDER BY rowid");
    assert!(prefs_before.len() >= 6, "the six decisions made here plus the application's own samples");
    // Before the migration the old table really cannot hold a Codex decision.
    assert!(conn
        .execute(
            "INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint)
             SELECT project_id, 'gitleaks', 'codex-cli', 'windows-x64', 'rejected', 'x', 'x', 'x' FROM project_registry LIMIT 1",
            []
        )
        .is_err());

    db::migrate(&mut conn).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);

    assert_eq!(
        rows(&conn, "SELECT * FROM project_preferences ORDER BY rowid"),
        prefs_before,
        "no decision changed, moved, re-dated or re-fingerprinted"
    );
    assert_eq!(rows(&conn, "SELECT * FROM preference_history ORDER BY id"), history_before);
    assert_eq!(rows(&conn, "SELECT * FROM audit_events ORDER BY id"), audit_before);
    assert_eq!(
        rows(&conn, "SELECT entry_id, record_json FROM catalog_entries ORDER BY entry_id"),
        entries_before,
        "catalog data untouched"
    );
    assert_eq!(rows(&conn, "SELECT * FROM project_registry ORDER BY rowid"), projects_before);
    assert!(audit::verify(&conn).unwrap().ok, "the audit chain still verifies");
    assert_eq!(
        n(&conn, "SELECT count(*) FROM project_preferences WHERE agent_surface = 'codex-cli'"),
        0,
        "the migration creates no Codex row"
    );

    // The widened table accepts exactly the new value, nothing else.
    projects::set_preference(&conn, "proje-bir", "microsoft-playwright-mcp", CODEX, WIN, "postponed", Some(2)).unwrap();
    assert!(conn
        .execute(
            "INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint)
             SELECT project_id, 'gitleaks', 'gemini-cli', 'windows-x64', 'rejected', 'x', 'x', 'x' FROM project_registry LIMIT 1",
            []
        )
        .is_err());
    // Every other constraint of the table survived the rebuild.
    for (col, bad) in [("decision", "installed"), ("platform_target", "linux-x64")] {
        let sql = format!(
            "INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint)
             SELECT project_id, 'strix', 'local-cli', {p}, {d}, 'x', 'x', 'x' FROM project_registry LIMIT 1",
            p = if col == "platform_target" { format!("'{bad}'") } else { "'windows-x64'".into() },
            d = if col == "decision" { format!("'{bad}'") } else { "'rejected'".into() }
        );
        assert!(conn.execute(&sql, []).is_err(), "{col}={bad}");
    }
    assert!(
        conn.execute("UPDATE project_preferences SET actual_install_state = 'installed' WHERE rowid = 1", []).is_err(),
        "the actual-state columns still cannot claim an install"
    );
    let dup = conn.execute(
        "INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint)
         SELECT project_id, entry_id, agent_surface, platform_target, 'rejected', 'x', 'x', 'x' FROM project_preferences LIMIT 1",
        [],
    );
    assert!(dup.is_err(), "the four-dimensional primary key is intact");
    // No stray table was left behind.
    assert_eq!(n(&conn, "SELECT count(*) FROM sqlite_master WHERE name LIKE 'project_preferences%'"), 1);
}

#[test]
fn migration_ten_is_repeatable_and_a_failing_step_rolls_back_to_the_untouched_v9_table() {
    let mut conn = v9_database_with_decisions();
    let before = rows(&conn, "SELECT * FROM project_preferences ORDER BY rowid");
    fn broken(tx: &rusqlite::Transaction) -> crate::error::AppResult<()> {
        tx.execute_batch(include_str!("../migrations/0010_codex_cli_surface.sql"))?;
        Err(crate::error::AppError::new("INJECTED_FAILURE", "crash after the DDL"))
    }
    let mut list = db::migrations_prefix(9);
    list.push(db::Migration::Rust(broken));
    assert_eq!(db::migrate_with(&mut conn, &list).unwrap_err().code, "INJECTED_FAILURE");
    assert_eq!(db::user_version(&conn).unwrap(), 9, "version unchanged");
    assert_eq!(rows(&conn, "SELECT * FROM project_preferences ORDER BY rowid"), before, "the old table is exactly as it was");
    assert_eq!(n(&conn, "SELECT count(*) FROM sqlite_master WHERE name = 'project_preferences_v10'"), 0);

    db::migrate(&mut conn).unwrap();
    let after = rows(&conn, "SELECT * FROM project_preferences ORDER BY rowid");
    assert_eq!(after, before);
    db::migrate(&mut conn).unwrap();
    assert_eq!(rows(&conn, "SELECT * FROM project_preferences ORDER BY rowid"), before, "re-running is a no-op");
}

#[test]
fn a_database_file_from_the_previous_build_is_backed_up_then_migrated_and_keeps_its_decisions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atm.sqlite");
    let backups = dir.path().join("backups");
    let before = {
        let mut raw = Connection::open(&path).unwrap();
        raw.pragma_update(None, "foreign_keys", true).unwrap();
        db::migrate_with(&mut raw, &db::migrations_prefix(9)).unwrap();
        catalog::seed_all(&raw).unwrap();
        projects::add_reference(&raw, "proje-bir", "Benim sitem", "").unwrap();
        projects::set_preference(&raw, "proje-bir", "agent-browser", "local-cli", WIN, "accepted", None).unwrap();
        rows(&raw, "SELECT * FROM project_preferences ORDER BY rowid")
    };
    let conn = db::open_file(&path, Some(&backups)).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
    assert_eq!(rows(&conn, "SELECT * FROM project_preferences ORDER BY rowid"), before);
    let backup = std::fs::read_dir(&backups).unwrap().next().expect("a pre-migration snapshot was written").unwrap().path();
    let old = Connection::open(&backup).unwrap();
    assert_eq!(db::user_version(&old).unwrap(), 9, "the snapshot is the v9 database");
    assert_eq!(rows(&old, "SELECT * FROM project_preferences ORDER BY rowid"), before);
}

#[test]
fn the_catalog_health_invariants_are_unaffected_by_a_codex_wish() {
    let conn = seeded();
    projects::add_reference(&conn, "proje-bir", "Benim sitem", "").unwrap();
    projects::set_preference(&conn, "proje-bir", "microsoft-playwright-mcp", CODEX, WIN, "rejected", None).unwrap();
    let ctx = crate::health::HealthContext {
        tray: crate::health::TrayState::NotAttempted,
        autostart: crate::health::AutostartState::NotQueried,
        previous_shutdown: crate::health::PreviousShutdown::FirstRun,
    };
    let report = crate::health::run(&conn, &ctx);
    assert_eq!(report.checks.iter().find(|c| c.id == "catalog-invariants").unwrap().status, "passed");
}
