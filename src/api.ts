import { invoke } from "@tauri-apps/api/core";
import type {
  AppError, AppInfo, AuditEvent, ChainReport, Dashboard, DiagnosticRow, HealthReport,
  ProjectRow, PromptSourceRow, Settings, StartupState, ToolRow,
} from "./types";

/** Normalises whatever the IPC layer throws into the AppError shape the UI renders. */
export function toAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "code" in e && "message" in e) {
    const { code, message } = e as AppError;
    return { code: String(code), message: String(message) };
  }
  return { code: "IPC_FAILED", message: "Uygulama çekirdeğiyle iletişim kurulamadı." };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    throw toAppError(e);
  }
}

export const api = {
  getAppInfo: () => call<AppInfo>("get_app_info"),
  getDashboard: () => call<Dashboard>("get_dashboard"),
  listTools: () => call<ToolRow[]>("list_tools"),
  listProjects: () => call<ProjectRow[]>("list_projects"),
  listPromptSources: () => call<PromptSourceRow[]>("list_prompt_sources"),
  runHealthChecks: () => call<HealthReport>("run_health_checks"),
  listAuditEvents: (limit = 50) => call<AuditEvent[]>("list_audit_events", { limit }),
  verifyAuditChain: () => call<ChainReport>("verify_audit_chain"),
  listDiagnostics: (limit = 50) => call<DiagnosticRow[]>("list_diagnostics", { limit }),
  getSettings: () => call<Settings>("get_settings"),
  setSetting: (key: string, value: string) => call<Settings>("set_setting", { key, value }),
  getStartupState: () => call<StartupState>("get_startup_state"),
  setStartupEnabled: (enabled: boolean) => call<StartupState>("set_startup_enabled", { enabled }),
  reportUiError: (message: string) => call<void>("report_ui_error", { message }),
};

/** Best effort: reporting an error must never throw a second one. */
export function reportError(e: unknown): void {
  const text = e instanceof Error ? `${e.name}: ${e.message}` : String(e);
  api.reportUiError(text.slice(0, 1000)).catch(() => undefined);
}
