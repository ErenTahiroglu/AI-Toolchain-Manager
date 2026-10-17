// In-memory stand-in for the Rust commands. It mirrors the wire shapes in src/types.ts; the real
// behaviour of each command is covered by the Rust tests and the --self-check contract test.
import type { HealthReport, ToolRow } from "../types";

export const TOOL_IDS = [
  "ponytail", "codebase-memory-mcp", "graphify", "gitleaks", "markitdown",
  "caveman", "cloudflare-security-audit-skill", "superpowers", "betterleaks",
];

export const makeTools = (): ToolRow[] =>
  TOOL_IDS.map((id) => ({
    toolId: id, displayName: id, category: "native-cli", repoUrl: `https://github.com/example/${id}`,
    reviewState: "unreviewed", installState: "not_managed", validationState: "unknown",
    record: {
      license: { state: "known", spdxId: "MIT" },
      targetEvidence: [{ platformTarget: "windows-x64", agentSurface: "local-cli", compatibilityState: "vendor-declared", limitation: "not tested" }],
      privacy: { telemetry: "unknown", mayTransmitProjectContent: null },
    },
  }));

export const report = (overall: HealthReport["overall"]): HealthReport => ({
  ranAt: "2026-10-17T12:00:00Z",
  overall,
  checks: [
    { id: "database", title: "Yerel veritabanı", status: overall === "failed" ? "failed" : "passed", explanation: "açıklama", errorCode: overall === "failed" ? "DB_INTEGRITY_FAILED" : null },
    { id: "single-instance", title: "Tek uygulama örneği", status: "unknown", explanation: "dışarıdan test edilmeli", errorCode: null },
    { id: "adapters", title: "Araç bağdaştırıcıları", status: "skipped", explanation: "yok", errorCode: null },
  ],
});

export interface Backend {
  calls: { cmd: string; args?: Record<string, unknown> }[];
  settings: Record<string, string>;
  startup: { preference: boolean; osEnabled: boolean | null };
  nextReport: HealthReport;
  lastReport: HealthReport | null;
  failures: Record<string, { code: string; message: string }>;
  handle: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
}

export function createBackend(): Backend {
  const b: Backend = {
    calls: [],
    settings: { theme: "system", close_to_tray: "true", startup_enabled: "false" },
    startup: { preference: false, osEnabled: false },
    nextReport: report("unknown"),
    lastReport: null,
    failures: {},
    async handle(cmd, args) {
      b.calls.push({ cmd, args });
      if (b.failures[cmd]) throw b.failures[cmd];
      switch (cmd) {
        case "get_app_info":
          return { version: "0.1.0", phase: "1A", platform: "windows", arch: "x86_64", dbVersion: 1, dataDir: "C:\\data", degraded: false, startupError: null };
        case "get_dashboard":
          return {
            toolCount: 9, unreviewedCount: 9, installedCount: 0, validatedCount: 0, sampleProjectCount: 3, promptSourceCount: 3,
            auditEventCount: 4, auditChain: { ok: true, count: 4, head: "a".repeat(64), problem: null }, lastReport: b.lastReport,
          };
        case "list_tools": return makeTools();
        case "list_projects":
          return [{
            alias: "ornek-web-sitesi", displayName: "Örnek: Web sitesi", description: "Uydurma", isSynthetic: true,
            decisions: [{ toolId: "gitleaks", toolName: "Gitleaks", agentSurface: "local-cli", platformTarget: "windows-x64", decision: "accepted", actualState: "unknown" }],
          }];
        case "list_prompt_sources":
          return ["prompts.chat", "the-prompt-library", "Fabric"].map((n) => ({
            sourceId: n, displayName: n, repoUrl: `https://github.com/x/${n}`, licenseNote: "MIT", plannedFormat: "csv", notes: "-",
            trustStatus: "untrusted-reference", status: "planned", readOnly: true,
          }));
        case "run_health_checks": b.lastReport = b.nextReport; return b.nextReport;
        case "list_audit_events": return [{ sequence: 1, timestamp: "2026-10-17T12:00:00Z", eventType: "health_check", outcome: "passed", checkId: "app-start", recordDigest: "b".repeat(64) }];
        case "verify_audit_chain": return { ok: true, count: 1, head: "b".repeat(64), problem: null };
        case "list_diagnostics": return [];
        case "get_settings": return { ...b.settings };
        case "set_setting": b.settings[String(args?.key)] = String(args?.value); return { ...b.settings };
        case "get_startup_state": return { ...b.startup };
        case "set_startup_enabled": b.startup = { preference: Boolean(args?.enabled), osEnabled: Boolean(args?.enabled) }; return { ...b.startup };
        case "report_ui_error": return undefined;
        default: throw { code: "UNKNOWN_COMMAND", message: cmd };
      }
    },
  };
  return b;
}
