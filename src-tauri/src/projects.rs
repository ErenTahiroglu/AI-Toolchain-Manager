//! Project registry and per-project tool preferences.
//!
//! A preference is the user's *desired* state for `(project, tool, agent surface, platform)`.
//! It is stored apart from the *actual* state, which stays `not_managed` / `unknown` in this phase.
//! A real project is only a user-chosen reference (alias + optional private note): the app never
//! opens, scans or modifies the folder and never changes any agent configuration.

use crate::{
    audit::{self, now_utc, NewEvent},
    catalog::{self, CatalogEntry},
    diagnostics,
    error::{AppError, AppResult},
};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

pub const SURFACES: &[&str] = &["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "codex-cli", "local-cli"];
pub const PLATFORMS: &[&str] = &["windows-x64", "macos-arm64"];
pub const DECISIONS: &[&str] = &["accepted", "rejected", "postponed", "disabled"];
const DEFAULT_POSTPONE_DAYS: i64 = 7;

fn static_of(list: &'static [&'static str], value: &str) -> Option<&'static str> {
    list.iter().copied().find(|v| *v == value)
}

pub fn valid_alias(alias: &str) -> bool {
    let b = alias.as_bytes();
    (2..=42).contains(&b.len())
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        && b[0] != b'-'
        && b[b.len() - 1] != b'-'
}

