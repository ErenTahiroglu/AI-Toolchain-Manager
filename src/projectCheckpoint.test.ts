// ATM-GOV-02 P2C-WP1 - the pure project checkpoint model (docs/atm-gov-02/P2C_R1_ACCEPTANCE.md: P, N, H, A and R items). Synthetic values only.
import { describe, expect, it } from "vitest";
import { ATM_ROW_KEY, isEceObservation, makeEceObservation, TARGET_REPOSITORY, TARGET_REPOSITORY_ID, TARGET_WORKFLOW, type EceObservation } from "./ciAssessment";
import {
  buildRows, type AtmRecommended, type AtmRow, type ForeignRecommended, type ForeignRow, type ObservationSlot, type OverviewRow, type UserClaims,
} from "./projectCheckpoint";
import { goodReport } from "./test/ciReport";
import type { AssessAtmCheckpointRequest, ProjectView } from "./types";

const SHA_A = "dd37b04f730569c3a8ad1b929ee6b80052ef22e2";
const SHA_B = "6ae8032000000000000000000000000000000000";
const ASKED: AssessAtmCheckpointRequest = { commitSha: SHA_A, runId: 41217731446, runAttempt: 1 };

const project = (id: string, over: Partial<ProjectView> = {}): ProjectView => ({
  projectId: id, alias: `alias-${id}`, displayName: `Project ${id}`, kind: "reference", localNote: "", createdAt: `2026-01-0${id.length % 9 + 1}T00:00:00Z`, preferences: [], ...over,
});
const claim = (over: Partial<UserClaims> = {}): UserClaims => ({
  repository: null, acceptedSha: null, acceptedLabel: null, claimedHeadSha: null, reviewClaim: "NONE", authorizationClaim: "NONE", blockers: [], updatedAt: null, ...over,
});
const atmClaim = (over: Partial<UserClaims> = {}) => claim({ repository: TARGET_REPOSITORY, ...over });
const obsOf = (report = goodReport(), asked = ASKED): EceObservation => {
  const o = makeEceObservation(report, asked);
  if (!o) throw new Error("fixture must be recognized");
  return o;
};
const slotFor = (key: string, obs = obsOf()): ObservationSlot => ({ obs, boundRowKey: key });
const rowsOf = (projects: ProjectView[], claims: Record<string, UserClaims>, slot: ObservationSlot | null = null) => buildRows(projects, new Map(Object.entries(claims)), slot);
const atmRows = (rows: OverviewRow[]) => rows.filter((r): r is AtmRow => r.role === "ATM");
const failedObs = () => obsOf(goodReport({ outcome: "ASSESSED_FAIL", ci: "FAILED", blocking: ["REQUIRED_JOBS_FAILED_OR_MISSING"], limitations: [] }));
const providerFailureObs = () =>
  obsOf(goodReport({ outcome: "INSUFFICIENT_EVIDENCE", ci: "NOT_EVALUATED", review: "NOT_EVALUATED", freshness: "NOT_EVALUATED", limitations: [], error: "PROVIDER_FAILURE", providerFailure: "RATE_LIMITED" }));

// A stamped copy built the only way a test can (the stamp is the observation's own symbol): used to reach the field checks, which the producer itself never violates.
const STAMP = (o: object) => Object.getOwnPropertySymbols(o)[0];
const deepFreeze = <T,>(x: T): T => { if (x && typeof x === "object") { Object.values(x).forEach(deepFreeze); Object.freeze(x); } return x; };
const forge = (mut: (o: Record<string, any>) => void, freeze = true): EceObservation => {
  const base = obsOf();
  const o: Record<string, any> = JSON.parse(JSON.stringify({ rowKey: base.rowKey, asked: base.asked, view: base.view }));
  mut(o);
  Object.defineProperty(o, STAMP(base), { value: true, enumerable: false });
  return (freeze ? deepFreeze(o) : o) as unknown as EceObservation;
};
// The native Rust `not_assessed()` contract (ece_ci.rs): NOT_ASSESSED / NOT_EVALUATED, review NOT_EVALUATED, authorization pending, three limitations, no blocking, an error.
const notAssessedReport = (error = "ATTEMPT_NOT_LATEST") => goodReport({
  outcome: "NOT_ASSESSED", ci: "NOT_EVALUATED", availability: "NOT_EVALUATED", review: "NOT_EVALUATED", authorization: "AUTHORIZATION_PENDING", freshness: "NOT_EVALUATED",
  limitations: ["NO_INDEPENDENT_REVIEW", "NO_HUMAN_AUTHORIZATION", "NOT_CURRENT_HEAD_ATTESTATION"], blocking: [], error, providerFailure: null,
});
const providerReport = (pf = "RATE_LIMITED") =>
  goodReport({ outcome: "INSUFFICIENT_EVIDENCE", ci: "NOT_EVALUATED", review: "NOT_EVALUATED", freshness: "NOT_EVALUATED", limitations: [], error: "PROVIDER_FAILURE", providerFailure: pf });

