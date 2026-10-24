// PRODUCT-05 - the read-only review queue ("İncelenmesi Gerekenler"): which saved decisions deserve a second look, derived - purely - from what the
// application already holds: every project's recorded wishes (ProjectView.preferences) and the STORED discovery classification (Inventory.requests).
//
// Rules this module keeps:
//  * One item per exact key (project alias + tool + operating system + agent surface), however many reasons it has. Nothing leaks across keys.
//  * A reason is a fact the saved data states (a flag, a removal, a due date, a backend status). Nothing is inferred, nothing is ranked by an invented
//    "severity", and viewing an item changes nothing: the queue shrinks only when the saved decision, the catalog state, the clock or the stored
//    evidence really changes. There is no way to mark an item as seen.
//  * Evidence that is missing, ambiguous, unrecognised or unreadable is UNCERTAIN, never "not scanned" / "not found", and an unfinished or failed read
//    never produces a reassuring empty queue (`complete` is false).
//  * `present_current` is a stored discovery result (possibly from a computer-wide or local-cli name lookup); it is shown as information, is never a
//    reason, and is never called resolved, installed or verified.
//  * The clock is a parameter. No timers, no polling.
import { observationOfWish, type InventoryRead } from "./observation";
import type { AppError, EvidenceOrigin, ProjectView, RequestStatus } from "./types";
import { TARGET_PLATFORMS, TARGET_SURFACES, normalize } from "./workspace";

export interface ProjectsRead {
  data: ProjectView[] | null;
  error: AppError | null;
  loading: boolean;
}

export type ReviewReason =
  /** The saved decision predates a material change of the tool's capabilities or compatibility in the catalog. */
  | { code: "material_change" }
  /** The tool was removed from the catalog; the old decision still exists (it can only be looked at or reset in the matrix). */
  | { code: "entry_removed" }
  /** A postponed decision whose reminder date has been reached. */
  | { code: "reminder_due"; remindAt: string }
  /** The backend's classification of an ACCEPTED wish against the stored observations says the wish and the stored evidence deserve a look. */
  | { code: "evidence"; status: RequestStatus };

export type UncertainReason =
  | "reminder_unreadable"
  | "no_row"
  | "ambiguous"
  | "unrecognised_status"
  | "duplicate_wish";

export interface ReviewItem {
  /** project alias + tool + operating system + agent surface: the one key of a decision */
  key: string;
  alias: string;
  projectName: string;
  /** An example project (invented data, not a folder of the user's). Its decisions are listed - and counted - but marked as examples. */
  synthetic: boolean;
  entryId: string;
  entryName: string;
  platform: string;
  surface: string;
  decision: string;
  reasons: ReviewReason[];
  uncertain: UncertainReason[];
  /** The backend status of an accepted wish, for information (present_current included). Null when none applies or none is known. */
  observation: { status: RequestStatus; scanFinishedAt: string | null; origin: EvidenceOrigin | null } | null;
  /** The matrix can show exactly this target (known operating system and agent surface). Otherwise nothing is opened and no other target is chosen. */
  openable: boolean;
}

/**
 * loading: first read in progress (no data). failed: first read failed (no data). old: data retained after a FAILED refresh. refreshing: data retained
 * while a new read is still running (nothing has failed, nothing is final yet). ready: data, no error, nothing running - the only final phase.
 */
export type ReadPhase = "loading" | "failed" | "ready" | "old" | "refreshing";

export interface ReviewQueue {
  /** Items with at least one reason, in a fixed order. */
  items: ReviewItem[];
  /** Items whose ONLY finding is uncertain evidence. Not counted as "needs review"; never dropped. */
  uncertain: ReviewItem[];
  projects: ReadPhase;
  evidence: ReadPhase;
  /** Both reads finished and neither failed afterwards: only then is an empty queue a statement. */
  complete: boolean;
  /** Accepted wishes whose stored evidence could not be consulted because the inventory is pending or failed (no earlier read to fall back on). */
  evidenceUnavailableFor: number;
}

