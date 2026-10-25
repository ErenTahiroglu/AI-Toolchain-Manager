// CONFIG-01 - a pure, deterministic, READ-ONLY planner for "what would change in a project's MCP configuration file". It is a PREVIEW:
//   * it reads no file, writes no file, spawns nothing, opens no network connection and calls no IPC command. Its only inputs are values the caller passes in -
//     the selected project alias, operating system, agent surface and catalog entry, the configuration TEXT the user chose to paste (or an explicit "new file"),
//     and a registry of exact, independently reviewed declarations;
//   * it NEVER invents a declaration. Publisher install commands, README examples, `npx ...@latest` and key-in-URL snippets are not templates. With no approved
//     declaration for (host, entry) the answer is BLOCKED_INSUFFICIENT_EVIDENCE and no diff is produced;
//   * it cannot lift any existing block: quarantined, execution-blocked, removed, disabled, unreviewed-by-itself or unknown-compatibility entries stay blocked, and
//     whatever diff exists for them is labelled hypothetical;
//   * it never returns, stores or logs the raw configuration text: every value shown is parsed, redacted and re-serialised, and only a SHA-256 digest of the
//     provided text is kept (to detect that a preview has gone stale). Values that look secret-bearing are masked.
// Sources: Claude Code project MCP servers live in `.mcp.json` at the project root under `mcpServers` (https://code.claude.com/docs/en/mcp); Antigravity's
// workspace file is `<workspace>/.agents/mcp_config.json` under `mcpServers` (https://www.antigravity.google/docs/mcp). See docs/config-01/SOURCE_AND_TARGET_MAP.md.
// CONFIG-02A: the registry now holds ONE declaration (a real catalog entry, Claude Code `.mcp.json`). Seeing that TEXT draft is deliberately separated from accepting or
// activating the tool: unknown vendor compatibility no longer blocks the draft (it is stated next to it), while removed, disabled, execution-blocked, quarantined and
// explicitly unsupported entries still do, and the matrix's accept rules (workspace.ts) are untouched.
// AGENT-01: Codex CLI's project file is TOML (`.codex/config.toml`, `[mcp_servers.<name>]`, trusted projects only). It is NEVER fed to the JSON parser below and
// no `.mcp.json` is produced for it: the surface gets the explicit status TOML_PREVIEW_NOT_IMPLEMENTED, no diff and static read-only guidance (docs/agent-01/).
import { agentHostFor, type ConfigFormat } from "./agentHosts";
import { REVIEWED_DECLARATIONS, type ReviewedDeclaration } from "./configDeclarations";
import type { CatalogEntry } from "./types";
import { TARGET_PLATFORMS, acceptBlockReason, compatClass, compatStateFor } from "./workspace";

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
type JsonObject = { [key: string]: JsonValue };

export type HostId = "claude-code-project-mcp" | "antigravity-workspace-mcp";

export interface HostSpec {
  id: HostId;
  /** The ONLY application agent surface this host belongs to. Nothing is guessed from another surface. */
  surface: string;
  label: string;
  /** Relative to the selected project root; fixed text, never built from user input. */
  relativePath: string;
  scope: string;
  /** Per-server fields the host's documentation defines for a local (stdio) and a remote server. A declaration may use only these. */
  stdioFields: readonly string[];
  remoteFields: readonly string[];
  remoteUrlField: string;
  /** Claude Code documents `${VAR}` expansion in env/headers; Antigravity's page does not, so a declaration for it may carry NO env/headers at all. */
  envExpansionDocumented: boolean;
  /** Files of the same agent that this preview never touches. */
  untouched: readonly string[];
}

export const HOSTS: readonly HostSpec[] = [
  {
    id: "claude-code-project-mcp",
    surface: "claude-code-terminal",
    label: "Claude Code — proje MCP dosyası",
    relativePath: ".mcp.json",
    scope: "Proje (paylaşılan): projenin kök klasöründeki .mcp.json",
    stdioFields: ["command", "args", "env"],
    remoteFields: ["type", "url", "headers"],
    remoteUrlField: "url",
    envExpansionDocumented: true,
    untouched: ["~/.claude.json (yerel ve kullanıcı kapsamı)", ".claude/settings.json", ".claude/settings.local.json", "CLAUDE.md"],
  },
  {
    id: "antigravity-workspace-mcp",
    surface: "antigravity-ide",
    label: "Antigravity IDE — çalışma alanı MCP dosyası",
    relativePath: ".agents/mcp_config.json",
    scope: "Çalışma alanı: çalışma alanı klasöründeki .agents/mcp_config.json",
    stdioFields: ["command", "args", "env", "cwd", "disabled"],
    remoteFields: ["serverUrl", "headers", "disabled"],
    remoteUrlField: "serverUrl",
    envExpansionDocumented: false,
    untouched: ["~/.gemini/config/mcp_config.json (kullanıcı geneli)", "AGENTS.md", "uygulama içi MCP mağazası ayarları"],
  },
];

const CODEX_GUIDANCE: readonly string[] = [
  "Codex'in bu projeyi GÜVENİLİR saydığını Codex içinde doğrulayın; güvenilmeyen projede .codex/config.toml yok sayılır. Bu uygulama güveni vermez, okumaz ve değiştirmez.",
  "Yayıncı belgesindeki [mcp_servers.<ad>] tablosunun alanları: yerel süreç için command, args, env, env_vars, cwd; Streamable HTTP için url, bearer_token_env_var, http_headers, env_http_headers. Belgedeki örnek komutlar bir şablon değildir.",
  "Sürümü sabitlenmemiş (@latest veya etiketsiz) bir çağrı kullanmayın; paket çalıştırıcıları (ör. npx) başlatılırken kayıt defterinden kod indirir. Kendi doğruladığınız tam sürümü kullanın.",
  "Anahtar, belirteç veya parola değerlerini dosyaya yazmayın; ortam değişkeni adı kullanan alanları tercih edin. Dosyayı sürüm denetimine koymadan önce içinde gizli değer olmadığını kendiniz doğrulayın.",
  "Kullanıcı genelindeki ~/.codex/config.toml dosyasını bu amaçla değiştirmeyin: model sağlayıcıları ve kimlik bilgileri orada yaşar ve proje dosyasında zaten yok sayılır.",
  "Sonucu Codex içinde (/mcp) görürsünüz; bu uygulama bir bağlantı denemez ve çalıştığını iddia etmez.",
];

