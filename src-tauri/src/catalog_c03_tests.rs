//! CATALOG-03 (Issue #28): four metadata-only candidates - Understand Anything, Claude-Mem, a Karpathy-inspired skill repository and Ruflo - through an
//! additive data file. Everything here runs on in-memory databases and the embedded JSON; nothing is fetched, executed, installed or written outside
//! the test, and none of the four tools is run, started or contacted.

use crate::catalog::{self, CatalogEntry, SEED_ENTRY_COUNT};
use crate::catalog_c02_tests::quality_gaps;
use rusqlite::Connection;
use serde_json::Value;

const C03: [&str; 4] = ["egonex-ai-understand-anything", "thedotmack-claude-mem", "multica-ai-andrej-karpathy-skills", "ruvnet-ruflo"];
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

// ---- 18 -> 22, exactly four identities, earlier bytes untouched ------------------------------------------------------------------------

#[test]
fn a_fresh_database_has_the_eighteen_earlier_entries_plus_exactly_the_four() {
    let conn = seeded();
    assert_eq!(SEED_ENTRY_COUNT, 22, "9 Phase 0 + 4 Phase 1B + 3 CATALOG-01 + 2 CATALOG-02 + 4 CATALOG-03");
    let all = catalog::list_catalog(&conn).unwrap();
    let mut ids: Vec<&str> = all.iter().map(|e| e.entry_id.as_str()).collect();
    ids.sort_unstable();
    let mut expected: Vec<&str> = EARLIER.iter().chain(C03.iter()).copied().collect();
    expected.sort_unstable();
    assert_eq!(ids, expected);
    let keys: std::collections::BTreeSet<&str> = all.iter().map(|e| e.source_key.as_str()).collect();
    assert_eq!(keys.len(), 22, "no duplicate source key");
    // exactly four new source identities, all lower-cased owner/repo keys
    let new_keys: Vec<&str> = all.iter().filter(|e| C03.contains(&e.entry_id.as_str())).map(|e| e.source_key.as_str()).collect();
    let mut sorted = new_keys.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, ["egonex-ai/understand-anything", "multica-ai/andrej-karpathy-skills", "ruvnet/ruflo", "thedotmack/claude-mem"]);
    assert_eq!(health(&conn), "passed");
}

#[test]
fn the_four_earlier_data_files_are_byte_identical_and_their_records_unchanged() {
    assert_eq!(lf_sha(include_str!("../../data/catalog-seed.json")), "8341414b7671630e92fd7ac1a86916299434fcbc56a8475d9c74cfa129c73ad2");
    assert_eq!(
        lf_sha(include_str!("../../data/catalog-candidates-1b.json")),
        "235b1ef8754f094f77287ef3ba52f793ac29390a61570236a7e8e01e613ba11e"
    );
    assert_eq!(
        lf_sha(include_str!("../../data/catalog-candidates-c01.json")),
        "2d1a104f31946dbb26f3134bfb511cdb55d8f6f2fe64496b08b9268fbf65278e"
    );
    assert_eq!(
        lf_sha(include_str!("../../data/catalog-candidates-c02.json")),
        "fac409c6c98eccfc42dc748ef986c0e4dde77fa547494b8814a00dd68cf4b9da"
    );
    let conn = seeded();
    for file in [
        include_str!("../../data/catalog-candidates-1b.json"),
        include_str!("../../data/catalog-candidates-c01.json"),
        include_str!("../../data/catalog-candidates-c02.json"),
    ] {
        let root: Value = serde_json::from_str(file).unwrap();
        for wrapper in root["records"].as_array().unwrap() {
            let id = wrapper["record"]["toolId"].as_str().unwrap();
            assert_eq!(entry(&conn, id).record, wrapper["record"], "{id}: the stored record is the one in its own file");
        }
    }
    for record in catalog::parse_seed().unwrap() {
        let id = record["toolId"].as_str().unwrap();
        assert_eq!(entry(&conn, id).record, record, "{id}");
    }
}

// ---- the four records ------------------------------------------------------------------------------------------------------------------------

