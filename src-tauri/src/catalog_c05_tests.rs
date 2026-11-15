//! CATALOG-05 (Issue #56): one metadata-only candidate, Unslop, through an additive data file. Everything here runs on in-memory databases and the embedded
//! JSON; nothing is fetched, executed, installed or written outside the test, and Unslop, Playwright, a browser or an image service is never started or contacted.

use crate::catalog::{self, CatalogEntry, SEED_ENTRY_COUNT};
use crate::catalog_c02_tests::quality_gaps;
use crate::{gate, plan, projects};
use rusqlite::Connection;
use serde_json::Value;

const ID: &str = "heydamir-unslop";
const LINK: &str = "https://github.com/heydamir/unslop";
const COMMIT: &str = "620163eedb3529d9cd23a3994ed8d287d06f760d";
const WIN: &str = "windows-x64";
const MAC: &str = "macos-arm64";
const EARLIER: [&str; 23] = [
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
    "egonex-ai-understand-anything",
    "thedotmack-claude-mem",
    "multica-ai-andrej-karpathy-skills",
    "ruvnet-ruflo",
    "google-elephant-goldfish-model",
];
/// The eleven project target surfaces (AGENT-01 and AGENT-02). None of them may make Unslop acceptable or runnable.
const SURFACES: [&str; 11] = [
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
    "local-cli",
];
const DOCUMENTED_SURFACES: [&str; 3] = ["claude-code-terminal", "codex-cli", "antigravity-ide"];

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

// ---- A. metadata and schema -------------------------------------------------------------------------------------------------------------

#[test]
fn a_fresh_database_has_the_twenty_three_earlier_entries_plus_exactly_the_unslop_entry() {
    let conn = seeded();
    let all = catalog::list_catalog(&conn).unwrap();
    assert_eq!(SEED_ENTRY_COUNT, 24, "9 Phase 0 + 4 Phase 1B + 3 CATALOG-01 + 2 CATALOG-02 + 4 CATALOG-03 + 1 CATALOG-04 + 1 CATALOG-05");
    assert_eq!(all.len(), 24, "count(*) of all rows");
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE origin = 'seed'"), 24);
    let mut ids: Vec<&str> = all.iter().map(|e| e.entry_id.as_str()).collect();
    ids.sort_unstable();
    let mut expected: Vec<&str> = EARLIER.iter().copied().chain([ID]).collect();
    expected.sort_unstable();
    assert_eq!(ids, expected);
    let keys: std::collections::BTreeSet<&str> = all.iter().map(|e| e.source_key.as_str()).collect();
    assert_eq!(keys.len(), 24, "no duplicate source key");
    assert_eq!(health(&conn), "passed");
}

