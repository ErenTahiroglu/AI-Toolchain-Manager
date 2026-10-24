// PRODUCT-06 - the pure part of the evidence-origin display: what is accepted from the wire and what is refused. No React, no IPC.
import { describe, expect, it } from "vitest";
import { describeOrigin, originTimeText, PROJECT_RELATIONS, SURFACE_RELATIONS } from "./origin";
import { formatTime } from "./labels";

const good = { projectRelation: "computer_wide", surfaceRelation: "exact_surface", observedAt: "2026-10-24T09:00:00Z", ageHours: 3 };

describe("describeOrigin takes only a complete, known descriptor", () => {
  it("accepts every combination of the two known relations", () => {
    for (const projectRelation of PROJECT_RELATIONS) {
      for (const surfaceRelation of SURFACE_RELATIONS) {
        expect(describeOrigin({ ...good, projectRelation, surfaceRelation })).toEqual({ kind: "described", project: projectRelation, surface: surfaceRelation, observedAt: good.observedAt, ageHours: 3 });
      }
    }
    expect([...PROJECT_RELATIONS].sort()).toEqual(["computer_wide", "project_specific"]);
    expect([...SURFACE_RELATIONS].sort()).toEqual(["exact_surface", "local_cli_fallback"]);
  });

  it("null, missing, non-objects and unknown or absent relations are 'unavailable' - never a guessed scope", () => {
    for (const bad of [null, undefined, "computer_wide", 7, true, [], {}, { ...good, projectRelation: undefined }, { ...good, surfaceRelation: undefined },
      { ...good, projectRelation: "global" }, { ...good, surfaceRelation: "claude-code-terminal" }, { ...good, projectRelation: "" }, { ...good, projectRelation: null },
      { ...good, projectRelation: "PROJECT_SPECIFIC" }, { ...good, surfaceRelation: "exact" }]) {
      expect(describeOrigin(bad), JSON.stringify(bad)).toEqual({ kind: "unavailable" });
    }
  });

  it("a timestamp is kept only when it is valid RFC 3339, and the age only with a valid timestamp", () => {
    for (const stamp of ["", "yesterday", "2026-10-24", "2026-10-24T09:00:00", "2026-13-01T00:00:00Z", "2026-02-30T00:00:00Z", 12345, null, undefined]) {
      const v = describeOrigin({ ...good, observedAt: stamp });
      expect(v, String(stamp)).toMatchObject({ kind: "described", observedAt: null, ageHours: null });
    }
    for (const stamp of ["2026-10-24T09:00:00Z", "2026-10-24T09:00:00+03:00", "2026-10-24T09:00:00.5-02:00"]) {
      expect(describeOrigin({ ...good, observedAt: stamp })).toMatchObject({ observedAt: stamp, ageHours: 3 });
    }
  });

  it("an age that is negative, fractional, not finite or not a number is dropped, not corrected", () => {
    for (const age of [-1, -0.5, 1.5, Number.NaN, Number.POSITIVE_INFINITY, "3", null, undefined, {}]) {
      expect(describeOrigin({ ...good, ageHours: age }), String(age)).toMatchObject({ kind: "described", observedAt: good.observedAt, ageHours: null });
    }
    expect(describeOrigin({ ...good, ageHours: 0 })).toMatchObject({ ageHours: 0 });
  });

  it("extra members on the wire (a path, raw evidence, a version) are not carried into the view", () => {
    const v = describeOrigin({ ...good, location: "C:\\Users\\x\\secret", evidence: "RAW", version: "1.2.3", scope: "global" });
    expect(Object.keys(v).sort()).toEqual(["ageHours", "kind", "observedAt", "project", "surface"]);
    expect(JSON.stringify(v)).not.toMatch(/secret|RAW|1\.2\.3|global/);
  });
});

describe("originTimeText", () => {
  it("states the time, the age at the moment of reading, and that it is not a live check", () => {
    const text = originTimeText({ kind: "described", project: "computer_wide", surface: "exact_surface", observedAt: good.observedAt, ageHours: 3 });
    expect(text).toContain(formatTime(good.observedAt));
    expect(text).toContain("envanter okunduğunda yaklaşık 3 saat önce");
    expect(text).toContain("canlı bir doğrulama değildir");
  });
  it("shows no age when none is available, and nothing at all without a valid timestamp", () => {
    const base = { kind: "described", project: "computer_wide", surface: "exact_surface" } as const;
    expect(originTimeText({ ...base, observedAt: good.observedAt, ageHours: null })).not.toMatch(/saat önce/);
    expect(originTimeText({ ...base, observedAt: null, ageHours: null })).toBeNull();
  });
});
