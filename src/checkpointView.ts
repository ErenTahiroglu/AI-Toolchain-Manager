// ATM-GOV-01 - "Checkpoint Merkezi": a strict, bounded, read-only VIEWER model for checkpoint metadata the user pastes by hand.
//
// Pure. No IPC, no file, no network, no clock (the caller passes `now`), no storage, no dependency. It parses an explicitly versioned envelope of
// UNTRUSTED text and builds a view-model that carries NO verdict: every checkpoint has the constant trust label, and there is no `verified`,
// `accepted`, `passed` or aggregate member anywhere. A syntactically valid commit SHA, run id, digest or repository name is not proof that the object
// exists, belongs to the stated workflow or was reviewed by anyone; this module never consults anything that could say so. Whatever the pasted
// text claims (success, accepted, verified) stays a CLAIM. Contract and limits: docs/atm-gov-01/CONTRACT_AND_BOUNDARY.md.
//
// The viewer envelope is NOT the canonical manifest of the separate Evidence-Checkpoint-Engine; do not grow verifier logic here (EXTERNAL_ENGINE_HANDOFF.md).
import { parseRfc3339 } from "./review";

export const SCHEMA_VERSION = "atm.checkpoint-view.v1";
export const TRUST_LABEL = "KAYNAK BEYANI — DOĞRULANMADI";

export const LIMITS = {
  chars: 40_000,
  depth: 8,
  values: 3_000,
  checkpoints: 10,
  workflows: 5,
  jobs: 30,
  expectedJobs: 30,
  artifacts: 10,
  issues: 20,
  pullRequests: 20,
  limitations: 10,
  missingEvidence: 8,
  phase: 80,
  name: 80,
  ref: 100,
  limitation: 160,
  maxAttempt: 1000,
  maxIssueNumber: 999_999_999,
} as const;

export const CONCLUSIONS = ["success", "failure", "cancelled", "skipped", "neutral", "timed_out", "action_required", "stale", "unknown"] as const;
export type ClaimedConclusion = (typeof CONCLUSIONS)[number];
export const AVAILABILITY = ["retained", "expired", "unavailable", "missing", "not_checked"] as const;
export type AvailabilityClaim = (typeof AVAILABILITY)[number];
export const REVIEWER_ROLES = ["author", "independent_reviewer", "ci_system", "unknown"] as const;
export type ReviewerRole = (typeof REVIEWER_ROLES)[number];
export const ASSESSMENTS = ["accepted", "rejected", "changes_requested", "pending", "unknown"] as const;
export type Assessment = (typeof ASSESSMENTS)[number];
export const MISSING_EVIDENCE = [
  "ci_not_checked", "artifact_not_checked", "branch_head_not_checked", "reviewer_not_confirmed", "logs_not_read", "commit_not_checked_remotely", "other_unspecified",
] as const;
export type MissingEvidence = (typeof MISSING_EVIDENCE)[number];

export type ErrorCode =
  | "empty" | "too_large" | "too_deep" | "too_many_values" | "invalid_json" | "duplicate_key" | "bad_number"
  | "unknown_schema_version" | "unexpected_key" | "missing_key" | "bad_type" | "bad_value" | "unsafe_text" | "too_many_items"
  | "duplicate_record" | "inconsistent_identity";

/** `path` is built ONLY from schema key names and array indices - never from any character of the input. */
export type ParseResult = { ok: false; code: ErrorCode; path: string } | { ok: true; checkpoints: CheckpointView[] };

export type NoteCode =
  | "reviewer_other_commit" | "expected_job_missing" | "reported_job_not_expected" | "artifact_retained_but_expiry_passed"
  | "artifact_expired_but_expiry_future" | "artifact_no_digest" | "workflow_no_jobs";

