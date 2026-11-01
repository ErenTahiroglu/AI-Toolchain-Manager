// ATM-GOV-02 P2B - the pure model behind "GitHub CI Kanıtını Denetle (ECE)": input validation and the CLOSED mapping from the native report to
// words. Pure: no IPC, no network, no clock, no storage. Whatever the native side returns is shown ONLY if every member is a documented value;
// one unknown value makes the whole result "unrecognized" and nothing in it is shown as a pass.
//
// Boundary (also stated on the page): this is a MACHINE observation of one historical CI run, judged by the independent ECE verifier. It is not a
// checkpoint acceptance, not an independent review and not a human authorization, and it says nothing about the current branch head.
import type { AssessAtmCheckpointRequest, CiAssessmentReport } from "./types";

export const REPORT_SCHEMA_VERSION = "atm.ece-ci-assessment.v1";
export const TARGET_REPOSITORY = "ErenTahiroglu/AI-Toolchain-Manager";
export const TARGET_REPOSITORY_ID = 1413039335;
export const TARGET_WORKFLOW = ".github/workflows/ci.yml";
export const REQUIRED_JOBS = ["windows-x64", "macos-arm64"] as const;
export const MAX_RUN_ATTEMPT = 1000;

export const LABEL_MACHINE = "MAKİNE CI KANITI — ECE DEĞERLENDİRMESİ";
export const LABEL_REVIEW = "BAĞIMSIZ İNCELEME: YAPILMADI";
export const LABEL_AUTHORIZATION = "İNSAN ONAYI: BEKLEMEDE";
export const LABEL_HISTORICAL = "TARİHSEL KANIT — GÜNCEL HEAD ONAYI DEĞİLDİR";
export const UNRECOGNIZED = "tanınmayan değer";

// ---- input ------------------------------------------------------------------------------------------------------------------------------

export type InputField = "commitSha" | "runId" | "runAttempt";
export type InputResult = { ok: true; request: AssessAtmCheckpointRequest } | { ok: false; field: InputField };

/** Strict, syntax only. The native side validates again; nothing here is proof that the commit or the run exists. */
export function parseInput(commitSha: string, runId: string, runAttempt: string): InputResult {
  if (!/^[0-9a-f]{40}$/.test(commitSha)) return { ok: false, field: "commitSha" };
  if (!/^[1-9][0-9]{0,15}$/.test(runId) || !Number.isSafeInteger(Number(runId))) return { ok: false, field: "runId" };
  if (!/^[1-9][0-9]{0,3}$/.test(runAttempt) || Number(runAttempt) > MAX_RUN_ATTEMPT) return { ok: false, field: "runAttempt" };
  return { ok: true, request: { commitSha, runId: Number(runId), runAttempt: Number(runAttempt) } };
}

export const INPUT_ERROR: Record<InputField, string> = {
  commitSha: "Tam, küçük harfli, 40 karakterlik bir commit SHA girin (kısa SHA, dal adı ya da büyük harf kabul edilmez).",
  runId: "Çalıştırma kimliği pozitif bir tam sayı olmalı.",
  runAttempt: `Deneme numarası 1 ile ${MAX_RUN_ATTEMPT} arasında bir tam sayı olmalı.`,
};

// ---- the closed vocabulary ------------------------------------------------------------------------------------------------------------

