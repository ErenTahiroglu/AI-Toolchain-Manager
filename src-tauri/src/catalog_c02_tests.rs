//! CATALOG-02 (Issue #27): the Playwright family. `microsoft-playwright` (Test / Library, from CATALOG-01) gets two siblings,
//! `microsoft-playwright-cli` and `microsoft-playwright-mcp`, through an additive data file. Everything here runs on in-memory databases and the
//! embedded JSON; nothing is fetched, executed, installed or written outside the test.

use crate::catalog::{self, CatalogEntry, SEED_ENTRY_COUNT};
use rusqlite::Connection;
use serde_json::Value;

const FAMILY: [&str; 3] = ["microsoft-playwright", "microsoft-playwright-cli", "microsoft-playwright-mcp"];
const C02: [&str; 2] = ["microsoft-playwright-cli", "microsoft-playwright-mcp"];
const SIXTEEN: [&str; 16] = [
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

fn health(conn: &Connection) -> &'static str {
    let ctx = crate::health::HealthContext {
        tray: crate::health::TrayState::NotAttempted,
        autostart: crate::health::AutostartState::NotQueried,
        previous_shutdown: crate::health::PreviousShutdown::FirstRun,
    };
    crate::health::run(conn, &ctx).checks.iter().find(|c| c.id == "catalog-invariants").unwrap().status
}

fn lf_sha(text: &str) -> String {
    crate::audit::sha256_hex(&text.replace("\r\n", "\n"))
}

fn state(conn: &Connection) -> Vec<(String, String, String, String, bool, bool)> {
    catalog::list_catalog(conn)
        .unwrap()
        .into_iter()
        .map(|e| (e.entry_id, e.record.to_string(), e.purpose, e.capability_fingerprint, e.enabled, e.removed_at.is_some()))
        .collect()
}

// ---- 16 -> 18, and the sixteen are untouched ---------------------------------------------------------------------------------------------

#[test]
fn a_fresh_database_has_the_sixteen_earlier_entries_plus_exactly_the_two_siblings() {
    let conn = seeded();
    assert_eq!(SEED_ENTRY_COUNT, 18, "9 Phase 0 + 4 Phase 1B + 3 CATALOG-01 + 2 CATALOG-02");
    let mut ids: Vec<String> = catalog::list_catalog(&conn).unwrap().into_iter().map(|e| e.entry_id).collect();
    ids.sort();
    let mut expected: Vec<String> = SIXTEEN.iter().chain(C02.iter()).map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(ids, expected);
    let keys: std::collections::BTreeSet<String> = catalog::list_catalog(&conn).unwrap().into_iter().map(|e| e.source_key).collect();
    assert_eq!(keys.len(), 18, "no duplicate source key");
    assert_eq!(health(&conn), "passed");
}

#[test]
fn the_three_earlier_data_files_are_byte_identical_and_their_records_unchanged() {
    assert_eq!(lf_sha(include_str!("../../data/catalog-seed.json")), "8341414b7671630e92fd7ac1a86916299434fcbc56a8475d9c74cfa129c73ad2");
    assert_eq!(
        lf_sha(include_str!("../../data/catalog-candidates-1b.json")),
        "235b1ef8754f094f77287ef3ba52f793ac29390a61570236a7e8e01e613ba11e"
    );
    assert_eq!(
        lf_sha(include_str!("../../data/catalog-candidates-c01.json")),
        "2d1a104f31946dbb26f3134bfb511cdb55d8f6f2fe64496b08b9268fbf65278e"
    );
    let conn = seeded();
    let c01: Value = serde_json::from_str(include_str!("../../data/catalog-candidates-c01.json")).unwrap();
    for wrapper in c01["records"].as_array().unwrap() {
        let id = wrapper["record"]["toolId"].as_str().unwrap();
        assert_eq!(entry(&conn, id).record, wrapper["record"], "{id}: the stored record is the CATALOG-01 record");
    }
}