#[test]
fn the_unslop_record_is_the_pinned_metadata_only_record() {
    let conn = seeded();
    let e = entry(&conn, ID);
    let r = &e.record;
    assert_eq!(e.display_name, "Unslop");
    assert_eq!(e.repo_url, LINK);
    assert_eq!(r["sourceSnapshot"]["immutableCommit"], COMMIT);
    assert_eq!(r["sourceSnapshot"]["observedRef"], COMMIT, "no moving branch name is recorded as the observed ref");
    assert_eq!(r["sourceSnapshot"]["revisionState"], "immutable-commit");
    assert_eq!(r["sourceSnapshot"]["releaseVersion"], Value::Null);
    assert_eq!(r["sourceSnapshot"]["observedAt"], "2026-11-15T00:00:00Z");
    assert_eq!(r["sourceSnapshot"]["evidenceIds"], serde_json::json!(["R61"]));
    assert_eq!(r["license"], serde_json::json!({ "state": "known", "spdxId": "CC-BY-4.0", "evidenceId": "R61" }));
    assert_eq!((e.license_state.as_str(), e.license_spdx.as_deref()), ("known", Some("CC-BY-4.0")));
    // one skill shared by three hosts, packaged for Claude Code as a plugin: the same pairing as the other single-skill source
    assert_eq!((e.kind.as_str(), r["category"].as_str().unwrap()), ("skill_collection", "agent-skill"));
    assert_eq!(entry(&conn, "cloudflare-security-audit-skill").kind, e.kind);
    assert_eq!((e.review_state.as_str(), e.execution_policy.as_str()), ("unreviewed", "blocked-until-scoped-authorization"));
    assert_eq!(
        (e.source_path.clone(), e.plugin_identity.clone()),
        (None, None),
        "one canonical repository identity, no second plugin-vs-skill record"
    );
    assert_eq!(e.source_key, "heydamir/unslop");
    let parsed = crate::github_url::parse(LINK).unwrap();
    assert_eq!(e.source_key, parsed.source_key, "the seed's source key equals the key a user link derives");
    assert_eq!((e.install_state.as_str(), e.verification_state.as_str()), ("not_managed", "unknown"));
    assert!(e.enabled && e.removed_at.is_none(), "visible in the catalog; visibility grants no authority");
    assert_eq!(r["privacy"]["telemetry"], "unknown");
    assert_eq!(r["privacy"]["mayTransmitProjectContent"], Value::Bool(true));
    assert_eq!(r["artifactEvidence"], Value::Null, "no artifact, checksum or signature is recorded");
    assert!(quality_gaps(&e).is_empty(), "{:?}", quality_gaps(&e));
    // the attribution the licence asks for is in the record
    let notes = r["privacy"]["notes"].to_string();
    assert!(notes.contains("Damir Sokolovsky") && notes.contains("CC BY 4.0") && notes.contains(LINK), "{notes}");
}

#[test]
fn compatibility_is_unknown_on_every_documented_host_and_platform_and_nowhere_else_is_implied() {
    let conn = seeded();
    let e = entry(&conn, ID);
    let targets = e.record["targetEvidence"].as_array().unwrap();
    assert_eq!(targets.len(), 6, "three documented hosts on two platforms, nothing padded");
    let mut seen: Vec<(String, String)> = targets
        .iter()
        .map(|t| (t["platformTarget"].as_str().unwrap().to_string(), t["agentSurface"].as_str().unwrap().to_string()))
        .collect();
    seen.sort();
    let mut want: Vec<(String, String)> =
        [WIN, MAC].iter().flat_map(|p| DOCUMENTED_SURFACES.iter().map(move |s| (p.to_string(), s.to_string()))).collect();
    want.sort();
    assert_eq!(seen, want);
    for t in targets {
        assert_eq!(t["compatibilityState"], "unknown");
        assert!(!t["limitation"].as_str().unwrap().is_empty());
        assert_eq!(t["evidenceId"], "R61");
        assert!(t.get("independentCheckId").is_none() && t.get("testedAt").is_none() && t.get("harnessVersion").is_none());
    }
    assert!(!e.record.to_string().contains("independently-tested"), "no row claims an independent test");
    assert!(!e.record.to_string().contains("vendor-declared"), "the publisher names hosts but no platform, so nothing is vendor-declared");
    // no surface is inferred from a sibling: every other combination has no row and therefore stays unknown
    for platform in [WIN, MAC] {
        for surface in SURFACES {
            assert_eq!(catalog::compatibility(&e, platform, surface), "unknown", "{platform}/{surface}");
        }
    }
    for s in ["codex-vscode", "cline-cli", "cline-vscode", "cline-jetbrains", "kiro-cli", "kiro-ide", "claude-desktop-code", "local-cli"] {
        assert!(!e.record.to_string().contains(&format!("\"agentSurface\":\"{s}\"")), "{s} has no evidence row");
    }
    let claims = e.record["verificationClaims"].to_string();
    assert!(claims.contains("publisher-describes") && claims.contains("not-run"));
    assert!(!claims.contains("independently"), "{claims}");
}

