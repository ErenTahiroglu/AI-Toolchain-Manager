//! T01–T12, T14, T16, T17 and the two Phase 0 review caveats, at the real wire: the BUILT process, raw JSON-RPC lines.
//! Where the SDK's exact behavior is not known from source reading, the assertion states the observable contract and the comment says so.

mod common;
use common::*;
use serde_json::{json, Value};

fn ok_probe(s: &mut Session, id: i64) {
    assert!(healthy(s, id), "process no longer answers a modern discover probe");
}

// ---- T01 modern discover ------------------------------------------------------------------------------------------------------------
#[test]
fn t01_modern_discover() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let r = s.call(discover(1), 1);
    let result = r.get("result").expect("discover result");
    let versions: Vec<&str> = result["supportedVersions"].as_array().expect("supportedVersions").iter().filter_map(Value::as_str).collect();
    let mut sorted = versions.clone();
    sorted.sort();
    let mut expected = SUPPORTED.to_vec();
    expected.sort();
    assert_eq!(sorted, expected, "exactly the three supported versions");
    assert!(result["capabilities"].get("tools").is_some());
    assert_eq!(result.pointer("/_meta/io.modelcontextprotocol~1serverInfo/name").and_then(Value::as_str), Some("atm-mcp"));
    assert!(result["instructions"].as_str().unwrap().chars().count() <= 512);
    let f = s.finish();
    assert!(f.success && !f.timed_out);
}

// ---- T02 modern list + call without discover -------------------------------------------------------------------------------------------
#[test]
fn t02_modern_list_and_call_without_discover() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let list = s.call(modern(1, "tools/list", json!({})), 1);
    let tools = list.pointer("/result/tools").and_then(Value::as_array).expect("tools");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(names, ["atm_catalog_search_v1", "atm_catalog_get_v1", "atm_runtime_status_v1", "atm_checkpoint_summary_v1"]);
    assert_eq!(list.pointer("/result/resultType").and_then(Value::as_str), Some("complete"));
    assert!(list.pointer("/result/ttlMs").is_some() && list.pointer("/result/cacheScope").is_some());
    for t in tools {
        assert_eq!(t.pointer("/annotations/readOnlyHint"), Some(&json!(true)));
        assert_eq!(t.pointer("/annotations/destructiveHint"), Some(&json!(false)));
        assert_eq!(t.pointer("/annotations/openWorldHint"), Some(&json!(false)));
        assert_eq!(t.pointer("/inputSchema/additionalProperties"), Some(&json!(false)));
    }
    let call = s.call(modern_call(2, "atm_catalog_search_v1", json!({"query": "gitleaks"})), 2);
    assert_eq!(call.pointer("/result/resultType").and_then(Value::as_str), Some("complete"));
    assert_eq!(structured(&call)["schema"], "atm.mcp.catalog_search.v1");
    assert_eq!(call.pointer("/result/isError"), Some(&json!(false)));
    // the text block carries the same JSON, compact
    let text = call.pointer("/result/content/0/text").and_then(Value::as_str).unwrap();
    assert_eq!(serde_json::from_str::<Value>(text).unwrap(), *structured(&call));
}

// ---- T03 / T04 legacy initialize ---------------------------------------------------------------------------------------------------------
fn legacy_flow(version: &str) {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let init = s.call(legacy_initialize(1, version), 1);
    assert_eq!(init.pointer("/result/protocolVersion").and_then(Value::as_str), Some(version));
    assert_eq!(init.pointer("/result/serverInfo/name").and_then(Value::as_str), Some("atm-mcp"));
    s.send(INITIALIZED);
    let list = s.call(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}).to_string(), 2);
    assert_eq!(list.pointer("/result/tools").and_then(Value::as_array).map(Vec::len), Some(4));
    assert!(list.pointer("/result/resultType").is_none(), "legacy peers must not receive resultType (SDK strips it)");
    let call = s.call(legacy_call(3, "atm_runtime_status_v1", json!({})), 3);
    assert_eq!(structured(&call)["schema"], "atm.mcp.runtime_status.v1");
    assert!(call.pointer("/result/resultType").is_none());
    assert!(s.finish().success);
}

