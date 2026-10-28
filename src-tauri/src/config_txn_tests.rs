//! CONFIG-02B tests of the PURE half: strict JSON, the one-stanza merge, the plan and the approval fixture. No file is touched; every value is
//! made up (the "secret" is an obviously fake sentinel).

use super::*;

const SENTINEL: &str = "SENTINEL-NOT-A-REAL-TOKEN-0123456789";
const ID: Identity = Identity { volume: 7, object: 42 };

fn insp(bytes: &[u8]) -> Inspection {
    Inspection::new(Presence::Present { identity: ID, size: bytes.len() as u64, sha256: sha256_hex(bytes) }, Some(bytes.to_vec()), 0)
}

fn absent() -> Inspection {
    Inspection::new(Presence::Absent, None, 0)
}

fn req(now: u64) -> Request<'static> {
    Request {
        alias: "benim-sitem",
        platform: Platform::WindowsX64,
        surface: SURFACE,
        entry_id: ENTRY_ID,
        catalog_pin: CATALOG_PIN,
        repository_url: REPOSITORY_URL,
        root: Identity { volume: 7, object: 1 },
        now,
        ttl: 300,
    }
}

fn plan_from(i: &Inspection, now: u64) -> Plan {
    prepare(&req(now), Some(Platform::WindowsX64), i).expect("plan")
}

fn merge(text: &str) -> Result<String, Refusal> {
    build_new_text(Some(text.as_bytes())).map(|b| String::from_utf8(b).unwrap())
}

// ---- golden: the same cases the TypeScript preview is tested against ----------------------------------------------------------------

#[test]
fn the_golden_cases_are_written_exactly_as_the_typescript_preview_shows_them() {
    let golden: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/config-02b-golden.json")).unwrap();
    let cases = golden["cases"].as_array().unwrap();
    assert!(cases.len() >= 5);
    for c in cases {
        let got = build_new_text(c["input"].as_str().map(str::as_bytes)).expect("merge");
        assert_eq!(String::from_utf8(got).unwrap(), c["expectedAfter"].as_str().unwrap(), "{}", c["name"]);
    }
}

// ---- strict parsing ------------------------------------------------------------------------------------------------------------------

