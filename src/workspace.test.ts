// PRODUCT-01 - the pure logic of the Project Tool Workspace. No rendering, no backend.
import { describe, expect, it } from "vitest";
import rustProjectsSource from "../src-tauri/src/projects.rs?raw";
import { makeEntry, pref } from "./test/backend";
import type { CatalogEntry, ProjectView } from "./types";
import {
  AFFIRMATIVE_INSTALL_STATES, AFFIRMATIVE_VERIFICATION_STATES, DECISION_VALUES, DEFAULT_TARGET, NO_FILTER, TARGET_PLATFORMS, TARGET_SURFACES, acceptBlockReason, buildRows,
  classifyInstall, classifyVerification, compatClass, compatStateFor, contextKey, evidenceFor, filterRows, isFiltering, normalize, recordedInstallText,
  recordedVerificationText, resolveTarget, sameKey, summarize, targetCounts,
} from "./workspace";

const ev = (platformTarget: string, agentSurface: string, compatibilityState: string) => ({ platformTarget, agentSurface, compatibilityState, limitation: `${compatibilityState} @ ${platformTarget}/${agentSurface}` });
const entry = (id: string, over: Partial<CatalogEntry> = {}) => makeEntry(id, { displayName: id, ...over });
const project = (alias: string, preferences: ProjectView["preferences"] = []): ProjectView => ({ projectId: `id-${alias}`, alias, displayName: alias, kind: "reference", localNote: "", createdAt: "x", preferences });

