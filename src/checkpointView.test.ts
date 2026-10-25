// ATM-GOV-01 - the pure, strict parser and view-model of "Checkpoint Merkezi". Deterministic: the clock is a parameter, nothing is read from anywhere.
// Every record here is made up; every "secret" is an obviously fake sentinel. Nothing is fetched, executed or stored.
import { describe, expect, it } from "vitest";
import source from "./checkpointView.ts?raw";
import {
  ASSESSMENTS, AVAILABILITY, CONCLUSIONS, ERROR_TEXT, LIMITS, MISSING_EVIDENCE, NOTE_TEXT, REVIEWER_ROLES, SCHEMA_VERSION, TRUST_LABEL, parseCheckpointInput,
  type ErrorCode, type ParseResult,
} from "./checkpointView";

const NOW = Date.UTC(2026, 9, 25, 12, 0, 0);
const SHA = "ba2fb81af4cd8bd802dfb0b34fff1af4f24bf71b";
const SHA2 = "68eda3f0407490fabccb517dcf822ab4818fc0bf";
const DIGEST = "a".repeat(64);
const ECHO = "SENTINEL_ECHO_9f2k";

type Obj = Record<string, unknown>;
const job = (over: Obj = {}) => ({ name: "windows-x64", claimedConclusion: "success", ...over });
const workflow = (over: Obj = {}) => ({ name: "CI", runId: 40175302645, runAttempt: 1, jobs: [job()], ...over });
const checkpoint = (over: Obj = {}) => ({ repository: "ornek/depo", phase: "ORNEK-01", sourceSha: SHA, workflows: [workflow()], ...over });
const envelope = (over: Obj = {}, ...cps: Obj[]) => ({ schemaVersion: SCHEMA_VERSION, checkpoints: cps.length ? cps : [checkpoint()], ...over });
const run = (v: unknown, now = NOW): ParseResult => parseCheckpointInput(typeof v === "string" ? v : JSON.stringify(v), now);
const code = (v: unknown, now = NOW): string => {
  const r = run(v, now);
  return r.ok ? "ok" : r.code;
};
const must = (r: ParseResult) => {
  if (!r.ok) throw new Error(`expected ok, got ${r.code} at ${r.path}`);
  return r.checkpoints;
};
/** Replace a nested value; `at` is an array of keys. */
const patch = (base: Obj, at: (string | number)[], value: unknown): Obj => {
  const copy = JSON.parse(JSON.stringify(base)) as Obj;
  let cur: any = copy;
  for (const k of at.slice(0, -1)) cur = cur[k];
  if (value === undefined) delete cur[at[at.length - 1]];
  else cur[at[at.length - 1]] = value;
  return copy;
};
const cpAt = (...rest: (string | number)[]) => ["checkpoints", 0, ...rest];

describe("a valid record is shown as a claim and carries no verdict", () => {
  it("minimal and full records parse, keep the pasted values and carry the constant trust label", () => {
    const [m] = must(run(envelope()));
    expect(m).toMatchObject({ trust: TRUST_LABEL, repository: "ornek/depo", phase: "ORNEK-01", sourceSha: SHA });
    expect(m.workflows[0]).toMatchObject({ runId: 40175302645, runAttempt: 1, ref: null, expectedJobs: null });
    const full = checkpoint({
      workflows: [workflow({ ref: "development", expectedJobs: ["windows-x64", "macos-arm64"], jobs: [job(), job({ name: "macos-arm64", claimedConclusion: "skipped" })] })],
      artifacts: [{ id: 7, name: "paket", sha256: DIGEST, expiresAt: "2026-11-01T00:00:00Z", availabilityClaim: "retained" }],
      issues: [33, 29], pullRequests: [5], reviewerClaim: { role: "independent_reviewer", assessment: "accepted", onSha: SHA },
      limitations: ["Yalnızca beyan"], missingEvidence: ["ci_not_checked", "branch_head_not_checked"],
    });
    const [f] = must(run(envelope({}, full)));
    expect(f.issues).toEqual([33, 29]);
    expect(f.reviewerClaim).toEqual({ role: "independent_reviewer", assessment: "accepted", onSha: SHA });
    expect(f.notes).toEqual([]);
    expect(f.trust).toBe(TRUST_LABEL);
    expect(TRUST_LABEL).toBe("KAYNAK BEYANI — DOĞRULANMADI");
  });

  it("the view-model has NO member that could carry a verdict, at any depth - not even for a record that claims success and acceptance", () => {
    const forged = checkpoint({
      workflows: [workflow({ jobs: [job({ claimedConclusion: "success" })] })],
      reviewerClaim: { role: "independent_reviewer", assessment: "accepted", onSha: SHA },
    });
    const keys = new Set<string>();
    const walk = (v: unknown) => {
      if (Array.isArray(v)) v.forEach(walk);
      else if (v && typeof v === "object") for (const [k, x] of Object.entries(v)) { keys.add(k); walk(x); }
    };
    walk(must(run(envelope({}, forged))));
    for (const k of keys) expect(k, k).not.toMatch(/^(verified|isVerified|accepted|isAccepted|passed|pass|trusted|isTrusted|verdict|status|result|ok|valid|approved|score|total|progress)$/i);
    expect(keys.has("trust")).toBe(true);
  });

  it("is deterministic and does not mutate or retain anything between calls", () => {
    const text = JSON.stringify(envelope());
    expect(JSON.stringify(run(text))).toBe(JSON.stringify(run(text)));
    expect(run(text)).not.toBe(run(text));
  });
});

