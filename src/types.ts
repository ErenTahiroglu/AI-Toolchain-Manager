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
  catalogCount: number;
  catalogEnabledCount: number;
  unreviewedCount: number;
  installedCount: number;
  validatedCount: number;
  sampleProjectCount: number;
  referenceProjectCount: number;
  preferenceCount: number;
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
  userAdded?: boolean;
  sourceSnapshot?: { observedRef?: string; immutableCommit?: string | null; releaseVersion?: string | null; revisionState?: string };
  license?: { state: string; spdxId?: string | null };
  targetEvidence?: TargetEvidence[];
  installMethods?: { methodId: string; publisherPin: string | null }[];
  privacy?: { telemetry?: string; mayTransmitProjectContent?: boolean | null; notes?: string[] };
  networkClaims?: string[];
  dependencies?: string[];
}

export type EntryKind = "executable_tool" | "agent_plugin" | "skill_collection" | "prompt_source";

export interface CatalogEntry {
  entryId: string;
  kind: EntryKind;
  origin: "seed" | "user";
  displayName: string;
  purpose: string;
  sourceKey: string;
  repoUrl: string;
  sourcePath: string | null;
  pluginIdentity: string | null;
  reviewState: string;
  metadataState: "documented-snapshot" | "not-fetched";
  executionPolicy: "manual-only" | "blocked-until-scoped-authorization";
  licenseState: string;
  licenseSpdx: string | null;
  enabled: boolean;
  installState: string;
  verificationState: string;
  revision: string;
  capabilityFingerprint: string;
  addedAt: string;
  removedAt: string | null;
  skillCount: number;
  record: ToolRecord;
  limitations: string[];
}

export interface UrlPreview {
  ok: boolean;
  parsed: { owner: string; repo: string; canonicalUrl: string; sourceKey: string } | null;
  error: AppError | null;
  duplicateOf: { entryId: string; displayName: string; kind: EntryKind; removed: boolean } | null;
  notes: string[];
}

export interface ExecutionBlock {
  code: string;
  message: string;
}

export type Decision = "accepted" | "rejected" | "postponed" | "disabled";

export interface PreferenceView {
  entryId: string;
  entryName: string;
  entryRemoved: boolean;
  agentSurface: string;
  platformTarget: string;
  decision: Decision;
  decidedAt: string;
  catalogRevision: string;
  revisionChanged: boolean;
  materialChange: boolean;
  remindAt: string | null;
  actualInstallState: string;
  actualVerificationState: string;
  observedScope: string;
  compatibility: string;
}

export interface ProjectView {
  projectId: string;
  alias: string;
  displayName: string;
  kind: "synthetic" | "reference";
  localNote: string;
  createdAt: string;
  preferences: PreferenceView[];
}

export interface Scenario {
  id: string;
  title: string;
  taskText: string;
  evidenceFiles: string[];
}

export interface SkillCandidate {
  skillId: string;
  name: string;
  description: string;
  sourceSha: string;
  licenseSpdx: string | null;
  licenseWarning: boolean;
  conflictsWith: string[];
}

export interface Recommendation {
  entryId: string;
  displayName: string;
  level: "suggest" | "informational";
  canEnable: boolean;
  rationale: string;
  impact: string;
  signals: string[];
  notes: string[];
  skillCandidates: SkillCandidate[];
}

export interface Suppressed {
  entryId: string;
  reasonCode: string;
  reason: string;
}

export interface Evaluation {
  taskId: string;
  phase: string;
  recommendations: Recommendation[];
  suppressed: Suppressed[];
  disclaimer: string;
}

export interface TaskInput {
  projectAlias: string;
  taskId: string;
  taskText: string;
  evidenceFiles: string[];
  agentSurface: string;
  platformTarget: string;
  phase?: "start" | "late";
  availability: { entryId: string; scope: "global" | "project"; enabled: boolean }[];
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

// ---- Phase 2A: read-only local discovery -------------------------------------------------------

export interface ScanRequest {
  pathExecutables: boolean;
  userConfig: boolean;
  project: { alias: string; directory: string } | null;
}

export interface ScanSummary {
  scanId: string;
  startedAt: string;
  finishedAt: string;
  platformTarget: string;
  authorizedPath: boolean;
  authorizedUserConfig: boolean;
  authorizedProject: boolean;
  status: string;
  observationCount: number;
}

export interface StoredObservation {
  id: number;
  scanId: string;
  subject: string;
  platformTarget: string;
  agentSurface: string;
  method: string;
  presence: "present" | "absent" | "unknown" | "unsupported";
  configuration: string;
  version: string | null;
  versionSource: string | null;
  scope: "global" | "project" | "local" | "unknown";
  projectAlias: string | null;
  confidence: "high" | "medium" | "low" | "none";
  evidence: string;
  location: string | null;
  limitations: string[];
  identityVerified: boolean;
  functionalityTested: boolean;
  observedAt: string;
  stale: boolean;
  ageHours: number;
}

export interface RequestVsObservation {
  entryId: string;
  entryName: string;
  acceptedProjects: number;
  observed: "present" | "absent" | "unknown" | "not_scanned";
  note: string;
}

export interface Inventory {
  lastScan: ScanSummary | null;
  observations: StoredObservation[];
  requests: RequestVsObservation[];
  notChecked: string[];
  staleAfterHours: number;
}
