//! CONFIG-02C (Issue #40): the typed, closed result of reading ONE project's `.mcp.json` on explicit user request.
//!
//! Authorization record: Issue #40, owner decisions D1-D9 (recorded by the independent reviewer from the owner's chat reply). The contract is
//! `docs/config-02c/READ_ONLY_CONTRACT.md`. In short: one typed command (`read_project_mcp_config`), a user-typed absolute project folder per
//! call, the constant file `.mcp.json` directly in it, at most 1 MiB, handle-relative no-follow access, a strict local parse used only to decide
//! whether a summary is defensible, and a response made ONLY of constants, closed enum values, the caller's own alias, a size and a SHA-256.
//!
//! What never crosses this module's boundary: the file's text or any piece of it, any path (the typed one included), any server name other than
//! the reviewed one (which is never echoed -- only a closed enum about it is), any command, URL, header, environment key or value, any parser
//! message, any operating-system error text. Nothing is written, logged, cached or persisted; no connection is opened; no process is started.
//!
//! The existing inventory scan (`discovery`) is a different reader with different semantics and is NOT called, changed or merged with this one.
//! This module names no catalog state: it cannot change a review state, a verification state, a preference or an installation.
use crate::{
    discovery::fsx,
    error::{AppError, AppResult},
    managed_fs::{
        read_mcp::{self, Failure, Fault, Stage},
        FsError,
    },
};
use serde::{
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
    Deserialize, Serialize,
};
use std::{
    cell::RefCell,
    collections::HashSet,
    fmt,
    path::{Component, Path, PathBuf},
};

pub const SCHEMA: &str = "config02c.read.v1";
pub const FILE: &str = ".mcp.json";
pub const SCOPE: &str = "project";
pub const SURFACE: &str = "claude-code-terminal";
pub const CONSENT: &str = "config02c-read-once-v1";
pub const MAX_BYTES: u64 = read_mcp::MAX_BYTES;
pub const MAX_DEPTH: usize = 64;
pub const MAX_SERVERS: u32 = 256;
pub const NOT_CLAIMS: [&str; 6] = ["not_loaded", "not_trusted", "not_installed", "not_enabled", "not_reachable", "not_verified"];

/// The reviewed declaration this reader compares with (a SYNTAX reference mirrored from the CONFIG-02A declaration; not a verified server).
pub const REVIEWED_ID: &str = "exa-mcp-server";
const REVIEWED_NAME: &str = "exa";
const REVIEWED_ENDPOINT: &str = "https://mcp.exa.ai/mcp";

