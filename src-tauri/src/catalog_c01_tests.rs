//! CATALOG-01 (Issue #26): three metadata-only candidates (Microsoft Playwright, Apify MCP Server, Exa MCP Server) added through an additive data file.
//! Everything here runs on in-memory databases and the embedded JSON; nothing is fetched, executed, installed or written outside the test.

use crate::catalog::{self, CatalogEntry, SEED_ENTRY_COUNT};
use rusqlite::Connection;
use serde_json::Value;

const NEW_IDS: [&str; 3] = ["microsoft-playwright", "apify-mcp-server", "exa-mcp-server"];
/// CATALOG-02 (Issue #27) added these two to the same catalog; they are part of every full-catalog expectation below.
const C02_IDS: [&str; 2] = ["microsoft-playwright-cli", "microsoft-playwright-mcp"];
const PRIOR_IDS: [&str; 13] = [
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
];
const WIN: &str = "windows-x64";
const MAC: &str = "macos-arm64";
const CC: &str = "claude-code-terminal";
const AG: &str = "antigravity-ide";
const CLI: &str = "local-cli";

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

fn health(conn: &Connection, id: &str) -> &'static str {
    let ctx = crate::health::HealthContext {
        tray: crate::health::TrayState::NotAttempted,
        autostart: crate::health::AutostartState::NotQueried,
        previous_shutdown: crate::health::PreviousShutdown::FirstRun,
    };
    crate::health::run(conn, &ctx).checks.iter().find(|c| c.id == id).unwrap().status
}

fn lf_sha(text: &str) -> String {
    crate::audit::sha256_hex(&text.replace("\r\n", "\n"))
}

// ---- exactly the 13 prior entries plus exactly three new ones -------------------------------------------------------------------------------

#[test]
fn a_fresh_database_has_the_same_thirteen_entries_plus_exactly_the_three_candidates() {
    let conn = seeded();
    let all = catalog::list_catalog(&conn).unwrap();
    assert_eq!(SEED_ENTRY_COUNT, 18, "9 Phase 0 + 4 Phase 1B + 3 CATALOG-01 + 2 CATALOG-02");
    assert_eq!(all.len(), 18);
    let mut ids: Vec<&str> = all.iter().map(|e| e.entry_id.as_str()).collect();
    ids.sort_unstable();
    let mut expected: Vec<&str> = PRIOR_IDS.iter().chain(NEW_IDS.iter()).chain(C02_IDS.iter()).copied().collect();
    expected.sort_unstable();
    assert_eq!(ids, expected);
    let mut keys: Vec<&str> = all.iter().map(|e| e.source_key.as_str()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 18, "no duplicate source key");
}

#[test]
fn the_historical_seed_files_and_their_records_are_untouched() {
    // byte-level pins (line endings normalised) of the two files that existed before CATALOG-01
    assert_eq!(lf_sha(include_str!("../../data/catalog-seed.json")), "8341414b7671630e92fd7ac1a86916299434fcbc56a8475d9c74cfa129c73ad2");
    assert_eq!(
        lf_sha(include_str!("../../data/catalog-candidates-1b.json")),
        "235b1ef8754f094f77287ef3ba52f793ac29390a61570236a7e8e01e613ba11e"
    );
    // and the stored records of the 13 are exactly those files' records
    let conn = seeded();
    for record in catalog::parse_seed().unwrap() {
        let id = record["toolId"].as_str().unwrap();
        assert_eq!(entry(&conn, id).record, record, "{id}");
    }
    let candidates: Value = serde_json::from_str(include_str!("../../data/catalog-candidates-1b.json")).unwrap();
    for wrapper in candidates["records"].as_array().unwrap() {
        let id = wrapper["record"]["toolId"].as_str().unwrap();
        assert_eq!(entry(&conn, id).record, wrapper["record"], "{id}");
    }
}

