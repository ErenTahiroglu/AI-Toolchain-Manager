//! The only functions the WebView can call. Each is listed in build.rs and granted individually in
//! capabilities/default.json. Inputs are validated here; errors are fixed sentences without paths.

use crate::{
    audit::{self, ChainReport, NewEvent},
    catalog::{self, ProjectRow, PromptSourceRow, ToolRow},
    db, diagnostics,
    error::{AppError, AppResult},
    health::{self, AutostartState, HealthContext, HealthReport},
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
        phase: "1A",
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
    tool_count: i64,
    unreviewed_count: i64,
    installed_count: i64,
    validated_count: i64,
    sample_project_count: i64,
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
            tool_count: n("SELECT count(*) FROM tools")?,
            unreviewed_count: n("SELECT count(*) FROM tools WHERE review_state = 'unreviewed'")?,
            installed_count: n("SELECT count(*) FROM tools WHERE install_state <> 'not_managed'")?,
            validated_count: n("SELECT count(*) FROM tools WHERE validation_state = 'passed'")?,
            sample_project_count: n("SELECT count(*) FROM projects WHERE is_synthetic = 1")?,
            prompt_source_count: n("SELECT count(*) FROM prompt_sources")?,
            audit_event_count: chain.count,
            audit_chain: chain,
            last_report,
        })
    })
}

#[tauri::command]
pub fn list_tools(state: State<'_, AppState>) -> AppResult<Vec<ToolRow>> {
    state.with_db(catalog::list_tools)
}

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> AppResult<Vec<ProjectRow>> {
    state.with_db(catalog::list_projects)
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
            &NewEvent {
                event_type: "health_check",
                outcome: "failed",
                check_id: "ui-error",
                error_code: Some("UI_UNHANDLED_ERROR"),
                verdict_source: "operational-health",
                actor_type: "system",
            },
        )?;
        Ok(())
    })
}