describe("the observation (ciAssessment.ts)", () => {
  it("makeEceObservation builds only from a recognized report", () => {
    expect(makeEceObservation(goodReport(), ASKED)).not.toBeNull();
    expect(makeEceObservation({ nope: 1 }, ASKED)).toBeNull();
    expect(makeEceObservation(goodReport({ outcome: "SOMETHING_NEW" }), ASKED)).toBeNull();
    expect(makeEceObservation(goodReport(), { ...ASKED, runAttempt: 2 })).toBeNull(); // answers about something else than asked
    expect(makeEceObservation(null, ASKED)).toBeNull();
  });

  it("A-07: isEceObservation checks the stamp, the row key, `asked`, recognition and provenance together", () => {
    const real = obsOf();
    expect(isEceObservation(real)).toBe(true);
    const fakes: unknown[] = [
      null, undefined, 1, "x", {}, { ...real }, JSON.parse(JSON.stringify(real)), { rowKey: ATM_ROW_KEY, asked: ASKED, view: real.view },
    ];
    for (const f of fakes) expect(isEceObservation(f)).toBe(false);
    // Each check on its own: a stamped, deeply frozen object with exactly one thing wrong.
    expect(isEceObservation(forge(() => {}))).toBe(true); // control: the forge itself is not what is rejected
    const bad: [string, (o: Record<string, any>) => void][] = [
      ["rowKey", (o) => { o.rowKey = "some-project"; }],
      ["short sha", (o) => { o.asked.commitSha = "dd37b04"; }],
      ["runId 0", (o) => { o.asked.runId = 0; }],
      ["attempt > max", (o) => { o.asked.runAttempt = 1001; }],
      ["not recognized", (o) => { o.view.recognized = false; }],
      ["repository", (o) => { o.view.provenance.repository = "someone/else"; }],
      ["repositoryId", (o) => { o.view.provenance.repositoryId = TARGET_REPOSITORY_ID + 1; }],
      ["workflowPath", (o) => { o.view.provenance.workflowPath = ".github/workflows/other.yml"; }],
      ["provenance sha vs asked", (o) => { o.view.provenance.commitSha = SHA_B; }],
      ["provenance runId vs asked", (o) => { o.view.provenance.runId = 1; }],
      ["provenance attempt vs asked", (o) => { o.view.provenance.runAttempt = 2; }],
    ];
    for (const [name, fn] of bad) expect(isEceObservation(forge(fn)), name).toBe(false);
    // Not frozen, only partly frozen, or only inheriting the stamp: rejected although every field is right.
    expect(isEceObservation(forge(() => {}, false))).toBe(false);
    const thawedList = forge(() => {}, false) as unknown as Record<string, any>;
    deepFreeze(thawedList.asked); deepFreeze(thawedList.view.provenance); Object.freeze(thawedList.view.blocking); Object.freeze(thawedList.view); Object.freeze(thawedList);
    expect(isEceObservation(thawedList)).toBe(false); // limitations left mutable
    expect(isEceObservation(Object.create(real))).toBe(false); // an inherited stamp is not an own stamp
    expect(TARGET_WORKFLOW).toBe(".github/workflows/ci.yml"); // the fixed target the checks above refer to
  });
});