#[test]
fn t03_legacy_initialize_2025_06_18_codex_default() {
    legacy_flow("2025-06-18");
}

#[test]
fn t04_legacy_initialize_2025_11_25() {
    legacy_flow("2025-11-25");
}

// ---- T05 unsupported legacy version ------------------------------------------------------------------------------------------------------
// EXPECTED OUTCOME, from rmcp 3.9.0 source (INFERRED; NOT_RUN): `negotiate_protocol_version` (service/server.rs 478-509) answers an
// `initialize` that names a version outside `supported_protocol_versions()` with the NEWEST supported version that still has an `initialize`
// handshake: max of {2025-06-18, 2025-11-25} = "2025-11-25". It is a result, never an error, and never 2024-11-05.
#[test]
fn t05_legacy_initialize_2024_11_05_is_answered_with_2025_11_25() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let r = s.call(legacy_initialize(1, "2024-11-05"), 1);
    assert_eq!(r.pointer("/result/protocolVersion").and_then(Value::as_str), Some("2025-11-25"), "{r}");
    assert!(r.get("error").is_none());
    assert!(s.finish().success);
}

// ---- T06 -32022 --------------------------------------------------------------------------------------------------------------------------
#[test]
fn t06_modern_unsupported_version_gets_minus_32022() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let mut params = json!({});
    params["_meta"] = meta("1900-01-01");
    let line = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": params}).to_string();
    let r = s.call(line, 1);
    assert_eq!(error_code(&r), Some(-32022));
    assert!(r.get("result").is_none());
    let supported: Vec<&str> = r.pointer("/error/data/supported").and_then(Value::as_array).expect("supported list").iter().filter_map(Value::as_str).collect();
    for v in SUPPORTED {
        assert!(supported.contains(&v));
    }
    ok_probe(&mut s, 2);
}

// ---- T07 missing / partial _meta -> -32602, never a result ------------------------------------------------------------------------------
#[test]
fn t07_modern_request_with_missing_or_partial_meta_is_invalid_params() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let r = s.call(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}).to_string(), 1);
    assert_eq!(error_code(&r), Some(-32602));
    assert!(r.get("result").is_none());
    let partial = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "atm_runtime_status_v1", "arguments": {},
        "_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"}}});
    let r = s.call(partial.to_string(), 2);
    assert_eq!(error_code(&r), Some(-32602));
    assert!(r.get("result").is_none());
    ok_probe(&mut s, 3);
}

// ---- T08 era mixing ---------------------------------------------------------------------------------------------------------------------
#[test]
fn t08a_discover_then_initialize_then_call() {
    // rmcp serve_server_with_ct_inner (service/server.rs 565-639): `server/discover` is answered without choosing a lifecycle; an
    // `initialize` then selects the legacy lifecycle. INFERRED from source, NOT_RUN.
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    assert!(s.call(discover(1), 1).get("result").is_some());
    let init = s.call(legacy_initialize(2, "2025-06-18"), 2);
    assert_eq!(init.pointer("/result/protocolVersion").and_then(Value::as_str), Some("2025-06-18"));
    s.send(INITIALIZED);
    let call = s.call(legacy_call(3, "atm_runtime_status_v1", json!({})), 3);
    assert!(call.pointer("/result/resultType").is_none(), "after initialize the session is legacy");
    assert!(s.finish().success);
}

