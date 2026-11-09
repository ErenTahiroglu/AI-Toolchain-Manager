//! CATALOG-04: one metadata-only candidate, the Elephant-Goldfish Model (EGM), through an additive data file. Everything here runs on in-memory
//! databases and the embedded JSON; nothing is fetched, executed, installed or written outside the test, and EGM is never run, started or contacted.

use crate::catalog::{self, CatalogEntry, SEED_ENTRY_COUNT};
use crate::catalog_c02_tests::quality_gaps;
use rusqlite::Connection;
use serde_json::Value;

const C03: [&str; 4] = ["egonex-ai-understand-anything", "thedotmack-claude-mem", "multica-ai-andrej-karpathy-skills", "ruvnet-ruflo"];
const C04: [&str; 1] = ["google-elephant-goldfish-model"];
const EARLIER: [&str; 18] = [
    "ponytail",
    "codebase-memory-mcp",
    "graphify",
    "gitleaks",
    "markitdown",
    "caveman",
    "cloudflare-security-audit-skill",
    "superpowers",
    "betterleaks",
    "anthropic-frontend-design",
    "strix",
    "agent-browser",
    "agentic-awesome-skills",
    "microsoft-playwright",
    "apify-mcp-server",
    "exa-mcp-server",
    "microsoft-playwright-cli",
    "microsoft-playwright-mcp",
];
const WIN: &str = "windows-x64";
const CLI: &str = "local-cli";
const EGM_LINK: &str = "https://github.com/google/elephant-goldfish-model";
const EGM_PURPOSE: &str = "Tasarım belgelerini taze bağlamlı yorumcu ajanlarla inceleyen Claude Code eklentisi (Google Elephant-Goldfish Model). Belgeler ajanın kendi model sağlayıcısında işlenir; bootstrap ajanı dosya yazma ve kabuk araçlarını bildirir. Bu uygulama kurmaz, çalıştırmaz.";

fn seeded() -> Connection {
    let conn = crate::db::open_memory().unwrap();
    catalog::seed_all(&conn).unwrap();
    conn
}

fn n(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn entry(conn: &Connection, id: &str) -> CatalogEntry {
    catalog::get_entry(conn, id).unwrap()
}

fn health(conn: &Connection) -> &'static str {
    let ctx = crate::health::HealthContext {
        tray: crate::health::TrayState::NotAttempted,
        autostart: crate::health::AutostartState::NotQueried,
        previous_shutdown: crate::health::PreviousShutdown::FirstRun,
    };
    crate::health::run(conn, &ctx).checks.iter().find(|c| c.id == "catalog-invariants").unwrap().status
}

fn state(conn: &Connection) -> Vec<(String, String, String, String, bool, bool)> {
    catalog::list_catalog(conn)
        .unwrap()
        .into_iter()
        .map(|e| (e.entry_id, e.record.to_string(), e.purpose, e.capability_fingerprint, e.enabled, e.removed_at.is_some()))
        .collect()
}

fn taken(conn: &Connection, id: &str) -> i64 {
    n(conn, &format!("SELECT count(*) FROM diagnostics WHERE code = 'SEED_SOURCE_TAKEN' AND detail LIKE '%{id}%'"))
}

#[test]
fn a_fresh_database_has_the_twenty_two_earlier_entries_plus_exactly_the_egm_entry() {
    let conn = seeded();
    let all = catalog::list_catalog(&conn).unwrap();
    assert_eq!(SEED_ENTRY_COUNT, 23, "9 Phase 0 + 4 Phase 1B + 3 CATALOG-01 + 2 CATALOG-02 + 4 CATALOG-03 + 1 CATALOG-04");
    assert_eq!(all.len(), 23, "count(*) of all rows");
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE origin = 'seed'"), 23, "count(*) of the seed rows");
    let mut ids: Vec<&str> = all.iter().map(|e| e.entry_id.as_str()).collect();
    ids.sort_unstable();
    let mut expected: Vec<&str> = EARLIER.iter().chain(C03.iter()).chain(C04.iter()).copied().collect();
    expected.sort_unstable();
    assert_eq!(ids, expected);
    let keys: std::collections::BTreeSet<&str> = all.iter().map(|e| e.source_key.as_str()).collect();
    assert_eq!(keys.len(), 23, "no duplicate source key");
    assert_eq!(health(&conn), "passed");
}