describe("P: which rows exist", () => {
  it("P-01: synthetic projects never produce a row; only `reference` projects do", () => {
    const rows = rowsOf([project("s1", { kind: "synthetic" }), project("r1"), project("s2", { kind: "synthetic", alias: "ai-toolchain-manager" })], {});
    const keys = rows.flatMap((r) => (r.identity.kind === "reference" ? [r.identity.projectId] : []));
    expect(keys).toEqual(["r1"]);
    expect(atmRows(rows)).toHaveLength(1); // the virtual ATM row, not a synthetic project
  });

  it("P-02: no project claims the ATM repository ⇒ one virtual `atm-fixed-target` row (with no claims) and the real rows are FOREIGN", () => {
    const rows = rowsOf([project("r1")], { r1: claim({ repository: "other/repo" }) });
    const [v, f] = rows;
    expect(v).toMatchObject({ role: "ATM", identity: { kind: "atm-fixed-target", rowKey: ATM_ROW_KEY }, claims: null, notes: ["VIRTUAL_ATM_ROW_NO_CLAIMS"] });
    expect(f).toMatchObject({ role: "FOREIGN", identity: { kind: "reference", projectId: "r1" } });
  });

  it("N: the ATM role comes from the repository claim, not from the alias, display name or note", () => {
    const lookalike = project("r1", { alias: "ai-toolchain-manager", displayName: "ErenTahiroglu/AI-Toolchain-Manager", localNote: TARGET_REPOSITORY });
    const rows = rowsOf([lookalike], {});
    expect(rows.find((r) => r.identity.kind === "reference")).toMatchObject({ role: "FOREIGN" });
    expect(atmRows(rows)[0].identity.kind).toBe("atm-fixed-target");
    const claimed = rowsOf([project("r2", { alias: "x" })], { r2: atmClaim({ repository: TARGET_REPOSITORY.toUpperCase() }) });
    expect(claimed.find((r) => r.identity.kind === "reference")).toMatchObject({ role: "ATM" }); // case-insensitive equality
    expect(atmRows(claimed).every((r) => r.identity.kind === "reference")).toBe(true); // no virtual row next to a real ATM row
  });

  it("N-05: at most one ATM row; later duplicate ATM claims are FOREIGN with a note and get no observation", () => {
    const oldest = project("a", { createdAt: "2026-01-01T00:00:00Z" });
    const newer = project("b", { createdAt: "2026-02-01T00:00:00Z" });
    const rows = rowsOf([newer, oldest], { a: atmClaim(), b: atmClaim() }, slotFor("a"));
    expect(atmRows(rows).map((r) => r.identity.kind === "reference" && r.identity.projectId)).toEqual(["a"]);
    const dup = rows.find((r) => r.identity.kind === "reference" && r.identity.projectId === "b") as ForeignRow;
    expect(dup.role).toBe("FOREIGN");
    expect(dup.notes).toEqual(["DUPLICATE_ATM_REPOSITORY_CLAIM"]);
    expect(dup.observed.ci).toBeNull();
    expect(atmRows(rows)[0].observed.ci).not.toBeNull();
  });
});

describe("R: observation binding", () => {
  const atmProj = project("atm1");
  it("R-04: a non-ATM project never gets an observation, even if the slot is bound to its id", () => {
    const rows = rowsOf([project("f1")], { f1: claim({ repository: "other/repo" }) }, slotFor("f1"));
    for (const r of rows.filter((x) => x.role === "FOREIGN")) expect(r.observed.ci).toBeNull();
  });

  it("R-05/R-06: an unstamped, spread, JSON-copied or tampered observation is rejected by buildRows", () => {
    const real = obsOf();
    const bad: unknown[] = [{ ...real }, JSON.parse(JSON.stringify(real)), {}];
    for (const b of bad) expect(atmRows(rowsOf([atmProj], { atm1: atmClaim() }, { obs: b as EceObservation, boundRowKey: "atm1" }))[0].observed.ci).toBeNull();
    const badOnes = [
      forge((o) => { o.view.provenance.repositoryId = 7; }), forge((o) => { o.view.provenance.repository = "x/y"; }), forge((o) => { o.view.provenance.workflowPath = "w.yml"; }),
      forge((o) => { o.view.recognized = false; }), forge((o) => { o.rowKey = "atm1"; }), forge((o) => { o.asked.commitSha = "abc"; }),
      forge((o) => { o.view.provenance.commitSha = SHA_B; }), forge(() => {}, false),
    ];
    for (const b of badOnes) expect(atmRows(rowsOf([atmProj], { atm1: atmClaim() }, slotFor("atm1", b)))[0].observed.ci).toBeNull();
    expect(atmRows(rowsOf([atmProj], { atm1: atmClaim() }, slotFor("atm1")))[0].observed.ci).not.toBeNull(); // control: the genuine one is applied
  });

  it("R-07/R-08: a slot bound to another row key is not carried over", () => {
    // bound to project A, ATM binding moves to project B
    const moved = rowsOf([project("A"), project("B")], { A: claim({ repository: "other/repo" }), B: atmClaim() }, slotFor("A"));
    expect(moved.every((r) => r.observed.ci === null)).toBe(true);
    // bound to the virtual row, ATM becomes a real project
    expect(atmRows(rowsOf([atmProj], { atm1: atmClaim() }, slotFor(ATM_ROW_KEY)))[0].observed.ci).toBeNull();
    // bound to a project, ATM becomes virtual
    expect(atmRows(rowsOf([], {}, slotFor("atm1")))[0].observed.ci).toBeNull();
    // virtual row with a matching slot gets it
    expect(atmRows(rowsOf([], {}, slotFor(ATM_ROW_KEY)))[0].observed.ci).not.toBeNull();
  });
});

