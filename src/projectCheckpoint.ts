// ATM-GOV-02 P2C-WP1 - the pure model behind the project checkpoint overview (docs/atm-gov-02/P2C_R1_DESIGN.md §3.1-3.4).
// Pure: no IPC, no file, no network, no storage, no timer. Input: the registry projects, the user's claims per project and the (session-only) ECE
// observation slot. Output: one row per real (`reference`) project, plus at most one ATM row.
//
// Boundary (also in the design): observed fields and claim fields are different types and never convert into each other. A user claim of review or
// authorization changes only `claims`; "head verified", "review verified" and "authorization verified" are not representable in this version.
import { ATM_ROW_KEY, isEceObservation, TARGET_REPOSITORY, type EceObservation } from "./ciAssessment";
import type { ProjectView } from "./types";

// ---- user claims (USER_CLAIM) -----------------------------------------------------------------------------------------------------------

export type ReviewClaim = "NONE" | "CLAIMED_INDEPENDENT";
export type AuthorizationClaim = "NONE" | "CLAIMED_GRANTED";
export interface UserClaims {
  repository: string | null;
  acceptedSha: string | null;
  acceptedLabel: string | null;
  claimedHeadSha: string | null;
  reviewClaim: ReviewClaim;
  authorizationClaim: AuthorizationClaim;
  blockers: { text: string; issue: number | null }[];
  updatedAt: string | null;
}

// ---- observed fields (never assignable from claims) ---------------------------------------------------------------------------------------

export interface ObservedCommon {
  review: "UNREVIEWED" | "NOT_EVALUATED";
  authorization: "AUTHORIZATION_PENDING" | "NOT_EVALUATED";
  head: { kind: "UNKNOWN" };
}
/** Blockers that cannot be lifted in this version; shown on every row, separately from `recommended`. */
export interface Standing {
  readonly headNotVerified: true;
  readonly independentReviewNotVerified: true;
  readonly humanAuthorizationNotVerified: true;
}

export type RowNote =
  | "DUPLICATE_ATM_REPOSITORY_CLAIM"
  | "VIRTUAL_ATM_ROW_NO_CLAIMS"
  | "OBSERVED_COMMIT_DIFFERS_FROM_CLAIMED_ACCEPTED" // the ECE observation is about another commit than the claimed accepted one
  | "CLAIMED_HEAD_DIFFERS_FROM_OBSERVED_COMMIT"; // a claim only: the claimed HEAD differs from the observed commit; nothing here verifies any HEAD

// ---- recommended action: two disjoint sets; NONE_PENDING does not exist -----------------------------------------------------------------

export type AtmRecommended =
  | { kind: "ENTER_ACCEPTED_CHECKPOINT" }
  | { kind: "RUN_ECE_FOR_CLAIMED_ACCEPTED_SHA" }
  | { kind: "RETRY_LATER" }
  | { kind: "REVIEW_ASSESSMENT_ERROR" } // the assessment did not complete (input/evidence error); NOT a CI failure
  | { kind: "FIX_OR_RERUN" }
  | { kind: "RESOLVE_BLOCKERS" }
  | { kind: "OUTSIDE_ATM_STEPS_REMAIN" };
export type ForeignRecommended =
  | { kind: "ENTER_CLAIMS" }
  | { kind: "RESOLVE_BLOCKERS" }
  | { kind: "EXTERNAL_VERIFICATION_REQUIRED" };

// ---- rows: a true discriminated union on `role` -------------------------------------------------------------------------------------------

interface RowCommon {
  claims: UserClaims | null;
  standing: Standing;
  notes: RowNote[];
}
export interface AtmRow extends RowCommon {
  role: "ATM";
  identity: { kind: "reference"; projectId: string; displayName: string } | { kind: "atm-fixed-target"; rowKey: typeof ATM_ROW_KEY };
  observed: ObservedCommon & { ci: EceObservation | null };
  recommended: AtmRecommended;
}
export interface ForeignRow extends RowCommon {
  role: "FOREIGN";
  identity: { kind: "reference"; projectId: string; displayName: string };
  observed: ObservedCommon & { ci: null };
  recommended: ForeignRecommended;
}
export type OverviewRow = AtmRow | ForeignRow;

/** `boundRowKey` is the ATM row key (projectId or "atm-fixed-target") at the moment the submission started. */
export interface ObservationSlot {
  obs: EceObservation;
  boundRowKey: string;
}

// ---- construction -----------------------------------------------------------------------------------------------------------------------

const ATM_REPOSITORY_LC = TARGET_REPOSITORY.toLowerCase();

const standing = (): Standing => ({ headNotVerified: true, independentReviewNotVerified: true, humanAuthorizationNotVerified: true });

/** The ATM role comes ONLY from the repository claim, never from the alias, the display name or the note. */
const claimsAtm = (c: UserClaims | null | undefined): boolean => typeof c?.repository === "string" && c.repository.toLowerCase() === ATM_REPOSITORY_LC;

const hasAnyClaim = (c: UserClaims | null): boolean =>
  !!c && (c.repository !== null || c.acceptedSha !== null || c.acceptedLabel !== null || c.claimedHeadSha !== null || c.reviewClaim !== "NONE" ||
    c.authorizationClaim !== "NONE" || c.blockers.length > 0);

const byAge = (a: ProjectView, b: ProjectView): number => (a.createdAt < b.createdAt ? -1 : a.createdAt > b.createdAt ? 1 : a.projectId < b.projectId ? -1 : a.projectId > b.projectId ? 1 : 0);