/// The closed set of outcomes. `not_requested` is a user-interface state only and is deliberately not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    RootNotFound,
    Absent,
    PresentSummarized,
    PresentMetadataOnly,
    Malformed,
    Denied,
    UnsafePath,
    UnsupportedFs,
    TooLarge,
    ChangedDuringRead,
    IoError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    DuplicateKey,
    ByteOrderMark,
    RootNotObject,
    ServersNotObject,
    TooDeep,
    TooManyServers,
    InvalidUtf8,
    InvalidJson,
    LoneSurrogate,
    EmptyFile,
    AccessDenied,
    Locked,
    Link,
    NotADirectory,
    NotRegularFile,
    HardLinked,
    CaseAlias,
    InvalidRootText,
    RootTooBroad,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NameState {
    Absent,
    Present,
    PresentOtherLetterCase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointState {
    Absent,
    PresentUnderThisName,
    PresentUnderOtherName,
    NotEvaluable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationState {
    pub id: &'static str,
    pub name_state: NameState,
    pub endpoint_state: EndpointState,
}

/// The whole request. It names no file, no surface, no platform and no path below the root. (No `Debug`: the typed folder must not be printable by accident.)
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReadMcpConfigRequest {
    pub alias: String,
    pub directory: String,
    pub consent: String,
}

/// The whole response: constants, closed enum values, the caller's alias, a size, a SHA-256, a count. Nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadMcpConfigResult {
    pub schema: &'static str,
    pub status: Status,
    pub reason: Option<Reason>,
    pub alias: String,
    pub file: &'static str,
    pub scope: &'static str,
    pub surface: &'static str,
    pub max_bytes: u64,
    pub size_bytes: Option<u64>,
    pub sha256: Option<String>,
    pub server_count: Option<u32>,
    pub reviewed_declarations: Option<Vec<DeclarationState>>,
    pub not_claims: [&'static str; 6],
}

fn bare(alias: &str, status: Status, reason: Option<Reason>) -> ReadMcpConfigResult {
    ReadMcpConfigResult {
        schema: SCHEMA,
        status,
        reason,
        alias: alias.to_string(),
        file: FILE,
        scope: SCOPE,
        surface: SURFACE,
        max_bytes: MAX_BYTES,
        size_bytes: None,
        sha256: None,
        server_count: None,
        reviewed_declarations: None,
        not_claims: NOT_CLAIMS,
    }
}

// ---- request checks that need no filesystem ------------------------------------------------------------------------------------------

/// Alias and consent. Both refusals happen before anything is opened. `registered_reference` is the caller's answer from the registry.
pub fn authorize(request: &ReadMcpConfigRequest, registered_reference: impl FnOnce(&str) -> bool) -> AppResult<()> {
    if request.consent != CONSENT {
        return Err(AppError::new("READ_NOT_AUTHORIZED", "Dosya okuma için açık onay alınmadı; hiçbir şey okunmadı."));
    }
    if !crate::projects::valid_alias(&request.alias) || !registered_reference(&request.alias) {
        return Err(AppError::new("PROJECT_NOT_FOUND", "Böyle bir proje kaydı yok."));
    }
    Ok(())
}

/// Text checks of the typed folder, with the same intent as `discovery::validate_project_dir` but WITHOUT touching the file system (the handle-relative
/// reader walks the chain itself). Returns the path rebuilt from its components, or the closed status to report.
pub fn root_from_text(raw: &str) -> Result<PathBuf, (Status, Option<Reason>)> {
    let invalid = (Status::UnsafePath, Some(Reason::InvalidRootText));
    let text = raw.trim();
    if text.is_empty() || text.chars().count() > 260 || text.chars().any(|c| c.is_control()) {
        return Err(invalid);
    }
    let path = Path::new(text);
    if fsx::is_network_or_device_path(path) {
        return Err((Status::UnsupportedFs, None));
    }
    if cfg!(windows) && (text.starts_with("\\\\") || text.starts_with("//")) {
        return Err(invalid); // verbatim (`\\?\`) spellings are refused: type the plain drive-letter path
    }
    if text.split(['/', '\\']).any(|segment| segment == ".." || segment == ".") || !path.is_absolute() {
        return Err(invalid);
    }
    let mut rebuilt = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => rebuilt.push(component.as_os_str()),
            Component::CurDir | Component::ParentDir => return Err(invalid),
        }
    }
    if rebuilt.parent().is_none() {
        return Err((Status::UnsafePath, Some(Reason::RootTooBroad)));
    }
    Ok(rebuilt)
}

// ---- the total mapping of the file-system vocabulary (contract section 3.3) ------------------------------------------------------------------