#[test]
fn each_candidate_has_the_expected_kind_license_source_and_no_positive_claim() {
    let conn = seeded();
    // (id, kind, category, spdx, repo, commit, source_key)
    let expected = [
        (
            "microsoft-playwright",
            "executable_tool",
            "library",
            "Apache-2.0",
            "https://github.com/microsoft/playwright",
            "91a765d1681cb3eeb0d0fb89fefec3cf0d21bd9b",
            "microsoft/playwright",
        ),
        (
            "apify-mcp-server",
            "agent_plugin",
            "mcp-server",
            "MIT",
            "https://github.com/apify/apify-mcp-server",
            "cdf5b23230b26cd42e5d0db81528365f6b9cdc9c",
            "apify/apify-mcp-server",
        ),
        (
            "exa-mcp-server",
            "agent_plugin",
            "mcp-server",
            "MIT",
            "https://github.com/exa-labs/exa-mcp-server",
            "0b499f095c4ca145eee07b09137641c037d58449",
            "exa-labs/exa-mcp-server",
        ),
    ];
    for (id, kind, category, spdx, repo, commit, key) in expected {
        let e = entry(&conn, id);
        assert_eq!((e.kind.as_str(), e.record["category"].as_str().unwrap()), (kind, category), "{id}");
        assert_eq!((e.license_state.as_str(), e.license_spdx.as_deref()), ("known", Some(spdx)), "{id}");
        assert_eq!((e.repo_url.as_str(), e.source_key.as_str()), (repo, key), "{id}");
        assert_eq!(e.record["sourceSnapshot"]["immutableCommit"], commit, "{id}");
        assert_eq!(e.record["sourceSnapshot"]["revisionState"], "immutable-commit", "{id}");
        assert!(e.revision.ends_with(&format!("@{}", &commit[..12])), "{id}: {}", e.revision);
        assert_eq!(
            (e.origin.as_str(), e.metadata_state.as_str(), e.review_state.as_str()),
            ("seed", "documented-snapshot", "unreviewed"),
            "{id}"
        );
        assert_eq!(
            (e.install_state.as_str(), e.verification_state.as_str(), e.execution_policy.as_str()),
            ("not_managed", "unknown", "manual-only"),
            "{id}"
        );
        assert!(e.enabled, "{id}: visible as a candidate (informational inclusion only)");
        assert!(
            e.removed_at.is_none() && e.plugin_identity.is_none() && e.source_path.is_none(),
            "{id}: no unverified plugin identity or sub-path"
        );
        assert_eq!(e.record["verificationClaims"][0]["state"], "publisher-describes", "{id}");
        assert!(e.record["artifactEvidence"].is_null(), "{id}: no artifact hash or signature is claimed");
        assert!(e.limitations.iter().any(|l| l == "İncelenmedi."), "{id}");
        assert!(e.limitations.iter().any(|l| l.contains("2026-10-24 tarihli")), "{id}: {:?}", e.limitations);
        assert!(e.limitations.iter().any(|l| l.contains("Kurulum ve işlev doğrulaması yok")), "{id}");
        assert!(e.limitations.iter().any(|l| l.contains("uyumluluk bilinmiyor")), "{id}: unknown targets are stated");
    }
}

#[test]
fn the_tool_kinds_and_scope_are_what_the_issue_asked_for() {
    let conn = seeded();
    let pw = entry(&conn, "microsoft-playwright");
    // the main repository is the framework, never presented as a ready-to-use MCP server
    let notes = pw.record["privacy"]["notes"].to_string();
    assert!(
        notes.contains("not a ready-to-use MCP server")
            && notes.contains("microsoft/playwright-mcp")
            && notes.contains("microsoft/playwright-cli")
    );
    assert_ne!(pw.record["category"], "mcp-server");
    // the other repositories of the family are never folded into this entry: CATALOG-01 did not catalogue them, CATALOG-02 added each as its own entry
    for (other, own_id) in
        [("microsoft/playwright-cli", "microsoft-playwright-cli"), ("microsoft/playwright-mcp", "microsoft-playwright-mcp")]
    {
        assert_eq!(n(&conn, &format!("SELECT count(*) FROM catalog_entries WHERE source_key = '{other}'")), 1);
        assert_eq!(n(&conn, &format!("SELECT count(*) FROM catalog_entries WHERE source_key = '{other}' AND entry_id = '{own_id}'")), 1);
    }
    assert_eq!(pw.source_key, "microsoft/playwright");
    assert!(entry(&conn, "apify-mcp-server").record["networkClaims"].to_string().contains("https://mcp.apify.com"));
    assert!(entry(&conn, "exa-mcp-server").record["networkClaims"].to_string().contains("https://mcp.exa.ai/mcp"));
}

// ---- compatibility labels follow the sources, and unknown is never promoted -----------------------------------------------------------