#[test]
fn each_record_has_the_expected_identity_kind_licence_pin_and_risk_posture() {
    let conn = seeded();
    // (id, kind, category, licence state, spdx, repo url, commit, review state, execution policy)
    let expected = [
        (
            "egonex-ai-understand-anything",
            "agent_plugin",
            "agent-plugin",
            "known",
            Some("MIT"),
            "https://github.com/Egonex-AI/Understand-Anything",
            "e24fcf6d371bcfa529bc110bacb1bb43de1b8d33",
            "unreviewed",
            "manual-only",
        ),
        (
            "thedotmack-claude-mem",
            "agent_plugin",
            "agent-plugin",
            "known",
            Some("Apache-2.0"),
            "https://github.com/thedotmack/claude-mem",
            "75fc021a40655cbdd258f0e070f55a03ea9c8642",
            "quarantined",
            "blocked-until-scoped-authorization",
        ),
        (
            "multica-ai-andrej-karpathy-skills",
            "skill_collection",
            "agent-skill",
            "review-required",
            None,
            "https://github.com/multica-ai/andrej-karpathy-skills",
            "2c606141936f1eeef17fa3043a72095b4765b9c2",
            "unreviewed",
            "manual-only",
        ),
        (
            "ruvnet-ruflo",
            "agent_plugin",
            "workflow",
            "known",
            Some("MIT"),
            "https://github.com/ruvnet/ruflo",
            "fe99f748c577158ec3e517c7b421670a4601f54f",
            "quarantined",
            "blocked-until-scoped-authorization",
        ),
    ];
    for (id, kind, category, licence, spdx, repo, commit, review, policy) in expected {
        let e = entry(&conn, id);
        assert_eq!((e.kind.as_str(), e.record["category"].as_str().unwrap()), (kind, category), "{id}");
        assert_eq!((e.license_state.as_str(), e.license_spdx.as_deref()), (licence, spdx), "{id}");
        assert_eq!(e.repo_url, repo, "{id}");
        assert_eq!(e.record["sourceSnapshot"]["immutableCommit"], commit, "{id}");
        assert_eq!(e.record["sourceSnapshot"]["revisionState"], "immutable-commit", "{id}");
        assert_eq!(e.record["sourceSnapshot"]["observedAt"], "2026-10-25T00:00:00Z", "{id}");
        assert!(e.record["sourceSnapshot"]["releaseVersion"].is_null(), "{id}: no release is invented");
        assert!(e.revision.ends_with(&format!("@{}", &commit[..12])), "{id}: {}", e.revision);
        assert_eq!((e.review_state.as_str(), e.execution_policy.as_str()), (review, policy), "{id}");
        assert_eq!((e.origin.as_str(), e.metadata_state.as_str()), ("seed", "documented-snapshot"), "{id}");
        assert_eq!((e.install_state.as_str(), e.verification_state.as_str()), ("not_managed", "unknown"), "{id}");
        assert!(e.enabled && e.removed_at.is_none(), "{id}: visible in the catalog (enabled means catalog visibility, not installation)");
        assert!(e.plugin_identity.is_none() && e.source_path.is_none(), "{id}");
        assert!(e.record["artifactEvidence"].is_null(), "{id}: no checksum or signature is invented");
        assert_eq!(e.record["verificationClaims"][0]["state"], "publisher-describes", "{id}");
        assert!(e.limitations.iter().any(|l| l.contains("2026-10-25 tarihli")), "{id}: {:?}", e.limitations);
        assert!(
            e.limitations.iter().any(|l| l.contains("uyumluluk bilinmiyor") || l.contains("vendor") || l.contains("platform/ajan")),
            "{id}: unknown targets are stated"
        );
        assert!(e.limitations.iter().any(|l| l.contains("Kurulum ve işlev doğrulaması yok")), "{id}");
    }
    // the two high privacy-impact entries carry the quarantine statement
    for id in ["thedotmack-claude-mem", "ruvnet-ruflo"] {
        assert!(entry(&conn, id).limitations.iter().any(|l| l.contains("Karantinada")), "{id}");
    }
}