// T08b EXPECTED OUTCOME, from rmcp 3.9.0 source (INFERRED; NOT_RUN):
//  * A first request that carries complete modern `_meta` opens an INLINE session (service/server.rs 640-671: `set_inline_protocol_version`).
//  * A later `initialize` is not refused: `handle_request` (handler/server.rs 80-135) does not check the era for `InitializeRequest`
//    (`uses_inline_negotiation` is false for it) and runs the default `initialize` -> a result with the requested legacy version.
//  * The inline marker is never cleared, so `request_metadata_required()` (service.rs 1054) stays true: a later request WITHOUT complete
//    `_meta` is refused with -32602 (handler/server.rs 105-125), even after `initialize`; a request WITH complete `_meta` is served.
#[test]
fn t08b_modern_request_then_initialize_then_requests() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let first = s.call(modern_call(1, "atm_runtime_status_v1", json!({})), 1);
    assert_eq!(first.pointer("/result/resultType").and_then(Value::as_str), Some("complete"));
    let init = s.call(legacy_initialize(2, "2025-06-18"), 2);
    assert_eq!(init.pointer("/result/protocolVersion").and_then(Value::as_str), Some("2025-06-18"), "{init}");
    assert!(init.get("error").is_none());
    s.send(INITIALIZED);
    let no_meta = s.call(legacy_call(3, "atm_runtime_status_v1", json!({})), 3);
    assert_eq!(error_code(&no_meta), Some(-32602), "{no_meta}");
    assert!(no_meta.get("result").is_none());
    let with_meta = s.call(modern_call(4, "atm_runtime_status_v1", json!({})), 4);
    assert_eq!(structured(&with_meta)["schema"], "atm.mcp.runtime_status.v1");
    assert!(s.finish().success);
}

// T08c EXPECTED OUTCOME (INFERRED; NOT_RUN): after a legacy `initialize`, a request that DECLARES 2026-07-28 in its own `_meta` is checked
// for complete metadata (handler/server.rs 99-125, `requested_version >= 2026-07-28`): complete -> served; partial -> -32602.
#[test]
fn t08c_initialize_then_modern_request_with_complete_and_partial_meta() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let init = s.call(legacy_initialize(1, "2025-06-18"), 1);
    assert_eq!(init.pointer("/result/protocolVersion").and_then(Value::as_str), Some("2025-06-18"));
    s.send(INITIALIZED);
    let ok = s.call(modern_call(2, "atm_runtime_status_v1", json!({})), 2);
    assert_eq!(structured(&ok)["schema"], "atm.mcp.runtime_status.v1", "{ok}");
    let partial = json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "atm_runtime_status_v1", "arguments": {},
        "_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"}}});
    let r = s.call(partial.to_string(), 3);
    assert_eq!(error_code(&r), Some(-32602), "{r}");
    assert!(s.finish().success);
}

// ---- T09 malformed / odd frames ---------------------------------------------------------------------------------------------------------
#[test]
fn t09_malformed_and_odd_frames_do_not_disturb_the_process() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    s.send("{this is not json");
    ok_probe(&mut s, 1);
    s.send(&format!("{}{}", discover(2), discover(3))); // two messages on one line
    ok_probe(&mut s, 4);
    s.send(r#"{"jsonrpc":"2.0","id":null,"method":"server/discover"}"#);
    ok_probe(&mut s, 5);
    s.send(r#"{"jsonrpc":"2.0","id":77,"result":{}}"#); // a client RESPONSE: this server sends no requests, so it is dropped
    ok_probe(&mut s, 6);
    s.send_bytes(&[0xff, 0xfe, 0x00]);
    ok_probe(&mut s, 7);
    let f = s.finish();
    assert!(f.success);
    // none of the ids 2, 3, 77 got an answer
    for m in f.leftover.iter() {
        assert!(![json!(2), json!(3), json!(77)].contains(&m["id"]), "unexpected answer {m}");
    }
}

// ---- T10 duplicate keys at the real wire (Phase 0 review caveat 1) -----------------------------------------------------------------------
// Every fixture below is RAW JSON TEXT sent as bytes (never a serde_json Value, which would collapse duplicates or normalize escapes before
// transmission). Escape sequences are spelled with `esc`, which builds a real backslash-u-hex sequence at run time.
//
// | id | fixture                                                         | contract                                              |
// |----|-----------------------------------------------------------------|-------------------------------------------------------|
// | 10 | arguments: "id" twice, plain                                     | REJECT, -32600 with id 10                             |
// | 11 | arguments: "id" and the escaped spelling of "id"                 | REJECT, -32600 with id 11 (compared after unescaping) |
// | 12 | params: "_meta" and the escaped spelling of "_meta"              | REJECT, -32600 with id 12                             |
// | 13 | envelope: "method" twice                                         | REJECT, -32600 with id 13                             |
// | 14 | envelope: "id" twice (14 and 15)                                 | NO reply addressed to 14 or 15 (ambiguous id)         |
// | 16 | envelope: "id" and the escaped spelling of "id" (16 and 17)      | NO reply addressed to 16 or 17                        |
// | 18 | a clean request after all of the above                           | SERVED (the stream stayed in sync)                    |
// | 19 | arguments: the single key "id" SPELLED with an escape            | SERVED (an escaped single key is not a duplicate)     |
fn raw_call(id_text: &str, name: &str, args_text: &str, meta_text: &str, tail: &str) -> String {
    format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":{id_text},\"method\":\"tools/call\",\"params\":{{\"name\":\"{name}\",\"arguments\":{args_text},{meta_text}{tail}}}}}"
    )
}

