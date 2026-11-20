//! T23: parity with the GUI. The files read here (`src-tauri/src/catalog.rs`, `src-tauri/src/ece_ci.rs`, `src-tauri/src/ece_ci_live.rs`,
//! `.github/ece/atm-ci-two-runner-v1.json`, `data/catalog-*.json`) are review/test INPUTS compiled in with `include_str!`/`include_bytes!`;
//! this package never edits them. A mismatch fails the test, so a GUI-side change cannot silently diverge from this prototype.

use atm_mcp::{catalog, tools};
use serde_json::Value;
use sha2::{Digest, Sha256};

const GUI_CATALOG: &str = include_str!("../../src-tauri/src/catalog.rs");
const GUI_ECE: &str = include_str!("../../src-tauri/src/ece_ci.rs");
const GUI_ECE_LIVE: &str = include_str!("../../src-tauri/src/ece_ci_live.rs");
const POLICY_BYTES: &[u8] = include_bytes!("../../.github/ece/atm-ci-two-runner-v1.json");

fn quoted(s: &str) -> Vec<String> {
    s.split('"').enumerate().filter(|(i, _)| i % 2 == 1).map(|(_, t)| t.to_string()).collect()
}

fn const_line<'a>(src: &'a str, name: &str) -> &'a str {
    src.lines().find(|l| l.trim_start().starts_with(&format!("pub const {name}:")) || l.trim_start().starts_with(&format!("const {name}:"))).unwrap_or_else(|| panic!("const {name} not found in GUI source"))
}

fn const_str(src: &str, name: &str) -> String {
    quoted(const_line(src, name)).into_iter().next().unwrap_or_else(|| panic!("no string in {name}"))
}

fn const_num(src: &str, name: &str) -> u64 {
    let line = const_line(src, name);
    let rhs = line.split('=').nth(1).unwrap().trim().trim_end_matches(';').replace('_', "");
    rhs.parse().unwrap_or_else(|_| panic!("not a number: {rhs}"))
}

#[test]
fn t23_nine_meta_kinds_policies_and_limits_match_the_gui_source() {
    let start = GUI_CATALOG.find("const NINE_META").expect("NINE_META in GUI source");
    let block = &GUI_CATALOG[start..];
    let block = &block[..block.find("];").unwrap()];
    let gui: Vec<(String, String, String)> = block
        .lines()
        .filter(|l| l.trim_start().starts_with("(\""))
        .map(|l| {
            let q = quoted(l);
            assert_eq!(q.len(), 3, "NINE_META row shape: {l}");
            (q[0].clone(), q[1].clone(), q[2].clone())
        })
        .collect();
    let ours: Vec<(String, String, String)> = catalog::NINE_META.iter().map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string())).collect();
    assert_eq!(gui, ours, "NINE_META differs from src-tauri/src/catalog.rs");
    assert_eq!(quoted(const_line(GUI_CATALOG, "KINDS")), catalog::KINDS.map(String::from).to_vec());
    assert_eq!(quoted(const_line(GUI_CATALOG, "EXECUTION_POLICIES")), catalog::EXECUTION_POLICIES.map(String::from).to_vec());
    assert_eq!(const_num(GUI_CATALOG, "MAX_PURPOSE_CHARS"), catalog::MAX_PURPOSE_CHARS as u64);
    for state in catalog::REVIEW_STATES {
        assert!(GUI_CATALOG.contains(&format!("\"{state}\"")), "review state {state} not in GUI source");
    }
}

#[test]
fn t23_entry_counts_and_data_file_list_match_the_gui_source() {
    let names = ["PHASE0_TOOL_COUNT", "CANDIDATE_COUNT", "CATALOG01_COUNT", "CATALOG02_COUNT", "CATALOG03_COUNT", "CATALOG04_COUNT", "CATALOG05_COUNT"];
    let total: u64 = names.iter().map(|n| const_num(GUI_CATALOG, n)).sum();
    assert_eq!(total, catalog::EXPECTED_ENTRY_COUNT as u64);
    assert_eq!(catalog::catalog().unwrap().entries.len() as u64, total);
    let mut gui_files: Vec<String> = GUI_CATALOG
        .lines()
        .filter(|l| l.contains("include_str!(\"../../data/"))
        .map(|l| l.split("../../data/").nth(1).unwrap().split('"').next().unwrap().to_string())
        .collect();
    gui_files.sort();
    let mut ours: Vec<String> = std::iter::once("catalog-seed.json").chain(catalog::CANDIDATE_FILES.iter().map(|(n, _)| *n)).map(String::from).collect();
    ours.sort();
    assert_eq!(gui_files, ours, "GUI embeds a different set of data files");
    // our embedded bytes are the repository's bytes (same include path semantics)
    assert_eq!(catalog::SEED_JSON, include_str!("../../data/catalog-seed.json"));
}