function atmRecommended(claims: UserClaims | null, ci: EceObservation | null): AtmRecommended {
  if (!claims || claims.acceptedSha === null) return { kind: "ENTER_ACCEPTED_CHECKPOINT" };
  // An observation about another commit does not cover the claimed accepted commit.
  if (ci === null || ci.asked.commitSha !== claims.acceptedSha) return { kind: "RUN_ECE_FOR_CLAIMED_ACCEPTED_SHA" };
  const v = ci.view;
  if (v.providerFailure !== null) return { kind: "RETRY_LATER" };
  // Any assessment error (NOT_ASSESSED, attempt-not-latest, schema/policy errors ...) is not a CI failure.
  if (v.error !== null) return { kind: "REVIEW_ASSESSMENT_ERROR" };
  // A genuinely assessed CI problem: the run was assessed (no error) and a required job failed, is missing, or was skipped/cancelled.
  if (v.outcome === "ASSESSED_FAIL" && (v.ci === "FAILED" || v.ci === "INCOMPLETE_SKIPPED")) return { kind: "FIX_OR_RERUN" };
  // Only a completed, error-free, limited assessment with a verified pass can lead on; every other combination (contradictory ones included) is not a pass.
  if (!(v.outcome === "ASSESSED_LIMITATIONS" && v.ci === "VERIFIED_PASS")) return { kind: "REVIEW_ASSESSMENT_ERROR" };
  if (claims.blockers.length > 0) return { kind: "RESOLVE_BLOCKERS" };
  return { kind: "OUTSIDE_ATM_STEPS_REMAIN" };
}

/** Explanatory notes about claimed SHAs versus the observed commit. Only for a valid bound observation and only when the claim exists. */
function shaNotes(claims: UserClaims | null, ci: EceObservation | null): RowNote[] {
  if (!claims || ci === null) return [];
  const notes: RowNote[] = [];
  if (claims.acceptedSha !== null && claims.acceptedSha !== ci.asked.commitSha) notes.push("OBSERVED_COMMIT_DIFFERS_FROM_CLAIMED_ACCEPTED");
  if (claims.claimedHeadSha !== null && claims.claimedHeadSha !== ci.asked.commitSha) notes.push("CLAIMED_HEAD_DIFFERS_FROM_OBSERVED_COMMIT");
  return notes;
}

function foreignRecommended(claims: UserClaims | null): ForeignRecommended {
  if (!hasAnyClaim(claims)) return { kind: "ENTER_CLAIMS" };
  if (claims !== null && claims.blockers.length > 0) return { kind: "RESOLVE_BLOCKERS" };
  return { kind: "EXTERNAL_VERIFICATION_REQUIRED" };
}

function observedCommon(ci: EceObservation | null): ObservedCommon {
  // Observed review/authorization come only from a recognized ECE report (its closed vocabulary), never from a claim.
  if (ci === null) return { review: "NOT_EVALUATED", authorization: "NOT_EVALUATED", head: { kind: "UNKNOWN" } };
  return {
    review: ci.view.review === "UNREVIEWED" ? "UNREVIEWED" : "NOT_EVALUATED",
    authorization: ci.view.authorization === "AUTHORIZATION_PENDING" ? "AUTHORIZATION_PENDING" : "NOT_EVALUATED",
    head: { kind: "UNKNOWN" },
  };
}

/**
 * One row per `reference` project (synthetic projects never produce a row) plus at most one ATM row: the oldest project that claims the ATM repository,
 * or the virtual `atm-fixed-target` row when no project does. Later projects claiming the same repository are FOREIGN rows with a note and never get
 * an observation. The slot is applied only to the ATM row, only if it is a genuine observation of the fixed target, and only if it was bound to this
 * very ATM row key.
 */
export function buildRows(projects: readonly ProjectView[], claims: ReadonlyMap<string, UserClaims>, slot: ObservationSlot | null): OverviewRow[] {
  const real = projects.filter((p) => p.kind === "reference");
  const claimOf = (p: ProjectView): UserClaims | null => claims.get(p.projectId) ?? null;
  const atmProject = real.filter((p) => claimsAtm(claimOf(p))).sort(byAge)[0] ?? null;
  const atmKey = atmProject ? atmProject.projectId : ATM_ROW_KEY;
  const ci: EceObservation | null = slot !== null && slot.boundRowKey === atmKey && isEceObservation(slot.obs) ? slot.obs : null;

  const rows: OverviewRow[] = [];
  if (atmProject === null) {
    rows.push({
      role: "ATM", identity: { kind: "atm-fixed-target", rowKey: ATM_ROW_KEY }, claims: null, standing: standing(), notes: ["VIRTUAL_ATM_ROW_NO_CLAIMS"],
      observed: { ...observedCommon(ci), ci }, recommended: atmRecommended(null, ci),
    });
  }
  for (const p of real) {
    const c = claimOf(p);
    const identity = { kind: "reference" as const, projectId: p.projectId, displayName: p.displayName };
    if (atmProject !== null && p.projectId === atmProject.projectId) {
      rows.push({ role: "ATM", identity, claims: c, standing: standing(), notes: shaNotes(c, ci), observed: { ...observedCommon(ci), ci }, recommended: atmRecommended(c, ci) });
    } else {
      rows.push({
        role: "FOREIGN", identity, claims: c, standing: standing(), notes: claimsAtm(c) ? ["DUPLICATE_ATM_REPOSITORY_CLAIM"] : [],
        observed: { ...observedCommon(null), ci: null }, recommended: foreignRecommended(c),
      });
    }
  }
  return rows;
}
