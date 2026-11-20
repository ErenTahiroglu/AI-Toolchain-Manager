//! T19, T20, T21, T22 on the BUILT process. These are author fixtures. They observe stdout, stderr, exit and the probe directories only:
//! they do NOT observe sockets, child processes or registry access at the operating-system level (see README, "Observation limits").

mod common;
use common::*;
use serde_json::{json, Value};

const SECRET: &str = "ATMSECRET-9f3c1d7a-do-not-leak";

// ---- T19 secret and path redaction ----------------------------------------------------------------------------------------------------
#[test]
fn t19_environment_secret_and_working_paths_never_appear_in_output() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[("ATM_FAKE_TOKEN", SECRET), ("GITHUB_TOKEN", SECRET), ("ANTHROPIC_API_KEY", SECRET)]);
    let mut id = 0;
    let mut calls = vec![("atm_runtime_status_v1", json!({})), ("atm_catalog_search_v1", json!({"query": "kod"})), ("atm_catalog_get_v1", json!({"id": "gitleaks"})), ("atm_catalog_get_v1", json!({"id": "nope"})), ("atm_checkpoint_summary_v1", json!({"commitSha": "0123456789abcdef0123456789abcdef01234567", "runId": 3, "runAttempt": 1}))];
    calls.push(("atm_catalog_search_v1", json!({"query": "../../secret"})));
    for (name, args) in calls {
        id += 1;
        let _ = s.call(modern_call(id, name, args), id);
    }
    let f = s.finish();
    let all = format!("{}\n{}", f.stdout_lines.join("\n"), f.stderr);
    assert!(!all.contains(SECRET), "secret leaked");
    assert!(!all.contains(&p.name()), "unique working-directory name leaked");
    let user = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
    if user.len() > 3 {
        assert!(!all.to_lowercase().contains(&user.to_lowercase()), "user name leaked");
    }
    for needle in ["C:\\", "C:/", "\\\\", "/home/", "/Users/", "AppData"] {
        assert!(!all.contains(needle), "path fragment {needle:?} leaked");
    }
}

// ---- T20 stdout purity ---------------------------------------------------------------------------------------------------------------
#[test]
fn t20_stdout_is_only_json_rpc_and_stderr_is_quiet() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    s.send("garbage that is not json");
    s.send(&"z".repeat(30_000)); // oversized, dropped
    let _ = s.call(discover(1), 1);
    let _ = s.call(modern(2, "tools/list", json!({})), 2);
    let _ = s.call(modern_call(3, "atm_catalog_search_v1", json!({"query": "kod"})), 3);
    let f = s.finish();
    assert!(!f.stdout_lines.is_empty());
    for line in &f.stdout_lines {
        let v: Value = serde_json::from_str(line).expect("stdout line is JSON");
        assert_eq!(v["jsonrpc"], "2.0");
        assert!(v.get("id").is_some() || v.get("method").is_some(), "neither response nor notification: {line}");
        assert!(!line.contains('\n') && !line.contains('\r'));
    }
    // fixed texts only, never caller data
    assert!(!f.stderr.contains("garbage") && !f.stderr.contains("zzzz"), "caller data on stderr: {:?}", f.stderr);
}

// ---- T21 EOF, repetition, cancellation ----------------------------------------------------------------------------------------------
#[test]
fn t21a_exits_promptly_when_stdin_reaches_eof() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    assert!(healthy(&mut s, 1));
    let f = s.finish();
    assert!(!f.timed_out, "did not exit after EOF");
    assert!(f.success, "non-zero exit after EOF");
    assert!(f.elapsed.as_millis() < 3000, "slow exit: {:?}", f.elapsed);
}

/// T21b-1: an UNFINISHED frame (no newline) at EOF. The harness really withholds the newline (`send_raw_no_newline`) and then closes stdin.
/// Contract (guard.rs `read_frame` returns end-of-stream with partial bytes dropped; GUARD_AUDIT.md): no reply for it, exit 0, no hang.
#[test]
fn t21b1_unfinished_frame_at_eof_is_dropped_and_the_process_exits() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    assert!(healthy(&mut s, 1));
    s.send_raw_no_newline(br#"{"jsonrpc":"2.0","id":2,"method":"server/disc"#);
    let f = s.finish();
    assert!(!f.timed_out, "hang on an unfinished frame at EOF");
    assert!(f.success);
    assert!(f.leftover.iter().all(|m| m["id"] != json!(2)), "an unfinished frame was answered");
    assert_eq!(f.stdout_lines.len(), 1, "only the first probe was answered: {:?}", f.stdout_lines);
}

/// T21b-2: a COMPLETE, valid request that merely lacks its terminating newline at EOF is also an unfinished frame: dropped, no reply.
#[test]
fn t21b2_valid_request_without_newline_at_eof_is_not_answered() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    s.send_raw_no_newline(discover(7).as_bytes());
    let f = s.finish();
    assert!(!f.timed_out && f.success);
    assert!(f.stdout_lines.is_empty(), "answered a frame that never ended: {:?}", f.stdout_lines);
}

/// T21b-3 (the earlier line-based variant, kept separately): a COMPLETE but malformed line at EOF is dropped, exit 0.
#[test]
fn t21b3_complete_malformed_line_at_eof_still_exits() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    s.send_bytes(br#"{"jsonrpc":"2.0","id":1,"method":"server/disc"#); // send_bytes adds the newline: this is a finished, broken frame
    let f = s.finish();
    assert!(!f.timed_out && f.success);
}