#[test]
fn the_egm_record_is_the_pinned_metadata_only_record() {
    let conn = seeded();
    let e = entry(&conn, "google-elephant-goldfish-model");
    let r = &e.record;
    assert_eq!(e.display_name, "Elephant-Goldfish Model");
    assert_eq!(e.repo_url, EGM_LINK);
    assert_eq!(r["sourceSnapshot"]["observedRef"], "main");
    assert_eq!(r["sourceSnapshot"]["immutableCommit"], "e9e1f06aa4e26e95cd4ed37210a0632202c429e9");
    assert_eq!(r["sourceSnapshot"]["releaseVersion"], Value::Null);
    assert_eq!(r["sourceSnapshot"]["revisionState"], "immutable-commit");
    assert_eq!(r["sourceSnapshot"]["observedAt"], "2026-11-08T00:00:00Z");
    assert_eq!(r["sourceSnapshot"]["evidenceIds"], serde_json::json!(["R57"]));
    assert_eq!(r["license"], serde_json::json!({ "state": "known", "spdxId": "Apache-2.0", "evidenceId": "R57" }));
    assert_eq!((e.kind.as_str(), r["category"].as_str().unwrap()), ("agent_plugin", "agent-plugin"));
    assert_eq!((e.review_state.as_str(), e.execution_policy.as_str()), ("unreviewed", "manual-only"));
    assert_eq!((e.source_path.clone(), e.plugin_identity.clone()), (None, None));
    let parsed = crate::github_url::parse(EGM_LINK).unwrap();
    assert_eq!(e.source_key, parsed.source_key, "the seed's source key equals the key a user link derives");
    assert_eq!((e.install_state.as_str(), e.verification_state.as_str()), ("not_managed", "unknown"));
    assert_eq!(e.purpose, EGM_PURPOSE);
    assert_eq!(r["privacy"]["telemetry"], "none-declared");
    assert_eq!(r["privacy"]["mayTransmitProjectContent"], Value::Bool(true));
    let targets = r["targetEvidence"].as_array().unwrap();
    assert_eq!(targets.len(), 2);
    let platforms: Vec<&str> = targets.iter().map(|t| t["platformTarget"].as_str().unwrap()).collect();
    assert_eq!(platforms, ["windows-x64", "macos-arm64"]);
    for t in targets {
        assert_eq!((t["agentSurface"].as_str().unwrap(), t["compatibilityState"].as_str().unwrap()), ("claude-code-terminal", "unknown"));
    }
    assert!(!r.to_string().contains("independently-tested"), "no row claims an independent test");
    let methods = r["installMethods"].as_array().unwrap();
    assert_eq!(methods.len(), 1);
    assert_eq!(methods[0]["methodId"], "native-plugin-manager");
    assert!(methods[0].get("command").is_none() && methods[0].get("sha256").is_none(), "no install command and no checksum");
    assert!(quality_gaps(&e).is_empty(), "{:?}", quality_gaps(&e));
    // production code that loads the file neither runs a program, nor opens a socket, nor reads a file
    let production = include_str!("catalog.rs").split("#[cfg(test)]").next().unwrap();
    assert!(production.contains("include_str!(\"../../data/catalog-candidates-c04.json\")"));
    for forbidden in ["std::process", "Command::", "std::net", "reqwest", "TcpStream", "std::fs", "File::open", "std::env"] {
        assert!(!production.contains(forbidden), "catalog.rs production code uses {forbidden}");
    }
}