#[test]
fn valid_documents_parse_and_keep_member_order_and_number_text() {
    let j = parse_strict(br#"{"b": 1, "a": [1.50, -0, 12345678901234567890123, 1e3], "c": {"x": null}}"#).unwrap();
    let Json::Obj(m) = &j else { panic!() };
    assert_eq!(m.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["b", "a", "c"]);
    // numbers are never re-printed: the writer emits the very text that was read
    let out = merge(r#"{"big": 12345678901234567890123, "f": 1.50, "e": 1e3}"#).unwrap();
    assert!(out.contains("12345678901234567890123") && out.contains("1.50") && out.contains("1e3"), "{out}");
}

#[test]
fn refused_texts_are_refused_with_the_right_code() {
    let cases: &[(&[u8], Refusal)] = &[
        (b"", Refusal::MalformedJson),
        (b"   \n", Refusal::MalformedJson),
        (b"// c\n{}", Refusal::MalformedJson),
        (b"{ /* c */ }", Refusal::MalformedJson),
        (br#"{"a": 1,}"#, Refusal::MalformedJson),
        (b"[1,2,]", Refusal::MalformedJson),
        (br#"{'a': 1}"#, Refusal::MalformedJson),
        (br#"{"a": NaN}"#, Refusal::MalformedJson),
        (br#"{"a": 01}"#, Refusal::MalformedJson),
        (br#"{"a": 1.}"#, Refusal::MalformedJson),
        (br#"{"a": .5}"#, Refusal::MalformedJson),
        (br#"{"a": 1} x"#, Refusal::MalformedJson),
        (br#"{"a" 1}"#, Refusal::MalformedJson),
        (b"{\"a\": \"line\nbreak\"}", Refusal::MalformedJson),
        (br#"{"a": 1, "a": 2}"#, Refusal::DuplicateKey),
        (br#"{"a": 1, "a": 2}"#, Refusal::DuplicateKey),
        (br#"{"x": {"k": 1, "k": 2}}"#, Refusal::DuplicateKey),
        (br#"{"mcpServers": {"exa": {}, "exa": {}}}"#, Refusal::DuplicateKey),
        (br#"{"a": "\ud800"}"#, Refusal::LoneSurrogate),
        (br#"{"a": "\udc00x"}"#, Refusal::LoneSurrogate),
        (b"\xEF\xBB\xBF{}", Refusal::Bom),
        (b"{\"a\": \"\xff\"}", Refusal::InvalidUtf8),
    ];
    for (text, want) in cases {
        assert_eq!(parse_strict(text), Err(*want), "{:?}", String::from_utf8_lossy(text));
        assert_eq!(build_new_text(Some(text)).err(), Some(*want), "merge of {:?}", String::from_utf8_lossy(text));
    }
    assert_eq!(parse_strict(&vec![b' '; (MAX_FILE_BYTES + 1) as usize]), Err(Refusal::FileTooLarge));
    let deep = format!("{}{}", "[".repeat(MAX_DEPTH + 2), "]".repeat(MAX_DEPTH + 2));
    assert_eq!(parse_strict(deep.as_bytes()), Err(Refusal::TooDeep));
}

#[test]
fn a_decoded_surrogate_pair_is_kept_as_one_character() {
    let out = merge(r#"{"s": "😀 ğ"}"#).unwrap();
    assert!(out.contains("😀 ğ"), "{out}");
}

// ---- the merge: what is added, what is refused ---------------------------------------------------------------------------------------

#[test]
fn exactly_the_owned_stanza_is_added_and_everything_else_is_unchanged() {
    let original = r#"{"mcpServers": {"other": {"command": "x", "args": ["--flag"], "env": {"K": "${SOME_VAR}"}}}, "theme": "dark", "nested": {"a": [1, {"b": true}]}}"#;
    let out = merge(original).unwrap();
    let before: serde_json::Value = serde_json::from_str(original).unwrap();
    let mut after: serde_json::Value = serde_json::from_str(&out).unwrap();
    let exa = after["mcpServers"].as_object_mut().unwrap().remove("exa").unwrap();
    assert_eq!(exa, serde_json::json!({"type": "http", "url": "https://mcp.exa.ai/mcp"}));
    assert_eq!(after, before);
    assert!(out.ends_with("}\n") && !out.contains('\r'));
}

#[test]
fn a_file_without_mcp_servers_gets_one_at_the_end() {
    let out = merge(r#"{"a": 1}"#).unwrap();
    assert_eq!(out, "{\n  \"a\": 1,\n  \"mcpServers\": {\n    \"exa\": {\n      \"type\": \"http\",\n      \"url\": \"https://mcp.exa.ai/mcp\"\n    }\n  }\n}\n");
}

#[test]
fn same_name_in_any_case_is_a_conflict() {
    for name in ["exa", "EXA", "Exa", "eXa"] {
        let text = format!(r#"{{"mcpServers": {{"{name}": {{"type": "http", "url": "https://elsewhere.invalid/x"}}}}}}"#);
        assert_eq!(merge(&text), Err(Refusal::ServerNameConflict), "{name}");
    }
}

#[test]
fn the_same_endpoint_under_another_name_is_a_conflict_however_it_is_spelled() {
    for url in [
        "https://mcp.exa.ai/mcp",
        "https://mcp.exa.ai/mcp/",
        "HTTPS://MCP.EXA.AI/mcp",
        "https://mcp.exa.ai:443/mcp",
        "https://mcp.exa.ai/mcp?tools=web_search_exa",
        "https://mcp.exa.ai/mcp#frag",
        "https://user@mcp.exa.ai/mcp",
    ] {
        let text = format!(r#"{{"mcpServers": {{"search": {{"type": "http", "url": "{url}"}}}}}}"#);
        assert_eq!(merge(&text), Err(Refusal::EndpointAlreadyConfigured), "{url}");
    }
    // a different path or host is a different endpoint; a server with no url is not an endpoint at all
    for ok in [
        r#"{"mcpServers": {"s": {"url": "https://mcp.exa.ai/other"}}}"#,
        r#"{"mcpServers": {"s": {"url": "https://exa.ai/mcp"}}}"#,
        r#"{"mcpServers": {"s": {"command": "x"}}}"#,
    ] {
        assert!(merge(ok).is_ok(), "{ok}");
    }
}

#[test]
fn shapes_the_writer_cannot_extend_are_refused() {
    assert_eq!(merge("[]"), Err(Refusal::RootNotObject));
    assert_eq!(merge("\"x\""), Err(Refusal::RootNotObject));
    assert_eq!(merge(r#"{"mcpServers": []}"#), Err(Refusal::ServersNotObject));
    assert_eq!(merge(r#"{"mcpServers": null}"#), Err(Refusal::ServersNotObject));
}

#[test]
fn text_derived_from_the_masked_preview_is_never_accepted_as_the_original() {
    let text = format!(r#"{{"mcpServers": {{"x": {{"headers": {{"Authorization": "{PREVIEW_MASK}"}}}}}}}}"#);
    assert_eq!(merge(&text), Err(Refusal::RedactionMarkerPresent));
}

#[test]
fn a_secret_in_the_original_is_preserved_exactly_and_never_reported() {
    let text = format!(r#"{{"mcpServers": {{"x": {{"headers": {{"Authorization": "Bearer {SENTINEL}"}}}}}}}}"#);
    let out = merge(&text).unwrap();
    assert!(out.contains(&format!("Bearer {SENTINEL}")), "the other server must come through unchanged");
    // refusals carry a code and nothing else
    let bad = format!(r#"{{"k": "{SENTINEL}", "k": 1}}"#);
    let r = merge(&bad).unwrap_err();
    assert!(!format!("{r:?} {r}").contains(SENTINEL));
}

#[test]
fn applying_the_result_again_adds_nothing() {
    let once = merge(r#"{"mcpServers": {}}"#).unwrap();
    assert_eq!(merge(&once), Err(Refusal::ServerNameConflict));
}

#[test]
fn url_identity_is_protocol_host_and_path_only() {
    assert_eq!(url_identity("https://mcp.exa.ai/mcp").as_deref(), Some("https://mcp.exa.ai/mcp"));
    assert_eq!(url_identity("http://Example.INVALID:80/a/b//").as_deref(), Some("http://example.invalid/a/b"));
    assert_eq!(url_identity("ftp://x/y"), None);
    assert_eq!(url_identity("https://"), None);
    assert_eq!(url_identity("not a url"), None);
}

// ---- the plan ------------------------------------------------------------------------------------------------------------------------

#[test]
fn a_plan_is_bound_to_the_project_os_surface_entry_pin_and_a_bounded_lifetime() {
    let i = absent();
    let base = req(1000);
    let ok = |r: &Request, h| prepare(r, h, &i);
    assert!(ok(&base, Some(Platform::WindowsX64)).is_ok());
    assert_eq!(ok(&base, Some(Platform::MacosArm64)).err(), Some(Refusal::WrongOs), "a plan for the other OS");
    assert_eq!(ok(&base, None).err(), Some(Refusal::UnsupportedHost));
    assert_eq!(ok(&Request { alias: "Bad Alias!", ..base.clone() }, Some(Platform::WindowsX64)).err(), Some(Refusal::InvalidProject));
    assert_eq!(ok(&Request { alias: "", ..base.clone() }, Some(Platform::WindowsX64)).err(), Some(Refusal::InvalidProject));
    for surface in ["antigravity-ide", "codex-cli", "claude-desktop-code", "local-cli", ""] {
        assert_eq!(ok(&Request { surface, ..base.clone() }, Some(Platform::WindowsX64)).err(), Some(Refusal::WrongSurface), "{surface}");
    }
    assert_eq!(ok(&Request { entry_id: "playwright-mcp", ..base.clone() }, Some(Platform::WindowsX64)).err(), Some(Refusal::WrongEntry));
    assert_eq!(
        ok(&Request { catalog_pin: "0000000000000000000000000000000000000000", ..base.clone() }, Some(Platform::WindowsX64)).err(),
        Some(Refusal::CatalogPinMismatch)
    );
    assert_eq!(
        ok(&Request { repository_url: "https://github.com/someone-else/exa-mcp-server", ..base.clone() }, Some(Platform::WindowsX64)).err(),
        Some(Refusal::CatalogPinMismatch)
    );
    assert_eq!(ok(&Request { ttl: 0, ..base.clone() }, Some(Platform::WindowsX64)).err(), Some(Refusal::TtlTooLong));
    assert_eq!(ok(&Request { ttl: MAX_TTL + 1, ..base.clone() }, Some(Platform::WindowsX64)).err(), Some(Refusal::TtlTooLong));
}

#[test]
fn the_plan_digest_changes_with_every_bound_fact() {
    let a = plan_from(&absent(), 1000);
    assert_eq!(a.digest(), plan_from(&absent(), 1000).digest(), "deterministic");
    let different = [
        plan_from(&absent(), 1001),             // lifetime
        plan_from(&insp(br#"{"a": 1}"#), 1000), // existing content
        prepare(&Request { alias: "baska-proje", ..req(1000) }, Some(Platform::WindowsX64), &absent()).unwrap(), // project
        prepare(&Request { root: Identity { volume: 7, object: 2 }, ..req(1000) }, Some(Platform::WindowsX64), &absent()).unwrap(), // root identity
    ];
    for d in &different {
        assert_ne!(d.digest(), a.digest());
    }
    assert_eq!(a.new_sha256(), sha256_hex(a.new_bytes()));
}

#[test]
fn the_inspected_tuple_must_describe_the_bytes_the_text_was_built_from() {
    let lying = Inspection::new(Presence::Present { identity: ID, size: 3, sha256: sha256_hex(b"abc") }, Some(br#"{"a": 1}"#.to_vec()), 0);
    assert_eq!(prepare(&req(1), Some(Platform::WindowsX64), &lying).err(), Some(Refusal::TargetChanged));
    let absent_with_bytes = Inspection::new(Presence::Absent, Some(b"{}".to_vec()), 0);
    assert_eq!(prepare(&req(1), Some(Platform::WindowsX64), &absent_with_bytes).err(), Some(Refusal::TargetChanged));
}

// ---- the approval fixture ------------------------------------------------------------------------------------------------------------

#[test]
fn an_approval_is_bound_to_one_plan_one_use_and_a_lifetime() {
    let p = plan_from(&absent(), 1000);
    let other = plan_from(&insp(br#"{"a": 1}"#), 1000);
    let mut ledger = Ledger::default();
    assert_eq!(authorize(&p, None, &mut ledger, 1000), Err(Refusal::ApprovalMissing));
    assert_eq!(authorize(&p, Some(&test_grant(&other, 1)), &mut ledger, 1000), Err(Refusal::ApprovalMismatch));
    assert_eq!(authorize(&p, Some(&test_grant(&p, 2)), &mut ledger, p.expires_at() + 1), Err(Refusal::PlanExpired));
    let a = test_grant(&p, 3);
    assert_eq!(authorize(&p, Some(&a), &mut ledger, 1000), Ok(()));
    assert_eq!(authorize(&p, Some(&a), &mut ledger, 1000), Err(Refusal::ApprovalReused), "one-shot");
    // a failed check does not spend an approval
    let b = test_grant(&p, 4);
    assert_eq!(authorize(&other, Some(&b), &mut ledger, 1000), Err(Refusal::ApprovalMismatch));
    assert_eq!(authorize(&p, Some(&b), &mut ledger, 1000), Ok(()));
}

#[test]
fn refusal_codes_are_unique_and_carry_no_free_text() {
    let all = [
        Refusal::InvalidProject,
        Refusal::WrongOs,
        Refusal::UnsupportedHost,
        Refusal::WrongSurface,
        Refusal::WrongEntry,
        Refusal::CatalogPinMismatch,
        Refusal::TtlTooLong,
        Refusal::FileTooLarge,
        Refusal::InvalidUtf8,
        Refusal::Bom,
        Refusal::MalformedJson,
        Refusal::DuplicateKey,
        Refusal::LoneSurrogate,
        Refusal::TooDeep,
        Refusal::RootNotObject,
        Refusal::ServersNotObject,
        Refusal::RedactionMarkerPresent,
        Refusal::ServerNameConflict,
        Refusal::EndpointAlreadyConfigured,
        Refusal::MergeInvariant,
        Refusal::TargetIsLink,
        Refusal::TargetNotRegularFile,
        Refusal::TargetHardLinked,
        Refusal::CaseAliasPresent,
        Refusal::RootIdentityChanged,
        Refusal::TargetChanged,
        Refusal::TargetAppeared,
        Refusal::TargetVanished,
        Refusal::BackupNotPrivate,
        Refusal::BackupIsRoot,
        Refusal::BackupExists,
        Refusal::BackupMismatch,
        Refusal::StageMismatch,
        Refusal::WriteAdapterBlocked,
        Refusal::ApprovalMissing,
        Refusal::ApprovalMismatch,
        Refusal::ApprovalReused,
        Refusal::PlanExpired,
        Refusal::WrongProject,
        Refusal::PlanPlatformMismatch,
        Refusal::VerifyFailed,
        Refusal::RollbackRefusedLaterEdit,
        Refusal::RollbackNotNeeded,
        Refusal::Injected,
    ];
    let codes: HashSet<&str> = all.iter().map(|r| r.code()).collect();
    assert_eq!(codes.len(), all.len());
    for c in codes {
        assert!(c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_'), "{c}");
    }
}

#[test]
fn the_constants_are_the_reviewed_declaration() {
    assert_eq!(TARGET_FILE, ".mcp.json");
    assert_eq!(write_pretty(&stanza()), "{\n  \"type\": \"http\",\n  \"url\": \"https://mcp.exa.ai/mcp\"\n}\n");
    assert!(!STANZA_URL.contains(['?', '@', '#']));
    assert_eq!(CATALOG_PIN.len(), 40);
}