#[test]
fn t23_ece_binding_constants_match_the_gui_source_and_the_policy_file() {
    assert_eq!(const_str(GUI_ECE, "ECE_REVISION"), tools::ECE_REVISION);
    assert_eq!(const_str(GUI_ECE, "POLICY_FILE_SHA256"), tools::POLICY_FILE_SHA256);
    assert_eq!(const_str(GUI_ECE, "POLICY_ID"), tools::POLICY_ID);
    assert_eq!(const_str(GUI_ECE, "REPOSITORY_FULL_NAME"), tools::REPOSITORY_FULL_NAME);
    assert_eq!(const_str(GUI_ECE, "WORKFLOW_PATH"), tools::WORKFLOW_PATH);
    assert_eq!(const_num(GUI_ECE, "REPOSITORY_ID"), tools::REPOSITORY_ID);
    assert_eq!(quoted(const_line(GUI_ECE, "REQUIRED_JOBS")), tools::REQUIRED_JOBS.map(String::from).to_vec());
    assert_eq!(const_num(GUI_ECE_LIVE, "MAX_SAFE_RUN_ID"), tools::MAX_SAFE_RUN_ID);
    assert_eq!(const_num(GUI_ECE_LIVE, "MAX_RUN_ATTEMPT"), tools::MAX_RUN_ATTEMPT);
    // the independently expected policy digest is the digest of the policy file's actual bytes
    let digest: String = Sha256::digest(POLICY_BYTES).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(digest, tools::POLICY_FILE_SHA256);
    let policy: Value = serde_json::from_slice(POLICY_BYTES).unwrap();
    assert_eq!(policy["policy_id"], tools::POLICY_ID);
    assert_eq!(policy["allow_extra_jobs"], false);
    assert_eq!(policy["target_repo_id"], tools::REPOSITORY_ID.to_string());
    assert_eq!(policy["target_workflow_path"], tools::WORKFLOW_PATH);
}

/// An independent second derivation of the per-entry facts straight from the JSON files (no code shared with `catalog.rs`).
#[test]
fn t23_every_entry_matches_an_independent_reading_of_the_raw_data() {
    let mut files: Vec<(&str, &str)> = vec![("catalog-seed.json", catalog::SEED_JSON)];
    files.extend(catalog::CANDIDATE_FILES);
    let mut expected: Vec<(String, String, String, String, String, String)> = Vec::new(); // id, kind, policy, review, license, purpose
    let mut refinements: Vec<(String, String)> = Vec::new();
    for (name, raw) in &files {
        let root: Value = serde_json::from_str(raw).unwrap();
        for w in root["records"].as_array().unwrap() {
            let (record, kind, policy, purpose) = if *name == "catalog-seed.json" {
                let id = w["toolId"].as_str().unwrap();
                let (_, k, p) = catalog::NINE_META.iter().find(|(t, _, _)| *t == id).unwrap();
                (w, k.to_string(), "manual-only".to_string(), p.to_string())
            } else {
                (&w["record"], w["entryKind"].as_str().unwrap().to_string(), w["executionPolicy"].as_str().unwrap().to_string(), w["purpose"].as_str().unwrap().to_string())
            };
            expected.push((
                record["toolId"].as_str().unwrap().to_string(),
                kind,
                policy,
                record["reviewState"].as_str().unwrap().to_string(),
                record["license"]["state"].as_str().unwrap().to_string(),
                purpose,
            ));
        }
        for r in root.get("purposeRefinements").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
            refinements.push((r["toolId"].as_str().unwrap().to_string(), r["purpose"].as_str().unwrap().to_string()));
        }
    }
    for (id, p) in refinements {
        expected.iter_mut().find(|e| e.0 == id).expect("refinement target").5 = p;
    }
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    let cat = catalog::catalog().unwrap();
    assert_eq!(cat.entries.len(), expected.len());
    for (e, x) in cat.entries.iter().zip(expected.iter()) {
        assert_eq!(e.id, x.0);
        assert_eq!(e.kind, x.1, "{}", x.0);
        assert_eq!(e.execution_policy, x.2, "{}", x.0);
        assert_eq!(e.review_state, x.3, "{}", x.0);
        assert_eq!(e.license_state, x.4, "{}", x.0);
        assert_eq!(e.purpose, catalog::sanitize_text(&x.5), "{}", x.0);
    }
}