#[test]
fn the_repository_identities_are_the_canonical_ones_and_the_stale_owner_is_never_a_source() {
    let conn = seeded();
    let ka = entry(&conn, "multica-ai-andrej-karpathy-skills");
    assert_eq!(ka.source_key, "multica-ai/andrej-karpathy-skills");
    let structured = [ka.repo_url.clone(), ka.source_key.clone(), ka.record["repository"].to_string(), ka.entry_id.clone()].join(" ");
    assert!(!structured.contains("forrestchang"), "the former owner is not in any identity field: {structured}");
    // the stale references are recorded only as labelled history and warnings
    let text = ka.record.to_string();
    assert!(text.contains("forrestchang") && text.contains("former owner") && text.contains("must not be used"));
    assert!(text.contains("stale") && text.contains("not authored or endorsed by him"));
    assert!(ka.purpose.contains("Karpathy’den esinlenmiş, üçüncü taraf"), "{}", ka.purpose);
    for bad in ["Karpathy tarafından", "Karpathy’nin resmî", "onaylı", "resmi"] {
        assert!(!ka.purpose.contains(bad), "the summary implies authorship or endorsement: {bad}");
    }
    // Ruflo: the packaging ambiguity is recorded, not resolved
    let rf = entry(&conn, "ruvnet-ruflo").record.to_string();
    assert!(rf.contains("claude-flow") && rf.contains("unverified") && rf.contains("not 'all modules installed'"));
    let methods: Vec<String> =
        entry(&conn, "ruvnet-ruflo").record["installMethods"].as_array().unwrap().iter().map(|m| m["methodId"].to_string()).collect();
    assert_eq!(methods, ["\"unknown\""], "no approved install method on an unverified package identity");
}

#[test]
fn the_licence_of_the_karpathy_repository_is_review_required_with_no_invented_proof() {
    let conn = seeded();
    let ka = entry(&conn, "multica-ai-andrej-karpathy-skills");
    assert_eq!((ka.license_state.as_str(), ka.license_spdx.as_deref()), ("review-required", None));
    assert!(ka.record["license"]["spdxId"].is_null() && ka.record["license"]["evidenceId"].is_null());
    let notes = ka.record["privacy"]["notes"].to_string();
    for needle in ["no LICENSE file", "GitHub detects none", "no independently reviewable licence grant", "no SPDX identifier is asserted"]
    {
        assert!(notes.contains(needle), "{needle}");
    }
    // the other three do have a licence file at the pinned commit and say so with evidence
    for id in ["egonex-ai-understand-anything", "thedotmack-claude-mem", "ruvnet-ruflo"] {
        let e = entry(&conn, id);
        assert_eq!(e.record["license"]["state"], "known", "{id}");
        assert!(e.record["license"]["evidenceId"].is_string(), "{id}");
    }
}

// ---- summaries and the quality gate ------------------------------------------------------------------------------------------------------

#[test]
fn each_summary_stands_alone_on_a_card_and_says_what_when_and_what_to_watch() {
    let conn = seeded();
    // (id, what, when/for whom, risk or limitation)
    let expected = [
        ("egonex-ai-understand-anything", "bilgi grafiği", "yeni bir projeye hızlı hâkim olmak", "çok jeton harcayabilir"),
        ("thedotmack-claude-mem", "sonraki oturumlara taşıyan bellek eklentisi", "uzun projelerde bağlam", "gizli bilgileri saklayabilir"),
        ("multica-ai-andrej-karpathy-skills", "dört ilkeli kısa bir yönerge", "önce düşün", "lisansı ayrı bir dosyayla doğrulanamıyor"),
        ("ruvnet-ruflo", "çok modüllü orkestrasyon çatısı", "çok sayıda ajanı koordine etmek isteyenler", "uzak iletişim açabilir"),
    ];
    for (id, what, when, risk) in expected {
        let p = entry(&conn, id).purpose;
        let chars = p.chars().count();
        assert!((100..=300).contains(&chars), "{id}: {chars} characters");
        let sentences = p.matches(". ").count() + usize::from(p.ends_with('.'));
        assert!((2..=3).contains(&sentences), "{id}: {sentences} sentences in {p:?}");
        for needle in [what, when, risk] {
            assert!(p.contains(needle), "{id} lacks {needle:?}: {p}");
        }
        assert!(p.to_lowercase().contains("bu uygulama"), "{id}: says what this application does NOT do");
        for bad in ["http", "npm ", "npx", "```", "curl", "README", "yeşil", "önerilir", "tavsiye"] {
            assert!(!p.contains(bad), "{id} contains {bad:?}: no copied README, command, URL or recommendation");
        }
    }
    for id in ["thedotmack-claude-mem", "ruvnet-ruflo"] {
        assert!(entry(&conn, id).purpose.contains("karantinada"), "{id}: the card says it is quarantined");
    }
}

