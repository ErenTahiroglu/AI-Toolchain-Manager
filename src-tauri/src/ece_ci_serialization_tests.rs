//! ATM-GOV-02 P2A: adding the ECE dependency must not change ANY existing ATM JSON contract.
//!
//! The expected values below were produced by the BASELINE build (dd37b04, before ECE was a dependency) and are written out as
//! literals. They fail if a dependency, or a feature a dependency switches on, changes how ATM's own JSON text is produced.
//! The case that motivated them: ECE-LIB-01 switched `serde_json/preserve_order` on for every consumer; ATM's startup
//! `catalog::seed_all` compares stored `record_json` TEXT with freshly serialized text, so all 22 seed rows would have been
//! rewritten at the first start after an upgrade. ECE-LIB-02 moved that behind a feature that `default-features = false` leaves off.

use crate::{audit, catalog, db, discovery};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// (data file, toolId, SHA-256 of `record.to_string()`), computed on the baseline build.
const SEED_RECORD_TEXT_SHA256: [(&str, &str, &str); 22] = [
    ("data/catalog-seed.json", "ponytail", "3b2b88196bde2b9795f4635a42e2b69841ae67d1c325d288d5672f428abbd8aa"),
    ("data/catalog-seed.json", "codebase-memory-mcp", "c862d3643793daf2e50aeeac224c6ec2ecac8687fd33ece1d0095752cc5cc1dd"),
    ("data/catalog-seed.json", "graphify", "c496966282d25e119e637b1ababef71c383b7302f1fd56f4376dbb9401aefe09"),
    ("data/catalog-seed.json", "gitleaks", "b100124c828d9aa7b17a2292ce7304007dbea66cd9b388bcc0fd7f6f8927269d"),
    ("data/catalog-seed.json", "markitdown", "f783412396a9e81be9217757912d9d1b308ce87c4965724ca4e292c5262909b5"),
    ("data/catalog-seed.json", "caveman", "ef014ea31c5ffe74ce63a664f8437f35faecd9c9c8025e8f9c38c0d259b6b582"),
    ("data/catalog-seed.json", "cloudflare-security-audit-skill", "a9aeb0ba65bc6413c8b35780da30f4b6c10e0915f06f0c3509a4477fdd1eb185"),
    ("data/catalog-seed.json", "superpowers", "83ce2be96e33fa763d8c2dea2c38061eedb58cf8fc5b3d3149640b7cda1b7475"),
    ("data/catalog-seed.json", "betterleaks", "e96783b8d5ce04685790e00326cae6c67b37746a26bf7b66b085a67922413db1"),
    ("data/catalog-candidates-1b.json", "anthropic-frontend-design", "5d4471f5689b42a2d220aa8d724e400d8543dbea15e568ef9321d115efe747e3"),
    ("data/catalog-candidates-1b.json", "strix", "69c8bb2ce017ed49fba0907c0dd4d095e998a23d6286ce282a1358836733ac89"),
    ("data/catalog-candidates-1b.json", "agent-browser", "f9708664b95bc04c6ccd06818acdeda7a5615c61936a492f297681dd145b56ab"),
    ("data/catalog-candidates-1b.json", "agentic-awesome-skills", "6c82e69b06382ffa2dd4b11352c2ccda6524eb0529f1061fe9fb26d0b6dc4b05"),
    ("data/catalog-candidates-c01.json", "microsoft-playwright", "2a6b756864cf651dc9deb4b3564d700729239fafc626dfbf8774af31dc363e18"),
    ("data/catalog-candidates-c01.json", "apify-mcp-server", "037661bcd517d69f3522dfbe7f4b097708aceac5dc19861a7f893c7cfecb0143"),
    ("data/catalog-candidates-c01.json", "exa-mcp-server", "6caf5e07dd328aee41b07a50e869accfc4d835f628085edbec68cce64ff665bc"),
    ("data/catalog-candidates-c02.json", "microsoft-playwright-cli", "67db19d81a48facec01b3ba48e2d28194a01e0f718fdb69152df6fc64282cc39"),
    ("data/catalog-candidates-c02.json", "microsoft-playwright-mcp", "c6e8489006af429908781200a282d18d3055c3adebb50e85449b46571346388f"),
    (
        "data/catalog-candidates-c03.json",
        "egonex-ai-understand-anything",
        "092324ec30e05ebffeb9b6b64a25a9930ba232dad9af13f807d5d3f06f5996ed",
    ),
    ("data/catalog-candidates-c03.json", "thedotmack-claude-mem", "43d7b6332d262782d0d1e7e00a56fad96bf4c4535ebe14d1fcbc63129a2edab3"),
    (
        "data/catalog-candidates-c03.json",
        "multica-ai-andrej-karpathy-skills",
        "4884afce4f8285fde39e7ee097e44c69eb44b87a2b339e3bcd5ed9a1a0fa5510",
    ),
    ("data/catalog-candidates-c03.json", "ruvnet-ruflo", "0275661dad91844e95c97492b7c3fd9b666556bc88e416c95c00af2d2e146f98"),
];