#[test]
fn target_labels_are_exactly_what_the_snapshots_document() {
    let conn = seeded();
    // (id, [(surface, windows state, macos state)])
    let expected = [
        ("microsoft-playwright", [(CLI, "vendor-declared"), (CC, "unknown"), (AG, "unknown")]),
        ("apify-mcp-server", [(CLI, "vendor-declared"), (CC, "vendor-declared"), (AG, "unknown")]),
        ("exa-mcp-server", [(CLI, "unknown"), (CC, "unknown"), (AG, "unknown")]),
    ];
    for (id, surfaces) in expected {
        let e = entry(&conn, id);
        for (surface, state) in surfaces {
            for platform in [WIN, MAC] {
                assert_eq!(catalog::compatibility(&e, platform, surface), state, "{id} / {platform} / {surface}");
            }
        }
        // a surface the record does not mention is unknown too, and no row is tested or unsupported
        assert_eq!(catalog::compatibility(&e, WIN, "claude-desktop-code"), "unknown", "{id}");
        let rows = e.record["targetEvidence"].as_array().unwrap();
        assert_eq!(rows.len(), 6, "{id}: 2 platforms x 3 surfaces, each stated");
        assert!(rows.iter().all(|r| matches!(r["compatibilityState"].as_str(), Some("vendor-declared" | "unknown"))), "{id}");
        assert!(rows.iter().all(|r| r["limitation"].as_str().is_some_and(|l| !l.is_empty())), "{id}: every label carries its limitation");
        assert!(rows.iter().all(|r| r.get("testedAt").is_none() && r.get("independentCheckId").is_none()), "{id}: nothing was tested");
    }
}

#[test]
fn nothing_is_accepted_recommended_or_active_by_arriving_in_the_catalog() {
    let conn = seeded();
    for id in NEW_IDS {
        assert_eq!(n(&conn, &format!("SELECT count(*) FROM project_preferences WHERE entry_id = '{id}'")), 0, "{id}: no wish");
        assert_eq!(n(&conn, &format!("SELECT count(*) FROM preference_history WHERE entry_id = '{id}'")), 0, "{id}");
    }
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences"), 6, "only the six demonstration decisions of earlier phases");
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE install_state <> 'not_managed' OR verification_state <> 'unknown'"), 0);
    // an unknown combination cannot even be requested as 'accepted'
    let alias = "ornek-web-sitesi";
    for id in ["exa-mcp-server", "microsoft-playwright"] {
        let err = crate::projects::set_preference(&conn, alias, id, AG, WIN, "accepted", None).unwrap_err();
        assert_eq!(err.code, "PREFERENCE_NOT_ACTIVATABLE", "{id}");
    }
    // while an honest 'not now' / 'reject' stays possible
    assert!(crate::projects::set_preference(&conn, alias, "exa-mcp-server", CC, WIN, "postponed", Some(30)).is_ok());
}

#[test]
fn records_carry_no_executable_authority_secret_or_forbidden_text() {
    let conn = seeded();
    for id in NEW_IDS {
        let text = entry(&conn, id).record.to_string();
        for bad in [
            "curl ",
            "wget ",
            "| bash",
            "| sh",
            "Invoke-Expression",
            "irm ",
            "powershell -",
            "exaApiKey",
            "APIFY_TOKEN",
            "Authorization",
            "sk-",
            "ghp_",
            "apify_api_",
        ] {
            assert!(!text.contains(bad), "{id} contains {bad:?}");
        }
        let record = entry(&conn, id).record;
        let methods: Vec<&str> = record["installMethods"].as_array().unwrap().iter().map(|m| m["methodId"].as_str().unwrap()).collect();
        assert!(methods.iter().all(|m| matches!(*m, "unknown" | "native-plugin-manager")), "{id}: {methods:?}");
        assert!(record["installMethods"].as_array().unwrap().iter().all(|m| m.get("command").is_none() && m.get("sha256").is_none()));
    }
}

// ---- repeat bootstrap, upgrade with decisions, removed/disabled entries --------------------------------------------------------------

fn fingerprintable_state(conn: &Connection) -> Vec<(String, String, String, bool, bool)> {
    catalog::list_catalog(conn)
        .unwrap()
        .into_iter()
        .map(|e| (e.entry_id, e.record.to_string(), e.capability_fingerprint, e.enabled, e.removed_at.is_some()))
        .collect()
}

#[test]
fn seeding_again_changes_nothing_and_creates_no_duplicate() {
    let conn = seeded();
    let before = (fingerprintable_state(&conn), n(&conn, "SELECT count(*) FROM diagnostics WHERE code = 'SEED_SOURCE_TAKEN'"));
    for _ in 0..3 {
        catalog::seed_all(&conn).unwrap();
    }
    assert_eq!((fingerprintable_state(&conn), n(&conn, "SELECT count(*) FROM diagnostics WHERE code = 'SEED_SOURCE_TAKEN'")), before);
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 18);
    assert_eq!(health(&conn, "catalog-invariants"), "passed");
}

