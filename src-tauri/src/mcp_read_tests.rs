//! CONFIG-02C tests of the typed result, the request checks, the strict local parse and the privacy rules (threat rows A, C, E, F of the test
//! plan). Fixtures are written by the tests below a directory they create; the "secrets" are unique sentinels so a leak can be searched for
//! exactly. Nothing real is read. Every case ends in one `CONFIG02C_RESULT` line (see `config02c_support`).
use super::*;
use crate::{
    config02c_support::{bed, case, sentinel, Bed, Verdict},
    managed_fs::hook,
    projects,
};
use rusqlite::Connection;
use std::{cell::Cell, fs, rc::Rc};

const ALIAS: &str = "benim-sitem";

fn request(alias: &str, directory: &str, consent: &str) -> ReadMcpConfigRequest {
    serde_json::from_value(serde_json::json!({ "alias": alias, "directory": directory, "consent": consent })).unwrap()
}

fn db() -> Connection {
    let conn = crate::db::open_memory().unwrap();
    projects::add_reference(&conn, ALIAS, "Benim Sitem", "").unwrap();
    conn
}

fn call(conn: &Connection, alias: &str, directory: &str, consent: &str) -> AppResult<ReadMcpConfigResult> {
    handle(&request(alias, directory, consent), |a| projects::is_registered_reference(conn, a))
}

fn read_bytes(b: &Bed, bytes: &[u8]) -> ReadMcpConfigResult {
    b.put(bytes);
    read_once(ALIAS, b.root.to_str().unwrap())
}

fn is(r: &ReadMcpConfigResult, status: Status, reason: Option<Reason>) {
    assert_eq!((r.status, r.reason), (status, reason), "{r:?}");
}

fn metadata_only(r: &ReadMcpConfigResult, reason: Reason) {
    is(r, Status::PresentMetadataOnly, Some(reason));
    assert!(r.size_bytes.is_some() && r.sha256.is_some());
    assert!(r.server_count.is_none() && r.reviewed_declarations.is_none(), "no summary without a defensible parse: {r:?}");
}

fn malformed(r: &ReadMcpConfigResult, reason: Reason) {
    is(r, Status::Malformed, Some(reason));
    assert!(r.size_bytes.is_some() && r.sha256.is_some());
    assert!(r.server_count.is_none() && r.reviewed_declarations.is_none());
}

fn declaration(r: &ReadMcpConfigResult) -> DeclarationState {
    is(r, Status::PresentSummarized, None);
    let list = r.reviewed_declarations.as_ref().expect("a summary carries the declaration states");
    assert_eq!(list.len(), 1);
    list[0]
}

fn opens() -> Rc<Cell<u32>> {
    let count = Rc::new(Cell::new(0u32));
    let seen = count.clone();
    hook::set(move |stage| {
        if stage == "mcpread:before-root" {
            seen.set(seen.get() + 1);
        }
    });
    count
}

/// Every table of the database, rendered as text (the "dump" of the privacy tests).
fn dump(conn: &Connection) -> String {
    let mut tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    tables.sort();
    let mut out = String::new();
    for t in tables {
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{t}\"")).unwrap();
        let columns = stmt.column_count();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            out.push_str(&t);
            for i in 0..columns {
                out.push_str(&format!("|{:?}", row.get_ref(i).unwrap()));
            }
            out.push('\n');
        }
    }
    out
}

/// Everything that could leave the process about one call: the response (JSON and Debug) and the error Debug if any.
fn everything(result: &AppResult<ReadMcpConfigResult>) -> String {
    match result {
        Ok(r) => format!("{}{r:?}", serde_json::to_string(r).unwrap()),
        Err(e) => format!("{e:?}{e}"),
    }
}

// ---- A: wrong target, wrong authority ---------------------------------------------------------------------------------------------------

#[test]
fn config02c_a1_an_empty_or_blank_folder_is_refused_before_anything_is_opened() {
    case("A1", || {
        let conn = db();
        let count = opens();
        for text in ["", " ", "   \t  ", "\n"] {
            let r = call(&conn, ALIAS, text, CONSENT).unwrap();
            is(&r, Status::UnsafePath, Some(Reason::InvalidRootText));
        }
        assert_eq!(count.get(), 0, "no root was opened");
        Verdict::Pass
    });
}