#[test]
fn the_framework_entry_keeps_its_identity_and_source_evidence() {
    let conn = seeded();
    let e = entry(&conn, "microsoft-playwright");
    assert_eq!((e.kind.as_str(), e.record["category"].as_str().unwrap()), ("executable_tool", "library"));
    assert_eq!(
        (e.repo_url.as_str(), e.source_key.as_str(), e.license_spdx.as_deref()),
        ("https://github.com/microsoft/playwright", "microsoft/playwright", Some("Apache-2.0"))
    );
    assert_eq!(e.record["sourceSnapshot"]["immutableCommit"], "91a765d1681cb3eeb0d0fb89fefec3cf0d21bd9b");
    assert_eq!(e.record["sourceSnapshot"]["evidenceIds"], serde_json::json!(["R35"]));
    assert!(e.record["privacy"]["notes"].to_string().contains("not a ready-to-use MCP server"));
}

// ---- the two new identities ---------------------------------------------------------------------------------------------------------------

#[test]
fn each_sibling_has_the_expected_kind_category_license_source_and_no_positive_claim() {
    let conn = seeded();
    // (id, kind, category, repo, commit, source_key, package/runtime note must appear)
    let expected = [
        (
            "microsoft-playwright-cli",
            "executable_tool",
            "native-cli",
            "https://github.com/microsoft/playwright-cli",
            "b1a5903c5b633b8c9f843ad7b629c2a78437040c",
            "microsoft/playwright-cli",
        ),
        (
            "microsoft-playwright-mcp",
            "agent_plugin",
            "mcp-server",
            "https://github.com/microsoft/playwright-mcp",
            "0c4107acbecb75472b23c05c0e42471a0011874f",
            "microsoft/playwright-mcp",
        ),
    ];
    for (id, kind, category, repo, commit, key) in expected {
        let e = entry(&conn, id);
        assert_eq!((e.kind.as_str(), e.record["category"].as_str().unwrap()), (kind, category), "{id}");
        assert_eq!((e.license_state.as_str(), e.license_spdx.as_deref()), ("known", Some("Apache-2.0")), "{id}");
        assert_eq!((e.repo_url.as_str(), e.source_key.as_str()), (repo, key), "{id}");
        assert_eq!(e.record["sourceSnapshot"]["immutableCommit"], commit, "{id}");
        assert_eq!(e.record["sourceSnapshot"]["revisionState"], "immutable-commit", "{id}");
        assert!(e.record["sourceSnapshot"]["releaseVersion"].is_null(), "{id}: no release is claimed");
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
        assert!(e.enabled && e.removed_at.is_none(), "{id}: visible as a candidate (informational inclusion only)");
        assert!(e.plugin_identity.is_none() && e.source_path.is_none(), "{id}");
        assert!(e.record["artifactEvidence"].is_null(), "{id}: no artifact hash or signature is claimed");
        assert_eq!(e.record["verificationClaims"][0]["state"], "publisher-describes", "{id}");
        assert!(
            e.limitations.iter().any(|l| l == "İncelenmedi.") && e.limitations.iter().any(|l| l.contains("2026-10-24 tarihli")),
            "{id}: {:?}",
            e.limitations
        );
        assert!(e.limitations.iter().any(|l| l.contains("uyumluluk bilinmiyor")), "{id}: unknown targets are stated");
    }
    // the runtime requirements are recorded as the manifests state them, and the MCP disagreement is disclosed rather than guessed away
    assert!(entry(&conn, "microsoft-playwright-cli").record["dependencies"].to_string().contains("Node.js 18 or newer"));
    let mcp = entry(&conn, "microsoft-playwright-mcp").record["dependencies"].to_string();
    assert!(
        mcp.contains("Node.js 18 or newer")
            && mcp.contains("says 20")
            && mcp.contains("not reconciled")
            && mcp.contains("no install requirement is approved")
    );
}

