// PRODUCT-01 - pure logic of the Project Tool Workspace ("Proje Arac Matrisi"). No I/O, no React, no clock.
// A row joins ONE project's recorded wishes with the current catalog for ONE (platform, agent surface) target.
// Nothing here installs, enables, verifies or reads anything: it only arranges data the existing typed commands already return.
import { kindLabel } from "./labels";
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
  recordedVerified: number;
}

export function summarize(rows: WorkspaceRow[]): WorkspaceSummary {
  const s: WorkspaceSummary = { total: rows.length, undecided: 0, accepted: 0, postponed: 0, rejected: 0, disabled: 0, unrecognised: 0, materialChange: 0, removedWithWish: 0, recordedInstalled: 0, recordedVerified: 0 };
  for (const r of rows) {
    const d = r.pref?.decision;
    if (!r.pref) s.undecided++;
    else if (d === "accepted" || d === "postponed" || d === "rejected" || d === "disabled") s[d]++;
    else s.unrecognised++;
    if (r.pref?.materialChange) s.materialChange++;
    if (r.removed) s.removedWithWish++;
    if ((r.pref?.actualInstallState ?? r.entry.installState) !== "not_managed") s.recordedInstalled++;
    if ((r.pref?.actualVerificationState ?? r.entry.verificationState) !== "unknown") s.recordedVerified++;
  }
  return s;
}

export const contextKey = (alias: string, platform: string, surface: string) => `${alias}\u0000${platform}\u0000${surface}`;

/** The answer of a save must describe exactly the key that was asked for. */
export function sameKey(p: PreferenceView, k: { entryId: string; agentSurface: string; platformTarget: string }): boolean {
  return p.entryId === k.entryId && p.agentSurface === k.agentSurface && p.platformTarget === k.platformTarget;
}