export const OUTCOME_LABEL: Record<string, string> = {
  ASSESSED_LIMITATIONS: "Sınırlı değerlendirme: makine kanıtı vardır; bu bir kabul değildir",
  ASSESSED_FAIL: "Değerlendirme: engelleyici neden var (gözlenen kanıt hedefle uyuşmuyor ya da CI geçmedi)",
  INSUFFICIENT_EVIDENCE: "Yetersiz kanıt: GitHub'dan değerlendirmek için yeterli veri alınamadı",
  NOT_ASSESSED: "Değerlendirilmedi",
};
export const CI_LABEL: Record<string, string> = {
  VERIFIED_PASS: "İki zorunlu iş de bu denemede başarılı sonuçlandı (makine gözlemi)",
  FAILED: "Zorunlu iş(ler) başarısız ya da eksik",
  INCOMPLETE_SKIPPED: "Zorunlu iş(ler) atlandı ya da iptal edildi",
  ATTEMPT_MISMATCH: "İstenen deneme, çalıştırmadaki denemelerle uyuşmuyor",
  UNVERIFIED_RUN: "Çalıştırma tamamlanmadı ya da okunamadı",
  NOT_EVALUATED: "Değerlendirilmedi",
};
export const AVAILABILITY_LABEL: Record<string, string> = {
  NOT_PROVIDED: "Çıktı (artifact) istenmedi; bu denetim çıktıya bakmaz",
  AVAILABLE: "Çıktı mevcut",
  PARTIALLY_AVAILABLE: "Çıktı kısmen mevcut",
  EXPIRED: "Çıktının süresi dolmuş",
  UNREACHABLE: "Çıktı bilgisine ulaşılamadı",
  NOT_EVALUATED: "Değerlendirilmedi",
};
export const REVIEW_LABEL: Record<string, string> = {
  UNREVIEWED: LABEL_REVIEW,
  NOT_EVALUATED: `${LABEL_REVIEW} (bu denetimde incelemeye bakılmadı)`,
};
export const AUTHORIZATION_LABEL: Record<string, string> = { AUTHORIZATION_PENDING: LABEL_AUTHORIZATION };
export const FRESHNESS_LABEL: Record<string, string> = {
  HISTORICAL_BASELINE: LABEL_HISTORICAL,
  NOT_EVALUATED: `Güncellik değerlendirilmedi — ${LABEL_HISTORICAL}`,
};
export const LIMITATION_LABEL: Record<string, string> = {
  NO_INDEPENDENT_REVIEW: "Bağımsız bir insan incelemesi doğrulanmadı.",
  NO_HUMAN_AUTHORIZATION: "İnsan onayı doğrulanmadı.",
  NOT_CURRENT_HEAD_ATTESTATION: "Bu, geçmiş bir çalıştırmadır; dalın şimdiki ucunu (HEAD) onaylamaz.",
  STANDALONE_RUN_ASSESSMENT: "Yapıştırılmış bir zarf ya da özet değerlendirilmedi; yalnızca GitHub çalıştırması değerlendirildi.",
  COMMIT_SIGNATURE_NOT_VERIFIED: "Commit imzası doğrulanmadı.",
  HOSTED_RUNNERS_ONLY: "Yalnızca GitHub'ın barındırdığı çalıştırıcılar; fiziksel cihaz testi yok.",
};
export const BLOCKING_LABEL: Record<string, string> = {
  REPOSITORY_IDENTITY_MISMATCH: "Depo kimliği uyuşmuyor",
  COMMIT_NOT_VERIFIED: "Commit bu depoda bu çalıştırmaya bağlanamadı",
  WORKFLOW_MISMATCH: "İş akışı dosyası beklenenle uyuşmuyor",
  RUN_ATTEMPT_MISMATCH: "Deneme numarası uyuşmuyor",
  REQUIRED_JOBS_FAILED_OR_MISSING: "Zorunlu iş başarısız, eksik ya da belirsiz (tekrarlı)",
  REQUIRED_JOBS_SKIPPED_OR_CANCELLED: "Zorunlu iş atlandı ya da iptal edildi",
  RUN_OR_JOBS_NOT_COMPLETED: "Çalıştırma ya da iş tamamlanmadı",
  ARTIFACT_EXPIRED: "Çıktının süresi dolmuş",
  UNEXPECTED_JOB_SET: "Çalıştırma, ilkede izin verilmeyen ek iş içeriyor",
  UNCLASSIFIED: "Sınıflandırılamayan engelleyici neden",
};
export const ERROR_LABEL: Record<string, string> = {
  POLICY_INTEGRITY: "Operatör ilkesi beklenen özetle eşleşmedi; hiçbir şey değerlendirilmedi",
  SCHEMA_UNAVAILABLE: "Değerlendirici şeması yüklenemedi",
  OUTPUT_SCHEMA_VIOLATION: "Değerlendirme çıktısı şemaya uymadı; sonuç üretilmedi",
  EVALUATOR_FAILURE: "Değerlendirici beklenmedik biçimde hata verdi",
  PROVIDER_FAILURE: "GitHub'dan kanıt alınamadı. Bu, bir CI başarısızlığı DEĞİLDİR",
  UNSUPPORTED_SCHEMA_VERSION: "Desteklenmeyen değerlendirme sürümü",
  MALFORMED_ASSESSMENT: "Değerlendirme beklenen kapalı biçimde değil; sonuç sayılmadı",
  BINDING_MISMATCH: "Değerlendirme, istenen hedefle uyuşmuyor",
  ATTEMPT_NOT_LATEST: "İstenen deneme, çalıştırmanın en son denemesi değil. Bu sürüm yalnızca en son denemeyi değerlendirir; eski deneme ile yeni denemenin verileri karıştırılmaz",
  PROVIDER_IDENTITY_INCONSISTENT: "GitHub'ın bildirdiği ad/sahip sabit hedefle uyuşmuyor; sonuç sayılmadı",
  CLIENT_INIT_FAILURE: "Ağ istemcisi oluşturulamadı; hiçbir istek gönderilmedi",
};
export const PROVIDER_FAILURE_LABEL: Record<string, string> = {
  RATE_LIMITED: "GitHub istek sınırı aşıldı (anonim erişim saatte yaklaşık 60 istekle sınırlıdır); bir süre sonra yeniden deneyin",
  NETWORK_UNAVAILABLE: "Ağa ulaşılamadı (çevrimdışı olabilirsiniz ya da bir ara sunucu/güvenlik duvarı engelliyor olabilir)",
  TIMEOUT: "GitHub zamanında yanıt vermedi",
  FORBIDDEN: "GitHub erişimi reddetti",
  NOT_FOUND: "GitHub'da bulunamadı (depo, çalıştırma ya da iş)",
  HTTP_ERROR: "GitHub beklenmedik bir HTTP durumu döndürdü",
  INVALID_RESPONSE: "GitHub yanıtı okunamadı ya da boştu",
  RESPONSE_TOO_LARGE: "GitHub yanıtı izin verilen boyutu aştı",
  INCOMPLETE_RESPONSE: "GitHub yanıtı eksik (sayfalama tamamlanamadı)",
};

