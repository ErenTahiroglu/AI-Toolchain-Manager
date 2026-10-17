//! Dynamic catalog registry (Phase 1B). Registering a repository is *metadata only*: nothing is
//! downloaded, executed, installed or verified. Install/verification state cannot be anything but
//! `not_managed` / `unknown` (SQL CHECK constraints), whatever the UI or the caller asks for.

use crate::{
    audit::{canonical_json, now_utc, sha256_hex, NewEvent},
    diagnostics,
    error::{AppError, AppResult},
    github_url::{self, ParsedRepo},
};
use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::Serialize;
use serde_json::{json, Value};

/// Phase 0 seed (nine tools) — single source of truth shared with the Phase 0 contract tests.
const SEED_JSON: &str = include_str!("../../data/catalog-seed.json");
/// Phase 1B candidates (four entries) — same record schema plus application wrapper fields.
const CANDIDATES_JSON: &str = include_str!("../../data/catalog-candidates-1b.json");
pub const PHASE0_TOOL_COUNT: usize = 9;
pub const CANDIDATE_COUNT: usize = 4;
pub const SEED_ENTRY_COUNT: usize = PHASE0_TOOL_COUNT + CANDIDATE_COUNT;

pub const KINDS: &[&str] = &["executable_tool", "agent_plugin", "skill_collection", "prompt_source"];
const EXECUTION_POLICIES: &[&str] = &["manual-only", "blocked-until-scoped-authorization"];
const MAX_PURPOSE_CHARS: usize = 300;

/// Classification and short purpose of the nine Phase 0 tools. Text is deliberately modest: where
/// the purpose was not reviewed, it says so instead of guessing.
const NINE_META: &[(&str, &str, &str)] = &[
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

fn seed_error(why: &str) -> AppError {
    AppError::new("SEED_INVALID", format!("Katalog tohum verisi geçersiz: {why}"))
}

// ---------------------------------------------------------------------------------------------
// Seed parsing
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SeedEntry {
    entry_id: String,
    kind: &'static str,
    display_name: String,
    purpose: String,
    source_key: String,
    repo_url: String,
    source_path: Option<String>,
    plugin_identity: Option<String>,
    review_state: String,
    execution_policy: &'static str,
    license_state: String,
    license_spdx: Option<String>,
    enabled_default: bool,
    revision: String,
    fingerprint: String,
    record: Value,
}

fn static_kind(kind: &str) -> AppResult<&'static str> {
    KINDS.iter().copied().find(|k| *k == kind).ok_or_else(|| seed_error("bilinmeyen tür"))
}

fn static_policy(policy: &str) -> AppResult<&'static str> {
    EXECUTION_POLICIES.iter().copied().find(|p| *p == policy).ok_or_else(|| seed_error("bilinmeyen çalıştırma ilkesi"))
}

fn str_at<'a>(v: &'a Value, pointer: &str) -> &'a str {
    v.pointer(pointer).and_then(Value::as_str).unwrap_or_default()
}

/// Changes only when what the tool can do or what it needs changes. A patch release or a new
/// commit does NOT change it, so a rejected suggestion is not repeated just because the version moved.
pub fn capability_fingerprint(record: &Value, kind: &str, execution_policy: &str) -> String {
    let targets: Vec<Value> = record
        .get("targetEvidence")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|t| json!([t.get("platformTarget"), t.get("agentSurface"), t.get("compatibilityState")])).collect())
        .unwrap_or_default();
    let methods: Vec<Value> = record
        .get("installMethods")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|m| m.get("methodId").cloned().unwrap_or(Value::Null)).collect())
        .unwrap_or_default();
    let material = json!({
        "kind": kind,
        "category": record.get("category"),
        "executionPolicy": execution_policy,
        "targets": targets,
        "methods": methods,
        "privilege": record.get("privilegeClaims"),
        "network": record.get("networkClaims"),
        "telemetry": record.pointer("/privacy/telemetry"),
        "transmits": record.pointer("/privacy/mayTransmitProjectContent"),
    });
    sha256_hex(&canonical_json(&material))
}

fn revision_of(record: &Value) -> String {
    let label = record
        .pointer("/sourceSnapshot/releaseVersion")
        .and_then(Value::as_str)
        .or_else(|| record.pointer("/sourceSnapshot/observedRef").and_then(Value::as_str))
        .unwrap_or("unknown");
    let commit =
        record.pointer("/sourceSnapshot/immutableCommit").and_then(Value::as_str).map(|c| &c[..c.len().min(12)]).unwrap_or("unpinned");
    format!("{label}@{commit}")
}

