// In-memory stand-in for the Rust commands. It mirrors the wire shapes in src/types.ts so the UI can be
// tested; the real behaviour of every command (validation, gates, migration, audit) is covered by the
// Rust tests and by the --self-check contract test, not by this mock.
import { createInstallMock, installCommand, type InstallMock } from "./installMock";
import type { CatalogEntry, CleanupPlan, EvidenceOrigin, Evaluation, HealthReport, InstallPlan, Inventory, PlanRequest, PreferenceView, ProjectView, Recommendation, RequestStatus, RequestVsObservation, ScanRef, StoredObservation } from "../types";

export const SEED_IDS = [
  "ponytail", "codebase-memory-mcp", "graphify", "gitleaks", "markitdown", "caveman", "cloudflare-security-audit-skill", "superpowers", "betterleaks",
  "anthropic-frontend-design", "strix", "agent-browser", "agentic-awesome-skills",
];
const KIND: Record<string, CatalogEntry["kind"]> = {
  ponytail: "agent_plugin", caveman: "agent_plugin", superpowers: "agent_plugin", "anthropic-frontend-design": "agent_plugin",
  "cloudflare-security-audit-skill": "skill_collection", "agentic-awesome-skills": "skill_collection",
};

export const makeEntry = (id: string, over: Partial<CatalogEntry> = {}): CatalogEntry => ({
  entryId: id, kind: KIND[id] ?? "executable_tool", origin: "seed", displayName: id, purpose: `Amaç: ${id}`, sourceKey: `x/${id}`,
  repoUrl: `https://github.com/example/${id}`, sourcePath: null, pluginIdentity: id === "anthropic-frontend-design" ? "frontend-design@claude-plugins-official" : null,
  reviewState: id === "strix" ? "quarantined" : "unreviewed", metadataState: "documented-snapshot",
  executionPolicy: id === "strix" ? "blocked-until-scoped-authorization" : "manual-only", licenseState: "known", licenseSpdx: "MIT", enabled: true,
  installState: "not_managed", verificationState: "unknown", revision: "v1@abcdef", capabilityFingerprint: "f".repeat(64), addedAt: "2026-10-17T00:00:00Z",
  removedAt: null, skillCount: 0,
  record: { targetEvidence: [{ platformTarget: "windows-x64", agentSurface: "local-cli", compatibilityState: "vendor-declared", limitation: "denenmedi" }] },
  limitations: ["İncelenmedi.", "Kurulum ve işlev doğrulaması yok; bu uygulama hiçbir şey kurmaz veya çalıştırmaz."],
  ...over,
});

export const report = (overall: HealthReport["overall"]): HealthReport => ({
  ranAt: "2026-10-17T12:00:00Z",
  overall,
  checks: [
    { id: "database", title: "Yerel veritabanı", status: overall === "failed" ? "failed" : "passed", explanation: "açıklama", errorCode: overall === "failed" ? "DB_INTEGRITY_FAILED" : null },
    { id: "single-instance", title: "Tek uygulama örneği", status: "unknown", explanation: "dışarıdan test edilmeli", errorCode: null },
    { id: "adapters", title: "Araç bağdaştırıcıları", status: "skipped", explanation: "yok", errorCode: null },
  ],
});

export const obs = (over: Partial<StoredObservation> = {}): StoredObservation => ({
  id: 1, scanId: "s1", subject: "gitleaks", platformTarget: "windows-x64", agentSurface: "local-cli", method: "path-executable", presence: "present",
  configuration: "not_applicable", version: null, versionSource: null, scope: "global", projectAlias: null, confidence: "low",
  evidence: "PATH klasörlerinden birinde 'gitleaks' adlı bir dosya bulundu. Bu yalnızca bir ad eşleşmesidir.", location: "PATH girdisi #1 · başka bir klasör · gitleaks.exe",
  limitations: ["Ad eşleşmesi: aynı adlı başka bir program olabilir."], identityVerified: false, functionalityTested: false,
  observedAt: new Date().toISOString(), stale: false, ageHours: 0, legacy: false, scanFinishedAt: new Date().toISOString(),
  scanAuthorizedPath: true, scanAuthorizedUserConfig: false, scanAuthorizedProject: false, scanAuthorizedFingerprint: false, provenance: "latest_scan", latestScanChecked: true,
  current: true, notCurrentReason: null,
  ladder: { assessed: true, filenameDetected: true, installRecord: false, publisher: "none", identity: "unknown", version: "unknown", versionTrust: "none", signatureBlock: "not_assessed", installedFilesSeen: false, artifactAuth: "none", reclassified: false, artifact: { policyVersion: 3, fingerprint: "not_requested", unknownReason: null, sha256: null, bytes: null, verifiedFormat: null, recordLink: "none", reference: "none", referenceKind: null, matchResult: "not_applicable", referenceEpoch: null, invalidated: false }, notPerformed: ["active_version_probe", "installed_file_digest", "no_package_detector_for_tool"] },
  ...over,
});