export interface JobView { name: string; claimed: ClaimedConclusion }
export interface WorkflowView {
  name: string;
  runId: number;
  runAttempt: number;
  /** An ASSERTION about the triggering ref; nothing was checked. */
  ref: string | null;
  jobs: JobView[];
  expectedJobs: string[] | null;
  notes: NoteCode[];
}
export interface ArtifactView {
  id: number;
  name: string | null;
  sha256: string | null;
  expiresAt: string | null;
  availabilityClaim: AvailabilityClaim | null;
  /** An interpretation of the DECLARED timestamp against the clock given to the parser. Not a remote check. */
  expiry: "not_declared" | "declared_past" | "declared_future";
  notes: NoteCode[];
}
export interface ReviewerClaimView { role: ReviewerRole; assessment: Assessment; onSha: string | null }
export interface CheckpointView {
  trust: typeof TRUST_LABEL;
  repository: string;
  phase: string;
  sourceSha: string;
  workflows: WorkflowView[];
  artifacts: ArtifactView[];
  issues: number[];
  pullRequests: number[];
  reviewerClaim: ReviewerClaimView | null;
  limitations: string[];
  missingEvidence: MissingEvidence[];
  notes: NoteCode[];
}

// ---- 1. the raw-text scanner (before JSON.parse; tolerant, never throws) ---------------------------------------------------------------

const INTEGER_TOKEN = /^(0|[1-9][0-9]{0,15})$/;

/**
 * Counts nesting depth and values, rejects duplicate object keys (also when spelled with different escapes) and any number token that is not a plain
 * non-negative integer. It runs first so a hostile text (very deep, repeated keys, huge numbers) is refused before it is trusted to JSON.parse. On
 * structurally broken text it simply stops; JSON.parse then reports `invalid_json`.
 */
function scan(text: string): ErrorCode | null {
  type Frame = { object: boolean; keys: Set<string>; expectKey: boolean };
  const stack: Frame[] = [];
  let values = 0;
  let i = 0;
  const n = text.length;
  while (i < n) {
    const c = text[i];
    if (c === '"') {
      let j = i + 1;
      while (j < n && text[j] !== '"') j += text[j] === "\\" ? 2 : 1;
      if (j >= n) return null; // unterminated: JSON.parse will say so
      const top = stack[stack.length - 1];
      if (top && top.object && top.expectKey) {
        let key: string;
        try {
          key = JSON.parse(text.slice(i, j + 1)) as string;
        } catch {
          return null;
        }
        if (top.keys.has(key)) return "duplicate_key";
        top.keys.add(key);
        top.expectKey = false;
      } else {
        values += 1;
        if (values > LIMITS.values) return "too_many_values";
      }
      i = j + 1;
      continue;
    }
    if (c === "{" || c === "[") {
      values += 1;
      if (values > LIMITS.values) return "too_many_values";
      stack.push({ object: c === "{", keys: new Set(), expectKey: c === "{" });
      if (stack.length > LIMITS.depth) return "too_deep";
    } else if (c === "}" || c === "]") {
      stack.pop();
    } else if (c === ",") {
      const top = stack[stack.length - 1];
      if (top && top.object) top.expectKey = true;
    } else if (c === "-" || (c >= "0" && c <= "9")) {
      let j = i;
      while (j < n && /[0-9eE+.-]/.test(text[j])) j += 1;
      if (!INTEGER_TOKEN.test(text.slice(i, j))) return "bad_number";
      values += 1;
      if (values > LIMITS.values) return "too_many_values";
      i = j;
      continue;
    } else if (/[a-z]/.test(c)) {
      let j = i;
      while (j < n && /[a-z]/.test(text[j])) j += 1; // true / false / null
      values += 1;
      i = j;
      continue;
    }
    i += 1;
  }
  return null;
}

// ---- 2. syntax validators -----------------------------------------------------------------------------------------------------------------

class Reject extends Error {
  constructor(public code: ErrorCode, public path: string) {
    super(code);
  }
}
function fail(code: ErrorCode, path: string): never {
  throw new Reject(code, path);
}