const SEED_FILES: [(&str, &str); 5] = [
    ("data/catalog-seed.json", include_str!("../../data/catalog-seed.json")),
    ("data/catalog-candidates-1b.json", include_str!("../../data/catalog-candidates-1b.json")),
    ("data/catalog-candidates-c01.json", include_str!("../../data/catalog-candidates-c01.json")),
    ("data/catalog-candidates-c02.json", include_str!("../../data/catalog-candidates-c02.json")),
    ("data/catalog-candidates-c03.json", include_str!("../../data/catalog-candidates-c03.json")),
];

#[test]
fn serde_json_objects_still_serialize_with_sorted_keys() {
    // The direct detector for `preserve_order` (and for `indexmap` replacing the map) being switched on by any dependency.
    assert_eq!(json!({"zeta": 1, "alpha": {"y": 1, "x": 2}}).to_string(), r#"{"alpha":{"x":2,"y":1},"zeta":1}"#);
    let parsed: Value = serde_json::from_str(r#"{"b":1,"a":{"d":1,"c":2}}"#).unwrap();
    assert_eq!(parsed.to_string(), r#"{"a":{"c":2,"d":1},"b":1}"#);
    let mut map = Map::new();
    map.insert("b".into(), json!(1));
    map.insert("a".into(), json!(2));
    assert_eq!(Value::Object(map).to_string(), r#"{"a":2,"b":1}"#);
}

#[test]
fn all_22_seed_records_serialize_byte_identically_to_the_baseline() {
    let mut seen = 0;
    for (file, raw) in SEED_FILES {
        let root: Value = serde_json::from_str(raw).unwrap();
        for wrapper in root["records"].as_array().unwrap() {
            let record = if wrapper.get("record").is_some() { &wrapper["record"] } else { wrapper };
            let id = record["toolId"].as_str().unwrap();
            let expected = SEED_RECORD_TEXT_SHA256
                .iter()
                .find(|(f, i, _)| *f == file && *i == id)
                .unwrap_or_else(|| panic!("no baseline for {file} {id}"));
            assert_eq!(sha256_hex(&record.to_string()), expected.2, "{file} {id}");
            seen += 1;
        }
    }
    assert_eq!(seen, 22);
}

fn seeded() -> Connection {
    let conn = db::open_memory().unwrap();
    catalog::seed_all(&conn).unwrap();
    conn
}

fn seed_rows(conn: &Connection) -> Vec<(String, String, String)> {
    let mut stmt = conn
        .prepare("SELECT entry_id, record_json, capability_fingerprint FROM catalog_entries WHERE origin = 'seed' ORDER BY entry_id")
        .unwrap();
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect();
    rows
}

#[test]
fn the_real_seeding_stores_exactly_the_baseline_text_for_every_seed_row() {
    let conn = seeded();
    let stored: HashMap<String, String> = seed_rows(&conn).into_iter().map(|(id, text, _)| (id, text)).collect();
    for (file, id, expected) in SEED_RECORD_TEXT_SHA256 {
        let text = stored.get(id).unwrap_or_else(|| panic!("seed row {id} ({file}) missing"));
        assert_eq!(sha256_hex(text), expected, "{id}");
    }
}

#[test]
fn restarting_does_not_rewrite_a_single_seed_row() {
    let conn = seeded();
    conn.execute("UPDATE catalog_entries SET updated_at = 'SENTINEL' WHERE origin = 'seed'", []).unwrap();
    let before = seed_rows(&conn);
    assert!(before.len() >= 22);
    // This is the startup path: it must be a no-op on an already seeded database. It guards idempotence WITHIN one build; the
    // CROSS-version hazard (a database written by the baseline build, read by a build with a different key order) is caught by the
    // pinned baseline hashes in the two tests above, which fail when `preserve_order` is on.
    catalog::seed_all(&conn).unwrap();
    let rewritten: i64 = conn
        .query_row("SELECT count(*) FROM catalog_entries WHERE origin = 'seed' AND updated_at <> 'SENTINEL'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rewritten, 0, "seed rows were rewritten at startup");
    assert_eq!(before, seed_rows(&conn));
}

#[test]
fn a_user_added_catalog_row_is_stored_with_the_baseline_text() {
    let conn = db::open_memory().unwrap();
    let entry = catalog::add_entry(&conn, "https://github.com/o-owner/r-repo", "executable_tool", "A test purpose.").unwrap();
    let stored: String =
        conn.query_row("SELECT record_json FROM catalog_entries WHERE entry_id = ?1", [&entry.entry_id], |r| r.get(0)).unwrap();
    assert_eq!(
        stored,
        concat!(
            r#"{"installMethods":[],"license":{"state":"unknown"},"note":"Yalnızca bağlantıdan türetildi; uzak veri çekilmedi.","#,
            r#""privacy":{"mayTransmitProjectContent":null,"telemetry":"unknown"},"#,
            r#""repository":{"name":"r-repo","owner":"o-owner","url":"https://github.com/o-owner/r-repo"},"#,
            r#""schemaVersion":0,"targetEvidence":[],"userAdded":true}"#
        )
    );
}

#[test]
fn legacy_discovery_provenance_is_rewritten_exactly_as_before() {
    let conn = db::open_memory().unwrap();
    conn.execute(
        "INSERT INTO discovery_scans (scan_id, started_at, finished_at, platform_target, authorized_path, authorized_user_config,
            authorized_project, status, observation_count) VALUES ('s1', 't', 't', 'windows-x64', 0, 0, 0, 'completed', 2)",
        [],
    )
    .unwrap();
    let insert = |subject: &str, provenance: &str| {
        conn.execute(
            "INSERT INTO discovery_observations (scan_id, subject, platform_target, agent_surface, method, presence, configuration, scope,
                confidence, evidence, limitations_json, identity_verified, observed_at, identity_state, provenance_json)
             VALUES ('s1', ?1, 'windows-x64', 'local-cli', 'path-executable', 'present', 'unknown', 'global', 'low', 'e', '[]', 0, 't',
                'validated', ?2)",
            rusqlite::params![subject, provenance],
        )
        .unwrap();
    };
    insert("legacy-a", r#"{"versionTrust":"trusted","assessed":true,"identity":"validated","z":{"b":1,"a":[1,2.5,3e2]}}"#);
    insert("legacy-b", "{}");
    discovery::store::reclassify_legacy_trust(&conn).unwrap();
    let text = |subject: &str| -> String {
        conn.query_row("SELECT provenance_json FROM discovery_observations WHERE subject = ?1", [subject], |r| r.get(0)).unwrap()
    };
    assert_eq!(
        text("legacy-a"),
        r#"{"assessed":true,"identity":"evidence_available","reclassified":true,"versionTrust":"installer_recorded","z":{"a":[1,2.5,300.0],"b":1}}"#
    );
    assert_eq!(text("legacy-b"), r#"{"reclassified":true}"#);
}

#[test]
fn audit_records_are_canonical_sorted_and_the_chain_verifies() {
    let mut record = Map::new();
    record.insert("sequence".into(), json!(9_007_199_254_740_993_u64));
    record.insert("schemaVersion".into(), json!(2));
    record.insert("note".into(), json!("ç\"x"));
    record.insert("previousHash".into(), Value::Null);
    record.insert("segmentStart".into(), json!(false));
    assert_eq!(
        audit::canonical_json(&Value::Object(record)),
        r#"{"note":"ç\"x","previousHash":null,"schemaVersion":2,"segmentStart":false,"sequence":9007199254740993}"#
    );
    let conn = db::open_memory().unwrap();
    for check in ["a", "b", "c"] {
        audit::append(&conn, &audit::NewEvent::plain("health_check", "passed", check, None, "operational-health", "manager")).unwrap();
    }
    let report = audit::verify(&conn).unwrap();
    assert!(report.ok && report.count == 3, "{report:?}");
    // The persisted text is itself the canonical (sorted) form of the record it holds.
    let mut stmt = conn.prepare("SELECT record_json FROM audit_events ORDER BY id").unwrap();
    for row in stmt.query_map([], |r| r.get::<_, String>(0)).unwrap() {
        let text = row.unwrap();
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(text, audit::canonical_json(&parsed));
    }
}

#[test]
fn numbers_atm_can_meet_round_trip_unchanged_with_float_roundtrip_on() {
    // `float_roundtrip` (switched on for the whole graph by ECE's RFC 8785 library) changes only how a float literal with 17 or more
    // significant digits at a rounding boundary is parsed. ATM never reads such a value from JSON; these are the values it can meet.
    let cases = [
        ("0", "0"),
        ("42", "42"),
        ("-1", "-1"),
        ("9007199254740993", "9007199254740993"),
        ("18446744073709551615", "18446744073709551615"),
        ("-9223372036854775808", "-9223372036854775808"),
        ("0.1", "0.1"),
        ("2.5", "2.5"),
        ("5.0", "5.0"),
        ("1000.0", "1000.0"),
        ("1e3", "1000.0"),
        ("0.000001", "1e-6"),
    ];
    for (input, output) in cases {
        let v: Value = serde_json::from_str(input).unwrap();
        assert_eq!(v.to_string(), output, "{input}");
    }
    let nested: Value = serde_json::from_str(r#"{"b":[1,2.5,{"c":0.1,"d":[1e3,12345678901234567890]}],"a":{"x":3.0,"y":-0.0}}"#).unwrap();
    assert_eq!(nested.to_string(), r#"{"a":{"x":3.0,"y":-0.0},"b":[1,2.5,{"c":0.1,"d":[1000.0,12345678901234567890]}]}"#);
}