describe("H/A: ECE PASS and claims do not create verified review, authorization or HEAD", () => {
  const proj = [project("atm1")];
  it("H-01: ECE PASS leaves head UNKNOWN, review unverified, authorization pending, and standing true", () => {
    const row = atmRows(rowsOf(proj, { atm1: atmClaim({ acceptedSha: SHA_A }) }, slotFor("atm1")))[0];
    expect(row.observed.ci?.view.ci).toBe("VERIFIED_PASS");
    expect(row.observed.head).toEqual({ kind: "UNKNOWN" });
    expect(row.observed.review).toBe("UNREVIEWED");
    expect(row.observed.authorization).toBe("AUTHORIZATION_PENDING");
    expect(row.standing).toEqual({ headNotVerified: true, independentReviewNotVerified: true, humanAuthorizationNotVerified: true });
    expect(row.recommended).toEqual({ kind: "OUTSIDE_ATM_STEPS_REMAIN" });
  });

  it("A-09/R-11: no combination of claims changes observed review, authorization, head or standing", () => {
    const reviews = ["NONE", "CLAIMED_INDEPENDENT"] as const;
    const auths = ["NONE", "CLAIMED_GRANTED"] as const;
    const heads = [null, SHA_A, SHA_B];
    const slots = [null, slotFor("atm1")];
    for (const slot of slots) {
      const baseline = atmRows(rowsOf(proj, { atm1: atmClaim({ acceptedSha: SHA_A }) }, slot))[0];
      for (const reviewClaim of reviews) for (const authorizationClaim of auths) for (const claimedHeadSha of heads) {
        const row = atmRows(rowsOf(proj, { atm1: atmClaim({ acceptedSha: SHA_A, reviewClaim, authorizationClaim, claimedHeadSha }) }, slot))[0];
        expect(row.observed).toEqual(baseline.observed);
        expect(row.standing).toEqual(baseline.standing);
        expect(row.recommended).toEqual(baseline.recommended);
        expect(row.claims).toMatchObject({ reviewClaim, authorizationClaim, claimedHeadSha }); // the claim itself is kept, as a claim
      }
    }
    for (const reviewClaim of reviews) for (const authorizationClaim of auths) {
      const f = rowsOf([project("f1")], { f1: claim({ repository: "other/repo", reviewClaim, authorizationClaim }) }).find((r) => r.role === "FOREIGN")!;
      expect(f.observed).toEqual({ review: "NOT_EVALUATED", authorization: "NOT_EVALUATED", head: { kind: "UNKNOWN" }, ci: null });
      expect(f.standing).toEqual({ headNotVerified: true, independentReviewNotVerified: true, humanAuthorizationNotVerified: true });
    }
  });

  it("observed review/authorization do not depend on the CI result", () => {
    const pass = atmRows(rowsOf(proj, { atm1: atmClaim({ acceptedSha: SHA_A }) }, slotFor("atm1")))[0].observed;
    const fail = atmRows(rowsOf(proj, { atm1: atmClaim({ acceptedSha: SHA_A }) }, slotFor("atm1", failedObs())))[0].observed;
    expect({ ...fail, ci: null }).toEqual({ ...pass, ci: null });
  });
});

