//! CONFIG-02B (Issue #39): the PURE half of the one-file MCP configuration transaction PROTOTYPE -- typed operation plan, strict JSON merge,
//! one-shot approval fixture and the refusal vocabulary. Compiled for TESTS ONLY (`#[cfg(test)] mod config_txn;` in `lib.rs`): no command,
//! no IPC, no capability, no GUI control, no shipping build can reach it. It performs no I/O at all; the file-system half is
//! `managed_fs_mcp.rs`, which reuses the Phase 3C handle-relative layer (`managed_fs`).
//!
//! What this is: a reference for what a future, separately authorised write phase would have to do, exercised on directories created by
//! the tests. What it is not: a consent mechanism (the approval below is a test fixture that nothing persists), an installer, a way to
//! name a path, or permission to touch any real project. `REAL_CONFIG_WRITE_NO_GO` and `REAL_INSTALLATION_NO_GO` stand.
//!
//! The ONE operation: add `mcpServers.exa = {"type":"http","url":"https://mcp.exa.ai/mcp"}` to `.mcp.json` of Claude Code's project scope --
//! the CONFIG-02A reviewed declaration (syntax only, runtime NOT verified, independent review of the declaration is a separate matter).
//! The text written is built HERE from the bytes read from the authorised file; the redacted before/after text of the React preview is
//! never an input, and a file that contains the preview's mask is refused.
#![allow(dead_code)]

use crate::managed_fs::Identity;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

// ---- the one operation (mirrors src/configDeclarations.ts; tests/test_config02b_contract.py cross-checks the two) --------------------

pub const TARGET_FILE: &str = ".mcp.json";
pub const HOST_ID: &str = "claude-code-project-mcp";
pub const SURFACE: &str = "claude-code-terminal";
pub const ENTRY_ID: &str = "exa-mcp-server";
pub const SERVER_NAME: &str = "exa";
pub const STANZA_TYPE: &str = "http";
pub const STANZA_URL: &str = "https://mcp.exa.ai/mcp";
pub const CATALOG_PIN: &str = "0b499f095c4ca145eee07b09137641c037d58449";
pub const REPOSITORY_URL: &str = "https://github.com/exa-labs/exa-mcp-server";
/// What the React preview writes in place of a masked value (`MASK` in `src/configPlan.ts`).
pub const PREVIEW_MASK: &str = "«gizlendi»";

pub const MAX_FILE_BYTES: u64 = 1 << 20;
pub const MAX_DEPTH: usize = 64;
/// A plan is valid for at most this long (logical seconds supplied by the caller; this module never reads a clock).
pub const MAX_TTL: u64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    WindowsX64,
    MacosArm64,
}

impl Platform {
    pub fn id(self) -> &'static str {
        match self {
            Platform::WindowsX64 => "windows-x64",
            Platform::MacosArm64 => "macos-arm64",
        }
    }

    /// The platform this binary was built for; `None` for anything the application does not support.
    pub fn host() -> Option<Platform> {
        if cfg!(all(windows, target_arch = "x86_64")) {
            Some(Platform::WindowsX64)
        } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Some(Platform::MacosArm64)
        } else {
            None
        }
    }
}

// ---- refusals ------------------------------------------------------------------------------------------------------------------------

/// Every way the prototype says no. A refusal carries a code and NOTHING else: no path, no file content, no value from the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    // plan: who/where/what
    InvalidProject,
    WrongOs,
    UnsupportedHost,
    WrongSurface,
    WrongEntry,
    CatalogPinMismatch,
    TtlTooLong,
    // the file's text
    FileTooLarge,
    InvalidUtf8,
    Bom,
    MalformedJson,
    DuplicateKey,
    LoneSurrogate,
    TooDeep,
    RootNotObject,
    ServersNotObject,
    RedactionMarkerPresent,
    // conflicts
    ServerNameConflict,
    EndpointAlreadyConfigured,
    MergeInvariant,
    // the file system
    TargetIsLink,
    TargetNotRegularFile,
    TargetHardLinked,
    CaseAliasPresent,
    RootIdentityChanged,
    TargetChanged,
    TargetAppeared,
    TargetVanished,
    BackupNotPrivate,
    BackupIsRoot,
    BackupExists,
    BackupMismatch,
    StageMismatch,
    /// The volume or system offers no atomic no-clobber primitive: nothing is written and there is NO fallback to an ordinary rename.
    WriteAdapterBlocked,
    Fs(&'static str),
    // approval / expiry
    ApprovalMissing,
    ApprovalMismatch,
    ApprovalReused,
    PlanExpired,
    WrongProject,
    PlanPlatformMismatch,
    // after the swap
    VerifyFailed,
    RollbackRefusedLaterEdit,
    RollbackNotNeeded,
    Injected,
}