#[test]
fn three_of_the_four_meet_every_machine_checked_quality_criterion_and_the_fourth_fails_only_the_licence_one() {
    let conn = seeded();
    for id in ["egonex-ai-understand-anything", "thedotmack-claude-mem", "ruvnet-ruflo"] {
        assert!(quality_gaps(&entry(&conn, id)).is_empty(), "{id}: {:?}", quality_gaps(&entry(&conn, id)));
    }
    let gaps = quality_gaps(&entry(&conn, "multica-ai-andrej-karpathy-skills"));
    assert_eq!(gaps.len(), 1, "{gaps:?}");
    assert!(gaps[0].starts_with("Q2"), "the single justified exception is the licence criterion: {gaps:?}");
}

// ---- compatibility: nothing by inference ---------------------------------------------------------------------------------------------------

#[test]
fn compatibility_labels_follow_the_snapshots_and_no_platform_is_accepted_by_inference() {
    let conn = seeded();
    for id in C03 {
        let e = entry(&conn, id);
        let rows = e.record["targetEvidence"].as_array().unwrap();
        assert_eq!(rows.len(), 6, "{id}: 2 platforms x 3 surfaces, each stated");
        let mut cells = std::collections::BTreeSet::new();
        for r in rows {
            cells.insert((r["platformTarget"].as_str().unwrap().to_string(), r["agentSurface"].as_str().unwrap().to_string()));
            assert!(matches!(r["compatibilityState"].as_str(), Some("vendor-declared" | "unknown")), "{id}");
            assert!(r["limitation"].as_str().is_some_and(|l| l.len() > 20), "{id}: every label carries its limitation");
            assert!(
                r.get("testedAt").is_none() && r.get("independentCheckId").is_none() && r.get("evidenceDigest").is_none(),
                "{id}: nothing was tested"
            );
        }
        assert_eq!(cells.len(), 6, "{id}: all six cells are distinct");
        assert_eq!(catalog::compatibility(&e, WIN, "claude-desktop-code"), "unknown", "{id}: a surface the record does not mention");
    }
    // the only declared cell of the four: Claude-Mem x Claude Code x Windows (its README has a Windows setup section for Claude Code hooks)
    let mut declared = Vec::new();
    for id in C03 {
        let e = entry(&conn, id);
        for platform in [WIN, MAC] {
            for surface in [CLI, CC, AG] {
                if catalog::compatibility(&e, platform, surface) == "vendor-declared" {
                    declared.push((id, platform, surface));
                }
            }
        }
    }
    assert_eq!(declared, [("thedotmack-claude-mem", WIN, CC)]);
    // Ruflo's "all platforms" sentence belongs to an install path whose package identity is unverified, so it is NOT taken as a declaration
    for platform in [WIN, MAC] {
        assert_eq!(catalog::compatibility(&entry(&conn, "ruvnet-ruflo"), platform, CC), "unknown");
        assert_eq!(
            catalog::compatibility(&entry(&conn, "egonex-ai-understand-anything"), platform, AG),
            "unknown",
            "Antigravity is named without a surface"
        );
        assert_eq!(
            catalog::compatibility(&entry(&conn, "thedotmack-claude-mem"), platform, AG),
            "unknown",
            "the README documents the CLI, not the IDE"
        );
    }
}

#[test]
fn nothing_becomes_accepted_active_or_installed_by_arriving() {
    let conn = seeded();
    for id in C03 {
        assert_eq!(n(&conn, &format!("SELECT count(*) FROM project_preferences WHERE entry_id = '{id}'")), 0, "{id}: no wish");
        assert_eq!(n(&conn, &format!("SELECT count(*) FROM preference_history WHERE entry_id = '{id}'")), 0, "{id}");
    }
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences"), 6, "only the six demonstration decisions of earlier phases");
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE install_state <> 'not_managed' OR verification_state <> 'unknown'"), 0);
    let alias = "ornek-web-sitesi";
    // quarantined entries cannot be accepted anywhere, even on their one declared cell; unknown cells cannot either
    let blocked = |id: &str, surface: &str, platform: &str| {
        crate::projects::set_preference(&conn, alias, id, surface, platform, "accepted", None).unwrap_err().code
    };
    assert_eq!(blocked("thedotmack-claude-mem", CC, WIN), "ACTIVATION_BLOCKED");
    assert_eq!(blocked("ruvnet-ruflo", CC, WIN), "ACTIVATION_BLOCKED");
    assert_eq!(blocked("egonex-ai-understand-anything", CC, WIN), "PREFERENCE_NOT_ACTIVATABLE");
    assert_eq!(blocked("multica-ai-andrej-karpathy-skills", CC, MAC), "PREFERENCE_NOT_ACTIVATABLE");
    // an honest 'not now' stays possible
    assert!(crate::projects::set_preference(&conn, alias, "egonex-ai-understand-anything", CC, WIN, "postponed", Some(30)).is_ok());
    assert!(crate::projects::set_preference(&conn, alias, "multica-ai-andrej-karpathy-skills", CC, WIN, "rejected", None).is_ok());
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences WHERE actual_install_state <> 'not_managed' OR actual_verification_state <> 'unknown'"), 0);
}

