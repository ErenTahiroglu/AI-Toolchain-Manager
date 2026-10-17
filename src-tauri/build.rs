// Every command the WebView may call is listed here, so Tauri generates one
// explicit `allow-*` permission per command. Nothing else is callable.
const COMMANDS: &[&str] = &[
    "get_app_info",
    "get_dashboard",
    "list_tools",
    "list_projects",
    "list_prompt_sources",
    "run_health_checks",
    "list_audit_events",
    "verify_audit_chain",
    "list_diagnostics",
    "get_settings",
    "set_setting",
    "get_startup_state",
    "set_startup_enabled",
    "report_ui_error",
];

fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)))
        .expect("failed to run tauri-build");
}
