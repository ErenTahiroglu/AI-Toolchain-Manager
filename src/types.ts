// Shapes returned by the typed Rust commands (src-tauri/src/commands.rs). camelCase on the wire.

export type CheckStatus = "passed" | "failed" | "unknown" | "skipped";

export interface AppError {
  code: string;
  message: string;
}

export interface AppInfo {
  version: string;
  phase: string;
  platform: string;
  arch: string;
  dbVersion: number | null;
  dataDir: string;
  degraded: boolean;
  startupError: AppError | null;
}

export interface CheckResult {
  id: string;
  title: string;
  status: CheckStatus;
  explanation: string;
  errorCode: string | null;
}

export interface HealthReport {
  ranAt: string;
  overall: Exclude<CheckStatus, "skipped">;
  checks: CheckResult[];
}

export interface ChainReport {
  ok: boolean;
  count: number;
  head: string | null;
  problem: string | null;
}

export interface Dashboard {
  toolCount: number;
  unreviewedCount: number;
  installedCount: number;
  validatedCount: number;
  sampleProjectCount: number;
  promptSourceCount: number;
  auditEventCount: number;
  auditChain: ChainReport;
  lastReport: HealthReport | null;
}

export interface TargetEvidence {
  platformTarget: string;
  agentSurface: string;
  compatibilityState: string;
  limitation: string;
}

export interface ToolRecord {
  sourceSnapshot?: { observedRef?: string; immutableCommit?: string | null; releaseVersion?: string | null; revisionState?: string };
  license?: { state: string; spdxId?: string };
  targetEvidence?: TargetEvidence[];
  installMethods?: { methodId: string; publisherPin: string | null }[];
  privacy?: { telemetry?: string; mayTransmitProjectContent?: boolean | null; notes?: string[] };
  networkClaims?: string[];
  dependencies?: string[];
}

export interface ToolRow {
  toolId: string;
  displayName: string;
  category: string;
  repoUrl: string;
  reviewState: string;
  installState: string;
  validationState: string;
  record: ToolRecord;
}

export interface DecisionRow {
  toolId: string;
  toolName: string;
  agentSurface: string;
  platformTarget: string;
  decision: string;
  actualState: string;
}

export interface ProjectRow {
  alias: string;
  displayName: string;
  description: string;
  isSynthetic: boolean;
  decisions: DecisionRow[];
}

export interface PromptSourceRow {
  sourceId: string;
  displayName: string;
  repoUrl: string;
  licenseNote: string;
  plannedFormat: string;
  notes: string;
  trustStatus: string;
  status: string;
  readOnly: boolean;
}

export interface AuditEvent {
  sequence: number;
  timestamp: string;
  eventType: string;
  outcome: CheckStatus | "blocked";
  checkId?: string;
  errorCode?: string;
  verdictSource?: string;
  recordDigest: string;
}

export interface DiagnosticRow {
  id: number;
  ts: string;
  level: "info" | "warn" | "error";
  component: string;
  code: string;
  detail: string;
}

export type Settings = Record<string, string>;

export interface StartupState {
  preference: boolean;
  osEnabled: boolean | null;
}