describe("recommended: two disjoint sets, no NONE_PENDING", () => {
  const proj = [project("atm1")];
  const atmRec = (c: UserClaims | null, slot: ObservationSlot | null) => atmRows(rowsOf(proj, c ? { atm1: c } : {}, slot))[0].recommended.kind;
  it("R-12: ATM priority, first match wins", () => {
    expect(atmRec(atmClaim(), null)).toBe("ENTER_ACCEPTED_CHECKPOINT");
    expect(atmRec(atmClaim({ acceptedSha: SHA_A }), null)).toBe("RUN_ECE_FOR_CLAIMED_ACCEPTED_SHA");
    expect(atmRec(atmClaim({ acceptedSha: SHA_B }), slotFor("atm1"))).toBe("RUN_ECE_FOR_CLAIMED_ACCEPTED_SHA"); // observation is about another commit
  });
  it("R-12: retry, fix, resolve, outside-steps", () => {
    const blockers = [{ text: "x", issue: null }];
    expect(atmRec(atmClaim({ acceptedSha: SHA_A, blockers }), slotFor("atm1", providerFailureObs()))).toBe("RETRY_LATER");
    expect(atmRec(atmClaim({ acceptedSha: SHA_A, blockers }), slotFor("atm1", failedObs()))).toBe("FIX_OR_RERUN");
    expect(atmRec(atmClaim({ acceptedSha: SHA_A, blockers }), slotFor("atm1"))).toBe("RESOLVE_BLOCKERS");
    expect(atmRec(atmClaim({ acceptedSha: SHA_A }), slotFor("atm1"))).toBe("OUTSIDE_ATM_STEPS_REMAIN");
  });
  it("R-12: foreign priority, and never an ECE action", () => {
    const rec = (c: UserClaims | null) => (rowsOf([project("f1")], c ? { f1: c } : {}).find((r) => r.role === "FOREIGN") as ForeignRow).recommended.kind;
    expect(rec(null)).toBe("ENTER_CLAIMS");
    expect(rec(claim())).toBe("ENTER_CLAIMS");
    expect(rec(claim({ repository: "o/r", blockers: [{ text: "x", issue: null }] }))).toBe("RESOLVE_BLOCKERS");
    expect(rec(claim({ repository: "o/r" }))).toBe("EXTERNAL_VERIFICATION_REQUIRED");
  });
  it("sets never mix and NONE_PENDING / AWAIT_* / VERIFY_CURRENT_HEAD_EXTERNALLY are never produced (deterministic sweep)", () => {
    const atmSet = new Set(["ENTER_ACCEPTED_CHECKPOINT", "RUN_ECE_FOR_CLAIMED_ACCEPTED_SHA", "RETRY_LATER", "REVIEW_ASSESSMENT_ERROR", "FIX_OR_RERUN", "RESOLVE_BLOCKERS", "OUTSIDE_ATM_STEPS_REMAIN"]);
    const foreignSet = new Set(["ENTER_CLAIMS", "RESOLVE_BLOCKERS", "EXTERNAL_VERIFICATION_REQUIRED"]);
    const slots = [null, slotFor("p0"), slotFor("p0", failedObs()), slotFor("p0", providerFailureObs()), slotFor("p0", obsOf(notAssessedReport())), slotFor(ATM_ROW_KEY)];
    const repos = [null, TARGET_REPOSITORY, TARGET_REPOSITORY.toLowerCase(), "other/repo"];
    const accepted = [null, SHA_A, SHA_B];
    const blockerSets = [[], [{ text: "b", issue: 1 }]];
    let seen = 0;
    for (const slot of slots) for (const repository of repos) for (const acceptedSha of accepted) for (const blockers of blockerSets) {
      const projects = [project("p0"), project("p1"), project("p2", { kind: "synthetic" })];
      const rows = rowsOf(projects, { p0: claim({ repository, acceptedSha, blockers }), p1: claim({ repository: repository ? TARGET_REPOSITORY : null, blockers }) }, slot);
      expect(atmRows(rows).length).toBe(1);
      for (const r of rows) {
        seen++;
        if (r.role === "ATM") expect(atmSet.has(r.recommended.kind)).toBe(true);
        else { expect(foreignSet.has(r.recommended.kind)).toBe(true); expect(r.observed.ci).toBeNull(); }
        expect(JSON.stringify(r)).not.toMatch(/NONE_PENDING|AWAIT_|VERIFY_CURRENT_HEAD_EXTERNALLY/);
        expect(r.standing).toEqual({ headNotVerified: true, independentReviewNotVerified: true, humanAuthorizationNotVerified: true });
      }
    }
    expect(seen).toBeGreaterThan(100);
  });
});