#[test]
fn install_methods_are_labels_and_the_record_carries_no_executable_text() {
    let conn = seeded();
    let e = entry(&conn, ID);
    let methods = e.record["installMethods"].as_array().unwrap();
    let ids: Vec<&str> = methods.iter().map(|m| m["methodId"].as_str().unwrap()).collect();
    assert_eq!(ids, ["native-plugin-manager", "manual-instructions"]);
    for m in methods {
        assert!(m.get("command").is_none() && m.get("sha256").is_none() && m.get("url").is_none(), "label only: {m}");
    }
    let text = e.record.to_string();
    for needle in [
        "curl ",
        "wget ",
        "iwr ",
        "iex ",
        "irm ",
        "| bash",
        "| sh",
        "Invoke-Expression",
        "powershell -",
        "npm install",
        "npx ",
        "pip install",
        "git clone",
        "cp -r",
        "mkdir ",
        "claude plugin",
        "/plugin ",
        "skill-installer",
        "API_KEY",
        "Authorization",
        "sk-",
        "ghp_",
    ] {
        assert!(!text.contains(needle), "the record contains {needle:?}");
    }
    assert_eq!(e.record["dependencies"].as_array().unwrap().len(), 4);
    assert!(e.record["privilegeClaims"].to_string().contains("127.0.0.1"), "the loopback server of the story script is disclosed");
    assert!(e.record["privilegeClaims"].to_string().contains("Chromium"), "the browser launch is disclosed");
    assert!(e.record["networkClaims"].to_string().contains("image service"), "the optional paid image service is disclosed");
}

// ---- B. database behaviour --------------------------------------------------------------------------------------------------------------

#[test]
fn upgrading_a_twenty_three_entry_database_adds_exactly_the_unslop_row_and_keeps_every_choice() {
    let conn = seeded();
    // the state of a database seeded by CATALOG-04: the Unslop row is not there yet
    conn.execute_batch("DELETE FROM catalog_entries WHERE entry_id = 'heydamir-unslop'").unwrap();
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 23);
    catalog::set_enabled(&conn, "agent-browser", false).unwrap();
    catalog::remove_entry(&conn, "gitleaks").unwrap();
    projects::add_reference(&conn, "proje", "Proje", "").unwrap();
    projects::set_preference(&conn, "proje", "betterleaks", "local-cli", WIN, "rejected", None).unwrap();
    projects::set_preference(&conn, "proje", "microsoft-playwright-mcp", "codex-cli", WIN, "postponed", Some(3)).unwrap();
    projects::set_preference(&conn, "proje", "microsoft-playwright-mcp", "kiro-ide", MAC, "rejected", None).unwrap();
    let prefs = n(&conn, "SELECT count(*) FROM project_preferences");
    let history = n(&conn, "SELECT count(*) FROM preference_history");
    let audit = n(&conn, "SELECT coalesce(max(sequence), 0) FROM audit_events");
    let preference_rows: Vec<String> = {
        let mut stmt = conn.prepare("SELECT * FROM project_preferences ORDER BY rowid").unwrap();
        let cols = stmt.column_count();
        stmt.query_map([], |r| Ok((0..cols).map(|i| format!("{:?}", r.get_ref(i).unwrap())).collect::<Vec<_>>().join("|")))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    let older = state(&conn);

    catalog::seed_all(&conn).unwrap();
    catalog::seed_all(&conn).unwrap();

    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 24, "count(*) of all rows");
    let e = entry(&conn, ID);
    assert_eq!((e.origin.as_str(), e.enabled, e.removed_at.is_none()), ("seed", true, true));
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences"), prefs);
    assert_eq!(n(&conn, "SELECT count(*) FROM preference_history"), history);
    assert_eq!(n(&conn, "SELECT coalesce(max(sequence), 0) FROM audit_events"), audit, "seeding writes no audit event");
    let after_prefs: Vec<String> = {
        let mut stmt = conn.prepare("SELECT * FROM project_preferences ORDER BY rowid").unwrap();
        let cols = stmt.column_count();
        stmt.query_map([], |r| Ok((0..cols).map(|i| format!("{:?}", r.get_ref(i).unwrap())).collect::<Vec<_>>().join("|")))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    assert_eq!(after_prefs, preference_rows, "every preference row, including the codex-cli and AGENT-02 surface rows, is byte-identical");
    let after: Vec<_> = state(&conn).into_iter().filter(|row| row.0 != ID).collect();
    assert_eq!(after, older, "every older entry row, including the user's choices, is unchanged");
    assert_eq!((e.install_state.as_str(), e.verification_state.as_str()), ("not_managed", "unknown"));
    assert_eq!(health(&conn), "passed");
    assert!(crate::audit::verify(&conn).unwrap().ok);
}