#[test]
fn the_three_are_distinct_candidates_not_one_entry_in_three_disguises() {
    let conn = seeded();
    let all: Vec<CatalogEntry> = FAMILY.iter().map(|id| entry(&conn, id)).collect();
    let distinct = |f: &dyn Fn(&CatalogEntry) -> String| all.iter().map(f).collect::<std::collections::BTreeSet<_>>().len();
    assert_eq!(distinct(&|e| e.entry_id.clone()), 3);
    assert_eq!(distinct(&|e| e.source_key.clone()), 3);
    assert_eq!(distinct(&|e| e.repo_url.clone()), 3);
    assert_eq!(distinct(&|e| e.capability_fingerprint.clone()), 3, "a decision about one is never reused for another");
    assert_eq!(distinct(&|e| e.record["sourceSnapshot"]["immutableCommit"].to_string()), 3);
    assert_eq!(distinct(&|e| e.purpose.clone()), 3);
    let cats: Vec<&str> = all.iter().map(|e| e.record["category"].as_str().unwrap()).collect();
    assert_eq!(cats, ["library", "native-cli", "mcp-server"]);
}

// ---- readable summaries --------------------------------------------------------------------------------------------------------------------

#[test]
fn each_family_summary_is_two_or_three_sentences_within_the_cap_and_says_what_when_and_what_to_watch() {
    let conn = seeded();
    // (id, a phrase that says WHAT it is, a phrase that says WHEN to choose it over another one, a phrase that is the limitation / privacy caveat)
    let expected = [
        ("microsoft-playwright", "uçtan uca tarayıcı testlerini", "yerini tutmaz", "çalışma zamanı indirir"),
        ("microsoft-playwright-cli", "komut satırı aracı", "MCP'den hafif", "Çerez ve oturum durumu tutabilir"),
        ("microsoft-playwright-mcp", "MCP sunucusu olarak açar", "CLI'dan zengindir", "jeton maliyeti yüksektir"),
    ];
    for (id, what, when, caveat) in expected {
        let p = entry(&conn, id).purpose;
        assert!(p.chars().count() <= 300, "{id}: {} characters", p.chars().count());
        let sentences = p.matches(". ").count() + usize::from(p.ends_with('.'));
        assert!((2..=3).contains(&sentences), "{id}: {sentences} sentences in {p:?}");
        for needle in [what, when, caveat] {
            assert!(p.contains(needle), "{id} lacks {needle:?}: {p}");
        }
        assert!(p.to_lowercase().contains("bu uygulama"), "{id}: says what this application does NOT do");
        // never a copied README: no URL, no command, no markdown
        for bad in ["http", "npm ", "npx", "```", "playwright-cli ", "README"] {
            assert!(!p.contains(bad), "{id} contains {bad:?}");
        }
    }
}

#[test]
fn the_refined_framework_summary_reaches_old_databases_without_touching_anything_else() {
    let conn = seeded();
    let refined = entry(&conn, "microsoft-playwright");
    // an older database holds the CATALOG-01 wording
    conn.execute("UPDATE catalog_entries SET purpose = 'eski CATALOG-01 metni' WHERE entry_id = 'microsoft-playwright'", []).unwrap();
    catalog::set_enabled(&conn, "microsoft-playwright", false).unwrap();
    let before = entry(&conn, "microsoft-playwright");
    catalog::seed_all(&conn).unwrap();
    let after = entry(&conn, "microsoft-playwright");
    assert_eq!(after.purpose, refined.purpose, "the refined wording is applied by the ordinary metadata refresh");
    assert_ne!(after.purpose, before.purpose);
    assert!(!after.enabled, "the user's choice is never touched");
    assert_eq!(
        (&after.record, &after.capability_fingerprint, &after.revision),
        (&before.record, &before.capability_fingerprint, &before.revision),
        "record and fingerprint are unchanged"
    );
}

// ---- compatibility: only what the sources say, unknown never promoted ---------------------------------------------------------------------

