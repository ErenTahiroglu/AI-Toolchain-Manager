// PRODUCT-07 H - the pure validator and the words of the scoped MCP-configuration block, plus the additive origin method. No rendering, no backend.
import { describe, expect, it } from "vitest";
import originSource from "./origin.ts?raw";
import scopedSource from "./scopedDeclarations.ts?raw";
import noteSource from "./components/ScopedConfigNote.tsx?raw";
import { describeOrigin, METHOD_TEXT, ORIGIN_METHODS, originMethodText } from "./origin";
import {
  CONFIGURATION_TEXT, COVERAGE_TEXT, DECLARATION_CONFIGURATIONS, DECLARATION_FRESHNESS, DECLARATION_SCOPES, FRESHNESS_TEXT, NOT_IMPLEMENTED_TEXT, PRESENCE_LABEL, SCOPE_COVERAGE,
  SCOPE_NAME, SCOPED_DISCLAIMER, SCOPED_HEADING, SCOPED_NOT_READ_NOTE, SCOPED_SEPARATE, describeScopedDeclarations, notImplementedExtra, scopeLines,
} from "./scopedDeclarations";

const CC = "claude-code-terminal";
const T0 = "2026-10-23T09:00:00Z";
const NONE = { project: "no_record", local: "no_record", global: "no_record" };
const reading = (scope: string, over: Record<string, unknown> = {}) => ({ scope, configuration: "configured", freshness: "current", observedAt: T0, ageHours: 3, ...over });
const wire = (readings: unknown[], coverage: Record<string, string> = NONE, over: Record<string, unknown> = {}) => ({
  agentSurface: CC,
  declarationDetector: { kind: "claude_mcp", coverage: { ...coverage, ...Object.fromEntries((readings as { scope: string }[]).map((r) => [r.scope, "read"])) } },
  declarations: readings,
  ...over,
});
const described = (v: ReturnType<typeof describeScopedDeclarations>) => {
  expect(v.kind).toBe("described");
  return v as Extract<typeof v, { kind: "described" }>;
};