#[test]
fn upgrading_an_existing_database_adds_the_three_and_keeps_every_choice() {
    let conn = seeded();
    // the state of a database that predates CATALOG-01 (and so CATALOG-02): none of the later rows is there yet
    conn.execute_batch(
        "DELETE FROM catalog_entries WHERE entry_id IN ('microsoft-playwright','apify-mcp-server','exa-mcp-server','microsoft-playwright-cli','microsoft-playwright-mcp')",
    )
    .unwrap();
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 13);
    // the user's own choices on the older entries
    catalog::set_enabled(&conn, "agent-browser", false).unwrap();
    catalog::remove_entry(&conn, "gitleaks").unwrap();
    crate::projects::add_reference(&conn, "proje", "Proje", "").unwrap();
    crate::projects::set_preference(&conn, "proje", "betterleaks", CLI, WIN, "rejected", None).unwrap();
    let prefs = n(&conn, "SELECT count(*) FROM project_preferences");
    let history = n(&conn, "SELECT count(*) FROM preference_history");
    let audit_head: Option<String> =
        conn.query_row("SELECT max(sequence) FROM audit_events", [], |r| r.get::<_, Option<i64>>(0)).unwrap().map(|s| s.to_string());
    let old_state: Vec<_> = fingerprintable_state(&conn);

    catalog::seed_all(&conn).unwrap();
    catalog::seed_all(&conn).unwrap();

    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 18);
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences"), prefs);
    assert_eq!(n(&conn, "SELECT count(*) FROM preference_history"), history);
    let max_seq: Option<String> =
        conn.query_row("SELECT max(sequence) FROM audit_events", [], |r| r.get::<_, Option<i64>>(0)).unwrap().map(|s| s.to_string());
    assert_eq!(max_seq, audit_head, "seeding writes no audit event");
    assert!(!entry(&conn, "agent-browser").enabled, "a disabled entry stays disabled");
    assert!(entry(&conn, "gitleaks").removed_at.is_some(), "a removed entry stays removed");
    for old in old_state {
        let now = fingerprintable_state(&conn).into_iter().find(|e| e.0 == old.0).unwrap();
        assert_eq!(now, old, "{}: the older entries are byte-for-byte as they were", old.0);
    }
    for id in NEW_IDS {
        assert!(entry(&conn, id).enabled);
    }
    assert_eq!(health(&conn, "catalog-invariants"), "passed");
}

#[test]
fn a_new_seed_never_resurrects_a_candidate_the_user_removed_or_disabled() {
    let conn = seeded();
    catalog::set_enabled(&conn, "exa-mcp-server", false).unwrap();
    catalog::remove_entry(&conn, "apify-mcp-server").unwrap();
    catalog::seed_all(&conn).unwrap();
    assert!(!entry(&conn, "exa-mcp-server").enabled);
    assert!(entry(&conn, "apify-mcp-server").removed_at.is_some());
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 18);
}

// ---- collisions with an entry the user added from a link ------------------------------------------------------------------------------

fn taken_diagnostics(conn: &Connection, id: &str) -> i64 {
    n(conn, &format!("SELECT count(*) FROM diagnostics WHERE code = 'SEED_SOURCE_TAKEN' AND detail LIKE '%{id}%'"))
}

