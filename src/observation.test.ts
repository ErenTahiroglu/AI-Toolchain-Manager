// PRODUCT-04 - pure tests of the exact-key join between a matrix row and the backend's Inventory.requests (src/observation.ts).
import { describe, expect, it } from "vitest";
import { REQUEST_STATUSES, isKnownRequestStatus, observationFor, requestMatches, type InventoryRead } from "./observation";
import { emptyInventory, makeEntry, makeRequest, pref } from "./test/backend";
import type { AppError, Inventory, PreferenceView, ProjectView } from "./types";
import { buildRows, type WorkspaceRow } from "./workspace";

const WIN = "windows-x64";
const MAC = "macos-arm64";
const CC = "claude-code-terminal";
const CLI = "local-cli";
const T = { alias: "ornek-web-sitesi", platform: WIN, surface: CC };

const project = (preferences: PreferenceView[]): ProjectView => ({
  projectId: "id", alias: T.alias, displayName: "Örnek", kind: "reference", localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences,
});
const wish = (decision: PreferenceView["decision"] | string, over: Partial<PreferenceView> = {}) =>
  pref({ entryId: "alfa", entryName: "alfa", agentSurface: CC, platformTarget: WIN, decision: decision as PreferenceView["decision"], ...over });
const rowOf = (preferences: PreferenceView[], removedAt: string | null = null): WorkspaceRow =>
  buildRows([makeEntry("alfa", { removedAt })], project(preferences), WIN, CC).rows[0];
const read = (over: Partial<InventoryRead> = {}): InventoryRead => ({ data: emptyInventory(), error: null, loading: false, ...over });
const withRequests = (...requests: Inventory["requests"]): InventoryRead => read({ data: { ...emptyInventory(), requests } });
const ERR: AppError = { code: "DB_BUSY", message: "Veritabanı meşgul." };

describe("the join key is project + tool + operating system + agent surface - all four", () => {
  const accepted = () => rowOf([wish("accepted")]);

  it("matches only the row whose four fields all agree", () => {
    const v = observationFor(accepted(), T, withRequests(makeRequest({ status: "present_current" })));
    expect(v).toMatchObject({ kind: "observed", status: "present_current" });
  });

  it.each([
    ["another project", { projectAlias: "benim-sitem" }],
    ["another tool", { entryId: "beta" }],
    ["another operating system", { platformTarget: MAC }],
    ["another agent surface", { agentSurface: CLI }],
  ])("%s never matches, even when the other three fields do", (_name, over) => {
    const v = observationFor(accepted(), T, withRequests(makeRequest({ status: "present_current", ...over })));
    expect(v.kind).toBe("no_row");
  });

  it("does not match by display name or project name either", () => {
    const v = observationFor(accepted(), T, withRequests(makeRequest({ entryId: "zzz", entryName: "alfa", projectAlias: "x", projectName: T.alias })));
    expect(v.kind).toBe("no_row");
  });

  it("picks the right one among many near-misses", () => {
    const right = makeRequest({ status: "absent_current" });
    const v = observationFor(accepted(), T, withRequests(
      makeRequest({ status: "present_current", projectAlias: "benim-sitem" }),
      makeRequest({ status: "present_current", platformTarget: MAC }),
      makeRequest({ status: "present_current", agentSurface: CLI }),
      right,
      makeRequest({ status: "present_current", entryId: "beta" }),
    ));
    expect(v).toMatchObject({ kind: "observed", status: "absent_current" });
    if (v.kind === "observed") expect(v.request).toBe(right);
  });

  it("requestMatches compares exactly (no case folding, no trimming)", () => {
    const r = makeRequest();
    expect(requestMatches(r, "alfa", T)).toBe(true);
    expect(requestMatches(r, "Alfa", T)).toBe(false);
    expect(requestMatches(r, "alfa", { ...T, alias: "ORNEK-WEB-SITESI" })).toBe(false);
    expect(requestMatches(r, "alfa", { ...T, platform: `${WIN} ` })).toBe(false);
  });

  it("two backend rows for one key are contradictory: none is shown", () => {
    const v = observationFor(accepted(), T, withRequests(makeRequest({ status: "present_current" }), makeRequest({ status: "absent_current" })));
    expect(v.kind).toBe("ambiguous");
  });
});