#[test]
fn compatibility_labels_are_exactly_what_the_snapshots_document() {
    let conn = seeded();
    // [(local-cli, claude-code-terminal, antigravity-ide)] for both operating systems
    let expected = [
        ("microsoft-playwright-cli", ["unknown", "unknown", "unknown"]),
        ("microsoft-playwright-mcp", ["unknown", "vendor-declared", "unknown"]),
    ];
    for (id, states) in expected {
        let e = entry(&conn, id);
        for (surface, state) in [CLI, CC, AG].into_iter().zip(states) {
            for platform in [WIN, MAC] {
                assert_eq!(catalog::compatibility(&e, platform, surface), state, "{id} / {platform} / {surface}");
            }
        }
        assert_eq!(catalog::compatibility(&e, WIN, "claude-desktop-code"), "unknown", "{id}: a surface the record does not mention");
        let rows = e.record["targetEvidence"].as_array().unwrap();
        assert_eq!(rows.len(), 6, "{id}: 2 platforms x 3 surfaces, each stated");
        assert!(rows.iter().all(|r| matches!(r["compatibilityState"].as_str(), Some("vendor-declared" | "unknown"))), "{id}");
        assert!(rows.iter().all(|r| r["limitation"].as_str().is_some_and(|l| !l.is_empty())), "{id}");
        assert!(rows.iter().all(|r| r.get("testedAt").is_none() && r.get("independentCheckId").is_none()), "{id}: nothing was tested");
    }
    // the preferred-default option (CLI) has NO declared combination, and that is stated, not hidden
    let cli = entry(&conn, "microsoft-playwright-cli");
    assert!(cli.limitations.iter().any(|l| l.contains("6 platform/ajan birleşiminde uyumluluk bilinmiyor")), "{:?}", cli.limitations);
}

#[test]
fn an_unknown_combination_cannot_be_accepted_and_nothing_is_installed_by_choosing() {
    let conn = seeded();
    let alias = "ornek-web-sitesi";
    for platform in [WIN, MAC] {
        for surface in [CLI, CC, AG] {
            let err =
                crate::projects::set_preference(&conn, alias, "microsoft-playwright-cli", surface, platform, "accepted", None).unwrap_err();
            assert_eq!(err.code, "PREFERENCE_NOT_ACTIVATABLE", "cli / {platform} / {surface}");
        }
    }
    // an ordinary 'not now' / 'reject' is always possible, and the one declared MCP combination can be requested - as a wish only
    assert!(crate::projects::set_preference(&conn, alias, "microsoft-playwright-cli", CC, WIN, "postponed", Some(30)).is_ok());
    assert!(crate::projects::set_preference(&conn, alias, "microsoft-playwright-mcp", AG, WIN, "rejected", None).is_ok());
    assert!(crate::projects::set_preference(&conn, alias, "microsoft-playwright-mcp", CC, WIN, "accepted", None).is_ok());
    // two of the family can be wished for at once: no false exclusivity
    assert!(crate::projects::set_preference(&conn, alias, "microsoft-playwright", CLI, MAC, "accepted", None).is_ok());
    assert_eq!(
        n(&conn, "SELECT count(*) FROM project_preferences WHERE entry_id LIKE 'microsoft-playwright%' AND decision = 'accepted'"),
        2
    );
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences WHERE actual_install_state <> 'not_managed' OR actual_verification_state <> 'unknown'"), 0);
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE install_state <> 'not_managed' OR verification_state <> 'unknown'"), 0);
}

#[test]
fn arriving_in_the_catalog_creates_no_wish_recommendation_or_activation() {
    let conn = seeded();
    for id in C02 {
        assert_eq!(n(&conn, &format!("SELECT count(*) FROM project_preferences WHERE entry_id = '{id}'")), 0, "{id}");
        assert_eq!(n(&conn, &format!("SELECT count(*) FROM preference_history WHERE entry_id = '{id}'")), 0, "{id}");
    }
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences"), 6, "only the six demonstration decisions of earlier phases");
}

