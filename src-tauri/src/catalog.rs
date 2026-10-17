//! Seeds and reads the nine-tool catalog, the three planned prompt sources and the clearly
//! synthetic sample projects. Seeding is idempotent and can never change an install state.

use crate::{
    audit::now_utc,
    error::{AppError, AppResult},
};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

/// Single source of truth shared with the Phase 0 contract tests.
const SEED_JSON: &str = include_str!("../../data/catalog-seed.json");
pub const EXPECTED_TOOL_COUNT: usize = 9;

fn seed_error(why: &str) -> AppError {
    AppError::new("SEED_INVALID", format!("Katalog tohum verisi geçersiz: {why}"))
}

pub fn parse_seed() -> AppResult<Vec<Value>> {
    let root: Value = serde_json::from_str(SEED_JSON).map_err(|_| seed_error("JSON okunamadı"))?;
    let records = root.get("records").and_then(Value::as_array).ok_or_else(|| seed_error("records yok"))?;
    if records.len() != EXPECTED_TOOL_COUNT {
        return Err(seed_error("dokuz kayıt bekleniyordu"));
    }
    let mut ids = std::collections::BTreeSet::new();
    for record in records {
        let id = record.get("toolId").and_then(Value::as_str).ok_or_else(|| seed_error("toolId yok"))?;
        if !ids.insert(id.to_string()) {
            return Err(seed_error("yinelenen toolId"));
        }
        // Phase 1A must never present a reviewed/approved state that no review produced.
        if record.get("reviewState").and_then(Value::as_str) != Some("unreviewed") {
            return Err(seed_error("tüm kayıtlar 'unreviewed' olmalı"));
        }
    }
    Ok(records.clone())
}

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

/// Synthetic examples only: (alias, display name, description)
const SAMPLE_PROJECTS: &[(&str, &str, &str)] = &[
    ("ornek-web-sitesi", "Örnek: Web sitesi", "Uydurma örnek proje. Gerçek bir klasörünüzle ilgisi yoktur."),
    ("ornek-veri-analizi", "Örnek: Veri analizi", "Uydurma örnek proje. Gerçek bir klasörünüzle ilgisi yoktur."),
    ("ornek-mobil-uygulama", "Örnek: Mobil uygulama", "Uydurma örnek proje. Gerçek bir klasörünüzle ilgisi yoktur."),
];

/// (project alias, tool id, agent surface, platform target, decision) — all taken from rows that
/// exist in the seed's targetEvidence, but they are demonstration decisions, not real choices.
const SAMPLE_DECISIONS: &[(&str, &str, &str, &str, &str)] = &[
    ("ornek-web-sitesi", "gitleaks", "local-cli", "windows-x64", "accepted"),
    ("ornek-web-sitesi", "caveman", "claude-code-terminal", "windows-x64", "later"),
    ("ornek-veri-analizi", "markitdown", "local-cli", "windows-x64", "accepted"),
    ("ornek-veri-analizi", "graphify", "local-cli", "windows-x64", "rejected"),
    ("ornek-mobil-uygulama", "superpowers", "claude-code-terminal", "macos-arm64", "disabled"),
    ("ornek-mobil-uygulama", "codebase-memory-mcp", "claude-code-terminal", "macos-arm64", "accepted"),
];

pub fn seed_all(conn: &Connection) -> AppResult<()> {
    let records = parse_seed()?;
    let now = now_utc();
    let tx = conn.unchecked_transaction()?;
    for record in &records {
        let s = |key: &str| record.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
        let url = record.pointer("/repository/url").and_then(Value::as_str).unwrap_or_default();
        // Metadata may be refreshed; install_state / validation_state are never touched here.
        tx.execute(
            "INSERT INTO tools (tool_id, display_name, category, repo_url, review_state, record_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(tool_id) DO UPDATE SET
               display_name = excluded.display_name, category = excluded.category, repo_url = excluded.repo_url,
               record_json = excluded.record_json, updated_at = excluded.updated_at",
            rusqlite::params![s("toolId"), s("displayName"), s("category"), url, s("reviewState"), record.to_string(), now],
        )?;
    }
    for (id, name, url, license, format, notes) in PROMPT_SOURCES {
        tx.execute(
            "INSERT OR IGNORE INTO prompt_sources
             (source_id, display_name, repo_url, license_note, planned_format, notes, trust_status, status, read_only)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'untrusted-reference', 'planned', 1)",
            rusqlite::params![id, name, url, license, format, notes],
        )?;
    }
    for (alias, name, description) in SAMPLE_PROJECTS {
        tx.execute(
            "INSERT OR IGNORE INTO projects (alias, display_name, description, is_synthetic, created_at)
             VALUES (?1, ?2, ?3, 1, ?4)",
            rusqlite::params![alias, name, description, now],
        )?;
    }
    for (alias, tool, surface, platform, decision) in SAMPLE_DECISIONS {
        tx.execute(
            "INSERT OR IGNORE INTO project_tool_decisions (project_id, tool_id, agent_surface, platform_target, decision)
             SELECT id, ?2, ?3, ?4, ?5 FROM projects WHERE alias = ?1",
            rusqlite::params![alias, tool, surface, platform, decision],
        )?;
    }
    tx.commit()?;
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRow {
    pub tool_id: String,
    pub display_name: String,
    pub category: String,
    pub repo_url: String,
    pub review_state: String,
    pub install_state: String,
    pub validation_state: String,
    pub record: Value,
}