export const hostForSurface = (surface: string): HostSpec | null => HOSTS.find((h) => h.surface === surface) ?? null;

export type PlanStatus =
  | "PREVIEW_ONLY_NOT_APPLIED"
  | "BLOCKED_INSUFFICIENT_EVIDENCE"
  | "BLOCKED_BY_CATALOG"
  | "UNSUPPORTED_SURFACE"
  | "INVALID_INPUT"
  | "INVALID_CURRENT_CONFIG"
  | "CONFLICT"
  | "TOML_PREVIEW_NOT_IMPLEMENTED";

export const STATUS_LABEL: Record<PlanStatus, string> = {
  PREVIEW_ONLY_NOT_APPLIED: "YALNIZCA ÖNİZLEME — UYGULANMADI",
  BLOCKED_INSUFFICIENT_EVIDENCE: "ENGELLİ — YETERSİZ KANIT (onaylı bildirim yok)",
  BLOCKED_BY_CATALOG: "ENGELLİ — katalog durumu izin vermiyor (açıklayıcı önizleme)",
  UNSUPPORTED_SURFACE: "DESTEKLENMEYEN YÜZEY",
  INVALID_INPUT: "GEÇERSİZ HEDEF",
  INVALID_CURRENT_CONFIG: "MEVCUT YAPILANDIRMA GEÇERSİZ",
  CONFLICT: "ÇAKIŞMA — hiçbir şey ezilmez",
  TOML_PREVIEW_NOT_IMPLEMENTED: "TOML ÖNİZLEMESİ UYGULANMADI — yalnızca elle yapılacak, salt okunur yönerge",
};

export const NO_FILE_TOUCHED =
  "Hiçbir dosyaya dokunulmadı ve hiçbir dosya okunmadı: yalnızca sizin verdiğiniz metin bu pencerenin belleğinde işlendi. Hiçbir şey uygulanmadı, kurulmadı, çalıştırılmadı veya bağlanılmadı.";

/** Shown instead of NO_FILE_TOUCHED when no text is processed at all (the Codex TOML surface): there is nothing to hold in memory either. */
export const NO_TEXT_PROCESSED =
  "Hiçbir dosyaya dokunulmadı ve hiçbir dosya okunmadı; bu hedef için hiçbir metin de işlenmedi (yapıştırılan metin ayrıştırılmaz). Hiçbir şey uygulanmadı, kurulmadı, çalıştırılmadı veya bağlanılmadı.";

/** Shown on every positive draft. Exactly this sentence (CONFIG-02A): the draft is text; nothing was applied, connected, installed or tested. */
export const DRAFT_NOTICE =
  "Yalnızca yapılandırma taslağı; uygulanmadı, bağlantı kurulmadı. Hizmetin ücretleri, gizliliği ve kimlik doğrulaması ayrıca değerlendirilmelidir.";

export interface Reason {
  code: string;
  text: string;
}

export type DiffLine = { kind: "ctx" | "add" | "del" | "gap"; text: string };

export type ConfigSource = { kind: "new" } | { kind: "text"; text: string };

export interface PlanRequest {
  alias: string;
  platform: string;
  surface: string;
  entry: Pick<CatalogEntry, "entryId" | "displayName" | "removedAt" | "enabled" | "executionPolicy" | "reviewState" | "record" | "capabilityFingerprint" | "revision">;
  source: ConfigSource;
}

export interface PreviewPlan {
  status: PlanStatus;
  /** Always false: nothing is ever applied by this module. */
  applied: false;
  /** Always 0: no file is read or written by this module. */
  touchedFiles: 0;
  statusLabel: string;
  notice: string;
  identity: { alias: string; platform: string; surface: string; entryId: string; hostId: HostId | null };
  relativePath: string | null;
  scope: string | null;
  /** The file format of the target ("json" / "toml"); null where no project file is documented. */
  configFormat: ConfigFormat | null;
  untouched: readonly string[];
  /** Static, read-only manual steps (only for TOML_PREVIEW_NOT_IMPLEMENTED); they are text, never a command this application runs. */
  guidance: string[];
  /** Deterministic key of EVERY input; a plan whose key differs from the current inputs' key is stale. */
  inputKey: string;
  /** SHA-256 of the text the user provided (never of any file on disk); null for an explicit new file. */
  sourceDigest: string | null;
  blockers: Reason[];
  problems: Reason[];
  constraints: string[];
  existingServerNames: string[];
  /** How many values were masked in what is shown. */
  maskedValues: number;
  /** Redacted, normalised "before": null for a new file or when nothing could be parsed. */
  original: string | null;
  /** Redacted, normalised "after": only when a reviewed declaration exists and the current text is valid. */
  proposed: string | null;
  diff: DiffLine[] | null;
  /** True when a diff is shown although the plan is blocked: it explains, it is not a recipe. */
  hypothetical: boolean;
  formattingNote: string | null;
  declaration: { serverName: string; reviewId: string; evidenceIds: string[]; note: string | null } | null;
  /** DRAFT_NOTICE exactly when status is PREVIEW_ONLY_NOT_APPLIED; null otherwise. */
  draftNotice: string | null;
}