#[test]
fn records_carry_no_executable_authority_secret_command_or_unexpected_url() {
    let conn = seeded();
    for id in C03 {
        let e = entry(&conn, id);
        let text = e.record.to_string();
        for bad in [
            "curl ",
            "wget ",
            "iwr ",
            "iex ",
            "irm ",
            "| bash",
            "| sh",
            "Invoke-Expression",
            "powershell -",
            "npm install -g",
            "npx ruflo",
            "npx claude-mem",
            "/plugin install",
            "/plugin marketplace",
            "API_KEY",
            "Authorization",
            "sk-",
            "ghp_",
        ] {
            assert!(!text.contains(bad), "{id} contains {bad:?}");
        }
        let methods: Vec<&str> = e.record["installMethods"].as_array().unwrap().iter().map(|m| m["methodId"].as_str().unwrap()).collect();
        assert!(methods.iter().all(|m| matches!(*m, "unknown" | "native-plugin-manager")), "{id}: {methods:?}");
        assert!(
            e.record["installMethods"].as_array().unwrap().iter().all(|m| m.get("command").is_none() && m.get("sha256").is_none()),
            "{id}"
        );
        for url in
            text.match_indices("http").map(|(i, _)| text[i..].split(['"', ' ', ')']).next().unwrap().trim_end_matches(['.', ',', ';']))
        {
            let allowed = [
                "https://github.com/Egonex-AI/Understand-Anything",
                "https://github.com/thedotmack/claude-mem",
                "https://github.com/multica-ai/andrej-karpathy-skills",
                "https://github.com/ruvnet/ruflo",
            ];
            assert!(allowed.contains(&url), "{id}: unexpected URL {url}");
        }
    }
    // pipe-to-shell installers are described only as forbidden, never reproduced
    for id in ["egonex-ai-understand-anything", "thedotmack-claude-mem", "ruvnet-ruflo"] {
        let text = entry(&conn, id).record.to_string();
        assert!(text.contains("pipe-to-shell") || text.contains("pipe a remote script into a shell"), "{id}");
    }
    assert!(entry(&conn, "egonex-ai-understand-anything").record.to_string().contains("must never be run or reproduced"));
}

#[test]
fn the_risk_statements_are_present_and_conservative() {
    let conn = seeded();
    let ua = entry(&conn, "egonex-ai-understand-anything").record;
    assert_eq!(ua["privacy"]["mayTransmitProjectContent"], true);
    assert!(ua["privacy"]["notes"].to_string().contains("large number of model tokens"));
    assert!(ua["privacy"]["notes"].to_string().contains("graphify") && ua["privacy"]["notes"].to_string().contains("not interchangeable"));
    let cm = entry(&conn, "thedotmack-claude-mem").record;
    let notes = cm["privacy"]["notes"].to_string();
    for needle in [
        "High privacy impact",
        "Local storage",
        "hosted options are different",
        "none of that may be assumed for other clients",
        "Quarantined",
    ] {
        assert!(notes.contains(needle), "{needle}");
    }
    assert_eq!(cm["privacy"]["mayTransmitProjectContent"], true);
    assert!(cm["dependencies"].to_string().contains("not reconciled"), "Node 20.12 vs 20.0 is disclosed, not resolved");
    let rf = entry(&conn, "ruvnet-ruflo").record;
    assert_eq!(rf["privacy"]["mayTransmitProjectContent"], true);
    assert!(
        rf["privacy"]["notes"].to_string().contains("High runtime privilege")
            && rf["privacy"]["notes"].to_string().contains("47 separate plugin modules")
    );
    assert!(entry(&conn, "multica-ai-andrej-karpathy-skills").record["privacy"]["mayTransmitProjectContent"].is_null());
    for id in C03 {
        assert_eq!(entry(&conn, id).record["privacy"]["telemetry"], "unknown", "{id}: telemetry is not stated in any snapshot");
    }
}

