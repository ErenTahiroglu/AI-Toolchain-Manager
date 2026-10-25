// PRODUCT-07 H - what the UI may say about SCOPED MCP-CONFIGURATION readings that sit BESIDE the stored presence result.
//
// Pure. The backend (src-tauri/src/discovery/declarations.rs) derives, from rows it already holds, 0..3 readings of the Claude MCP configuration files
// (project .mcp.json, the project's entry in ~/.claude.json, the user-level ~/.claude.json) and, for each scope with no reading, WHY there is none. This module
// validates what arrived - fail closed on anything unexpected - and chooses words. It never turns "no reading" into "not configured", never merges the two
// questions ("is the program/plugin on this computer?" vs "does a configuration file record a matching MCP definition?"), and never says installed, approved,
// loaded, active, running, working, verified, compatible or safe. Contract: docs/product-07/H_WIRE_CONTRACT.md.
import { formatTime, surfaceLabel } from "./labels";
import { parseRfc3339 } from "./review";

export const DECLARATION_SCOPES = ["project", "local", "global"] as const;
export type DeclarationScope = (typeof DECLARATION_SCOPES)[number];
export const DECLARATION_CONFIGURATIONS = ["configured", "not_configured", "malformed", "unreadable", "not_assessed"] as const;
export type DeclarationConfiguration = (typeof DECLARATION_CONFIGURATIONS)[number];
export const DECLARATION_FRESHNESS = ["current", "previous_area_not_checked", "previous_stale", "previous_other"] as const;
export type DeclarationFreshness = (typeof DECLARATION_FRESHNESS)[number];
export const SCOPE_COVERAGE = ["read", "no_scan", "platform_not_scanned", "not_authorized", "project_not_in_latest_scan", "no_record"] as const;
export type ScopeCoverage = (typeof SCOPE_COVERAGE)[number];
export const DETECTOR_KINDS = ["claude_mcp", "not_implemented"] as const;
export type DetectorKind = (typeof DETECTOR_KINDS)[number];

/** The only surface on which the Claude MCP reader can have produced readings. A `claude_mcp` detector claimed for any other surface is not believed. */
export const CLAUDE_SURFACE = "claude-code-terminal";

export interface DeclarationReading {
  scope: DeclarationScope;
  configuration: DeclarationConfiguration;
  freshness: DeclarationFreshness;
  /** Only a valid RFC 3339 timestamp. */
  observedAt: string | null;
  /** Whole hours at the moment the inventory was read; only with a valid timestamp, never negative. */
  ageHours: number | null;
}

export type ScopedView =
  /** An older backend (no members), or a payload this version cannot trust. Nothing is shown and NO coverage claim is made. */
  | { kind: "unavailable" }
  /** The backend states that no detector reads MCP definitions for this entry/surface. NOT "not configured", NOT "supported". */
  | { kind: "not_implemented"; surface: string }
  | { kind: "described"; readings: DeclarationReading[]; coverage: Record<DeclarationScope, ScopeCoverage> };

const isOneOf = <T extends string>(list: readonly T[], v: unknown): v is T => typeof v === "string" && (list as readonly string[]).includes(v);
const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);

/**
 * Takes the request as the wire delivered it. Anything that is not exactly the documented shape - an unknown enum value, a wrong type, more than three
 * readings, a repeated or mis-ordered scope, a reading whose scope the coverage does not call "read", a detector that does not match the surface - makes the
 * WHOLE view unavailable. Nothing is partially trusted and nothing is repaired.
 */