pub fn status_of_failure(f: Failure) -> (Status, Option<Reason>) {
    use Status::*;
    match (f.stage, f.fault) {
        (_, Fault::Absent) => (Absent, None),
        (_, Fault::HardLinked) => (UnsafePath, Some(Reason::HardLinked)),
        (_, Fault::CaseAlias) => (UnsafePath, Some(Reason::CaseAlias)),
        (Stage::Root, Fault::Fs(FsError::NotFound)) => (RootNotFound, None),
        (Stage::Root, Fault::Fs(FsError::NotDir)) => (UnsafePath, Some(Reason::NotADirectory)),
        (Stage::Root, Fault::Fs(FsError::InvalidName)) => (UnsafePath, Some(Reason::InvalidRootText)),
        (_, Fault::Fs(FsError::Link)) => (UnsafePath, Some(Reason::Link)),
        (_, Fault::Fs(FsError::NotDir)) => (UnsafePath, Some(Reason::NotADirectory)),
        (_, Fault::Fs(FsError::NotFile)) => (UnsafePath, Some(Reason::NotRegularFile)),
        (_, Fault::Fs(FsError::CrossDevice | FsError::Network | FsError::Unsupported)) => (UnsupportedFs, None),
        (_, Fault::Fs(FsError::Denied)) => (Denied, Some(Reason::AccessDenied)),
        (_, Fault::Fs(FsError::Locked)) => (Denied, Some(Reason::Locked)),
        (Stage::Read, Fault::Fs(FsError::TooLarge)) => (TooLarge, None),
        (_, Fault::Fs(FsError::Changed)) => (ChangedDuringRead, None),
        // the entry or the root disappeared after it had been seen: not "absent", a change during the read
        (Stage::Open | Stage::Read | Stage::Listing, Fault::Fs(FsError::NotFound)) => (ChangedDuringRead, None),
        (_, Fault::Fs(_)) => (IoError, None),
    }
}

// ---- reviewed-declaration comparison (a pure text rule; mirrors the 02B `url_identity`, re-implemented and tested here) ------------------

/// protocol + host + path of an http(s) URL, lower-cased where case does not matter and without port 80/443, query, fragment, user info or
/// trailing slashes. `None` for anything that is not an http(s) URL. Used only to compare; the result is never returned.
pub fn url_identity(url: &str) -> Option<String> {
    let lower = url.trim();
    let (scheme, rest) = if lower.len() >= 8 && lower.is_char_boundary(8) && lower[..8].eq_ignore_ascii_case("https://") {
        ("https", &lower[8..])
    } else if lower.len() >= 7 && lower.is_char_boundary(7) && lower[..7].eq_ignore_ascii_case("http://") {
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

fn is_reviewed_endpoint(url: &str) -> bool {
    url_identity(url).as_deref() == Some(REVIEWED_ENDPOINT)
}

// ---- the strict local parse: used ONLY to decide whether a summary is defensible; it is not the parser of Claude Code ----------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum NameClass {
    Reviewed,
    OtherLetterCase,
    Other,
}

impl NameClass {
    fn of(key: &str) -> NameClass {
        if key == REVIEWED_NAME {
            NameClass::Reviewed
        } else if key.eq_ignore_ascii_case(REVIEWED_NAME) {
            NameClass::OtherLetterCase
        } else {
            NameClass::Other
        }
    }

    fn like_reviewed(self) -> bool {
        self != NameClass::Other
    }
}

#[derive(Clone, Copy)]
enum Role {
    Root,
    ServersMap,
    Server(NameClass),
    Url,
    Plain,
}

/// Booleans and a counter only. No string of the file is kept: names and URLs are compared inside the visitor and dropped.
#[derive(Default)]
struct Facts {
    duplicate_key: bool,
    too_deep: bool,
    root_not_object: bool,
    servers_not_object: bool,
    seen_servers: bool,
    server_count: u32,
    name_reviewed: bool,
    name_other_case: bool,
    endpoint_under_reviewed: bool,
    endpoint_under_other: bool,
    reviewed_like_not_evaluable: bool,
}

enum Out {
    /// A string, and whether it is the reviewed endpoint.
    Text(bool),
    Other,
}

struct Node<'a> {
    role: Role,
    depth: usize,
    facts: &'a RefCell<Facts>,
}