export const emptyInventory = (): Inventory => ({
  lastScan: null, observations: [], requests: [], staleAfterHours: 24,
  notChecked: ["Henüz hiç tarama yapılmadı; aşağıdakilerin hiçbirine bakılmadı.", "Programların sürümleri: kimliği doğrulanmamış hiçbir program çalıştırılmadı."],
});

export const scanRef = (over: Partial<ScanRef> = {}): ScanRef => ({
  scanId: "scan-12345678", finishedAt: "2026-10-23T09:00:00Z", authorizedPath: true, authorizedUserConfig: false, authorizedProject: false, authorizedFingerprint: false, ...over,
});

/** The notes the Rust classifier writes (store.rs::classify_request), so the UI tests show what the backend really says. */
export const REQUEST_NOTES: Record<RequestStatus, string> = {
  present_current: "Son taramada bu bilgisayarda bulundu (bulunması, çalıştığı veya projede etkin olduğu anlamına gelmez).",
  absent_current: "Etkinleştirmek istediniz, ancak son taramada bu bilgisayarda bulunamadı.",
  unknown_current: "Etkinleştirmek istediniz; son taramada bu bilgisayarda olup olmadığı belirlenemedi.",
  previously_present: "Eski sonuç (Son tarama 24 saatten eski; yeniden tarayın.) Yeniden tarayın; eski bir sonuç hiçbir işlem için dayanak sayılmaz.",
  previously_absent: "Eski sonuç (Son tarama 24 saatten eski; yeniden tarayın.) Yeniden tarayın; eski bir sonuç hiçbir işlem için dayanak sayılmaz.",
  previously_unknown: "Eski sonuç (Son tarama 24 saatten eski; yeniden tarayın.) Yeniden tarayın; eski bir sonuç hiçbir işlem için dayanak sayılmaz.",
  platform_mismatch: "Yalnızca başka bir işletim sistemi/mimari için gözlem var; bu tercihin platformu için hiçbir şey gözlenmedi.",
  not_scanned: "Bu proje, ajan yüzeyi ve platform için henüz bir gözlem yok. Bu, aracın olmadığı anlamına gelmez.",
};

/** The evidence-origin descriptor in the wire shape (store.rs::EvidenceOrigin). */
export const originOf = (over: Partial<EvidenceOrigin> = {}): EvidenceOrigin => ({
  projectRelation: "computer_wide", surfaceRelation: "exact_surface", observedAt: "2026-10-23T09:00:00Z", ageHours: 3, ...over,
});

/** One row of Inventory.requests in the wire shape. Like the backend, only the statuses that name a scan carry a basis. */
export const makeRequest = (over: Partial<RequestVsObservation> = {}): RequestVsObservation => {
  const status = over.status ?? "present_current";
  return {
    entryId: "alfa", entryName: "Alfa Araç", projectAlias: "ornek-web-sitesi", projectName: "Örnek: Web sitesi", agentSurface: "claude-code-terminal", platformTarget: "windows-x64",
    status, note: REQUEST_NOTES[status], basis: status === "not_scanned" || status === "platform_mismatch" ? null : scanRef(),
    // like the backend: a descriptor exists exactly when an observation was selected
    origin: status === "not_scanned" || status === "platform_mismatch" ? null : originOf(), ...over,
  };
};