/** What the page may show. `recognized: false` means: show only the unrecognized notice. */
export type AssessmentView =
  | { recognized: false }
  | {
      recognized: true;
      outcome: string;
      ci: string;
      availability: string;
      review: string;
      authorization: string;
      freshness: string;
      limitations: string[];
      blocking: string[];
      error: string | null;
      providerFailure: string | null;
      /** true only when the native side reports a completed assessment of the run (no error class). */
      assessed: boolean;
      provenance: CiAssessmentReport["provenance"];
    };

const has = (map: Record<string, string>, v: unknown): v is string => typeof v === "string" && Object.prototype.hasOwnProperty.call(map, v);
const SHA = /^[0-9a-f]{40}$/;
const HEX64 = /^[0-9a-f]{64}$/;

/**
 * Maps the native report to a view, or says it is unrecognized. Every enum member must be documented; the numbers and strings of the provenance
 * must have the exact fixed target values and a well-formed commit; contradictions ("limitations" without a verified CI, an error with a
 * verified CI) are unrecognized too. Unknown never degrades to a pass.
 */
export function describeReport(report: unknown, asked: AssessAtmCheckpointRequest): AssessmentView {
  const bad: AssessmentView = { recognized: false };
  if (!report || typeof report !== "object") return bad;
  const r = report as Record<string, unknown>;
  if (r.schemaVersion !== REPORT_SCHEMA_VERSION) return bad;
  const p = r.provenance as Record<string, unknown> | undefined;
  if (!p || typeof p !== "object") return bad;
  const provenanceOk =
    p.repository === TARGET_REPOSITORY && p.repositoryId === TARGET_REPOSITORY_ID && p.workflowPath === TARGET_WORKFLOW &&
    typeof p.eceRevision === "string" && SHA.test(p.eceRevision) && typeof p.policyId === "string" && typeof p.policyFileSha256 === "string" &&
    HEX64.test(p.policyFileSha256) && typeof p.commitSha === "string" && SHA.test(p.commitSha) && Number.isSafeInteger(p.runId) &&
    Number.isSafeInteger(p.runAttempt);
  if (!provenanceOk) return bad;
  // The answer must be about what was asked, not about something else.
  if (p.commitSha !== asked.commitSha || p.runId !== asked.runId || p.runAttempt !== asked.runAttempt) return bad;
  if (!has(OUTCOME_LABEL, r.outcome) || !has(CI_LABEL, r.ci) || !has(AVAILABILITY_LABEL, r.availability) || !has(REVIEW_LABEL, r.review)) return bad;
  if (!has(AUTHORIZATION_LABEL, r.authorization) || !has(FRESHNESS_LABEL, r.freshness)) return bad;
  if (!Array.isArray(r.limitations) || !r.limitations.every((l) => has(LIMITATION_LABEL, l))) return bad;
  if (!Array.isArray(r.blocking) || !r.blocking.every((b) => has(BLOCKING_LABEL, b))) return bad;
  const error = r.error === null ? null : has(ERROR_LABEL, r.error) ? (r.error as string) : undefined;
  const providerFailure = r.providerFailure === null ? null : has(PROVIDER_FAILURE_LABEL, r.providerFailure) ? (r.providerFailure as string) : undefined;
  if (error === undefined || providerFailure === undefined) return bad;
  if (providerFailure !== null && error !== "PROVIDER_FAILURE") return bad;
  // Contradictions are never shown: a limited-but-assessed outcome needs a verified CI, no blocking reason and no error.
  if (r.outcome === "ASSESSED_LIMITATIONS" && (r.ci !== "VERIFIED_PASS" || r.blocking.length > 0 || error !== null)) return bad;
  if (error !== null && (r.outcome === "ASSESSED_LIMITATIONS" || r.outcome === "ASSESSED_FAIL")) return bad;
  return {
    recognized: true,
    outcome: r.outcome as string,
    ci: r.ci as string,
    availability: r.availability as string,
    review: r.review as string,
    authorization: r.authorization as string,
    freshness: r.freshness as string,
    limitations: r.limitations as string[],
    blocking: r.blocking as string[],
    error,
    providerFailure,
    assessed: error === null,
    provenance: p as unknown as CiAssessmentReport["provenance"],
  };
}
