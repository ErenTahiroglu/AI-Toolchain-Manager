//! Embedded, immutable, allowlisted projection of the public catalog data (`data/catalog-*.json`).
//!
//! The projection mirrors the semantics of the GUI's `src-tauri/src/catalog.rs::all_seed_entries` (nine Phase 0 tools with a fixed
//! kind/purpose table, then six wrapped candidate files, then the purpose refinements of `catalog-candidates-c02.json`). No database,
//! no file, no network: the bytes are compiled in. A parity test compares this module with the GUI source text.
//!
//! Only allowlisted, validated fields leave this module. Never emitted: install methods, dependencies, privilege and network claims,
//! artifact evidence, verification claims, privacy blocks, evidence free text, arbitrary URLs.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;

pub const SEED_JSON: &str = include_str!("../../data/catalog-seed.json");
pub const CANDIDATE_FILES: [(&str, &str); 6] = [
    ("catalog-candidates-1b.json", include_str!("../../data/catalog-candidates-1b.json")),
    ("catalog-candidates-c01.json", include_str!("../../data/catalog-candidates-c01.json")),
    ("catalog-candidates-c02.json", include_str!("../../data/catalog-candidates-c02.json")),
    ("catalog-candidates-c03.json", include_str!("../../data/catalog-candidates-c03.json")),
    ("catalog-candidates-c04.json", include_str!("../../data/catalog-candidates-c04.json")),
    ("catalog-candidates-c05.json", include_str!("../../data/catalog-candidates-c05.json")),
];

/// Kind and purpose of the nine Phase 0 tools (same table as the GUI's `NINE_META`; a parity test enforces equality).
pub const NINE_META: [(&str, &str, &str); 9] = [
    ("ponytail", "agent_plugin", "Ajan eklentisi (Ponytail). Amacı bu sürümde ayrıntılı incelenmedi."),
    ("codebase-memory-mcp", "executable_tool", "Kod tabanı için yerel bilgi grafiği sunan MCP sunucusu."),
    ("graphify", "executable_tool", "Kod ve belgelerden bilgi grafiği çıkaran komut aracı."),
    ("gitleaks", "executable_tool", "Kod depolarında sızmış gizli anahtarları arayan tarayıcı."),
    ("markitdown", "executable_tool", "Belgeleri Markdown biçimine çeviren komut aracı."),
    ("caveman", "agent_plugin", "Kısa ve az konuşan yanıt tarzı sağlayan ajan eklentisi."),
    ("cloudflare-security-audit-skill", "skill_collection", "Güvenlik denetimi için ajan yeteneği (tek yetenekli kaynak)."),
    ("superpowers", "agent_plugin", "Ajanlara iş akışı becerileri ekleyen eklenti."),
    ("betterleaks", "executable_tool", "Kod depolarında sızmış gizli anahtarları arayan tarayıcı."),
];

pub const KINDS: [&str; 4] = ["executable_tool", "agent_plugin", "skill_collection", "prompt_source"];
pub const EXECUTION_POLICIES: [&str; 2] = ["manual-only", "blocked-until-scoped-authorization"];
pub const REVIEW_STATES: [&str; 4] = ["unreviewed", "metadata-reviewed", "blocked", "quarantined"];
pub const PLATFORMS: [&str; 2] = ["windows-x64", "macos-arm64"];
pub const SURFACES: [&str; 4] = ["claude-code-terminal", "codex-cli", "antigravity-ide", "local-cli"];
pub const MAX_PURPOSE_CHARS: usize = 300;
pub const EXPECTED_ENTRY_COUNT: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compat {
    pub platform: &'static str,
    pub surface: &'static str,
    pub state: &'static str,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub display_name: String,
    pub kind: &'static str,
    pub purpose: String,
    pub review_state: &'static str,
    pub execution_policy: &'static str,
    pub license_state: &'static str,
    pub license_spdx: Option<String>,
    pub repo_owner: Option<String>,
    pub repo_name: Option<String>,
    pub repo_url: Option<String>,
    pub observed_date: Option<String>,
    pub immutable_commit: Option<String>,
    pub compat: Vec<Compat>,
    pub has_unknown_compat: bool,
    pub repo_url_withheld: bool,
    /// Lower-cased, accent-folded text used only for matching.
    pub folded: String,
}