const SHA1 = /^[0-9a-f]{40}$/;
const SHA256 = /^[0-9a-f]{64}$/;
const REPOSITORY = /^([A-Za-z0-9](?:-?[A-Za-z0-9]){0,38})\/([A-Za-z0-9._-]{1,100})$/;
const REF = /^[A-Za-z0-9][A-Za-z0-9._/-]{0,99}$/;
/** Letters of any script, digits, space and a small set of punctuation. No markup, no brackets, no slashes, no quotes, no `@ $ % & = ? ! * | ~ ^`. */
const INERT = /^[\p{L}\p{N} .,:;()#+_\-–—'’·]+$/u;

const SECRET_OR_ACTIVE: RegExp[] = [
  /:\/\//, /\bwww\./i, /javascript/i, /vbscript/i, /\bdata:/i,
  /\b[a-z][a-z0-9+.-]{1,9}:(?!\s|$)/i, // any scheme-like "word:text" (a colon followed by a space is fine)
  /\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{6,}/i, /github_pat_/i, /\bsk-[A-Za-z0-9]/i, /\bAKIA[0-9A-Z]{8,}/, /\bxox[abprs]-/i, /-{3,}\s*BEGIN/i,
  /[0-9a-fA-F]{32,}/, /[A-Za-z0-9_-]{40,}/,
  /\b(?:password|passwd|secret|token|api[ _-]?key|bearer|authorization|credential)s?\b/i,
];

const isPlainObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v) && Object.getPrototypeOf(v) === Object.prototype;

function exactKeys(o: Record<string, unknown>, required: readonly string[], optional: readonly string[], path: string): void {
  for (const k of Object.keys(o)) if (!required.includes(k) && !optional.includes(k)) fail("unexpected_key", path); // the key name is never echoed
  for (const k of required) if (!Object.prototype.hasOwnProperty.call(o, k)) fail("missing_key", `${path}.${k}`);
}

function inertText(v: unknown, max: number, path: string): string {
  if (typeof v !== "string") return fail("bad_type", path);
  if (v.length < 1 || v.length > max || v !== v.trim() || v !== v.normalize("NFC")) return fail("bad_value", path);
  if (!INERT.test(v)) return fail("unsafe_text", path);
  if (SECRET_OR_ACTIVE.some((r) => r.test(v))) return fail("unsafe_text", path);
  return v;
}

function oneOf<T extends string>(v: unknown, list: readonly T[], path: string): T {
  if (typeof v !== "string") return fail("bad_type", path);
  return (list as readonly string[]).includes(v) ? (v as T) : fail("bad_value", path);
}

function integer(v: unknown, min: number, max: number, path: string): number {
  if (typeof v !== "number") return fail("bad_type", path);
  return Number.isSafeInteger(v) && v >= min && v <= max ? v : fail("bad_value", path);
}

function array(v: unknown, max: number, path: string): unknown[] {
  if (!Array.isArray(v)) return fail("bad_type", path);
  return v.length <= max ? v : fail("too_many_items", path);
}

function sha(v: unknown, re: RegExp, path: string): string {
  if (typeof v !== "string") return fail("bad_type", path);
  return re.test(v) ? v : fail("bad_value", path);
}

function uniqueNumbers(list: unknown[], max: number, path: string): number[] {
  const out = list.map((x, i) => integer(x, 1, max, `${path}[${i}]`));
  if (new Set(out).size !== out.length) fail("duplicate_record", path);
  return out;
}

// ---- 3. the closed notes and the words ---------------------------------------------------------------------------------------------------------