#[test]
fn records_carry_no_executable_authority_secret_or_unexpected_url() {
    let conn = seeded();
    for id in C02 {
        let record = entry(&conn, id).record;
        let text = record.to_string();
        for bad in [
            "curl ",
            "wget ",
            "| bash",
            "| sh",
            "Invoke-Expression",
            "irm ",
            "powershell -",
            "--secrets",
            "API_KEY",
            "Authorization",
            "sk-",
            "ghp_",
            "claude mcp add",
            "npm install -g",
            "npx @playwright",
        ] {
            assert!(!text.contains(bad), "{id} contains {bad:?}");
        }
        let methods: Vec<&str> = record["installMethods"].as_array().unwrap().iter().map(|m| m["methodId"].as_str().unwrap()).collect();
        assert_eq!(methods, ["unknown"], "{id}: no approved method");
        assert!(record["installMethods"].as_array().unwrap().iter().all(|m| m.get("command").is_none() && m.get("sha256").is_none()));
        for url in text.match_indices("http").map(|(i, _)| text[i..].split(['"', ' ']).next().unwrap()) {
            assert!(url.starts_with("https://github.com/microsoft/playwright-"), "{id}: unexpected URL {url}");
        }
    }
}

// ---- upgrade 16 -> 18, repeat bootstrap, removed / disabled ------------------------------------------------------------------------------

#[test]
fn upgrading_a_sixteen_entry_database_adds_exactly_the_two_and_keeps_every_choice() {
    let conn = seeded();
    // the state of a database seeded by CATALOG-01: the two sibling rows are not there yet
    conn.execute_batch("DELETE FROM catalog_entries WHERE entry_id IN ('microsoft-playwright-cli','microsoft-playwright-mcp')").unwrap();
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 16);
    catalog::set_enabled(&conn, "exa-mcp-server", false).unwrap();
    catalog::remove_entry(&conn, "apify-mcp-server").unwrap();
    crate::projects::add_reference(&conn, "proje", "Proje", "").unwrap();
    crate::projects::set_preference(&conn, "proje", "microsoft-playwright", CLI, WIN, "accepted", None).unwrap();
    catalog::set_enabled(&conn, "microsoft-playwright", false).unwrap();
    let prefs = n(&conn, "SELECT count(*) FROM project_preferences");
    let history = n(&conn, "SELECT count(*) FROM preference_history");
    let audit = n(&conn, "SELECT coalesce(max(sequence), 0) FROM audit_events");
    let older = state(&conn);

    catalog::seed_all(&conn).unwrap();
    catalog::seed_all(&conn).unwrap();

    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 18);
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences"), prefs);
    assert_eq!(n(&conn, "SELECT count(*) FROM preference_history"), history);
    assert_eq!(n(&conn, "SELECT coalesce(max(sequence), 0) FROM audit_events"), audit, "seeding writes no audit event");
    let now = state(&conn);
    for old in &older {
        assert_eq!(now.iter().find(|e| e.0 == old.0).unwrap(), old, "{}: unchanged by the upgrade", old.0);
    }
    assert!(!entry(&conn, "exa-mcp-server").enabled && !entry(&conn, "microsoft-playwright").enabled);
    assert!(entry(&conn, "apify-mcp-server").removed_at.is_some());
    for id in C02 {
        assert!(entry(&conn, id).enabled && entry(&conn, id).origin == "seed");
    }
    assert_eq!(health(&conn), "passed");
    assert!(crate::audit::verify(&conn).unwrap().ok);
}

#[test]
fn repeat_seeding_is_idempotent_and_never_resurrects_a_removed_or_disabled_sibling() {
    let conn = seeded();
    let first = state(&conn);
    for _ in 0..3 {
        catalog::seed_all(&conn).unwrap();
    }
    assert_eq!(state(&conn), first);
    catalog::set_enabled(&conn, "microsoft-playwright-cli", false).unwrap();
    catalog::remove_entry(&conn, "microsoft-playwright-mcp").unwrap();
    catalog::seed_all(&conn).unwrap();
    assert!(!entry(&conn, "microsoft-playwright-cli").enabled);
    assert!(entry(&conn, "microsoft-playwright-mcp").removed_at.is_some());
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 18);
    assert_eq!(health(&conn), "passed");
}

// ---- collisions with entries the user added from a link ----------------------------------------------------------------------------------