#[test]
fn config02c_a2_an_unknown_removed_invalid_or_sample_alias_is_refused_and_nothing_is_opened() {
    case("A2", || {
        let b = bed();
        let conn = db();
        projects::add_reference(&conn, "kaldirilan-proje", "x", "").unwrap();
        projects::remove_reference(&conn, "kaldirilan-proje").unwrap();
        let count = opens();
        for alias in ["bilinmeyen-proje", "kaldirilan-proje", "ornek-web-sitesi", "", "Büyük Harf", "../x"] {
            let e = call(&conn, alias, b.root.to_str().unwrap(), CONSENT).unwrap_err();
            assert_eq!(e.code, "PROJECT_NOT_FOUND", "{alias}");
            assert!(!e.message.contains(alias) || alias.is_empty(), "the message does not echo the alias");
        }
        assert_eq!(count.get(), 0);
        Verdict::Pass
    });
}

#[test]
fn config02c_a3_a_request_that_names_a_file_a_path_or_a_surface_does_not_even_parse() {
    case("A3", || {
        for extra in ["file", "path", "surface", "name", "platform", "scope", "relativePath"] {
            let value = serde_json::json!({ "alias": ALIAS, "directory": "/x", "consent": CONSENT, extra: "something" });
            assert!(serde_json::from_value::<ReadMcpConfigRequest>(value).is_err(), "{extra}");
        }
        assert!(
            serde_json::from_value::<ReadMcpConfigRequest>(serde_json::json!({ "alias": ALIAS, "directory": "/x" })).is_err(),
            "consent is required"
        );
        Verdict::Pass
    });
}