export const NOTE_TEXT: Record<NoteCode, string> = {
  reviewer_other_commit: "Bildirilen değerlendirme, bu kaydın commit'inden farklı bir commit için yazılmış; bu kayıt için geçerli sayılmaz.",
  expected_job_missing: "Beklenen bir iş, bildirilen işler arasında yok. Bu, işin başarısız olduğunu ya da hiç çalışmadığını göstermez; yalnızca bildirimde bulunmadığını gösterir.",
  reported_job_not_expected: "Bildirilen bir iş, beklenen iş listesinde yok.",
  artifact_retained_but_expiry_passed: "Çıktı “saklanıyor” olarak bildirilmiş, ancak bildirilen son kullanma zamanı bu cihazın saatine göre geçmiş görünüyor. Hiçbiri denetlenmedi.",
  artifact_expired_but_expiry_future: "Çıktı “süresi dolmuş” olarak bildirilmiş, ancak bildirilen son kullanma zamanı bu cihazın saatine göre henüz gelmemiş görünüyor. Hiçbiri denetlenmedi.",
  artifact_no_digest: "Çıktı için SHA-256 özeti bildirilmemiş.",
  workflow_no_jobs: "Bu iş akışı için hiç iş bildirilmemiş.",
};

export const CONCLUSION_LABEL: Record<ClaimedConclusion, string> = {
  success: "başarılı", failure: "başarısız", cancelled: "iptal edildi", skipped: "atlandı", neutral: "tarafsız", timed_out: "zaman aşımı",
  action_required: "eylem gerekli", stale: "eskimiş", unknown: "bilinmiyor",
};
export const AVAILABILITY_LABEL: Record<AvailabilityClaim, string> = {
  retained: "saklanıyor", expired: "süresi dolmuş", unavailable: "kullanılamıyor", missing: "yok", not_checked: "denetlenmedi",
};
export const ROLE_LABEL: Record<ReviewerRole, string> = {
  author: "yazar", independent_reviewer: "bağımsız inceleyici olduğunu söyleyen", ci_system: "CI sistemi", unknown: "belirtilmemiş",
};
export const ASSESSMENT_LABEL: Record<Assessment, string> = {
  accepted: "kabul", rejected: "ret", changes_requested: "değişiklik istendi", pending: "bekliyor", unknown: "bilinmiyor",
};
export const MISSING_LABEL: Record<MissingEvidence, string> = {
  ci_not_checked: "CI sonucu denetlenmedi",
  artifact_not_checked: "Çıktı denetlenmedi",
  branch_head_not_checked: "Dal ucunun (HEAD) bu commit olup olmadığı denetlenmedi",
  reviewer_not_confirmed: "İnceleyici beyanı doğrulanamadı",
  logs_not_read: "Günlükler okunmadı",
  commit_not_checked_remotely: "Commit'in varlığı uzaktan denetlenmedi",
  other_unspecified: "Belirtilmemiş başka eksik kanıt",
};

export const ERROR_TEXT: Record<ErrorCode, string> = {
  empty: "Henüz bir kayıt yapıştırılmadı.",
  too_large: "Metin izin verilen boyutu aşıyor.",
  too_deep: "Metnin iç içe yapısı izin verilen derinliği aşıyor.",
  too_many_values: "Metin izin verilen değer sayısını aşıyor.",
  invalid_json: "Metin geçerli bir JSON değil.",
  duplicate_key: "Aynı alan adı bir nesnede birden fazla kez geçiyor.",
  bad_number: "Bir sayı yalnızca işaretsiz, tam ve en fazla 16 basamaklı olabilir.",
  unknown_schema_version: "Şema sürümü bilinmiyor ya da yok.",
  unexpected_key: "Beklenmeyen bir alan var.",
  missing_key: "Zorunlu bir alan eksik.",
  bad_type: "Bir alanın türü beklenenden farklı.",
  bad_value: "Bir alanın değeri biçime ya da sınırlara uymuyor.",
  unsafe_text: "Bir metin alanı güvenli düz metin kurallarına uymuyor (işaretleme, adres, gizli bilgi benzeri içerik ya da izin verilmeyen karakter).",
  too_many_items: "Bir liste izin verilen öğe sayısını aşıyor.",
  duplicate_record: "Yinelenen bir kayıt ya da kimlik var.",
  inconsistent_identity: "Kayıtlar arasında tutarsız bir depo/commit/çalıştırma kimliği var.",
};