describe("forged statuses are claims, never promoted; unknown vocabulary is refused", () => {
  it.each(["PASS", "PASSED", "VERIFIED", "ACCEPTED", "OK", "green", "Success", "SUCCESS", "success ", "passed"])("claimed conclusion %s is not in the closed list", (v) => {
    expect(code(patch(envelope(), cpAt("workflows", 0, "jobs", 0, "claimedConclusion"), v))).toBe("bad_value");
  });
  it("every listed conclusion, availability, role, assessment and missing-evidence entry is accepted as a CLAIM", () => {
    for (const c of CONCLUSIONS) expect(code(patch(envelope(), cpAt("workflows", 0, "jobs", 0, "claimedConclusion"), c)), c).toBe("ok");
    for (const a of AVAILABILITY) expect(code(patch(envelope(), cpAt("artifacts"), [{ id: 1, availabilityClaim: a }])), a).toBe("ok");
    for (const r of REVIEWER_ROLES) for (const a of ASSESSMENTS) expect(code(patch(envelope(), cpAt("reviewerClaim"), { role: r, assessment: a })), `${r}/${a}`).toBe("ok");
    expect(code(patch(envelope(), cpAt("missingEvidence"), [...MISSING_EVIDENCE]))).toBe("ok");
  });
  it("extra 'verified'-style fields anywhere are unexpected keys, not trusted properties", () => {
    for (const [at, k] of [
      [[], "verified"], [cpAt(), "status"], [cpAt(), "accepted"], [cpAt("workflows", 0), "conclusion"], [cpAt("workflows", 0, "jobs", 0), "passed"],
      [cpAt("reviewerClaim"), "independent"], [cpAt("artifacts", 0), "verified"],
    ] as [(string | number)[], string][]) {
      const base = patch(patch(envelope(), cpAt("reviewerClaim"), { role: "author", assessment: "accepted" }), cpAt("artifacts"), [{ id: 1 }]);
      const target = at.length === 0 ? base : (() => { const t = JSON.parse(JSON.stringify(base)); let cur = t; for (const x of at) cur = cur[x]; cur[k] = true; return t; })();
      if (at.length === 0) (target as Obj)[k] = true;
      expect(code(target), `${at.join(".")}.${k}`).toBe("unexpected_key");
    }
  });
  it("a reviewer claim about ANOTHER commit is kept as a claim and flagged with a neutral note", () => {
    const [c] = must(run(patch(envelope(), cpAt("reviewerClaim"), { role: "independent_reviewer", assessment: "accepted", onSha: SHA2 })));
    expect(c.notes).toEqual(["reviewer_other_commit"]);
    expect(c.reviewerClaim!.assessment).toBe("accepted");
    expect(NOTE_TEXT.reviewer_other_commit).toMatch(/geçerli sayılmaz/);
  });
});