export function describeScopedDeclarations(request: { agentSurface: string; declarationDetector?: unknown; declarations?: unknown }): ScopedView {
  const detector = request.declarationDetector;
  if (!isObject(detector) || !isOneOf(DETECTOR_KINDS, detector.kind)) return { kind: "unavailable" };
  const readingsWire = request.declarations;
  if (!Array.isArray(readingsWire)) return { kind: "unavailable" };

  if (detector.kind === "not_implemented") {
    // the only consistent shape: null coverage and no readings
    if (detector.coverage !== null || readingsWire.length !== 0) return { kind: "unavailable" };
    return { kind: "not_implemented", surface: request.agentSurface };
  }
  if (request.agentSurface !== CLAUDE_SURFACE) return { kind: "unavailable" };
  const cov = detector.coverage;
  if (!isObject(cov) || Object.keys(cov).length !== 3) return { kind: "unavailable" };
  const coverage = {} as Record<DeclarationScope, ScopeCoverage>;
  for (const scope of DECLARATION_SCOPES) {
    const c = cov[scope];
    if (!isOneOf(SCOPE_COVERAGE, c)) return { kind: "unavailable" };
    coverage[scope] = c;
  }
  if (readingsWire.length > 3) return { kind: "unavailable" };
  const readings: DeclarationReading[] = [];
  let lastOrder = -1;
  for (const w of readingsWire) {
    if (!isObject(w) || Object.keys(w).length !== 5) return { kind: "unavailable" };
    if (!isOneOf(DECLARATION_SCOPES, w.scope) || !isOneOf(DECLARATION_CONFIGURATIONS, w.configuration) || !isOneOf(DECLARATION_FRESHNESS, w.freshness)) return { kind: "unavailable" };
    const order = DECLARATION_SCOPES.indexOf(w.scope);
    if (order <= lastOrder) return { kind: "unavailable" }; // repeated or out of the fixed order project -> local -> global
    lastOrder = order;
    const observedAt = typeof w.observedAt === "string" && parseRfc3339(w.observedAt) !== null ? w.observedAt : null;
    const ageHours = observedAt !== null && typeof w.ageHours === "number" && Number.isInteger(w.ageHours) && w.ageHours >= 0 ? w.ageHours : null;
    readings.push({ scope: w.scope, configuration: w.configuration, freshness: w.freshness, observedAt, ageHours });
  }
  // a scope is "read" exactly when a reading exists for it
  for (const scope of DECLARATION_SCOPES) {
    if ((coverage[scope] === "read") !== readings.some((r) => r.scope === scope)) return { kind: "unavailable" };
  }
  return { kind: "described", readings, coverage };
}

// ---- words ------------------------------------------------------------------------------------------------------------------------------

export const SCOPED_HEADING = "Kapsama göre MCP tanımı (yapılandırma dosyası okuması)";
export const PRESENCE_LABEL = "Saklanan varlık sonucu (program adı / eklenti kaydı araması):";
export const SCOPED_SEPARATE =
  "Bu bölüm yukarıdaki varlık sonucundan ayrıdır: o, bir program adının ya da eklenti kaydının bulunup bulunmadığını söyler; bu bölüm ise yapılandırma dosyalarında araçla eşleşen bir MCP tanımı kaydedilip kaydedilmediğini söyler. İkisi farklı sorulara cevap verir ve birlikte doğru olabilir; her kapsam ayrı okunur ve birbirine eklenmez.";
export const SCOPED_DISCLAIMER =
  "“Tanım kaydedilmiş” yalnızca okunan dosyada araçla eşleşen bir ad ya da komut dosyası adı bulunduğu anlamına gelir; aracın kurulu, onaylı, yüklenmiş, çalışan ya da doğrulanmış olduğunu göstermez.";
export const SCOPED_NOT_READ_NOTE = "Okunmamış bir kapsam “tanım yok” anlamına gelmez; yalnızca o kapsam için güncel bir okuma olmadığını gösterir.";

export const SCOPE_NAME: Record<DeclarationScope, string> = {
  project: "Bu projenin .mcp.json dosyası",
  local: "Bu proje için yerel Claude Code ayarı (~/.claude.json)",
  global: "Kullanıcı geneli Claude Code ayarı (~/.claude.json; tüm projelere uygulanır)",
};