describe("describeScopedDeclarations takes only the exact documented shape", () => {
  it("accepts 0, 1, 2 and 3 readings in the fixed order project -> local -> global", () => {
    expect(described(describeScopedDeclarations(wire([]))).readings).toEqual([]);
    expect(described(describeScopedDeclarations(wire([reading("local")]))).readings.map((r) => r.scope)).toEqual(["local"]);
    expect(described(describeScopedDeclarations(wire([reading("project"), reading("global")]))).readings.map((r) => r.scope)).toEqual(["project", "global"]);
    const three = described(describeScopedDeclarations(wire([reading("project"), reading("local"), reading("global")])));
    expect(three.readings.map((r) => r.scope)).toEqual(["project", "local", "global"]);
    expect(three.coverage).toEqual({ project: "read", local: "read", global: "read" });
  });

  it("an older backend (no members) or an incomplete payload is unavailable and makes no coverage claim", () => {
    expect(describeScopedDeclarations({ agentSurface: CC }).kind).toBe("unavailable");
    expect(describeScopedDeclarations({ agentSurface: CC, declarationDetector: undefined, declarations: [] }).kind).toBe("unavailable");
    expect(describeScopedDeclarations({ agentSurface: CC, declarationDetector: { kind: "claude_mcp", coverage: NONE } }).kind).toBe("unavailable"); // readings missing
    expect(describeScopedDeclarations({ agentSurface: CC, declarations: [] }).kind).toBe("unavailable"); // detector missing
    for (const junk of [null, 0, "claude_mcp", [], true]) {
      expect(describeScopedDeclarations({ agentSurface: CC, declarationDetector: junk, declarations: [] }).kind).toBe("unavailable");
    }
  });

  it("fails closed on every malformed variation (the WHOLE view is dropped, nothing is partially trusted)", () => {
    const bad: unknown[] = [
      wire([reading("project", { configuration: "disabled" })]), // a value the MCP detector never emits
      wire([reading("project", { configuration: "CONFIGURED" })]),
      wire([reading("project", { freshness: "stale" })]),
      wire([reading("nowhere")]), // unknown scope
      wire([reading("local"), reading("project")]), // out of order
      wire([reading("project"), reading("project")]), // repeated
      wire([reading("project"), reading("local"), reading("global"), reading("global")]), // four
      wire([{ ...reading("project"), extra: "C:\\Users\\SENTINEL" }]), // an extra member
      wire([{ scope: "project", configuration: "configured", freshness: "current", observedAt: T0 }]), // a missing member
      wire([reading("project")], { project: "no_record" }, { declarationDetector: { kind: "claude_mcp", coverage: { project: "no_record", local: "no_record", global: "no_record" } } }), // reading exists but coverage does not say "read"
      { agentSurface: CC, declarationDetector: { kind: "claude_mcp", coverage: { ...NONE, project: "read" } }, declarations: [] }, // coverage says "read" but no reading
      { agentSurface: CC, declarationDetector: { kind: "claude_mcp", coverage: { ...NONE, local: "unknown" } }, declarations: [] }, // unknown coverage value
      { agentSurface: CC, declarationDetector: { kind: "claude_mcp", coverage: { project: "no_record", local: "no_record" } }, declarations: [] }, // a scope missing
      { agentSurface: CC, declarationDetector: { kind: "claude_mcp", coverage: { ...NONE, extra: "no_record" } }, declarations: [] },
      { agentSurface: CC, declarationDetector: { kind: "claude_mcp", coverage: null }, declarations: [] }, // coverage null only for not_implemented
      { agentSurface: CC, declarationDetector: { kind: "something_else", coverage: null }, declarations: [] },
      { agentSurface: CC, declarationDetector: { kind: "claude_mcp", coverage: NONE }, declarations: "none" },
      { agentSurface: CC, declarationDetector: { kind: "not_implemented", coverage: NONE }, declarations: [] }, // not_implemented with coverage
      { agentSurface: CC, declarationDetector: { kind: "not_implemented", coverage: null }, declarations: [reading("project")] }, // not_implemented with readings
    ];
    bad.forEach((b, i) => expect(describeScopedDeclarations(b as Parameters<typeof describeScopedDeclarations>[0]).kind, `variation ${i}`).toBe("unavailable"));
  });

  it("an unsupported host is never treated as scanned: a claude_mcp detector claimed for any other surface is not believed", () => {
    for (const surface of ["codex-cli", "antigravity-ide", "claude-desktop-code", "local-cli", "", "Claude-Code-Terminal"]) {
      expect(describeScopedDeclarations(wire([reading("project")], NONE, { agentSurface: surface })).kind, surface).toBe("unavailable");
    }
  });

  it("not_implemented is a statement about the application: it keeps the surface and carries no readings and no coverage", () => {
    for (const surface of ["codex-cli", "antigravity-ide", "claude-desktop-code", "local-cli", CC]) {
      const v = describeScopedDeclarations({ agentSurface: surface, declarationDetector: { kind: "not_implemented", coverage: null }, declarations: [] });
      expect(v).toEqual({ kind: "not_implemented", surface });
    }
  });

  it("times follow the origin rules: only a valid RFC 3339 stamp, only a non-negative whole age; nothing is repaired", () => {
    const v = (over: Record<string, unknown>) => described(describeScopedDeclarations(wire([reading("project", over)]))).readings[0];
    expect(v({})).toMatchObject({ observedAt: T0, ageHours: 3 });
    expect(v({ observedAt: "garbage" })).toMatchObject({ observedAt: null, ageHours: null });
    expect(v({ observedAt: "2026-02-30T00:00:00Z" })).toMatchObject({ observedAt: null, ageHours: null });
    expect(v({ ageHours: -1 })).toMatchObject({ observedAt: T0, ageHours: null });
    expect(v({ ageHours: 2.5 })).toMatchObject({ ageHours: null });
    expect(v({ ageHours: "3" })).toMatchObject({ ageHours: null });
    expect(v({ observedAt: null, ageHours: 3 })).toMatchObject({ observedAt: null, ageHours: null });
  });
});