#[test]
fn upgrading_a_twenty_two_entry_database_adds_exactly_the_egm_row_and_keeps_every_choice() {
    let conn = seeded();
    // the state of a database seeded by CATALOG-03: the EGM row is not there yet
    conn.execute_batch("DELETE FROM catalog_entries WHERE entry_id = 'google-elephant-goldfish-model'").unwrap();
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 22);
    catalog::set_enabled(&conn, "agent-browser", false).unwrap();
    catalog::remove_entry(&conn, "gitleaks").unwrap();
    crate::projects::add_reference(&conn, "proje", "Proje", "").unwrap();
    crate::projects::set_preference(&conn, "proje", "betterleaks", CLI, WIN, "rejected", None).unwrap();
    let prefs = n(&conn, "SELECT count(*) FROM project_preferences");
    let history = n(&conn, "SELECT count(*) FROM preference_history");
    let audit = n(&conn, "SELECT coalesce(max(sequence), 0) FROM audit_events");
    let older = state(&conn);

    catalog::seed_all(&conn).unwrap();
    catalog::seed_all(&conn).unwrap();

    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 23, "count(*) of all rows");
    let egm = entry(&conn, "google-elephant-goldfish-model");
    assert_eq!((egm.origin.as_str(), egm.enabled, egm.removed_at.is_none()), ("seed", true, true));
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences"), prefs);
    assert_eq!(n(&conn, "SELECT count(*) FROM preference_history"), history);
    assert_eq!(n(&conn, "SELECT coalesce(max(sequence), 0) FROM audit_events"), audit, "seeding writes no audit event");
    let after: Vec<_> = state(&conn).into_iter().filter(|row| row.0 != "google-elephant-goldfish-model").collect();
    assert_eq!(after, older, "every older row, including the user's choices, is unchanged");
    assert_eq!(health(&conn), "passed");
    assert!(crate::audit::verify(&conn).unwrap().ok);
}

#[test]
fn an_egm_link_the_user_added_earlier_wins_and_is_never_duplicated() {
    let conn = crate::db::open_memory().unwrap();
    let added = catalog::add_entry(&conn, EGM_LINK, "agent_plugin", "benim notum").unwrap();
    assert_eq!(added.entry_id, "google-elephant-goldfish-model", "the link-derived id equals the seed's id");
    catalog::seed_all(&conn).unwrap();
    catalog::seed_all(&conn).unwrap();

    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 23, "count(*) of all rows");
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE origin = 'seed'"), 22, "count(*) of the seed rows");
    let e = entry(&conn, "google-elephant-goldfish-model");
    assert_eq!((e.origin.as_str(), e.revision.as_str(), e.purpose.as_str()), ("user", "user-added", "benim notum"));
    assert_eq!(e.record["userAdded"], Value::Bool(true));
    assert!(taken(&conn, "google-elephant-goldfish-model") >= 1, "the collision is reported");
    assert_eq!(catalog::seed_entries_held_by_the_user(&conn).unwrap(), 1);
    assert_eq!(health(&conn), "passed");

    // a user row the user removed stays removed and the seed is not revived
    let conn = crate::db::open_memory().unwrap();
    catalog::add_entry(&conn, EGM_LINK, "agent_plugin", "").unwrap();
    catalog::remove_entry(&conn, "google-elephant-goldfish-model").unwrap();
    catalog::seed_all(&conn).unwrap();
    let e = entry(&conn, "google-elephant-goldfish-model");
    assert_eq!(e.origin, "user");
    assert!(e.removed_at.is_some());

    // without a user owner, a missing seed is a failed invariant
    let conn = seeded();
    conn.execute_batch("DELETE FROM catalog_entries WHERE entry_id = 'google-elephant-goldfish-model'").unwrap();
    assert_eq!(catalog::seed_entries_held_by_the_user(&conn).unwrap(), 0);
    assert_eq!(health(&conn), "failed");
}

#[test]
fn an_egm_link_added_after_the_seed_is_refused_or_restores_the_removed_seed() {
    let conn = seeded();
    for link in [
        EGM_LINK,
        "https://github.com/google/elephant-goldfish-model.git",
        "HTTPS://GITHUB.COM/Google/Elephant-Goldfish-Model",
        "https://github.com/google/elephant-goldfish-model/",
    ] {
        assert_eq!(catalog::add_entry(&conn, link, "agent_plugin", "").unwrap_err().code, "ENTRY_DUPLICATE", "{link}");
        assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 23, "count(*) of all rows after {link}");
    }
    catalog::remove_entry(&conn, "google-elephant-goldfish-model").unwrap();
    let restored = catalog::add_entry(&conn, EGM_LINK, "agent_plugin", "").unwrap();
    assert_eq!(restored.entry_id, "google-elephant-goldfish-model");
    assert!(restored.removed_at.is_none());
    assert!(!restored.enabled, "a restored entry starts disabled");

    let conn = seeded();
    catalog::remove_entry(&conn, "google-elephant-goldfish-model").unwrap();
    assert_eq!(catalog::add_entry(&conn, EGM_LINK, "executable_tool", "").unwrap_err().code, "KIND_CHANGE_NOT_ALLOWED");
}
