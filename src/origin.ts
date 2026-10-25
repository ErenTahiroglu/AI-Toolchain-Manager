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
      /** PRODUCT-07 H: the presence winner's detection technique, only from the closed list. OMITTED (not null) for an older backend, a pseudo-method or anything unknown. */
      method?: OriginMethodName;
    };

export const ORIGIN_METHODS = [
  "path-executable", "native-install-location", "legacy-local-install", "claude-settings-file", "claude-plugin-declaration", "claude-mcp-config",
] as const;
export type OriginMethodName = (typeof ORIGIN_METHODS)[number];

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
  const method = (ORIGIN_METHODS as readonly unknown[]).includes(o.method) ? (o.method as OriginMethodName) : null; // anything else, including a missing member, is "unknown"
  return method === null ? { kind: "described", project, surface, observedAt, ageHours } : { kind: "described", project, surface, observedAt, ageHours, method };
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

/**
 * PRODUCT-07 H (D7): what KIND of lookup produced the presence result. Wording only; the stored origin values and the backend note are untouched.
 * For a plugin record the project/local label identifies the settings scope that was checked, while the presence itself comes from the shared user-level
 * plugin record/cache: it is never presented as a project-specific installation.
 */
export const METHOD_TEXT: Record<OriginMethodName, string> = {
  "path-executable": "Kanıt türü: PATH klasörlerinde dosya adı eşleşmesi (bilgisayar düzeyinde bir arama; dosyanın bu program olduğunu ya da bu projede kullanıldığını göstermez).",
  "native-install-location": "Kanıt türü: Claude Code'un belgelenmiş kurulum konumunda dosya adı (bilgisayar düzeyinde).",
  "legacy-local-install": "Kanıt türü: eski yerel kurulum konumunda dosya adı (bilgisayar düzeyinde).",
  "claude-settings-file": "Kanıt türü: bir Claude Code ayar dosyasının okunması (dosyanın varlığı, bir aracın var olduğunu göstermez).",
  "claude-plugin-declaration":
    "Kanıt türü: Claude Code eklenti kaydı ve önbelleği (kullanıcı düzeyinde). “Proje” ya da “yerel” etiketi yalnızca hangi ayar kapsamının denetlendiğini gösterir; varlık bilgisi paylaşılan kullanıcı önbelleğinden gelebilir ve projeye özel bir kurulum değildir.",
  "claude-mcp-config": "Kanıt türü: bir Claude Code MCP yapılandırma dosyasının okunması (bu yöntem bir programın varlığı hakkında bilgi vermez).",
};

export function originMethodText(v: Extract<OriginView, { kind: "described" }>): string | null {
  return v.method === undefined ? null : METHOD_TEXT[v.method];
}

/** "Gözlem zamanı …" - only when the backend gave a valid timestamp; the age is the one computed when the inventory was read, not today's. */
export function originTimeText(v: Extract<OriginView, { kind: "described" }>): string | null {
  if (v.observedAt === null) return null;
  const age = v.ageHours === null ? "" : ` (envanter okunduğunda yaklaşık ${v.ageHours} saat önce)`;
  return `Gözlem zamanı: ${formatTime(v.observedAt)}${age}. Bu, kaydedilmiş bir tarama sonucuna aittir; canlı bir doğrulama değildir.`;
}