describe("the words never turn a missing reading into a negative, and never claim more than a recorded match", () => {
  const all = (): string[] => [
    ...Object.values(CONFIGURATION_TEXT), ...Object.values(FRESHNESS_TEXT), ...Object.values(COVERAGE_TEXT), ...Object.values(SCOPE_NAME), NOT_IMPLEMENTED_TEXT, SCOPED_HEADING,
    PRESENCE_LABEL, SCOPED_SEPARATE, SCOPED_NOT_READ_NOTE, ...["codex-cli", "antigravity-ide", "claude-desktop-code", "local-cli", CC, "x"].map(notImplementedExtra), ...Object.values(METHOD_TEXT),
  ];

  it("no new string says installed, enabled, approved, loaded, running, working, verified, compatible, supported or successful (the one disclaimer sentence is the only place those words appear, negated)", () => {
    const banned = /\b(kurulu|kuruldu|etkin|etkinleştirildi|onaylı|onaylandı|yüklenmiş|yüklendi|çalışan|çalışıyor|çalıştı|doğrulanmış|doğrulandı|uyumlu|destekleniyor|desteklenir|başarılı|başarı|güvenli)\b/i;
    for (const s of all()) expect(s, s).not.toMatch(banned);
    expect(SCOPED_DISCLAIMER).toMatch(/kurulu, onaylı, yüklenmiş, çalışan ya da doğrulanmış olduğunu göstermez/);
  });

  it("coexistence is explained, never called a conflict or an error", () => {
    expect(SCOPED_SEPARATE).toMatch(/birlikte doğru olabilir/);
    for (const s of all()) expect(s, s).not.toMatch(/çelişki|çelişir|çatışma|uyuşmazlık|\bhata\b|\bsorun\b|yanlış/i);
  });

  it("a scope without a reading is described by its coverage and never as a missing definition", () => {
    for (const c of SCOPE_COVERAGE.filter((x) => x !== "read")) {
      const t = COVERAGE_TEXT[c];
      expect(t, c).not.toMatch(/bulunamadı|tanımlı değil|kaydedilmemiş|yapılandırılmamış|bildirilmemiş|tanım yok/);
      expect(t, c).toMatch(/okunmadı|okuma yok|bakılmadı/);
    }
    expect(SCOPED_NOT_READ_NOTE).toMatch(/“tanım yok” anlamına gelmez/);
    expect(NOT_IMPLEMENTED_TEXT).toMatch(/“tanım yok” anlamına gelmez/);
    expect(NOT_IMPLEMENTED_TEXT).toMatch(/destek iddiası da değildir/);
  });

  it("only a read-and-found-nothing result may say 'no definition was found'; an unreadable, malformed or unassessed file says that nothing can be said", () => {
    const texts = DECLARATION_CONFIGURATIONS.map((c) => CONFIGURATION_TEXT[c]);
    expect(new Set(texts).size, "every configuration has its own sentence").toBe(texts.length);
    for (const c of ["malformed", "unreadable", "not_assessed"] as const) {
      expect(CONFIGURATION_TEXT[c], c).toMatch(/bir şey söylenemez/);
      expect(CONFIGURATION_TEXT[c], c).not.toMatch(/bulunamadı|eşleşen|kaydedilmiş/);
    }
    expect(CONFIGURATION_TEXT.not_configured).toMatch(/bulunamadı/);
    expect(CONFIGURATION_TEXT.configured).not.toMatch(/bulunamadı|bir şey söylenemez/);
  });

  it("a confirmed 'no match' is scoped to its own scope; a recorded match is only a name/command match", () => {
    expect(CONFIGURATION_TEXT.not_configured).toMatch(/yalnızca bu kapsamda/);
    expect(CONFIGURATION_TEXT.configured).toMatch(/ad ya da komut dosyası adı eşleşmesi/);
    expect(SCOPE_NAME.global).toMatch(/tüm projelere uygulanır/);
    expect(SCOPE_NAME.project).toMatch(/Bu projenin/);
    expect(SCOPE_NAME.local).toMatch(/Bu proje için/);
  });

  it("every enum value has a text and a previous reading never uses the 'read in the latest scan' wording", () => {
    for (const c of DECLARATION_CONFIGURATIONS) expect(CONFIGURATION_TEXT[c], c).toBeTruthy();
    for (const f of DECLARATION_FRESHNESS) expect(FRESHNESS_TEXT[f], f).toBeTruthy();
    for (const f of DECLARATION_FRESHNESS.filter((x) => x !== "current")) {
      expect(FRESHNESS_TEXT[f], f).toMatch(/^Eski bir (taramadan|kayıttan)/);
      expect(FRESHNESS_TEXT[f], f).toMatch(/güncel (değil|sayılmadı)/);
      expect(FRESHNESS_TEXT[f], f).not.toMatch(/Son taramada okundu/);
    }
    // an old scan is old even if it says configured
    const v = described(describeScopedDeclarations(wire([reading("project", { configuration: "configured", freshness: "previous_area_not_checked" })])));
    const line = scopeLines(v).find((l) => l.scope === "project")!;
    expect(line.current).toBe(false);
    expect(line.text).toMatch(/^Eski bir taramadan/);
    expect(line.text).not.toMatch(/Son taramada okundu/);
  });

  it("scopeLines always yields the three scopes in the fixed order, a reading or its coverage reason", () => {
    const v = described(describeScopedDeclarations(wire([reading("global")], { project: "not_authorized", local: "project_not_in_latest_scan" })));
    const lines = scopeLines(v);
    expect(lines.map((l) => l.scope)).toEqual([...DECLARATION_SCOPES]);
    expect(lines[0]).toMatchObject({ current: null, text: COVERAGE_TEXT.not_authorized });
    expect(lines[1]).toMatchObject({ current: null, text: COVERAGE_TEXT.project_not_in_latest_scan });
    expect(lines[2].current).toBe(true);
    expect(lines[2].text).toContain(CONFIGURATION_TEXT.configured);
  });

  it("the Codex sentence names Codex, says nothing is read and that a machine-level lookup is not Codex-specific", () => {
    const t = notImplementedExtra("codex-cli");
    expect(t).toMatch(/Codex CLI için hiçbir yapılandırma dosyası okunmaz/);
    expect(t).toMatch(/Codex'e özgü değildir/);
    expect(notImplementedExtra("local-cli")).toMatch(/ajan değil/);
  });
});

describe("origin.method is a closed whitelist; the wording follows the technique, not the alias", () => {
  const base = { projectRelation: "project_specific", surfaceRelation: "exact_surface", observedAt: T0, ageHours: 3 };

  it("accepts exactly the six techniques and drops everything else (an older backend sends none)", () => {
    for (const m of ORIGIN_METHODS) expect(describeOrigin({ ...base, method: m })).toMatchObject({ kind: "described", method: m });
    for (const m of [null, undefined, "no-detector", "not-documented", "unsupported-platform", "", "PATH-EXECUTABLE", "C:\\Users\\SENTINEL", 3, {}, ["path-executable"]]) {
      const v = describeOrigin({ ...base, method: m });
      expect(v.kind).toBe("described");
      expect("method" in v, String(m)).toBe(false);
    }
    expect(describeOrigin(base)).toEqual({ kind: "described", project: "project_specific", surface: "exact_surface", observedAt: T0, ageHours: 3 }); // exactly the old view
  });

  it("a plugin record is described as a user-level record/cache - never as a project-specific installation (D7)", () => {
    const v = describeOrigin({ ...base, method: "claude-plugin-declaration" });
    if (v.kind !== "described") throw new Error("described expected");
    const t = originMethodText(v)!;
    expect(t).toMatch(/kullanıcı düzeyinde/);
    expect(t).toMatch(/yalnızca hangi ayar kapsamının denetlendiğini gösterir/);
    expect(t).toMatch(/projeye özel bir kurulum değildir/);
  });

  it("a PATH lookup is a machine-level name match; no technique text mentions local-cli or relates the observation to the project", () => {
    const v = describeOrigin({ ...base, projectRelation: "computer_wide", method: "path-executable" });
    if (v.kind !== "described") throw new Error("described expected");
    expect(originMethodText(v)).toMatch(/bilgisayar düzeyinde bir arama/);
    for (const t of Object.values(METHOD_TEXT)) {
      expect(t, t).not.toMatch(/local-cli|bu projeyle ilişkilendirildi/);
    }
    const none = describeOrigin(base);
    if (none.kind !== "described") throw new Error("described expected");
    expect(originMethodText(none)).toBeNull();
  });
});

describe("the new modules are pure", () => {
  it("no IPC, file, clipboard, network, storage, timer or process access", () => {
    const forbidden = /invoke\(|@tauri-apps|\bapi\.|fetch\(|XMLHttpRequest|WebSocket|navigator\.clipboard|localStorage|sessionStorage|indexedDB|setTimeout|setInterval|createObjectURL|FileReader|window\.open|child_process|require\(|import\.meta\.env|process\.env/;
    for (const [name, source] of [["scopedDeclarations.ts", scopedSource], ["ScopedConfigNote.tsx", noteSource], ["origin.ts", originSource]] as const) {
      expect(source, name).not.toMatch(forbidden);
    }
    expect(noteSource).not.toMatch(/<button|onClick|dangerouslySetInnerHTML|<a /);
  });
});
