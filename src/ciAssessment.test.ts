// ATM-GOV-02 P2B - the pure model: strict input syntax and a CLOSED mapping of the native report. Unknown never degrades to a pass.
import { describe, expect, it } from "vitest";
import { describeReport, LABEL_AUTHORIZATION, LABEL_HISTORICAL, LABEL_MACHINE, LABEL_REVIEW, MAX_RUN_ATTEMPT, OUTCOME_LABEL, parseInput } from "./ciAssessment";
import { goodReport } from "./test/ciReport";
import type { AssessAtmCheckpointRequest, CiAssessmentReport } from "./types";

const SHA = "dd37b04f730569c3a8ad1b929ee6b80052ef22e2";
const ASKED: AssessAtmCheckpointRequest = { commitSha: SHA, runId: 41217731446, runAttempt: 1 };

describe("input syntax", () => {
  it("accepts exactly a 40-char lower-case hex SHA, a positive safe run id and an attempt 1..1000, and builds the three-field request", () => {
    expect(parseInput(SHA, "41217731446", "1")).toEqual({ ok: true, request: { commitSha: SHA, runId: 41217731446, runAttempt: 1 } });
    expect(parseInput(SHA, "9007199254740991", String(MAX_RUN_ATTEMPT))).toMatchObject({ ok: true });
  });

  it("rejects everything else, naming the field", () => {
    const bad: [string, string, string, string][] = [
      ["", "1", "1", "commitSha"], ["dd37b04", "1", "1", "commitSha"], [SHA.toUpperCase(), "1", "1", "commitSha"], [SHA + "0", "1", "1", "commitSha"],
      [" " + SHA.slice(1), "1", "1", "commitSha"], ["development", "1", "1", "commitSha"], ["zz" + SHA.slice(2), "1", "1", "commitSha"],
      [SHA, "", "1", "runId"], [SHA, "0", "1", "runId"], [SHA, "-5", "1", "runId"], [SHA, "1e3", "1", "runId"], [SHA, "12.5", "1", "runId"], [SHA, "0123", "1", "runId"],
      [SHA, "abc", "1", "runId"], [SHA, "9007199254740992", "1", "runId"], [SHA, "99999999999999999999", "1", "runId"],
      [SHA, "1", "", "runAttempt"], [SHA, "1", "0", "runAttempt"], [SHA, "1", "1001", "runAttempt"], [SHA, "1", "1.5", "runAttempt"], [SHA, "1", "-1", "runAttempt"],
    ];
    for (const [sha, run, att, field] of bad) expect(parseInput(sha, run, att), `${sha}|${run}|${att}`).toEqual({ ok: false, field });
  });
});

describe("the closed mapping of the native report", () => {
  it("shows a completed limited assessment with the four fixed labels", () => {
    const v = describeReport(goodReport(), ASKED);
    expect(v.recognized).toBe(true);
    if (!v.recognized) return;
    expect(v.assessed).toBe(true);
    expect(OUTCOME_LABEL[v.outcome]).toContain("kabul değildir");
    expect([LABEL_MACHINE, LABEL_REVIEW, LABEL_AUTHORIZATION, LABEL_HISTORICAL]).toEqual([
      "MAKİNE CI KANITI — ECE DEĞERLENDİRMESİ", "BAĞIMSIZ İNCELEME: YAPILMADI", "İNSAN ONAYI: BEKLEMEDE", "TARİHSEL KANIT — GÜNCEL HEAD ONAYI DEĞİLDİR",
    ]);
  });

  it("an unknown value in ANY member makes the whole report unrecognized", () => {
    const mutations: Partial<CiAssessmentReport>[] = [
      { outcome: "ASSESSED_PROVISIONAL_PASS" }, { outcome: "ACCEPTED" }, { ci: "SUPER_PASS" }, { availability: "PERFECT" }, { review: "INDEPENDENT_REVIEW_PASS" },
      { review: "REVIEWED" }, { authorization: "EXPLICITLY_AUTHORIZED" }, { authorization: "AUTHORIZED" }, { freshness: "CURRENT_HEAD" }, { limitations: ["NO_INDEPENDENT_REVIEW", "SOMETHING_NEW"] },
      { blocking: ["INVENTED"], outcome: "ASSESSED_FAIL", ci: "FAILED" }, { error: "NEW_ERROR", outcome: "NOT_ASSESSED" }, { schemaVersion: "atm.ece-ci-assessment.v2" },
      { schemaVersion: undefined as unknown as string },
    ];
    for (const m of mutations) expect(describeReport(goodReport(m), ASKED), JSON.stringify(m)).toEqual({ recognized: false });
    for (const junk of [null, undefined, 1, "x", [], {}]) expect(describeReport(junk, ASKED)).toEqual({ recognized: false });
  });

  it("an answer about something other than what was asked is unrecognized, and so is a different target", () => {
    const wrongCommit = goodReport();
    wrongCommit.provenance = { ...wrongCommit.provenance, commitSha: "0".repeat(40) };
    expect(describeReport(wrongCommit, ASKED)).toEqual({ recognized: false });
    for (const [k, v] of [["runId", 1], ["runAttempt", 2], ["repository", "ErenTahiroglu/Other"], ["repositoryId", 1432704795], ["workflowPath", ".github/workflows/other.yml"]] as const) {
      const r = goodReport();
      r.provenance = { ...r.provenance, [k]: v };
      expect(describeReport(r, ASKED), k).toEqual({ recognized: false });
    }
  });

  it("contradictions are never shown: a limited outcome needs a verified CI, no blocking reason and no error; an error is never an assessed outcome", () => {
    expect(describeReport(goodReport({ ci: "FAILED" }), ASKED)).toEqual({ recognized: false });
    expect(describeReport(goodReport({ blocking: ["UNEXPECTED_JOB_SET"] }), ASKED)).toEqual({ recognized: false });
    expect(describeReport(goodReport({ error: "PROVIDER_FAILURE", providerFailure: "TIMEOUT" }), ASKED)).toEqual({ recognized: false });
    expect(describeReport(goodReport({ outcome: "ASSESSED_FAIL", ci: "FAILED", error: "ATTEMPT_NOT_LATEST" }), ASKED)).toEqual({ recognized: false });
    expect(describeReport(goodReport({ providerFailure: "TIMEOUT" }), ASKED), "a provider failure kind needs the provider-failure error class").toEqual({ recognized: false });
  });

  it("a provider failure is recognized as NOT assessed, with its closed kind", () => {
    const v = describeReport(
      goodReport({ outcome: "INSUFFICIENT_EVIDENCE", ci: "UNVERIFIED_RUN", availability: "UNREACHABLE", review: "NOT_EVALUATED", freshness: "NOT_EVALUATED", error: "PROVIDER_FAILURE", providerFailure: "RATE_LIMITED" }),
      ASKED,
    );
    expect(v.recognized && v.assessed).toBe(false);
    expect(v.recognized && v.providerFailure).toBe("RATE_LIMITED");
  });
});