#[test]
fn t21c_repeated_requests_are_all_answered_identically() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let mut first: Option<Value> = None;
    for i in 1..=60 {
        let r = s.call(modern_call(i, "atm_runtime_status_v1", json!({})), i);
        let v = structured(&r).clone();
        match &first {
            None => first = Some(v),
            Some(f) => assert_eq!(f, &v),
        }
    }
    assert!(s.finish().success);
}

#[test]
fn t21d_cancellation_notification_is_harmless() {
    // Handlers finish instantly, so a real in-flight cancel cannot be staged from outside; this records that cancelling an answered and an
    // unknown id neither crashes nor produces a second message for the id (weak evidence, stated as such in the README).
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let r = s.call(modern_call(1, "atm_runtime_status_v1", json!({})), 1);
    assert!(r.get("result").is_some());
    for rid in [1, 999] {
        s.send(&json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": rid, "_meta": meta("2026-07-28")}}).to_string());
    }
    assert!(healthy(&mut s, 2));
    let f = s.finish();
    assert!(f.success);
    let for_id_1 = f.stdout_lines.iter().filter(|l| serde_json::from_str::<Value>(l).map(|v| v["id"] == json!(1)).unwrap_or(false)).count();
    assert_eq!(for_id_1, 1, "more than one message for a cancelled id");
}

#[test]
fn t21e_burst_of_frames_in_one_write_is_answered_in_full() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let burst: Vec<String> = (1..=25).map(|i| modern_call(i, "atm_catalog_get_v1", json!({"id": "gitleaks"}))).collect();
    s.send(&burst.join("\n"));
    for i in 1..=25 {
        let r = s.recv_id(&json!(i), 15_000).unwrap_or_else(|| panic!("missing answer {i}"));
        assert_eq!(structured(&r)["id"], "gitleaks");
    }
    assert!(s.finish().success);
}

// ---- T22 no filesystem writes by the running server ---------------------------------------------------------------------------------
#[test]
fn t22_server_writes_nothing_under_home_appdata_data_dir_temp_or_cwd() {
    let p = Probe::new();
    assert!(p.listing().is_empty());
    let mut s = Session::start(&p, &[]);
    let _ = s.call(discover(1), 1);
    for (i, (name, args)) in [("atm_runtime_status_v1", json!({})), ("atm_catalog_search_v1", json!({"query": "tool"})), ("atm_catalog_get_v1", json!({"id": "graphify"})), ("atm_checkpoint_summary_v1", json!({"commitSha": "0123456789abcdef0123456789abcdef01234567", "runId": 9, "runAttempt": 2}))].into_iter().enumerate() {
        let _ = s.call(modern_call(10 + i as i64, name, args), 10 + i as i64);
    }
    s.send("{broken");
    let f = s.finish();
    assert!(f.success);
    assert_eq!(p.listing(), Vec::<String>::new(), "the server created files or directories");
}

#[test]
fn t22b_no_database_or_data_directory_is_created_even_when_atm_data_dir_points_at_a_missing_path() {
    let p = Probe::new();
    let missing = p.root.join("does-not-exist-yet");
    let mut s = Session::start(&p, &[("ATM_DATA_DIR", missing.to_str().unwrap())]);
    assert!(healthy(&mut s, 1));
    let f = s.finish();
    assert!(f.success);
    assert!(!missing.exists(), "ATM_DATA_DIR was created or opened");
}

// ---- Windows quoting / encoding over the real pipe --------------------------------------------------------------------------------
#[test]
fn utf8_turkish_query_travels_over_the_pipe_intact_and_is_not_echoed() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let r = s.call(modern_call(1, "atm_catalog_search_v1", json!({"query": "Güvenlik"})), 1);
    let ids: Vec<&str> = structured(&r)["items"].as_array().unwrap().iter().filter_map(|i| i["id"].as_str()).collect();
    assert!(ids.contains(&"cloudflare-security-audit-skill"), "{ids:?}");
    // escaped form of the same text (\u00fc) behaves identically
    let escaped = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"atm_catalog_search_v1","arguments":{"query":"G\u00fcvenlik"},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{},"io.modelcontextprotocol/clientInfo":{"name":"t","version":"0"}}}}"#;
    let r2 = s.call(escaped.to_string(), 2);
    assert_eq!(structured(&r)["items"], structured(&r2)["items"]);
    // a hostile query is refused and not echoed
    let bad = s.call(modern_call(3, "atm_catalog_search_v1", json!({"query": "a\u{202E}EVILTOKEN"})), 3);
    assert_eq!(structured(&bad)["error"]["code"], "ATM_INPUT_INVALID");
    assert!(!bad.to_string().contains("EVILTOKEN"));
    // BOM and CRLF framing are tolerated
    s.send_bytes(b"\xEF\xBB\xBF{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"server/discover\",\"params\":{\"_meta\":{\"io.modelcontextprotocol/protocolVersion\":\"2026-07-28\",\"io.modelcontextprotocol/clientCapabilities\":{},\"io.modelcontextprotocol/clientInfo\":{\"name\":\"t\",\"version\":\"0\"}}}}\r");
    assert!(s.recv_id(&json!(4), 10_000).map(|m| m.get("result").is_some()).unwrap_or(false));
    assert!(s.finish().success);
}