export const SCOPE_SENTENCE =
  "Bu kayıt yalnızca bildirilen commit için bir beyandır; bu uygulama o commit'in güncel dal ucu (HEAD) olup olmadığını denetlemez.";
export const NOT_CHECKED_SENTENCE =
  "Hiçbir şey uzaktan denetlenmedi: ne commit'in, ne çalıştırmanın, ne işlerin, ne çıktının var olduğu, ne de değerlendirmenin gerçek olduğu bilinir.";

// ---- 4. the parser ------------------------------------------------------------------------------------------------------------------------

function parseJob(v: unknown, path: string): JobView {
  if (!isPlainObject(v)) return fail("bad_type", path);
  exactKeys(v, ["name", "claimedConclusion"], [], path);
  return { name: inertText(v.name, LIMITS.name, `${path}.name`), claimed: oneOf(v.claimedConclusion, CONCLUSIONS, `${path}.claimedConclusion`) };
}

function parseWorkflow(v: unknown, path: string): WorkflowView {
  if (!isPlainObject(v)) return fail("bad_type", path);
  exactKeys(v, ["name", "runId", "runAttempt", "jobs"], ["ref", "expectedJobs"], path);
  const name = inertText(v.name, LIMITS.name, `${path}.name`);
  const runId = integer(v.runId, 1, Number.MAX_SAFE_INTEGER, `${path}.runId`);
  const runAttempt = integer(v.runAttempt, 1, LIMITS.maxAttempt, `${path}.runAttempt`);
  let ref: string | null = null;
  if (v.ref !== undefined) {
    if (typeof v.ref !== "string") fail("bad_type", `${path}.ref`);
    const r = v.ref as string;
    if (!REF.test(r) || r.includes("//") || r.includes("..")) fail("bad_value", `${path}.ref`);
    if (SECRET_OR_ACTIVE.some((re) => re.test(r))) fail("unsafe_text", `${path}.ref`);
    ref = r;
  }
  const jobs = array(v.jobs, LIMITS.jobs, `${path}.jobs`).map((j, i) => parseJob(j, `${path}.jobs[${i}]`));
  if (new Set(jobs.map((j) => j.name)).size !== jobs.length) fail("duplicate_record", `${path}.jobs`);
  let expectedJobs: string[] | null = null;
  if (v.expectedJobs !== undefined) {
    expectedJobs = array(v.expectedJobs, LIMITS.expectedJobs, `${path}.expectedJobs`).map((x, i) => inertText(x, LIMITS.name, `${path}.expectedJobs[${i}]`));
    if (new Set(expectedJobs).size !== expectedJobs.length) fail("duplicate_record", `${path}.expectedJobs`);
  }
  const notes: NoteCode[] = [];
  if (jobs.length === 0) notes.push("workflow_no_jobs");
  if (expectedJobs) {
    const reported = new Set(jobs.map((j) => j.name));
    if (expectedJobs.some((e) => !reported.has(e))) notes.push("expected_job_missing");
    if (jobs.some((j) => !expectedJobs!.includes(j.name))) notes.push("reported_job_not_expected");
  }
  return { name, runId, runAttempt, ref, jobs, expectedJobs, notes };
}