#[test]
fn seeding_twice_creates_no_duplicate_and_changes_nothing() {
    let conn = seeded();
    let first = state(&conn);
    for _ in 0..3 {
        catalog::seed_all(&conn).unwrap();
    }
    assert_eq!(state(&conn), first);
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE source_key = 'heydamir/unslop'"), 1);
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 24);
    assert_eq!(health(&conn), "passed");
}

#[test]
fn an_unslop_link_the_user_added_earlier_wins_and_is_never_duplicated() {
    let conn = crate::db::open_memory().unwrap();
    let added = catalog::add_entry(&conn, LINK, "skill_collection", "benim notum").unwrap();
    assert_eq!(added.entry_id, ID, "the link-derived id equals the seed's id");
    catalog::seed_all(&conn).unwrap();
    catalog::seed_all(&conn).unwrap();

    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 24, "count(*) of all rows");
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE origin = 'seed'"), 23, "count(*) of the seed rows");
    let e = entry(&conn, ID);
    assert_eq!((e.origin.as_str(), e.revision.as_str(), e.purpose.as_str()), ("user", "user-added", "benim notum"));
    assert_eq!(e.record["userAdded"], Value::Bool(true));
    assert!(taken(&conn, ID) >= 1, "the collision is reported");
    assert_eq!(catalog::seed_entries_held_by_the_user(&conn).unwrap(), 1);
    assert_eq!(health(&conn), "passed");

    // a user row the user removed stays removed and the seed is not revived
    let conn = crate::db::open_memory().unwrap();
    catalog::add_entry(&conn, LINK, "skill_collection", "").unwrap();
    catalog::remove_entry(&conn, ID).unwrap();
    catalog::seed_all(&conn).unwrap();
    let e = entry(&conn, ID);
    assert_eq!(e.origin, "user");
    assert!(e.removed_at.is_some());

    // without a user owner, a missing seed is a failed invariant
    let conn = seeded();
    conn.execute_batch("DELETE FROM catalog_entries WHERE entry_id = 'heydamir-unslop'").unwrap();
    assert_eq!(catalog::seed_entries_held_by_the_user(&conn).unwrap(), 0);
    assert_eq!(health(&conn), "failed");
}

#[test]
fn every_spelling_of_the_unslop_link_added_after_the_seed_is_refused_or_restores_the_removed_seed() {
    let conn = seeded();
    for link in [
        LINK,
        "https://github.com/heydamir/unslop.git",
        "HTTPS://GITHUB.COM/HeyDamir/Unslop",
        "https://github.com/heydamir/unslop/",
        "https://github.com/HEYDAMIR/UNSLOP.git",
    ] {
        assert_eq!(catalog::add_entry(&conn, link, "skill_collection", "").unwrap_err().code, "ENTRY_DUPLICATE", "{link}");
        assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 24, "count(*) of all rows after {link}");
    }
    catalog::remove_entry(&conn, ID).unwrap();
    let restored = catalog::add_entry(&conn, LINK, "skill_collection", "").unwrap();
    assert_eq!(restored.entry_id, ID);
    assert!(restored.removed_at.is_none());
    assert!(!restored.enabled, "a restored entry starts disabled");

    let conn = seeded();
    catalog::remove_entry(&conn, ID).unwrap();
    assert_eq!(catalog::add_entry(&conn, LINK, "agent_plugin", "").unwrap_err().code, "KIND_CHANGE_NOT_ALLOWED");
}

#[test]
fn a_disabled_or_removed_unslop_entry_stays_that_way_after_a_restart() {
    let conn = seeded();
    catalog::set_enabled(&conn, ID, false).unwrap();
    catalog::seed_all(&conn).unwrap();
    assert!(!entry(&conn, ID).enabled);
    catalog::remove_entry(&conn, ID).unwrap();
    catalog::seed_all(&conn).unwrap();
    assert!(entry(&conn, ID).removed_at.is_some());
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 24);
    assert_eq!(health(&conn), "passed");
}