/** Statuses of an accepted wish that ask for a look. present_current is deliberately absent: a stored finding is not a reason, and not a resolution either. */
export const EVIDENCE_REVIEW_STATUSES: readonly RequestStatus[] = [
  "absent_current", "unknown_current",
  "previously_present", "previously_absent", "previously_unknown",
  "platform_mismatch", "not_scanned",
];

export const reviewKey = (alias: string, entryId: string, platform: string, surface: string) => [alias, entryId, platform, surface].join("\u0000");

const RFC3339 = /^(\d{4})-(\d{2})-(\d{2})[Tt](\d{2}):(\d{2}):(\d{2})(?:\.(\d+))?([Zz]|[+-]\d{2}:\d{2})$/;

/**
 * Strict RFC 3339 -> milliseconds since the epoch, or null. No locale, no `Date.parse` leniency (it rolls "02-30" over to March and accepts date-only
 * strings): the calendar day, the clock fields and the offset must all be valid.
 */
export function parseRfc3339(text: string): number | null {
  const m = RFC3339.exec(text);
  if (!m) return null;
  const [year, month, day, hour, minute, second] = [m[1], m[2], m[3], m[4], m[5], m[6]].map(Number);
  if (month < 1 || month > 12 || hour > 23 || minute > 59 || second > 59) return null;
  const daysInMonth = new Date(Date.UTC(year, month, 0)).getUTCDate();
  if (day < 1 || day > daysInMonth) return null;
  let offsetMinutes = 0;
  if (m[8] !== "Z" && m[8] !== "z") {
    const [oh, om] = m[8].slice(1).split(":").map(Number);
    if (oh > 23 || om > 59) return null;
    offsetMinutes = (m[8][0] === "-" ? -1 : 1) * (oh * 60 + om);
  }
  const millis = m[7] ? Number(`0.${m[7]}`.slice(0, 5)) * 1000 : 0;
  const utc = Date.UTC(year, month - 1, day, hour, minute, second) + Math.round(millis);
  const out = utc - offsetMinutes * 60_000;
  return Number.isFinite(out) ? out : null;
}

// useLoad keeps the previous good data while a new read runs, so `loading` must be looked at even when data is present
const phaseOf = (r: { data: unknown; error: AppError | null; loading: boolean }): ReadPhase =>
  r.data === null ? (r.error ? "failed" : "loading") : r.error ? "old" : r.loading ? "refreshing" : "ready";

const cmp = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
const byItem = (a: ReviewItem, b: ReviewItem) =>
  cmp(normalize(a.projectName), normalize(b.projectName)) || cmp(a.alias, b.alias) || cmp(normalize(a.entryName), normalize(b.entryName)) ||
  cmp(a.entryId, b.entryId) || cmp(a.platform, b.platform) || cmp(a.surface, b.surface);

/**
 * The one sentence both the queue and the Dashboard tile show, so their counts and caveats can never disagree. A number is stated as exact only when
 * both reads finished and neither failed afterwards; otherwise it is "at least N", and while the decisions themselves are unread there is no number.
 */
export function reviewHeadline(q: ReviewQueue): string {
  if (q.projects === "loading") return "Kararlar okunuyor…";
  if (q.projects === "failed") return "Kararlar okunamadı; incelenecek öğelerin sayısı bilinmiyor. Bu, “hiç yok” anlamına gelmez.";
  const n = q.items.length;
  // a read that is still running is provisional, whatever the retained data says (including an earlier clean zero)
  if (!q.complete) {
    const running = q.projects === "refreshing" || q.evidence === "refreshing";
    // not "at least N": the retained snapshot can shrink as well as grow when the new answer arrives (six cached items may become five)
    return `Eldeki geçici görüntüde ${n} öğe var; ${running ? "okuma sürüyor" : "liste eksik veya eski olabilir"}, sayı artabilir veya azalabilir (sonuç kesin değil).`;
  }
  if (n === 0) {
    return "Bu ölçütlere göre incelenmesi gereken öğe yok. Bu, her şeyin doğru olduğu ya da herhangi bir aracın kurulu ve çalışır olduğu anlamına gelmez; yalnızca aşağıdaki ölçütler denetlendi.";
  }
  return `${n} öğe incelenmeyi bekliyor.`;
}