// ---- SHA-256 (synchronous, no dependency) ------------------------------------------------------------------------------------------

const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
  0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
  0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
  0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);

export function sha256Hex(text: string): string {
  const data = new TextEncoder().encode(text);
  const bitLength = data.length * 8;
  const padded = new Uint8Array(((data.length + 9 + 63) >> 6) << 6);
  padded.set(data);
  padded[data.length] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(padded.length - 8, Math.floor(bitLength / 0x100000000));
  view.setUint32(padded.length - 4, bitLength >>> 0);
  const h = new Uint32Array([0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]);
  const w = new Uint32Array(64);
  const rotr = (x: number, n: number) => (x >>> n) | (x << (32 - n));
  for (let offset = 0; offset < padded.length; offset += 64) {
    for (let i = 0; i < 16; i++) w[i] = view.getUint32(offset + i * 4);
    for (let i = 16; i < 64; i++) {
      const s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >>> 3);
      const s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >>> 10);
      w[i] = (w[i - 16] + s0 + w[i - 7] + s1) >>> 0;
    }
    let [a, b, c, d, e, f, g, hh] = h;
    for (let i = 0; i < 64; i++) {
      const t1 = (hh + (rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)) + ((e & f) ^ (~e & g)) + K[i] + w[i]) >>> 0;
      const t2 = ((rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)) + ((a & b) ^ (a & c) ^ (b & c))) >>> 0;
      hh = g; g = f; f = e; e = (d + t1) >>> 0; d = c; c = b; b = a; a = (t1 + t2) >>> 0;
    }
    h[0] = (h[0] + a) >>> 0; h[1] = (h[1] + b) >>> 0; h[2] = (h[2] + c) >>> 0; h[3] = (h[3] + d) >>> 0;
    h[4] = (h[4] + e) >>> 0; h[5] = (h[5] + f) >>> 0; h[6] = (h[6] + g) >>> 0; h[7] = (h[7] + hh) >>> 0;
  }
  return [...h].map((x) => x.toString(16).padStart(8, "0")).join("");
}

// ---- redaction -----------------------------------------------------------------------------------------------------------------------------

export const MASK = "«gizlendi»";
const SECRET_KEY = /(key|token|secret|passw|credential|authoriz|bearer|cookie|signature|auth)/i;
const SECRET_FLAG = /^--?[\w-]*(key|token|secret|passw|credential|authoriz|bearer|auth)[\w-]*$/i;
// a reference to an environment variable, optionally as the documented `Bearer ${NAME}` header form - never a value
const PLACEHOLDER = /^(?:Bearer )?\$\{[A-Za-z_][A-Za-z0-9_]*\}$/;
const SECRET_SHAPES: RegExp[] = [
  /\b(?:sk|pk|rk)-[A-Za-z0-9_-]{16,}/,
  /\bgh[pousr]_[A-Za-z0-9]{20,}/,
  /\bgithub_pat_[A-Za-z0-9_]{20,}/,
  /\bxox[abprs]-[A-Za-z0-9-]{10,}/,
  /\bAKIA[0-9A-Z]{16}\b/,
  /\bAIza[0-9A-Za-z_-]{30,}/,
  /\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\./,
  /^[A-Za-z0-9+/=_-]{32,}$/,
];

const isObject = (v: unknown): v is JsonObject => v !== null && typeof v === "object" && !Array.isArray(v);

function redactString(s: string, secretContext: boolean, count: { n: number }): string {
  if (PLACEHOLDER.test(s)) return s; // a reference, not a value
  if (secretContext) { count.n += 1; return MASK; }
  if (/^https?:\/\//i.test(s)) {
    try {
      const u = new URL(s);
      let changed = false;
      if (u.username || u.password) { u.username = ""; u.password = ""; changed = true; }
      if (u.search) { u.search = ""; changed = true; }
      if (u.hash) { u.hash = ""; changed = true; }
      if (changed) { count.n += 1; return `${u.toString().replace(/[?#]$/, "")}${MASK}`; }
    } catch { /* not a URL after all: fall through to the shape checks */ }
  }
  const flag = /^(--?[\w-]+)=(.*)$/.exec(s);
  if (flag && SECRET_FLAG.test(flag[1]) && !PLACEHOLDER.test(flag[2])) { count.n += 1; return `${flag[1]}=${MASK}`; }
  if (SECRET_SHAPES.some((re) => re.test(s))) { count.n += 1; return MASK; }
  return s;
}

function redactValue(v: JsonValue, secretContext: boolean, count: { n: number }): JsonValue {
  if (typeof v === "string") return redactString(v, secretContext, count);
  if (Array.isArray(v)) {
    const out: JsonValue[] = [];
    let maskNext = false;
    for (const item of v) {
      if (typeof item === "string" && maskNext && !PLACEHOLDER.test(item)) { count.n += 1; out.push(MASK); maskNext = false; continue; }
      maskNext = typeof item === "string" && SECRET_FLAG.test(item) && !item.includes("=");
      out.push(redactValue(item, secretContext, count));
    }
    return out;
  }
  if (isObject(v)) {
    // fromEntries defines own properties, so a key literally named "__proto__" stays data and is never turned into a prototype assignment
    return Object.fromEntries(Object.entries(v).map(([k, val]) => [k, redactValue(val, secretContext || SECRET_KEY.test(k), count)]));
  }
  return v;
}

/** A deep copy with every plausibly secret-bearing value masked. Pure; returns how many values were masked. */
export function redactForDisplay(v: JsonValue): { value: JsonValue; masked: number } {
  const count = { n: 0 };
  const value = redactValue(v, false, count);
  return { value, masked: count.n };
}

// ---- JSON handling ---------------------------------------------------------------------------------------------------------------------------

const MAX_TEXT = 200_000;
const MAX_SERVERS = 200;

