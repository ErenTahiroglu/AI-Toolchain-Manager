// Phase 3D-4: the jq pilot configuration is Windows-only and separate from the synthetic one. Both are refused at compile time otherwise.
#[cfg(all(feature = "jq-pilot", not(windows)))]
compile_error!("the `jq-pilot` feature is Windows-only: it supplies the jq 1.8.2 windows-x64 pilot and nothing else");
#[cfg(all(feature = "jq-pilot", feature = "synthetic-pilot"))]
compile_error!("`jq-pilot` and `synthetic-pilot` are separate pilot configurations and must not be combined");

mod adapter;
#[cfg(test)]
mod agent01_tests;
mod approval;
mod approval_store;
mod archive_check;
mod artifact_provider;
mod attestation;
mod audit;
mod backups;
mod catalog;
#[cfg(test)]
mod catalog_c01_tests;
#[cfg(test)]
mod catalog_c02_tests;
#[cfg(test)]
mod catalog_c03_tests;
#[cfg(test)]
mod closure_tests;
#[cfg(windows)]
mod cng_verify;
mod commands;
#[cfg(test)]
mod config02c_support;
#[cfg(test)]
mod config_txn;
mod consent;
mod crash;
mod db;
mod device_trust;
mod diagnostics;
mod discovery;
mod discovery_probe;
mod error;
mod gate;
mod github_url;
mod health;
mod install_flow;
mod lifecycle;
mod managed_fs;
mod managed_fs_probe;
mod managed_root;
mod mcp_read;
mod os_approval;
mod os_hello;
mod pilot_allowance;
mod plan;
mod projects;
mod recipe;
mod recommend;
mod redact;
mod selfcheck;
mod settings;
mod skills;
mod state;
mod synthetic;