function parseArtifact(v: unknown, path: string, now: number): ArtifactView {
  if (!isPlainObject(v)) return fail("bad_type", path);
  exactKeys(v, ["id"], ["name", "sha256", "expiresAt", "availabilityClaim"], path);
  const id = integer(v.id, 1, Number.MAX_SAFE_INTEGER, `${path}.id`);
  const name = v.name === undefined ? null : inertText(v.name, LIMITS.name, `${path}.name`);
  const sha256 = v.sha256 === undefined ? null : sha(v.sha256, SHA256, `${path}.sha256`);
  let expiresAt: string | null = null;
  let stamp: number | null = null;
  if (v.expiresAt !== undefined) {
    if (typeof v.expiresAt !== "string") fail("bad_type", `${path}.expiresAt`);
    stamp = parseRfc3339(v.expiresAt as string);
    if (stamp === null) fail("bad_value", `${path}.expiresAt`);
    expiresAt = v.expiresAt as string;
  }
  const availabilityClaim = v.availabilityClaim === undefined ? null : oneOf(v.availabilityClaim, AVAILABILITY, `${path}.availabilityClaim`);
  const expiry = stamp === null ? "not_declared" : stamp <= now ? "declared_past" : "declared_future";
  const notes: NoteCode[] = [];
  if (sha256 === null) notes.push("artifact_no_digest");
  if (availabilityClaim === "retained" && expiry === "declared_past") notes.push("artifact_retained_but_expiry_passed");
  if (availabilityClaim === "expired" && expiry === "declared_future") notes.push("artifact_expired_but_expiry_future");
  return { id, name, sha256, expiresAt, availabilityClaim, expiry, notes };
}

function parseCheckpoint(v: unknown, path: string, now: number): CheckpointView {
  if (!isPlainObject(v)) return fail("bad_type", path);
  exactKeys(v, ["repository", "phase", "sourceSha", "workflows"], ["artifacts", "issues", "pullRequests", "reviewerClaim", "limitations", "missingEvidence"], path);
  if (typeof v.repository !== "string") fail("bad_type", `${path}.repository`);
  const repository = v.repository as string;
  const m = REPOSITORY.exec(repository);
  if (!m || m[2] === "." || m[2] === "..") fail("bad_value", `${path}.repository`);
  const phase = inertText(v.phase, LIMITS.phase, `${path}.phase`);
  const sourceSha = sha(v.sourceSha, SHA1, `${path}.sourceSha`);
  const wfWire = array(v.workflows, LIMITS.workflows, `${path}.workflows`);
  if (wfWire.length === 0) fail("bad_value", `${path}.workflows`);
  const workflows = wfWire.map((w, i) => parseWorkflow(w, `${path}.workflows[${i}]`));
  const seenRuns = new Set<string>();
  for (const w of workflows) {
    const k = `${w.runId}/${w.runAttempt}`;
    if (seenRuns.has(k)) fail("duplicate_record", `${path}.workflows`);
    seenRuns.add(k);
  }
  const artifacts = v.artifacts === undefined ? [] : array(v.artifacts, LIMITS.artifacts, `${path}.artifacts`).map((a, i) => parseArtifact(a, `${path}.artifacts[${i}]`, now));
  if (new Set(artifacts.map((a) => a.id)).size !== artifacts.length) fail("duplicate_record", `${path}.artifacts`);
  const issues = v.issues === undefined ? [] : uniqueNumbers(array(v.issues, LIMITS.issues, `${path}.issues`), LIMITS.maxIssueNumber, `${path}.issues`);
  const pullRequests = v.pullRequests === undefined ? [] : uniqueNumbers(array(v.pullRequests, LIMITS.pullRequests, `${path}.pullRequests`), LIMITS.maxIssueNumber, `${path}.pullRequests`);
  let reviewerClaim: ReviewerClaimView | null = null;
  if (v.reviewerClaim !== undefined) {
    const r = v.reviewerClaim;
    const rp = `${path}.reviewerClaim`;
    if (!isPlainObject(r)) fail("bad_type", rp);
    const ro = r as Record<string, unknown>;
    exactKeys(ro, ["role", "assessment"], ["onSha"], rp);
    reviewerClaim = {
      role: oneOf(ro.role, REVIEWER_ROLES, `${rp}.role`),
      assessment: oneOf(ro.assessment, ASSESSMENTS, `${rp}.assessment`),
      onSha: ro.onSha === undefined ? null : sha(ro.onSha, SHA1, `${rp}.onSha`),
    };
  }
  const limitations = v.limitations === undefined ? [] : array(v.limitations, LIMITS.limitations, `${path}.limitations`).map((x, i) => inertText(x, LIMITS.limitation, `${path}.limitations[${i}]`));
  const missingEvidence = v.missingEvidence === undefined ? [] : array(v.missingEvidence, LIMITS.missingEvidence, `${path}.missingEvidence`).map((x, i) => oneOf(x, MISSING_EVIDENCE, `${path}.missingEvidence[${i}]`));
  if (new Set(missingEvidence).size !== missingEvidence.length) fail("duplicate_record", `${path}.missingEvidence`);
  const notes: NoteCode[] = [];
  if (reviewerClaim && reviewerClaim.onSha !== null && reviewerClaim.onSha !== sourceSha) notes.push("reviewer_other_commit");
  return { trust: TRUST_LABEL, repository, phase, sourceSha, workflows, artifacts, issues, pullRequests, reviewerClaim, limitations, missingEvidence, notes };
}