/** Duplicate keys inside one object (JSON.parse silently keeps the last one). Only called on text that already parsed. */
export function findDuplicateKeys(text: string): string[] {
  const dups: string[] = [];
  const stack: Array<{ kind: "o"; keys: Set<string>; expectKey: boolean } | { kind: "a" }> = [];
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === '"') {
      let j = i + 1;
      while (j < text.length && text[j] !== '"') j += text[j] === "\\" ? 2 : 1;
      let s: string;
      try { s = JSON.parse(text.slice(i, j + 1)) as string; } catch { s = text.slice(i, j + 1); }
      i = j + 1;
      const top = stack[stack.length - 1];
      if (top && top.kind === "o" && top.expectKey) {
        if (top.keys.has(s)) dups.push(s.slice(0, 40)); else top.keys.add(s);
        top.expectKey = false;
      }
      continue;
    }
    if (c === "{") stack.push({ kind: "o", keys: new Set(), expectKey: true });
    else if (c === "[") stack.push({ kind: "a" });
    else if (c === "}" || c === "]") stack.pop();
    else if (c === ",") { const top = stack[stack.length - 1]; if (top && top.kind === "o") top.expectKey = true; }
    i += 1;
  }
  return dups;
}

function looksLikeJsonWithComments(text: string): boolean {
  let inString = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inString) { if (c === "\\") i += 1; else if (c === '"') inString = false; continue; }
    if (c === '"') { inString = true; continue; }
    if (c === "/" && (text[i + 1] === "/" || text[i + 1] === "*")) return true;
  }
  return false;
}

export type ParseResult = { ok: true; value: JsonObject; hadBom: boolean } | { ok: false; problems: Reason[] };

/** Strict parse of the pasted text. The engine's own error message is NEVER shown: it can quote the text, and the text may contain secrets. */
export function parseConfigText(raw: string): ParseResult {
  if (raw.length > MAX_TEXT) return { ok: false, problems: [{ code: "too_large", text: `Metin çok büyük (en çok ${MAX_TEXT.toLocaleString("tr-TR")} karakter).` }] };
  const hadBom = raw.charCodeAt(0) === 0xfeff;
  const text = hadBom ? raw.slice(1) : raw;
  if (text.trim() === "") return { ok: false, problems: [{ code: "empty", text: "Metin boş. Dosya yoksa “Yeni dosya” seçin; var olan bir dosyanın metnini yapıştırın." }] };
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    const hint = looksLikeJsonWithComments(text) ? " Yorum satırları (JSONC) desteklenmiyor: yorumlar kayıpsız korunamaz." : "";
    return { ok: false, problems: [{ code: "malformed_json", text: `Metin geçerli bir JSON değil.${hint} (Hata ayrıntısı, gizli değer sızdırmamak için gösterilmez.)` }] };
  }
  if (!isObject(value)) return { ok: false, problems: [{ code: "not_an_object", text: "Yapılandırmanın en üst düzeyi bir JSON nesnesi olmalı." }] };
  const dups = findDuplicateKeys(text);
  if (dups.length > 0) {
    return { ok: false, problems: [{ code: "duplicate_keys", text: `Aynı nesnede yinelenen anahtar var (${[...new Set(dups)].slice(0, 5).map((d) => `“${d}”`).join(", ")}); hangisinin geçerli olacağı belirsiz.` }] };
  }
  return { ok: true, value, hadBom };
}

/** Structure the host's documentation defines: `mcpServers` (when present) is an object of objects. Unknown top-level keys are preserved, not judged. */
export function checkServersShape(config: JsonObject): Reason[] {
  const servers = config.mcpServers;
  if (servers === undefined) return [];
  if (!isObject(servers)) return [{ code: "servers_not_object", text: "“mcpServers” bir nesne olmalı." }];
  const names = Object.keys(servers);
  if (names.length > MAX_SERVERS) return [{ code: "too_many_servers", text: `“mcpServers” içinde ${names.length} giriş var; en çok ${MAX_SERVERS} desteklenir.` }];
  const bad = names.filter((n) => !isObject(servers[n]));
  if (bad.length > 0) return [{ code: "server_not_object", text: `Şu sunucu tanımları nesne değil: ${bad.slice(0, 5).map((n) => `“${n.slice(0, 40)}”`).join(", ")}.` }];
  return [];
}

// ---- declaration hygiene (defence in depth for the reviewed registry) -----------------------------------------------------------------