const META_TEXT: &str = "\"_meta\":{\"io.modelcontextprotocol/protocolVersion\":\"2026-07-28\",\"io.modelcontextprotocol/clientCapabilities\":{},\"io.modelcontextprotocol/clientInfo\":{\"name\":\"t\",\"version\":\"0\"}}";

fn esc(c: char) -> String {
    format!("{}u{:04x}", char::from(92u8), c as u32)
}

#[test]
fn t10_duplicate_keys_at_envelope_and_nested_arguments() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let esc_i = esc('i');
    let esc_underscore = esc('_');

    let f10 = raw_call("10", "atm_catalog_get_v1", "{\"id\":\"gitleaks\",\"id\":\"graphify\"}", META_TEXT, "");
    let f11 = raw_call("11", "atm_catalog_get_v1", &format!("{{\"id\":\"gitleaks\",\"{esc_i}d\":\"graphify\"}}"), META_TEXT, "");
    let f12 = raw_call("12", "atm_catalog_get_v1", "{\"id\":\"gitleaks\"}", META_TEXT, &format!(",\"{esc_underscore}meta\":{{}}"));
    let f13 = format!("{{\"jsonrpc\":\"2.0\",\"id\":13,\"method\":\"tools/list\",\"method\":\"server/discover\",\"params\":{{{META_TEXT}}}}}");
    let f14 = format!("{{\"jsonrpc\":\"2.0\",\"id\":14,\"id\":15,\"method\":\"tools/list\",\"params\":{{{META_TEXT}}}}}");
    let f16 = format!("{{\"jsonrpc\":\"2.0\",\"id\":16,\"{esc_i}d\":17,\"method\":\"tools/list\",\"params\":{{{META_TEXT}}}}}");
    let f18 = raw_call("18", "atm_catalog_get_v1", "{\"id\":\"gitleaks\"}", META_TEXT, "");
    let f19 = raw_call("19", "atm_catalog_get_v1", &format!("{{\"{esc_i}d\":\"gitleaks\"}}"), META_TEXT, "");

    // The fixtures must really differ at the byte level where the table says they do (guards against a normalized fixture).
    assert!(f11.contains(&format!("{esc_i}d")) && !f10.contains(&esc_i));
    assert!(f12.contains(&format!("{esc_underscore}meta")));
    assert!(f16.contains(&format!("{esc_i}d")) && f19.contains(&format!("{esc_i}d")));
    assert_ne!(f10, f11);

    for (id, frame) in [(10, &f10), (11, &f11), (12, &f12), (13, &f13)] {
        s.send(frame);
        let r = s.recv_id(&json!(id), 10_000).unwrap_or_else(|| panic!("no answer for {id}"));
        assert_eq!(error_code(&r), Some(-32600), "frame {id}: {r}");
        assert!(r.get("result").is_none(), "a duplicate-key request must never produce a result (frame {id})");
    }
    s.send(&f14);
    s.send(&f16);
    let ok = s.call(f18, 18);
    assert_eq!(structured(&ok)["id"], "gitleaks");
    let escaped_single = s.call(f19, 19);
    assert_eq!(structured(&escaped_single)["id"], "gitleaks", "an escaped single key must still be served");
    let f = s.finish();
    for m in f.leftover.iter() {
        assert!(![json!(14), json!(15), json!(16), json!(17)].contains(&m["id"]), "ambiguous id answered: {m}");
    }
    // exactly one reply per rejected, readable id
    for id in [10, 11, 12, 13] {
        let n = f.stdout_lines.iter().filter(|l| serde_json::from_str::<Value>(l).map(|v| v["id"] == json!(id)).unwrap_or(false)).count();
        assert_eq!(n, 1, "id {id} was answered {n} times");
    }
}