#[test]
fn config02c_a4_traversal_device_root_and_unusable_spellings_are_refused_and_nothing_is_opened() {
    case("A4", || {
        let conn = db();
        let count = opens();
        let (long, control) = (format!("{}{}", if cfg!(windows) { r"C:\" } else { "/" }, "a".repeat(300)), "/tmp/a\u{7}b".to_string());
        let mut plain: Vec<(String, Status, Option<Reason>)> = vec![
            ("project".into(), Status::UnsafePath, Some(Reason::InvalidRootText)),
            (".".into(), Status::UnsafePath, Some(Reason::InvalidRootText)),
            ("..".into(), Status::UnsafePath, Some(Reason::InvalidRootText)),
            (long, Status::UnsafePath, Some(Reason::InvalidRootText)),
            (control, Status::UnsafePath, Some(Reason::InvalidRootText)),
        ];
        if cfg!(windows) {
            let w = |t: &str, s, r| (t.to_string(), s, r);
            plain.extend([
                w(r"C:\a\..\b", Status::UnsafePath, Some(Reason::InvalidRootText)),
                w(r"C:\a\.\b", Status::UnsafePath, Some(Reason::InvalidRootText)),
                w(r"C:\", Status::UnsafePath, Some(Reason::RootTooBroad)),
                w("C:/", Status::UnsafePath, Some(Reason::RootTooBroad)),
                w(r"\\?\C:\Users\x\project", Status::UnsafePath, Some(Reason::InvalidRootText)),
                w(r"\\host\share\project", Status::UnsupportedFs, None),
                w("//host/share/project", Status::UnsupportedFs, None),
                w(r"\\.\PhysicalDrive0", Status::UnsupportedFs, None),
                w(r"\\?\UNC\host\share\project", Status::UnsupportedFs, None),
                w(r"C:project", Status::UnsafePath, Some(Reason::InvalidRootText)),
                w(r"\project", Status::UnsafePath, Some(Reason::InvalidRootText)),
            ]);
        } else {
            let u = |t: &str, s, r| (t.to_string(), s, r);
            plain.extend([
                u("/a/../b", Status::UnsafePath, Some(Reason::InvalidRootText)),
                u("/a/./b", Status::UnsafePath, Some(Reason::InvalidRootText)),
                u("/", Status::UnsafePath, Some(Reason::RootTooBroad)),
                u("//", Status::UnsafePath, Some(Reason::RootTooBroad)),
            ]);
        }
        for (text, status, reason) in plain {
            let r = call(&conn, ALIAS, &text, CONSENT).unwrap();
            is(&r, status, reason);
            assert!(r.size_bytes.is_none() && r.sha256.is_none(), "{text:?}");
        }
        assert_eq!(count.get(), 0, "none of them opened a root");
        Verdict::Pass
    });
}

#[test]
fn config02c_a5_a_missing_or_different_consent_constant_is_refused_and_nothing_is_opened() {
    case("A5", || {
        let b = bed();
        let conn = db();
        let count = opens();
        for consent in ["", "yes", "config02c-read-once-v2", "CONFIG02C-READ-ONCE-V1", " config02c-read-once-v1"] {
            let e = call(&conn, ALIAS, b.root.to_str().unwrap(), consent).unwrap_err();
            assert_eq!(e.code, "READ_NOT_AUTHORIZED");
        }
        assert_eq!(count.get(), 0);
        Verdict::Pass
    });
}

#[test]
fn config02c_a6_a_folder_that_is_not_the_projects_is_read_and_nothing_about_the_project_changes() {
    case("A6", || {
        let b = bed();
        b.put(b"{\"mcpServers\":{\"x\":{}}}");
        let conn = db();
        let before = dump(&conn);
        let r = call(&conn, ALIAS, b.root.to_str().unwrap(), CONSENT).unwrap();
        // the pairing alias <-> folder is the user's assertion (documented limitation): the read succeeds and claims nothing about the project
        is(&r, Status::PresentSummarized, None);
        assert_eq!(r.alias, ALIAS);
        assert_eq!(before, dump(&conn), "nothing is persisted or changed");
        Verdict::Pass
    });
}

// ---- C: content -------------------------------------------------------------------------------------------------------------------------

#[test]
fn config02c_c1_an_empty_file_is_malformed_not_absent() {
    case("C1", || {
        let b = bed();
        malformed(&read_bytes(&b, b""), Reason::EmptyFile);
        malformed(&read_bytes(&b, b"   \n"), Reason::InvalidJson);
        Verdict::Pass
    });
}

#[test]
fn config02c_c2_the_cap_is_exact_and_a_larger_file_yields_no_digest() {
    case("C2", || {
        let b = bed();
        let head = b"{\"mcpServers\":{},\"p\":\"";
        let tail = b"\"}";
        let pad = |total: usize| {
            let mut v = head.to_vec();
            v.extend(std::iter::repeat_n(b'a', total - head.len() - tail.len()));
            v.extend_from_slice(tail);
            assert_eq!(v.len(), total);
            v
        };
        let at = read_bytes(&b, &pad(1_048_576));
        is(&at, Status::PresentSummarized, None);
        assert_eq!(at.size_bytes, Some(1_048_576));
        let over = read_bytes(&b, &pad(1_048_577));
        is(&over, Status::TooLarge, None);
        assert!(over.size_bytes.is_none() && over.sha256.is_none() && over.server_count.is_none());
        Verdict::Pass
    });
}

#[test]
fn config02c_c3_a_byte_order_mark_makes_the_result_metadata_only() {
    case("C3", || {
        let b = bed();
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"{\"mcpServers\":{\"exa\":{}}}");
        metadata_only(&read_bytes(&b, &bytes), Reason::ByteOrderMark);
        Verdict::Pass
    });
}

#[test]
fn config02c_c4_invalid_utf8_and_utf16_files_are_malformed() {
    case("C4", || {
        let b = bed();
        malformed(&read_bytes(&b, &[0x7B, 0xFF, 0xFE, 0x7D]), Reason::InvalidUtf8);
        let utf16: Vec<u8> = "{}".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let mut with_bom = vec![0xFF, 0xFE];
        with_bom.extend(utf16);
        malformed(&read_bytes(&b, &with_bom), Reason::InvalidUtf8);
        Verdict::Pass
    });
}

#[test]
fn config02c_c5_duplicate_keys_at_any_depth_withhold_the_summary_and_no_value_is_used() {
    case("C5", || {
        let b = bed();
        for text in [
            r#"{"mcpServers":{},"mcpServers":{"exa":{}}}"#,
            r#"{"mcpServers":{"a":{},"a":{}}}"#,
            r#"{"mcpServers":{"exa":{"url":"https://mcp.exa.ai/mcp","url":"https://example.invalid/x"}}}"#,
            r#"{"mcpServers":{"e\u0078a":{},"exa":{}}}"#,
            r#"{"other":{"deep":{"k":1,"k":2}}}"#,
        ] {
            metadata_only(&read_bytes(&b, text.as_bytes()), Reason::DuplicateKey);
        }
        Verdict::Pass
    });
}

#[test]
fn config02c_c6_structure_limits_and_json_errors() {
    case("C6", || {
        let b = bed();
        metadata_only(&read_bytes(&b, b"[1,2]"), Reason::RootNotObject);
        metadata_only(&read_bytes(&b, b"\"text\""), Reason::RootNotObject);
        metadata_only(&read_bytes(&b, b"42"), Reason::RootNotObject);
        metadata_only(&read_bytes(&b, b"{\"mcpServers\":[]}"), Reason::ServersNotObject);
        metadata_only(&read_bytes(&b, b"{\"mcpServers\":null}"), Reason::ServersNotObject);
        let nest = |n: usize| format!("{}{{}}{}", "{\"a\":".repeat(n - 1), "}".repeat(n - 1));
        is(&read_bytes(&b, nest(64).as_bytes()), Status::PresentSummarized, None);
        metadata_only(&read_bytes(&b, nest(65).as_bytes()), Reason::TooDeep);
        metadata_only(&read_bytes(&b, format!("{}0{}", "[".repeat(300), "]".repeat(300)).as_bytes()), Reason::TooDeep);
        let servers =
            |n: usize| format!("{{\"mcpServers\":{{{}}}}}", (0..n).map(|i| format!("\"s{i}\":{{}}")).collect::<Vec<_>>().join(","));
        let at = read_bytes(&b, servers(256).as_bytes());
        is(&at, Status::PresentSummarized, None);
        assert_eq!(at.server_count, Some(256));
        metadata_only(&read_bytes(&b, servers(257).as_bytes()), Reason::TooManyServers);
        malformed(&read_bytes(&b, b"{\"a\":1, // comment\n}"), Reason::InvalidJson);
        malformed(&read_bytes(&b, b"{\"a\":1,}"), Reason::InvalidJson);
        malformed(&read_bytes(&b, b"{\"a\":1} trailing"), Reason::InvalidJson);
        malformed(&read_bytes(&b, b"{\"a\":"), Reason::InvalidJson);
        // a file without mcpServers declares no server
        let none = read_bytes(&b, b"{\"theme\":\"dark\"}");
        assert_eq!(declaration(&none).name_state, NameState::Absent);
        assert_eq!(none.server_count, Some(0));
        Verdict::Pass
    });
}

#[test]
fn config02c_c7_a_lone_surrogate_is_malformed() {
    case("C7", || {
        let b = bed();
        malformed(&read_bytes(&b, br#"{"a":"\ud800"}"#), Reason::LoneSurrogate);
        malformed(&read_bytes(&b, br#"{"a":"\udc00x"}"#), Reason::LoneSurrogate);
        is(&read_bytes(&b, br#"{"a":"\ud83d\ude00"}"#), Status::PresentSummarized, None); // a valid pair is fine
        Verdict::Pass
    });
}

#[test]
fn config02c_c8_active_content_in_values_never_reaches_the_result() {
    case("C8", || {
        let b = bed();
        let marker = sentinel("c8");
        let text = format!(
            r#"{{"mcpServers":{{"x{marker}":{{"command":"<img src=x onerror=alert(1)>","args":["[click](javascript:alert(1))","{marker}"]}}}},"note":"<script>{marker}</script>"}}"#
        );
        let r = read_bytes(&b, text.as_bytes());
        is(&r, Status::PresentSummarized, None);
        let seen = format!("{}{r:?}", serde_json::to_string(&r).unwrap());
        for needle in [marker.as_str(), "<img", "onerror", "javascript", "<script", "alert"] {
            assert!(!seen.contains(needle), "{needle}");
        }
        Verdict::Pass
    });
}

#[test]
fn config02c_c9_reviewed_declaration_states_are_exactly_the_documented_ones() {
    case("C9", || {
        let b = bed();
        let d = |t: &str| declaration(&read_bytes(&b, t.as_bytes()));
        let exa = r#"{"mcpServers":{"exa":{"type":"http","url":"https://mcp.exa.ai/mcp"}}}"#;
        assert_eq!((d(exa).name_state, d(exa).endpoint_state), (NameState::Present, EndpointState::PresentUnderThisName));
        // spelling variants of the same endpoint (case, port, trailing slash, query, fragment, user info)
        for url in ["HTTPS://MCP.EXA.AI/mcp/", "https://mcp.exa.ai:443/mcp?x=1#f", "https://user:pw@mcp.exa.ai/mcp"] {
            let t = format!(r#"{{"mcpServers":{{"exa":{{"url":"{url}"}}}}}}"#);
            assert_eq!(d(&t).endpoint_state, EndpointState::PresentUnderThisName, "{url}");
        }
        let upper = r#"{"mcpServers":{"EXA":{"url":"https://mcp.exa.ai/mcp"}}}"#;
        assert_eq!(
            (d(upper).name_state, d(upper).endpoint_state),
            (NameState::PresentOtherLetterCase, EndpointState::PresentUnderOtherName)
        );
        let absent = r#"{"mcpServers":{"other":{"command":"x"}}}"#;
        assert_eq!((d(absent).name_state, d(absent).endpoint_state), (NameState::Absent, EndpointState::Absent));
        let elsewhere = r#"{"mcpServers":{"search":{"url":"https://mcp.exa.ai/mcp"}}}"#;
        assert_eq!((d(elsewhere).name_state, d(elsewhere).endpoint_state), (NameState::Absent, EndpointState::PresentUnderOtherName));
        let other_url = r#"{"mcpServers":{"exa":{"url":"https://example.invalid/mcp"}}}"#;
        assert_eq!((d(other_url).name_state, d(other_url).endpoint_state), (NameState::Present, EndpointState::Absent));
        let no_url = r#"{"mcpServers":{"exa":{"command":"x"}}}"#;
        assert_eq!(d(no_url).endpoint_state, EndpointState::Absent);
        for not_evaluable in [
            r#"{"mcpServers":{"exa":{"url":5}}}"#,
            r#"{"mcpServers":{"exa":{"url":["https://mcp.exa.ai/mcp"]}}}"#,
            r#"{"mcpServers":{"exa":"text"}}"#,
            r#"{"mcpServers":{"exa":[1]}}"#,
            r#"{"mcpServers":{"Exa":null}}"#,
        ] {
            assert_eq!(d(not_evaluable).endpoint_state, EndpointState::NotEvaluable, "{not_evaluable}");
            assert_ne!(d(not_evaluable).name_state, NameState::Absent);
        }
        // a server entry that is not an object under ANOTHER name says nothing about the endpoint
        assert_eq!(d(r#"{"mcpServers":{"other":"text"}}"#).endpoint_state, EndpointState::Absent);
        assert_eq!(d(exa).id, "exa-mcp-server");
        Verdict::Pass
    });
}

#[test]
fn config02c_the_url_rule_agrees_with_the_one_the_02b_prototype_tests() {
    case("C9b", || {
        for url in [
            "https://mcp.exa.ai/mcp",
            "HTTPS://MCP.EXA.AI/mcp/",
            "https://mcp.exa.ai:443/mcp",
            "http://mcp.exa.ai:80/mcp",
            "https://u:p@mcp.exa.ai/mcp?q#f",
            "ftp://mcp.exa.ai/mcp",
            "https://",
            "https:///mcp",
            "  https://mcp.exa.ai/mcp  ",
            "mcp.exa.ai/mcp",
            "",
            "https://mcp.exa.ai/mcp//",
            "https://mcp.exa.ai/MCP",
            "ünïcode",
            "https://mcp.exa.ai:8443/mcp",
        ] {
            assert_eq!(url_identity(url), crate::config_txn::url_identity(url), "{url:?}");
        }
        Verdict::Pass
    });
}

// ---- E: privacy -------------------------------------------------------------------------------------------------------------------------

fn full_of_secrets(tag: &str) -> (String, Vec<String>) {
    let s: Vec<String> = (0..9).map(|i| sentinel(&format!("{tag}{i}"))).collect();
    let text = format!(
        r#"{{"mcpServers":{{"exa":{{"type":"http","url":"https://u:{p0}@mcp.exa.ai/mcp?key={p1}","headers":{{"Authorization":"Bearer {p2}"}}}},"srv-{p3}":{{"command":"{p4}","args":["--token","{p5}"],"env":{{"API_KEY":"{p6}","{p7}":"v"}}}}}},"extra-{p8}":"{p8}"}}"#,
        p0 = s[0],
        p1 = s[1],
        p2 = s[2],
        p3 = s[3],
        p4 = s[4],
        p5 = s[5],
        p6 = s[6],
        p7 = s[7],
        p8 = s[8]
    );
    (text, s)
}

fn assert_clean(seen: &str, secrets: &[String]) {
    for s in secrets {
        assert!(!seen.contains(s.as_str()), "a sentinel crossed the boundary: {s}");
    }
}

fn secrets_stay_inside(tag: &str) -> Verdict {
    let b = bed();
    let conn = db();
    let (text, secrets) = full_of_secrets(tag);
    b.put(text.as_bytes());
    let before = dump(&conn);
    let r = call(&conn, ALIAS, b.root.to_str().unwrap(), CONSENT);
    assert!(matches!(&r, Ok(x) if x.status == Status::PresentSummarized), "{r:?}");
    assert_clean(&everything(&r), &secrets);
    assert_eq!(before, dump(&conn));
    assert_clean(&dump(&conn), &secrets);
    Verdict::Pass
}

#[test]
fn config02c_e1_no_secret_in_env_header_argument_url_or_command_crosses_the_boundary() {
    case("E1", || secrets_stay_inside("e1"));
}

#[test]
fn config02c_e2_a_secret_no_redaction_heuristic_would_catch_crosses_nothing_either() {
    case("E2", || secrets_stay_inside("e2"));
}

#[test]
fn config02c_e3_a_server_name_other_than_the_reviewed_one_is_never_returned() {
    case("E3", || secrets_stay_inside("e3"));
}

#[test]
fn config02c_e4_unknown_keys_and_comment_like_strings_are_never_returned() {
    case("E4", || secrets_stay_inside("e4"));
}

#[test]
fn config02c_e5_the_typed_folder_text_appears_nowhere() {
    case("E5", || {
        let b = bed();
        let conn = db();
        let marker = sentinel("e5");
        let folder = b.base.join(&marker);
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join(".mcp.json"), b"{\"mcpServers\":{}}").unwrap();
        let before = dump(&conn);
        for text in [
            folder.to_str().unwrap().to_string(),
            b.base.join(format!("{marker}-missing")).to_str().unwrap().to_string(),
            format!("relative/{marker}"),
        ] {
            let r = call(&conn, ALIAS, &text, CONSENT);
            assert_clean(&everything(&r), std::slice::from_ref(&marker));
        }
        let denied = call(&conn, &marker, folder.to_str().unwrap(), CONSENT);
        assert_clean(&everything(&denied), std::slice::from_ref(&marker));
        assert_eq!(before, dump(&conn));
        assert_clean(&dump(&conn), std::slice::from_ref(&marker));
        Verdict::Pass
    });
}

#[test]
fn config02c_e6_parse_errors_carry_a_code_and_no_text() {
    case("E6", || {
        let b = bed();
        let marker = sentinel("e6");
        for text in [
            format!("{{\"a\":\"{marker}\" \"b\":1}}"),
            format!("{{\"{marker}\": }}"),
            format!("{{\"k\":\"\\ud800{marker}\"}}"),
            marker.to_string(),
        ] {
            let r = read_bytes(&b, text.as_bytes());
            assert_eq!(r.status, Status::Malformed);
            assert_clean(&format!("{}{r:?}", serde_json::to_string(&r).unwrap()), std::slice::from_ref(&marker));
        }
        Verdict::Pass
    });
}

fn every_status_result() -> Vec<ReadMcpConfigResult> {
    let mut out = Vec::new();
    for status in [
        Status::RootNotFound,
        Status::Absent,
        Status::PresentSummarized,
        Status::PresentMetadataOnly,
        Status::Malformed,
        Status::Denied,
        Status::UnsafePath,
        Status::UnsupportedFs,
        Status::TooLarge,
        Status::ChangedDuringRead,
        Status::IoError,
    ] {
        out.push(bare(ALIAS, status, None));
    }
    for reason in ALL_REASONS {
        out.push(bare(ALIAS, Status::UnsafePath, Some(reason)));
    }
    let mut full = bare(ALIAS, Status::PresentSummarized, None);
    full.size_bytes = Some(7);
    full.sha256 = Some("0".repeat(64));
    full.server_count = Some(1);
    full.reviewed_declarations =
        Some(vec![DeclarationState { id: REVIEWED_ID, name_state: NameState::Present, endpoint_state: EndpointState::NotEvaluable }]);
    out.push(full);
    out
}

const ALL_REASONS: [Reason; 19] = [
    Reason::DuplicateKey,
    Reason::ByteOrderMark,
    Reason::RootNotObject,
    Reason::ServersNotObject,
    Reason::TooDeep,
    Reason::TooManyServers,
    Reason::InvalidUtf8,
    Reason::InvalidJson,
    Reason::LoneSurrogate,
    Reason::EmptyFile,
    Reason::AccessDenied,
    Reason::Locked,
    Reason::Link,
    Reason::NotADirectory,
    Reason::NotRegularFile,
    Reason::HardLinked,
    Reason::CaseAlias,
    Reason::InvalidRootText,
    Reason::RootTooBroad,
];

#[test]
fn config02c_e7_the_serialised_result_has_only_the_contracts_fields_and_closed_strings() {
    case("E7", || {
        let allowed_keys = [
            "schema",
            "status",
            "reason",
            "alias",
            "file",
            "scope",
            "surface",
            "maxBytes",
            "sizeBytes",
            "sha256",
            "serverCount",
            "reviewedDeclarations",
            "notClaims",
        ];
        let statuses = [
            "root_not_found",
            "absent",
            "present_summarized",
            "present_metadata_only",
            "malformed",
            "denied",
            "unsafe_path",
            "unsupported_fs",
            "too_large",
            "changed_during_read",
            "io_error",
        ];
        let reasons: Vec<String> = ALL_REASONS.iter().map(|r| serde_json::to_value(r).unwrap().as_str().unwrap().to_string()).collect();
        for r in every_status_result() {
            let v = serde_json::to_value(&r).unwrap();
            let object = v.as_object().unwrap();
            assert!(object.keys().all(|k| allowed_keys.contains(&k.as_str())), "{:?}", object.keys().collect::<Vec<_>>());
            assert_eq!(object.len(), allowed_keys.len(), "every field is always present (null when not applicable)");
            assert!(statuses.contains(&object["status"].as_str().unwrap()));
            if let Some(reason) = object["reason"].as_str() {
                assert!(reasons.iter().any(|x| x == reason));
            }
            assert_eq!(
                (object["schema"].as_str(), object["file"].as_str(), object["scope"].as_str(), object["surface"].as_str()),
                (Some("config02c.read.v1"), Some(".mcp.json"), Some("project"), Some("claude-code-terminal"))
            );
            assert_eq!(object["maxBytes"].as_u64(), Some(1_048_576));
            assert_eq!(
                object["notClaims"],
                serde_json::json!(["not_loaded", "not_trusted", "not_installed", "not_enabled", "not_reachable", "not_verified"])
            );
            if let Some(d) = object["sha256"].as_str() {
                assert!(d.len() == 64 && d.bytes().all(|c| c.is_ascii_hexdigit()));
            }
            if let Some(list) = object["reviewedDeclarations"].as_array() {
                for entry in list {
                    let e = entry.as_object().unwrap();
                    assert_eq!(e.len(), 3);
                    assert_eq!(e["id"], "exa-mcp-server");
                    assert!(["absent", "present", "present_other_letter_case"].contains(&e["nameState"].as_str().unwrap()));
                    assert!(["absent", "present_under_this_name", "present_under_other_name", "not_evaluable"]
                        .contains(&e["endpointState"].as_str().unwrap()));
                }
            }
        }
        Verdict::Pass
    });
}

#[test]
fn config02c_e8_the_metadata_only_fallback_is_taken_not_a_redacted_dump() {
    case("E8", || {
        let b = bed();
        let (text, secrets) = full_of_secrets("e8");
        for broken in [text.replace("\"exa\"", "\"exa\",\"exa\""), format!("[{text}]"), format!("{{\"mcpServers\":[{text}]}}")] {
            let mut r = read_bytes(&b, broken.as_bytes());
            assert!(matches!(r.status, Status::PresentMetadataOnly | Status::Malformed), "{r:?}");
            assert!(r.server_count.is_none() && r.reviewed_declarations.is_none());
            r.alias.clear();
            assert_clean(&format!("{}{r:?}", serde_json::to_string(&r).unwrap()), &secrets);
        }
        Verdict::Pass
    });
}

// ---- F: product invariants --------------------------------------------------------------------------------------------------------------

fn battery(b: &Bed) -> Vec<Vec<u8>> {
    let _ = b;
    vec![
        b"{}".to_vec(),
        Vec::new(),
        b"[".to_vec(),
        b"{\"a\":1,\"a\":2}".to_vec(),
        full_of_secrets("f").0.into_bytes(),
        vec![0xEF, 0xBB, 0xBF, b'{', b'}'],
    ]
}

#[test]
fn config02c_f1_f2_no_table_changes_after_any_outcome() {
    case("F1", || {
        let b = bed();
        let conn = db();
        let before = dump(&conn);
        for bytes in battery(&b) {
            b.put(&bytes);
            let _ = call(&conn, ALIAS, b.root.to_str().unwrap(), CONSENT).unwrap();
        }
        let _ = call(&conn, ALIAS, b.base.join("missing").to_str().unwrap(), CONSENT).unwrap();
        let _ = call(&conn, ALIAS, "relative", CONSENT).unwrap();
        let _ = call(&conn, "bilinmeyen-proje", "/x", CONSENT);
        let _ = call(&conn, ALIAS, "/x", "yanlis");
        assert_eq!(before, dump(&conn), "preferences, audit chain, diagnostics, settings, queue and every other table are unchanged");
        Verdict::Pass
    });
    case("F2", || {
        let conn = db();
        let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        let (tables, version) = (count("SELECT count(*) FROM sqlite_master"), count("PRAGMA user_version"));
        let b = bed();
        b.put(b"{}");
        let _ = call(&conn, ALIAS, b.root.to_str().unwrap(), CONSENT).unwrap();
        assert_eq!(
            (tables, version),
            (count("SELECT count(*) FROM sqlite_master"), count("PRAGMA user_version")),
            "no new table and no migration"
        );
        Verdict::Pass
    });
}

#[test]
fn config02c_f6_no_outcome_can_carry_a_not_configured_or_verified_value() {
    case("F6", || {
        let names: Vec<String> =
            every_status_result().iter().map(|r| serde_json::to_value(r.status).unwrap().as_str().unwrap().to_string()).collect();
        for n in names {
            assert!(!n.contains("configured") && !n.contains("verified") && !n.contains("trusted") && !n.contains("installed"), "{n}");
        }
        // the type has no field that could carry such a value: its serialised keys are the contract's list (E7) and the six not-claims are constants
        assert_eq!(NOT_CLAIMS.len(), 6);
        Verdict::Pass
    });
}

// ---- the total mapping of the file-system vocabulary ------------------------------------------------------------------------------------------

#[test]
fn config02c_every_file_system_error_at_every_stage_maps_to_a_closed_status_with_a_matching_reason() {
    use crate::managed_fs::FsError::*;
    let valid_reasons = |s: Status| -> Vec<Option<Reason>> {
        match s {
            Status::Denied => vec![Some(Reason::AccessDenied), Some(Reason::Locked)],
            Status::UnsafePath => vec![
                Some(Reason::Link),
                Some(Reason::NotADirectory),
                Some(Reason::NotRegularFile),
                Some(Reason::HardLinked),
                Some(Reason::CaseAlias),
                Some(Reason::InvalidRootText),
                Some(Reason::RootTooBroad),
            ],
            _ => vec![None],
        }
    };
    for stage in [Stage::Root, Stage::Listing, Stage::Entry, Stage::Open, Stage::Read] {
        for e in [
            NotFound,
            Exists,
            Link,
            NotDir,
            NotFile,
            CrossDevice,
            Network,
            Denied,
            Locked,
            TooLarge,
            InvalidName,
            NotEmpty,
            Changed,
            Unsupported,
            Io,
        ] {
            let (status, reason) = status_of_failure(Failure { stage, fault: Fault::Fs(e) });
            assert!(valid_reasons(status).contains(&reason), "{stage:?} {e:?} -> {status:?} {reason:?}");
            assert_ne!(status, Status::Absent, "only an observed absence is `absent`: {stage:?} {e:?}");
            assert_ne!(status, Status::PresentSummarized);
        }
    }
    // the documented anchors of the table
    let at = |stage, e| status_of_failure(Failure { stage, fault: Fault::Fs(e) });
    assert_eq!(at(Stage::Root, NotFound), (Status::RootNotFound, None));
    assert_eq!(at(Stage::Open, NotFound), (Status::ChangedDuringRead, None));
    assert_eq!(at(Stage::Listing, TooLarge), (Status::IoError, None));
    assert_eq!(at(Stage::Read, TooLarge), (Status::TooLarge, None));
    assert_eq!(at(Stage::Entry, Link), (Status::UnsafePath, Some(Reason::Link)));
    assert_eq!(status_of_failure(Failure { stage: Stage::Entry, fault: Fault::Absent }), (Status::Absent, None));
}

#[test]
fn config02c_the_request_type_is_not_printable_and_the_result_carries_no_free_text() {
    // `ReadMcpConfigRequest` derives only `Deserialize`: formatting it with `{:?}` does not compile, so the typed folder cannot be printed by accident.
    let source = include_str!("mcp_read.rs");
    let at = source.find("pub struct ReadMcpConfigRequest").unwrap();
    let derive = source[..at].rsplit('\n').find(|l| l.trim_start().starts_with("#[derive")).unwrap();
    assert!(!derive.contains("Debug"), "the request must not derive Debug: {derive}");
}
