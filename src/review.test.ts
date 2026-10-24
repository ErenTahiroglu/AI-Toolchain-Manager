// PRODUCT-05 - pure tests of the review-queue derivation (src/review.ts). The clock is a parameter: nothing here reads the wall clock.
import { describe, expect, it } from "vitest";
import type { InventoryRead } from "./observation";
import { EVIDENCE_REVIEW_STATUSES, deriveReview, filterItems, parseRfc3339, reviewHeadline, reviewKey, type ProjectsRead } from "./review";
import { emptyInventory, makeRequest, pref } from "./test/backend";
import type { AppError, Inventory, PreferenceView, ProjectView, RequestStatus } from "./types";

const NOW = Date.UTC(2026, 9, 24, 12, 0, 0); // 2026-10-24T12:00:00Z
const WIN = "windows-x64";
const MAC = "macos-arm64";
const CC = "claude-code-terminal";
const SURFACES = ["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "local-cli"];
const ERR: AppError = { code: "DB_BUSY", message: "meşgul" };

const wish = (entryId: string, over: Partial<PreferenceView> = {}): PreferenceView =>
  pref({ entryId, entryName: entryId, agentSurface: CC, platformTarget: WIN, decision: "accepted", ...over });
const project = (alias: string, preferences: PreferenceView[], displayName = alias): ProjectView => ({
  projectId: `id-${alias}`, alias, displayName, kind: "reference", localNote: "GİZLİ-NOT", createdAt: "2026-10-17T00:00:00Z", preferences,
});
const ready = (data: ProjectView[]): ProjectsRead => ({ data, error: null, loading: false });
const inv = (...requests: Inventory["requests"]): InventoryRead => ({ data: { ...emptyInventory(), requests }, error: null, loading: false });
const req = (status: RequestStatus, over: Partial<Parameters<typeof makeRequest>[0]> = {}) => makeRequest({ entryId: "alfa", projectAlias: "a", status, ...over });
const derive = (projects: ProjectView[], inventory: InventoryRead = inv(), now = NOW) => deriveReview(ready(projects), inventory, now);
const one = (preference: PreferenceView, inventory: InventoryRead = inv(), now = NOW) => derive([project("a", [preference])], inventory, now);

function deepFreeze<T>(o: T): T {
  if (o && typeof o === "object" && !Object.isFrozen(o)) {
    Object.freeze(o);
    for (const v of Object.values(o as object)) deepFreeze(v);
  }
  return o;
}

describe("every reason, alone", () => {
  it("material change", () => {
    const q = one(wish("alfa", { materialChange: true, decision: "rejected" }));
    expect(q.items).toHaveLength(1);
    expect(q.items[0].reasons).toEqual([{ code: "material_change" }]);
  });

  it("a removed catalog entry that still has a decision", () => {
    const q = one(wish("alfa", { entryRemoved: true }));
    expect(q.items[0].reasons).toEqual([{ code: "entry_removed" }]);
    expect(q.items[0].openable).toBe(true); // the matrix shows it as removed / reset-only
  });

  it("a postponed decision whose reminder is due", () => {
    const q = one(wish("alfa", { decision: "postponed", remindAt: "2026-10-24T11:59:59Z" }));
    expect(q.items[0].reasons).toEqual([{ code: "reminder_due", remindAt: "2026-10-24T11:59:59Z" }]);
  });

  it.each(EVIDENCE_REVIEW_STATUSES)("accepted wish whose stored classification is %s", (status) => {
    const q = one(wish("alfa"), inv(req(status)));
    expect(q.items).toHaveLength(1);
    expect(q.items[0].reasons).toEqual([{ code: "evidence", status }]);
    expect(q.items[0].observation?.status).toBe(status);
    expect(q.uncertain).toEqual([]);
  });

  it("present_current is information, never a reason - and never 'resolved' by disappearing from a list it was never on", () => {
    const q = one(wish("alfa"), inv(req("present_current")));
    expect(q.items).toEqual([]);
    expect(q.uncertain).toEqual([]);
    // next to a real reason it is carried as a plain stored finding
    const both = one(wish("alfa", { materialChange: true }), inv(req("present_current")));
    expect(both.items[0].reasons).toEqual([{ code: "material_change" }]);
    expect(both.items[0].observation).toEqual({ status: "present_current", scanFinishedAt: "2026-10-23T09:00:00Z" });
  });

  it("the review statuses are exactly the ones the backend defines as not-current-or-negative; present_current is not among them", () => {
    expect([...EVIDENCE_REVIEW_STATUSES].sort()).toEqual(["absent_current", "not_scanned", "platform_mismatch", "previously_absent", "previously_present", "previously_unknown", "unknown_current"]);
  });

  it("a wish with nothing to review produces nothing", () => {
    expect(one(wish("alfa"), inv(req("present_current"))).items).toEqual([]);
    expect(one(wish("alfa", { decision: "rejected" })).items).toEqual([]);
    expect(derive([project("a", [])]).items).toEqual([]);
  });
});

describe("one item per exact key, with all its reasons", () => {
  it("several reasons on one key are ONE item carrying all of them, in a fixed order", () => {
    const q = one(wish("alfa", { materialChange: true }), inv(req("not_scanned")));
    expect(q.items).toHaveLength(1);
    expect(q.items[0].reasons.map((r) => r.code)).toEqual(["material_change", "evidence"]);
    const removed = one(wish("alfa", { materialChange: true, entryRemoved: true }));
    expect(removed.items).toHaveLength(1);
    expect(removed.items[0].reasons.map((r) => r.code)).toEqual(["material_change", "entry_removed"]);
    const postponed = one(wish("alfa", { decision: "postponed", remindAt: "2026-10-01T00:00:00Z", materialChange: true }));
    expect(postponed.items[0].reasons.map((r) => r.code)).toEqual(["material_change", "reminder_due"]);
  });

  it("a removed entry never gets a stored-evidence reason, even if the inventory holds a row for the key", () => {
    const q = one(wish("alfa", { entryRemoved: true }), inv(req("not_scanned")));
    expect(q.items[0].reasons.map((r) => r.code)).toEqual(["entry_removed"]);
    expect(q.items[0].observation).toBeNull();
  });

  it("only an ACCEPTED wish is compared with the stored evidence", () => {
    for (const decision of ["postponed", "rejected", "disabled"] as const) {
      const q = one(wish("alfa", { decision, remindAt: decision === "postponed" ? "2099-01-01T00:00:00Z" : null }), inv(req("not_scanned")));
      expect(q.items, decision).toEqual([]);
      expect(q.uncertain, decision).toEqual([]);
    }
  });

  it("the same tool in two projects, two systems and four surfaces is eight separate keys - none carries another's evidence", () => {
    const prefs = [wish("alfa", { platformTarget: WIN }), wish("alfa", { platformTarget: MAC }), ...SURFACES.slice(1).map((s) => wish("alfa", { agentSurface: s }))];
    const projects = [project("a", prefs), project("b", prefs.slice(0, 2)), project("c", [wish("alfa")])]; // 5 + 2 + 1 = 8 keys
    const q = derive(projects, inv(...["a", "b", "c"].flatMap((alias) => req("not_scanned", { projectAlias: alias })))); // only the CC/WIN rows of each project exist
    const keys = new Set([...q.items, ...q.uncertain].map((i) => i.key));
    expect(keys.size).toBe(8);
    // evidence reasons exist only where the exact four-field key has a backend row
    const withEvidence = q.items.map((i) => `${i.alias}/${i.platform}/${i.surface}`).sort();
    expect(withEvidence).toEqual([`a/${WIN}/${CC}`, `b/${WIN}/${CC}`, `c/${WIN}/${CC}`]);
    // every other accepted wish is uncertain ("no matching row"), not "not scanned"
    expect(q.uncertain).toHaveLength(5);
    for (const u of q.uncertain) expect(u.uncertain).toEqual(["no_row"]);
  });

  it("distinct tools with the same display name stay distinct items", () => {
    const q = derive([project("a", [wish("t1", { entryName: "İkiz", materialChange: true }), wish("t2", { entryName: "İkiz", materialChange: true })])]);
    expect(q.items.map((i) => i.entryId)).toEqual(["t1", "t2"]);
    expect(new Set(q.items.map((i) => i.key)).size).toBe(2);
  });

  it("a second record for one exact key does not make a second item; the doubt is stated", () => {
    const q = derive([project("a", [wish("alfa", { materialChange: true }), wish("alfa", { materialChange: true })])]);
    expect(q.items).toHaveLength(1);
    expect(q.items[0].uncertain).toContain("duplicate_wish");
    const q2 = derive([project("a", [wish("alfa", { decision: "rejected" }), wish("alfa", { decision: "rejected" })])]);
    expect(q2.items).toEqual([]);
    expect(q2.uncertain).toHaveLength(1);
    expect(q2.uncertain[0].uncertain).toEqual(["duplicate_wish"]);
  });

  it("reviewKey separates all four fields", () => {
    const base = reviewKey("a", "alfa", WIN, CC);
    expect(new Set([base, reviewKey("b", "alfa", WIN, CC), reviewKey("a", "beta", WIN, CC), reviewKey("a", "alfa", MAC, CC), reviewKey("a", "alfa", WIN, "local-cli")]).size).toBe(5);
  });
});

describe("determinism and immutability", () => {
  const projects = () => [
    project("zz", [wish("b", { materialChange: true }), wish("a", { materialChange: true, platformTarget: MAC })], "Zeta"),
    project("aa", [wish("c", { materialChange: true, entryName: "Çay" }), wish("a", { materialChange: true, entryName: "Alfa" })], "Alfa proje"),
  ];

  it("the order does not depend on the order of the input", () => {
    const forward = derive(projects());
    const reversed = derive(projects().reverse().map((p) => ({ ...p, preferences: [...p.preferences].reverse() })));
    expect(reversed.items.map((i) => i.key)).toEqual(forward.items.map((i) => i.key));
    expect(forward.items.map((i) => `${i.projectName}/${i.entryName}/${i.platform}`)).toEqual([`Alfa proje/Alfa/${WIN}`, `Alfa proje/Çay/${WIN}`, `Zeta/a/${MAC}`, `Zeta/b/${WIN}`]);
  });

  it("derives the same answer twice (viewing changes nothing) and never modifies its inputs", () => {
    const p = deepFreeze(ready(projects()));
    const i = deepFreeze(inv(req("not_scanned"), req("absent_current", { entryId: "b", projectAlias: "zz" })));
    const first = deriveReview(p, i, NOW);
    const second = deriveReview(p, i, NOW);
    expect(second).toEqual(first);
    expect(first.items.length).toBeGreaterThan(0);
  });
});

describe("the reminder date", () => {
  const due = (remindAt: string | null, now = NOW) => one(wish("alfa", { decision: "postponed", remindAt }), inv(), now);
  const isDue = (remindAt: string | null, now = NOW) => due(remindAt, now).items.some((i) => i.reasons.some((r) => r.code === "reminder_due"));

  it("exact boundary: now == remindAt is due; one second earlier is not", () => {
    expect(isDue("2026-10-24T12:00:00Z")).toBe(true);
    expect(isDue("2026-10-24T12:00:01Z")).toBe(false);
    expect(isDue("2026-10-24T12:00:00Z", NOW - 1)).toBe(false);
    expect(isDue("2026-10-24T11:59:59Z")).toBe(true);
  });

  it("future and past", () => {
    expect(isDue("2099-01-01T00:00:00Z")).toBe(false);
    expect(isDue("2001-01-01T00:00:00Z")).toBe(true);
  });

  it("time-zone offsets are applied, not ignored", () => {
    expect(isDue("2026-10-24T14:00:00+02:00")).toBe(true); // == 12:00Z, the boundary
    expect(isDue("2026-10-24T14:00:01+02:00")).toBe(false);
    expect(isDue("2026-10-24T13:00:00+02:00")).toBe(true); // 11:00Z
    expect(isDue("2026-10-24T13:00:00-02:00")).toBe(false); // 15:00Z
    expect(isDue("2026-10-24T07:00:00-05:00")).toBe(true); // == 12:00Z
    expect(isDue("2026-10-24T17:30:00+05:30")).toBe(true); // == 12:00Z
    expect(isDue("2026-10-24T12:00:00.250Z", NOW - 1)).toBe(false);
    expect(isDue("2026-10-24T12:00:00.250Z", NOW + 250)).toBe(true);
  });

  it.each(["", "yarın", "2026-10-24", "2026-10-24T12:00:00", "2026-02-30T00:00:00Z", "2026-13-01T00:00:00Z", "2026-10-24T25:00:00Z", "2026-10-24T12:60:00Z", "2026-10-24 12:00:00Z", "2026-10-24T12:00:00+25:00", "1792843200"])(
    "malformed %j is not 'due' and is not silently dropped: the decision is listed with an unreadable-reminder doubt", (bad) => {
      const q = due(bad);
      expect(q.items).toEqual([]);
      expect(q.uncertain).toHaveLength(1);
      expect(q.uncertain[0].uncertain).toEqual(["reminder_unreadable"]);
    });

  it("a postponed decision without a reminder, and a reminder on a decision that is not postponed, are not due", () => {
    expect(due(null).items).toEqual([]);
    expect(due(null).uncertain).toEqual([]);
    const accepted = one(wish("alfa", { decision: "accepted", remindAt: "2001-01-01T00:00:00Z" }), inv(req("present_current")));
    expect(accepted.items).toEqual([]);
    expect(accepted.uncertain).toEqual([]);
  });

  it("parseRfc3339 returns exact epoch milliseconds, or null", () => {
    expect(parseRfc3339("1970-01-01T00:00:00Z")).toBe(0);
    expect(parseRfc3339("2026-10-24T12:00:00Z")).toBe(NOW);
    expect(parseRfc3339("2026-10-24T12:00:00z")).toBe(NOW);
    expect(parseRfc3339("2026-10-24t12:00:00Z")).toBe(NOW);
    expect(parseRfc3339("2024-02-29T00:00:00Z")).toBe(Date.UTC(2024, 1, 29));
    expect(parseRfc3339("2025-02-29T00:00:00Z")).toBeNull();
    expect(parseRfc3339("2026-10-24T24:00:00Z")).toBeNull();
    expect(parseRfc3339("2026-10-24T23:59:60Z")).toBeNull(); // leap seconds are not accepted (stated, not guessed)
  });
});

describe("evidence that is missing, ambiguous, unrecognised, pending or failed", () => {
  const accepted = () => [project("a", [wish("alfa")])];

  it("an accepted wish with no matching backend row is UNCERTAIN - not 'not scanned' - and listed separately", () => {
    const q = derive(accepted(), inv());
    expect(q.items).toEqual([]);
    expect(q.uncertain).toHaveLength(1);
    expect(q.uncertain[0].uncertain).toEqual(["no_row"]);
    expect(q.uncertain[0].reasons).toEqual([]);
    expect(q.uncertain[0].observation).toBeNull();
  });

  it("two contradictory rows and an unknown status are uncertain too", () => {
    expect(derive(accepted(), inv(req("present_current"), req("absent_current"))).uncertain[0].uncertain).toEqual(["ambiguous"]);
    for (const status of ["installed", "verified", "", "PRESENT_CURRENT"]) {
      const q = derive(accepted(), inv(req(status as RequestStatus)));
      expect(q.items, status).toEqual([]);
      expect(q.uncertain[0].uncertain, status).toEqual(["unrecognised_status"]);
      expect(q.uncertain[0].observation, status).toBeNull();
    }
  });

  it("an uncertain finding next to a real reason stays on the same item", () => {
    const q = derive([project("a", [wish("alfa", { materialChange: true })])], inv());
    expect(q.items).toHaveLength(1);
    expect(q.items[0].uncertain).toEqual(["no_row"]);
    expect(q.uncertain).toEqual([]);
  });

  it("inventory still loading: no evidence reasons, no per-row doubts, the queue is NOT complete, and the unread count is stated", () => {
    const q = deriveReview(ready(accepted()), { data: null, error: null, loading: true }, NOW);
    expect(q.evidence).toBe("loading");
    expect(q.complete).toBe(false);
    expect(q.evidenceUnavailableFor).toBe(1);
    expect(q.items).toEqual([]);
    expect(q.uncertain).toEqual([]);
    expect(reviewHeadline(q)).toContain("En az 0");
    expect(reviewHeadline(q)).not.toContain("incelenmesi gereken öğe yok");
  });

  it("inventory failed on the first read: same, and the headline never claims there is nothing to review", () => {
    const q = deriveReview(ready([project("a", [wish("alfa", { materialChange: true }), wish("beta")])]), { data: null, error: ERR, loading: false }, NOW);
    expect(q.evidence).toBe("failed");
    expect(q.complete).toBe(false);
    expect(q.evidenceUnavailableFor).toBe(2);
    expect(q.items).toHaveLength(1); // the decision-based reason does not depend on the inventory
    expect(reviewHeadline(q)).toMatch(/^En az 1 /);
  });

  it("failed refresh after a good read keeps the earlier classification but the queue is no longer complete", () => {
    const q = deriveReview(ready(accepted()), { data: { ...emptyInventory(), requests: [req("previously_present")] }, error: ERR, loading: false }, NOW);
    expect(q.evidence).toBe("old");
    expect(q.complete).toBe(false);
    expect(q.items[0].reasons).toEqual([{ code: "evidence", status: "previously_present" }]);
    expect(reviewHeadline(q)).toMatch(/^En az 1 /);
  });

  it("projects loading / failed / old", () => {
    const loading = deriveReview({ data: null, error: null, loading: true }, inv(), NOW);
    expect(loading.projects).toBe("loading");
    expect(loading.complete).toBe(false);
    expect(reviewHeadline(loading)).toBe("Kararlar okunuyor…");
    const failed = deriveReview({ data: null, error: ERR, loading: false }, inv(), NOW);
    expect(failed.projects).toBe("failed");
    expect(failed.items).toEqual([]);
    expect(reviewHeadline(failed)).toContain("bilinmiyor");
    expect(reviewHeadline(failed)).not.toContain("incelenmesi gereken öğe yok");
    const old = deriveReview({ data: [project("a", [wish("alfa", { materialChange: true })])], error: ERR, loading: false }, inv(), NOW);
    expect(old.projects).toBe("old");
    expect(old.items).toHaveLength(1);
    expect(old.complete).toBe(false);
  });

  it("a finished, clean read is the ONLY thing that may say 'nothing to review' - and it says what that does not mean", () => {
    const q = derive(accepted(), inv(req("present_current")));
    expect(q.complete).toBe(true);
    const text = reviewHeadline(q);
    expect(text).toContain("incelenmesi gereken öğe yok");
    expect(text).toContain("anlamına gelmez");
    expect(reviewHeadline(derive([project("a", [wish("alfa", { materialChange: true })])]))).toBe("1 öğe incelenmeyi bekliyor.");
  });
});

describe("example projects", () => {
  it("decisions of an example (synthetic) project are listed but marked as invented data", () => {
    const example = { ...project("a", [wish("alfa", { materialChange: true })]), kind: "synthetic" as const };
    const real = project("b", [wish("alfa", { materialChange: true })]);
    const q = derive([example, real]);
    expect(q.items.map((i) => [i.alias, i.synthetic])).toEqual([["a", true], ["b", false]]);
  });
});

describe("targets the matrix cannot show", () => {
  it("an operating system or agent surface this version does not know is listed but not openable, and nothing is substituted", () => {
    const q = derive([project("a", [wish("alfa", { platformTarget: "linux-x64", materialChange: true }), wish("beta", { agentSurface: "vim-plugin", materialChange: true }), wish("gama", { materialChange: true })])]);
    expect(q.items.map((i) => [i.entryId, i.openable])).toEqual([["alfa", false], ["beta", false], ["gama", true]]);
    expect(q.items[0].platform).toBe("linux-x64");
    expect(q.items[1].surface).toBe("vim-plugin");
  });
});

describe("filtering", () => {
  const q = derive([
    project("a", [wish("alfa", { materialChange: true }), wish("beta", { decision: "postponed", remindAt: "2001-01-01T00:00:00Z" })]),
    project("b", [wish("alfa", { entryRemoved: true })]),
  ]);

  it("by project, by reason, and by both - without changing the underlying queue", () => {
    expect(filterItems(q.items, "all", "all")).toHaveLength(3);
    expect(filterItems(q.items, "a", "all").map((i) => i.entryId)).toEqual(["alfa", "beta"]);
    expect(filterItems(q.items, "all", "entry_removed").map((i) => i.alias)).toEqual(["b"]);
    expect(filterItems(q.items, "a", "entry_removed")).toEqual([]);
    expect(filterItems(q.items, "all", "evidence")).toEqual([]);
    expect(q.items).toHaveLength(3);
  });
});

describe("P05-RT-01: a read that is still running is never final", () => {
  const NONE = "incelenmesi gereken öğe yok";
  const clean = () => [project("a", [wish("alfa")])]; // an accepted wish with a present_current row: nothing to review
  const cleanInv = () => inv(req("present_current"));
  const pending = <T,>(data: T) => ({ data, error: null, loading: true });

  it("projects refreshing (previous good data retained): not complete, no 'nothing to review', marked provisional", () => {
    const q = deriveReview(pending(clean()), cleanInv(), NOW);
    expect(q.projects).toBe("refreshing");
    expect(q.evidence).toBe("ready");
    expect(q.complete).toBe(false);
    expect(reviewHeadline(q)).not.toContain(NONE);
    expect(reviewHeadline(q)).toBe("En az 0 öğe incelenmeyi bekliyor (okuma sürüyor; sonuç kesin değil).");
  });

  it("inventory refreshing (previous good data retained): same", () => {
    const q = deriveReview(ready(clean()), pending(cleanInv().data), NOW);
    expect(q.projects).toBe("ready");
    expect(q.evidence).toBe("refreshing");
    expect(q.complete).toBe(false);
    expect(reviewHeadline(q)).not.toContain(NONE);
    expect(reviewHeadline(q)).toBe("En az 0 öğe incelenmeyi bekliyor (okuma sürüyor; sonuç kesin değil).");
  });

  it("both refreshing: same", () => {
    const q = deriveReview(pending(clean()), pending(cleanInv().data), NOW);
    expect([q.projects, q.evidence, q.complete]).toEqual(["refreshing", "refreshing", false]);
    expect(reviewHeadline(q)).not.toContain(NONE);
  });

  it("retained NON-empty data is still shown, but the count is 'at least' and marked not final", () => {
    const withReason = [project("a", [wish("alfa", { materialChange: true })])];
    for (const [p, i] of [[pending(withReason), cleanInv()], [ready(withReason), pending(cleanInv().data)], [pending(withReason), pending(cleanInv().data)]] as const) {
      const q = deriveReview(p, i, NOW);
      expect(q.items).toHaveLength(1);
      expect(q.complete).toBe(false);
      expect(reviewHeadline(q)).toBe("En az 1 öğe incelenmeyi bekliyor (okuma sürüyor; sonuç kesin değil).");
    }
  });

  it("a refresh that already failed is 'old', not 'refreshing', even while a retry runs; a first-read failure and a first read in progress keep their own phases", () => {
    expect(deriveReview({ data: clean(), error: ERR, loading: true }, cleanInv(), NOW).projects).toBe("old");
    expect(deriveReview(ready(clean()), { data: cleanInv().data, error: ERR, loading: true }, NOW).evidence).toBe("old");
    expect(deriveReview({ data: null, error: ERR, loading: true }, cleanInv(), NOW).projects).toBe("failed");
    expect(deriveReview({ data: null, error: null, loading: true }, cleanInv(), NOW).projects).toBe("loading");
    expect(deriveReview(ready(clean()), { data: null, error: ERR, loading: true }, NOW).evidence).toBe("failed");
  });

  it("a complete answer needs BOTH sources: data present, no error, not loading - checked over every combination", () => {
    const states = [
      { data: null, error: null, loading: false }, { data: null, error: null, loading: true },
      { data: null, error: ERR, loading: false }, { data: null, error: ERR, loading: true },
      { data: "d", error: null, loading: false }, { data: "d", error: null, loading: true },
      { data: "d", error: ERR, loading: false }, { data: "d", error: ERR, loading: true },
    ] as const;
    const finished = (s: (typeof states)[number]) => s.data !== null && s.error === null && s.loading === false;
    for (const a of states) {
      for (const b of states) {
        const q = deriveReview({ ...a, data: a.data === null ? null : clean() }, { ...b, data: b.data === null ? null : cleanInv().data }, NOW);
        expect(q.complete, JSON.stringify([a, b])).toBe(finished(a) && finished(b));
        if (!q.complete) expect(reviewHeadline(q), JSON.stringify([a, b])).not.toContain(NONE);
      }
    }
  });

  it("only a finished, clean pair of reads says 'nothing to review' (the control)", () => {
    const q = deriveReview(ready(clean()), cleanInv(), NOW);
    expect(q.complete).toBe(true);
    expect(reviewHeadline(q)).toContain(NONE);
  });
});