describe("every backend status is passed through unchanged", () => {
  it.each(REQUEST_STATUSES)("%s", (status) => {
    const request = makeRequest({ status });
    const v = observationFor(rowOf([wish("accepted")]), T, withRequests(request));
    expect(v).toMatchObject({ kind: "observed", status, old: false });
    if (v.kind === "observed") {
      expect(v.request).toBe(request); // not copied, not upgraded, not re-derived
      expect(v.request.note).toBe(request.note);
      expect(v.request.basis).toBe(request.basis);
    }
  });

  it("a status this version does not know is never a success", () => {
    for (const status of ["installed", "verified", "present", "", "PRESENT_CURRENT"]) {
      expect(isKnownRequestStatus(status), status).toBe(false);
      const v = observationFor(rowOf([wish("accepted")]), T, withRequests(makeRequest({ status: status as never })));
      expect(v.kind, status).toBe("unrecognised");
    }
  });
});

describe("only an accepted wish is compared", () => {
  const stale = withRequests(makeRequest({ status: "present_current" })); // an old read may still hold a row for a wish that has since changed

  it.each(["postponed", "rejected", "disabled", "something-new"])("%s: no comparison, even if the inventory holds a row for that key", (decision) => {
    expect(observationFor(rowOf([wish(decision)]), T, stale).kind).toBe("no_wish");
  });

  it("no wish at all: no comparison - which is not 'not scanned' and not 'absent'", () => {
    expect(observationFor(rowOf([]), T, stale).kind).toBe("no_wish");
    expect(observationFor(rowOf([]), T, read({ data: null, error: ERR })).kind).toBe("no_wish"); // true whatever the inventory says
  });

  it("a wish for another target is not this row's wish", () => {
    const row = buildRows([makeEntry("alfa")], project([wish("accepted", { platformTarget: MAC })]), WIN, CC).rows[0];
    expect(row.pref).toBeNull();
    expect(observationFor(row, T, stale).kind).toBe("no_wish");
  });

  it("a catalog entry that was removed gets no comparison and no invented observation, even if a row is present", () => {
    const row = rowOf([wish("accepted")], "2026-10-18T00:00:00Z");
    expect(row.removed).toBe(true);
    expect(observationFor(row, T, stale).kind).toBe("removed");
    expect(observationFor(row, T, read({ data: null, loading: true })).kind).toBe("removed");
  });
});

describe("loading, failure and age of the stored read", () => {
  const accepted = () => rowOf([wish("accepted")]);

  it("before any answer: loading - nothing is shown as an observation", () => {
    expect(observationFor(accepted(), T, read({ data: null, loading: true }))).toEqual({ kind: "loading" });
  });

  it("a failed first read is 'unreadable' with the error - never 'not scanned', never an empty result", () => {
    const v = observationFor(accepted(), T, read({ data: null, error: ERR }));
    expect(v).toEqual({ kind: "unreadable", error: ERR });
    expect(v.kind).not.toBe("no_row");
  });

  it("a failed refresh keeps the earlier answer but marks it old", () => {
    const v = observationFor(accepted(), T, { data: { ...emptyInventory(), requests: [makeRequest({ status: "present_current" })] }, error: ERR, loading: false });
    expect(v).toMatchObject({ kind: "observed", status: "present_current", old: true });
  });

  it("an accepted wish without a backend row is explained, not filled in, and says whether a re-read is running", () => {
    expect(observationFor(accepted(), T, read())).toEqual({ kind: "no_row", reading: false, old: false });
    expect(observationFor(accepted(), T, read({ loading: true }))).toEqual({ kind: "no_row", reading: true, old: false });
    expect(observationFor(accepted(), T, { data: emptyInventory(), error: ERR, loading: false })).toEqual({ kind: "no_row", reading: false, old: true });
  });

  it("an empty inventory (no scans, no requests) never yields an observation", () => {
    const v = observationFor(accepted(), T, read({ data: { ...emptyInventory(), lastScan: null, observations: [], requests: [] } }));
    expect(v.kind).toBe("no_row");
  });

  it("raw observations in the inventory are not a source: only Inventory.requests is used", () => {
    // the backend decides which observation counts for a wish; the UI never re-derives it from observations (that would be a second, weaker classifier)
    const data = { ...emptyInventory(), observations: [{ subject: "alfa", platformTarget: WIN, agentSurface: CC, presence: "present", current: true }] as never, requests: [] };
    expect(observationFor(accepted(), T, read({ data })).kind).toBe("no_row");
  });
});
