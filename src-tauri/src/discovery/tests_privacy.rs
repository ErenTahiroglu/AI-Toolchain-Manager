//! RT-2A-02: nothing that identifies another account, a private folder, an MCP server name or an undocumented
//! "version" text may reach the database, the diagnostics, the audit chain, the UI data or the headless output.
//! Everything here is synthetic; the "other user" is a made-up name.

use super::*;
use crate::{diagnostics, discovery::privacy};
use rusqlite::types::ValueRef;

const OTHER: &str = "OtherUser";

/// Every text value of every table, as one string (what a forensic look at the database would see).
fn dump_all(conn: &Connection) -> String {
    let tables: Vec<String> = {
        let mut s = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'").unwrap();
        s.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
    };
    let mut out = String::new();
    for t in tables {
        let mut stmt = conn.prepare(&format!("SELECT * FROM {t}")).unwrap();
        let n = stmt.column_count();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            for i in 0..n {
                match row.get_ref(i).unwrap() {
                    ValueRef::Text(b) | ValueRef::Blob(b) => {
                        out.push_str(&String::from_utf8_lossy(b));
                        out.push('\n');
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

fn assert_no_leak(haystack: &str, env: &Env, what: &str) {
    let lower = haystack.to_lowercase();
    let root = env.root.to_string_lossy().to_lowercase();
    for needle in [
        OTHER.to_lowercase(),
        root.clone(),
        root.replace('\\', "/"),
        root.trim_start_matches(r"\\?\").to_string(),
        "acme-client".to_string(),
    ] {
        assert!(!lower.contains(&needle), "{what} leaks `{needle}`");
    }
    assert!(!haystack.contains("SENTINEL"), "{what} leaks a secret-shaped value");
}

#[test]
fn another_accounts_folder_outside_home_never_reaches_any_output() {
    let env = Env::new();
    let other_tools = env.root.join("Users").join(OTHER).join("Tools");
    fs::create_dir_all(&other_tools).unwrap();
    fs::write(other_tools.join(exe("gitleaks")), "x").unwrap();
    let sys = env.root.join("sys").join("pm");
    fs::create_dir_all(&sys).unwrap();
    fs::write(sys.join(exe("markitdown")), "x").unwrap();
    let under_home = env.home.join("tools").join("bin");
    fs::create_dir_all(&under_home).unwrap();
    fs::write(under_home.join(exe("strix")), "x").unwrap();
    let mut roots = env.roots();
    roots.path_entries = vec![other_tools.clone(), sys.clone(), under_home.clone()];
    roots.system_dirs = vec![env.root.join("sys")];

    let conn = seeded();
    crate::discovery::run_scan(&conn, &roots, &all_auth(None)).unwrap();
    let inv = store::inventory(&conn).unwrap();
    let loc = |subject: &str| {
        inv.observations.iter().find(|o| o.subject == subject && o.method == "path-executable").and_then(|o| o.location.clone()).unwrap()
    };
    assert_eq!(loc("gitleaks"), format!("PATH girdisi #1 · başka bir klasör · {}", exe("gitleaks")));
    assert_eq!(loc("markitdown"), format!("PATH girdisi #2 · sistem veya paket yöneticisi klasörü · {}", exe("markitdown")));
    assert_eq!(loc("strix"), format!("PATH girdisi #3 · kullanıcı klasörü altında · {}", exe("strix")));
    for o in &inv.observations {
        if let Some(l) = &o.location {
            assert!(privacy::is_safe_location(l), "{l}");
        }
    }
    assert_no_leak(&serde_json::to_string(&inv).unwrap(), &env, "inventory JSON");
    assert_no_leak(&dump_all(&conn), &env, "SQLite dump (observations, diagnostics, audit, settings)");
}

#[test]
fn mcp_server_names_are_never_stored_only_how_many_matched() {
    let env = Env::new();
    env.home_write(
        ".claude.json",
        &serde_json::json!({
            "mcpServers": {
                "acme-client-codebase-memory": { "command": "codebase-memory-mcp" },
                "ghp_SENTINELSENTINELSENTINELSENTINEL12-codebase-memory": { "command": "x" },
                "unrelated-private-name": { "command": "npx" }
            }
        })
        .to_string(),
    );
    let conn = seeded();
    crate::discovery::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    let inv = store::inventory(&conn).unwrap();
    let m = inv.observations.iter().find(|o| o.subject == "codebase-memory-mcp" && o.method == "claude-mcp-config").unwrap();
    assert!(m.evidence.contains("2 MCP sunucu tanımı"), "{}", m.evidence);
    assert!(!m.evidence.contains("(ad:"));
    assert_no_leak(&dump_all(&conn), &env, "SQLite dump");
    assert!(!dump_all(&conn).contains("unrelated-private-name"));
    assert_no_leak(&serde_json::to_string(&inv).unwrap(), &env, "inventory JSON");
}

#[test]
fn only_documented_version_forms_are_shown_and_cache_lookup_still_works_for_other_names() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[("superpowers@marketplace-x", true), ("caveman@m", true), ("ponytail@m", true)]));
    env.home_write(
        ".claude/plugins/installed_plugins.json",
        &registry(&[
            ("superpowers@marketplace-x", "user", "6.4.2"),
            ("caveman@m", "user", "ghp_SENTINELSENTINELSENTINELSENTINEL12"),
            ("ponytail@m", "user", "acme-internal-build-7"),
        ]),
    );
    cache(&env, "marketplace-x", "superpowers", "6.4.2");
    cache(&env, "m", "ponytail", "acme-internal-build-7");
    let conn = seeded();
    crate::discovery::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    let inv = store::inventory(&conn).unwrap();
    let get = |s: &str| {
        inv.observations.iter().find(|o| o.subject == s && o.method == "claude-plugin-declaration" && o.scope == "global").unwrap()
    };
    assert_eq!(get("superpowers").version.as_deref(), Some("6.4.2"));
    assert_eq!(get("superpowers").version_source.as_deref(), Some("Claude Code kurulum kaydı"));
    // A path-safe but undocumented version text is not shown, yet it is still enough to find the cache folder.
    assert_eq!(get("ponytail").version, None);
    assert_eq!(get("ponytail").version_source, None);
    assert_eq!(get("ponytail").presence, "present");
    assert_eq!(get("caveman").version, None);
    assert_no_leak(&dump_all(&conn), &env, "SQLite dump");
    assert!(!dump_all(&conn).contains("acme-internal"));
}

#[test]
fn malicious_version_and_text_values_are_replaced_before_persistence_even_without_finalize() {
    let env = Env::new();
    let conn = seeded();
    let evil = |version: &str| {
        Observation::new("gitleaks", platform(), "local-cli", Method::PathExecutable)
            .presence(Presence::Present)
            .location(Some(format!(r"C:\Users\{OTHER}\Tools\gitleaks.exe")))
            .evidence(format!(r"found at C:\Users\{OTHER}\Tools\gitleaks.exe and /Users/{OTHER}/bin (ad: acme-client)"))
            .limit(format!("see \\\\fileserver\\{OTHER}\\share"))
            .version(Some(version.to_string()), "attacker supplied label")
    };
    let raw: Vec<Observation> =
        ["1.0.0-ghp-SENTINEL", "../../escape", "my-company-internal-1", "latest", "1.0.0+secret"].iter().map(|v| evil(v)).collect();
    // `save_scan` is called with un-finalized observations on purpose: the gate must hold on its own.
    let auth = Authorization { path_executables: true, user_config: false, project: None };
    let summary = store::save_scan(&conn, &env.roots(), &auth, &raw, "2026-10-17T00:00:00Z").unwrap();
    assert_eq!(summary.observation_count, 5);
    let stored = store::latest_observations(&conn).unwrap();
    assert!(!stored.is_empty());
    for o in &stored {
        assert_eq!(o.version, None, "{:?}", o.version);
        assert_eq!(o.version_source, None);
        assert_eq!(o.location.as_deref(), Some(privacy::LOCATION_REMOVED));
        assert!(!o.evidence.to_lowercase().contains("users") && !o.evidence.contains("acme-client"), "{}", o.evidence);
        assert!(o.limitations.iter().all(|l| !l.contains("fileserver")), "{:?}", o.limitations);
    }
    assert_no_leak(&dump_all(&conn), &env, "SQLite dump");
    let all = dump_all(&conn);
    assert!(
        !all.contains("fileserver") && !all.contains("attacker supplied"),
        "no raw text from the hostile observation reached the database"
    );
    // And the diagnostics only carry counts.
    let diag: String = diagnostics::list(&conn, 50).unwrap().iter().map(|d| d.detail.clone()).collect::<Vec<_>>().join("|");
    assert!(!diag.to_lowercase().contains("users") && !diag.contains(OTHER), "{diag}");
}

#[test]
fn a_tampered_database_row_is_sanitized_on_read_and_fails_the_self_audit() {
    let env = Env::new();
    let conn = seeded();
    env.fake_exe("gitleaks");
    crate::discovery::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    assert_eq!(discovery_status(&conn).0, "passed");
    conn.execute(
        "UPDATE discovery_observations SET location = ?1, evidence = ?2, version = '9.9.9-private-name', version_source = 'x' WHERE subject = 'gitleaks' AND method = 'path-executable'",
        [format!(r"C:\Users\{OTHER}\Tools\gitleaks.exe"), format!(r"at C:\Users\{OTHER}\x (ad: acme-client)")],
    )
    .unwrap();
    let inv = store::inventory(&conn).unwrap();
    let json = serde_json::to_string(&inv).unwrap();
    assert!(!json.contains(OTHER) && !json.contains("acme-client") && !json.contains("private-name"), "the read path applies the gate too");
    assert_eq!(discovery_status(&conn), ("failed", Some("DISCOVERY_EVIDENCE_UNSAFE")));
    for (column, value) in [
        ("location", format!(r"C:\Users\{OTHER}\x")),
        ("project_alias", "Bad Alias".to_string()),
        ("version_source", "free text".to_string()),
    ] {
        let conn = seeded();
        crate::discovery::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
        conn.execute(
            &format!("UPDATE discovery_observations SET {column} = ?1 WHERE id = (SELECT min(id) FROM discovery_observations)"),
            [value],
        )
        .unwrap();
        assert_eq!(discovery_status(&conn), ("failed", Some("DISCOVERY_EVIDENCE_UNSAFE")), "{column}");
    }
}

#[test]
fn the_headless_probe_output_contains_no_absolute_path_and_only_grammar_conforming_locations() {
    let dir = tempfile::tempdir().unwrap();
    let conn = seeded();
    let out = crate::discovery_probe::synthetic_probe(&conn, dir.path()).unwrap();
    let text = serde_json::to_string(&out).unwrap();
    let canonical = dir.path().canonicalize().unwrap().to_string_lossy().to_lowercase();
    let lower = text.to_lowercase();
    assert!(
        !lower.contains(&canonical) && !lower.contains(&canonical.replace('\\', "\\\\")) && !lower.contains(&canonical.replace('\\', "/"))
    );
    for o in out["observations"].as_array().unwrap() {
        if let Some(l) = o["location"].as_str() {
            assert!(privacy::is_safe_location(l), "{l}");
        }
        assert!(!privacy::contains_absolute_path(o["evidence"].as_str().unwrap()), "{o}");
        for l in o["limitations"].as_array().unwrap() {
            assert!(!privacy::contains_absolute_path(l.as_str().unwrap()), "{l}");
        }
    }
    assert_eq!(out["treeUnchanged"], true);
    assert_eq!(out["dummySecretInObservations"], false);
}

// ------------------------------------------------------------------------------------------------
// Legacy (schema v4) databases that already hold the leaking rows
// ------------------------------------------------------------------------------------------------

/// (subject, surface, method, presence, configuration, version, version_source, scope, alias, evidence, location, limitations)
type LegacyRow = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
    &'static str,
    Option<&'static str>,
    &'static str,
    Option<&'static str>,
    &'static str,
);

/// A schema-v4 database exactly as the Phase 2A baseline produced it, but filled with made-up data that contains the
/// problems found by the Red Team: an absolute path of another account, an MCP server name, undocumented versions.
pub(super) fn legacy_v4_database(conn: &mut Connection) {
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    let v4 = [
        db::Migration::Sql(include_str!("../../migrations/0001_init.sql")),
        db::Migration::Rust(|tx| {
            tx.execute_batch(include_str!("../../migrations/0002_catalog_registry.sql"))?;
            crate::catalog::migrate_phase1a_data(tx)
        }),
        db::Migration::Sql(include_str!("../../migrations/0003_scoped_suggestion_log.sql")),
        db::Migration::Sql(include_str!("../../migrations/0004_discovery.sql")),
    ];
    db::migrate_with(conn, &v4).unwrap();
    assert_eq!(db::user_version(conn).unwrap(), 4);
    catalog::seed_all(conn).unwrap();
    projects::add_reference(conn, "probe-project", "Probe project", "").unwrap();
    projects::set_preference(conn, "probe-project", FD, "claude-code-terminal", PLATFORM_WINDOWS, "accepted", None).unwrap();
    projects::set_preference(conn, "probe-project", "gitleaks", "local-cli", PLATFORM_WINDOWS, "accepted", None).unwrap();
    for (id, finished) in [("legacy-scan-1", "2026-10-16T09:00:00Z"), ("legacy-scan-2", "2026-10-16T10:00:00Z")] {
        conn.execute(
            "INSERT INTO discovery_scans VALUES (?1, ?2, ?2, 'windows-x64', 1, 1, 1, 'completed', 6)",
            rusqlite::params![id, finished],
        )
        .unwrap();
        // (subject, surface, method, presence, configuration, version, version_source, scope, alias, evidence, location, limitations)
        let rows: [LegacyRow; 6] = [
            (
                "gitleaks", "local-cli", "path-executable", "present", "not_applicable", None, None, "global", None,
                "PATH klasörlerinden birinde 'gitleaks' adlı bir dosya bulundu.",
                Some(r"C:\Users\OtherUser\Tools\gitleaks.exe"),
                r#"["Ad eşleşmesi: aynı adlı başka bir program olabilir.","Bulundu: C:\\Users\\OtherUser\\Tools"]"#,
            ),
            (
                "agent:claude-code", "claude-code-terminal", "native-install-location", "present", "not_applicable", None, None, "global", None,
                "Claude Code'un belgelenmiş yerel kurulum konumunda 'claude' dosyası bulundu.",
                Some("~/.local/bin/claude.exe"),
                r#"["Dosyanın varlığı çalıştığını göstermez."]"#,
            ),
            (
                "codebase-memory-mcp", "claude-code-terminal", "claude-mcp-config", "unknown", "configured", None, None, "global", None,
                "Kullanıcı MCP yapılandırması (~/.claude.json) içinde katalogdaki araçla eşleşen 1 MCP sunucu tanımı var (ad: acme-client-codebase-memory). Sunucu çalıştırılmadı.",
                None,
                r#"["Başvurunun varlığı programın kurulu olduğunu göstermez."]"#,
            ),
            (
                "superpowers", "claude-code-terminal", "claude-plugin-declaration", "present", "configured", Some("acme-internal-build-7"), Some("Claude Code kurulum kaydı"), "global", None,
                "Eklenti, kullanıcı ayarları içinde açık olarak bildirilmiş.",
                None,
                r#"[]"#,
            ),
            (
                "anthropic-frontend-design", "claude-code-terminal", "claude-plugin-declaration", "present", "configured", Some("1.0.0"), Some("Claude Code kurulum kaydı"), "project", Some("probe-project"),
                "Eklenti proje ayarları içinde açık olarak bildirilmiş.",
                None,
                r#"[]"#,
            ),
            (
                "agent:antigravity-ide", "antigravity-ide", "not-documented", "unknown", "unknown", None, None, "global", None,
                "Antigravity masaüstü IDE'si için doğrulanmış bir yöntem yok.",
                None,
                r#"[]"#,
            ),
        ];
        for (subject, surface, method, presence, configuration, version, source, scope, alias, evidence, location, limitations) in rows {
            conn.execute(
                "INSERT INTO discovery_observations
                 (scan_id, subject, platform_target, agent_surface, method, presence, configuration, version, version_source, scope, project_alias,
                  confidence, evidence, location, limitations_json, identity_verified, functionality_tested, observed_at)
                 VALUES (?1, ?2, 'windows-x64', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'low', ?11, ?12, ?13, 0, 0, ?14)",
                rusqlite::params![id, subject, surface, method, presence, configuration, version, source, scope, alias, evidence, location, limitations, finished],
            )
            .unwrap();
        }
    }
}

fn table_text(conn: &Connection, sql: &str) -> Vec<String> {
    let mut stmt = conn.prepare(sql).unwrap();
    let n = stmt.column_count();
    stmt.query_map([], |r| (0..n).map(|i| r.get::<_, rusqlite::types::Value>(i).map(|v| format!("{v:?}"))).collect::<Result<Vec<_>, _>>())
        .unwrap()
        .map(|r| r.unwrap().join("|"))
        .collect()
}

fn preserved_state(conn: &Connection) -> Vec<Vec<String>> {
    [
        "SELECT * FROM project_registry ORDER BY 1",
        "SELECT * FROM project_preferences ORDER BY 1,2,3,4",
        "SELECT * FROM preference_history ORDER BY id",
        "SELECT * FROM audit_events ORDER BY 1",
        "SELECT * FROM catalog_entries ORDER BY 1",
        "SELECT scan_id, started_at, finished_at, platform_target, authorized_path, authorized_user_config, authorized_project, status, observation_count FROM discovery_scans ORDER BY 1",
        "SELECT id, scan_id, subject, platform_target, agent_surface, method, presence, configuration, scope, project_alias, confidence, identity_verified, functionality_tested, observed_at FROM discovery_observations ORDER BY id",
    ]
    .iter()
    .map(|q| table_text(conn, q))
    .collect()
}

#[test]
fn the_legacy_fixture_really_contains_the_leaks_the_migration_must_remove() {
    let mut conn = Connection::open_in_memory().unwrap();
    legacy_v4_database(&mut conn);
    let all = dump_all(&conn);
    for leak in [OTHER, "acme-client-codebase-memory", "(ad:", "acme-internal-build-7"] {
        assert!(all.contains(leak), "the legacy data set must demonstrate `{leak}`");
    }
}

fn assert_clean_after_migration(conn: &Connection) {
    let all = dump_all(conn);
    for leak in [OTHER, "acme-client", "(ad:", "acme-internal", "Users"] {
        assert!(!all.contains(leak), "migration 5 left `{leak}` behind");
    }
    let inv = store::inventory(conn).unwrap();
    assert!(inv.observations.iter().all(|o| o.legacy), "migrated rows stay marked as legacy");
    let g = inv.observations.iter().find(|o| o.subject == "gitleaks").unwrap();
    assert_eq!(g.location.as_deref(), Some(privacy::LOCATION_REMOVED));
    assert_eq!(g.presence, "present", "presence is preserved");
    let claude = inv.observations.iter().find(|o| o.method == "native-install-location").unwrap();
    assert_eq!(claude.location.as_deref(), Some("~/.local/bin/claude.exe"), "an already-safe location is kept");
    let sp = inv.observations.iter().find(|o| o.subject == "superpowers").unwrap();
    assert_eq!((sp.version.as_deref(), sp.version_source.as_deref()), (None, None));
    let fd = inv.observations.iter().find(|o| o.subject == "anthropic-frontend-design").unwrap();
    assert_eq!(fd.version.as_deref(), Some("1.0.0"), "a documented version is kept");
    let mcp = inv.observations.iter().find(|o| o.subject == "codebase-memory-mcp").unwrap();
    assert!(mcp.evidence.contains("1 MCP sunucu tanımı var.") || mcp.evidence.contains("1 MCP sunucu tanımı var"), "{}", mcp.evidence);
    assert!(audit::verify(conn).unwrap().ok);
    assert_ne!(discovery_status(conn).1, Some("DISCOVERY_EVIDENCE_UNSAFE"));
}

#[test]
fn migration_5_scrubs_a_legacy_v4_database_in_place_and_preserves_everything_else() {
    let mut conn = Connection::open_in_memory().unwrap();
    legacy_v4_database(&mut conn);
    let before = preserved_state(&conn);
    let scans_before = table_text(&conn, "SELECT count(*) FROM discovery_observations");
    db::migrate(&mut conn).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
    assert_eq!(preserved_state(&conn), before, "preferences, history, audit chain, catalog, scans and observation facts are untouched");
    assert_eq!(table_text(&conn, "SELECT count(*) FROM discovery_observations"), scans_before, "no observation was deleted");
    assert_eq!(
        table_text(&conn, "SELECT DISTINCT record_version FROM discovery_observations"),
        vec!["Integer(1)".to_string()],
        "legacy rows keep record_version 1"
    );
    assert_clean_after_migration(&conn);
    let note: String = conn.query_row("SELECT detail FROM diagnostics WHERE code = 'DISCOVERY_LEGACY_SCRUBBED'", [], |r| r.get(0)).unwrap();
    assert!(note.contains("temizlendi") && !note.contains(OTHER));
    // Repeating startup is a no-op, and a new scan writes version-2 rows next to the legacy ones.
    db::migrate(&mut conn).unwrap();
    let env = Env::new();
    crate::discovery::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    let versions = table_text(&conn, "SELECT DISTINCT record_version FROM discovery_observations ORDER BY 1");
    assert_eq!(versions, vec!["Integer(1)".to_string(), "Integer(2)".to_string()]);
}

#[test]
fn a_failed_migration_5_rolls_back_and_leaves_the_v4_database_as_it_was() {
    fn broken(tx: &rusqlite::Transaction) -> crate::error::AppResult<()> {
        tx.execute_batch(include_str!("../../migrations/0005_discovery_privacy.sql"))?;
        crate::discovery::store::scrub_legacy_observations(tx)?;
        Err(crate::error::AppError::new("INJECTED_FAILURE", "crash after scrubbing"))
    }
    let mut conn = Connection::open_in_memory().unwrap();
    legacy_v4_database(&mut conn);
    let before = dump_all(&conn);
    let list = [
        db::Migration::Sql(include_str!("../../migrations/0001_init.sql")),
        db::Migration::Rust(|tx| {
            tx.execute_batch(include_str!("../../migrations/0002_catalog_registry.sql"))?;
            crate::catalog::migrate_phase1a_data(tx)
        }),
        db::Migration::Sql(include_str!("../../migrations/0003_scoped_suggestion_log.sql")),
        db::Migration::Sql(include_str!("../../migrations/0004_discovery.sql")),
        db::Migration::Rust(broken),
    ];
    assert_eq!(db::migrate_with(&mut conn, &list).unwrap_err().code, "INJECTED_FAILURE");
    assert_eq!(db::user_version(&conn).unwrap(), 4);
    assert_eq!(dump_all(&conn), before, "nothing changed");
    assert!(conn.prepare("SELECT record_version FROM discovery_observations").is_err(), "the column was rolled back too");
}

const SYNTHETIC_V4_FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/phase2a-v4-synthetic.sqlite.fixture");

/// Regenerates the committed fixture from the synthetic builder above. Run by hand:
/// `ATM_WRITE_FIXTURE=<path> cargo test generate_synthetic_v4_fixture -- --ignored`
#[test]
#[ignore]
fn generate_synthetic_v4_fixture() {
    let target = std::env::var("ATM_WRITE_FIXTURE").expect("set ATM_WRITE_FIXTURE to the output file");
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work.sqlite");
    let mut conn = Connection::open(&work).unwrap();
    legacy_v4_database(&mut conn);
    let _ = fs::remove_file(&target);
    // VACUUM INTO writes a compact copy without free pages, so nothing deleted earlier can remain in the file.
    conn.execute("VACUUM INTO ?1", [&target]).unwrap();
}

#[test]
fn the_committed_v4_fixture_migrates_to_v5_through_the_real_file_path_with_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atm.sqlite");
    fs::copy(SYNTHETIC_V4_FIXTURE, &path).unwrap();
    let raw = Connection::open(&path).unwrap();
    assert_eq!(db::user_version(&raw).unwrap(), 4);
    assert!(dump_all(&raw).contains(OTHER), "the fixture demonstrates the leak before migration");
    let before = preserved_state(&raw);
    drop(raw);
    let backups = dir.path().join("backups");
    let conn = db::open_file(&path, Some(&backups)).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
    assert_eq!(preserved_state(&conn), before);
    assert_clean_after_migration(&conn);
    let made: Vec<_> = fs::read_dir(&backups).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert!(!made.is_empty(), "a pre-migration backup was written before the schema changed");
}