export const PLAN_STATEMENT =
  "Bu bir ÖNİZLEMEDİR. Hiçbir şey kurulmadı, indirilmedi, çalıştırılmadı veya değiştirilmedi; üçüncü taraf araçlara dokunulmadı. Bu plan bir izin değildir: onaylanmış bir tarif ve ayrı bir kullanıcı onayı olmadan hiçbir adım uygulanamaz.";

/** A plan in the wire shape of the Rust planner (the real planner is covered by the Rust tests and the contract test). */
export const makePlan = (r: PlanRequest, over: Partial<InstallPlan> = {}): InstallPlan => ({
  schemaVersion: 2, plannerVersion: "1.1.0", planId: "a".repeat(64), state: "preview_only", executable: false, executed: false, modifiesAnything: false,
  statement: PLAN_STATEMENT,
  entry: { entryId: r.entryId, displayName: r.entryId, kind: "executable_tool", source: "example/" + r.entryId, reviewState: "unreviewed", executionPolicy: "manual-only", licenseState: "known", revision: "v1@abcdef", sourcePin: "none" },
  target: { platformTarget: r.platformTarget, agentSurface: r.agentSurface, scope: r.scope, projectAlias: r.projectAlias, thisComputer: "windows-x64", thisComputerMatches: r.platformTarget === "windows-x64", recipeTargetId: null, catalogCompatibility: "vendor-declared" },
  eligibility: {
    status: "blocked", canExecute: false,
    blockers: [
      { code: "no_approved_recipe", text: "Bu araç için onaylanmış, tipli bir kurulum tarifi yok ve bu sürümde tarif onaylama/saklama mekanizması da yok." },
      { code: "metadata_unreviewed", text: "Katalog bilgileri bağımsız olarak incelenmedi; yayıncı beyanlarına dayanıyor." },
    ],
    warnings: [{ code: "admin_unknown", text: "Yönetici izni gerekip gerekmediği bu yöntem için önceden bilinemez." }],
  },
  methods: [
    { methodId: "verified-release-archive", label: "Doğrulanmış sürüm dosyası (yöneticinin kendi klasörüne)", supported: true, note: "" },
    { methodId: "skills-cli", label: "Desteklenmeyen yöntem", supported: false, note: "Üçüncü taraf 'skills' komut satırı aracı: tipli tarif türü yok." },
  ],
  chosenMethod: "verified-release-archive",
  prerequisites: [{ id: "agent_present", text: "Ajanın bu bilgisayarda olup olmadığı bilinmiyor: izin verilmiş bir tarama kaydı yok.", state: "unknown", source: "observed_scan", basis: "no_current_evidence" }],
  declaredDependencies: ["Node.js >=22"],
  steps: [
    { index: 1, kind: "fetch-release-artifact", summary: "Yayıncının sürüm dosyası, onaylı tarifte yazılı adresten indirilecekti.", typed: { operation: "fetch-release-artifact", artifactId: "primary" }, missingParameters: [], network: "yes", writes: ["manager_staging"], writeLabels: ["Yöneticinin kendi hazırlık klasörü"], admin: "no", reversibility: "nothing_to_undo" },
    { index: 2, kind: "verify-artifact-digest", summary: "İndirilen dosyanın SHA-256 özeti, tarifte yazılı beklenen değerle karşılaştırılacaktı.", typed: null, missingParameters: ["expectedSha256"], network: "no", writes: [], writeLabels: [], admin: "no", reversibility: "nothing_to_undo" },
  ],
  permissions: ["Geçerli kullanıcı hesabı (yönetici yetkisi gerekmediği varsayılır)", "İnternet erişimi (yalnızca onaylı bir uygulama aşamasında; bu önizleme bağlantı kurmadı)"],
  network: { plannedUse: true, destinations: ["GitHub sürüm indirme adresi (tarifte yazılı)"], usedToBuildPreview: false },
  telemetry: { state: "unknown", text: "Telemetri durumu bilinmiyor." },
  dataAndCredentials: { state: "unknown", text: "Proje içeriğinin gönderilip gönderilmediği bilinmiyor. Bu uygulama hiçbir kimlik bilgisi istemez veya saklamaz." },
  diskUsage: { state: "unknown", text: "Disk kullanımı güvenilir biçimde bilinmiyor (katalogda boyut bilgisi yok)." },
  configChanges: [],
  conflicts: [{ code: "existing_installation_found", text: "Son tarama bu aracın bu bilgisayarda zaten bulunduğunu gösteriyor; yeni kurulum mevcut kopyayla çakışabilir ve mevcut kopya değiştirilmez." }],
  uninstall: { state: "manager_owned", text: "Yöneticinin sürümlü klasörü silinebilir; aracın kendi ürettiği dizinler/veriler kullanıcıya ait sayılır ve kalır." },
  rollback: { available: false, reliability: "none_implemented", text: "Geri alma bu sürümde YOK." },
  unknowns: ["Aracın işlevinin bu bilgisayarda çalıştığı doğrulanmadı ve bu planla doğrulanmayacak."],
  recipe: { state: "none", recipeId: null, revision: null, recipeDigest: null, synthetic: false, problems: [], rollbackState: null, authentication: "none" },
  approvalState: "not_given",
  ...over,
});