export const CONFIGURATION_TEXT: Record<DeclarationConfiguration, string> = {
  configured: "araçla eşleşen bir MCP tanımı kaydedilmiş (ad ya da komut dosyası adı eşleşmesi).",
  not_configured: "araçla eşleşen bir MCP tanımı bulunamadı (yalnızca bu kapsamda).",
  malformed: "dosya geçerli JSON değil; bu kapsam hakkında bir şey söylenemez.",
  unreadable: "dosya okunmadı; bu kapsam hakkında bir şey söylenemez.",
  not_assessed: "bu kapsam değerlendirilemedi; bir şey söylenemez.",
};

export const FRESHNESS_TEXT: Record<DeclarationFreshness, string> = {
  current: "Son taramada okundu",
  previous_area_not_checked: "Eski bir taramadan (güncel değil; son taramada bu alana bakılmadı)",
  previous_stale: "Eski bir taramadan (güncel değil; son tarama 24 saatten eski)",
  previous_other: "Eski bir kayıttan (güncel sayılmadı)",
};

export const COVERAGE_TEXT: Record<Exclude<ScopeCoverage, "read">, string> = {
  no_scan: "Henüz hiç tarama yok; bu kapsam okunmadı.",
  platform_not_scanned: "Son tarama başka bir işletim sistemi içindi; bu kapsam bu hedef için okunmadı.",
  not_authorized: "Son taramaya bu alanı okuma izni verilmedi; bu kapsam okunmadı.",
  project_not_in_latest_scan: "Son taramada bu projenin klasörüne bakılmadı; bu kapsam okunmadı.",
  no_record: "Bu kapsam için kayıtlı bir okuma yok.",
};

export const NOT_IMPLEMENTED_TEXT =
  "Bu araç ve ajan yüzeyi için MCP tanımlarını okuyan bir algılayıcı yok; bu yüzden yapılandırma tanımı hakkında bir şey söylenemez. Bu, “tanım yok” anlamına gelmez ve bir destek iddiası da değildir.";

/** Extra, surface-specific sentence. Never says a machine-level program lookup is specific to an agent. */
export function notImplementedExtra(surface: string): string {
  switch (surface) {
    case "codex-cli":
      return "Codex CLI için hiçbir yapılandırma dosyası okunmaz. Yukarıdaki program adı araması bilgisayar düzeyindedir (yedek eşleşme olabilir); Codex'e özgü değildir.";
    case "antigravity-ide":
    case "claude-desktop-code":
      return `${surfaceLabel(surface)} için MCP yapılandırması okunmaz; Claude Code terminal dosyaları bu yüzeye ait sayılmaz.`;
    case "local-cli":
      return "Bu yüzey bir ajan değil, bilgisayar düzeyinde bir program aramasıdır; MCP tanımı okunmaz.";
    default:
      return "Yapılandırma dosyaları yalnızca katalogdaki MCP girdisi için, Claude Code terminal yüzeyinde okunur.";
  }
}

export function readingTimeText(r: DeclarationReading): string | null {
  if (r.observedAt === null) return null;
  const age = r.ageHours === null ? "" : ` (envanter okunduğunda yaklaşık ${r.ageHours} saat önce)`;
  return `Gözlem zamanı: ${formatTime(r.observedAt)}${age}.`;
}

/** One line per scope, in the fixed display order: a reading, or the reason there is none. Never "tanım yok" for a scope that was not read. */
export interface ScopeLine {
  scope: DeclarationScope;
  current: boolean | null; // null: no reading
  text: string;
}

export function scopeLines(v: Extract<ScopedView, { kind: "described" }>): ScopeLine[] {
  return DECLARATION_SCOPES.map((scope) => {
    const r = v.readings.find((x) => x.scope === scope);
    if (r) {
      const time = readingTimeText(r);
      return { scope, current: r.freshness === "current", text: `${FRESHNESS_TEXT[r.freshness]}: ${CONFIGURATION_TEXT[r.configuration]}${time ? ` ${time}` : ""}` };
    }
    const c = v.coverage[scope];
    return { scope, current: null, text: c === "read" ? COVERAGE_TEXT.no_record : COVERAGE_TEXT[c] };
  });
}