export type ReasonFilter = "all" | ReviewReason["code"];

/** Narrows a list to one project (by alias) and/or one kind of reason. Filtering never changes what exists, only what is listed. */
export const filterItems = (items: readonly ReviewItem[], alias: string | "all", reason: ReasonFilter): ReviewItem[] =>
  items.filter((i) => (alias === "all" || i.alias === alias) && (reason === "all" || i.reasons.some((r) => r.code === reason)));

/** Pure. `now` is milliseconds since the epoch, supplied by the caller. Inputs are never modified. */
export function deriveReview(projects: ProjectsRead, inventory: InventoryRead, now: number): ReviewQueue {
  const projectsPhase = phaseOf(projects);
  const evidencePhase = phaseOf(inventory);
  const byKey = new Map<string, ReviewItem>();
  let acceptedWithoutEvidence = 0;

  for (const project of projects.data ?? []) {
    for (const p of project.preferences) {
      const key = reviewKey(project.alias, p.entryId, p.platformTarget, p.agentSurface);
      const seen = byKey.get(key);
      if (seen) {
        if (!seen.uncertain.includes("duplicate_wish")) seen.uncertain.push("duplicate_wish"); // a second record for one key: the first is kept, the doubt is stated
        continue;
      }
      const item: ReviewItem = {
        key, alias: project.alias, projectName: project.displayName, synthetic: project.kind === "synthetic", entryId: p.entryId, entryName: p.entryName, platform: p.platformTarget, surface: p.agentSurface,
        decision: p.decision, reasons: [], uncertain: [], observation: null,
        openable: (TARGET_PLATFORMS as readonly string[]).includes(p.platformTarget) && (TARGET_SURFACES as readonly string[]).includes(p.agentSurface),
      };
      if (p.materialChange === true) item.reasons.push({ code: "material_change" });
      if (p.entryRemoved === true) item.reasons.push({ code: "entry_removed" });
      if (p.decision === "postponed" && p.remindAt !== null) {
        const due = parseRfc3339(p.remindAt);
        if (due === null) item.uncertain.push("reminder_unreadable");
        else if (due <= now) item.reasons.push({ code: "reminder_due", remindAt: p.remindAt });
      }
      // the same answer the matrix row gives (one engine): only an accepted wish on a catalog entry that still exists is compared, by the exact key
      const view = observationOfWish({ removed: p.entryRemoved === true, decision: p.decision }, p.entryId, { alias: project.alias, platform: p.platformTarget, surface: p.agentSurface }, inventory);
      switch (view.kind) {
        case "observed":
          item.observation = { status: view.status, scanFinishedAt: view.request.basis?.finishedAt ?? null, origin: view.request.origin ?? null };
          if (EVIDENCE_REVIEW_STATUSES.includes(view.status)) item.reasons.push({ code: "evidence", status: view.status });
          break;
        case "no_row": item.uncertain.push("no_row"); break;
        case "ambiguous": item.uncertain.push("ambiguous"); break;
        case "unrecognised": item.uncertain.push("unrecognised_status"); break;
        case "loading":
        case "unreadable": acceptedWithoutEvidence += 1; break; // stated once, for the whole queue, instead of one doubt per row
        default: break; // removed, no_wish: no stored comparison applies
      }
      byKey.set(key, item);
    }
  }

  const all = [...byKey.values()].sort(byItem);
  return {
    items: all.filter((i) => i.reasons.length > 0),
    uncertain: all.filter((i) => i.reasons.length === 0 && i.uncertain.length > 0),
    projects: projectsPhase,
    evidence: evidencePhase,
    complete: projectsPhase === "ready" && evidencePhase === "ready",
    evidenceUnavailableFor: acceptedWithoutEvidence,
  };
}