export interface Backend {
  calls: { cmd: string; args?: Record<string, unknown> }[];
  settings: Record<string, string>;
  startup: { preference: boolean; osEnabled: boolean | null };
  nextReport: HealthReport;
  lastReport: HealthReport | null;
  failures: Record<string, { code: string; message: string }>;
  entries: CatalogEntry[];
  projects: ProjectView[];
  nextEvaluation: Evaluation;
  inventory: Inventory;
  backupPlan: CleanupPlan;
  /** Lets a test return a plan with another recipe status. */
  planOverride: ((r: PlanRequest) => InstallPlan) | null;
  /** The installation workflow stand-in (Phase 3D-2): by default every tool is "verification incomplete". */
  install: InstallMock;
  handle: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
}

export const pref = (over: Partial<PreferenceView> = {}): PreferenceView => ({
  entryId: "gitleaks", entryName: "gitleaks", entryRemoved: false, agentSurface: "local-cli", platformTarget: "windows-x64", decision: "accepted",
  decidedAt: "2026-10-17T12:00:00Z", catalogRevision: "v1@abcdef", revisionChanged: false, materialChange: false, remindAt: null,
  actualInstallState: "not_managed", actualVerificationState: "unknown", observedScope: "unknown", compatibility: "vendor-declared", ...over,
});

export const rec = (over: Partial<Recommendation> = {}): Recommendation => ({
  entryId: "anthropic-frontend-design", displayName: "Anthropic Frontend Design", level: "suggest", canEnable: true, rationale: "Görev bir arayüz işi.",
  impact: "Hiçbir şey kurulmaz.", signals: ["görev ifadesi: react"], notes: [], skillCandidates: [], ...over,
});