impl Refusal {
    pub fn code(&self) -> &'static str {
        match self {
            Refusal::InvalidProject => "invalid_project",
            Refusal::WrongOs => "wrong_os",
            Refusal::UnsupportedHost => "unsupported_host",
            Refusal::WrongSurface => "wrong_surface",
            Refusal::WrongEntry => "wrong_entry",
            Refusal::CatalogPinMismatch => "catalog_pin_mismatch",
            Refusal::TtlTooLong => "ttl_too_long",
            Refusal::FileTooLarge => "file_too_large",
            Refusal::InvalidUtf8 => "invalid_utf8",
            Refusal::Bom => "byte_order_mark",
            Refusal::MalformedJson => "malformed_json",
            Refusal::DuplicateKey => "duplicate_key",
            Refusal::LoneSurrogate => "lone_surrogate",
            Refusal::TooDeep => "too_deep",
            Refusal::RootNotObject => "root_not_object",
            Refusal::ServersNotObject => "mcp_servers_not_object",
            Refusal::RedactionMarkerPresent => "redaction_marker_present",
            Refusal::ServerNameConflict => "server_name_conflict",
            Refusal::EndpointAlreadyConfigured => "endpoint_already_configured",
            Refusal::MergeInvariant => "merge_invariant",
            Refusal::TargetIsLink => "target_is_link",
            Refusal::TargetNotRegularFile => "target_not_regular_file",
            Refusal::TargetHardLinked => "target_hard_linked",
            Refusal::CaseAliasPresent => "case_alias_present",
            Refusal::RootIdentityChanged => "root_identity_changed",
            Refusal::TargetChanged => "target_changed",
            Refusal::TargetAppeared => "target_appeared",
            Refusal::TargetVanished => "target_vanished",
            Refusal::BackupNotPrivate => "backup_not_private",
            Refusal::BackupIsRoot => "backup_is_project_root",
            Refusal::BackupExists => "backup_exists",
            Refusal::BackupMismatch => "backup_mismatch",
            Refusal::StageMismatch => "stage_mismatch",
            Refusal::WriteAdapterBlocked => "WRITE_ADAPTER_BLOCKED",
            Refusal::Fs(c) => c,
            Refusal::ApprovalMissing => "approval_missing",
            Refusal::ApprovalMismatch => "approval_mismatch",
            Refusal::ApprovalReused => "approval_reused",
            Refusal::PlanExpired => "plan_expired",
            Refusal::WrongProject => "wrong_project",
            Refusal::PlanPlatformMismatch => "plan_platform_mismatch",
            Refusal::VerifyFailed => "verify_failed_needs_attention",
            Refusal::RollbackRefusedLaterEdit => "rollback_refused_later_edit",
            Refusal::RollbackNotNeeded => "rollback_not_needed",
            Refusal::Injected => "injected_failure",
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

// ---- strict JSON ---------------------------------------------------------------------------------------------------------------------

/// A JSON value that keeps the ORDER of object members and the exact text of numbers, so that merging one member in and writing the
/// document out cannot change the meaning of anything else.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

type PResult<T> = Result<T, Refusal>;

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn expect(&mut self, b: u8) -> PResult<()> {
        if self.peek() == Some(b) {
            self.i += 1;
            Ok(())
        } else {
            Err(Refusal::MalformedJson)
        }
    }

    fn lit(&mut self, word: &str, v: Json) -> PResult<Json> {
        if self.s[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(Refusal::MalformedJson)
        }
    }

    fn value(&mut self, depth: usize) -> PResult<Json> {
        if depth > MAX_DEPTH {
            return Err(Refusal::TooDeep);
        }
        self.ws();
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.lit("true", Json::Bool(true)),
            Some(b'f') => self.lit("false", Json::Bool(false)),
            Some(b'n') => self.lit("null", Json::Null),
            Some(b'-') | Some(b'0'..=b'9') => self.number(),
            _ => Err(Refusal::MalformedJson), // includes comments, trailing commas, single quotes, NaN ...
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.i;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        self.i - start
    }

    fn number(&mut self) -> PResult<Json> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        match self.peek() {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(Refusal::MalformedJson),
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            if self.digits() == 0 {
                return Err(Refusal::MalformedJson);
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.i += 1;
            }
            if self.digits() == 0 {
                return Err(Refusal::MalformedJson);
            }
        }
        Ok(Json::Num(String::from_utf8_lossy(&self.s[start..self.i]).into_owned()))
    }