describe("syntax of identities: strict, never normalised, never proof", () => {
  it.each([
    ["uppercase", SHA.toUpperCase()], ["39 chars", SHA.slice(1)], ["41 chars", SHA + "a"], ["abbreviated", SHA.slice(0, 7)], ["non-hex", SHA.replace("b", "g")],
    ["leading space", " " + SHA], ["trailing newline", SHA + "\n"], ["empty", ""], ["mixed case", SHA.slice(0, 20) + SHA.slice(20).toUpperCase()],
  ])("source SHA: %s is rejected", (_n, v) => {
    expect(code(patch(envelope(), cpAt("sourceSha"), v))).toBe("bad_value");
  });
  it("source SHA of the wrong type is rejected", () => {
    for (const v of [123, null, [], {}, true]) expect(code(patch(envelope(), cpAt("sourceSha"), v))).toBe("bad_type");
  });
  it.each([["uppercase", DIGEST.toUpperCase()], ["63", "a".repeat(63)], ["65", "a".repeat(65)], ["non-hex", "z".repeat(64)], ["a 40-char sha", SHA]])("artifact digest: %s is rejected", (_n, v) => {
    expect(code(patch(envelope(), cpAt("artifacts"), [{ id: 1, sha256: v }]))).toBe("bad_value");
  });
  it.each([
    "ornek", "a/b/c", "-a/b", "a-/b", "a--b/c", "a/..", "a/.", "a/", "/b", "a /b", "a/ b", "https://github.com/a/b", "a/b?x=1", "a/b#frag", "a/b\\c", "", "ornek/de po", "ornek/depo\n",
    "a".repeat(40) + "/b", "a/" + "b".repeat(101), "ornek/<b>", "ornek/depo.git/x",
  ])("repository %j is rejected", (v) => {
    expect(["bad_value", "bad_type"]).toContain(code(patch(envelope(), cpAt("repository"), v)));
  });
  it("repository comparison is case-insensitive on the lower-cased form only; the pasted spelling is kept", () => {
    const [c] = must(run(patch(envelope(), cpAt("repository"), "Ornek/Depo")));
    expect(c.repository).toBe("Ornek/Depo");
    const dup = envelope({}, checkpoint({ repository: "Ornek/Depo" }), checkpoint({ repository: "ornek/depo" }));
    expect(code(dup)).toBe("duplicate_record");
    expect(code(envelope({}, checkpoint({ repository: "ornek/depo" }), checkpoint({ repository: "ornek/baska", workflows: [workflow({ runId: 999 })] })))).toBe("ok");
  });
});

describe("run id, attempt and every other number", () => {
  it.each([
    ["zero", "0", "bad_value"], ["negative", "-1", "bad_number"], ["negative zero", "-0", "bad_number"], ["fraction", "1.5", "bad_number"], ["exponent", "1e3", "bad_number"],
    ["upper exponent", "1E3", "bad_number"], ["leading zero", "01", "bad_number"], ["17 digits", "12345678901234567", "bad_number"], ["beyond the safe range", "9007199254740993", "bad_value"],
    ["huge", "99999999999999999999999", "bad_number"],
  ])("runId %s is rejected", (_n, token, expected) => {
    const text = JSON.stringify(envelope()).replace("40175302645", token);
    expect(code(text)).toBe(expected);
  });
  it("runId of the wrong type is rejected, the largest safe integer is accepted", () => {
    for (const v of ["40175302645", null, [1], true, {}]) expect(code(patch(envelope(), cpAt("workflows", 0, "runId"), v))).toBe("bad_type");
    expect(code(JSON.stringify(envelope()).replace("40175302645", "9007199254740991"))).toBe("ok");
  });
  it("run attempt must be a positive integer within the bound", () => {
    for (const v of [0, 1001, 99999]) expect(code(patch(envelope(), cpAt("workflows", 0, "runAttempt"), v)), String(v)).toBe("bad_value");
    for (const v of ["1", null, 1.5]) expect(["bad_type", "bad_number"]).toContain(code(JSON.stringify(patch(envelope(), cpAt("workflows", 0, "runAttempt"), v))));
    expect(code(patch(envelope(), cpAt("workflows", 0, "runAttempt"), 1000))).toBe("ok");
    expect(code(patch(envelope(), cpAt("workflows", 0, "runAttempt"), 1))).toBe("ok");
  });
  it("issue, PR and artifact numbers must be positive and bounded; duplicates are refused", () => {
    for (const bad of [[0], [1000000000], [-5]]) expect(["bad_value", "bad_number"]).toContain(code(JSON.stringify(patch(envelope(), cpAt("issues"), bad))));
    expect(code(patch(envelope(), cpAt("issues"), [33, 33]))).toBe("duplicate_record");
    expect(code(patch(envelope(), cpAt("pullRequests"), [4, 4]))).toBe("duplicate_record");
    expect(code(patch(envelope(), cpAt("artifacts"), [{ id: 3 }, { id: 3 }]))).toBe("duplicate_record");
    expect(code(patch(envelope(), cpAt("artifacts"), [{ id: 0 }]))).toBe("bad_value");
    expect(code(patch(envelope(), cpAt("issues"), ["33"]))).toBe("bad_type");
    expect(code(patch(envelope(), cpAt("issues"), 33))).toBe("bad_type");
  });
  it("the same run id under two repositories or two commits is an inconsistent identity; the same run+attempt twice is a duplicate", () => {
    expect(code(envelope({}, checkpoint({ repository: "ornek/a" }), checkpoint({ repository: "ornek/b" })))).toBe("inconsistent_identity");
    expect(code(envelope({}, checkpoint({ phase: "A-01" }), checkpoint({ phase: "A-02", sourceSha: SHA2 })))).toBe("inconsistent_identity");
    expect(code(envelope({}, checkpoint({ phase: "A-01" }), checkpoint({ phase: "A-02" })))).toBe("duplicate_record"); // same repo+commit, same run+attempt in two records
    expect(code(envelope({}, checkpoint({ phase: "A-01" }), checkpoint({ phase: "A-02", workflows: [workflow({ runAttempt: 2 })] })))).toBe("ok"); // another attempt of the same run
    expect(code(patch(envelope(), cpAt("workflows"), [workflow(), workflow()]))).toBe("duplicate_record");
  });
});