/// Audit schema target ids exist only for these combinations; everything else is `unknown`.
fn audit_target(platform: &str, surface: &str) -> &'static str {
    match (platform, surface) {
        ("windows-x64", "claude-code-terminal") => "windows-x64-claude-code",
        ("macos-arm64", "claude-code-terminal") => "macos-arm64-claude-code",
        ("macos-arm64", "antigravity-ide") => "macos-arm64-antigravity-ide",
        _ => "unknown",
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceView {
    pub entry_id: String,
    pub entry_name: String,
    pub entry_removed: bool,
    pub agent_surface: String,
    pub platform_target: String,
    pub decision: String,
    pub decided_at: String,
    pub catalog_revision: String,
    /// The catalog revision moved on since the decision (informational; never re-prompts by itself).
    pub revision_changed: bool,
    /// Capabilities / compatibility / data flows changed materially since the decision.
    pub material_change: bool,
    pub remind_at: Option<String>,
    pub actual_install_state: String,
    pub actual_verification_state: String,
    pub observed_scope: String,
    pub compatibility: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub project_id: String,
    pub alias: String,
    pub display_name: String,
    pub kind: String,
    pub local_note: String,
    pub created_at: String,
    pub preferences: Vec<PreferenceView>,
}

fn preferences_for(conn: &Connection, project_id: &str) -> AppResult<Vec<PreferenceView>> {
    let mut stmt = conn.prepare(
        "SELECT p.entry_id, p.agent_surface, p.platform_target, p.decision, p.decided_at, p.catalog_revision, p.capability_fingerprint,
                p.remind_at, p.actual_install_state, p.actual_verification_state, p.observed_scope
         FROM project_preferences p JOIN catalog_entries e ON e.entry_id = p.entry_id
         WHERE p.project_id = ?1 ORDER BY e.display_name COLLATE NOCASE, p.agent_surface, p.platform_target",
    )?;
    let rows = stmt.query_map([project_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, Option<String>>(7)?,
            r.get::<_, String>(8)?,
            r.get::<_, String>(9)?,
            r.get::<_, String>(10)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (entry_id, surface, platform, decision, decided_at, revision, fingerprint, remind_at, install, verification, scope) = row?;
        let entry = catalog::get_entry(conn, &entry_id)?;
        out.push(PreferenceView {
            compatibility: catalog::compatibility(&entry, &platform, &surface),
            entry_name: entry.display_name.clone(),
            entry_removed: entry.removed_at.is_some(),
            revision_changed: entry.revision != revision,
            material_change: entry.capability_fingerprint != fingerprint,
            entry_id,
            agent_surface: surface,
            platform_target: platform,
            decision,
            decided_at,
            catalog_revision: revision,
            remind_at,
            actual_install_state: install,
            actual_verification_state: verification,
            observed_scope: scope,
        });
    }
    Ok(out)
}

pub fn list_projects(conn: &Connection) -> AppResult<Vec<ProjectView>> {
    let mut stmt = conn.prepare(
        "SELECT project_id, alias, display_name, kind, local_note, created_at FROM project_registry
         WHERE removed_at IS NULL ORDER BY kind DESC, created_at, alias",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (project_id, alias, display_name, kind, local_note, created_at) = row?;
        out.push(ProjectView {
            preferences: preferences_for(conn, &project_id)?,
            project_id,
            alias,
            display_name,
            kind,
            local_note,
            created_at,
        });
    }
    Ok(out)
}

struct ProjectRef {
    project_id: String,
    kind: String,
    removed: bool,
}

fn find_project(conn: &Connection, alias: &str) -> AppResult<Option<ProjectRef>> {
    Ok(conn
        .query_row("SELECT project_id, kind, removed_at IS NOT NULL FROM project_registry WHERE alias = ?1", [alias], |r| {
            Ok(ProjectRef { project_id: r.get(0)?, kind: r.get(1)?, removed: r.get::<_, i64>(2)? == 1 })
        })
        .optional()?)
}

fn live_project(conn: &Connection, alias: &str) -> AppResult<ProjectRef> {
    match find_project(conn, alias)? {
        Some(p) if !p.removed => Ok(p),
        _ => Err(AppError::new("PROJECT_NOT_FOUND", "Böyle bir proje kaydı yok.")),
    }
}

/// Does this alias name a registered, not removed project? (Used before any approval can bind to a project.)
pub fn is_registered(conn: &Connection, alias: &str) -> bool {
    live_project(conn, alias).is_ok()
}

/// Is this alias a registered, not removed *reference* project (not the built-in sample, which has no folder)?
pub fn is_registered_reference(conn: &Connection, alias: &str) -> bool {
    live_project(conn, alias).is_ok_and(|p| p.kind == "reference")
}

fn clean_text(raw: &str, max: usize) -> String {
    let cleaned: String = raw.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max).collect()
}

/// Registers a project as a *reference*: an alias plus an optional private note. Nothing is read from disk.
pub fn add_reference(conn: &Connection, alias: &str, display_name: &str, note: &str) -> AppResult<ProjectView> {
    if !valid_alias(alias) {
        return Err(AppError::new(
            "PROJECT_ALIAS_INVALID",
            "Takma ad 2-42 karakter olmalı; yalnızca küçük harf, rakam ve tire kullanılabilir (yol, boşluk veya özel karakter yok).",
        ));
    }
    let name = clean_text(display_name, 80);
    if name.is_empty() {
        return Err(AppError::new("PROJECT_NAME_REQUIRED", "Proje için bir ad yazın."));
    }
    let note = clean_text(note, 200);
    let now = now_utc();
    let project_id = match find_project(conn, alias)? {
        Some(p) if p.removed && p.kind == "reference" => {
            conn.execute(
                "UPDATE project_registry SET removed_at = NULL, display_name = ?2, local_note = ?3 WHERE project_id = ?1",
                rusqlite::params![p.project_id, name, note],
            )?;
            p.project_id
        }
        Some(_) => return Err(AppError::new("PROJECT_ALIAS_TAKEN", "Bu takma ad zaten kullanılıyor.")),
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO project_registry (project_id, alias, display_name, kind, local_note, created_at) VALUES (?1, ?2, ?3, 'reference', ?4, ?5)",
                rusqlite::params![id, alias, name, note, now],
            )?;
            id
        }
    };
    // Only the alias is logged; the private note never leaves the registry row.
    diagnostics::log(conn, "info", "projects", "PROJECT_REFERENCE_ADDED", alias)?;
    list_projects(conn)?
        .into_iter()
        .find(|p| p.project_id == project_id)
        .ok_or_else(|| AppError::new("PROJECT_NOT_FOUND", "Böyle bir proje kaydı yok."))
}