/**
 * Parses pasted text. `now` (ms since the epoch) is the ONLY clock: it is used solely to interpret DECLARED artifact expiry timestamps. The result never
 * contains a verdict. Errors carry a closed code and a schema path, never a character of the input.
 */
export function parseCheckpointInput(text: unknown, now: number): ParseResult {
  if (typeof text !== "string") return { ok: false, code: "bad_type", path: "$" };
  if (text.length > LIMITS.chars) return { ok: false, code: "too_large", path: "$" };
  const trimmed = text.trim();
  if (trimmed === "") return { ok: false, code: "empty", path: "$" };
  const scanned = scan(trimmed);
  if (scanned !== null) return { ok: false, code: scanned, path: "$" };
  let root: unknown;
  try {
    root = JSON.parse(trimmed);
  } catch {
    return { ok: false, code: "invalid_json", path: "$" };
  }
  try {
    if (!isPlainObject(root)) return fail("bad_type", "$");
    exactKeys(root, ["schemaVersion", "checkpoints"], [], "$");
    if (root.schemaVersion !== SCHEMA_VERSION) return fail("unknown_schema_version", "$.schemaVersion");
    const list = array(root.checkpoints, LIMITS.checkpoints, "$.checkpoints");
    if (list.length === 0) return fail("bad_value", "$.checkpoints");
    const checkpoints = list.map((c, i) => parseCheckpoint(c, `$.checkpoints[${i}]`, now));
    const seen = new Set<string>();
    const runOwner = new Map<number, string>();
    const runAttempts = new Set<string>();
    for (const [i, c] of checkpoints.entries()) {
      const key = `${c.repository.toLowerCase()}\u0000${c.sourceSha}\u0000${c.phase}`;
      if (seen.has(key)) fail("duplicate_record", `$.checkpoints[${i}]`);
      seen.add(key);
      const owner = `${c.repository.toLowerCase()}\u0000${c.sourceSha}`;
      for (const w of c.workflows) {
        const prior = runOwner.get(w.runId);
        if (prior !== undefined && prior !== owner) fail("inconsistent_identity", `$.checkpoints[${i}].workflows`);
        runOwner.set(w.runId, owner);
        const ra = `${w.runId}/${w.runAttempt}`;
        if (runAttempts.has(ra)) fail("duplicate_record", `$.checkpoints[${i}].workflows`);
        runAttempts.add(ra);
      }
    }
    return { ok: true, checkpoints };
  } catch (e) {
    if (e instanceof Reject) return { ok: false, code: e.code, path: e.path };
    return { ok: false, code: "invalid_json", path: "$" }; // an unexpected failure is refused, never partially shown
  }
}