impl<'a> Node<'a> {
    fn child(&self, role: Role) -> Node<'a> {
        Node { role, depth: self.depth + 1, facts: self.facts }
    }

    /// A value that is not an object/array arrived where a role expected more.
    fn not_a_container(&self) {
        let mut f = self.facts.borrow_mut();
        match self.role {
            Role::Root => f.root_not_object = true,
            Role::ServersMap => f.servers_not_object = true,
            Role::Server(class) if class.like_reviewed() => f.reviewed_like_not_evaluable = true,
            _ => {}
        }
    }

    fn entered(&self) {
        if self.depth > MAX_DEPTH {
            self.facts.borrow_mut().too_deep = true;
        }
    }
}

impl<'de, 'a> DeserializeSeed<'de> for Node<'a> {
    type Value = Out;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Out, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de, 'a> Visitor<'de> for Node<'a> {
    type Value = Out;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Out, E> {
        self.not_a_container();
        Ok(Out::Other)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Out, E> {
        self.not_a_container();
        Ok(Out::Other)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Out, E> {
        self.not_a_container();
        Ok(Out::Other)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Out, E> {
        self.not_a_container();
        Ok(Out::Other)
    }
    fn visit_unit<E: de::Error>(self) -> Result<Out, E> {
        self.not_a_container();
        Ok(Out::Other)
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Out, E> {
        if matches!(self.role, Role::Url) {
            return Ok(Out::Text(is_reviewed_endpoint(v)));
        }
        self.not_a_container();
        Ok(Out::Other)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Out, A::Error> {
        self.entered();
        self.not_a_container();
        while seq.next_element_seed(self.child(Role::Plain))?.is_some() {}
        Ok(Out::Other)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Out, A::Error> {
        self.entered();
        let mut keys: HashSet<String> = HashSet::new();
        // what the url member of a reviewed-like server looked like: None = no url member, Some(None) = not a string, Some(Some(m)) = string (m: the endpoint)
        let mut url_member: Option<Option<bool>> = None;
        while let Some(key) = map.next_key::<String>()? {
            let fresh = keys.insert(key.clone());
            if !fresh {
                self.facts.borrow_mut().duplicate_key = true;
            }
            let role = match (self.role, key.as_str()) {
                (Role::Root, "mcpServers") if fresh => {
                    self.facts.borrow_mut().seen_servers = true;
                    Role::ServersMap
                }
                (Role::ServersMap, _) => {
                    let class = NameClass::of(&key);
                    let mut f = self.facts.borrow_mut();
                    f.server_count = f.server_count.saturating_add(u32::from(fresh));
                    match class {
                        NameClass::Reviewed => f.name_reviewed = true,
                        NameClass::OtherLetterCase => f.name_other_case = true,
                        NameClass::Other => {}
                    }
                    Role::Server(class)
                }
                (Role::Server(_), "url") if fresh => Role::Url,
                _ => Role::Plain,
            };
            let out = map.next_value_seed(self.child(role))?;
            if matches!(role, Role::Url) {
                url_member = Some(match out {
                    Out::Text(m) => Some(m),
                    Out::Other => None,
                });
            }
        }
        if let Role::Server(class) = self.role {
            let mut f = self.facts.borrow_mut();
            match url_member {
                Some(Some(true)) => {
                    if class == NameClass::Reviewed {
                        f.endpoint_under_reviewed = true;
                    } else {
                        f.endpoint_under_other = true;
                    }
                }
                Some(None) if class.like_reviewed() => f.reviewed_like_not_evaluable = true,
                _ => {}
            }
        }
        Ok(Out::Other)
    }
}

/// What the bytes are, by this application's local policy. Never carries file text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classified {
    Summarized { server_count: u32, declaration: DeclarationState },
    MetadataOnly(Reason),
    Malformed(Reason),
}

pub fn classify(bytes: &[u8]) -> Classified {
    if bytes.is_empty() {
        return Classified::Malformed(Reason::EmptyFile);
    }
    let Ok(text) = std::str::from_utf8(bytes) else { return Classified::Malformed(Reason::InvalidUtf8) };
    let (text, bom) = match text.strip_prefix('\u{feff}') {
        Some(rest) => (rest, true),
        None => (text, false),
    };
    let facts = RefCell::new(Facts::default());
    let mut parser = serde_json::Deserializer::from_str(text);
    let parsed = Node { role: Role::Root, depth: 1, facts: &facts }.deserialize(&mut parser).and_then(|_| parser.end());
    let f = facts.into_inner();
    if let Err(e) = parsed {
        // the message is inspected here and dropped; it is never returned or logged
        let message = e.to_string();
        return if f.too_deep && message.contains("recursion limit") {
            Classified::MetadataOnly(Reason::TooDeep)
        } else if message.contains("surrogate") || message.contains("hex escape") {
            Classified::Malformed(Reason::LoneSurrogate)
        } else {
            Classified::Malformed(Reason::InvalidJson)
        };
    }
    let reason = if f.duplicate_key {
        Some(Reason::DuplicateKey)
    } else if bom {
        Some(Reason::ByteOrderMark)
    } else if f.root_not_object {
        Some(Reason::RootNotObject)
    } else if f.servers_not_object {
        Some(Reason::ServersNotObject)
    } else if f.too_deep {
        Some(Reason::TooDeep)
    } else if f.server_count > MAX_SERVERS {
        Some(Reason::TooManyServers)
    } else {
        None
    };
    if let Some(r) = reason {
        return Classified::MetadataOnly(r);
    }
    let name_state = if f.name_reviewed {
        NameState::Present
    } else if f.name_other_case {
        NameState::PresentOtherLetterCase
    } else {
        NameState::Absent
    };
    let endpoint_state = if f.endpoint_under_reviewed {
        EndpointState::PresentUnderThisName
    } else if f.endpoint_under_other {
        EndpointState::PresentUnderOtherName
    } else if f.reviewed_like_not_evaluable {
        EndpointState::NotEvaluable
    } else {
        EndpointState::Absent
    };
    Classified::Summarized { server_count: f.server_count, declaration: DeclarationState { id: REVIEWED_ID, name_state, endpoint_state } }
}

// ---- the one read -----------------------------------------------------------------------------------------------------------------------

/// Builds the response from what the reader returned. Pure: no file access.
pub fn result_from(alias: &str, outcome: Result<read_mcp::Observed, Failure>) -> ReadMcpConfigResult {
    match outcome {
        Err(f) => {
            let (status, reason) = status_of_failure(f);
            bare(alias, status, reason)
        }
        Ok(seen) => {
            let (status, reason, summary) = match classify(&seen.bytes) {
                Classified::Summarized { server_count, declaration } => {
                    (Status::PresentSummarized, None, Some((server_count, declaration)))
                }
                Classified::MetadataOnly(r) => (Status::PresentMetadataOnly, Some(r), None),
                Classified::Malformed(r) => (Status::Malformed, Some(r), None),
            };
            let mut out = bare(alias, status, reason);
            out.size_bytes = Some(seen.size);
            out.sha256 = Some(seen.sha256);
            if let Some((count, declaration)) = summary {
                out.server_count = Some(count);
                out.reviewed_declarations = Some(vec![declaration]);
            }
            out
        }
    }
}

/// One click, one read. The caller has already passed `authorize`. Nothing is retained when this returns.
pub fn read_once(alias: &str, directory: &str) -> ReadMcpConfigResult {
    match root_from_text(directory) {
        Err((status, reason)) => bare(alias, status, reason),
        Ok(root) => result_from(alias, read_mcp::read(&root)),
    }
}

/// The whole command body: authorization first (nothing is opened if it fails), then one read. `registered_reference` answers from the registry.
pub fn handle(request: &ReadMcpConfigRequest, registered_reference: impl FnOnce(&str) -> bool) -> AppResult<ReadMcpConfigResult> {
    authorize(request, registered_reference)?;
    Ok(read_once(&request.alias, &request.directory))
}

#[cfg(test)]
#[path = "mcp_read_tests.rs"]
mod tests;