    fn hex4(&mut self) -> PResult<u32> {
        if self.i + 4 > self.s.len() {
            return Err(Refusal::MalformedJson);
        }
        let mut v = 0u32;
        for k in 0..4 {
            let d = (self.s[self.i + k] as char).to_digit(16).ok_or(Refusal::MalformedJson)?;
            v = v * 16 + d;
        }
        self.i += 4;
        Ok(v)
    }

    fn string(&mut self) -> PResult<String> {
        self.expect(b'"')?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let b = self.peek().ok_or(Refusal::MalformedJson)?;
            self.i += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let e = self.peek().ok_or(Refusal::MalformedJson)?;
                    self.i += 1;
                    match e {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let cp = if (0xD800..0xDC00).contains(&hi) {
                                if self.s[self.i..].starts_with(b"\\u") {
                                    self.i += 2;
                                    let lo = self.hex4()?;
                                    if !(0xDC00..0xE000).contains(&lo) {
                                        return Err(Refusal::LoneSurrogate);
                                    }
                                    0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                                } else {
                                    return Err(Refusal::LoneSurrogate);
                                }
                            } else if (0xDC00..0xE000).contains(&hi) {
                                return Err(Refusal::LoneSurrogate);
                            } else {
                                hi
                            };
                            let ch = char::from_u32(cp).ok_or(Refusal::LoneSurrogate)?;
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return Err(Refusal::MalformedJson),
                    }
                }
                0..=0x1f => return Err(Refusal::MalformedJson), // a raw control character is not allowed in a JSON string
                _ => out.push(b),
            }
        }
        String::from_utf8(out).map_err(|_| Refusal::InvalidUtf8)
    }

    fn array(&mut self, depth: usize) -> PResult<Json> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(Refusal::MalformedJson),
            }
        }
    }

    fn object(&mut self, depth: usize) -> PResult<Json> {
        self.expect(b'{')?;
        let mut members: Vec<(String, Json)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Json::Obj(members));
        }
        loop {
            self.ws();
            let k = self.string()?;
            if !seen.insert(k.clone()) {
                return Err(Refusal::DuplicateKey); // also when the two spellings differ only by escapes: keys are compared decoded
            }
            self.ws();
            self.expect(b':')?;
            let v = self.value(depth + 1)?;
            members.push((k, v));
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Json::Obj(members));
                }
                _ => return Err(Refusal::MalformedJson),
            }
        }
    }
}

/// Strict RFC 8259 parse: no comments, no trailing commas, no duplicate keys at any depth, no byte-order mark, no lone surrogates,
/// bounded size and depth. Anything else is a refusal -- the prototype never "repairs" a file.
pub fn parse_strict(bytes: &[u8]) -> Result<Json, Refusal> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Refusal::FileTooLarge);
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Err(Refusal::Bom);
    }
    if std::str::from_utf8(bytes).is_err() {
        return Err(Refusal::InvalidUtf8);
    }
    let mut p = Parser { s: bytes, i: 0 };
    let v = p.value(0)?;
    p.ws();
    if p.i != bytes.len() {
        return Err(Refusal::MalformedJson);
    }
    Ok(v)
}

fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_value(v: &Json, indent: usize, out: &mut String) {
    let pad = |n: usize| " ".repeat(n);
    match v {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Num(n) => out.push_str(n),
        Json::Str(s) => quote(s, out),
        Json::Arr(a) if a.is_empty() => out.push_str("[]"),
        Json::Arr(a) => {
            out.push_str("[\n");
            for (i, x) in a.iter().enumerate() {
                out.push_str(&pad(indent + 2));
                write_value(x, indent + 2, out);
                out.push_str(if i + 1 < a.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad(indent));
            out.push(']');
        }
        Json::Obj(m) if m.is_empty() => out.push_str("{}"),
        Json::Obj(m) => {
            out.push_str("{\n");
            for (i, (k, x)) in m.iter().enumerate() {
                out.push_str(&pad(indent + 2));
                quote(k, out);
                out.push_str(": ");
                write_value(x, indent + 2, out);
                out.push_str(if i + 1 < m.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad(indent));
            out.push('}');
        }
    }
}

/// Two-space indentation, one trailing newline -- the same layout as the React preview's `JSON.stringify(v, null, 2) + "\n"`.
pub fn write_pretty(v: &Json) -> String {
    let mut out = String::new();
    write_value(v, 0, &mut out);
    out.push('\n');
    out
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

// ---- the merge -----------------------------------------------------------------------------------------------------------------------

/// protocol + host + path of an http(s) URL, lower-cased where case does not matter and without port 80/443, query, fragment, user info or
/// trailing slashes. `None` for anything that is not an http(s) URL.
pub fn url_identity(url: &str) -> Option<String> {
    let lower = url.trim();
    let (scheme, rest) = if lower.len() >= 8 && lower[..8].eq_ignore_ascii_case("https://") {
        ("https", &lower[8..])
    } else if lower.len() >= 7 && lower[..7].eq_ignore_ascii_case("http://") {
        ("http", &lower[7..])
    } else {
        return None;
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let host_port = authority.rsplit('@').next().unwrap_or(authority).to_ascii_lowercase();
    let host_port = match (scheme, host_port.strip_suffix(":443"), host_port.strip_suffix(":80")) {
        ("https", Some(h), _) => h.to_string(),
        ("http", _, Some(h)) => h.to_string(),
        _ => host_port,
    };
    let path = tail.split(['?', '#']).next().unwrap_or("").trim_end_matches('/');
    if host_port.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{host_port}{path}"))
}

pub fn stanza() -> Json {
    Json::Obj(vec![("type".into(), Json::Str(STANZA_TYPE.into())), ("url".into(), Json::Str(STANZA_URL.into()))])
}

/// The complete new file text. `current` is the exact content of the authorised file (`None`: the file is expected to be absent).
/// Every member of the original document is kept, in its place, byte-for-byte in meaning; exactly one member is added.
pub fn build_new_text(current: Option<&[u8]>) -> Result<Vec<u8>, Refusal> {
    let doc = match current {
        None => Json::Obj(vec![("mcpServers".into(), Json::Obj(vec![(SERVER_NAME.into(), stanza())]))]),
        Some(bytes) => {
            if String::from_utf8_lossy(bytes).contains(PREVIEW_MASK) {
                return Err(Refusal::RedactionMarkerPresent); // text derived from the masked preview must never become a file
            }
            let original = parse_strict(bytes)?;
            let Json::Obj(members) = &original else { return Err(Refusal::RootNotObject) };
            let mut merged = members.clone();
            match merged.iter_mut().find(|(k, _)| k == "mcpServers") {
                None => merged.push(("mcpServers".into(), Json::Obj(vec![(SERVER_NAME.into(), stanza())]))),
                Some((_, Json::Obj(servers))) => {
                    for (name, value) in servers.iter() {
                        if name.eq_ignore_ascii_case(SERVER_NAME) {
                            return Err(Refusal::ServerNameConflict);
                        }
                        if let Some(Json::Str(u)) = value.get("url") {
                            if url_identity(u).is_some() && url_identity(u) == url_identity(STANZA_URL) {
                                return Err(Refusal::EndpointAlreadyConfigured);
                            }
                        }
                    }
                    servers.push((SERVER_NAME.into(), stanza()));
                }
                Some(_) => return Err(Refusal::ServersNotObject),
            }
            let merged = Json::Obj(merged);
            check_only_one_member_added(&original, &merged)?;
            merged
        }
    };
    let text = write_pretty(&doc);
    // the writer's output must be what the strict reader reads back, and nothing but the owned stanza may differ from the original
    if parse_strict(text.as_bytes())? != doc {
        return Err(Refusal::MergeInvariant);
    }
    Ok(text.into_bytes())
}

fn check_only_one_member_added(original: &Json, merged: &Json) -> Result<(), Refusal> {
    let (Json::Obj(a), Json::Obj(b)) = (original, merged) else { return Err(Refusal::MergeInvariant) };
    let extra_root = b.len().checked_sub(a.len()).ok_or(Refusal::MergeInvariant)?;
    // every original member is still where it was; all but `mcpServers` are untouched
    for (i, (k, v)) in a.iter().enumerate() {
        if b[i].0 != *k || (k != "mcpServers" && b[i].1 != *v) {
            return Err(Refusal::MergeInvariant);
        }
    }
    if extra_root > 1 {
        return Err(Refusal::MergeInvariant);
    }
    let servers_of = |o: &Json| o.get("mcpServers").cloned();
    match (servers_of(original), servers_of(merged)) {
        (None, Some(Json::Obj(s))) if s.len() == 1 && s[0].0 == SERVER_NAME && extra_root == 1 => Ok(()),
        (Some(Json::Obj(before)), Some(Json::Obj(after)))
            if extra_root == 0 && after.len() == before.len() + 1 && after[..before.len()] == before[..] =>
        {
            if after[before.len()].0 == SERVER_NAME && after[before.len()].1 == stanza() {
                Ok(())
            } else {
                Err(Refusal::MergeInvariant)
            }
        }
        _ => Err(Refusal::MergeInvariant),
    }
}

// ---- the operation plan --------------------------------------------------------------------------------------------------------------

/// What the authorised file was when it was inspected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Presence {
    Absent,
    Present { identity: Identity, size: u64, sha256: String },
}

/// The result of reading the authorised file through the root handle. `bytes` is private on purpose: only `prepare` consumes it.
#[derive(Debug)]
pub struct Inspection {
    pub presence: Presence,
    /// Files in the root whose names look like this prototype's own staging files (reported, never deleted: ownership cannot be proven).
    pub stray_stage_files: usize,
    bytes: Option<Vec<u8>>,
}

impl Inspection {
    pub(crate) fn new(presence: Presence, bytes: Option<Vec<u8>>, stray_stage_files: usize) -> Inspection {
        Inspection { presence, stray_stage_files, bytes }
    }

    /// The bytes that were read (for the backup copy); `None` when the file was absent.
    pub(crate) fn original_bytes(&self) -> Option<&[u8]> {
        self.bytes.as_deref()
    }
}

/// Everything the caller asserts about the intended operation. In this milestone the root and every token are test fixtures.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    pub alias: &'a str,
    pub platform: Platform,
    pub surface: &'a str,
    pub entry_id: &'a str,
    pub catalog_pin: &'a str,
    pub repository_url: &'a str,
    /// Identity of the root directory handle the plan was made from.
    pub root: Identity,
    /// Logical time in seconds; this module has no clock.
    pub now: u64,
    pub ttl: u64,
}

#[derive(Debug, Clone)]
pub struct Plan {
    alias: String,
    platform: Platform,
    surface: String,
    entry_id: String,
    catalog_pin: String,
    root: Identity,
    expected: Presence,
    new_sha256: String,
    new_bytes: Vec<u8>,
    expires_at: u64,
    digest: String,
}

impl Plan {
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn alias(&self) -> &str {
        &self.alias
    }
    pub fn platform(&self) -> Platform {
        self.platform
    }
    pub fn root(&self) -> Identity {
        self.root
    }
    pub fn expected(&self) -> &Presence {
        &self.expected
    }
    pub fn new_sha256(&self) -> &str {
        &self.new_sha256
    }
    pub fn new_bytes(&self) -> &[u8] {
        &self.new_bytes
    }
    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }
}

/// The digest an approval is bound to: every field that decides what is written, where and on what basis.
fn plan_digest(p: &Plan) -> String {
    let expected = match &p.expected {
        Presence::Absent => "absent".to_string(),
        Presence::Present { identity, size, sha256 } => format!("present:{}:{}:{}:{}", identity.volume, identity.object, size, sha256),
    };
    let text = format!(
        "atm-config-02b-plan-v1\nproject={}\nos={}\nsurface={}\nhost={}\nentry={}\npin={}\nfile={}\nroot={}:{}\nexpected={}\nnew={}\nexpires={}\n",
        p.alias,
        p.platform.id(),
        p.surface,
        HOST_ID,
        p.entry_id,
        p.catalog_pin,
        TARGET_FILE,
        p.root.volume,
        p.root.object,
        expected,
        p.new_sha256,
        p.expires_at
    );
    sha256_hex(text.as_bytes())
}

/// Builds the plan from what was just read. `host` is the platform this code is running on; a plan for the other OS is refused.
pub fn prepare(req: &Request, host: Option<Platform>, insp: &Inspection) -> Result<Plan, Refusal> {
    if !crate::projects::valid_alias(req.alias) {
        return Err(Refusal::InvalidProject);
    }
    let host = host.ok_or(Refusal::UnsupportedHost)?;
    if req.platform != host {
        return Err(Refusal::WrongOs);
    }
    if req.surface != SURFACE {
        return Err(Refusal::WrongSurface);
    }
    if req.entry_id != ENTRY_ID {
        return Err(Refusal::WrongEntry);
    }
    if req.catalog_pin != CATALOG_PIN || req.repository_url != REPOSITORY_URL {
        return Err(Refusal::CatalogPinMismatch);
    }
    if req.ttl == 0 || req.ttl > MAX_TTL {
        return Err(Refusal::TtlTooLong);
    }
    let new_bytes = build_new_text(insp.bytes.as_deref())?;
    // the inspected tuple must describe the very bytes the text was built from
    match (&insp.presence, &insp.bytes) {
        (Presence::Absent, None) => {}
        (Presence::Present { sha256, size, .. }, Some(b)) if *sha256 == sha256_hex(b) && *size == b.len() as u64 => {}
        _ => return Err(Refusal::TargetChanged),
    }
    let mut plan = Plan {
        alias: req.alias.to_string(),
        platform: req.platform,
        surface: req.surface.to_string(),
        entry_id: req.entry_id.to_string(),
        catalog_pin: req.catalog_pin.to_string(),
        root: req.root,
        expected: insp.presence.clone(),
        new_sha256: sha256_hex(&new_bytes),
        new_bytes,
        expires_at: req.now + req.ttl,
        digest: String::new(),
    };
    plan.digest = plan_digest(&plan);
    Ok(plan)
}

// ---- the approval FIXTURE ------------------------------------------------------------------------------------------------------------

/// A one-shot approval for exactly one plan. THIS IS A TEST FIXTURE: there is no consent screen, nothing persists it, and no production
/// code constructs one. It exists so that the prototype can prove that a commit needs a token bound to the plan's digest.
#[derive(Debug, Clone)]
pub struct Approval {
    plan_digest: String,
    nonce: u64,
    expires_at: u64,
}

pub fn test_grant(plan: &Plan, nonce: u64) -> Approval {
    Approval { plan_digest: plan.digest.clone(), nonce, expires_at: plan.expires_at }
}

#[derive(Debug, Default)]
pub struct Ledger {
    used: HashSet<u64>,
}

/// Checks and CONSUMES the approval. A commit that fails afterwards needs a new plan and a new approval: nothing is retried silently.
pub fn authorize(plan: &Plan, approval: Option<&Approval>, ledger: &mut Ledger, now: u64) -> Result<(), Refusal> {
    let a = approval.ok_or(Refusal::ApprovalMissing)?;
    if a.plan_digest != plan.digest {
        return Err(Refusal::ApprovalMismatch);
    }
    if now > plan.expires_at || now > a.expires_at {
        return Err(Refusal::PlanExpired);
    }
    if !ledger.used.insert(a.nonce) {
        return Err(Refusal::ApprovalReused);
    }
    Ok(())
}

/// How far a commit got. The failure table in `docs/config-02b/CONTRACT_AND_LIMITS.md` lists, for every state, what is on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Prepared,
    Authorized,
    BackedUp,
    Staged,
    Swapped,
    Verified,
    Aborted,
    NeedsAttention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failure {
    pub refusal: Refusal,
    pub reached: State,
}

#[cfg(test)]
#[path = "config_txn_tests.rs"]
mod tests;