fn taken(conn: &Connection, id: &str) -> i64 {
    n(conn, &format!("SELECT count(*) FROM diagnostics WHERE code = 'SEED_SOURCE_TAKEN' AND detail LIKE '%{id}%'"))
}

#[test]
fn user_added_links_to_the_siblings_win_are_reported_and_never_duplicated_or_refined() {
    let conn = crate::db::open_memory().unwrap();
    // the link-derived ids equal the seeds' ids, so the user owns both ids; the framework link too
    let cli = catalog::add_entry(&conn, "https://github.com/microsoft/playwright-cli", "executable_tool", "benim notum").unwrap();
    let mcp = catalog::add_entry(&conn, "https://github.com/microsoft/playwright-mcp", "agent_plugin", "").unwrap();
    let pw = catalog::add_entry(&conn, "https://github.com/microsoft/playwright", "executable_tool", "kendi amacım").unwrap();
    assert_eq!(
        (cli.entry_id.as_str(), mcp.entry_id.as_str(), pw.entry_id.as_str()),
        ("microsoft-playwright-cli", "microsoft-playwright-mcp", "microsoft-playwright")
    );
    catalog::seed_all(&conn).unwrap();
    catalog::seed_all(&conn).unwrap();

    // 13 earlier + Apify + Exa + the three user rows; the three colliding seeds were not inserted
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 18);
    for source in ["microsoft/playwright", "microsoft/playwright-cli", "microsoft/playwright-mcp"] {
        assert_eq!(n(&conn, &format!("SELECT count(*) FROM catalog_entries WHERE source_key = '{source}'")), 1, "{source}");
    }
    for id in FAMILY {
        let e = entry(&conn, id);
        assert_eq!(
            (e.origin.as_str(), e.metadata_state.as_str(), e.record["userAdded"].clone()),
            ("user", "not-fetched", Value::Bool(true)),
            "{id}"
        );
        assert!(taken(&conn, id) >= 1, "{id}: the collision is reported");
    }
    assert_eq!(entry(&conn, "microsoft-playwright-cli").purpose, "benim notum");
    assert_eq!(entry(&conn, "microsoft-playwright").purpose, "kendi amacım", "a user's own purpose is never replaced by the refined text");
    assert_eq!(catalog::seed_entries_held_by_the_user(&conn).unwrap(), 3);
    assert_eq!(health(&conn), "passed");
}

#[test]
fn a_removed_user_entry_for_a_sibling_stays_removed_and_the_invariant_explains_the_gap() {
    let conn = crate::db::open_memory().unwrap();
    let mcp = catalog::add_entry(&conn, "https://github.com/microsoft/playwright-mcp", "agent_plugin", "").unwrap();
    catalog::remove_entry(&conn, &mcp.entry_id).unwrap();
    catalog::seed_all(&conn).unwrap();
    assert!(entry(&conn, "microsoft-playwright-mcp").removed_at.is_some());
    assert_eq!(entry(&conn, "microsoft-playwright-mcp").origin, "user");
    assert_eq!(health(&conn), "passed");
    // the explanation covers only entries the user owns: a truncated catalog still fails
    let conn = seeded();
    conn.execute_batch("DELETE FROM catalog_entries WHERE entry_id = 'microsoft-playwright-cli'").unwrap();
    assert_eq!(catalog::seed_entries_held_by_the_user(&conn).unwrap(), 0);
    assert_eq!(health(&conn), "failed");
}

// ---- description-quality gate for the whole catalog ----------------------------------------------------------------------------------------