// Replay: rejected frames followed immediately by valid frames in ONE write, then EOF. Each rejected id is answered exactly once, every
// valid frame is answered, the process exits.
#[test]
fn t10b_rejected_frames_in_a_burst_with_eof_are_answered_exactly_once() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let dup = |id: i64| raw_call(&id.to_string(), "atm_catalog_get_v1", "{\"id\":\"a\",\"id\":\"b\"}", META_TEXT, "");
    let good = |id: i64| raw_call(&id.to_string(), "atm_catalog_get_v1", "{\"id\":\"gitleaks\"}", META_TEXT, "");
    let burst: Vec<String> = vec![dup(30), good(31), dup(32), dup(33), good(34)];
    s.send(&burst.join("\n"));
    for id in [31, 34] {
        assert!(s.recv_id(&json!(id), 15_000).is_some(), "valid frame {id} unanswered");
    }
    for id in [30, 32, 33] {
        let r = s.recv_id(&json!(id), 15_000).unwrap_or_else(|| panic!("rejected frame {id} unanswered"));
        assert_eq!(error_code(&r), Some(-32600));
    }
    let f = s.finish();
    assert!(f.success && !f.timed_out);
    for id in [30, 31, 32, 33, 34] {
        let n = f.stdout_lines.iter().filter(|l| serde_json::from_str::<Value>(l).map(|v| v["id"] == json!(id)).unwrap_or(false)).count();
        assert_eq!(n, 1, "id {id} was answered {n} times");
    }
}

