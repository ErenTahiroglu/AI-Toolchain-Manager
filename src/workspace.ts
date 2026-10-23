// PRODUCT-01 - pure logic of the Project Tool Workspace ("Proje Arac Matrisi"). No I/O, no React, no clock.
// A row joins ONE project's recorded wishes with the current catalog for ONE (platform, agent surface) target.
// Nothing here installs, enables, verifies or reads anything: it only arranges data the existing typed commands already return.
import { installLabel, kindLabel, platformLabel, surfaceLabel, validationLabel } from "./labels";
import type { CatalogEntry, EntryKind, PreferenceView, ProjectView, TargetEvidence } from "./types";

// The values the Rust side accepts (src-tauri/src/projects.rs: SURFACES, PLATFORMS, DECISIONS). A test pins these lists to that source.
export const TARGET_SURFACES = ["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "local-cli"] as const;
export const TARGET_PLATFORMS = ["windows-x64", "macos-arm64"] as const;
export const DECISION_VALUES = ["accepted", "postponed", "rejected", "disabled"] as const;
export type DecisionValue = (typeof DECISION_VALUES)[number];

/** Display wording only. The stored values (accepted / postponed / rejected / disabled) and their meaning are unchanged. */
export const WORKSPACE_DECISION_LABEL: Record<DecisionValue, string> = {
  accepted: "Bu projede kullanmak istiyorum",
  postponed: "Şimdi değil (ertele)",
  rejected: "Reddet (tekrar sorma)",
  disabled: "Bu projede kapalı tut",
};

/** What each wish does NOT do. It is shown next to the choice so that an intent is never read as an action. */
export const WORKSPACE_DECISION_HELP: Record<DecisionValue, string> = {
  accepted: "Yalnızca isteğiniz kaydedilir. Kurulum, doğrulama veya ajanda etkinleştirme yapılmaz.",
  postponed: "Seçtiğiniz gün sayısı sonrası için bir hatırlatma tarihi kaydedilir; başka hiçbir şey olmaz.",
  rejected: "Bu araç bu birleşim için tekrar önerilmez. Hiçbir şey silinmez veya kaldırılmaz.",
  disabled: "Yalnızca “bu projede kapalı olsun” isteği kaydedilir. Kayıt silinmez ve ajandaki bir eklenti kapatılmaz (uygulama ajan ayarlarına dokunmaz).",
};

export const workspaceDecisionLabel = (d: string) => (d in WORKSPACE_DECISION_LABEL ? WORKSPACE_DECISION_LABEL[d as DecisionValue] : d);

export type CompatClass = "declared" | "unknown" | "unsupported";

/**
 * Anything that is not an explicit declaration or test result is NOT support. An unrecognised string is "unknown", never "declared".
 * "declared" is not "verified": vendor-declared means the vendor says so, nothing here was tried.
 */
export function compatClass(state: string): CompatClass {
  if (state === "vendor-declared" || state === "independently-tested") return "declared";
  if (state === "unsupported") return "unsupported";
  return "unknown";
}

/** The evidence row recorded for exactly this platform and surface (first match, like the Rust side), or null. */
export function evidenceFor(entry: CatalogEntry, platform: string, surface: string): TargetEvidence | null {
  const list = entry.record?.targetEvidence;
  if (!Array.isArray(list)) return null;
  return list.find((t) => t.platformTarget === platform && t.agentSurface === surface) ?? null;
}

export const compatStateFor = (entry: CatalogEntry, platform: string, surface: string) => evidenceFor(entry, platform, surface)?.compatibilityState ?? "unknown";

/**
 * Advisory mirror of the checks set_preference makes before it stores "accepted" (same order). The backend stays the authority and its
 * refusal is shown if it ever disagrees; this only explains in advance why that choice is unavailable.
 */
export function acceptBlockReason(entry: CatalogEntry, compat: string): string | null {
  if (entry.removedAt !== null) return "Katalogdan kaldırılmış bir kayıt için yeni karar kaydedilemez.";
  if (entry.executionPolicy === "blocked-until-scoped-authorization") return "Kapsamlı yazılı yetki olmadan bu araç için “kullanmak istiyorum” seçilemez; yalnızca bilgi olarak görülebilir.";
  if (!entry.enabled) return "Katalogda kapalı: önce Araç Kütüphanesi sayfasında katalogda etkinleştirin.";
  if (compatClass(compat) !== "declared") return "Bu işletim sistemi ve ajan yüzeyi için uyumluluk bilinmiyor veya desteklenmiyor; “kullanmak istiyorum” seçilemez. “Şimdi değil” veya “Reddet” seçilebilir.";
  return null;
}

export interface WorkspaceRow {
  entry: CatalogEntry;
  removed: boolean;
  evidence: TargetEvidence | null;
  compat: string;
  compatClass: CompatClass;
  /** This project's recorded wish for exactly this (entry, surface, platform) - never another project's, never another target's. */
  pref: PreferenceView | null;
  acceptBlock: string | null;
}

const prefKey = (entryId: string, surface: string, platform: string) => `${entryId}\u0000${surface}\u0000${platform}`;

/** Turkish-insensitive, accent-insensitive, locale-independent normalisation for search and ordering. */
export function normalize(s: string): string {
  return s.normalize("NFKD").replace(/[̀-ͯ]/g, "").replace(/[İIı]/g, "i").toLowerCase();
}

const byName = (a: WorkspaceRow, b: WorkspaceRow) => {
  if (a.removed !== b.removed) return a.removed ? 1 : -1;
  const x = normalize(a.entry.displayName);
  const y = normalize(b.entry.displayName);
  if (x !== y) return x < y ? -1 : 1;
  return a.entry.entryId < b.entry.entryId ? -1 : a.entry.entryId > b.entry.entryId ? 1 : 0;
};

/**
 * One row per active catalog entry, plus - only when this project already has a recorded wish for this target - a clearly removed row for a
 * catalog entry that was taken out of the catalog. A removed entry without a wish is not shown. Wishes for entries the catalog list does not
 * contain are returned separately instead of being hidden.
 */
export function buildRows(entries: CatalogEntry[], project: ProjectView, platform: string, surface: string): { rows: WorkspaceRow[]; orphanPrefs: PreferenceView[] } {
  const prefs = new Map<string, PreferenceView>();
  for (const p of project.preferences) {
    if (p.agentSurface === surface && p.platformTarget === platform) prefs.set(prefKey(p.entryId, surface, platform), p);
  }
  const rows: WorkspaceRow[] = [];
  const seen = new Set<string>();
  for (const entry of entries) {
    if (seen.has(entry.entryId)) continue;
    seen.add(entry.entryId);
    const pref = prefs.get(prefKey(entry.entryId, surface, platform)) ?? null;
    const removed = entry.removedAt !== null;
    if (removed && !pref) continue;
    const evidence = evidenceFor(entry, platform, surface);
    const compat = evidence?.compatibilityState ?? "unknown";
    rows.push({ entry, removed, evidence, compat, compatClass: compatClass(compat), pref, acceptBlock: acceptBlockReason(entry, compat) });
  }
  const orphanPrefs = [...prefs.values()].filter((p) => !seen.has(p.entryId));
  rows.sort(byName);
  return { rows, orphanPrefs };
}

export type DecisionFilter = "all" | "undecided" | DecisionValue;
export type CompatFilter = "all" | CompatClass;
export interface WorkspaceFilter {
  query: string;
  kind: "all" | EntryKind;
  decision: DecisionFilter;
  compat: CompatFilter;
}
export const NO_FILTER: WorkspaceFilter = { query: "", kind: "all", decision: "all", compat: "all" };

export const isFiltering = (f: WorkspaceFilter) => f.query.trim() !== "" || f.kind !== "all" || f.decision !== "all" || f.compat !== "all";

const haystack = (r: WorkspaceRow) => normalize([r.entry.displayName, r.entry.entryId, r.entry.purpose, r.entry.sourceKey, kindLabel(r.entry.kind)].join(" "));

/** Stable: the result keeps the order of the input rows. Every search word must occur. */
export function filterRows(rows: WorkspaceRow[], f: WorkspaceFilter): WorkspaceRow[] {
  const words = normalize(f.query).split(/\s+/).filter(Boolean);
  return rows.filter((r) => {
    if (f.kind !== "all" && r.entry.kind !== f.kind) return false;
    if (f.decision === "undecided" ? r.pref !== null : f.decision !== "all" && r.pref?.decision !== f.decision) return false;
    if (f.compat !== "all" && r.compatClass !== f.compat) return false;
    if (words.length === 0) return true;
    const h = haystack(r);
    return words.every((w) => h.includes(w));
  });
}

// ---- the application's OWN record of the real state -----------------------------------------------------------------------------
// The database only allows install_state = 'not_managed' and verification_state = 'unknown' today (migrations 0001 / 0002), so the record has no
// positive value at all. These two lists are the ONLY place a positive value could be declared; a future status must be added here on purpose,
// with a review. Any other string - unfamiliar, corrupt, from a newer version - is "unrecognised": shown as such, counted as such, never as success.
export const AFFIRMATIVE_INSTALL_STATES: readonly string[] = [];
export const AFFIRMATIVE_VERIFICATION_STATES: readonly string[] = [];

export type RecordedState = "affirmed" | "none" | "unrecognised";

const classify = (state: string, affirmative: readonly string[], none: string): RecordedState =>
  affirmative.includes(state) ? "affirmed" : state === none ? "none" : "unrecognised";
export const classifyInstall = (state: string): RecordedState => classify(state, AFFIRMATIVE_INSTALL_STATES, "not_managed");
export const classifyVerification = (state: string): RecordedState => classify(state, AFFIRMATIVE_VERIFICATION_STATES, "unknown");

const shown = (state: string) => (state.length > 40 ? `${state.slice(0, 40)}…` : state);

/** Wording for one recorded value. An unrecognised value is named as unrecognised and is explicitly not a success. */
export function recordedInstallText(state: string): string {
  const c = classifyInstall(state);
  return c === "none" ? installLabel(state) : c === "affirmed" ? "Kurulu olarak kaydedilmiş (kayıt)" : `Tanınmayan durum (“${shown(state)}”) — başarı sayılmaz`;
}
export function recordedVerificationText(state: string): string {
  const c = classifyVerification(state);
  return c === "none" ? validationLabel(state) : c === "affirmed" ? "Doğrulanmış olarak kaydedilmiş (kayıt)" : `Tanınmayan durum (“${shown(state)}”) — başarı sayılmaz`;
}

export interface WorkspaceSummary {
  total: number;
  undecided: number;
  accepted: number;
  postponed: number;
  rejected: number;
  disabled: number;
  unrecognised: number;
  materialChange: number;
  removedWithWish: number;
  /** The application's OWN record (the catalog row / the stored wish row) - not an observation of this computer. */
  recordedInstalled: number;
  recordedNotInstalled: number;
  recordedVerified: number;
  recordedNotVerified: number;
  /** Rows whose recorded install OR verification value is neither the known "none" value nor a declared affirmative one. */
  recordedUnrecognised: number;
}

export function summarize(rows: WorkspaceRow[]): WorkspaceSummary {
  const s: WorkspaceSummary = {
    total: rows.length, undecided: 0, accepted: 0, postponed: 0, rejected: 0, disabled: 0, unrecognised: 0, materialChange: 0, removedWithWish: 0,
    recordedInstalled: 0, recordedNotInstalled: 0, recordedVerified: 0, recordedNotVerified: 0, recordedUnrecognised: 0,
  };
  for (const r of rows) {
    const d = r.pref?.decision;
    if (!r.pref) s.undecided++;
    else if (d === "accepted" || d === "postponed" || d === "rejected" || d === "disabled") s[d]++;
    else s.unrecognised++;
    if (r.pref?.materialChange) s.materialChange++;
    if (r.removed) s.removedWithWish++;
    const install = classifyInstall(r.pref?.actualInstallState ?? r.entry.installState);
    const verification = classifyVerification(r.pref?.actualVerificationState ?? r.entry.verificationState);
    if (install === "affirmed") s.recordedInstalled++;
    else if (install === "none") s.recordedNotInstalled++;
    if (verification === "affirmed") s.recordedVerified++;
    else if (verification === "none") s.recordedNotVerified++;
    if (install === "unrecognised" || verification === "unrecognised") s.recordedUnrecognised++;
  }
  return s;
}

export const contextKey = (alias: string, platform: string, surface: string) => `${alias}\u0000${platform}\u0000${surface}`;

/** The answer of a save must describe exactly the key that was asked for. */
export function sameKey(p: PreferenceView, k: { entryId: string; agentSurface: string; platformTarget: string }): boolean {
  return p.entryId === k.entryId && p.agentSurface === k.agentSurface && p.platformTarget === k.platformTarget;
}

// ---- the target the matrix shows: one project + one operating system + one agent surface ------------------------------------------

export interface TargetRequest {
  alias: string;
  platform: string;
  surface: string;
}
export const DEFAULT_TARGET: TargetRequest = { alias: "", platform: TARGET_PLATFORMS[0], surface: TARGET_SURFACES[0] };

const isPlatform = (v: string) => (TARGET_PLATFORMS as readonly string[]).includes(v);
const isSurface = (v: string) => (TARGET_SURFACES as readonly string[]).includes(v);

export interface ResolvedTarget {
  project: ProjectView | null;
  platform: string;
  surface: string;
  /** Plain-language notes when a requested value could not be honoured. Empty when everything was found. */
  notes: string[];
}

/**
 * Turns a REQUESTED target (from the user's selects, or handed over from another page) into one that exists. Nothing is guessed silently:
 * a project that is gone, or an operating system / surface this build does not know, falls back to a safe value AND says so. While the
 * project list is still loading (`projects === null`) no project is chosen and no note is produced.
 */
export function resolveTarget(req: TargetRequest, projects: ProjectView[] | null): ResolvedTarget {
  const notes: string[] = [];
  let platform = req.platform;
  if (!isPlatform(platform)) {
    notes.push(`İstenen işletim sistemi tanınmıyor; ${platformLabel(TARGET_PLATFORMS[0])} gösteriliyor.`);
    platform = TARGET_PLATFORMS[0];
  }
  let surface = req.surface;
  if (!isSurface(surface)) {
    notes.push(`İstenen ajan yüzeyi tanınmıyor; ${surfaceLabel(TARGET_SURFACES[0])} gösteriliyor.`);
    surface = TARGET_SURFACES[0];
  }
  if (projects === null) return { project: null, platform, surface, notes };
  const found = projects.find((p) => p.alias === req.alias) ?? null;
  const project = found ?? projects[0] ?? null;
  if (!found && req.alias !== "" && project) notes.push("İstenen proje artık yok veya bulunamadı (silinmiş olabilir); listedeki ilk proje gösteriliyor.");
  return { project, platform, surface, notes };
}

export interface TargetCount {
  platform: string;
  surface: string;
  count: number;
  /** How many of those decisions belong to a catalog entry that has since been removed. */
  removed: number;
}

/**
 * Read-only navigator data: where this project has recorded decisions, derived from the already loaded project (no new data call).
 * Ordered by the canonical platform and surface order. Decisions whose platform or surface this build does not offer cannot be selected;
 * they are counted in `unreachable` instead of being dropped silently.
 */
export function targetCounts(project: ProjectView): { targets: TargetCount[]; unreachable: number; total: number } {
  const map = new Map<string, TargetCount>();
  let unreachable = 0;
  for (const p of project.preferences) {
    if (!isPlatform(p.platformTarget) || !isSurface(p.agentSurface)) {
      unreachable++;
      continue;
    }
    const k = `${p.platformTarget}\u0000${p.agentSurface}`;
    const t = map.get(k) ?? { platform: p.platformTarget, surface: p.agentSurface, count: 0, removed: 0 };
    t.count++;
    if (p.entryRemoved) t.removed++;
    map.set(k, t);
  }
  const rank = (t: TargetCount) => (TARGET_PLATFORMS as readonly string[]).indexOf(t.platform) * 100 + (TARGET_SURFACES as readonly string[]).indexOf(t.surface);
  return { targets: [...map.values()].sort((a, b) => rank(a) - rank(b)), unreachable, total: project.preferences.length };
}