#[test]
fn a_user_added_source_wins_and_the_collision_is_reported_without_a_duplicate() {
    let conn = crate::db::open_memory().unwrap();
    // the user had already added two of the three repositories from a link (different ids for Apify and Exa, the SAME id for Playwright)
    let apify = catalog::add_entry(&conn, "https://github.com/apify/apify-mcp-server", "agent_plugin", "benim notum").unwrap();
    let pw = catalog::add_entry(&conn, "https://github.com/microsoft/playwright", "executable_tool", "").unwrap();
    assert_eq!(apify.entry_id, "apify-apify-mcp-server");
    assert_eq!(pw.entry_id, "microsoft-playwright", "the link-derived id equals the suggested seed id");
    catalog::seed_all(&conn).unwrap();
    catalog::seed_all(&conn).unwrap();

    // 13 shipped earlier + Exa + the two CATALOG-02 seeds + the two user rows; the two colliding seeds were not inserted
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 18);
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE source_key = 'apify/apify-mcp-server'"), 1);
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE source_key = 'microsoft/playwright'"), 1);
    let apify_after = entry(&conn, "apify-apify-mcp-server");
    assert_eq!(
        (apify_after.origin.as_str(), apify_after.purpose.as_str(), apify_after.review_state.as_str()),
        ("user", "benim notum", "unreviewed")
    );
    assert_eq!(apify_after.record["userAdded"], true, "the user's record was not replaced by the seed");
    let pw_after = entry(&conn, "microsoft-playwright");
    assert_eq!(
        (pw_after.origin.as_str(), pw_after.metadata_state.as_str(), pw_after.display_name.as_str()),
        ("user", "not-fetched", "microsoft/playwright")
    );
    assert_eq!(pw_after.record["userAdded"], true);
    assert!(catalog::get_entry(&conn, "apify-mcp-server").is_err(), "no seed row for the owned source");
    assert_eq!(entry(&conn, "exa-mcp-server").origin, "seed", "the third candidate is seeded normally");
    // reported (at every start, like the existing collision report) - never silent
    assert!(taken_diagnostics(&conn, "apify-mcp-server") >= 1 && taken_diagnostics(&conn, "microsoft-playwright") >= 1);
    assert_eq!(taken_diagnostics(&conn, "exa-mcp-server"), 0);
    // and the explained absence does not turn the health invariant into a failure
    assert_eq!(catalog::seed_entries_held_by_the_user(&conn).unwrap(), 2);
    assert_eq!(health(&conn, "catalog-invariants"), "passed");
}

#[test]
fn a_removed_user_entry_keeps_its_id_and_the_health_invariant_still_explains_the_gap() {
    let conn = crate::db::open_memory().unwrap();
    let pw = catalog::add_entry(&conn, "https://github.com/microsoft/playwright", "executable_tool", "").unwrap();
    catalog::remove_entry(&conn, &pw.entry_id).unwrap();
    catalog::seed_all(&conn).unwrap();
    assert!(entry(&conn, "microsoft-playwright").removed_at.is_some(), "the user's removal is respected, nothing is revived");
    assert_eq!(entry(&conn, "microsoft-playwright").origin, "user");
    assert_eq!(health(&conn, "catalog-invariants"), "passed");
}

#[test]
fn a_database_without_the_shipped_seeds_still_fails_the_invariant() {
    // the explanation applies only to entries the user owns: an empty or truncated catalog is still a failure
    let conn = crate::db::open_memory().unwrap();
    assert_eq!(health(&conn, "catalog-invariants"), "failed");
    let conn = seeded();
    conn.execute_batch("DELETE FROM catalog_entries WHERE entry_id = 'exa-mcp-server'").unwrap();
    assert_eq!(catalog::seed_entries_held_by_the_user(&conn).unwrap(), 0);
    assert_eq!(health(&conn, "catalog-invariants"), "failed");
}

// ---- the older entries' displayed text and the loader's boundaries -----------------------------------------------------------------------

#[test]
fn the_read_date_in_the_limitations_is_the_one_the_record_states() {
    let conn = seeded();
    let date = |id: &str| entry(&conn, id).limitations.into_iter().find(|l| l.starts_with("Bilgiler ")).unwrap();
    // CATALOG-01: read 2026-10-24; Phase 1B candidates: 2026-10-17 (unchanged); Phase 0 records: stamped 2026-10-10 (this line used to say 10-17 for them too)
    assert!(date("exa-mcp-server").contains("2026-10-24 tarihli"));
    assert!(date("strix").contains("2026-10-17 tarihli"));
    assert!(date("gitleaks").contains("2026-10-10 tarihli"));
    // a user-added entry still shows the offline text, not a date
    let user = catalog::add_entry(&conn, "https://github.com/acme/widget", "agent_plugin", "").unwrap();
    assert!(
        user.limitations.iter().any(|l| l.contains("Çevrimdışı kayıt")) && !user.limitations.iter().any(|l| l.starts_with("Bilgiler "))
    );
}

#[test]
fn the_catalog_loader_is_pure_data_no_process_network_or_file_access() {
    let src = include_str!("catalog.rs");
    let production = src.split("#[cfg(test)]").next().unwrap();
    for forbidden in ["std::process", "Command::", "std::net", "reqwest", "TcpStream", "std::fs", "File::open", "tauri::api", "std::env"] {
        assert!(!production.contains(forbidden), "catalog.rs production code uses {forbidden}");
    }
    // the new data file is embedded at build time, not read at run time
    assert!(production.contains("include_str!(\"../../data/catalog-candidates-c01.json\")"));
}