describe("jobs and artifacts: claims, never results", () => {
  it("failed, skipped, cancelled, missing and extra jobs are shown as what the source said, with neutral notes", () => {
    const [c] = must(run(patch(envelope(), cpAt("workflows", 0), workflow({
      expectedJobs: ["windows-x64", "macos-arm64"],
      jobs: [job({ claimedConclusion: "failure" }), job({ name: "lint", claimedConclusion: "cancelled" })],
    }))));
    const w = c.workflows[0];
    expect(w.jobs.map((j) => j.claimed)).toEqual(["failure", "cancelled"]);
    expect(w.notes).toEqual(["expected_job_missing", "reported_job_not_expected"]);
    expect(NOTE_TEXT.expected_job_missing).toMatch(/başarısız olduğunu ya da hiç çalışmadığını göstermez/);
    expect(c.notes).toEqual([]);
  });
  it("a workflow without jobs is noted, a complete expected list is not", () => {
    expect(must(run(patch(envelope(), cpAt("workflows", 0, "jobs"), [])))[0].workflows[0].notes).toEqual(["workflow_no_jobs"]);
    expect(must(run(patch(envelope(), cpAt("workflows", 0, "expectedJobs"), ["windows-x64"])))[0].workflows[0].notes).toEqual([]);
  });
  it("duplicate job names (reported or expected) are ambiguous and refused", () => {
    expect(code(patch(envelope(), cpAt("workflows", 0, "jobs"), [job(), job({ claimedConclusion: "failure" })]))).toBe("duplicate_record");
    expect(code(patch(envelope(), cpAt("workflows", 0, "expectedJobs"), ["a", "a"]))).toBe("duplicate_record");
  });
  it("absent, retained, expired and unchecked artifacts: the claim is shown, the declared expiry is only interpreted against the given clock", () => {
    expect(must(run(envelope()))[0].artifacts).toEqual([]);
    const art = (over: Obj) => must(run(patch(envelope(), cpAt("artifacts"), [{ id: 1, sha256: DIGEST, ...over }])))[0].artifacts[0];
    expect(art({ expiresAt: "2026-10-25T11:59:59Z", availabilityClaim: "retained" })).toMatchObject({ expiry: "declared_past", notes: ["artifact_retained_but_expiry_passed"] });
    expect(art({ expiresAt: "2026-10-25T12:00:01Z", availabilityClaim: "expired" })).toMatchObject({ expiry: "declared_future", notes: ["artifact_expired_but_expiry_future"] });
    expect(art({ expiresAt: "2026-10-25T12:00:01Z", availabilityClaim: "retained" })).toMatchObject({ expiry: "declared_future", notes: [] });
    expect(art({ availabilityClaim: "not_checked" })).toMatchObject({ expiry: "not_declared", notes: [], availabilityClaim: "not_checked" });
    expect(art({ availabilityClaim: "missing" })).toMatchObject({ availabilityClaim: "missing" });
    expect(must(run(patch(envelope(), cpAt("artifacts"), [{ id: 2 }])))[0].artifacts[0].notes).toEqual(["artifact_no_digest"]);
  });
  it("expiry is deterministic in the clock: the boundary instant counts as passed and moving the clock flips the interpretation", () => {
    const text = JSON.stringify(patch(envelope(), cpAt("artifacts"), [{ id: 1, sha256: DIGEST, expiresAt: "2026-10-25T12:00:00Z" }]));
    const at = (now: number) => must(parseCheckpointInput(text, now))[0].artifacts[0].expiry;
    expect(at(NOW - 1)).toBe("declared_future");
    expect(at(NOW)).toBe("declared_past");
    expect(at(NOW + 1)).toBe("declared_past");
    expect(at(0)).toBe("declared_future");
  });
  it("an invalid expiry stamp is refused, not guessed", () => {
    for (const v of ["2026-02-30T00:00:00Z", "yesterday", "2026-10-25", "2026-10-25T25:00:00Z", "2026-10-25T00:00:00", "", " 2026-10-25T00:00:00Z"]) {
      expect(code(patch(envelope(), cpAt("artifacts"), [{ id: 1, expiresAt: v }])), JSON.stringify(v)).toBe("bad_value");
    }
    for (const v of [20261025, null, true]) expect(code(patch(envelope(), cpAt("artifacts"), [{ id: 1, expiresAt: v }]))).toBe("bad_type");
  });
});