/// Removes the reference from the list. The folder it may point to is never touched.
pub fn remove_reference(conn: &Connection, alias: &str) -> AppResult<()> {
    let project = live_project(conn, alias)?;
    if project.kind != "reference" {
        return Err(AppError::new("PROJECT_NOT_REMOVABLE", "Örnek projeler kaldırılamaz."));
    }
    conn.execute("UPDATE project_registry SET removed_at = ?2 WHERE project_id = ?1", rusqlite::params![project.project_id, now_utc()])?;
    diagnostics::log(conn, "info", "projects", "PROJECT_REFERENCE_REMOVED", alias)?;
    Ok(())
}

/// What the engine needs to know about an earlier decision.
#[derive(Debug, Clone)]
pub struct StoredPreference {
    pub decision: String,
    pub remind_at: Option<String>,
    pub capability_fingerprint: String,
}

pub fn stored_preference(
    conn: &Connection,
    alias: &str,
    entry_id: &str,
    surface: &str,
    platform: &str,
) -> AppResult<Option<StoredPreference>> {
    Ok(conn
        .query_row(
            "SELECT p.decision, p.remind_at, p.capability_fingerprint FROM project_preferences p
             JOIN project_registry r ON r.project_id = p.project_id
             WHERE r.alias = ?1 AND p.entry_id = ?2 AND p.agent_surface = ?3 AND p.platform_target = ?4",
            rusqlite::params![alias, entry_id, surface, platform],
            |r| Ok(StoredPreference { decision: r.get(0)?, remind_at: r.get(1)?, capability_fingerprint: r.get(2)? }),
        )
        .optional()?)
}

fn view_of(conn: &Connection, alias: &str, entry_id: &str, surface: &str, platform: &str) -> AppResult<PreferenceView> {
    let project = live_project(conn, alias)?;
    preferences_for(conn, &project.project_id)?
        .into_iter()
        .find(|p| p.entry_id == entry_id && p.agent_surface == surface && p.platform_target == platform)
        .ok_or_else(|| AppError::new("PREFERENCE_NOT_FOUND", "Bu kombinasyon için kayıtlı bir karar yok."))
}