// ---- upgrade 18 -> 22, repeat bootstrap, removed / disabled ------------------------------------------------------------------------------

#[test]
fn upgrading_an_eighteen_entry_database_adds_exactly_the_four_and_keeps_every_choice() {
    let conn = seeded();
    // the state of a database seeded by CATALOG-02: the four CATALOG-03 rows are not there yet
    conn.execute_batch(
        "DELETE FROM catalog_entries WHERE entry_id IN ('egonex-ai-understand-anything','thedotmack-claude-mem','multica-ai-andrej-karpathy-skills','ruvnet-ruflo')",
    )
    .unwrap();
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 18);
    catalog::set_enabled(&conn, "exa-mcp-server", false).unwrap();
    catalog::remove_entry(&conn, "microsoft-playwright-mcp").unwrap();
    crate::projects::add_reference(&conn, "proje", "Proje", "").unwrap();
    crate::projects::set_preference(&conn, "proje", "microsoft-playwright", CLI, WIN, "accepted", None).unwrap();
    let prefs = n(&conn, "SELECT count(*) FROM project_preferences");
    let history = n(&conn, "SELECT count(*) FROM preference_history");
    let audit = n(&conn, "SELECT coalesce(max(sequence), 0) FROM audit_events");
    let older = state(&conn);

    catalog::seed_all(&conn).unwrap();
    catalog::seed_all(&conn).unwrap();

    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 22);
    assert_eq!(n(&conn, "SELECT count(*) FROM project_preferences"), prefs);
    assert_eq!(n(&conn, "SELECT count(*) FROM preference_history"), history);
    assert_eq!(n(&conn, "SELECT coalesce(max(sequence), 0) FROM audit_events"), audit, "seeding writes no audit event");
    let now = state(&conn);
    for old in &older {
        assert_eq!(now.iter().find(|e| e.0 == old.0).unwrap(), old, "{}: unchanged by the upgrade", old.0);
    }
    assert!(!entry(&conn, "exa-mcp-server").enabled && entry(&conn, "microsoft-playwright-mcp").removed_at.is_some());
    for id in C03 {
        assert!(entry(&conn, id).enabled && entry(&conn, id).origin == "seed", "{id}");
    }
    assert_eq!(health(&conn), "passed");
    assert!(crate::audit::verify(&conn).unwrap().ok);
}

#[test]
fn repeat_seeding_is_idempotent_and_never_resurrects_a_removed_or_disabled_candidate() {
    let conn = seeded();
    let first = state(&conn);
    for _ in 0..3 {
        catalog::seed_all(&conn).unwrap();
    }
    assert_eq!(state(&conn), first);
    catalog::set_enabled(&conn, "ruvnet-ruflo", false).unwrap();
    catalog::remove_entry(&conn, "thedotmack-claude-mem").unwrap();
    catalog::seed_all(&conn).unwrap();
    assert!(!entry(&conn, "ruvnet-ruflo").enabled);
    assert!(entry(&conn, "thedotmack-claude-mem").removed_at.is_some());
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 22);
    assert_eq!(health(&conn), "passed");
}

// ---- collisions with entries the user added from a link ----------------------------------------------------------------------------------

fn taken(conn: &Connection, id: &str) -> i64 {
    n(conn, &format!("SELECT count(*) FROM diagnostics WHERE code = 'SEED_SOURCE_TAKEN' AND detail LIKE '%{id}%'"))
}

