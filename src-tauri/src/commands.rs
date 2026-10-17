//! The only functions the WebView can call. Each is listed in build.rs and granted individually in
//! capabilities/default.json. Inputs are validated here; errors are fixed sentences without paths.

use crate::{
    audit::{self, ChainReport, NewEvent},
    catalog::{self, CatalogEntry, PromptSourceRow, UrlPreview},
    db, diagnostics,
    discovery::{
        self,
        detect::Roots,
        store::{Inventory, ScanSummary},
        ScanRequest,
    },
    error::{AppError, AppResult},
    health::{self, AutostartState, HealthContext, HealthReport},
    projects::{self, PreferenceView, ProjectView},
    recommend::{self, Evaluation, Scenario, TaskInput},
    redact, settings,
    state::AppState,
};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, sync::atomic::Ordering};
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

const MAX_UI_ERRORS_PER_SESSION: u32 = 50;

fn autostart_state(app: &AppHandle) -> AutostartState {
    match app.autolaunch().is_enabled() {
        Ok(true) => AutostartState::Enabled,
        Ok(false) => AutostartState::Disabled,
        Err(_) => AutostartState::QueryFailed,
    }
}

pub fn health_context(app: &AppHandle, state: &AppState) -> HealthContext {
    HealthContext { tray: state.tray_state(), autostart: autostart_state(app), previous_shutdown: state.previous_shutdown }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    phase: &'static str,
    platform: &'static str,
    arch: &'static str,
    db_version: Option<i64>,
    data_dir: String,
    degraded: bool,
    startup_error: Option<AppError>,
}

