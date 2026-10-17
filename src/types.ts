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
  /** Read the content of program files found by name, only to compute SHA-256 fingerprints. Nothing is run. */
  fingerprintArtifacts: boolean;
}

export interface ScanSummary {
  scanId: string;
  startedAt: string;
  finishedAt: string;
  platformTarget: string;
  authorizedPath: boolean;
  authorizedUserConfig: boolean;
  authorizedProject: boolean;
  authorizedFingerprint: boolean;
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
  /** Written before the privacy gate existed; scrubbed in place by migration 5. */
  legacy: boolean;
  scanFinishedAt: string;
  scanAuthorizedPath: boolean;
  scanAuthorizedUserConfig: boolean;
  scanAuthorizedProject: boolean;
  scanAuthorizedFingerprint: boolean;
  provenance: "latest_scan" | "earlier_scan";
  /** Did the latest scan look at the area this observation belongs to? */
  latestScanChecked: boolean;
  /** From the latest scan, fresh, and for this computer's platform. Only a current observation describes today. */
  current: boolean;
  notCurrentReason: string | null;
  ladder: Ladder;
}

// "validated" is RESERVED for authenticated evidence bound to the installed artifact; no detector produces it yet.
export type IdentityState = "not_assessed" | "unknown" | "evidence_available" | "record_corroborated" | "validated" | "conflicted";
export type NotPerformed =
  | "active_version_probe" | "signature_chain_validation" | "signature_parsing_unsupported" | "installed_file_digest"
  | "package_metadata_not_found" | "no_package_detector_for_tool" | "record_to_file_linkage" | "artifact_authentication"
  | "artifact_fingerprint" | "plugin_directory_digest" | "authenticated_reference";

/** Phase 2C artifact evidence (policy v1). A fingerprint identifies bytes; only an AUTHENTICATED exact-file reference verifies. */
export interface ArtifactEvidence {
  policyVersion: number;
  fingerprint: "not_requested" | "computed" | "unknown";
  unknownReason: string | null;
  sha256: string | null;
  bytes: number | null;
  /** The file format established from the first bytes of the hashed file (never from its name). */
  verifiedFormat: "pe-x64" | "pe-arm64" | "pe-x86" | "macho-arm64" | "macho-x64" | "macho-universal" | "macho-universal-arm64" | "macho-universal-x64" | "macho-universal-arm64-x64" | "elf-x64" | "elf-arm64" | "script-shebang" | "zip" | "gzip" | "unrecognised" | null;
  recordLink: "none" | "unverifiable" | "folder_by_name";
  reference: "none" | "unauthenticated" | "authenticated";
  referenceKind: "archive" | "installer" | "executable" | "script" | "plugin_file" | "package" | null;
  matchResult: "not_applicable" | "incomparable" | "matches_unauthenticated" | "mismatches_unauthenticated" | "matches_authenticated" | "mismatches_authenticated";
  referenceEpoch: string | null;
  invalidated: boolean;
}

/** The Phase 2B evidence ladder. `assessed` is false for rows written before it existed. */
export interface Ladder {
  assessed: boolean;
  filenameDetected: boolean;
  installRecord: boolean;
  publisher: "none" | "self_declared" | "marketplace";
  identity: IdentityState;
  version: "unknown" | "known" | "conflicted";
  versionTrust: "none" | "declared" | "installer_recorded" | "validated";
  signatureBlock: "not_assessed" | "absent" | "present_unvalidated" | "unreadable";
  installedFilesSeen: boolean;
  artifactAuth: "none" | "digest_match" | "signature_chain";
  /** An earlier build stored a higher trust level than the evidence supports; it was lowered. */
  reclassified: boolean;
  artifact: ArtifactEvidence;
  notPerformed: NotPerformed[];
}

export type BackupPrivacyStatus = "violations_observed" | "potentially_sensitive" | "none_detected_in_examined_content" | "partially_scanned_no_findings" | "not_assessable";

export interface BackupLayer {
  layer: "observation_rows" | "all_text" | "raw_main" | "raw_wal" | "wal_logical_sql";
  outcome: "completed" | "partial" | "unreadable" | "unsupported" | "not_present";
  reason: string;
}

