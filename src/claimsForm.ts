// ATM-GOV-02 P2C-WP2 - the pure syntax check of the SESSION-ONLY claim form (nothing here is stored, sent or looked up). It turns what the user typed into
// `UserClaims`, or into one plain-language message per field. Syntax only: a value that passes is still only a claim.
import { TARGET_REPOSITORY } from "./ciAssessment";
import type { AuthorizationClaim, ReviewClaim, UserClaims } from "./projectCheckpoint";

export interface ClaimsFormInput {
  repository: string;
  acceptedSha: string;
  claimedHeadSha: string;
  reviewClaim: boolean;
  authorizationClaim: boolean;
  /** One blocker per line: `metin` or `metin #123`. */
  blockers: string;
}
export type ClaimsFormField = "repository" | "acceptedSha" | "claimedHeadSha" | "blockers";
export type ClaimsFormResult = { ok: true; claims: UserClaims } | { ok: false; errors: Partial<Record<ClaimsFormField, string>> };

export const CLAIM_LIMITS = { maxBlockers: 10, maxBlockerText: 160, maxIssueNumber: 999_999_999 } as const;

const REPO = /^[A-Za-z0-9._-]{1,39}\/[A-Za-z0-9._-]{1,100}$/;
const SHA = /^[0-9a-f]{40}$/;
// Control characters and line/paragraph separators (spelled with escapes in a string, so the source has no raw control characters).
const CONTROL = new RegExp("[" + "\\u0000-\\u001f\\u007f-\\u009f\\u2028\\u2029" + "]");

export const EMPTY_FORM: ClaimsFormInput = { repository: "", acceptedSha: "", claimedHeadSha: "", reviewClaim: false, authorizationClaim: false, blockers: "" };

export const isAtmRepository = (repository: string | null): boolean => repository !== null && repository.toLowerCase() === TARGET_REPOSITORY.toLowerCase();

/** The form shown for already entered claims. */
export const formFromClaims = (c: UserClaims | null): ClaimsFormInput =>
  c === null ? EMPTY_FORM : {
    repository: c.repository ?? "", acceptedSha: c.acceptedSha ?? "", claimedHeadSha: c.claimedHeadSha ?? "",
    reviewClaim: c.reviewClaim === "CLAIMED_INDEPENDENT", authorizationClaim: c.authorizationClaim === "CLAIMED_GRANTED",
    blockers: c.blockers.map((b) => (b.issue === null ? b.text : `${b.text} #${b.issue}`)).join("\n"),
  };

export function parseClaimsForm(input: ClaimsFormInput): ClaimsFormResult {
  const errors: Partial<Record<ClaimsFormField, string>> = {};
  // Nothing is silently corrected: the value is checked exactly as typed (only an entirely empty field means "no claim").
  const repository = input.repository === "" ? null : input.repository;
  if (repository !== null) {
    const [owner, name] = repository.split("/");
    if (!REPO.test(repository) || owner === "." || owner === ".." || name === "." || name === "..") {
      errors.repository = "Depo “sahip/ad” biçiminde olmalı: harf, rakam, nokta, tire ve alt çizgi; boşluk, ek eğik çizgi ve “.”/“..” parçaları kabul edilmez.";
    }
  }
  const sha = (value: string, field: "acceptedSha" | "claimedHeadSha"): string | null => {
    if (value === "") return null;
    if (!SHA.test(value)) errors[field] = "Tam, küçük harfli, 40 karakterlik bir commit SHA olmalı (kısa SHA, büyük harf ya da boşluk kabul edilmez).";
    return value;
  };
  const acceptedSha = sha(input.acceptedSha, "acceptedSha");
  const claimedHeadSha = sha(input.claimedHeadSha, "claimedHeadSha");

  const blockers: UserClaims["blockers"] = [];
  const lines = input.blockers === "" ? [] : input.blockers.split("\n");
  if (lines.length > CLAIM_LIMITS.maxBlockers) errors.blockers = `En çok ${CLAIM_LIMITS.maxBlockers} engel girilebilir.`;
  else {
    for (const [i, raw] of lines.entries()) {
      const m = /^(.*?)(?: #([0-9]+))?$/.exec(raw)!;
      const text = m[1];
      const issue = m[2] === undefined ? null : Number(m[2]);
      if (text.trim() === "" || CONTROL.test(raw) || text.length > CLAIM_LIMITS.maxBlockerText) {
        errors.blockers = `${i + 1}. satır: metin boş olamaz, en çok ${CLAIM_LIMITS.maxBlockerText} karakter olmalı ve denetim karakteri içermemeli.`;
        break;
      }
      if (issue !== null && (!Number.isSafeInteger(issue) || issue < 1 || issue > CLAIM_LIMITS.maxIssueNumber)) {
        errors.blockers = `${i + 1}. satır: Issue numarası 1 ile ${CLAIM_LIMITS.maxIssueNumber} arasında bir tam sayı olmalı. Numara yalnız bir metindir; durumu sorgulanmaz.`;
        break;
      }
      blockers.push({ text, issue });
    }
  }
  if (Object.keys(errors).length > 0) return { ok: false, errors };
  const reviewClaim: ReviewClaim = input.reviewClaim ? "CLAIMED_INDEPENDENT" : "NONE";
  const authorizationClaim: AuthorizationClaim = input.authorizationClaim ? "CLAIMED_GRANTED" : "NONE";
  return { ok: true, claims: { repository, acceptedSha, acceptedLabel: null, claimedHeadSha, reviewClaim, authorizationClaim, blockers, updatedAt: null } };
}