#[test]
fn links_the_user_added_earlier_win_are_reported_and_never_duplicated() {
    let conn = crate::db::open_memory().unwrap();
    let links = [
        ("https://github.com/Egonex-AI/Understand-Anything", "executable_tool", "kendi notum"),
        ("https://github.com/thedotmack/claude-mem", "agent_plugin", ""),
        ("https://github.com/multica-ai/andrej-karpathy-skills", "skill_collection", "benim amacım"),
        ("https://github.com/ruvnet/ruflo", "agent_plugin", ""),
    ];
    let mut owned = Vec::new();
    for (url, kind, purpose) in links {
        owned.push(catalog::add_entry(&conn, url, kind, purpose).unwrap().entry_id);
    }
    assert_eq!(owned, C03, "the link-derived ids equal the seeds' ids, so the user owns all four ids");
    catalog::seed_all(&conn).unwrap();
    catalog::seed_all(&conn).unwrap();

    // 18 shipped earlier + the 4 user rows; the four colliding seeds were not inserted
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries"), 22);
    for id in C03 {
        let e = entry(&conn, id);
        assert_eq!(
            (e.origin.as_str(), e.metadata_state.as_str(), e.record["userAdded"].clone()),
            ("user", "not-fetched", Value::Bool(true)),
            "{id}"
        );
        assert!(taken(&conn, id) >= 1, "{id}: the collision is reported");
        assert_eq!(
            n(&conn, &format!("SELECT count(*) FROM catalog_entries WHERE source_key = '{}'", e.source_key)),
            1,
            "{id}: no duplicate source"
        );
    }
    assert_eq!(entry(&conn, "egonex-ai-understand-anything").purpose, "kendi notum");
    assert_eq!(entry(&conn, "multica-ai-andrej-karpathy-skills").purpose, "benim amacım");
    assert_eq!(catalog::seed_entries_held_by_the_user(&conn).unwrap(), 4);
    assert_eq!(health(&conn), "passed");
}

#[test]
fn a_removed_user_entry_stays_removed_and_a_stale_owner_link_is_a_different_source_never_merged() {
    let conn = crate::db::open_memory().unwrap();
    let ua = catalog::add_entry(&conn, "https://github.com/Egonex-AI/Understand-Anything", "agent_plugin", "").unwrap();
    catalog::remove_entry(&conn, &ua.entry_id).unwrap();
    // the former owner named by the stale README is a different source key: it is neither blocked nor merged with the canonical repository
    let stale = catalog::add_entry(&conn, "https://github.com/forrestchang/andrej-karpathy-skills", "skill_collection", "").unwrap();
    assert_eq!(stale.entry_id, "forrestchang-andrej-karpathy-skills");
    catalog::seed_all(&conn).unwrap();
    assert!(entry(&conn, "egonex-ai-understand-anything").removed_at.is_some(), "the user's removal is respected");
    assert_eq!(entry(&conn, "egonex-ai-understand-anything").origin, "user");
    assert_eq!(entry(&conn, "multica-ai-andrej-karpathy-skills").origin, "seed", "the canonical repository is seeded as itself");
    assert_eq!(entry(&conn, "forrestchang-andrej-karpathy-skills").origin, "user", "the stale-owner link stays the user's own record");
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE source_key LIKE '%andrej-karpathy-skills'"), 2);
    assert_eq!(health(&conn), "passed");
    // the explanation covers only entries the user owns: a truncated catalog still fails
    let conn = seeded();
    conn.execute_batch("DELETE FROM catalog_entries WHERE entry_id = 'ruvnet-ruflo'").unwrap();
    assert_eq!(catalog::seed_entries_held_by_the_user(&conn).unwrap(), 0);
    assert_eq!(health(&conn), "failed");
}

// ---- no side effects --------------------------------------------------------------------------------------------------------------------

#[test]
fn seeding_has_no_side_effect_beyond_catalog_rows_and_the_loader_is_pure_data() {
    let conn = crate::db::open_memory().unwrap();
    let before = (
        n(&conn, "SELECT count(*) FROM audit_events"),
        n(&conn, "SELECT count(*) FROM discovery_scans"),
        n(&conn, "SELECT count(*) FROM discovery_observations"),
    );
    catalog::seed_all(&conn).unwrap();
    let after = (
        n(&conn, "SELECT count(*) FROM audit_events"),
        n(&conn, "SELECT count(*) FROM discovery_scans"),
        n(&conn, "SELECT count(*) FROM discovery_observations"),
    );
    assert_eq!(before, after, "no audit event, scan or observation is written by seeding");
    assert_eq!(n(&conn, "SELECT count(*) FROM catalog_entries WHERE entry_id IN ('egonex-ai-understand-anything','thedotmack-claude-mem','multica-ai-andrej-karpathy-skills','ruvnet-ruflo') AND execution_policy NOT IN ('manual-only','blocked-until-scoped-authorization')"), 0);
    let production = include_str!("catalog.rs").split("#[cfg(test)]").next().unwrap();
    assert!(production.contains("include_str!(\"../../data/catalog-candidates-c03.json\")"));
    for forbidden in ["std::process", "Command::", "std::net", "reqwest", "TcpStream", "std::fs", "File::open", "std::env"] {
        assert!(!production.contains(forbidden), "catalog.rs production code uses {forbidden}");
    }
}