describe("RT-WP1-01: the observation is an immutable snapshot", () => {
  const mutate = (fn: () => void) => expect(fn).toThrow(TypeError);
  it("the observed CI result cannot be changed afterwards (FAILED never becomes VERIFIED_PASS)", () => {
    const obs = failedObs() as any;
    expect(isEceObservation(obs)).toBe(true);
    mutate(() => { obs.view.ci = "VERIFIED_PASS"; });
    mutate(() => { obs.view.outcome = "ASSESSED_LIMITATIONS"; });
    mutate(() => { obs.view.error = "ATTEMPT_NOT_LATEST"; });
    mutate(() => { obs.view.assessed = false; });
    mutate(() => { obs.view.blocking.length = 0; });
    mutate(() => { obs.view.blocking.push("X"); });
    mutate(() => { obs.view.limitations.push("X"); });
    mutate(() => { obs.view = { ...obs.view }; });
    mutate(() => { obs.rowKey = "other"; });
    mutate(() => { obs.asked.commitSha = SHA_B; });
    mutate(() => { obs.view.provenance.commitSha = SHA_B; });
    mutate(() => { delete obs.view.provenance; });
    expect(obs.view.ci).toBe("FAILED");
    expect(isEceObservation(obs)).toBe(true);
    expect(atmRows(rowsOf([project("atm1")], { atm1: atmClaim({ acceptedSha: SHA_A }) }, slotFor("atm1", obs)))[0].recommended).toEqual({ kind: "FIX_OR_RERUN" });
  });
  it("every nested object and array of the snapshot is frozen", () => {
    const obs = obsOf() as any;
    for (const x of [obs, obs.asked, obs.view, obs.view.provenance, obs.view.limitations, obs.view.blocking]) expect(Object.isFrozen(x)).toBe(true);
  });
  it("the snapshot is independent of the native report, and the report is neither frozen nor changed", () => {
    const report = goodReport();
    const before = JSON.stringify(report);
    const obs = makeEceObservation(report, ASKED)!;
    expect(JSON.stringify(report)).toBe(before);
    expect(Object.isFrozen(report) || Object.isFrozen(report.provenance) || Object.isFrozen(report.limitations) || Object.isFrozen(report.blocking)).toBe(false);
    report.ci = "FAILED"; report.outcome = "ASSESSED_FAIL"; report.provenance.commitSha = SHA_B; report.limitations.push("X"); report.blocking.push("Y");
    expect(obs.view.ci).toBe("VERIFIED_PASS");
    expect(obs.view.outcome).toBe("ASSESSED_LIMITATIONS");
    expect(obs.view.provenance.commitSha).toBe(SHA_A);
    expect(obs.view.limitations).not.toContain("X");
    expect(obs.view.blocking).toEqual([]);
    expect(isEceObservation(obs)).toBe(true);
  });
  it("a spread or JSON copy of a valid observation is not a valid observation", () => {
    const obs = obsOf();
    expect(isEceObservation({ ...obs })).toBe(false);
    expect(isEceObservation(JSON.parse(JSON.stringify(obs)))).toBe(false);
    expect(isEceObservation(Object.assign({}, obs))).toBe(false);
  });
});