// ---- T11 frame cap before decode (Phase 0 review caveat 2), depth, big lines -------------------------------------------------------------
#[test]
fn t11_oversized_deep_and_huge_frames() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    // 20 KiB line: valid JSON, over the 16 KiB cap -> dropped without a reply; the next frame still works
    let big = format!(r#"{{"jsonrpc":"2.0","id":20,"method":"tools/list","params":{{"pad":"{}"}}}}"#, "a".repeat(20 * 1024));
    s.send(&big);
    ok_probe(&mut s, 21);
    // exactly-below-cap line is parsed normally (the answer is an error for missing _meta, which proves it was decoded)
    let below = format!(r#"{{"jsonrpc":"2.0","id":22,"method":"tools/list","params":{{"pad":"{}"}}}}"#, "a".repeat(15 * 1024));
    let r = s.call(below, 22);
    assert!(r.get("error").is_some());
    // 10 MB line with a newline only at the end: discarded in chunks, then the stream is back in sync
    let huge = vec![b'x'; 10 * 1024 * 1024];
    s.send_bytes(&huge);
    ok_probe(&mut s, 23);
    // 40-deep nesting (over the guard's 32, under serde_json's 128): answered -32600 with the id
    let deep40 = format!(r#"{{"jsonrpc":"2.0","id":24,"method":"tools/list","params":{{"x":{}1{}}}}}"#, "[".repeat(40), "]".repeat(40));
    s.send(&deep40);
    assert_eq!(error_code(&s.recv_id(&json!(24), 10_000).expect("answer")), Some(-32600));
    // 1000-deep nesting: past serde_json's own limit, even the id cannot be read -> dropped, process healthy
    let deep1000 = format!(r#"{{"jsonrpc":"2.0","id":25,"method":"tools/list","params":{{"x":{}1{}}}}}"#, "[".repeat(1000), "]".repeat(1000));
    s.send(&deep1000);
    ok_probe(&mut s, 26);
    let f = s.finish();
    assert!(f.success && !f.timed_out);
    assert!(f.stdout_lines.iter().all(|l| l.len() < 64 * 1024), "no oversized output");
}

// ---- T12 unknown tool, case variant, extra keys -----------------------------------------------------------------------------------------
#[test]
fn t12_unknown_tool_case_variant_and_extra_argument_keys() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    for (id, name) in [(1, "atm_unknown_v1"), (2, "ATM_CATALOG_SEARCH_V1"), (3, "atm_catalog_search_v2"), (4, "")] {
        let r = s.call(modern_call(id, name, json!({})), id);
        assert_eq!(error_code(&r), Some(-32602), "tool name {name:?}");
        if !name.is_empty() {
            assert!(!r.to_string().contains(name), "tool name must not be echoed");
        }
    }
    for (id, key) in [(5, "accepted"), (6, "url"), (7, "path"), (8, "command")] {
        let r = s.call(modern_call(id, "atm_catalog_get_v1", json!({"id": "gitleaks", key: "x"})), id);
        assert_eq!(r.pointer("/result/isError"), Some(&json!(true)));
        assert_eq!(structured(&r)["error"]["code"], "ATM_INPUT_INVALID");
        assert!(!r.to_string().contains(key), "unknown key name must not be echoed");
    }
    ok_probe(&mut s, 9);
}

// ---- T14 unknown but well-formed id ------------------------------------------------------------------------------------------------------
#[test]
fn t14_unknown_catalog_id() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let r = s.call(modern_call(1, "atm_catalog_get_v1", json!({"id": "no-such-tool"})), 1);
    assert_eq!(r.pointer("/result/isError"), Some(&json!(true)));
    assert_eq!(structured(&r)["error"]["code"], "ATM_UNKNOWN_CATALOG_ID");
    assert!(!r.to_string().contains("no-such-tool"), "id must not be echoed");
}

// ---- T16 fake evidence PASS over the wire -----------------------------------------------------------------------------------------------
#[test]
fn t16_fake_pass_fields_are_refused_and_never_echoed() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let args = json!({"commitSha": "0123456789abcdef0123456789abcdef01234567", "runId": 1, "runAttempt": 1, "outcome": "VERIFIED_PASS", "accepted": true, "ci": "VERIFIED_PASS"});
    let r = s.call(modern_call(1, "atm_checkpoint_summary_v1", args), 1);
    assert_eq!(r.pointer("/result/isError"), Some(&json!(true)));
    let text = r.to_string();
    assert!(!text.contains("VERIFIED_PASS") && !text.contains("accepted"));
}

// ---- T17 checkpoint tool over the wire ---------------------------------------------------------------------------------------------------
#[test]
fn t17_checkpoint_is_always_not_assessed() {
    let p = Probe::new();
    let mut s = Session::start(&p, &[]);
    let r = s.call(modern_call(1, "atm_checkpoint_summary_v1", json!({"commitSha": "0123456789abcdef0123456789abcdef01234567", "runId": 41349383788u64, "runAttempt": 1})), 1);
    let v = structured(&r);
    assert_eq!(v["outcome"], "NOT_ASSESSED");
    assert_eq!(v["ci"], "NOT_EVALUATED");
    assert_eq!(v["authorization"], "AUTHORIZATION_PENDING");
    assert_eq!(v["error"], "LIVE_VERIFICATION_NOT_AVAILABLE_IN_MCP_V1");
    assert_eq!(v["binding"]["repositoryId"], 1413039335u64);
    let f = s.finish();
    assert!(f.success);
}