fn source_key_of(record: &Value, source_path: Option<&str>) -> String {
    let base = format!("{}/{}", str_at(record, "/repository/owner"), str_at(record, "/repository/name")).to_ascii_lowercase();
    match source_path {
        Some(path) => format!("{base}#{path}"),
        None => base,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_seed(
    record: &Value,
    kind: &'static str,
    purpose: &str,
    policy: &'static str,
    source_path: Option<String>,
    plugin_identity: Option<String>,
    enabled_default: bool,
) -> AppResult<SeedEntry> {
    let entry_id = str_at(record, "/toolId").to_string();
    if entry_id.is_empty() {
        return Err(seed_error("toolId yok"));
    }
    let review_state = str_at(record, "/reviewState").to_string();
    if !matches!(review_state.as_str(), "unreviewed" | "metadata-reviewed" | "blocked" | "quarantined") {
        return Err(seed_error("geçersiz reviewState"));
    }
    Ok(SeedEntry {
        source_key: source_key_of(record, source_path.as_deref()),
        entry_id,
        kind,
        display_name: str_at(record, "/displayName").to_string(),
        purpose: purpose.to_string(),
        repo_url: str_at(record, "/repository/url").to_string(),
        source_path,
        plugin_identity,
        review_state,
        execution_policy: policy,
        license_state: str_at(record, "/license/state").to_string(),
        license_spdx: record.pointer("/license/spdxId").and_then(Value::as_str).map(str::to_string),
        enabled_default,
        revision: revision_of(record),
        fingerprint: capability_fingerprint(record, kind, policy),
        record: record.clone(),
    })
}

/// The nine Phase 0 records. Every one must stay `unreviewed`; nothing here may claim a review.
pub fn parse_seed() -> AppResult<Vec<Value>> {
    let root: Value = serde_json::from_str(SEED_JSON).map_err(|_| seed_error("JSON okunamadı"))?;
    let records = root.get("records").and_then(Value::as_array).ok_or_else(|| seed_error("records yok"))?;
    if records.len() != PHASE0_TOOL_COUNT {
        return Err(seed_error("dokuz kayıt bekleniyordu"));
    }
    for record in records {
        if record.get("reviewState").and_then(Value::as_str) != Some("unreviewed") {
            return Err(seed_error("tüm kayıtlar 'unreviewed' olmalı"));
        }
    }
    Ok(records.clone())
}

fn nine_seed_entries() -> AppResult<Vec<SeedEntry>> {
    parse_seed()?
        .iter()
        .map(|record| {
            let id = str_at(record, "/toolId");
            let (_, kind, purpose) =
                NINE_META.iter().find(|(tool, _, _)| *tool == id).ok_or_else(|| seed_error("sınıflandırması olmayan araç"))?;
            build_seed(record, static_kind(kind)?, purpose, "manual-only", None, None, true)
        })
        .collect()
}

fn candidate_seed_entries() -> AppResult<Vec<SeedEntry>> {
    let root: Value = serde_json::from_str(CANDIDATES_JSON).map_err(|_| seed_error("aday JSON okunamadı"))?;
    let records = root.get("records").and_then(Value::as_array).ok_or_else(|| seed_error("aday records yok"))?;
    if records.len() != CANDIDATE_COUNT {
        return Err(seed_error("dört aday bekleniyordu"));
    }
    records
        .iter()
        .map(|wrapper| {
            let record = wrapper.get("record").ok_or_else(|| seed_error("record yok"))?;
            let kind = static_kind(str_at(wrapper, "/entryKind"))?;
            let policy = static_policy(str_at(wrapper, "/executionPolicy"))?;
            let purpose = str_at(wrapper, "/purpose");
            if purpose.is_empty() {
                return Err(seed_error("amaç yok"));
            }
            let opt = |k: &str| wrapper.get(k).and_then(Value::as_str).map(str::to_string);
            // "enabled" only means the entry takes part in the catalog UI and informational suggestions. A quarantined
            // candidate (Strix) is therefore visible, but its execution policy still blocks every run and acceptance.
            build_seed(record, kind, purpose, policy, opt("sourcePath"), opt("pluginIdentity"), true)
        })
        .collect()
}

fn all_seed_entries() -> AppResult<Vec<SeedEntry>> {
    let mut all = nine_seed_entries()?;
    all.extend(candidate_seed_entries()?);
    let mut ids = std::collections::BTreeSet::new();
    let mut keys = std::collections::BTreeSet::new();
    for e in &all {
        if !ids.insert(e.entry_id.clone()) || !keys.insert(e.source_key.clone()) {
            return Err(seed_error("yinelenen kimlik veya kaynak"));
        }
    }
    Ok(all)
}

// ---------------------------------------------------------------------------------------------
// Prompt sources and synthetic sample data (unchanged Phase 1A content)
// ---------------------------------------------------------------------------------------------

/// (id, name, url, license note, planned format, notes)
const PROMPT_SOURCES: &[(&str, &str, &str, &str, &str, &str)] = &[
    (
        "prompts-chat",
        "prompts.chat",
        "https://github.com/f/prompts.chat",
        "Kaynak README'ye göre istem verisi CC0, web sitesi kodu MIT. Henüz doğrulanmadı.",
        "Salt okunur CSV / Markdown anlık görüntüleri",
        "İçerik kalitesi değişkendir. Bu aşamada hiçbir şey indirilmez veya aranmaz.",
    ),
    (
        "the-prompt-library",
        "the-prompt-library",
        "https://github.com/JuliusBrussee/the-prompt-library",
        "Depo MIT. Üçüncü taraf sistem istemi kopyaları, hakları doğrulanmadıkça dışarıda bırakılacak.",
        "Salt okunur, doğrulanmış YAML istem kayıtları",
        "Bu aşamada hiçbir şey indirilmez veya aranmaz.",
    ),
    (
        "fabric",
        "Fabric",
        "https://github.com/danielmiessler/Fabric",
        "Depo MIT. Fabric çalışma zamanı, sağlayıcılar ve API anahtarları gerekmez.",
        "Salt okunur data/patterns metin anlık görüntüleri",
        "Bu aşamada hiçbir şey indirilmez veya aranmaz.",
    ),
];

/// Synthetic examples only: (alias, display name)
const SAMPLE_PROJECTS: &[(&str, &str)] = &[
    ("ornek-web-sitesi", "Örnek: Web sitesi"),
    ("ornek-veri-analizi", "Örnek: Veri analizi"),
    ("ornek-mobil-uygulama", "Örnek: Mobil uygulama"),
];

/// (project alias, tool id, agent surface, platform target, decision) — demonstration decisions only.
const SAMPLE_DECISIONS: &[(&str, &str, &str, &str, &str)] = &[
    ("ornek-web-sitesi", "gitleaks", "local-cli", "windows-x64", "accepted"),
    ("ornek-web-sitesi", "caveman", "claude-code-terminal", "windows-x64", "postponed"),
    ("ornek-veri-analizi", "markitdown", "local-cli", "windows-x64", "accepted"),
    ("ornek-veri-analizi", "graphify", "local-cli", "windows-x64", "rejected"),
    ("ornek-mobil-uygulama", "superpowers", "claude-code-terminal", "macos-arm64", "disabled"),
    ("ornek-mobil-uygulama", "codebase-memory-mcp", "claude-code-terminal", "macos-arm64", "accepted"),
];

// ---------------------------------------------------------------------------------------------
// Writing entries
// ---------------------------------------------------------------------------------------------

fn insert_seed_row(tx: &Transaction, e: &SeedEntry, added_at: &str, enabled: bool) -> AppResult<()> {
    tx.execute(
        "INSERT INTO catalog_entries
         (entry_id, kind, origin, display_name, purpose, source_key, repo_url, source_path, plugin_identity, review_state,
          metadata_state, execution_policy, license_state, license_spdx, enabled, revision, capability_fingerprint,
          record_json, added_at, updated_at)
         VALUES (?1, ?2, 'seed', ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'documented-snapshot', ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?17)",
        rusqlite::params![
            e.entry_id,
            e.kind,
            e.display_name,
            e.purpose,
            e.source_key,
            e.repo_url,
            e.source_path,
            e.plugin_identity,
            e.review_state,
            e.execution_policy,
            e.license_state,
            e.license_spdx,
            enabled as i64,
            e.revision,
            e.fingerprint,
            e.record.to_string(),
            added_at
        ],
    )?;
    Ok(())
}

/// Copy Phase 1A data into the Phase 1B tables, then freeze the legacy tables. Runs inside the
/// migration transaction: any failure rolls everything back and the old database stays as it was.
pub fn migrate_phase1a_data(tx: &Transaction) -> AppResult<()> {
    let now = now_utc();
    let legacy: Vec<(String, String, String)> = {
        let mut stmt = tx.prepare("SELECT tool_id, record_json, updated_at FROM tools ORDER BY tool_id")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for (tool_id, record_json, updated_at) in &legacy {
        let record: Value = serde_json::from_str(record_json)?;
        let meta = NINE_META.iter().find(|(id, _, _)| id == tool_id);
        let (kind, purpose) = match meta {
            Some((_, kind, purpose)) => (static_kind(kind)?, *purpose),
            None => (static_kind("executable_tool")?, "Amaç henüz yazılmadı."),
        };
        let entry = build_seed(&record, kind, purpose, "manual-only", None, None, true)?;
        insert_seed_row(tx, &entry, updated_at, true)?;
    }

    let projects: Vec<(i64, String, String, String)> = {
        let mut stmt = tx.prepare("SELECT id, alias, display_name, created_at FROM projects ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for (old_id, alias, display_name, created_at) in &projects {
        let project_id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO project_registry (project_id, alias, display_name, kind, local_note, created_at) VALUES (?1, ?2, ?3, 'synthetic', '', ?4)",
            rusqlite::params![project_id, alias, display_name, created_at],
        )?;
        let decisions: Vec<(String, String, String, String)> = {
            let mut stmt =
                tx.prepare("SELECT tool_id, agent_surface, platform_target, decision FROM project_tool_decisions WHERE project_id = ?1")?;
            let rows = stmt.query_map([old_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (tool_id, surface, platform, old_decision) in decisions {
            // Phase 1A called "postponed" by the name "later".
            let decision = if old_decision == "later" { "postponed".to_string() } else { old_decision };
            let (revision, fingerprint): (String, String) =
                tx.query_row("SELECT revision, capability_fingerprint FROM catalog_entries WHERE entry_id = ?1", [&tool_id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
            tx.execute(
                "INSERT INTO project_preferences
                 (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![project_id, tool_id, surface, platform, decision, created_at, revision, fingerprint],
            )?;
            tx.execute(
                "INSERT INTO preference_history (project_id, entry_id, agent_surface, platform_target, action, at, catalog_revision)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![project_id, tool_id, surface, platform, decision, created_at, revision],
            )?;
        }
    }

    // The legacy tables keep their data (nothing is deleted) but can no longer diverge from the registry.
    for table in ["tools", "projects", "project_tool_decisions"] {
        for op in ["INSERT", "UPDATE", "DELETE"] {
            tx.execute_batch(&format!(
                "CREATE TRIGGER {table}_frozen_{lop} BEFORE {op} ON {table}
                 BEGIN SELECT RAISE(ABORT, '{table} is frozen since schema v2; use the Phase 1B registry tables'); END;",
                lop = op.to_ascii_lowercase()
            ))?;
        }
    }
    if !legacy.is_empty() || !projects.is_empty() {
        tx.execute(
            "INSERT INTO diagnostics (ts, level, component, code, detail) VALUES (?1, 'info', 'migration', 'DB_MIGRATED_V2', ?2)",
            rusqlite::params![now, format!("Faz 1A verisi taşındı: {} araç, {} proje.", legacy.len(), projects.len())],
        )?;
    }
    Ok(())
}

pub fn seed_all(conn: &Connection) -> AppResult<()> {
    let entries = all_seed_entries()?;
    let now = now_utc();
    let tx = conn.unchecked_transaction()?;
    for e in &entries {
        let exists: Option<i64> =
            tx.query_row("SELECT 1 FROM catalog_entries WHERE entry_id = ?1", [&e.entry_id], |r| r.get(0)).optional()?;
        if exists.is_none() {
            // A user-added live entry may already own this source; never fight it, and say so.
            let taken: Option<String> = tx
                .query_row("SELECT entry_id FROM catalog_entries WHERE source_key = ?1 AND removed_at IS NULL", [&e.source_key], |r| {
                    r.get(0)
                })
                .optional()?;
            if taken.is_some() {
                diagnostics::log(&tx, "warn", "catalog", "SEED_SOURCE_TAKEN", &format!("tohum kaydı atlandı: {}", e.entry_id))?;
                continue;
            }
            insert_seed_row(&tx, e, &now, e.enabled_default)?;
        } else {
            // Metadata refresh only. enabled / removed_at / added_at belong to the user and are never touched.
            tx.execute(
                "UPDATE catalog_entries SET kind = ?2, display_name = ?3, purpose = ?4, repo_url = ?5, source_path = ?6, plugin_identity = ?7,
                   review_state = ?8, execution_policy = ?9, license_state = ?10, license_spdx = ?11, revision = ?12,
                   capability_fingerprint = ?13, record_json = ?14, updated_at = ?15
                 WHERE entry_id = ?1 AND origin = 'seed' AND (record_json <> ?14 OR purpose <> ?4 OR kind <> ?2 OR capability_fingerprint <> ?13)",
                rusqlite::params![
                    e.entry_id, e.kind, e.display_name, e.purpose, e.repo_url, e.source_path, e.plugin_identity, e.review_state,
                    e.execution_policy, e.license_state, e.license_spdx, e.revision, e.fingerprint, e.record.to_string(), now
                ],
            )?;
        }
    }
    for (id, name, url, license, format, notes) in PROMPT_SOURCES {
        tx.execute(
            "INSERT OR IGNORE INTO prompt_sources
             (source_id, display_name, repo_url, license_note, planned_format, notes, trust_status, status, read_only)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'untrusted-reference', 'planned', 1)",
            rusqlite::params![id, name, url, license, format, notes],
        )?;
    }
    // Sample projects and their demonstration decisions are written once, when the project is first
    // created, so a decision the user reset is never resurrected on the next start.
    for (alias, name) in SAMPLE_PROJECTS {
        let created = tx.execute(
            "INSERT OR IGNORE INTO project_registry (project_id, alias, display_name, kind, local_note, created_at)
             VALUES (?1, ?2, ?3, 'synthetic', '', ?4)",
            rusqlite::params![uuid::Uuid::new_v4().to_string(), alias, name, now],
        )?;
        if created == 1 {
            for (_, tool, surface, platform, decision) in SAMPLE_DECISIONS.iter().filter(|d| d.0 == *alias) {
                tx.execute(
                    "INSERT OR IGNORE INTO project_preferences
                     (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint)
                     SELECT p.project_id, e.entry_id, ?3, ?4, ?5, ?6, e.revision, e.capability_fingerprint
                     FROM project_registry p, catalog_entries e WHERE p.alias = ?1 AND e.entry_id = ?2",
                    rusqlite::params![alias, tool, surface, platform, decision, now],
                )?;
                tx.execute(
                    "INSERT INTO preference_history (project_id, entry_id, agent_surface, platform_target, action, at, catalog_revision)
                     SELECT p.project_id, e.entry_id, ?3, ?4, ?5, ?6, e.revision
                     FROM project_registry p, catalog_entries e WHERE p.alias = ?1 AND e.entry_id = ?2",
                    rusqlite::params![alias, tool, surface, platform, decision, now],
                )?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Reading entries
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub entry_id: String,
    pub kind: String,
    pub origin: String,
    pub display_name: String,
    pub purpose: String,
    pub source_key: String,
    pub repo_url: String,
    pub source_path: Option<String>,
    pub plugin_identity: Option<String>,
    pub review_state: String,
    pub metadata_state: String,
    pub execution_policy: String,
    pub license_state: String,
    pub license_spdx: Option<String>,
    pub enabled: bool,
    pub install_state: String,
    pub verification_state: String,
    pub revision: String,
    pub capability_fingerprint: String,
    pub added_at: String,
    pub removed_at: Option<String>,
    pub skill_count: i64,
    pub record: Value,
    pub limitations: Vec<String>,
}

const SELECT_ENTRY: &str =
    "SELECT entry_id, kind, origin, display_name, purpose, source_key, repo_url, source_path, plugin_identity, review_state,
    metadata_state, execution_policy, license_state, license_spdx, enabled, install_state, verification_state, revision,
    capability_fingerprint, added_at, removed_at, record_json,
    (SELECT count(*) FROM skill_index s WHERE s.entry_id = catalog_entries.entry_id) FROM catalog_entries";

fn limitations(e: &CatalogEntry) -> Vec<String> {
    let mut out = Vec::new();
    if e.metadata_state == "not-fetched" {
        out.push("Çevrimdışı kayıt: yalnızca bağlantıdan türetilen bilgiler var; lisans, sürüm ve uyumluluk bilinmiyor.".to_string());
    } else {
        out.push("Bilgiler 2026-10-17 tarihli herkese açık belgelerden alındı; bağımsız inceleme yapılmadı.".to_string());
    }
    match e.review_state.as_str() {
        "quarantined" => out.push("Karantinada: kapsamlı yazılı yetki olmadan hiçbir çalıştırma yapılamaz.".to_string()),
        "unreviewed" => out.push("İncelenmedi.".to_string()),
        _ => {}
    }
    match e.license_state.as_str() {
        "known" => {}
        "review-required" => out.push("Lisans, öğe bazında ayrıca incelenmeli (bileşenlerin lisansları farklı olabilir).".to_string()),
        _ => out.push("Lisans bilinmiyor.".to_string()),
    }
    let unknown_targets = e
        .record
        .get("targetEvidence")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter(|t| t.get("compatibilityState").and_then(Value::as_str) == Some("unknown")).count())
        .unwrap_or(0);
    if e.record.get("targetEvidence").and_then(Value::as_array).map(|a| a.is_empty()).unwrap_or(true) {
        out.push("Hangi platform ve ajanda çalıştığı bilinmiyor.".to_string());
    } else if unknown_targets > 0 {
        out.push(format!("{unknown_targets} platform/ajan birleşiminde uyumluluk bilinmiyor."));
    }
    out.push("Kurulum ve işlev doğrulaması yok; bu uygulama hiçbir şey kurmaz veya çalıştırmaz.".to_string());
    out
}

fn row_to_entry(r: &rusqlite::Row) -> rusqlite::Result<CatalogEntry> {
    let record_json: String = r.get(21)?;
    let mut e = CatalogEntry {
        entry_id: r.get(0)?,
        kind: r.get(1)?,
        origin: r.get(2)?,
        display_name: r.get(3)?,
        purpose: r.get(4)?,
        source_key: r.get(5)?,
        repo_url: r.get(6)?,
        source_path: r.get(7)?,
        plugin_identity: r.get(8)?,
        review_state: r.get(9)?,
        metadata_state: r.get(10)?,
        execution_policy: r.get(11)?,
        license_state: r.get(12)?,
        license_spdx: r.get(13)?,
        enabled: r.get::<_, i64>(14)? == 1,
        install_state: r.get(15)?,
        verification_state: r.get(16)?,
        revision: r.get(17)?,
        capability_fingerprint: r.get(18)?,
        added_at: r.get(19)?,
        removed_at: r.get(20)?,
        skill_count: r.get(22)?,
        record: serde_json::from_str(&record_json).unwrap_or(Value::Null),
        limitations: Vec::new(),
    };
    e.limitations = limitations(&e);
    Ok(e)
}

pub fn list_catalog(conn: &Connection) -> AppResult<Vec<CatalogEntry>> {
    let sql = format!("{SELECT_ENTRY} ORDER BY removed_at IS NOT NULL, display_name COLLATE NOCASE");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_entry)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get_entry(conn: &Connection, entry_id: &str) -> AppResult<CatalogEntry> {
    let sql = format!("{SELECT_ENTRY} WHERE entry_id = ?1");
    conn.query_row(&sql, [entry_id], row_to_entry)
        .optional()?
        .ok_or_else(|| AppError::new("ENTRY_NOT_FOUND", "Katalogda böyle bir kayıt yok."))
}

/// Compatibility state recorded for a platform/surface pair; anything not recorded is `unknown`.
pub fn compatibility(entry: &CatalogEntry, platform: &str, surface: &str) -> String {
    entry
        .record
        .get("targetEvidence")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter().find(|t| {
                t.get("platformTarget").and_then(Value::as_str) == Some(platform)
                    && t.get("agentSurface").and_then(Value::as_str) == Some(surface)
            })
        })
        .and_then(|t| t.get("compatibilityState").and_then(Value::as_str))
        .unwrap_or("unknown")
        .to_string()
}

// ---------------------------------------------------------------------------------------------
// User actions: preview, add, enable/disable, remove, restore
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Duplicate {
    pub entry_id: String,
    pub display_name: String,
    pub removed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlPreview {
    pub ok: bool,
    pub parsed: Option<ParsedRepo>,
    pub error: Option<AppError>,
    pub duplicate_of: Option<Duplicate>,
    pub notes: Vec<String>,
}

fn find_by_source(conn: &Connection, source_key: &str) -> AppResult<Option<Duplicate>> {
    Ok(conn
        .query_row(
            "SELECT entry_id, display_name, removed_at IS NOT NULL FROM catalog_entries WHERE source_key = ?1
             ORDER BY removed_at IS NOT NULL LIMIT 1",
            [source_key],
            |r| Ok(Duplicate { entry_id: r.get(0)?, display_name: r.get(1)?, removed: r.get::<_, i64>(2)? == 1 }),
        )
        .optional()?)
}

/// Pure and offline: validates the link and reports duplicates. Nothing is fetched.
pub fn preview_url(conn: &Connection, url: &str) -> AppResult<UrlPreview> {
    let notes = vec![
        "Bu bir ön izlemedir: hiçbir şey indirilmedi veya çalıştırılmadı; uzak GitHub verisi çekilmedi (çevrimdışı çalışma).".to_string(),
        "Yalnızca bağlantıdan türetilen bilgiler gösterilir. Lisans, sürüm ve uyumluluk bilinmiyor olarak kaydedilir.".to_string(),
    ];
    match github_url::parse(url) {
        Err(error) => Ok(UrlPreview { ok: false, parsed: None, error: Some(error), duplicate_of: None, notes }),
        Ok(parsed) => {
            let duplicate_of = find_by_source(conn, &parsed.source_key)?;
            let ok = duplicate_of.as_ref().map(|d| d.removed).unwrap_or(true);
            Ok(UrlPreview { ok, parsed: Some(parsed), error: None, duplicate_of, notes })
        }
    }
}

fn clean_purpose(raw: &str) -> String {
    let cleaned: String = raw.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = cleaned.chars().take(MAX_PURPOSE_CHARS).collect();
    if cut.is_empty() {
        "Amacı henüz yazılmadı.".to_string()
    } else {
        cut
    }
}

fn unique_entry_id(conn: &Connection, parsed: &ParsedRepo) -> AppResult<String> {
    let base = github_url::entry_slug(parsed);
    for n in 1..=99 {
        let candidate = if n == 1 { base.clone() } else { format!("{}-{n}", &base[..base.len().min(60)]) };
        let taken: Option<i64> =
            conn.query_row("SELECT 1 FROM catalog_entries WHERE entry_id = ?1", [&candidate], |r| r.get(0)).optional()?;
        if taken.is_none() {
            return Ok(candidate);
        }
    }
    Err(AppError::new("ENTRY_ID_EXHAUSTED", "Bu depo için benzersiz bir kimlik üretilemedi."))
}

pub fn add_entry(conn: &Connection, url: &str, kind: &str, purpose: &str) -> AppResult<CatalogEntry> {
    let parsed = github_url::parse(url)?;
    let kind = static_kind(kind).map_err(|_| AppError::new("KIND_INVALID", "Geçersiz tür seçildi."))?;
    let purpose = clean_purpose(purpose);
    if let Some(dup) = find_by_source(conn, &parsed.source_key)? {
        if !dup.removed {
            return Err(AppError::new("ENTRY_DUPLICATE", format!("Bu depo zaten katalogda: {}.", dup.display_name)));
        }
        // Re-adding a removed source restores that entry (its decision history stays attached).
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE catalog_entries SET removed_at = NULL, kind = CASE WHEN origin = 'user' THEN ?2 ELSE kind END,
               purpose = CASE WHEN origin = 'user' THEN ?3 ELSE purpose END, enabled = 0, updated_at = ?4 WHERE entry_id = ?1",
            rusqlite::params![dup.entry_id, kind, purpose, now_utc()],
        )?;
        diagnostics::log(&tx, "info", "catalog", "CATALOG_ENTRY_RESTORED", &dup.entry_id)?;
        tx.commit()?;
        return get_entry(conn, &dup.entry_id);
    }
    let entry_id = unique_entry_id(conn, &parsed)?;
    let record = json!({
        "schemaVersion": 0,
        "userAdded": true,
        "repository": { "url": parsed.canonical_url, "owner": parsed.owner, "name": parsed.repo },
        "targetEvidence": [],
        "installMethods": [],
        "license": { "state": "unknown" },
        "privacy": { "telemetry": "unknown", "mayTransmitProjectContent": null },
        "note": "Yalnızca bağlantıdan türetildi; uzak veri çekilmedi.",
    });
    let fingerprint = capability_fingerprint(&record, kind, "manual-only");
    let now = now_utc();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO catalog_entries
         (entry_id, kind, origin, display_name, purpose, source_key, repo_url, source_path, plugin_identity, review_state, metadata_state,
          execution_policy, license_state, license_spdx, enabled, revision, capability_fingerprint, record_json, added_at, updated_at)
         VALUES (?1, ?2, 'user', ?3, ?4, ?5, ?6, NULL, NULL, 'unreviewed', 'not-fetched', 'manual-only', 'unknown', NULL, 0, 'user-added', ?7, ?8, ?9, ?9)",
        rusqlite::params![
            entry_id,
            kind,
            format!("{}/{}", parsed.owner, parsed.repo),
            purpose,
            parsed.source_key,
            parsed.canonical_url,
            fingerprint,
            record.to_string(),
            now
        ],
    )?;
    diagnostics::log(&tx, "info", "catalog", "CATALOG_ENTRY_ADDED", &entry_id)?;
    tx.commit()?;
    get_entry(conn, &entry_id)
}

fn require_live(conn: &Connection, entry_id: &str) -> AppResult<CatalogEntry> {
    let entry = get_entry(conn, entry_id)?;
    if entry.removed_at.is_some() {
        return Err(AppError::new("ENTRY_REMOVED", "Bu kayıt katalogdan kaldırılmış."));
    }
    Ok(entry)
}

pub fn set_enabled(conn: &Connection, entry_id: &str, enabled: bool) -> AppResult<CatalogEntry> {
    require_live(conn, entry_id)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE catalog_entries SET enabled = ?2, updated_at = ?3 WHERE entry_id = ?1",
        rusqlite::params![entry_id, enabled as i64, now_utc()],
    )?;
    diagnostics::log(&tx, "info", "catalog", if enabled { "CATALOG_ENTRY_ENABLED" } else { "CATALOG_ENTRY_DISABLED" }, entry_id)?;
    tx.commit()?;
    get_entry(conn, entry_id)
}

/// Removes the registry mapping only. Nothing is uninstalled (nothing was ever installed) and the
/// user's earlier decisions stay in the history.
pub fn remove_entry(conn: &Connection, entry_id: &str) -> AppResult<CatalogEntry> {
    require_live(conn, entry_id)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE catalog_entries SET removed_at = ?2, enabled = 0, updated_at = ?2 WHERE entry_id = ?1",
        rusqlite::params![entry_id, now_utc()],
    )?;
    diagnostics::log(&tx, "info", "catalog", "CATALOG_ENTRY_REMOVED", entry_id)?;
    // `catalog_remove` is the schema-valid audit event for this action (Phase 0 audit-event schema v2).
    let mut event = NewEvent::plain("catalog_remove", "passed", "catalog-remove", None, "not-applicable", "user");
    event.tool_id = Some(entry_id.to_string());
    crate::audit::append(&tx, &event)?;
    tx.commit()?;
    get_entry(conn, entry_id)
}

pub fn restore_entry(conn: &Connection, entry_id: &str) -> AppResult<CatalogEntry> {
    let entry = get_entry(conn, entry_id)?;
    if entry.removed_at.is_none() {
        return Ok(entry);
    }
    if let Some(other) = find_by_source(conn, &entry.source_key)? {
        if other.entry_id != entry.entry_id && !other.removed {
            return Err(AppError::new("ENTRY_DUPLICATE", "Aynı kaynaktan başka bir kayıt zaten katalogda."));
        }
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE catalog_entries SET removed_at = NULL, enabled = 0, updated_at = ?2 WHERE entry_id = ?1",
        rusqlite::params![entry_id, now_utc()],
    )?;
    diagnostics::log(&tx, "info", "catalog", "CATALOG_ENTRY_RESTORED", entry_id)?;
    tx.commit()?;
    get_entry(conn, entry_id)
}

// ---------------------------------------------------------------------------------------------
// Prompt sources (Phase 1A, unchanged)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptSourceRow {
    pub source_id: String,
    pub display_name: String,
    pub repo_url: String,
    pub license_note: String,
    pub planned_format: String,
    pub notes: String,
    pub trust_status: String,
    pub status: String,
    pub read_only: bool,
}

pub fn list_prompt_sources(conn: &Connection) -> AppResult<Vec<PromptSourceRow>> {
    let mut stmt = conn.prepare(
        "SELECT source_id, display_name, repo_url, license_note, planned_format, notes, trust_status, status, read_only
         FROM prompt_sources ORDER BY rowid",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(PromptSourceRow {
            source_id: r.get(0)?,
            display_name: r.get(1)?,
            repo_url: r.get(2)?,
            license_note: r.get(3)?,
            planned_format: r.get(4)?,
            notes: r.get(5)?,
            trust_status: r.get(6)?,
            status: r.get(7)?,
            read_only: r.get::<_, i64>(8)? == 1,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> Connection {
        let conn = crate::db::open_memory().unwrap();
        seed_all(&conn).unwrap();
        conn
    }

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn seed_loads_thirteen_entries_with_correct_classification() {
        let conn = seeded();
        let all = list_catalog(&conn).unwrap();
        assert_eq!(all.len(), SEED_ENTRY_COUNT);
        let kind = |id: &str| all.iter().find(|e| e.entry_id == id).unwrap().kind.clone();
        assert_eq!(kind("anthropic-frontend-design"), "agent_plugin");
        assert_eq!(kind("strix"), "executable_tool");
        assert_eq!(kind("agent-browser"), "executable_tool");
        assert_eq!(kind("agentic-awesome-skills"), "skill_collection");
        assert_eq!(kind("gitleaks"), "executable_tool");
        assert_eq!(kind("superpowers"), "agent_plugin");
        assert!(all.iter().all(|e| KINDS.contains(&e.kind.as_str())));
        assert!(all.iter().all(|e| e.install_state == "not_managed" && e.verification_state == "unknown"));
        assert!(all.iter().all(|e| e.origin == "seed" && e.metadata_state == "documented-snapshot"));
        let fd = all.iter().find(|e| e.entry_id == "anthropic-frontend-design").unwrap();
        assert_eq!(fd.plugin_identity.as_deref(), Some("frontend-design@claude-plugins-official"));
        assert_eq!(fd.source_path.as_deref(), Some("plugins/frontend-design"));
        assert_eq!(fd.source_key, "anthropics/claude-plugins-official#plugins/frontend-design");
    }

    #[test]
    fn strix_is_quarantined_and_execution_blocked() {
        let conn = seeded();
        let strix = get_entry(&conn, "strix").unwrap();
        assert_eq!(strix.review_state, "quarantined");
        assert_eq!(strix.execution_policy, "blocked-until-scoped-authorization");
        assert!(strix.enabled, "visible for information; execution is what stays blocked");
        assert!(strix.limitations.iter().any(|l| l.contains("Karantinada")));
        // The collection is not trusted as a whole: license needs per-skill review.
        let aas = get_entry(&conn, "agentic-awesome-skills").unwrap();
        assert_eq!(aas.license_state, "review-required");
        assert!(aas.limitations.iter().any(|l| l.contains("öğe bazında")));
    }

    #[test]
    fn no_seed_entry_claims_a_review_or_a_verification() {
        let conn = seeded();
        for e in list_catalog(&conn).unwrap() {
            assert!(matches!(e.review_state.as_str(), "unreviewed" | "quarantined"), "{} claims {}", e.entry_id, e.review_state);
            assert_eq!(e.verification_state, "unknown");
        }
    }

    #[test]
    fn seeding_is_repeatable_and_never_overrides_user_choices() {
        let conn = seeded();
        let snapshot = |c: &Connection| {
            (
                count(c, "SELECT count(*) FROM catalog_entries"),
                count(c, "SELECT count(*) FROM project_registry"),
                count(c, "SELECT count(*) FROM project_preferences"),
                count(c, "SELECT count(*) FROM prompt_sources"),
                count(c, "SELECT count(*) FROM preference_history"),
            )
        };
        let first = snapshot(&conn);
        assert_eq!(first, (13, 3, 6, 3, 6));
        set_enabled(&conn, "agent-browser", false).unwrap();
        remove_entry(&conn, "gitleaks").unwrap();
        // The user resets a demonstration decision; a restart must not bring it back.
        conn.execute("DELETE FROM project_preferences WHERE entry_id = 'graphify'", []).unwrap();
        seed_all(&conn).unwrap();
        seed_all(&conn).unwrap();
        assert_eq!(count(&conn, "SELECT count(*) FROM catalog_entries"), 13);
        assert_eq!(count(&conn, "SELECT count(*) FROM project_preferences"), 5);
        assert!(!get_entry(&conn, "agent-browser").unwrap().enabled);
        assert!(get_entry(&conn, "gitleaks").unwrap().removed_at.is_some());
    }

    #[test]
    fn database_refuses_installed_verified_or_unknown_kinds() {
        let conn = seeded();
        assert!(conn.execute("UPDATE catalog_entries SET install_state='installed'", []).is_err());
        assert!(conn.execute("UPDATE catalog_entries SET verification_state='passed'", []).is_err());
        assert!(conn.execute("UPDATE catalog_entries SET kind='anything'", []).is_err());
        assert!(conn.execute("UPDATE catalog_entries SET execution_policy='auto-run'", []).is_err());
        assert!(conn.execute("UPDATE project_preferences SET actual_install_state='installed'", []).is_err());
        assert!(conn.execute("UPDATE project_preferences SET actual_verification_state='passed'", []).is_err());
        assert!(conn.execute("UPDATE project_preferences SET observed_scope='project'", []).is_err());
        assert!(conn.execute("INSERT INTO catalog_entries (entry_id) VALUES ('x')", []).is_err());
    }

    #[test]
    fn user_can_add_a_repository_as_an_offline_unreviewed_candidate() {
        let conn = seeded();
        let e = add_entry(&conn, "https://github.com/Some-Org/Cool.Tool", "executable_tool", "Deneme\naraç\u{0007}").unwrap();
        assert_eq!(e.entry_id, "some-org-cool-tool");
        assert_eq!((e.origin.as_str(), e.review_state.as_str(), e.metadata_state.as_str()), ("user", "unreviewed", "not-fetched"));
        assert_eq!(
            (e.license_state.as_str(), e.install_state.as_str(), e.verification_state.as_str()),
            ("unknown", "not_managed", "unknown")
        );
        assert!(!e.enabled, "a new candidate starts disabled");
        assert_eq!(e.execution_policy, "manual-only");
        assert_eq!(e.purpose, "Deneme araç");
        assert!(e.limitations.iter().any(|l| l.contains("Çevrimdışı")));
        assert!(e.limitations.iter().any(|l| l.contains("Lisans bilinmiyor")));
        assert_eq!(compatibility(&e, "windows-x64", "claude-code-terminal"), "unknown");
        assert!(set_enabled(&conn, &e.entry_id, true).unwrap().enabled);
    }

    #[test]
    fn duplicate_links_are_rejected_in_every_spelling_including_seeds() {
        let conn = seeded();
        add_entry(&conn, "https://github.com/acme/widget", "agent_plugin", "").unwrap();
        for dup in ["https://github.com/acme/widget", "https://github.com/ACME/Widget/", "HTTPS://GITHUB.COM/acme/widget.git"] {
            assert_eq!(add_entry(&conn, dup, "agent_plugin", "").unwrap_err().code, "ENTRY_DUPLICATE", "{dup}");
        }
        // A seed already owns this source.
        assert_eq!(add_entry(&conn, "https://github.com/usestrix/strix", "executable_tool", "").unwrap_err().code, "ENTRY_DUPLICATE");
        assert_eq!(
            add_entry(&conn, "https://github.com/vercel-labs/Agent-Browser", "executable_tool", "").unwrap_err().code,
            "ENTRY_DUPLICATE"
        );
        assert_eq!(count(&conn, "SELECT count(*) FROM catalog_entries"), 14);
        let preview = preview_url(&conn, "https://github.com/acme/WIDGET").unwrap();
        assert!(!preview.ok && preview.duplicate_of.is_some());
    }

    #[test]
    fn misleading_or_unsafe_links_never_reach_the_database() {
        let conn = seeded();
        for bad in [
            "https://github.com.evil.example/acme/widget",
            "http://github.com/acme/widget",
            "https://github.com/acme/widget/tree/main",
            "https://github.com/settings/profile",
            "https://user:pw@github.com/acme/widget",
            "https://github.com/acme/widget?x=1",
            "https://github.com/acme/../widget",
            "ssh://git@github.com/acme/widget",
            "javascript:alert(1)",
            "https://github.com/acme/wid get",
            "https://github.com/acme/widget/blob/main/install.sh",
            "https://raw.githubusercontent.com/acme/widget/main/install.sh",
        ] {
            assert!(add_entry(&conn, bad, "executable_tool", "").is_err(), "accepted {bad}");
            assert!(!preview_url(&conn, bad).unwrap().ok, "preview ok for {bad}");
        }
        assert_eq!(count(&conn, "SELECT count(*) FROM catalog_entries"), 13);
        assert_eq!(add_entry(&conn, "https://github.com/acme/widget", "everything", "").unwrap_err().code, "KIND_INVALID");
    }

    #[test]
    fn long_or_hostile_purpose_text_is_neutralised() {
        let conn = seeded();
        let nasty = format!("<script>alert(1)</script> $(rm -rf /) {}", "x".repeat(2000));
        let e = add_entry(&conn, "https://github.com/acme/widget", "agent_plugin", &nasty).unwrap();
        assert!(e.purpose.chars().count() <= MAX_PURPOSE_CHARS);
        // It is stored as inert text; the UI renders it as text, never as markup or a command.
        assert!(e.purpose.contains("<script>"));
        assert_eq!(e.install_state, "not_managed");
    }

    #[test]
    fn remove_and_restore_do_not_touch_anything_installed_and_keep_history() {
        let conn = seeded();
        let before_prefs = count(&conn, "SELECT count(*) FROM project_preferences");
        let removed = remove_entry(&conn, "graphify").unwrap();
        assert!(removed.removed_at.is_some() && !removed.enabled);
        assert_eq!(count(&conn, "SELECT count(*) FROM project_preferences"), before_prefs, "decisions are kept");
        assert_eq!(set_enabled(&conn, "graphify", true).unwrap_err().code, "ENTRY_REMOVED");
        assert_eq!(remove_entry(&conn, "graphify").unwrap_err().code, "ENTRY_REMOVED");
        let events = crate::audit::list_ascending(&conn).unwrap();
        assert_eq!(events.last().unwrap()["eventType"], "catalog_remove");
        assert_eq!(events.last().unwrap()["toolId"], "graphify");
        assert!(crate::audit::verify(&conn).unwrap().ok);
        let restored = restore_entry(&conn, "graphify").unwrap();
        assert!(restored.removed_at.is_none());
        assert!(!restored.enabled, "a restored entry starts disabled until the user enables it");
        // Re-adding a removed user link restores it instead of creating a duplicate.
        let e = add_entry(&conn, "https://github.com/acme/widget", "agent_plugin", "a").unwrap();
        remove_entry(&conn, &e.entry_id).unwrap();
        let again = add_entry(&conn, "https://github.com/acme/widget", "agent_plugin", "b").unwrap();
        assert_eq!(again.entry_id, e.entry_id);
        assert_eq!(count(&conn, "SELECT count(*) FROM catalog_entries WHERE source_key = 'acme/widget'"), 1);
    }

    #[test]
    fn fingerprint_ignores_version_bumps_but_changes_with_capabilities() {
        let conn = seeded();
        let mut rec = get_entry(&conn, "agent-browser").unwrap().record;
        let base = capability_fingerprint(&rec, "executable_tool", "manual-only");
        rec["sourceSnapshot"]["releaseVersion"] = json!("v9.9.9");
        rec["sourceSnapshot"]["immutableCommit"] = json!("0".repeat(40));
        assert_eq!(capability_fingerprint(&rec, "executable_tool", "manual-only"), base, "a new release alone must not change it");
        rec["targetEvidence"][0]["compatibilityState"] = json!("unsupported");
        assert_ne!(capability_fingerprint(&rec, "executable_tool", "manual-only"), base);
        let mut net = get_entry(&conn, "agent-browser").unwrap().record;
        net["networkClaims"] = json!(["now also uploads everything"]);
        assert_ne!(capability_fingerprint(&net, "executable_tool", "manual-only"), base);
    }

    #[test]
    fn prompt_sources_remain_three_read_only_planned_references() {
        let conn = seeded();
        let sources = list_prompt_sources(&conn).unwrap();
        assert_eq!(sources.len(), 3);
        assert!(sources.iter().all(|s| s.read_only && s.status == "planned" && s.trust_status == "untrusted-reference"));
    }
}