// ---- C. security invariants -------------------------------------------------------------------------------------------------------------

#[test]
fn acceptance_is_refused_on_every_surface_and_platform_but_postpone_and_reject_stay_possible() {
    let conn = seeded();
    projects::add_reference(&conn, "proje-bir", "Benim sitem", "").unwrap();
    let baseline = n(&conn, "SELECT count(*) FROM project_preferences");
    for s in SURFACES {
        for platform in [WIN, MAC] {
            let err = projects::set_preference(&conn, "proje-bir", ID, s, platform, "accepted", None).unwrap_err();
            assert_eq!(
                err.code, "ACTIVATION_BLOCKED",
                "{platform}/{s}: the execution policy refuses acceptance before any compatibility is looked at"
            );
        }
    }
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences"), baseline, "nothing was stored by a refused acceptance");
    // informational decisions remain available and write only the preference and its history
    projects::set_preference(&conn, "proje-bir", ID, "codex-cli", WIN, "postponed", Some(2)).unwrap();
    projects::set_preference(&conn, "proje-bir", ID, "kiro-ide", MAC, "rejected", None).unwrap();
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences"), baseline + 2);
}

#[test]
fn nothing_can_run_or_be_planned_for_unslop() {
    let conn = seeded();
    let e = entry(&conn, ID);
    for action in ["run", "install", "activate", "execute", "scan"] {
        assert_eq!(gate::check_execution(&e, action, None).unwrap_err().code, gate::AUTHORIZATION_REQUIRED, "{action}");
    }
    let host = plan::HostFacts { platform: WIN.to_string(), elevated: Some(false) };
    for surface in plan::SURFACES {
        for platform in [WIN, MAC] {
            let r = plan::PlanRequest {
                entry_id: ID.to_string(),
                platform_target: platform.to_string(),
                agent_surface: surface.to_string(),
                scope: "user".to_string(),
                method_id: None,
                project_alias: None,
            };
            if let Ok(p) = plan::plan_for(&conn, &r, &host) {
                assert!(!p.executable && !p.executed && !p.modifies_anything, "{platform}/{surface}");
                assert!(!p.eligibility.can_execute, "{platform}/{surface}");
                assert!(p.eligibility.blockers.iter().any(|b| b.code == "execution_policy_blocked"), "{platform}/{surface}");
                assert!(
                    p.steps.iter().all(|s| s.typed.is_none()),
                    "{platform}/{surface}: no typed operation exists without a reviewed recipe"
                );
            }
        }
    }
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE entry_id = 'heydamir-unslop' AND install_state <> 'not_managed'"), 0);
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE entry_id = 'heydamir-unslop' AND verification_state <> 'unknown'"), 0);
}

#[test]
fn the_unslop_wiring_adds_no_program_socket_file_or_environment_access_and_no_other_file_names_the_id() {
    let production = include_str!("catalog.rs").split("#[cfg(test)]").next().unwrap();
    assert!(production.contains("include_str!(\"../../data/catalog-candidates-c05.json\")"));
    for forbidden in ["std::process", "Command::", "std::net", "reqwest", "TcpStream", "std::fs", "File::open", "std::env"] {
        assert!(!production.contains(forbidden), "catalog.rs production code uses {forbidden}");
    }
    // the privileged layers neither name the entry nor the repository
    for (name, src) in [
        ("plan.rs", include_str!("plan.rs")),
        ("recipe.rs", include_str!("recipe.rs")),
        ("gate.rs", include_str!("gate.rs")),
        ("install_flow.rs", include_str!("install_flow.rs")),
        ("projects.rs", include_str!("projects.rs")),
        ("recommend.rs", include_str!("recommend.rs")),
    ] {
        let production = src.split("#[cfg(test)]").next().unwrap();
        assert!(!production.contains("heydamir") && !production.contains("unslop"), "{name} names Unslop");
    }
    let lib = include_str!("lib.rs");
    assert!(!lib.contains("unslop") && !lib.contains("heydamir"), "lib.rs registers no Unslop command");
}