export interface BackupEntry {
  fileName: string;
  schemaVersion: number | null;
  sizeBytes: number;
  modified: string;
  privacyStatus: BackupPrivacyStatus;
  reason: string;
  /** complete | partial | incomplete: whether every layer finished. */
  coverage: "complete" | "partial" | "incomplete";
  layers: BackupLayer[];
  /** null = the discovery table is absent or unreadable ("not examined", not "zero"). */
  observationRowsFailingGate: number | null;
  otherTextFindings: number;
  tablesWithFindings: string[];
  rawPathLikeMatches: number;
  /** Raw layers (main file / -wal) in which a credential, e-mail or private-path shape was seen (a count, never content). */
  rawSensitiveShapeLayers: number;
  hasWalSidecar: boolean;
}

export interface CleanupPlan {
  entries: BackupEntry[];
  preconditions: { id: string; ok: boolean; text: string }[];
  examined: string[];
  notExamined: string[];
  reviewCandidates: string[];
  noViolationDetected: string[];
  partiallyScanned: string[];
  notAssessable: string[];
  directoryStatus: "ok" | "missing" | "refused_redirect" | "refused_non_local" | "unreadable";
  limits: { maxTextCells: number; maxRawBytes: number; maxSqliteImageBytes: number; timeoutSeconds: number };
  unrecognisedFiles: number;
  deletionAvailable: boolean;
  executed: boolean;
  notes: string[];
}

export interface ScanRef {
  scanId: string;
  finishedAt: string;
  authorizedPath: boolean;
  authorizedUserConfig: boolean;
  authorizedProject: boolean;
  authorizedFingerprint: boolean;
}

export type RequestStatus =
  | "present_current" | "absent_current" | "unknown_current"
  | "previously_present" | "previously_absent" | "previously_unknown"
  | "platform_mismatch" | "not_scanned";

export interface RequestVsObservation {
  entryId: string;
  entryName: string;
  projectAlias: string;
  projectName: string;
  agentSurface: string;
  platformTarget: string;
  status: RequestStatus;
  note: string;
  basis: ScanRef | null;
}

export interface Inventory {
  lastScan: ScanSummary | null;
  observations: StoredObservation[];
  requests: RequestVsObservation[];
  notChecked: string[];
  staleAfterHours: number;
}

// ---- Phase 2D: installation PLAN preview (read-only; a plan is data, never an action or a permission) ------------------
export interface PlanRequest {
  entryId: string;
  platformTarget: string;
  agentSurface: string;
  scope: string;
  methodId: string | null;
  /** Required when scope is "project"; null otherwise. */
  projectAlias: string | null;
}
export interface PlanBlocker { code: string; text: string }
export interface PlanPrerequisite { id: string; text: string; state: "met" | "unmet" | "unknown"; source: "observed_scan" | "publisher_declared" | "not_checked"; basis: "current_scan" | "no_current_evidence" | "publisher_declared" | "not_checked" }
export interface PlanStep {
  index: number;
  kind: string;
  summary: string;
  typed: Record<string, unknown> | null;
  missingParameters: string[];
  network: "yes" | "no";
  writes: string[];
  writeLabels: string[];
  admin: "no" | "maybe";
  reversibility: "host_removable" | "version_switch" | "nothing_to_undo" | "unknown";
}
export interface PlanMethod { methodId: string; label: string; supported: boolean; note: string }
export interface PlanDisclosure { state: string; text: string }
export interface InstallPlan {
  schemaVersion: number;
  plannerVersion: string;
  planId: string;
  /** Always "preview_only": there is no installed / running / approved state. */
  state: "preview_only";
  executable: false;
  executed: false;
  modifiesAnything: false;
  statement: string;
  entry: {
    entryId: string; displayName: string; kind: string; source: string; reviewState: string; executionPolicy: string;
    licenseState: string; revision: string; sourcePin: "none" | "malformed" | "declared_unverified";
  };
  target: {
    platformTarget: string; agentSurface: string; scope: string; thisComputer: string; thisComputerMatches: boolean;
    recipeTargetId: string | null; catalogCompatibility: string;
  };
  eligibility: { status: "blocked"; canExecute: false; blockers: PlanBlocker[]; warnings: PlanBlocker[] };
  methods: PlanMethod[];
  chosenMethod: string | null;
  prerequisites: PlanPrerequisite[];
  declaredDependencies: string[];
  steps: PlanStep[];
  permissions: string[];
  network: { plannedUse: boolean; destinations: string[]; usedToBuildPreview: false };
  telemetry: PlanDisclosure;
  dataAndCredentials: PlanDisclosure;
  diskUsage: PlanDisclosure;
  configChanges: { area: string; change: string; text: string }[];
  conflicts: PlanBlocker[];
  uninstall: PlanDisclosure;
  rollback: { available: false; reliability: string; text: string };
  unknowns: string[];
}