pub struct Catalog {
    pub entries: Vec<Entry>,
    pub data_sha256: String,
}

fn pick(list: &'static [&'static str], value: &str) -> Option<&'static str> {
    list.iter().copied().find(|v| *v == value)
}

fn str_at<'a>(v: &'a Value, pointer: &str) -> &'a str {
    v.pointer(pointer).and_then(Value::as_str).unwrap_or_default()
}

/// Characters that never cross the boundary in free text: controls, bidi controls, zero-width and BOM characters.
pub fn is_forbidden_char(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}' | '\u{00AD}' | '\u{061C}')
}

/// Source text for the model: forbidden characters removed, whitespace collapsed, NFC, at most `MAX_PURPOSE_CHARS` characters.
pub fn sanitize_text(raw: &str) -> String {
    let kept: String = raw.chars().map(|c| if is_forbidden_char(c) { ' ' } else { c }).collect();
    let collapsed = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.nfc().take(MAX_PURPOSE_CHARS).collect()
}

/// Matching form: NFD, combining marks dropped, Turkish dotless/dotted i folded, lower-cased.
pub fn fold(text: &str) -> String {
    text.nfd()
        .filter(|c| !matches!(*c, '\u{0300}'..='\u{036F}'))
        .flat_map(|c| match c {
            'İ' | 'I' | 'ı' => 'i'.to_lowercase().collect::<Vec<_>>(),
            other => other.to_lowercase().collect::<Vec<_>>(),
        })
        .collect()
}

fn valid_github_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://github.com/") else { return false };
    let mut parts = rest.split('/');
    let (Some(owner), Some(name), None) = (parts.next(), parts.next(), parts.next()) else { return false };
    let owner_ok = !owner.is_empty() && owner.len() <= 39 && owner.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    let name_ok = !name.is_empty() && name.len() <= 100 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_');
    owner_ok && name_ok
}

fn valid_ident(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

fn valid_spdx(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+' | b' ' | b'(' | b')'))
}

fn valid_id(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty() && b.len() <= 64 && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit()) && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

fn build(record: &Value, kind: &str, policy: &str, purpose: &str) -> Result<Entry, String> {
    let id = str_at(record, "/toolId").to_string();
    if !valid_id(&id) {
        return Err("invalid tool id".into());
    }
    let kind = pick(&KINDS, kind).ok_or("unknown kind")?;
    let execution_policy = pick(&EXECUTION_POLICIES, policy).ok_or("unknown execution policy")?;
    // An unknown review state maps to the LOWEST state, never a better one.
    let review_state = pick(&REVIEW_STATES, str_at(record, "/reviewState")).unwrap_or("unreviewed");
    let license_state = match str_at(record, "/license/state") {
        "known" => "known",
        "review-required" => "review-required",
        _ => "unknown",
    };
    let license_spdx = record.pointer("/license/spdxId").and_then(Value::as_str).filter(|s| valid_spdx(s)).map(str::to_string);
    let owner = Some(str_at(record, "/repository/owner")).filter(|s| valid_ident(s, 39)).map(str::to_string);
    let name = Some(str_at(record, "/repository/name")).filter(|s| valid_ident(s, 100)).map(str::to_string);
    let raw_url = str_at(record, "/repository/url");
    let repo_url = Some(raw_url).filter(|u| valid_github_url(u)).map(str::to_string);
    let repo_url_withheld = repo_url.is_none();
    let observed_date = record
        .pointer("/sourceSnapshot/observedAt")
        .and_then(Value::as_str)
        .and_then(|s| s.get(..10))
        .filter(|d| {
            let b = d.as_bytes();
            b.len() == 10 && b.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() })
        })
        .map(str::to_string);
    let immutable_commit = record
        .pointer("/sourceSnapshot/immutableCommit")
        .and_then(Value::as_str)
        .filter(|c| c.len() == 40 && c.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        .map(str::to_string);
    let mut compat = Vec::new();
    let mut has_unknown_compat = false;
    let mut any_target = false;
    for t in record.get("targetEvidence").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
        any_target = true;
        let platform = pick(&PLATFORMS, str_at(t, "/platformTarget"));
        let surface = pick(&SURFACES, str_at(t, "/agentSurface"));
        // Only `vendor-declared` is a positive source claim; anything else is `unknown`.
        let state = if str_at(t, "/compatibilityState") == "vendor-declared" { "vendor-declared" } else { "unknown" };
        if state == "unknown" {
            has_unknown_compat = true;
        }
        if let (Some(platform), Some(surface)) = (platform, surface) {
            compat.push(Compat { platform, surface, state });
        } else {
            has_unknown_compat = true;
        }
    }
    if !any_target {
        has_unknown_compat = true;
    }
    compat.sort_by(|a, b| (a.platform, a.surface).cmp(&(b.platform, b.surface)));
    let display_name = sanitize_text(str_at(record, "/displayName"));
    let purpose = sanitize_text(purpose);
    let folded = fold(&format!("{id}\n{display_name}\n{purpose}"));
    Ok(Entry {
        id,
        display_name,
        kind,
        purpose,
        review_state,
        execution_policy,
        license_state,
        license_spdx,
        repo_owner: owner,
        repo_name: name,
        repo_url,
        observed_date,
        immutable_commit,
        compat,
        has_unknown_compat,
        repo_url_withheld,
        folded,
    })
}