pub fn list_tools(conn: &Connection) -> AppResult<Vec<ToolRow>> {
    let mut stmt = conn.prepare(
        "SELECT tool_id, display_name, category, repo_url, review_state, install_state, validation_state, record_json
         FROM tools ORDER BY display_name COLLATE NOCASE",
    )?;
    let rows =
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get::<_, String>(7)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (tool_id, display_name, category, repo_url, review_state, install_state, validation_state, json) = row?;
        out.push(ToolRow {
            tool_id,
            display_name,
            category,
            repo_url,
            review_state,
            install_state,
            validation_state,
            record: serde_json::from_str(&json)?,
        });
    }
    Ok(out)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRow {
    pub tool_id: String,
    pub tool_name: String,
    pub agent_surface: String,
    pub platform_target: String,
    pub decision: String,
    pub actual_state: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRow {
    pub alias: String,
    pub display_name: String,
    pub description: String,
    pub is_synthetic: bool,
    pub decisions: Vec<DecisionRow>,
}

pub fn list_projects(conn: &Connection) -> AppResult<Vec<ProjectRow>> {
    let mut projects = Vec::new();
    let mut stmt = conn.prepare("SELECT id, alias, display_name, description, is_synthetic FROM projects ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?))
    })?;
    for row in rows {
        let (id, alias, display_name, description, synthetic) = row?;
        let mut dstmt = conn.prepare(
            "SELECT d.tool_id, t.display_name, d.agent_surface, d.platform_target, d.decision, d.actual_state
             FROM project_tool_decisions d JOIN tools t ON t.tool_id = d.tool_id
             WHERE d.project_id = ?1 ORDER BY t.display_name COLLATE NOCASE",
        )?;
        let decisions = dstmt
            .query_map([id], |r| {
                Ok(DecisionRow {
                    tool_id: r.get(0)?,
                    tool_name: r.get(1)?,
                    agent_surface: r.get(2)?,
                    platform_target: r.get(3)?,
                    decision: r.get(4)?,
                    actual_state: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        projects.push(ProjectRow { alias, display_name, description, is_synthetic: synthetic == 1, decisions });
    }
    Ok(projects)
}

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

    fn counts(conn: &Connection) -> (i64, i64, i64, i64) {
        let n = |t: &str| conn.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get::<_, i64>(0)).unwrap();
        (n("tools"), n("projects"), n("project_tool_decisions"), n("prompt_sources"))
    }

    #[test]
    fn seed_loads_nine_unreviewed_tools_and_three_prompt_sources() {
        let conn = crate::db::open_memory().unwrap();
        seed_all(&conn).unwrap();
        assert_eq!(counts(&conn), (9, 3, 6, 3));
        let tools = list_tools(&conn).unwrap();
        assert!(tools
            .iter()
            .all(|t| t.review_state == "unreviewed" && t.install_state == "not_managed" && t.validation_state == "unknown"));
        let sources = list_prompt_sources(&conn).unwrap();
        assert!(sources.iter().all(|s| s.read_only && s.status == "planned" && s.trust_status == "untrusted-reference"));
        assert!(list_projects(&conn).unwrap().iter().all(|p| p.is_synthetic));
    }

    #[test]
    fn seeding_is_repeatable() {
        let conn = crate::db::open_memory().unwrap();
        seed_all(&conn).unwrap();
        let first = counts(&conn);
        seed_all(&conn).unwrap();
        seed_all(&conn).unwrap();
        assert_eq!(counts(&conn), first);
    }

    #[test]
    fn database_refuses_installed_or_real_project_state() {
        let conn = crate::db::open_memory().unwrap();
        seed_all(&conn).unwrap();
        assert!(conn.execute("UPDATE tools SET install_state='installed'", []).is_err());
        assert!(conn.execute("UPDATE tools SET validation_state='passed'", []).is_err());
        assert!(conn
            .execute("INSERT INTO projects (alias, display_name, description, is_synthetic, created_at) VALUES ('real','r','r',0,'x')", [])
            .is_err());
        assert!(conn.execute("UPDATE project_tool_decisions SET actual_state='installed'", []).is_err());
    }

    #[test]
    fn duplicate_logical_decision_keys_are_rejected() {
        let conn = crate::db::open_memory().unwrap();
        seed_all(&conn).unwrap();
        let dup = conn.execute(
            "INSERT INTO project_tool_decisions (project_id, tool_id, agent_surface, platform_target, decision)
             SELECT project_id, tool_id, agent_surface, platform_target, 'rejected' FROM project_tool_decisions LIMIT 1",
            [],
        );
        assert!(dup.is_err());
    }
}
