import { invoke } from "@tauri-apps/api/core";
import type {
  AppError, AppInfo, AuditEvent, CatalogEntry, ChainReport, Dashboard, DiagnosticRow, Evaluation, ExecutionBlock, HealthReport,
  CleanupPlan, InstallAction, InstalledDetails, InstallPlan, InstallPreview, InstallResult, Inventory, PlanRequest, PreferenceView, ProjectView,
  PromptSourceRow, RecoverySummary, ScanRequest, ScanSummary, Scenario, Settings, StartupState, TaskInput, ToolState, UrlPreview,
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

export interface PreferenceKey {
  alias: string;
  entryId: string;
  agentSurface: string;
  platformTarget: string;
}

export const api = {
  getAppInfo: () => call<AppInfo>("get_app_info"),
  getDashboard: () => call<Dashboard>("get_dashboard"),

  // Catalog registry. None of these downloads, installs, runs or verifies anything.
  listCatalog: () => call<CatalogEntry[]>("list_catalog"),
  previewGithubUrl: (url: string) => call<UrlPreview>("preview_github_url", { url }),
  addCatalogEntry: (url: string, kind: string, purpose: string) => call<CatalogEntry>("add_catalog_entry", { url, kind, purpose }),
  setCatalogEntryEnabled: (entryId: string, enabled: boolean) => call<CatalogEntry>("set_catalog_entry_enabled", { entryId, enabled }),
  removeCatalogEntry: (entryId: string) => call<CatalogEntry>("remove_catalog_entry", { entryId }),
  restoreCatalogEntry: (entryId: string) => call<CatalogEntry>("restore_catalog_entry", { entryId }),
  explainExecutionBlock: (entryId: string) => call<ExecutionBlock>("explain_execution_block", { entryId }),

  // Projects and desired (never actual) per-project choices.
  listProjects: () => call<ProjectView[]>("list_projects"),
  addProjectReference: (alias: string, displayName: string, note: string) => call<ProjectView>("add_project_reference", { alias, displayName, note }),
  removeProjectReference: (alias: string) => call<void>("remove_project_reference", { alias }),
  setProjectPreference: (k: PreferenceKey, decision: string, remindDays?: number) =>
    call<PreferenceView>("set_project_preference", {
      alias: k.alias, entryId: k.entryId, agentSurface: k.agentSurface, platformTarget: k.platformTarget, decision, remindDays: remindDays ?? null,
    }),
  resetProjectPreference: (k: PreferenceKey) =>
    call<void>("reset_project_preference", { alias: k.alias, entryId: k.entryId, agentSurface: k.agentSurface, platformTarget: k.platformTarget }),

  // Deterministic, local, zero model calls.
  listScenarios: () => call<Scenario[]>("list_scenarios"),
  evaluateTask: (input: TaskInput) => call<Evaluation>("evaluate_task", { input }),

  // Installation PLAN preview. Reads the catalog and stored observations only; nothing is installed, run or written.
  planInstall: (request: PlanRequest) => call<InstallPlan>("plan_install", { request }),

  // The installation workflow (Phase 3D-2). The interface sends identifiers and one acknowledgement; Rust decides everything else:
  // there is no parameter for a path, a URL, a command line, bytes or a digest to trust.
  listInstallStates: () => call<ToolState[]>("list_install_states"),
  prepareInstall: (entryId: string, action: InstallAction) => call<InstallPreview>("prepare_install", { request: { entryId, action } }),
  cancelInstall: (offerId: string) => call<void>("cancel_install", { request: { offerId } }),
  confirmInstall: (offerId: string, previewDigest: string, acknowledged: boolean) =>
    call<InstallResult>("confirm_install", { request: { offerId, previewDigest, acknowledged } }),
  getInstalledDetails: (entryId: string) => call<InstalledDetails>("get_installed_details", { entryId }),
  recoverInstallations: () => call<RecoverySummary>("recover_installations"),

  // Read-only discovery. Never installs, runs or verifies anything.
  runDiscoveryScan: (request: ScanRequest) => call<ScanSummary>("run_discovery_scan", { request }),
  listInventory: () => call<Inventory>("list_inventory"),
  // Read-only preview of old database backups; it cannot delete anything.
  previewBackupCleanup: () => call<CleanupPlan>("preview_backup_cleanup"),

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