fn load() -> Result<Catalog, String> {
    let mut entries: Vec<Entry> = Vec::new();
    let seed: Value = serde_json::from_str(SEED_JSON).map_err(|_| "seed json")?;
    let records = seed.get("records").and_then(Value::as_array).ok_or("seed records")?;
    if records.len() != NINE_META.len() {
        return Err("seed count".into());
    }
    for record in records {
        let id = str_at(record, "/toolId");
        let (_, kind, purpose) = NINE_META.iter().find(|(t, _, _)| *t == id).ok_or("unclassified seed tool")?;
        entries.push(build(record, kind, "manual-only", purpose)?);
    }
    let mut refinements: Vec<(String, String)> = Vec::new();
    for (file, raw) in CANDIDATE_FILES {
        let root: Value = serde_json::from_str(raw).map_err(|_| format!("{file} json"))?;
        for wrapper in root.get("records").and_then(Value::as_array).ok_or("candidate records")? {
            let record = wrapper.get("record").ok_or("candidate record")?;
            let purpose = str_at(wrapper, "/purpose");
            if purpose.is_empty() {
                return Err("candidate purpose".into());
            }
            entries.push(build(record, str_at(wrapper, "/entryKind"), str_at(wrapper, "/executionPolicy"), purpose)?);
        }
        for r in root.get("purposeRefinements").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
            refinements.push((str_at(r, "/toolId").to_string(), str_at(r, "/purpose").to_string()));
        }
    }
    for (id, purpose) in refinements {
        if purpose.is_empty() || purpose.chars().count() > MAX_PURPOSE_CHARS {
            return Err("refinement purpose".into());
        }
        let e = entries.iter_mut().find(|e| e.id == id).ok_or("refinement target")?;
        e.purpose = sanitize_text(&purpose);
        e.folded = fold(&format!("{}\n{}\n{}", e.id, e.display_name, e.purpose));
    }
    let mut seen = std::collections::BTreeSet::new();
    for e in &entries {
        if !seen.insert(e.id.clone()) {
            return Err("duplicate id".into());
        }
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let mut hasher = Sha256::new();
    for (name, raw) in std::iter::once(("catalog-seed.json", SEED_JSON)).chain(CANDIDATE_FILES) {
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update((raw.len() as u64).to_le_bytes());
        hasher.update(raw.as_bytes());
    }
    let digest = hasher.finalize();
    let data_sha256 = digest.iter().map(|b| format!("{b:02x}")).collect();
    Ok(Catalog { entries, data_sha256 })
}

/// The catalog, built once. A build-time data error is a startup failure, never a partial catalog.
pub fn catalog() -> Result<&'static Catalog, &'static str> {
    static CELL: OnceLock<Result<Catalog, String>> = OnceLock::new();
    match CELL.get_or_init(load) {
        Ok(c) => Ok(c),
        Err(_) => Err("catalog data invalid"),
    }
}