export function createBackend(): Backend {
  const b: Backend = {
    calls: [],
    settings: { theme: "system", close_to_tray: "true", startup_enabled: "false" },
    startup: { preference: false, osEnabled: false },
    nextReport: report("unknown"),
    lastReport: null,
    failures: {},
    entries: SEED_IDS.map((id) => makeEntry(id)),
    projects: [
      { projectId: "p1", alias: "ornek-web-sitesi", displayName: "Örnek: Web sitesi", kind: "synthetic", localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences: [pref()] },
    ],
    nextEvaluation: { taskId: "t", phase: "start", recommendations: [], suppressed: [], disclaimer: "Hiçbir şey kurulmadı." },
    inventory: emptyInventory(),
    backupPlan: { entries: [], preconditions: [], examined: ["Tüm tablolardaki metin değerleri (örüntü taraması)"], notExamined: ["Hiçbir örüntüye uymayan kişisel metin taranamaz"], reviewCandidates: [], noViolationDetected: [], partiallyScanned: [], notAssessable: [], directoryStatus: "ok", limits: { maxTextCells: 500000, maxRawBytes: 268435456, maxSqliteImageBytes: 134217728, timeoutSeconds: 30 }, unrecognisedFiles: 0, deletionAvailable: false, executed: false, notes: ["Bu bir ÖNİZLEMEDİR: hiçbir yedek silinmedi, taşınmadı veya değiştirilmedi."] },
    planOverride: null,
    install: createInstallMock(),
    async handle(cmd, args) {
      b.calls.push({ cmd, args });
      if (b.failures[cmd]) throw b.failures[cmd];
      const find = (id: unknown) => b.entries.find((e) => e.entryId === id)!;
      switch (cmd) {
        case "get_app_info":
          return { version: "0.2.0", phase: "1B", platform: "windows", arch: "x86_64", dbVersion: 2, dataDir: "C:\\data", degraded: false, startupError: null };
        case "get_dashboard":
          return {
            catalogCount: 13, catalogEnabledCount: 13, unreviewedCount: 12, installedCount: 0, validatedCount: 0, sampleProjectCount: 3, referenceProjectCount: 0,
            preferenceCount: 6, promptSourceCount: 3, auditEventCount: 4, auditChain: { ok: true, count: 4, head: "a".repeat(64), problem: null }, lastReport: b.lastReport,
          };
        case "list_catalog": return b.entries.map((e) => ({ ...e }));
        case "preview_github_url": {
          const url = String(args?.url);
          if (!/^https:\/\/github\.com\/[\w-]+\/[\w.-]+\/?$/i.test(url.trim())) {
            return { ok: false, parsed: null, error: { code: "URL_HOST_NOT_GITHUB", message: "Yalnızca github.com adresi kabul edilir." }, duplicateOf: null, notes: ["Çevrimdışı ön izleme."] };
          }
          const [, owner, repo] = url.trim().match(/github\.com\/([\w-]+)\/([\w.-]+)/i)!;
          const key = `${owner}/${repo}`.toLowerCase();
          const any = b.entries.find((e) => e.sourceKey === key);
          const live = !!any && any.removedAt === null;
          return {
            ok: !live, parsed: { owner, repo, canonicalUrl: `https://github.com/${owner}/${repo}`, sourceKey: key }, error: null,
            duplicateOf: any ? { entryId: any.entryId, displayName: any.displayName, kind: any.kind, removed: !live } : null, notes: ["Çevrimdışı ön izleme."],
          };
        }
        case "add_catalog_entry": {
          const m = String(args?.url).match(/github\.com\/([\w-]+)\/([\w.-]+)/i)!;
          const removed = b.entries.find((e) => e.sourceKey === `${m[1]}/${m[2]}`.toLowerCase() && e.removedAt !== null);
          if (removed) {
            if (removed.kind !== args?.kind) throw { code: "KIND_CHANGE_NOT_ALLOWED", message: "Tür geri eklerken değiştirilemez." };
            removed.removedAt = null; removed.enabled = false;
            return { ...removed };
          }
          const e = makeEntry(`${m[1]}-${m[2]}`.toLowerCase(), {
            origin: "user", kind: args?.kind as CatalogEntry["kind"], purpose: String(args?.purpose || "Amacı henüz yazılmadı."), displayName: `${m[1]}/${m[2]}`,
            sourceKey: `${m[1]}/${m[2]}`.toLowerCase(), enabled: false, metadataState: "not-fetched", licenseState: "unknown", licenseSpdx: null, record: { userAdded: true },
            limitations: ["Çevrimdışı kayıt: yalnızca bağlantıdan türetilen bilgiler var; lisans, sürüm ve uyumluluk bilinmiyor."],
          });
          b.entries.push(e);
          return e;
        }
        case "set_catalog_entry_enabled": find(args?.entryId).enabled = Boolean(args?.enabled); return { ...find(args?.entryId) };
        case "remove_catalog_entry": find(args?.entryId).removedAt = "2026-10-17T13:00:00Z"; find(args?.entryId).enabled = false; return { ...find(args?.entryId) };
        case "restore_catalog_entry": find(args?.entryId).removedAt = null; return { ...find(args?.entryId) };
        case "explain_execution_block":
          return find(args?.entryId).executionPolicy === "blocked-until-scoped-authorization"
            ? { code: "SCOPED_AUTHORIZATION_REQUIRED", message: "Aktif güvenlik testi için kapsamlı yetki gerekir." }
            : { code: "EXECUTION_NOT_AVAILABLE_PHASE_1B", message: "Faz 1B'de hiçbir araç kurulamaz, çalıştırılamaz veya doğrulanamaz." };
        case "list_projects": return JSON.parse(JSON.stringify(b.projects));
        case "add_project_reference": {
          const p: ProjectView = { projectId: `p-${String(args?.alias)}`, alias: String(args?.alias), displayName: String(args?.displayName), kind: "reference", localNote: String(args?.note), createdAt: "2026-10-17T14:00:00Z", preferences: [] };
          b.projects.push(p);
          return p;
        }
        case "remove_project_reference": b.projects = b.projects.filter((p) => p.alias !== args?.alias); return undefined;
        case "set_project_preference": {
          const p = pref({ entryId: String(args?.entryId), entryName: String(args?.entryId), agentSurface: String(args?.agentSurface), platformTarget: String(args?.platformTarget), decision: args?.decision as PreferenceView["decision"] });
          const project = b.projects.find((x) => x.alias === args?.alias)!;
          project.preferences = project.preferences.filter((x) => !(x.entryId === p.entryId && x.agentSurface === p.agentSurface && x.platformTarget === p.platformTarget)).concat(p);
          return p;
        }
        case "reset_project_preference": {
          const project = b.projects.find((x) => x.alias === args?.alias)!;
          project.preferences = project.preferences.filter((x) => !(x.entryId === args?.entryId && x.agentSurface === args?.agentSurface && x.platformTarget === args?.platformTarget));
          return undefined;
        }
        case "list_scenarios":
          return [
            { id: "react-component", title: "React bileşeni tasarlamak", taskText: "React bileşeni tasarla", evidenceFiles: ["PricingTable.tsx"] },
            { id: "backend-with-css-in-repo", title: "Yalnızca arka uç işi", taskText: "SQL migration düzelt", evidenceFiles: ["styles.css"] },
          ];
        // the real command echoes the task id and phase it was asked about; the canned recommendations are the test's choice
        case "evaluate_task": { const input = args?.input as { taskId: string; phase?: string }; return { ...b.nextEvaluation, taskId: input.taskId, phase: input.phase ?? "start" }; }
        case "list_inventory": return JSON.parse(JSON.stringify(b.inventory));
        case "preview_backup_cleanup": return JSON.parse(JSON.stringify(b.backupPlan));
        case "plan_install": {
          const r = args?.request as PlanRequest;
          if (!b.entries.some((e) => e.entryId === r.entryId)) throw { code: "CATALOG_ENTRY_NOT_FOUND", message: "Kayıt bulunamadı." };
          return JSON.parse(JSON.stringify(b.planOverride ? b.planOverride(r) : makePlan(r)));
        }
        case "run_discovery_scan": {
          const r = args?.request as { pathExecutables: boolean; userConfig: boolean; project: unknown };
          if (!r.pathExecutables && !r.userConfig && !r.project) throw { code: "SCAN_NOT_AUTHORIZED", message: "Taramak için en az bir bölüme izin vermelisiniz; hiçbir şey okunmadı." };
          return { scanId: "s-new", startedAt: "x", finishedAt: "x", platformTarget: "windows-x64", authorizedPath: r.pathExecutables, authorizedUserConfig: r.userConfig, authorizedProject: !!r.project, authorizedFingerprint: false, status: "completed", observationCount: b.inventory.observations.length };
        }
        case "list_prompt_sources":
          return ["prompts.chat", "the-prompt-library", "Fabric"].map((n) => ({
            sourceId: n, displayName: n, repoUrl: `https://github.com/x/${n}`, licenseNote: "MIT", plannedFormat: "csv", notes: "-", trustStatus: "untrusted-reference", status: "planned", readOnly: true,
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
        default: {
          const r = installCommand(b, cmd, args);
          if (r.handled) return r.value;
          throw { code: "UNKNOWN_COMMAND", message: cmd };
        }
      }
    },
  };
  return b;
}