const SERVER_NAME = /^[a-z0-9][a-z0-9._-]{0,63}$/;
const RESERVED_NAMES = new Set(["__proto__", "constructor", "prototype"]);
const RUNNERS = new Set(["npx", "uvx", "pnpx", "bunx", "npm", "pnpm", "yarn"]);
const UNPINNED = /(@|==|:)(latest|next|beta|alpha|canary|rc|dev|nightly|\*)\b/i;
const EXACT_PIN = /^(?:@[\w.-]+\/)?[\w.-]+(?:@|==)\d+\.\d+\.\d+(?:[-+][\w.-]+)?$/;
const SHELL_META = /[\s;&|<>$`"'\\(){}*?!#~]/;
const PATH_LIKE = /(^|[\\/])\.\.([\\/]|$)|^[A-Za-z]:|^[\\/]|^~|^\\\\/;

export function validateDeclaration(host: HostSpec, d: ReviewedDeclaration): string[] {
  const bad: string[] = [];
  if (d.hostId !== host.id) bad.push("host_mismatch");
  if (!SERVER_NAME.test(d.serverName) || RESERVED_NAMES.has(d.serverName)) bad.push("server_name");
  const r = d.review;
  if (!r || !r.reviewId || !Array.isArray(r.evidenceIds) || r.evidenceIds.length === 0 || !r.evidenceIds.every((e) => /^R\d{2,3}$/.test(e)) || !/^[a-f0-9]{40}$/.test(r.reviewedAgainst ?? "")) {
    bad.push("review_record");
  }
  if (r && r.repositoryUrl !== undefined && !/^https:\/\/github\.com\/[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/.test(r.repositoryUrl)) bad.push("review_repository");
  if (r && r.note !== undefined && (typeof r.note !== "string" || r.note.length > 400 || /[<>\u0000-\u001f]/.test(r.note))) bad.push("review_note");
  if (d.warnings !== undefined && (!Array.isArray(d.warnings) || d.warnings.length > 8 || d.warnings.some((w) => typeof w !== "string" || w.length === 0 || w.length > 600 || /[<>\u0000-\u001f]/.test(w)))) bad.push("warnings");
  const decl = d.declaration;
  if (!isObject(decl)) return [...bad, "declaration_not_object"];
  const keys = Object.keys(decl);
  const remote = host.remoteUrlField in decl;
  const allowed = remote ? host.remoteFields : host.stdioFields;
  if (keys.some((k) => !allowed.includes(k))) bad.push("unknown_field");
  if (remote === ("command" in decl)) bad.push("transport_ambiguous");
  if (host.id === "claude-code-project-mcp" && remote && decl.type !== "http") bad.push("claude_remote_needs_type_http");
  if (!remote) {
    const command = decl.command;
    if (typeof command !== "string" || command === "" || SHELL_META.test(command) || PATH_LIKE.test(command)) bad.push("command");
    const args = decl.args;
    if (args !== undefined && (!Array.isArray(args) || args.some((a) => typeof a !== "string" || a.length > 200 || /[\u0000-\u001f]/.test(a)))) bad.push("args");
    const argList = (Array.isArray(args) ? args : []).filter((a): a is string => typeof a === "string");
    if (argList.some((a) => UNPINNED.test(a))) bad.push("unpinned_tag");
    if (typeof command === "string" && RUNNERS.has(command.replace(/\.(cmd|exe)$/i, "").toLowerCase()) && !argList.some((a) => EXACT_PIN.test(a))) bad.push("runner_needs_exact_version");
    if (argList.some((a) => !a.startsWith("-") && PATH_LIKE.test(a))) bad.push("path_in_args");
    if (typeof decl.cwd === "string" && PATH_LIKE.test(decl.cwd)) bad.push("cwd_path");
  } else {
    const url = decl[host.remoteUrlField];
    let ok = false;
    if (typeof url === "string" && url.length <= 200) {
      try { const u = new URL(url); ok = u.protocol === "https:" && !u.username && !u.password && !u.search && !u.hash && u.hostname.includes("."); } catch { ok = false; }
    }
    if (!ok) bad.push("url");
  }
  for (const field of ["env", "headers"] as const) {
    const bag = decl[field];
    if (bag === undefined) continue;
    if (!host.envExpansionDocumented) { bad.push(`${field}_not_expressible`); continue; }
    if (!isObject(bag)) { bad.push(`${field}_type`); continue; }
    for (const [k, v] of Object.entries(bag)) {
      if (typeof v !== "string") { bad.push(`${field}_value_type`); continue; }
      if (SECRET_KEY.test(k) ? !PLACEHOLDER.test(v) : v.length > 64 || SECRET_SHAPES.some((re) => re.test(v))) bad.push(`${field}_literal_value`);
    }
  }
  if (decl.disabled !== undefined && typeof decl.disabled !== "boolean") bad.push("disabled_type");
  return bad;
}

// ---- diff --------------------------------------------------------------------------------------------------------------------------------

/** A small, deterministic line diff: common prefix and suffix are trimmed, the middle is compared with an LCS. Context is 2 lines. */
export function lineDiff(before: readonly string[], after: readonly string[], context = 2): DiffLine[] {
  let start = 0;
  while (start < before.length && start < after.length && before[start] === after[start]) start += 1;
  let endB = before.length;
  let endA = after.length;
  while (endB > start && endA > start && before[endB - 1] === after[endA - 1]) { endB -= 1; endA -= 1; }
  const a = before.slice(start, endB);
  const b = after.slice(start, endA);
  const table: number[][] = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
  for (let i = a.length - 1; i >= 0; i--) for (let j = b.length - 1; j >= 0; j--) table[i][j] = a[i] === b[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
  const mid: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) { mid.push({ kind: "ctx", text: a[i] }); i += 1; j += 1; }
    else if (j < b.length && (i >= a.length || table[i][j + 1] >= table[i + 1][j])) { mid.push({ kind: "add", text: b[j] }); j += 1; }
    else { mid.push({ kind: "del", text: a[i] }); i += 1; }
  }
  const lead = before.slice(Math.max(0, start - context), start).map((text): DiffLine => ({ kind: "ctx", text }));
  const tail = before.slice(endB, endB + context).map((text): DiffLine => ({ kind: "ctx", text }));
  if (!mid.some((l) => l.kind !== "ctx")) return []; // nothing changed: no hunk
  const out: DiffLine[] = [];
  if (start > context) out.push({ kind: "gap", text: `… ${start - context} değişmeyen satır …` });
  out.push(...lead, ...mid, ...tail);
  const rest = before.length - (endB + context);
  if (rest > 0) out.push({ kind: "gap", text: `… ${rest} değişmeyen satır …` });
  return out;
}

// ---- the planner ---------------------------------------------------------------------------------------------------------------------------

const pretty = (v: JsonValue) => `${JSON.stringify(v, null, 2)}\n`;
const lines = (s: string) => s.replace(/\n$/, "").split("\n");

function declarationFor(host: HostSpec | null, entryId: string, declarations: readonly ReviewedDeclaration[]): ReviewedDeclaration | null {
  if (!host) return null;
  return declarations.find((d) => d.hostId === host.id && d.entryId === entryId) ?? null;
}

/** Deterministic key of every input that the plan depends on. Two plans are interchangeable only if their keys are equal. */
export function planInputKey(req: PlanRequest, declarations: readonly ReviewedDeclaration[] = REVIEWED_DECLARATIONS): string {
  const host = hostForSurface(req.surface);
  const decl = declarationFor(host, req.entry.entryId, declarations);
  return JSON.stringify([
    req.alias, req.platform, req.surface, req.entry.entryId, req.entry.removedAt, req.entry.enabled, req.entry.executionPolicy, req.entry.reviewState,
    req.entry.capabilityFingerprint, req.entry.revision, req.source.kind, req.source.kind === "text" ? sha256Hex(req.source.text) : null,
    decl ? [decl.review.reviewId, sha256Hex(JSON.stringify([decl.declaration, decl.review, decl.warnings ?? []])), decl.serverName] : null,
    // the catalog's own pinned source: a changed pin must invalidate a shown preview
    (req.entry.record as { sourceSnapshot?: { immutableCommit?: unknown }; repository?: { url?: unknown } } | undefined)?.sourceSnapshot?.immutableCommit ?? null,
    (req.entry.record as { repository?: { url?: unknown } } | undefined)?.repository?.url ?? null,
  ]);
}

export const isPlanStale = (plan: PreviewPlan, req: PlanRequest, declarations: readonly ReviewedDeclaration[] = REVIEWED_DECLARATIONS): boolean =>
  plan.inputKey !== planInputKey(req, declarations);

export function isMcpEntry(entry: Pick<CatalogEntry, "record">): boolean {
  return entry.record?.category === "mcp-server";
}

function equivalentSignature(d: JsonObject, urlField: string): string | null {
  const url = d[urlField];
  if (typeof url === "string") {
    // the same endpoint is the same server whatever query string, fragment, case of the host or trailing slash it is written with
    try {
      const u = new URL(url);
      return `url:${u.protocol}//${u.host}${u.pathname.replace(/\/+$/, "")}`.toLowerCase();
    } catch {
      return `url:${url.toLowerCase().replace(/\/+$/, "")}`;
    }
  }
  if (typeof d.command === "string") return `cmd:${d.command}\u0000${Array.isArray(d.args) ? d.args.join("\u0000") : ""}`;
  return null;
}