/// The machine-checkable half of the catalog-wide description-quality requirement (docs/catalog-02/NEXT_BACKLOG.md): the criteria an entry's
/// stored data can fail. "Relevant project situations" cannot be checked by a machine and stays a manual review item.
fn quality_gaps(e: &CatalogEntry) -> Vec<&'static str> {
    let r = &e.record;
    let mut gaps = Vec::new();
    let chars = e.purpose.chars().count();
    if !(100..=300).contains(&chars) || e.purpose.contains("ayrıntılı incelenmedi") || e.purpose.contains("henüz yazılmadı") {
        gaps.push("Q1 purpose summary: missing, placeholder or too terse (under 100 characters)");
    }
    if !(r["license"]["state"] == "known" && r["license"]["spdxId"].is_string() && r["license"]["evidenceId"].is_string()) {
        gaps.push("Q2 license: not a known SPDX license with evidence");
    }
    if !(r["sourceSnapshot"]["immutableCommit"].is_string() && r["sourceSnapshot"]["observedAt"].is_string()) {
        gaps.push("Q3 source: no immutable pin or evidence date");
    }
    if r["dependencies"].as_array().is_none_or(|d| d.is_empty()) {
        gaps.push("Q4 runtime requirements: none stated");
    }
    if r["networkClaims"].as_array().is_none_or(|d| d.is_empty()) || r["privacy"]["notes"].as_array().is_none_or(|d| d.is_empty()) {
        gaps.push("Q5 network / cost / privacy implications: not stated");
    }
    if r["targetEvidence"].as_array().is_none_or(|t| t.is_empty() || t.iter().any(|x| x["limitation"].as_str().is_none_or(str::is_empty))) {
        gaps.push("Q6 compatibility: unknown cells without a stated limitation");
    }
    gaps
}

#[test]
fn the_description_quality_gaps_of_the_catalog_are_exactly_the_documented_backlog() {
    let conn = seeded();
    // entry -> codes of the criteria it fails TODAY. This list is the audit result recorded in docs/catalog-02/NEXT_BACKLOG.md; it may only
    // shrink when an entry is fixed, and a NEW entry must pass every criterion or be added here on purpose.
    let documented: &[(&str, &[&str])] = &[
        ("ponytail", &["Q1", "Q3"]),
        ("codebase-memory-mcp", &["Q1", "Q4"]),
        ("graphify", &["Q1"]),
        ("gitleaks", &["Q1", "Q4"]),
        ("markitdown", &["Q1"]),
        ("caveman", &["Q1"]),
        ("cloudflare-security-audit-skill", &["Q1"]),
        ("superpowers", &["Q1", "Q4"]),
        ("betterleaks", &["Q1", "Q4"]),
        ("anthropic-frontend-design", &["Q4"]),
        ("agentic-awesome-skills", &["Q2", "Q4"]),
    ];
    let mut actual: Vec<(String, Vec<String>)> = catalog::list_catalog(&conn)
        .unwrap()
        .iter()
        .map(|e| (e.entry_id.clone(), quality_gaps(e).iter().map(|g| g[..2].to_string()).collect::<Vec<_>>()))
        .filter(|(_, g)| !g.is_empty())
        .collect();
    actual.sort();
    let mut expected: Vec<(String, Vec<String>)> =
        documented.iter().map(|(id, g)| (id.to_string(), g.iter().map(|s| s.to_string()).collect())).collect();
    expected.sort();
    assert_eq!(actual, expected);
    // the entries added in CATALOG-01 and CATALOG-02 meet every machine-checkable criterion
    for id in ["microsoft-playwright", "microsoft-playwright-cli", "microsoft-playwright-mcp", "apify-mcp-server", "exa-mcp-server"] {
        assert!(quality_gaps(&entry(&conn, id)).is_empty(), "{id}: {:?}", quality_gaps(&entry(&conn, id)));
    }
}

// ---- the loader stays pure data --------------------------------------------------------------------------------------------------------------

#[test]
fn the_loader_embeds_the_new_data_file_and_has_no_process_network_or_file_access() {
    let production = include_str!("catalog.rs").split("#[cfg(test)]").next().unwrap();
    assert!(production.contains("include_str!(\"../../data/catalog-candidates-c02.json\")"));
    for forbidden in ["std::process", "Command::", "std::net", "reqwest", "TcpStream", "std::fs", "File::open", "std::env"] {
        assert!(!production.contains(forbidden), "catalog.rs production code uses {forbidden}");
    }
}