#[tauri::command]
pub fn get_app_info(state: State<'_, AppState>) -> AppResult<AppInfo> {
    let db_version = state.with_db(db::user_version).ok();
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        phase: "1B",
        platform: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        db_version,
        data_dir: state.data_dir.clone(),
        degraded: state.startup_error.is_some() || db_version.is_none(),
        startup_error: state.startup_error.clone(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    catalog_count: i64,
    catalog_enabled_count: i64,
    unreviewed_count: i64,
    installed_count: i64,
    validated_count: i64,
    sample_project_count: i64,
    reference_project_count: i64,
    preference_count: i64,
    prompt_source_count: i64,
    audit_event_count: i64,
    audit_chain: ChainReport,
    last_report: Option<HealthReport>,
}

#[tauri::command]
pub fn get_dashboard(state: State<'_, AppState>) -> AppResult<Dashboard> {
    let last_report = state.last_report.lock().ok().and_then(|g| g.clone());
    state.with_db(|c| {
        let n = |sql: &str| -> AppResult<i64> { Ok(c.query_row(sql, [], |r| r.get(0))?) };
        let chain = audit::verify(c)?;
        Ok(Dashboard {
            catalog_count: n("SELECT count(*) FROM catalog_entries WHERE removed_at IS NULL")?,
            catalog_enabled_count: n("SELECT count(*) FROM catalog_entries WHERE removed_at IS NULL AND enabled = 1")?,
            unreviewed_count: n("SELECT count(*) FROM catalog_entries WHERE removed_at IS NULL AND review_state = 'unreviewed'")?,
            installed_count: n("SELECT count(*) FROM catalog_entries WHERE install_state <> 'not_managed'")?,
            validated_count: n("SELECT count(*) FROM catalog_entries WHERE verification_state = 'passed'")?,
            sample_project_count: n("SELECT count(*) FROM project_registry WHERE removed_at IS NULL AND kind = 'synthetic'")?,
            reference_project_count: n("SELECT count(*) FROM project_registry WHERE removed_at IS NULL AND kind = 'reference'")?,
            preference_count: n("SELECT count(*) FROM project_preferences")?,
            prompt_source_count: n("SELECT count(*) FROM prompt_sources")?,
            audit_event_count: chain.count,
            audit_chain: chain,
            last_report,
        })
    })
}

#[tauri::command]
pub fn list_catalog(state: State<'_, AppState>) -> AppResult<Vec<CatalogEntry>> {
    state.with_db(catalog::list_catalog)
}

/// Offline and side-effect free: validates a GitHub link and reports duplicates. Fetches nothing.
#[tauri::command]
pub fn preview_github_url(state: State<'_, AppState>, url: String) -> AppResult<UrlPreview> {
    state.with_db(|c| catalog::preview_url(c, &url))
}

#[tauri::command]
pub fn add_catalog_entry(state: State<'_, AppState>, url: String, kind: String, purpose: String) -> AppResult<CatalogEntry> {
    state.with_db(|c| catalog::add_entry(c, &url, &kind, &purpose))
}

#[tauri::command]
pub fn set_catalog_entry_enabled(state: State<'_, AppState>, entry_id: String, enabled: bool) -> AppResult<CatalogEntry> {
    state.with_db(|c| catalog::set_enabled(c, &entry_id, enabled))
}

/// Removes the catalog mapping only. Nothing is uninstalled; nothing was ever installed.
#[tauri::command]
pub fn remove_catalog_entry(state: State<'_, AppState>, entry_id: String) -> AppResult<CatalogEntry> {
    state.with_db(|c| catalog::remove_entry(c, &entry_id))
}

#[tauri::command]
pub fn restore_catalog_entry(state: State<'_, AppState>, entry_id: String) -> AppResult<CatalogEntry> {
    state.with_db(|c| catalog::restore_entry(c, &entry_id))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionBlock {
    code: &'static str,
    message: String,
}

/// Tells the UI why "install" / "run" is unavailable, from the single place that decides it. Always blocked in Phase 1B.
#[tauri::command]
pub fn explain_execution_block(state: State<'_, AppState>, entry_id: String) -> AppResult<ExecutionBlock> {
    state.with_db(|c| {
        let entry = catalog::get_entry(c, &entry_id)?;
        match crate::gate::check_execution(&entry, "run", None) {
            Err(e) => Ok(ExecutionBlock { code: e.code, message: e.message }),
            // Unreachable by construction; fail closed if it ever happens.
            Ok(()) => Ok(ExecutionBlock { code: "EXECUTION_NOT_AVAILABLE_PHASE_1B", message: "Çalıştırma kullanılamaz.".to_string() }),
        }
    })
}

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> AppResult<Vec<ProjectView>> {
    state.with_db(projects::list_projects)
}

/// Registers a project as a reference (alias + private note). The folder is never opened or scanned.
#[tauri::command]
pub fn add_project_reference(state: State<'_, AppState>, alias: String, display_name: String, note: String) -> AppResult<ProjectView> {
    state.with_db(|c| projects::add_reference(c, &alias, &display_name, &note))
}

#[tauri::command]
pub fn remove_project_reference(state: State<'_, AppState>, alias: String) -> AppResult<()> {
    state.with_db(|c| projects::remove_reference(c, &alias))
}

/// Records the user's *desired* choice. It never installs, enables in a host, or verifies anything.
#[tauri::command]
pub fn set_project_preference(
    state: State<'_, AppState>,
    alias: String,
    entry_id: String,
    agent_surface: String,
    platform_target: String,
    decision: String,
    remind_days: Option<i64>,
) -> AppResult<PreferenceView> {
    state.with_db(|c| projects::set_preference(c, &alias, &entry_id, &agent_surface, &platform_target, &decision, remind_days))
}

#[tauri::command]
pub fn reset_project_preference(
    state: State<'_, AppState>,
    alias: String,
    entry_id: String,
    agent_surface: String,
    platform_target: String,
) -> AppResult<()> {
    state.with_db(|c| projects::reset_preference(c, &alias, &entry_id, &agent_surface, &platform_target))
}

#[tauri::command]
pub fn list_scenarios() -> AppResult<Vec<Scenario>> {
    recommend::scenarios()
}

/// Deterministic, local, zero model calls. Returns suggestions with reasons; changes nothing but the
/// "already shown for this task" log.
#[tauri::command]
pub fn evaluate_task(state: State<'_, AppState>, input: TaskInput) -> AppResult<Evaluation> {
    state.with_db(|c| recommend::evaluate(c, &input))
}

/// Read-only scan of this computer, limited to what the user authorised in `request`. It never executes a
/// program, starts an agent or MCP server, uses the network, or writes outside this app's own database.
#[tauri::command]
pub fn run_discovery_scan(state: State<'_, AppState>, request: ScanRequest) -> AppResult<ScanSummary> {
    state.with_db(|c| {
        let auth = discovery::authorize(c, &request)?;
        discovery::run_scan(c, &Roots::from_environment(), &auth)
    })
}

/// Read-only preview of old pre-migration backups. It never deletes, moves or rewrites a backup.
#[tauri::command]
pub fn preview_backup_cleanup(state: State<'_, AppState>) -> AppResult<crate::backups::CleanupPlan> {
    let dir = std::path::Path::new(&state.data_dir).join("backups");
    state.with_db(|c| crate::backups::cleanup_plan(c, &dir))
}

#[tauri::command]
pub fn list_inventory(state: State<'_, AppState>) -> AppResult<Inventory> {
    state.with_db(discovery::store::inventory)
}

#[tauri::command]
pub fn list_prompt_sources(state: State<'_, AppState>) -> AppResult<Vec<PromptSourceRow>> {
    state.with_db(catalog::list_prompt_sources)
}

#[tauri::command]
pub fn run_health_checks(app: AppHandle, state: State<'_, AppState>) -> AppResult<HealthReport> {
    let ctx = health_context(&app, &state);
    let report = state.with_db(|c| health::run_and_record(c, &ctx))?;
    if let Ok(mut slot) = state.last_report.lock() {
        *slot = Some(report.clone());
    }
    Ok(report)
}

#[tauri::command]
pub fn list_audit_events(state: State<'_, AppState>, limit: Option<i64>) -> AppResult<Vec<Value>> {
    state.with_db(|c| audit::list(c, limit.unwrap_or(50)))
}

#[tauri::command]
pub fn verify_audit_chain(state: State<'_, AppState>) -> AppResult<ChainReport> {
    state.with_db(audit::verify)
}

#[tauri::command]
pub fn list_diagnostics(state: State<'_, AppState>, limit: Option<i64>) -> AppResult<Vec<diagnostics::DiagnosticRow>> {
    state.with_db(|c| diagnostics::list(c, limit.unwrap_or(50)))
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppResult<BTreeMap<String, String>> {
    state.with_db(settings::get_all)
}

#[tauri::command]
pub fn set_setting(state: State<'_, AppState>, key: String, value: String) -> AppResult<BTreeMap<String, String>> {
    if key == "startup_enabled" {
        return Err(AppError::new("SETTING_USE_STARTUP_COMMAND", "Başlangıç tercihi yalnızca özel komutla değiştirilebilir."));
    }
    state.with_db(|c| {
        settings::set(c, &key, &value)?;
        diagnostics::log(c, "info", "settings", "SETTING_CHANGED", &format!("{key}={value}"))?;
        settings::get_all(c)
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupState {
    preference: bool,
    /// `None` when the operating system could not be queried.
    os_enabled: Option<bool>,
}

#[tauri::command]
pub fn get_startup_state(app: AppHandle, state: State<'_, AppState>) -> AppResult<StartupState> {
    let preference = state.with_db(|c| Ok(settings::get(c, "startup_enabled")? == "true"))?;
    let os_enabled = match autostart_state(&app) {
        AutostartState::Enabled => Some(true),
        AutostartState::Disabled => Some(false),
        _ => None,
    };
    Ok(StartupState { preference, os_enabled })
}

/// Only ever called from a user action in Settings. Verifies the OS state after changing it and
/// stores the preference only when the OS actually reflects it.
#[tauri::command]
pub fn set_startup_enabled(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> AppResult<StartupState> {
    let launcher = app.autolaunch();
    let changed = if enabled { launcher.enable() } else { launcher.disable() };
    let log = |level: &str, code: &str, detail: &str| {
        let _ = state.with_db(|c| diagnostics::log(c, level, "startup", code, detail));
    };
    if changed.is_err() {
        log("error", "STARTUP_CHANGE_FAILED", "İşletim sistemi başlangıç kaydı değiştirilemedi.");
        return Err(AppError::new("STARTUP_CHANGE_FAILED", "Başlangıç ayarı işletim sistemine yazılamadı."));
    }
    if launcher.is_enabled().ok() != Some(enabled) {
        log("error", "STARTUP_VERIFY_FAILED", "Değişiklik sonrası işletim sistemi durumu beklenenle uyuşmadı.");
        return Err(AppError::new("STARTUP_VERIFY_FAILED", "Başlangıç ayarı yazıldı ancak işletim sistemi durumu doğrulanamadı."));
    }
    state.with_db(|c| {
        settings::set(c, "startup_enabled", if enabled { "true" } else { "false" })?;
        diagnostics::log(c, "info", "startup", "STARTUP_CHANGED", if enabled { "enabled" } else { "disabled" })
    })?;
    Ok(StartupState { preference: enabled, os_enabled: Some(enabled) })
}

/// Called by the React error boundary and global handlers. The text is untrusted: truncated,
/// redacted and rate-limited before it is stored.
#[tauri::command]
pub fn report_ui_error(state: State<'_, AppState>, message: String) -> AppResult<()> {
    if state.ui_error_count.fetch_add(1, Ordering::Relaxed) >= MAX_UI_ERRORS_PER_SESSION {
        return Ok(());
    }
    let bounded: String = message.chars().take(1000).collect();
    state.with_db(|c| {
        diagnostics::log(c, "error", "ui", "UI_UNHANDLED_ERROR", &redact::sanitize(&bounded, 300))?;
        audit::append(
            c,
            &NewEvent::plain("health_check", "failed", "ui-error", Some("UI_UNHANDLED_ERROR"), "operational-health", "system"),
        )?;
        Ok(())
    })
}