export function planConfigPreview(req: PlanRequest, declarations: readonly ReviewedDeclaration[] = REVIEWED_DECLARATIONS): PreviewPlan {
  const host = hostForSurface(req.surface);
  const agent = agentHostFor(req.surface);
  const tomlTarget = host === null && agent !== null && agent.previewKind === "toml-not-implemented" ? agent : null;
  const inputKey = planInputKey(req, declarations);
  const blockers: Reason[] = [];
  const problems: Reason[] = [];
  const constraints: string[] = [];
  let maskedValues = 0;
  let original: string | null = null;
  let proposed: string | null = null;
  let diff: DiffLine[] | null = null;
  let formattingNote: string | null = null;
  let existingServerNames: string[] = [];
  // The Codex TOML surface never processes the text (it is ignored, not parsed, not hashed): there is no digest to keep.
  const sourceDigest = tomlTarget === null && req.source.kind === "text" ? sha256Hex(req.source.text) : null;
  let usedDeclaration: ReviewedDeclaration | null = null;

  const done = (status: PlanStatus, hypothetical = false): PreviewPlan => ({
    status,
    applied: false,
    touchedFiles: 0,
    statusLabel: STATUS_LABEL[status],
    notice: tomlTarget ? NO_TEXT_PROCESSED : NO_FILE_TOUCHED,
    identity: { alias: req.alias, platform: req.platform, surface: req.surface, entryId: req.entry.entryId, hostId: host?.id ?? null },
    relativePath: host?.relativePath ?? tomlTarget?.relativeTargetPath ?? null,
    scope: host?.scope ?? (tomlTarget ? `Proje (yalnızca güvenilen projelerde): projenin kökündeki ${tomlTarget.relativeTargetPath}` : null),
    configFormat: host ? "json" : tomlTarget?.configFormat ?? null,
    untouched: host?.untouched ?? tomlTarget?.untouched ?? [],
    guidance: status === "TOML_PREVIEW_NOT_IMPLEMENTED" ? [...CODEX_GUIDANCE] : [],
    inputKey,
    sourceDigest,
    blockers,
    problems,
    constraints,
    existingServerNames,
    maskedValues,
    original,
    proposed,
    diff,
    hypothetical,
    formattingNote,
    declaration: usedDeclaration
      ? { serverName: usedDeclaration.serverName, reviewId: usedDeclaration.review.reviewId, evidenceIds: [...usedDeclaration.review.evidenceIds], note: usedDeclaration.review.note ?? null }
      : null,
    draftNotice: status === "PREVIEW_ONLY_NOT_APPLIED" ? DRAFT_NOTICE : null,
  });

  // 1. identity: nothing is defaulted
  if (req.alias.trim() === "" || req.entry.entryId.trim() === "" || !(TARGET_PLATFORMS as readonly string[]).includes(req.platform)) {
    problems.push({ code: "identity", text: "Proje, işletim sistemi ve araç seçilmeden önizleme yapılmaz; hiçbiri varsayılmaz." });
    return done("INVALID_INPUT");
  }
  // 2. the surface decides the host; another surface is never mapped onto it
  if (!host && !tomlTarget) {
    problems.push({
      code: "unsupported_surface",
      text: "Bu ajan yüzeyi için bir yapılandırma dosyası önizlemesi tanımlı değil. JSON önizlemesi yalnızca “Claude Code (terminal)” (.mcp.json) ve “Antigravity IDE” (.agents/mcp_config.json) için vardır; “Codex CLI” (TOML) ayrı bir durumla, yalnızca elle yönerge olarak ele alınır. Başka bir yüzeyin dosyası tahmin edilmez.",
    });
    return done("UNSUPPORTED_SURFACE");
  }
  // 3. is this an MCP server at all
  if (!isMcpEntry(req.entry)) {
    blockers.push({ code: "not_mcp", text: "Bu katalog kaydı bir MCP sunucusu olarak kayıtlı değil; bir MCP yapılandırma önizlemesi yapılamaz." });
  }
  // 4. the catalog gates. A TEXT draft is not an activation: for the JSON hosts only UNKNOWN vendor compatibility stops being a blocker (it is stated beside the draft and the
  //    matrix still refuses "kullanmak istiyorum" for it). Removed, disabled, execution-blocked, quarantined/blocked and explicitly unsupported entries still block the draft.
  //    The TOML target keeps the full CONFIG-01/AGENT-01 listing unchanged (it shows no draft at all).
  const compat = compatStateFor(req.entry as CatalogEntry, req.platform, req.surface);
  const entryBlock = acceptBlockReason(req.entry as CatalogEntry, host ? "vendor-declared" : compat); // with a declared stand-in only the entry-level reasons remain
  if (entryBlock !== null) blockers.push({ code: "accept_block", text: entryBlock });
  if (req.entry.reviewState === "quarantined" || req.entry.reviewState === "blocked") blockers.push({ code: "quarantined", text: `Katalog durumu “${req.entry.reviewState}”: yalnızca açıklayıcı bir önizleme gösterilebilir.` });
  if (!host) {
    if (compatClass(compat) !== "declared") blockers.push({ code: "compat_not_declared", text: "Bu işletim sistemi ve ajan yüzeyi için uyumluluk bilinmiyor veya desteklenmiyor; yayıncı beyanı yok." });
  } else if (compatClass(compat) === "unsupported") {
    blockers.push({ code: "compat_not_declared", text: "Bu işletim sistemi ve ajan yüzeyi için katalog “desteklenmiyor” kaydı taşıyor; taslak gösterilmez." });
  } else if (compatClass(compat) === "unknown") {
    constraints.push("Bu hedef için uyumluluk bilinmiyor. Bu bir yapılandırma TASLAĞIDIR: aracın kabul edilmesi, etkinleştirilmesi ya da kurulmasıyla ilgisi yoktur. Matriste “bu projede kullanmak istiyorum” kararı, uyumluluk bilinmediği sürece seçilemez; bu önizleme bunu değiştirmez.");
  }
  constraints.push(`Uyumluluk (bu hedef için kayıt): ${compat}. “Yayıncı beyan etti” bir işlev testi değildir.`);
  constraints.push(`Katalog durumu: ${req.entry.reviewState}. Bu önizleme, aracın desteklendiği, çalıştığı, kurulu olduğu veya güvenli olduğu anlamına gelmez; mevcut kabul/etkinleştirme engellerini kaldırmaz.`);
  if (!host) {
    constraints.push("Codex CLI proje yapılandırması bir TOML dosyasıdır (.codex/config.toml, [mcp_servers.<ad>] tabloları); Claude Code'un .mcp.json veya Antigravity'nin JSON dosyası DEĞİLDİR. Bu uygulama Codex TOML'unu ne okur, ne ayrıştırır, ne JSON olarak gösterir.");
    constraints.push("Proje .codex/config.toml yalnızca proje Codex tarafından GÜVENİLİR sayılıyorsa yüklenir; proje kökünden çalışma dizinine kadar bulunan her .codex/config.toml yüklenir ve aynı anahtarda çalışma dizinine en yakın dosya kazanır (yayıncı belgesi).");
    constraints.push("ChatGPT masaüstü uygulaması, Codex CLI ve IDE eklentisi aynı config.toml'u paylaşabilir (yayıncı belgesi); bu hedef yalnızca Codex CLI'dır ve diğer yüzeyler bunun yerine geçmez. Model sağlayıcı ayarları (model_provider, model_providers) kullanıcı yapılandırmasında yaşar ve proje dosyasında yok sayılır.");
    constraints.push("`codex mcp add --oauth-client-secret` gizli anahtarı config.toml içine yazar ve http_headers statik değer taşır (yayıncı belgesi): gizli değerler proje dosyasına girmemelidir.");
  } else if (host.id === "claude-code-project-mcp") {
    constraints.push("Claude Code, etkileşimli oturumlarda proje MCP sunucuları için onay ister; “-p” ve Agent SDK çalıştırmalarında bu istem gösterilemez ve proje sunucuları sormadan yüklenir (yayıncı belgesi).");
  } else {
    constraints.push("Antigravity belgesi, ortam değişkeni genişletmesinin desteklendiğini belirtmiyor; bu yüzden bu dosya için bildirimlerde env/headers kullanılamaz. Yalnızca Antigravity IDE hedeflenir; uygulama, CLI ve IDE arayüzleri birbirinin yerine geçmez.");
  }

  // 4b. a TOML target stops here: no text is processed, no declaration is composed, no diff exists
  if (!host) {
    blockers.push({
      code: "toml_preview_not_implemented",
      text: "Codex için TOML önizlemesi uygulanmadı (TOML_PREVIEW_NOT_IMPLEMENTED): bu uygulamada TOML için doğruluğu kanıtlanmış bir ayrıştırıcı ve fark üretici yoktur ve JSON ayrıştırıcıya TOML verilmez. Hiçbir fark üretilmedi; yalnızca aşağıdaki elle yönerge gösterilir.",
    });
    return done("TOML_PREVIEW_NOT_IMPLEMENTED");
  }

  // 5. the current configuration, ONLY as the text the user supplied
  let parsed: JsonObject | null = null;
  let normalizedBefore = "";
  if (req.source.kind === "text") {
    const result = parseConfigText(req.source.text);
    if (!result.ok) {
      problems.push(...result.problems);
    } else {
      parsed = result.value;
      const shape = checkServersShape(parsed);
      if (shape.length > 0) problems.push(...shape);
      else {
        const servers = isObject(parsed.mcpServers) ? parsed.mcpServers : {};
        existingServerNames = Object.keys(servers).map((n) => n.slice(0, 64));
        const shown = redactForDisplay(parsed);
        maskedValues += shown.masked;
        normalizedBefore = pretty(shown.value);
        original = normalizedBefore;
        const rawNorm = pretty(parsed);
        const stripped = req.source.text.replace(/^\uFEFF/, "").trim();
        if (result.hadBom || stripped !== rawNorm.trim()) formattingNote = "Dosya biçimi (girinti, boşluk, varsa BOM) normalize edilerek gösteriliyor; anlamsal içerik korunur. Gerçek bir yazma olsaydı biçim değişirdi.";
      }
    }
  }
  if (maskedValues > 0) constraints.push(`${maskedValues} değer gizli olabileceği için maskelendi. Bu görüntü yeniden biçimlendirilmiş ve maskelenmiştir; bundan bir dosya üretilemez.`);

  // 6. the reviewed declaration: absent means no diff at all
  const reviewed = declarationFor(host, req.entry.entryId, declarations);
  if (reviewed) {
    const violations = validateDeclaration(host, reviewed);
    const record = (req.entry.record ?? {}) as { sourceSnapshot?: { immutableCommit?: unknown }; repository?: { url?: unknown } };
    const sourceMatches =
      record.sourceSnapshot?.immutableCommit === reviewed.review.reviewedAgainst &&
      (reviewed.review.repositoryUrl === undefined || record.repository?.url === reviewed.review.repositoryUrl);
    if (violations.length > 0) {
      problems.push({ code: "declaration_rejected", text: `Kayıt defterindeki bildirim ilke denetiminden geçemedi (${violations.join(", ")}); kullanılmadı.` });
    } else if (!sourceMatches) {
      problems.push({
        code: "source_mismatch",
        text: "Bildirimin gözden geçirildiği kaynak (depo ve sabit commit) bu katalog kaydının kendi sabitlenmiş kaynağıyla eşleşmiyor ya da katalog kaydında sabit commit yok; kaynak değişmiş olabilir. Bildirim kullanılmadı; yeniden gözden geçirilmesi gerekir.",
      });
    } else {
      usedDeclaration = reviewed;
      for (const w of reviewed.warnings ?? []) constraints.push(w);
    }
  }
  if (!usedDeclaration) {
    blockers.push({
      code: "no_reviewed_declaration",
      text: "Bu araç ve bu ajan dosyası için bağımsız olarak gözden geçirilmiş, sabit sürümlü ve kesin bir MCP bildirimi onaylanmadı. Yayıncının kurulum komutları, README örnekleri, “@latest” kullanan çağrılar veya anahtar içeren adresler şablon sayılmaz; makul görünen bir komut uydurulmaz.",
    });
  }

  // 7. compose the proposal (only with a usable declaration and a usable current text)
  const currentUsable = req.source.kind === "new" || (parsed !== null && checkServersShape(parsed).length === 0);
  let conflict = false;
  if (usedDeclaration && currentUsable) {
    const base: JsonObject = req.source.kind === "new" || parsed === null ? {} : (JSON.parse(JSON.stringify(parsed)) as JsonObject);
    const servers: JsonObject = isObject(base.mcpServers) ? base.mcpServers : {};
    const wanted = usedDeclaration.serverName.toLowerCase();
    if (Object.prototype.hasOwnProperty.call(servers, usedDeclaration.serverName)) {
      conflict = true;
      problems.push({ code: "server_name_exists", text: `“${usedDeclaration.serverName}” adlı bir sunucu zaten tanımlı; üzerine yazılmaz.` });
    } else if (Object.keys(servers).some((n) => n.toLowerCase() === wanted)) {
      conflict = true; // a name that differs only in letter case reads as the same server to a person (and to some clients)
      problems.push({ code: "server_name_exists", text: `Büyük/küçük harf farkı dışında “${usedDeclaration.serverName}” ile aynı adlı bir sunucu zaten tanımlı; karışıklığı önlemek için üzerine yazılmaz.` });
    }
    const signature = equivalentSignature(usedDeclaration.declaration as JsonObject, host.remoteUrlField);
    if (signature && Object.values(servers).some((s) => isObject(s) && equivalentSignature(s, host.remoteUrlField) === signature)) {
      conflict = true;
      problems.push({ code: "equivalent_exists", text: "Aynı komut/adres başka bir ad altında zaten tanımlı; yinelenen bildirim eklenmez." });
    }
    if (!conflict) {
      servers[usedDeclaration.serverName] = JSON.parse(JSON.stringify(usedDeclaration.declaration)) as JsonValue;
      base.mcpServers = servers;
      const shown = redactForDisplay(base);
      proposed = pretty(shown.value);
      diff = lineDiff(req.source.kind === "new" ? [] : lines(normalizedBefore), lines(proposed));
    }
  }

  // 8. one primary status
  const configBroken = problems.some((p) => !["server_name_exists", "equivalent_exists", "declaration_rejected", "source_mismatch"].includes(p.code));
  if (configBroken) return done("INVALID_CURRENT_CONFIG");
  if (!usedDeclaration) return done("BLOCKED_INSUFFICIENT_EVIDENCE");
  if (blockers.some((b) => b.code !== "no_reviewed_declaration")) return done("BLOCKED_BY_CATALOG", diff !== null);
  if (conflict) return done("CONFLICT");
  return done("PREVIEW_ONLY_NOT_APPLIED");
}