pub fn set_preference(
    conn: &Connection,
    alias: &str,
    entry_id: &str,
    surface: &str,
    platform: &str,
    decision: &str,
    remind_days: Option<i64>,
) -> AppResult<PreferenceView> {
    let project = live_project(conn, alias)?;
    let surface = static_of(SURFACES, surface).ok_or_else(|| AppError::new("SURFACE_INVALID", "Bilinmeyen ajan yüzeyi."))?;
    let platform = static_of(PLATFORMS, platform).ok_or_else(|| AppError::new("PLATFORM_INVALID", "Bilinmeyen platform."))?;
    let decision = static_of(DECISIONS, decision).ok_or_else(|| AppError::new("DECISION_INVALID", "Bilinmeyen karar."))?;
    let entry: CatalogEntry = catalog::get_entry(conn, entry_id)?;
    if entry.removed_at.is_some() {
        return Err(AppError::new("ENTRY_REMOVED", "Bu kayıt katalogdan kaldırılmış."));
    }
    if decision == "accepted" {
        // "Accepted" means "I want this enabled for this project". It is only allowed where that could
        // ever be honoured; it still installs nothing and never proves a tool works.
        if entry.execution_policy == "blocked-until-scoped-authorization" {
            return Err(AppError::new(
                "ACTIVATION_BLOCKED",
                "Bu araç, kapsamlı yazılı yetki olmadan etkinleştirilemez; yalnızca bilgi olarak görülebilir.",
            ));
        }
        if !entry.enabled {
            return Err(AppError::new("ENTRY_DISABLED", "Önce kataloğda bu kaydı etkinleştirin."));
        }
        let compat = catalog::compatibility(&entry, platform, surface);
        if !matches!(compat.as_str(), "vendor-declared" | "independently-tested") {
            return Err(AppError::new(
                "PREFERENCE_NOT_ACTIVATABLE",
                "Bu platform ve ajan birleşiminde uyumluluk bilinmiyor veya desteklenmiyor; etkinleştirme kararı kaydedilemez. 'Şimdi değil' veya 'Reddet' seçebilirsiniz.",
            ));
        }
    }
    let now = now_utc();
    let remind_at = (decision == "postponed").then(|| {
        let days = remind_days.unwrap_or(DEFAULT_POSTPONE_DAYS).clamp(1, 365);
        (chrono::Utc::now() + chrono::Duration::days(days)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    });
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO project_preferences
         (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint, remind_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(project_id, entry_id, agent_surface, platform_target) DO UPDATE SET
           decision = excluded.decision, decided_at = excluded.decided_at, catalog_revision = excluded.catalog_revision,
           capability_fingerprint = excluded.capability_fingerprint, remind_at = excluded.remind_at",
        rusqlite::params![
            project.project_id,
            entry.entry_id,
            surface,
            platform,
            decision,
            now,
            entry.revision,
            entry.capability_fingerprint,
            remind_at
        ],
    )?;
    tx.execute(
        "INSERT INTO preference_history (project_id, entry_id, agent_surface, platform_target, action, at, catalog_revision)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![project.project_id, entry.entry_id, surface, platform, decision, now, entry.revision],
    )?;
    // Schema-valid audit events exist for rejection and deferral; the others are in preference_history.
    let audit_type = match decision {
        "rejected" => Some("reject"),
        "postponed" => Some("defer"),
        _ => None,
    };
    if let Some(event_type) = audit_type {
        let mut event = NewEvent::plain(event_type, "passed", "project-preference", None, "not-applicable", "user");
        event.tool_id = Some(entry.entry_id.clone());
        event.project_alias = Some(alias.to_string());
        event.target_id = Some(audit_target(platform, surface));
        audit::append(&tx, &event)?;
    }
    tx.commit()?;
    view_of(conn, alias, &entry.entry_id, surface, platform)
}

/// Explicit reset by the user: forgets the decision so the tool may be suggested again.
pub fn reset_preference(conn: &Connection, alias: &str, entry_id: &str, surface: &str, platform: &str) -> AppResult<()> {
    let project = live_project(conn, alias)?;
    let surface = static_of(SURFACES, surface).ok_or_else(|| AppError::new("SURFACE_INVALID", "Bilinmeyen ajan yüzeyi."))?;
    let platform = static_of(PLATFORMS, platform).ok_or_else(|| AppError::new("PLATFORM_INVALID", "Bilinmeyen platform."))?;
    let previous = stored_preference(conn, alias, entry_id, surface, platform)?
        .ok_or_else(|| AppError::new("PREFERENCE_NOT_FOUND", "Bu kombinasyon için kayıtlı bir karar yok."))?;
    let revision: String = conn.query_row("SELECT revision FROM catalog_entries WHERE entry_id = ?1", [entry_id], |r| r.get(0))?;
    let now = now_utc();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM project_preferences WHERE project_id = ?1 AND entry_id = ?2 AND agent_surface = ?3 AND platform_target = ?4",
        rusqlite::params![project.project_id, entry_id, surface, platform],
    )?;
    tx.execute(
        "INSERT INTO preference_history (project_id, entry_id, agent_surface, platform_target, action, at, catalog_revision)
         VALUES (?1, ?2, ?3, ?4, 'reset', ?5, ?6)",
        rusqlite::params![project.project_id, entry_id, surface, platform, now, revision],
    )?;
    if previous.decision == "rejected" {
        let mut event = NewEvent::plain("reset_rejection", "passed", "project-preference", None, "not-applicable", "user");
        event.tool_id = Some(entry_id.to_string());
        event.project_alias = Some(alias.to_string());
        event.target_id = Some(audit_target(platform, surface));
        audit::append(&tx, &event)?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> Connection {
        let conn = crate::db::open_memory().unwrap();
        catalog::seed_all(&conn).unwrap();
        conn
    }

    fn add_project(conn: &Connection, alias: &str) {
        add_reference(conn, alias, "Benim sitem", "").unwrap();
    }

    #[test]
    fn real_project_is_only_a_reference_and_its_note_stays_private() {
        let conn = seeded();
        let v = add_reference(&conn, "benim-sitem", "  Benim\nSitem ", "C:\\Users\\dummy\\sites\\benim token=DUMMYSECRET1").unwrap();
        assert_eq!((v.kind.as_str(), v.display_name.as_str()), ("reference", "Benim Sitem"));
        assert!(v.preferences.is_empty());
        // Never in diagnostics, never in the audit chain.
        let diags = diagnostics::list(&conn, 100).unwrap();
        assert!(diags.iter().any(|d| d.code == "PROJECT_REFERENCE_ADDED" && d.detail == "benim-sitem"));
        let dump = format!("{diags:?}{:?}", audit::list_ascending(&conn).unwrap());
        assert!(!dump.contains("dummy") && !dump.contains("DUMMYSECRET1"), "{dump}");
        assert_eq!(list_projects(&conn).unwrap().len(), 4);
    }

    #[test]
    fn aliases_cannot_be_paths_or_look_alikes() {
        let conn = seeded();
        for bad in ["", "a", "A-b", "C:\\x", "../x", "a/b", "a b", "-ab", "ab-", "ünlü", "a_b", "x".repeat(43).as_str(), "ab\n", "ab.cd"]
        {
            assert_eq!(add_reference(&conn, bad, "x", "").unwrap_err().code, "PROJECT_ALIAS_INVALID", "{bad:?}");
        }
        assert_eq!(add_reference(&conn, "ab", "   ", "").unwrap_err().code, "PROJECT_NAME_REQUIRED");
        add_project(&conn, "proje-bir");
        assert_eq!(add_reference(&conn, "proje-bir", "x", "").unwrap_err().code, "PROJECT_ALIAS_TAKEN");
        assert_eq!(
            add_reference(&conn, "ornek-web-sitesi", "x", "").unwrap_err().code,
            "PROJECT_ALIAS_TAKEN",
            "synthetic alias is reserved"
        );
    }

    #[test]
    fn reference_can_be_removed_and_restored_but_samples_cannot_be_removed() {
        let conn = seeded();
        add_project(&conn, "proje-bir");
        assert_eq!(remove_reference(&conn, "ornek-web-sitesi").unwrap_err().code, "PROJECT_NOT_REMOVABLE");
        remove_reference(&conn, "proje-bir").unwrap();
        assert!(list_projects(&conn).unwrap().iter().all(|p| p.alias != "proje-bir"));
        assert_eq!(
            set_preference(&conn, "proje-bir", "gitleaks", "local-cli", "windows-x64", "rejected", None).unwrap_err().code,
            "PROJECT_NOT_FOUND"
        );
        add_reference(&conn, "proje-bir", "Yeniden", "").unwrap();
        assert!(list_projects(&conn).unwrap().iter().any(|p| p.alias == "proje-bir"));
    }

    #[test]
    fn decision_records_time_revision_and_keeps_desired_apart_from_actual() {
        let conn = seeded();
        add_project(&conn, "proje-bir");
        let p = set_preference(&conn, "proje-bir", "anthropic-frontend-design", "claude-code-terminal", "windows-x64", "accepted", None)
            .unwrap();
        assert_eq!(p.decision, "accepted");
        assert!(!p.decided_at.is_empty() && p.catalog_revision.contains('@') && !p.catalog_revision.starts_with("unknown@"));
        // Desired state says "accepted"; the actual state is separate and still not installed / unverified / scope unknown.
        assert_eq!(
            (p.actual_install_state.as_str(), p.actual_verification_state.as_str(), p.observed_scope.as_str()),
            ("not_managed", "unknown", "unknown")
        );
        assert_eq!(p.compatibility, "vendor-declared");
        assert!(!p.revision_changed && !p.material_change);
    }

    #[test]
    fn accepting_requires_known_compatibility_an_enabled_entry_and_no_execution_block() {
        let conn = seeded();
        add_project(&conn, "proje-bir");
        let accept =
            |entry: &str, surface: &str, platform: &str| set_preference(&conn, "proje-bir", entry, surface, platform, "accepted", None);
        // Plugin unavailable on Antigravity IDE: leads to an explanation, never to an installer.
        assert_eq!(accept("anthropic-frontend-design", "antigravity-ide", "windows-x64").unwrap_err().code, "PREFERENCE_NOT_ACTIVATABLE");
        // Unknown OS/surface combination.
        assert_eq!(accept("anthropic-frontend-design", "claude-code-terminal", "linux-x64").unwrap_err().code, "PLATFORM_INVALID");
        assert_eq!(accept("anthropic-frontend-design", "vscode", "windows-x64").unwrap_err().code, "SURFACE_INVALID");
        // agent-browser has only vendor-declared CLI evidence, nothing for Claude Code.
        assert_eq!(accept("agent-browser", "claude-code-terminal", "windows-x64").unwrap_err().code, "PREFERENCE_NOT_ACTIVATABLE");
        assert!(accept("agent-browser", "local-cli", "windows-x64").is_ok());
        // Strix can never be "accepted" (blocked until scoped authorization), whatever else is true.
        catalog::set_enabled(&conn, "strix", true).unwrap();
        assert_eq!(accept("strix", "local-cli", "windows-x64").unwrap_err().code, "ACTIVATION_BLOCKED");
        // Disabled catalog entry.
        catalog::set_enabled(&conn, "anthropic-frontend-design", false).unwrap();
        assert_eq!(accept("anthropic-frontend-design", "claude-code-terminal", "windows-x64").unwrap_err().code, "ENTRY_DISABLED");
        // Rejecting / postponing is always possible, even where acceptance is not.
        assert!(set_preference(&conn, "proje-bir", "strix", "local-cli", "windows-x64", "rejected", None).is_ok());
        assert!(
            set_preference(&conn, "proje-bir", "anthropic-frontend-design", "antigravity-ide", "windows-x64", "postponed", Some(3)).is_ok()
        );
        assert_eq!(
            set_preference(&conn, "proje-bir", "x", "local-cli", "windows-x64", "rejected", None).unwrap_err().code,
            "ENTRY_NOT_FOUND"
        );
        assert_eq!(
            set_preference(&conn, "proje-bir", "gitleaks", "local-cli", "windows-x64", "installed", None).unwrap_err().code,
            "DECISION_INVALID"
        );
        assert_eq!(
            set_preference(&conn, "proje-bir", "gitleaks", "local-cli", "windows-x64", "verified", None).unwrap_err().code,
            "DECISION_INVALID"
        );
    }

    #[test]
    fn decisions_are_unique_per_key_updatable_and_appended_to_history() {
        let conn = seeded();
        add_project(&conn, "proje-bir");
        let key = ("anthropic-frontend-design", "claude-code-terminal", "windows-x64");
        set_preference(&conn, "proje-bir", key.0, key.1, key.2, "rejected", None).unwrap();
        set_preference(&conn, "proje-bir", key.0, key.1, key.2, "accepted", None).unwrap();
        let rows: i64 = conn
            .query_row(
                "SELECT count(*) FROM project_preferences p JOIN project_registry r USING (project_id) WHERE r.alias='proje-bir'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, 1);
        let history: i64 = conn
            .query_row(
                "SELECT count(*) FROM preference_history h JOIN project_registry r USING (project_id) WHERE r.alias='proje-bir'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(history, 2);
        assert!(conn.execute("UPDATE preference_history SET action='accepted'", []).is_err());
        assert!(conn.execute("DELETE FROM preference_history", []).is_err());
        // Same tool on another platform is an independent decision.
        set_preference(&conn, "proje-bir", key.0, key.1, "macos-arm64", "rejected", None).unwrap();
        assert_eq!(list_projects(&conn).unwrap().iter().find(|p| p.alias == "proje-bir").unwrap().preferences.len(), 2);
    }

    #[test]
    fn reject_and_defer_events_conform_to_the_audit_contract_and_reset_is_audited() {
        let conn = seeded();
        add_project(&conn, "proje-bir");
        set_preference(&conn, "proje-bir", "agent-browser", "local-cli", "windows-x64", "rejected", None).unwrap();
        set_preference(&conn, "proje-bir", "agentic-awesome-skills", "claude-code-terminal", "windows-x64", "postponed", None).unwrap();
        reset_preference(&conn, "proje-bir", "agent-browser", "local-cli", "windows-x64").unwrap();
        let events = audit::list_ascending(&conn).unwrap();
        let kinds: Vec<&str> = events.iter().map(|e| e["eventType"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["reject", "defer", "reset_rejection"]);
        for e in &events {
            assert_eq!(e["projectAlias"], "proje-bir");
            assert_eq!(e["actorType"], "user");
            assert!(e["toolId"].is_string());
        }
        assert_eq!(events[0]["targetId"], "unknown", "local-cli has no audit target id");
        assert_eq!(events[1]["targetId"], "windows-x64-claude-code");
        assert!(audit::verify(&conn).unwrap().ok);
        // Reset of a non-rejection decision is history-only; resetting a missing decision is an error.
        reset_preference(&conn, "proje-bir", "agentic-awesome-skills", "claude-code-terminal", "windows-x64").unwrap();
        assert_eq!(audit::list_ascending(&conn).unwrap().len(), 3);
        assert_eq!(
            reset_preference(&conn, "proje-bir", "agent-browser", "local-cli", "windows-x64").unwrap_err().code,
            "PREFERENCE_NOT_FOUND"
        );
        let actions: Vec<String> = conn
            .prepare("SELECT action FROM preference_history ORDER BY id DESC LIMIT 2")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(actions, ["reset", "reset"]);
    }

    #[test]
    fn postponed_decision_gets_a_bounded_reminder_date() {
        let conn = seeded();
        add_project(&conn, "proje-bir");
        let p = set_preference(&conn, "proje-bir", "gitleaks", "local-cli", "windows-x64", "postponed", Some(100_000)).unwrap();
        let remind = chrono::DateTime::parse_from_rfc3339(p.remind_at.as_deref().unwrap()).unwrap();
        let days = (remind.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_days();
        assert!((364..=365).contains(&days), "{days}");
        let r = set_preference(&conn, "proje-bir", "gitleaks", "local-cli", "windows-x64", "rejected", None).unwrap();
        assert!(r.remind_at.is_none());
    }

    #[test]
    fn catalog_revision_bump_is_informational_but_capability_change_is_flagged() {
        let conn = seeded();
        add_project(&conn, "proje-bir");
        set_preference(&conn, "proje-bir", "agent-browser", "local-cli", "windows-x64", "rejected", None).unwrap();
        conn.execute("UPDATE catalog_entries SET revision = 'v9.9.9@abcdef' WHERE entry_id = 'agent-browser'", []).unwrap();
        let p = &list_projects(&conn).unwrap().into_iter().find(|p| p.alias == "proje-bir").unwrap().preferences[0].clone();
        assert!(p.revision_changed && !p.material_change);
        conn.execute("UPDATE catalog_entries SET capability_fingerprint = ?1 WHERE entry_id = 'agent-browser'", ["f".repeat(64)]).unwrap();
        let p = &list_projects(&conn).unwrap().into_iter().find(|p| p.alias == "proje-bir").unwrap().preferences[0].clone();
        assert!(p.material_change);
    }
}