use audit::NewEvent;
use health::{PreviousShutdown, TrayState};
use state::AppState;
use std::{
    path::PathBuf,
    sync::{atomic::AtomicU32, Mutex},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, RunEvent, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;

type BoxError = Box<dyn std::error::Error>;

const SESSION_KEY: &str = "_session_state";

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// `ATM_DATA_DIR` lets tests and CI use an isolated directory; otherwise the OS per-user app-data dir.
fn resolve_data_dir(app: &AppHandle) -> Result<PathBuf, BoxError> {
    if let Some(dir) = std::env::var_os("ATM_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    Ok(app.path().app_data_dir()?)
}

fn audit_event(conn: &rusqlite::Connection, outcome: &'static str, check_id: &'static str, code: Option<&'static str>) {
    let _ = audit::append(conn, &NewEvent::plain("health_check", outcome, check_id, code, "operational-health", "system"));
}

fn build_tray(app: &tauri::App) -> Result<(), BoxError> {
    let open = MenuItem::with_id(app, "show", "Pencereyi aç", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Çık", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let icon = app.default_window_icon().cloned().ok_or("uygulama simgesi bulunamadı")?;
    TrayIconBuilder::with_id("main")
        .tooltip("AI Toolchain Manager")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn setup(app: &mut tauri::App) -> Result<(), BoxError> {
    let data_dir = resolve_data_dir(app.handle())?;
    std::fs::create_dir_all(&data_dir)?;
    crash::set_data_dir(data_dir.clone());

    let (conn, startup_error) = match db::open_file(&data_dir.join("atm.sqlite"), Some(&data_dir.join("backups"))) {
        Ok(conn) => (Some(conn), None),
        Err(e) => (None, Some(e)),
    };

    let mut previous_shutdown = PreviousShutdown::FirstRun;
    if let Some(conn) = &conn {
        if let Err(e) = catalog::seed_all(conn) {
            let _ = diagnostics::log(conn, "error", "catalog", e.code, "Tohum verisi yazılamadı.");
        }
        previous_shutdown = match settings::get_internal(conn, SESSION_KEY).ok().flatten().as_deref() {
            Some("clean") => PreviousShutdown::Clean,
            Some(_) => PreviousShutdown::Unclean,
            None => PreviousShutdown::FirstRun,
        };
        if let Some(text) = crash::take_pending(&data_dir) {
            let _ = diagnostics::log(conn, "error", "crash", "PREVIOUS_CRASH_RECORDED", &text);
            audit_event(conn, "failed", "previous-crash", Some("PANIC_RECORDED"));
        }
        if previous_shutdown == PreviousShutdown::Unclean {
            let _ = diagnostics::log(conn, "warn", "lifecycle", "UNCLEAN_SHUTDOWN", "Önceki oturum düzgün kapanmadı.");
        }
        let _ = settings::set_internal(conn, SESSION_KEY, "running");
        audit_event(conn, "passed", "app-start", None);
    } else if let Some(e) = &startup_error {
        // No database: keep the failure observable in a file the next start can still ingest.
        crash::record_fatal(&data_dir, e.code);
    }

    let install = install_flow::InstallService::for_app(data_dir.clone());
    if let Some(conn) = &conn {
        install.startup(conn);
    }
    app.manage(AppState {
        db: Mutex::new(conn),
        startup_error,
        data_dir: data_dir.to_string_lossy().into_owned(),
        tray: Mutex::new(TrayState::NotAttempted),
        previous_shutdown,
        last_report: Mutex::new(None),
        ui_error_count: AtomicU32::new(0),
        install,
    });
    let state = app.state::<AppState>();

    let tray_ok = match build_tray(app) {
        Ok(()) => true,
        Err(e) => {
            let _ = state.with_db(|c| diagnostics::log(c, "error", "tray", "TRAY_CREATE_FAILED", &e.to_string()));
            false
        }
    };
    if let Ok(mut t) = state.tray.lock() {
        *t = if tray_ok { TrayState::Created } else { TrayState::Failed };
    }

    // Startup health run: local, deterministic, no network, never installs or changes a tool.
    let ctx = commands::health_context(app.handle(), &state);
    if let Ok(report) = state.with_db(|c| health::run_and_record(c, &ctx)) {
        if let Ok(mut slot) = state.last_report.lock() {
            *slot = Some(report);
        }
    }

    // Started by the OS at login with --minimized: stay in the tray, but only if a tray exists.
    let minimized = std::env::args().any(|a| a == "--minimized");
    if !(minimized && tray_ok) {
        show_main_window(app.handle());
    }
    Ok(())
}

fn mark_clean_exit(app: &AppHandle) {
    let state = app.state::<AppState>();
    let _ = state.with_db(|c| {
        audit_event(c, "passed", "app-shutdown", None);
        settings::set_internal(c, SESSION_KEY, "clean")
    });
}

pub fn run() {
    if let Some(code) = selfcheck::maybe_run() {
        std::process::exit(code);
    }
    if let Some(code) = discovery_probe::maybe_discover_local() {
        std::process::exit(code);
    }
    crash::install_hook();

    let built = tauri::Builder::default()
        // Must be registered first so a second launch is intercepted before anything else runs.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main_window(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .setup(setup)
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                let hide = state.tray_state() == TrayState::Created
                    && state.with_db(|c| settings::get(c, "close_to_tray")).map(|v| v == "true").unwrap_or(false);
                if hide {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_dashboard,
            commands::list_catalog,
            commands::preview_github_url,
            commands::add_catalog_entry,
            commands::set_catalog_entry_enabled,
            commands::remove_catalog_entry,
            commands::restore_catalog_entry,
            commands::explain_execution_block,
            commands::list_projects,
            commands::add_project_reference,
            commands::remove_project_reference,
            commands::set_project_preference,
            commands::reset_project_preference,
            commands::list_scenarios,
            commands::evaluate_task,
            commands::run_discovery_scan,
            commands::list_inventory,
            commands::read_project_mcp_config,
            commands::preview_backup_cleanup,
            commands::plan_install,
            commands::list_install_states,
            commands::prepare_install,
            commands::cancel_install,
            commands::confirm_install,
            commands::get_installed_details,
            commands::recover_installations,
            commands::list_prompt_sources,
            commands::run_health_checks,
            commands::list_audit_events,
            commands::verify_audit_chain,
            commands::list_diagnostics,
            commands::get_settings,
            commands::set_setting,
            commands::get_startup_state,
            commands::set_startup_enabled,
            commands::report_ui_error,
        ])
        .build(tauri::generate_context!());

    match built {
        Ok(app) => app.run(|handle, event| {
            if let RunEvent::Exit = event {
                mark_clean_exit(handle);
            }
        }),
        Err(e) => {
            // For example: WebView2 missing. Leave a redacted marker for the next start.
            if let Some(dir) = std::env::var_os("ATM_DATA_DIR").map(PathBuf::from) {
                std::fs::create_dir_all(&dir).ok();
                crash::record_fatal(&dir, &e.to_string());
            }
            eprintln!("{}", redact::sanitize(&e.to_string(), 300));
            std::process::exit(1);
        }
    }
}