describe("RT-WP1-02: an assessment error is not a CI failure", () => {
  const rec = (report: ReturnType<typeof goodReport>, accepted = SHA_A) =>
    atmRows(rowsOf([project("atm1")], { atm1: atmClaim({ acceptedSha: accepted }) }, slotFor("atm1", obsOf(report))))[0].recommended.kind;
  it("native not_assessed() (NOT_ASSESSED / NOT_EVALUATED + an error) recommends reviewing the assessment error, never FIX_OR_RERUN", () => {
    for (const e of ["ATTEMPT_NOT_LATEST", "BINDING_MISMATCH", "POLICY_INTEGRITY", "EVALUATOR_FAILURE", "CLIENT_INIT_FAILURE", "OUTPUT_SCHEMA_VIOLATION"]) {
      expect(rec(notAssessedReport(e)), e).toBe("REVIEW_ASSESSMENT_ERROR");
    }
  });
  it("provider failure ⇒ RETRY_LATER; a genuinely assessed failure or skip ⇒ FIX_OR_RERUN; a completed pass ⇒ outside-steps", () => {
    expect(rec(providerReport("TIMEOUT"))).toBe("RETRY_LATER");
    expect(rec(goodReport({ outcome: "ASSESSED_FAIL", ci: "FAILED", blocking: ["REQUIRED_JOBS_FAILED_OR_MISSING"], limitations: [] }))).toBe("FIX_OR_RERUN");
    expect(rec(goodReport({ outcome: "ASSESSED_FAIL", ci: "INCOMPLETE_SKIPPED", blocking: ["REQUIRED_JOBS_SKIPPED_OR_CANCELLED"], limitations: [] }))).toBe("FIX_OR_RERUN");
    expect(rec(goodReport())).toBe("OUTSIDE_ATM_STEPS_REMAIN");
  });
  it("contradictory or inconclusive combinations are never read as a completed pass or as a CI failure", () => {
    expect(rec(goodReport({ outcome: "NOT_ASSESSED", ci: "VERIFIED_PASS", error: null }))).toBe("REVIEW_ASSESSMENT_ERROR");
    expect(rec(goodReport({ outcome: "ASSESSED_FAIL", ci: "VERIFIED_PASS", limitations: [] }))).toBe("REVIEW_ASSESSMENT_ERROR");
    expect(rec(goodReport({ outcome: "INSUFFICIENT_EVIDENCE", ci: "UNVERIFIED_RUN", limitations: [] }))).toBe("REVIEW_ASSESSMENT_ERROR");
    expect(rec(goodReport({ outcome: "ASSESSED_FAIL", ci: "ATTEMPT_MISMATCH", limitations: [] }))).toBe("REVIEW_ASSESSMENT_ERROR");
    expect(rec(goodReport({ outcome: "NOT_ASSESSED", ci: "NOT_EVALUATED", error: null, limitations: [] }))).toBe("REVIEW_ASSESSMENT_ERROR");
  });
  it("a user blocker does not turn an error into a pass: only a completed pass reaches RESOLVE_BLOCKERS", () => {
    const withBlocker = (report: ReturnType<typeof goodReport>) =>
      atmRows(rowsOf([project("atm1")], { atm1: atmClaim({ acceptedSha: SHA_A, blockers: [{ text: "b", issue: null }] }) }, slotFor("atm1", obsOf(report))))[0].recommended.kind;
    expect(withBlocker(goodReport())).toBe("RESOLVE_BLOCKERS");
    expect(withBlocker(notAssessedReport())).toBe("REVIEW_ASSESSMENT_ERROR");
  });
  it("review/authorization/head/standing keep their fixed values for an error observation", () => {
    const row = atmRows(rowsOf([project("atm1")], { atm1: atmClaim({ acceptedSha: SHA_A }) }, slotFor("atm1", obsOf(notAssessedReport()))))[0];
    expect(row.observed).toMatchObject({ review: "NOT_EVALUATED", authorization: "AUTHORIZATION_PENDING", head: { kind: "UNKNOWN" } });
    expect(row.standing).toEqual({ headNotVerified: true, independentReviewNotVerified: true, humanAuthorizationNotVerified: true });
  });
});

