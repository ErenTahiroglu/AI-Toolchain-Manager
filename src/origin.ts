// PRODUCT-06 - what the UI may say about WHERE the evidence behind a stored status came from.
//
// The backend (store.rs::classify_request) attaches a small descriptor, `origin`, taken from the ONE observation it selected: whether that observation
// carries the preference's project alias or none (computer-wide), whether it was recorded for the requested agent surface or is a `local-cli`
// program lookup standing in for it, and its own timestamp and age. It is metadata about the evidence, not a verdict about the tool. This module only
// validates what arrived and chooses words; it never infers a scope the backend did not state, never repairs a timestamp, and never turns a name
// match into "installed", "working", "verified", "active" or "supported".
import { formatTime } from "./labels";
import { parseRfc3339 } from "./review";
import type { EvidenceOrigin } from "./types";

export type OriginView =
  /** No descriptor (no selected observation), or one this version cannot read. Nothing is guessed. */
  | { kind: "unavailable" }
  | {
      kind: "described";
      project: EvidenceOrigin["projectRelation"];
      surface: EvidenceOrigin["surfaceRelation"];
      /** Only a valid RFC 3339 timestamp. */
      observedAt: string | null;
      /** Whole hours at the moment the inventory was read; only with a valid timestamp, and never negative. */
      ageHours: number | null;
    };

export const PROJECT_RELATIONS: readonly EvidenceOrigin["projectRelation"][] = ["project_specific", "computer_wide"];
export const SURFACE_RELATIONS: readonly EvidenceOrigin["surfaceRelation"][] = ["exact_surface", "local_cli_fallback"];

/** Takes the wire value as `unknown`: an older backend (no member), a null, or a value from a newer one all end up "unavailable". */
export function describeOrigin(origin: unknown): OriginView {
  if (typeof origin !== "object" || origin === null) return { kind: "unavailable" };
  const o = origin as Record<string, unknown>;
  const project = (PROJECT_RELATIONS as readonly unknown[]).includes(o.projectRelation) ? (o.projectRelation as EvidenceOrigin["projectRelation"]) : null;
  const surface = (SURFACE_RELATIONS as readonly unknown[]).includes(o.surfaceRelation) ? (o.surfaceRelation as EvidenceOrigin["surfaceRelation"]) : null;
  if (project === null || surface === null) return { kind: "unavailable" };
  const observedAt = typeof o.observedAt === "string" && parseRfc3339(o.observedAt) !== null ? o.observedAt : null;
  const ageHours = observedAt !== null && typeof o.ageHours === "number" && Number.isInteger(o.ageHours) && o.ageHours >= 0 ? o.ageHours : null;
  return { kind: "described", project, surface, observedAt, ageHours };
}

export const ORIGIN_HEADING = "Kanıtın kaynağı";
export const ORIGIN_SEPARATE = "Bu bilgi, kayıtlı isteğinizden ve uygulamanın kendi kurulum kaydından ayrıdır.";
export const ORIGIN_UNAVAILABLE = "Kanıtın kaynağı belirlenemedi; bilgisayar geneli mi, projeye özel mi olduğu söylenemez.";
export const ORIGIN_NO_OBSERVATION = "Bu durumda seçilmiş bir gözlem yok.";

export const PROJECT_TEXT: Record<EvidenceOrigin["projectRelation"], string> = {
  computer_wide: "Kanıt kapsamı: bilgisayar geneli. Projeden bağımsız bir kayıttır; bu projeye özgü bir kanıt değildir.",
  project_specific:
    "Kanıt kapsamı: bu proje. Gözlem, yalnızca arka uçta kaydedilen proje takma adı kadarıyla bu projeyle ilişkilendirildi; bir ajanın bu aracı kullandığını ya da kimliğinin onaylandığını göstermez.",
};

export const SURFACE_TEXT: Record<EvidenceOrigin["surfaceRelation"], string> = {
  exact_surface: "Ajan yüzeyi: istenen yüzey için kaydedilmiş gözlem (o yüzeyin bu aracı yapılandırdığı anlamına gelmez).",
  local_cli_fallback:
    "Ajan yüzeyi: yedek eşleşme. İstenen yüzey yerine bilgisayardaki program araması (local-cli) kullanıldı; bu, istenen ajan yüzeyinin bu aracı yapılandırdığına kanıt değildir.",
};

/** "Gözlem zamanı …" - only when the backend gave a valid timestamp; the age is the one computed when the inventory was read, not today's. */
export function originTimeText(v: Extract<OriginView, { kind: "described" }>): string | null {
  if (v.observedAt === null) return null;
  const age = v.ageHours === null ? "" : ` (envanter okunduğunda yaklaşık ${v.ageHours} saat önce)`;
  return `Gözlem zamanı: ${formatTime(v.observedAt)}${age}. Bu, kaydedilmiş bir tarama sonucuna aittir; canlı bir doğrulama değildir.`;
}