describe("the target lists are exactly the values the Rust side accepts", () => {
  const list = (name: string) => {
    const m = rustProjectsSource.match(new RegExp(`pub const ${name}: &\\[&str\\] = &\\[([^\\]]*)\\]`));
    expect(m, name).not.toBeNull();
    return [...m![1].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
  };
  it("surfaces, platforms and decisions equal src-tauri/src/projects.rs", () => {
    expect([...TARGET_SURFACES]).toEqual(list("SURFACES"));
    expect([...TARGET_PLATFORMS]).toEqual(list("PLATFORMS"));
    expect([...DECISION_VALUES].sort()).toEqual(list("DECISIONS").sort());
  });
  it("the advisory acceptance rules mirror the ones set_preference enforces", () => {
    expect(rustProjectsSource).toContain('"blocked-until-scoped-authorization"');
    expect(rustProjectsSource).toContain('"ENTRY_DISABLED"');
    expect(rustProjectsSource).toContain('matches!(compat.as_str(), "vendor-declared" | "independently-tested")');
  });
});

describe("compatibility", () => {
  it("only an explicit declaration or test result counts; anything else is never support", () => {
    expect(compatClass("vendor-declared")).toBe("declared");
    expect(compatClass("independently-tested")).toBe("declared");
    expect(compatClass("unsupported")).toBe("unsupported");
    for (const odd of ["unknown", "", "Vendor-Declared", "supported", "yes", "true", "verified", "installed"]) expect(compatClass(odd)).toBe("unknown");
  });
  it("is read for exactly the platform and the surface; no record means unknown", () => {
    const e = entry("a", { record: { targetEvidence: [ev("windows-x64", "local-cli", "vendor-declared"), ev("macos-arm64", "local-cli", "unsupported")] } });
    expect(compatStateFor(e, "windows-x64", "local-cli")).toBe("vendor-declared");
    expect(compatStateFor(e, "macos-arm64", "local-cli")).toBe("unsupported");
    expect(compatStateFor(e, "windows-x64", "claude-code-terminal")).toBe("unknown");
    expect(evidenceFor(e, "windows-x64", "antigravity-ide")).toBeNull();
    expect(compatStateFor(entry("b", { record: {} }), "windows-x64", "local-cli")).toBe("unknown");
  });
});

describe("acceptBlockReason follows the backend order: removed, execution policy, catalog switch, compatibility", () => {
  const ok = entry("a");
  it("allows a declared, enabled, manual-only entry", () => expect(acceptBlockReason(ok, "vendor-declared")).toBeNull());
  it("blocks unknown / unsupported compatibility", () => {
    expect(acceptBlockReason(ok, "unknown")).toMatch(/uyumluluk bilinmiyor veya desteklenmiyor/);
    expect(acceptBlockReason(ok, "unsupported")).not.toBeNull();
  });
  it("blocks a catalog-disabled entry and tells where to switch it on", () => expect(acceptBlockReason(entry("a", { enabled: false }), "vendor-declared")).toMatch(/Katalogda kapalı/));
  it("blocks the execution-policy entry first", () => expect(acceptBlockReason(entry("s", { executionPolicy: "blocked-until-scoped-authorization", enabled: false }), "unknown")).toMatch(/yazılı yetki/));
  it("blocks a removed entry first of all", () => expect(acceptBlockReason(entry("r", { removedAt: "2026-01-01T00:00:00Z" }), "vendor-declared")).toMatch(/kaldırılmış/));
});

describe("buildRows is scoped to one project and one target", () => {
  const entries = [entry("alfa"), entry("beta"), entry("eski", { removedAt: "2026-10-18T00:00:00Z" }), entry("eski-karar", { removedAt: "2026-10-18T00:00:00Z" })];
  const mk = (over: Parameters<typeof pref>[0]) => pref({ agentSurface: "claude-code-terminal", platformTarget: "windows-x64", ...over });

  it("takes only this project's wish for exactly this (entry, surface, platform)", () => {
    const a = project("a", [mk({ entryId: "alfa", decision: "accepted" }), mk({ entryId: "alfa", platformTarget: "macos-arm64", decision: "rejected" }), mk({ entryId: "alfa", agentSurface: "local-cli", decision: "disabled" })]);
    const b = project("b", []);
    const ra = buildRows(entries, a, "windows-x64", "claude-code-terminal").rows.find((r) => r.entry.entryId === "alfa")!;
    expect(ra.pref?.decision).toBe("accepted");
    expect(buildRows(entries, a, "macos-arm64", "claude-code-terminal").rows.find((r) => r.entry.entryId === "alfa")!.pref?.decision).toBe("rejected");
    expect(buildRows(entries, a, "windows-x64", "local-cli").rows.find((r) => r.entry.entryId === "alfa")!.pref?.decision).toBe("disabled");
    expect(buildRows(entries, a, "macos-arm64", "local-cli").rows.find((r) => r.entry.entryId === "alfa")!.pref).toBeNull();
    expect(buildRows(entries, b, "windows-x64", "claude-code-terminal").rows.every((r) => r.pref === null)).toBe(true);
  });

  it("shows a removed entry only when this project holds a wish for this target, and labels it removed", () => {
    const p = project("a", [mk({ entryId: "eski-karar", decision: "rejected" })]);
    const { rows } = buildRows(entries, p, "windows-x64", "claude-code-terminal");
    expect(rows.map((r) => r.entry.entryId)).toEqual(["alfa", "beta", "eski-karar"]);
    const removed = rows.find((r) => r.entry.entryId === "eski-karar")!;
    expect(removed.removed).toBe(true);
    expect(removed.acceptBlock).toMatch(/kaldırılmış/);
    expect(buildRows(entries, p, "macos-arm64", "claude-code-terminal").rows.map((r) => r.entry.entryId)).toEqual(["alfa", "beta"]);
  });

  it("reports wishes whose entry is not in the list instead of hiding them", () => {
    const p = project("a", [mk({ entryId: "yok", decision: "rejected" })]);
    expect(buildRows(entries, p, "windows-x64", "claude-code-terminal").orphanPrefs.map((x) => x.entryId)).toEqual(["yok"]);
  });

  it("orders deterministically: name (Turkish letters folded), then id, removed last", () => {
    const es = [entry("z", { displayName: "İzmir Aracı" }), entry("y", { displayName: "ızgara" }), entry("x", { displayName: "Alfa" }), entry("w", { displayName: "alfa" }), entry("v", { displayName: "Eski", removedAt: "t" })];
    const p = project("a", [mk({ entryId: "v", decision: "rejected" })]);
    const first = buildRows(es, p, "windows-x64", "claude-code-terminal").rows.map((r) => r.entry.entryId);
    expect(first).toEqual(["w", "x", "y", "z", "v"]);
    expect(buildRows([...es].reverse(), p, "windows-x64", "claude-code-terminal").rows.map((r) => r.entry.entryId)).toEqual(first);
  });
});

describe("filtering is stable, combinable and Turkish-insensitive", () => {
  const entries = [
    entry("istanbul", { displayName: "İstanbul Aracı", purpose: "Şehir haritası", kind: "executable_tool", record: { targetEvidence: [ev("windows-x64", "claude-code-terminal", "vendor-declared")] } }),
    entry("eklenti", { displayName: "Eklenti", purpose: "Ajan eklentisi", kind: "agent_plugin", record: { targetEvidence: [ev("windows-x64", "claude-code-terminal", "unsupported")] } }),
    entry("bilinmeyen", { displayName: "Bilinmeyen", purpose: "Çevrimdışı kayıt", kind: "skill_collection" }),
  ];
  const p = project("a", [pref({ entryId: "istanbul", agentSurface: "claude-code-terminal", platformTarget: "windows-x64", decision: "accepted" })]);
  const rows = buildRows(entries, p, "windows-x64", "claude-code-terminal").rows;
  const ids = (f: Partial<typeof NO_FILTER>) => filterRows(rows, { ...NO_FILTER, ...f }).map((r) => r.entry.entryId);

  it("no filter keeps every row in order", () => expect(ids({})).toEqual(rows.map((r) => r.entry.entryId)));
  it("search ignores case, accents and the Turkish dotted / dotless i", () => {
    expect(ids({ query: "istanbul" })).toEqual(["istanbul"]);
    expect(ids({ query: "İSTANBUL" })).toEqual(["istanbul"]);
    expect(ids({ query: "sehir haritasi" })).toEqual(["istanbul"]);
    expect(ids({ query: "CEVRIMDISI" })).toEqual(["bilinmeyen"]);
    expect(ids({ query: "istanbul zzz" })).toEqual([]);
  });
  it("kind, decision and compatibility filters combine with AND", () => {
    expect(ids({ kind: "agent_plugin" })).toEqual(["eklenti"]);
    expect(ids({ decision: "accepted" })).toEqual(["istanbul"]);
    expect(ids({ decision: "undecided" })).toEqual(["bilinmeyen", "eklenti"]);
    expect(ids({ decision: "rejected" })).toEqual([]);
    expect(ids({ compat: "declared" })).toEqual(["istanbul"]);
    expect(ids({ compat: "unsupported" })).toEqual(["eklenti"]);
    expect(ids({ compat: "unknown" })).toEqual(["bilinmeyen"]);
    expect(ids({ decision: "undecided", compat: "unknown", kind: "skill_collection" })).toEqual(["bilinmeyen"]);
    expect(ids({ decision: "accepted", compat: "unknown" })).toEqual([]);
  });
  it("is repeatable and never changes the relative order", () => {
    expect(ids({ query: "e" })).toEqual(ids({ query: "e" }));
    const all = ids({});
    expect(ids({ query: "e" })).toEqual(all.filter((x) => ids({ query: "e" }).includes(x)));
  });
  it("normalize folds the Turkish letters", () => expect(normalize("ÇĞİÖŞÜ çğıöşü Iı")).toBe("cgiosu cgiosu ii"));
  it("isFiltering knows an untouched filter", () => {
    expect(isFiltering(NO_FILTER)).toBe(false);
    expect(isFiltering({ ...NO_FILTER, query: "  " })).toBe(false);
    expect(isFiltering({ ...NO_FILTER, kind: "agent_plugin" })).toBe(true);
  });
});

describe("summarize keeps the wish and the application's own record apart", () => {
  it("counts wishes by decision and never turns a wish into an installed or verified record", () => {
    const entries = [entry("a"), entry("b"), entry("c"), entry("d")];
    const key = { agentSurface: "local-cli", platformTarget: "windows-x64" };
    const p = project("p", [
      pref({ ...key, entryId: "a", decision: "accepted" }),
      pref({ ...key, entryId: "b", decision: "disabled", materialChange: true }),
      pref({ ...key, entryId: "c", decision: "postponed" }),
    ]);
    const s = summarize(buildRows(entries, p, "windows-x64", "local-cli").rows);
    expect(s).toMatchObject({ total: 4, accepted: 1, disabled: 1, postponed: 1, rejected: 0, undecided: 1, materialChange: 1, recordedInstalled: 0, recordedVerified: 0 });
  });
  it("counts the known 'none' values as not installed / not verified, and never as success", () => {
    const s = summarize(buildRows([entry("a"), entry("b")], project("p"), "windows-x64", "local-cli").rows);
    expect(s).toMatchObject({ recordedInstalled: 0, recordedVerified: 0, recordedNotInstalled: 2, recordedNotVerified: 2, recordedUnrecognised: 0 });
  });
  it("an unfamiliar, corrupt or future status string is 'unrecognised' - never installed, never verified", () => {
    for (const odd of ["installed", "managed_installed", "installed_verified", "INSTALLED", "Not_Managed", " not_managed", "not_managed ", "ok", "true", "1", "", "passed", "verified", "active", "enabled"]) {
      const e = entry("a", { installState: odd, verificationState: odd });
      const s = summarize(buildRows([e], project("p"), "windows-x64", "local-cli").rows);
      expect(s, JSON.stringify(odd)).toMatchObject({ recordedInstalled: 0, recordedVerified: 0, recordedNotInstalled: 0, recordedNotVerified: 0, recordedUnrecognised: 1 });
      expect(recordedInstallText(odd), odd).toMatch(/Tanınmayan durum .* başarı sayılmaz/);
      expect(recordedVerificationText(odd), odd).toMatch(/Tanınmayan durum .* başarı sayılmaz/);
      expect(`${recordedInstallText(odd)} ${recordedVerificationText(odd)}`).not.toMatch(/Kurulu olarak|Doğrulanmış olarak|Kurulu ve doğrulandı/);
    }
  });
  it("the wish row's own record wins over the catalog row's, and an unfamiliar value there is also unrecognised", () => {
    const key = { agentSurface: "local-cli", platformTarget: "windows-x64" };
    const p = project("p", [pref({ ...key, entryId: "a", decision: "accepted", actualInstallState: "installed", actualVerificationState: "unknown" })]);
    const s = summarize(buildRows([entry("a")], p, "windows-x64", "local-cli").rows);
    expect(s).toMatchObject({ recordedInstalled: 0, recordedNotVerified: 1, recordedUnrecognised: 1 });
  });
  it("no value is declared affirmative today (the database only allows not_managed / unknown), so none can be counted as success", () => {
    expect(AFFIRMATIVE_INSTALL_STATES).toEqual([]);
    expect(AFFIRMATIVE_VERIFICATION_STATES).toEqual([]);
    expect(classifyInstall("not_managed")).toBe("none");
    expect(classifyVerification("unknown")).toBe("none");
  });
  it("very long state strings are shortened in the text", () => {
    expect(recordedInstallText("x".repeat(500)).length).toBeLessThan(120);
  });
  it("counts a decision value it does not know separately instead of folding it into another", () => {
    const p = project("p", [pref({ agentSurface: "local-cli", platformTarget: "windows-x64", entryId: "a", decision: "installed" as never })]);
    const s = summarize(buildRows([entry("a")], p, "windows-x64", "local-cli").rows);
    expect(s).toMatchObject({ unrecognised: 1, accepted: 0, undecided: 0 });
  });
});

describe("keys", () => {
  it("contextKey differs for every part", () => {
    const base = contextKey("a", "windows-x64", "local-cli");
    expect(new Set([base, contextKey("b", "windows-x64", "local-cli"), contextKey("a", "macos-arm64", "local-cli"), contextKey("a", "windows-x64", "antigravity-ide")]).size).toBe(4);
  });
  it("sameKey compares entry, surface and platform", () => {
    const p = pref({ entryId: "a", agentSurface: "local-cli", platformTarget: "windows-x64" });
    expect(sameKey(p, { entryId: "a", agentSurface: "local-cli", platformTarget: "windows-x64" })).toBe(true);
    expect(sameKey(p, { entryId: "b", agentSurface: "local-cli", platformTarget: "windows-x64" })).toBe(false);
    expect(sameKey(p, { entryId: "a", agentSurface: "claude-code-terminal", platformTarget: "windows-x64" })).toBe(false);
    expect(sameKey(p, { entryId: "a", agentSurface: "local-cli", platformTarget: "macos-arm64" })).toBe(false);
  });
});

describe("resolveTarget: a requested target is honoured when it exists and replaced - with a note - when it does not", () => {
  const list = [project("a"), project("b")];
  it("keeps a valid request exactly", () => {
    const r = resolveTarget({ alias: "b", platform: "macos-arm64", surface: "antigravity-ide" }, list);
    expect(r).toMatchObject({ platform: "macos-arm64", surface: "antigravity-ide", notes: [] });
    expect(r.project?.alias).toBe("b");
  });
  it("falls back to the first project and says the requested one is gone", () => {
    const r = resolveTarget({ alias: "silinmis", platform: "windows-x64", surface: "local-cli" }, list);
    expect(r.project?.alias).toBe("a");
    expect(r.notes.join(" ")).toMatch(/İstenen proje artık yok/);
  });
  it("an empty request alias means 'no preference' and produces no note", () => {
    expect(resolveTarget(DEFAULT_TARGET, list)).toMatchObject({ notes: [] });
    expect(resolveTarget(DEFAULT_TARGET, list).project?.alias).toBe("a");
  });
  it("replaces an unknown operating system or surface with the first known one, and says so", () => {
    const r = resolveTarget({ alias: "a", platform: "linux-x64", surface: "vscode" }, list);
    expect(r.platform).toBe(TARGET_PLATFORMS[0]);
    expect(r.surface).toBe(TARGET_SURFACES[0]);
    expect(r.notes).toHaveLength(2);
  });
  it("while the project list is still loading nothing is chosen and nothing is claimed", () => {
    expect(resolveTarget({ alias: "b", platform: "windows-x64", surface: "local-cli" }, null)).toMatchObject({ project: null, notes: [] });
  });
  it("an empty project list gives no project and no 'gone' note", () => {
    expect(resolveTarget({ alias: "b", platform: "windows-x64", surface: "local-cli" }, [])).toMatchObject({ project: null, notes: [] });
  });
});

describe("targetCounts: the read-only navigator", () => {
  const w = (over: Parameters<typeof pref>[0]) => pref({ agentSurface: "claude-code-terminal", platformTarget: "windows-x64", ...over });
  it("counts decisions per target in canonical order, including decisions on removed entries", () => {
    const p = project("a", [
      w({ entryId: "1" }), w({ entryId: "2", entryRemoved: true }), w({ entryId: "3", platformTarget: "macos-arm64", agentSurface: "local-cli" }), w({ entryId: "4", agentSurface: "local-cli" }),
    ]);
    const n = targetCounts(p);
    expect(n.total).toBe(4);
    expect(n.unreachable).toBe(0);
    expect(n.targets.map((t) => `${t.platform}/${t.surface}:${t.count}:${t.removed}`)).toEqual([
      "windows-x64/claude-code-terminal:2:1", "windows-x64/local-cli:1:0", "macos-arm64/local-cli:1:0",
    ]);
  });
  it("counts decisions on a target this build does not offer as unreachable instead of dropping them silently", () => {
    const n = targetCounts(project("a", [w({ entryId: "1" }), w({ entryId: "2", platformTarget: "linux-x64" }), w({ entryId: "3", agentSurface: "vscode" })]));
    expect(n).toMatchObject({ unreachable: 2, total: 3 });
    expect(n.targets).toHaveLength(1);
  });
  it("no decisions: nothing to navigate", () => expect(targetCounts(project("a"))).toEqual({ targets: [], unreachable: 0, total: 0 }));
});