describe("shape: exact keys, versions, JSON syntax", () => {
  it("unknown, missing or non-string schema versions are refused", () => {
    for (const v of ["atm.checkpoint-view.v2", "atm.checkpoint-view.v0", "ATM.CHECKPOINT-VIEW.V1", "atm.checkpoint-view.v1 ", "", 1, null, ["atm.checkpoint-view.v1"]]) {
      expect(code(patch(envelope(), ["schemaVersion"], v)), JSON.stringify(v)).toBe("unknown_schema_version");
    }
    expect(code(patch(envelope(), ["schemaVersion"], undefined))).toBe("missing_key");
  });
  it("an unexpected key at any level (including prototype-style names) is refused and its name is never reported", () => {
    const where: [string, (string | number)[]][] = [["root", []], ["checkpoint", cpAt()], ["workflow", cpAt("workflows", 0)], ["job", cpAt("workflows", 0, "jobs", 0)]];
    for (const key of ["extra", "__proto__", "constructor", "toString", "script", "url", ECHO]) {
      for (const [, at] of where) {
        const text = JSON.stringify(envelope()).replace(/^\{/, "{").replace(
          at.length === 0 ? '"schemaVersion"' : at.length === 2 ? '"repository"' : at.length === 4 ? '"name":"CI"' : '"claimedConclusion"',
          (m) => `"${key}":1,${m}`,
        );
        const r = run(text);
        expect(r.ok, `${key}`).toBe(false);
        expect(JSON.stringify(r)).not.toContain(ECHO);
      }
    }
    expect(code('{"schemaVersion":"atm.checkpoint-view.v1","checkpoints":[],"__proto__":{"x":1}}')).toBe("unexpected_key");
  });
  it("required keys are required at every level; null is never accepted", () => {
    for (const at of [["checkpoints"], cpAt("repository"), cpAt("phase"), cpAt("sourceSha"), cpAt("workflows"), cpAt("workflows", 0, "name"), cpAt("workflows", 0, "runId"), cpAt("workflows", 0, "runAttempt"), cpAt("workflows", 0, "jobs"), cpAt("workflows", 0, "jobs", 0, "name"), cpAt("workflows", 0, "jobs", 0, "claimedConclusion")]) {
      expect(code(patch(envelope(), at, undefined)), at.join(".")).toBe("missing_key");
    }
    for (const at of [cpAt("artifacts"), cpAt("issues"), cpAt("reviewerClaim"), cpAt("limitations"), cpAt("workflows", 0, "ref"), cpAt("workflows", 0, "expectedJobs")]) {
      expect(code(patch(envelope(), at, null)), at.join(".")).not.toBe("ok");
    }
  });
  it("wrong container types are refused", () => {
    for (const v of [[], "x", 1, null, true]) expect(code(v)).not.toBe("ok");
    for (const v of ["x", 1, {}, null]) expect(code(patch(envelope(), ["checkpoints"], v))).not.toBe("ok");
    expect(code(patch(envelope(), ["checkpoints"], []))).toBe("bad_value");
    expect(code(patch(envelope(), cpAt("workflows"), []))).toBe("bad_value");
    expect(code(patch(envelope(), ["checkpoints"], ["x"]))).toBe("bad_type");
  });
  it.each(["{", "}", "", "   ", "nul", "[", '{"a":}', "{'a':1}", '{"a":1,}', '{"a":1} {"b":2}', "// c\n{}", "{/*c*/}", "undefined", "NaN", "Infinity", '{"a":01}', '"\\ud800"'])("malformed or non-JSON text %j never parses", (t) => {
    const r = parseCheckpointInput(t, NOW);
    expect(r.ok).toBe(false);
  });
  it("non-string input and empty input are classified, not thrown", () => {
    for (const v of [undefined, null, 5, {}, []]) expect(parseCheckpointInput(v as unknown, NOW)).toEqual({ ok: false, code: "bad_type", path: "$" });
    expect(parseCheckpointInput("  \n ", NOW)).toEqual({ ok: false, code: "empty", path: "$" });
  });
});

describe("bounds: size, nesting, counts", () => {
  it("input over the character limit is refused before anything else", () => {
    expect(code("x".repeat(LIMITS.chars + 1))).toBe("too_large");
    expect(code(" ".repeat(LIMITS.chars + 1))).toBe("too_large");
    expect(code(JSON.stringify(envelope({}, checkpoint({ limitations: ["x"] })))).length).toBeGreaterThan(0);
  });
  it("deep nesting is refused without being parsed", () => {
    expect(code("[".repeat(LIMITS.depth + 1) + "]".repeat(LIMITS.depth + 1))).toBe("too_deep");
    expect(code("[".repeat(30000))).toBe("too_deep");
    expect(code('{"a":'.repeat(40) + "1" + "}".repeat(40))).toBe("too_deep");
    const deepPayload = patch(envelope(), cpAt("limitations"), [[[[[[[["x"]]]]]]]]);
    expect(code(deepPayload)).toBe("too_deep");
  });
  it("too many values is refused", () => {
    expect(code("[" + "1,".repeat(LIMITS.values + 5) + "1]")).toBe("too_many_values");
  });
  it("list counts: one over the limit is refused, the limit itself is accepted", () => {
    const many = <T,>(n: number, f: (i: number) => T) => Array.from({ length: n }, (_, i) => f(i));
    const cps = (n: number) => many(n, (i) => checkpoint({ phase: `P-${i}`, workflows: [workflow({ runId: 100 + i })] }));
    expect(code(envelope({ checkpoints: cps(LIMITS.checkpoints) }))).toBe("ok");
    expect(code(envelope({ checkpoints: cps(LIMITS.checkpoints + 1) }))).toBe("too_many_items");
    const wf = (n: number) => many(n, (i) => workflow({ runId: 500 + i }));
    expect(code(patch(envelope(), cpAt("workflows"), wf(LIMITS.workflows)))).toBe("ok");
    expect(code(patch(envelope(), cpAt("workflows"), wf(LIMITS.workflows + 1)))).toBe("too_many_items");
    const jobs = (n: number) => many(n, (i) => job({ name: `is-${i}` }));
    expect(code(patch(envelope(), cpAt("workflows", 0, "jobs"), jobs(LIMITS.jobs)))).toBe("ok");
    expect(code(patch(envelope(), cpAt("workflows", 0, "jobs"), jobs(LIMITS.jobs + 1)))).toBe("too_many_items");
    const arts = (n: number) => many(n, (i) => ({ id: i + 1 }));
    expect(code(patch(envelope(), cpAt("artifacts"), arts(LIMITS.artifacts)))).toBe("ok");
    expect(code(patch(envelope(), cpAt("artifacts"), arts(LIMITS.artifacts + 1)))).toBe("too_many_items");
    const nums = (n: number) => many(n, (i) => i + 1);
    expect(code(patch(envelope(), cpAt("issues"), nums(LIMITS.issues)))).toBe("ok");
    expect(code(patch(envelope(), cpAt("issues"), nums(LIMITS.issues + 1)))).toBe("too_many_items");
    expect(code(patch(envelope(), cpAt("pullRequests"), nums(LIMITS.pullRequests + 1)))).toBe("too_many_items");
    const lim = (n: number) => many(n, (i) => `Sinir ${i}`);
    expect(code(patch(envelope(), cpAt("limitations"), lim(LIMITS.limitations)))).toBe("ok");
    expect(code(patch(envelope(), cpAt("limitations"), lim(LIMITS.limitations + 1)))).toBe("too_many_items");
    expect(code(patch(envelope(), cpAt("missingEvidence"), [...MISSING_EVIDENCE, ...MISSING_EVIDENCE]))).toBe("too_many_items");
  });
  it("string length limits are exact", () => {
    // ordinary words (an unbroken 80-character run would rightly look like an opaque secret and be refused for that reason)
    const words = (n: number) => ("kelime ".repeat(Math.ceil(n / 7) + 1).slice(0, n - 1) + "x");
    expect(words(LIMITS.phase)).toHaveLength(LIMITS.phase);
    expect(code(patch(envelope(), cpAt("phase"), words(LIMITS.phase)))).toBe("ok");
    expect(code(patch(envelope(), cpAt("phase"), words(LIMITS.phase + 1)))).toBe("bad_value");
    expect(code(patch(envelope(), cpAt("limitations"), [words(LIMITS.limitation)]))).toBe("ok");
    expect(code(patch(envelope(), cpAt("limitations"), [words(LIMITS.limitation + 1)]))).toBe("bad_value");
    expect(code(patch(envelope(), cpAt("workflows", 0, "name"), words(LIMITS.name)))).toBe("ok");
    expect(code(patch(envelope(), cpAt("workflows", 0, "name"), words(LIMITS.name + 1)))).toBe("bad_value");
    expect(code(patch(envelope(), cpAt("phase"), "a".repeat(LIMITS.phase)))).toBe("unsafe_text"); // an unbroken run that long is opaque, not prose
    expect(code(patch(envelope(), cpAt("phase"), ""))).toBe("bad_value");
  });
});

describe("duplicate JSON keys are refused, however they are spelled", () => {
  it("at the root, nested, and with escaped spellings of the same key", () => {
    expect(code('{"schemaVersion":"atm.checkpoint-view.v1","schemaVersion":"atm.checkpoint-view.v1","checkpoints":[]}')).toBe("duplicate_key");
    const nested = JSON.stringify(envelope()).replace('"phase":"ORNEK-01"', '"phase":"ORNEK-01","phase":"ORNEK-02"');
    expect(code(nested)).toBe("duplicate_key");
    const escaped = JSON.stringify(envelope()).replace('"phase":"ORNEK-01"', '"phase":"ORNEK-01","ph\\u0061se":"ORNEK-02"');
    expect(code(escaped)).toBe("duplicate_key");
    expect(code('{"a":{"x":1},"b":{"x":2}}')).not.toBe("duplicate_key"); // the same key in different objects is fine (and then refused for its shape)
    expect(code('[{"a":1},{"a":2}]')).not.toBe("duplicate_key");
  });
});

describe("free text: inert, bounded, no markup, address, path or secret-shaped content", () => {
  const FIELDS: [string, (v: string) => Obj][] = [
    ["phase", (v) => patch(envelope(), cpAt("phase"), v)],
    ["workflow name", (v) => patch(envelope(), cpAt("workflows", 0, "name"), v)],
    ["job name", (v) => patch(envelope(), cpAt("workflows", 0, "jobs", 0, "name"), v)],
    ["expected job", (v) => patch(envelope(), cpAt("workflows", 0, "expectedJobs"), [v])],
    ["artifact name", (v) => patch(envelope(), cpAt("artifacts"), [{ id: 1, name: v }])],
    ["limitation", (v) => patch(envelope(), cpAt("limitations"), [v])],
  ];
  const HOSTILE: [string, string][] = [
    ["script tag", `<script>alert('${ECHO}')</script>`], ["img onerror", `<img src=x onerror=alert(1)>`], ["bold html", "<b>kalın</b>"], ["markdown link", `[tıkla](http://evil.example/${ECHO})`],
    ["markdown emphasis", "**kalın** ve `kod`"], ["markdown image", "![x](y)"], ["javascript uri", "javascript:alert(1)"], ["data uri", "data:text/html,xx"], ["https url", `https://evil.example/${ECHO}`],
    ["bare scheme text", "ftp:x"], ["www", "www.evil.test"], ["file uri", "file:///etc/passwd"], ["mailto", "mailto:x"], ["windows path", "C:\\Users\\kisi\\gizli"], ["unix path", "/etc/passwd"],
    ["home path", "~/.ssh/id_rsa"], ["parent path", "../../x"], ["github token", "ghp_" + "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8"], ["fine-grained token", "github_pat_11ABCDEFG0123456789"],
    ["openai-style key", "sk-ABCDEFGHIJKLMNOPQRSTUV"], ["aws key", "AKIAIOSFODNN7EXAMPLE"], ["slack token", "xoxb-123456789012"], ["private key block", "-----BEGIN PRIVATE KEY-----"],
    ["long hex", "0123456789abcdef0123456789abcdef"], ["long opaque", "A".repeat(41)], ["the word password", "password değeri"], ["the word token", "erişim token bilgisi"], ["bearer", "Bearer xyz"],
    ["authorization", "Authorization başlığı"], ["api key", "api key burada"], ["secret", "secret anahtar"], ["credential", "credentials"], ["control char", "a\u0000b"], ["newline", "a\nb"], ["tab", "a\tb"],
    ["rtl override", "ab\u202ecd"], ["zero width", "a\u200bb"], ["emoji", "başarılı 🎉"], ["leading space", " a"], ["trailing space", "a "], ["curly braces", "{{secret}}"], ["template", "${x}"],
    ["pipe and semicolon commands", "a | b && c"], ["equals", "a=b"], ["question", "a?b"], ["at sign", "a@b"], ["percent", "%41"], ["backslash", "a\\b"], ["quote", 'a"b'], ["angle", "a>b"],
    ["NFD form", "I\u0307yi"],
  ];
  for (const [field, build] of FIELDS) {
    it(`${field}: every hostile or unsafe value is refused and never echoed`, () => {
      for (const [label, value] of HOSTILE) {
        const r = run(build(value));
        expect(r.ok, `${field} / ${label}`).toBe(false);
        expect(JSON.stringify(r), `${field} / ${label}`).not.toContain(ECHO);
        expect(JSON.stringify(r), `${field} / ${label}`).not.toContain("evil");
      }
    });
    it(`${field}: ordinary Turkish text is accepted unchanged`, () => {
      const safe = "Faz 3D-6 · İnceleme (ön), gözden geçirme: devam; #33 + ÇŞĞÜÖİ çşğüöı";
      const [c] = must(run(build(safe)));
      expect(JSON.stringify(c)).toContain(safe);
    });
    it(`${field}: a non-string value is a type error`, () => {
      const base = JSON.stringify(build("MARKER"));
      for (const wrong of ["5", "null", "true", "[]", "{}"]) {
        const r = run(base.replace(/"MARKER"/, wrong));
        expect(r.ok, `${field} / ${wrong}`).toBe(false);
      }
    });
  }
  it("the ref is an assertion with its own narrow syntax; a path, a URL, parent segments or a secret are refused", () => {
    for (const ok of ["development", "main", "feature/x-1", "v1.2.3", "release_2026"]) expect(code(patch(envelope(), cpAt("workflows", 0, "ref"), ok)), ok).toBe("ok");
    for (const bad of ["/etc/passwd", "../x", "a//b", "-x", "https://x", "a b", "a;b", "ghp_" + "A1b2C3d4E5f6G7h8I9j0", "x".repeat(101), "", "a:b", "javascript"]) {
      expect(code(patch(envelope(), cpAt("workflows", 0, "ref"), bad)), bad).not.toBe("ok");
    }
    expect(code(patch(envelope(), cpAt("workflows", 0, "ref"), 5))).toBe("bad_type");
  });
});

describe("errors are generic: a closed code and a schema path, never a character of the input", () => {
  it("no failing input ever appears in the result, and the path only ever contains schema key names and indices", () => {
    const inputs: unknown[] = [
      `{"${ECHO}":1}`, `{"schemaVersion":"${ECHO}"}`, patch(envelope(), cpAt("phase"), `<${ECHO}>`), patch(envelope(), cpAt("repository"), ECHO), patch(envelope(), cpAt("sourceSha"), ECHO),
      patch(envelope({ [ECHO]: 1 }), [], undefined), `{ ${ECHO} `, ECHO, `[${ECHO}]`, patch(envelope(), cpAt("workflows", 0, "jobs", 0, "claimedConclusion"), ECHO),
      patch(envelope(), cpAt("limitations"), [`ghp_${ECHO}AAAAAAAAAAAAAAAAAAAA`]), patch(envelope(), cpAt("artifacts"), [{ id: 1, [ECHO]: 1 }]),
    ];
    for (const i of inputs) {
      const r = run(typeof i === "string" ? i : JSON.stringify(i));
      expect(r.ok).toBe(false);
      if (!r.ok) {
        expect(JSON.stringify(r)).not.toContain(ECHO);
        expect(r.path).toMatch(/^\$(\.(schemaVersion|checkpoints|repository|phase|sourceSha|workflows|artifacts|issues|pullRequests|reviewerClaim|limitations|missingEvidence|name|runId|runAttempt|ref|jobs|expectedJobs|claimedConclusion|id|sha256|expiresAt|availabilityClaim|role|assessment|onSha)|\[\d+\])*$/);
        expect(ERROR_TEXT[r.code as ErrorCode]).toBeTruthy();
      }
    }
  });
  it("every error code has a Turkish sentence that contains no brace, angle bracket or raw value", () => {
    for (const [k, v] of Object.entries(ERROR_TEXT)) {
      expect(v, k).not.toMatch(/[<>{}\\]/);
      expect(v.length, k).toBeGreaterThan(10);
    }
  });
});

describe("the module is pure", () => {
  it("no IPC, file, clipboard, network, storage, timer, process, dynamic code or clock", () => {
    const forbidden = /invoke\(|@tauri-apps|\bapi\.|fetch\(|XMLHttpRequest|WebSocket|navigator\.|localStorage|sessionStorage|indexedDB|setTimeout|setInterval|queueMicrotask|createObjectURL|FileReader|window\.|document\.|child_process|require\(|import\.meta\.env|process\.env|Date\.now|new Date|eval\(|new Function|dangerouslySetInnerHTML/;
    expect(source).not.toMatch(forbidden);
    expect(source.match(/^import .*$/gm)).toEqual(['import { parseRfc3339 } from "./review";']);
  });
});