describe("RT-WP1-03: SHA mismatch notes", () => {
  const notes = (c: Partial<UserClaims>, slot: ObservationSlot | null = slotFor("atm1")) => atmRows(rowsOf([project("atm1")], { atm1: atmClaim(c) }, slot))[0].notes;
  it("observed X, accepted Y, claimed HEAD Z ⇒ both notes", () => {
    const Z = "c".repeat(40);
    expect(notes({ acceptedSha: SHA_B, claimedHeadSha: Z })).toEqual(["OBSERVED_COMMIT_DIFFERS_FROM_CLAIMED_ACCEPTED", "CLAIMED_HEAD_DIFFERS_FROM_OBSERVED_COMMIT"]);
  });
  it("only the differing claim produces its note", () => {
    expect(notes({ acceptedSha: SHA_B, claimedHeadSha: SHA_A })).toEqual(["OBSERVED_COMMIT_DIFFERS_FROM_CLAIMED_ACCEPTED"]);
    expect(notes({ acceptedSha: SHA_A, claimedHeadSha: SHA_B })).toEqual(["CLAIMED_HEAD_DIFFERS_FROM_OBSERVED_COMMIT"]);
  });
  it("no false notes: equal SHAs, missing claims, no observation, invalid or unbound observation", () => {
    expect(notes({ acceptedSha: SHA_A, claimedHeadSha: SHA_A })).toEqual([]); // equality is still only a claim: no verification wording
    expect(notes({})).toEqual([]);
    expect(notes({ acceptedSha: SHA_B, claimedHeadSha: SHA_B }, null)).toEqual([]);
    expect(notes({ acceptedSha: SHA_B, claimedHeadSha: SHA_B }, slotFor("atm1", forge((o) => { o.view.provenance.repositoryId = 7; })))).toEqual([]);
    expect(notes({ acceptedSha: SHA_B, claimedHeadSha: SHA_B }, slotFor("other-key"))).toEqual([]);
  });
  it("a foreign row gets no SHA note and no observation; duplicate and virtual notes are kept", () => {
    const rows = rowsOf([project("f1"), project("a"), project("b", { createdAt: "2027-01-01T00:00:00Z" })],
      { f1: claim({ repository: "o/r", acceptedSha: SHA_B, claimedHeadSha: SHA_B }), a: atmClaim({ acceptedSha: SHA_B }), b: atmClaim({ acceptedSha: SHA_B }) }, slotFor("a"));
    const byId = (id: string) => rows.find((r) => r.identity.kind === "reference" && r.identity.projectId === id)!;
    expect(byId("f1").notes).toEqual([]);
    expect(byId("f1").observed.ci).toBeNull();
    expect(byId("b").notes).toEqual(["DUPLICATE_ATM_REPOSITORY_CLAIM"]);
    expect(byId("a").notes).toEqual(["OBSERVED_COMMIT_DIFFERS_FROM_CLAIMED_ACCEPTED"]);
    expect(atmRows(rowsOf([], {}, slotFor(ATM_ROW_KEY)))[0].notes).toEqual(["VIRTUAL_ATM_ROW_NO_CLAIMS"]); // virtual row: no claims ⇒ no SHA notes
  });
});

describe("type level (checked by `tsc`, `@ts-expect-error` must be an error)", () => {
  it("R-01..R-03 and NONE_PENDING cannot be expressed", () => {
    const obs = obsOf();
    const foreign = (rowsOf([project("f1")], {}).find((r) => r.role === "FOREIGN")) as ForeignRow;
    const atm = atmRows(rowsOf([], {}))[0];
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const typeChecks = () => {
      // @ts-expect-error R-01: a foreign row cannot carry an observation
      foreign.observed.ci = obs;
      // @ts-expect-error R-02: a foreign row cannot get an ATM action
      foreign.recommended = { kind: "RUN_ECE_FOR_CLAIMED_ACCEPTED_SHA" } satisfies AtmRecommended;
      // @ts-expect-error R-02: an ATM row cannot get a foreign action
      atm.recommended = { kind: "EXTERNAL_VERIFICATION_REQUIRED" } satisfies ForeignRecommended;
      // @ts-expect-error R-03: a foreign row has no virtual identity
      foreign.identity = { kind: "atm-fixed-target", rowKey: ATM_ROW_KEY };
      // @ts-expect-error NONE_PENDING is not a recommended action
      const a: AtmRecommended = { kind: "NONE_PENDING" };
      // @ts-expect-error NONE_PENDING is not a recommended action
      const f: ForeignRecommended = { kind: "NONE_PENDING" };
      // @ts-expect-error observed review cannot hold a claim value
      foreign.observed.review = "CLAIMED_INDEPENDENT";
      // @ts-expect-error observed authorization cannot hold a claim value
      foreign.observed.authorization = "CLAIMED_GRANTED";
      // @ts-expect-error a verified head is not representable
      foreign.observed.head = { kind: "VERIFIED" };
      // @ts-expect-error standing cannot be lifted
      foreign.standing.headNotVerified = false;
      // @ts-expect-error a plain object is not an observation
      const o: EceObservation = { rowKey: ATM_ROW_KEY, asked: ASKED, view: obs.view };
      return [a, f, o];
    };
    expect(typeof typeChecks).toBe("function"); // never called: the assertions are the compiler's
  });
});
