// AGENT-01 - the Codex CLI target in the configuration planner: a TOML surface is never fed to the JSON parser, never produces a JSON file or a diff, never reads or
// echoes text, and fails closed. Synthetic fixtures only: no file, no network, no IPC. Every "secret" is an obviously fake sentinel.
import { describe, expect, it } from "vitest";
import { type ReviewedDeclaration } from "./configDeclarations";
import { NO_TEXT_PROCESSED, STATUS_LABEL, isPlanStale, parseConfigText, planConfigPreview, planInputKey, type PlanRequest, type PreviewPlan } from "./configPlan";
import { makeEntry } from "./test/backend";

const WIN = "windows-x64";
const MAC = "macos-arm64";
const CC = "claude-code-terminal";
const AG = "antigravity-ide";
const CODEX = "codex-cli";
const SENTINEL = "ghp_SENTINELSENTINELSENTINELSENTINEL12";
const SENTINEL2 = "sk-SENTINEL-SECRET-VALUE-0123456789abcdef";
const PIN = "0123456789abcdef0123456789abcdef01234567";

const ev = (platformTarget: string, agentSurface: string, compatibilityState: string) => ({ platformTarget, agentSurface, compatibilityState, limitation: "x" });
const mcp = (over: Record<string, unknown> = {}) =>
  makeEntry("synthetic-mcp", { displayName: "Sentetik MCP", kind: "agent_plugin", record: { category: "mcp-server", targetEvidence: [ev(WIN, CC, "vendor-declared"), ev(MAC, CC, "vendor-declared")] }, ...over });
/** Even a perfectly valid JSON declaration for BOTH JSON hosts must never be turned into a Codex file. */
const DECLS: ReviewedDeclaration[] = [
  { hostId: "claude-code-project-mcp", entryId: "synthetic-mcp", serverName: "synthetic", declaration: { command: "npx", args: ["-y", "@example/synthetic-mcp@1.2.3"] }, review: { reviewId: "REV-SYN-1", evidenceIds: ["R99"], reviewedAgainst: PIN } },
  { hostId: "antigravity-workspace-mcp", entryId: "synthetic-mcp", serverName: "synthetic", declaration: { serverUrl: "https://synthetic.example.invalid/mcp" }, review: { reviewId: "REV-SYN-2", evidenceIds: ["R99"], reviewedAgainst: PIN } },
];
const req = (over: Partial<PlanRequest> = {}): PlanRequest => ({ alias: "benim-sitem", platform: WIN, surface: CODEX, entry: mcp(), source: { kind: "new" }, ...over });
const plan = (over: Partial<PlanRequest> = {}, decls: readonly ReviewedDeclaration[] = DECLS): PreviewPlan => planConfigPreview(req(over), decls);
const everything = (p: PreviewPlan) => JSON.stringify(p);

describe("Codex CLI: TOML_PREVIEW_NOT_IMPLEMENTED with truthful scope and static guidance", () => {
  it("answers with the explicit TOML status for every OS, mode and registry - never a JSON preview", () => {
    for (const platform of [WIN, MAC]) {
      for (const source of [{ kind: "new" }, { kind: "text", text: "" }, { kind: "text", text: "[mcp_servers.x]\ncommand = \"a\"\n" }, { kind: "text", text: '{"mcpServers":{}}' }] as PlanRequest["source"][]) {
        for (const decls of [[], DECLS]) {
          const p = plan({ platform, source }, decls);
          expect(p.status).toBe("TOML_PREVIEW_NOT_IMPLEMENTED");
          expect(p.statusLabel).toBe(STATUS_LABEL.TOML_PREVIEW_NOT_IMPLEMENTED);
          expect([p.original, p.proposed, p.diff, p.declaration, p.sourceDigest, p.formattingNote]).toEqual([null, null, null, null, null, null]);
          expect(p.hypothetical).toBe(false);
          expect(p.applied).toBe(false);
          expect(p.touchedFiles).toBe(0);
          expect(p.existingServerNames).toEqual([]);
          expect(p.maskedValues).toBe(0);
        }
      }
    }
  });

  it("states the TOML target honestly: path, format, scope, trust condition and the files it never touches", () => {
    const p = plan();
    expect(p.relativePath).toBe(".codex/config.toml");
    expect(p.configFormat).toBe("toml");
    expect(p.scope).toMatch(/güvenilen/);
    expect(p.identity).toEqual({ alias: "benim-sitem", platform: WIN, surface: CODEX, entryId: "synthetic-mcp", hostId: null });
    expect(p.untouched.join(" ")).toContain("~/.codex/config.toml");
    expect(p.untouched.join(" ")).toContain("AGENTS.md");
    const prose = p.constraints.join(" ");
    expect(prose).toMatch(/TOML/);
    expect(prose).toMatch(/GÜVENİLİR/);
    expect(prose).toMatch(/model_provider/);
    expect(prose).toMatch(/aynı config\.toml/);
    expect(p.blockers.map((b) => b.code)).toContain("toml_preview_not_implemented");
    expect(p.notice).toBe(NO_TEXT_PROCESSED);
    expect(p.notice).toMatch(/Hiçbir dosyaya dokunulmadı/);
    expect(p.notice).toMatch(/hiçbir metin de işlenmedi/);
  });

  it("the guidance is static text: no executable command line, no @latest, no network call, no secret, no path outside the project file", () => {
    const g = plan().guidance;
    expect(g.length).toBeGreaterThanOrEqual(5);
    const text = g.join("\n");
    expect(text).not.toMatch(/npx -y|uvx|curl|irm |\| ?(ba)?sh|iex|codex mcp add|--oauth/);
    expect(text).not.toMatch(/"command"|"args"|\[mcp_servers\.[a-z0-9_-]+\]\s*\n\s*command/);
    expect(text).toMatch(/GÜVENİLİR/);
    expect(text).toMatch(/\/mcp/);
    expect(text).toMatch(/ortam değişkeni adı/);
    expect(text).toMatch(/sabitlenmemiş/);
  });

  it("is a different answer from every JSON host: no .mcp.json, no JSON file, no mcpServers, no Claude/Antigravity text in the plan", () => {
    const text = everything(plan());
    expect(text).not.toContain('"hostId":"claude-code-project-mcp"');
    expect(text).not.toContain("antigravity-workspace-mcp");
    expect(text).not.toMatch(/\.agents\/mcp_config\.json/);
    expect(text).not.toContain("mcpServers");
    // the JSON constraint wording must not leak into the Codex plan
    expect(text).not.toMatch(/Claude Code, etkileşimli oturumlarda proje MCP/);
    expect(plan({ surface: CC }).relativePath).toBe(".mcp.json");
    expect(plan({ surface: AG }).relativePath).toBe(".agents/mcp_config.json");
  });

  it("the catalog gates are reported next to the TOML blocker and cannot be lifted: unknown compatibility, quarantine, removed, disabled, not an MCP entry", () => {
    const codes = (p: PreviewPlan) => p.blockers.map((b) => b.code);
    const base = plan();
    expect(codes(base)).toEqual(expect.arrayContaining(["accept_block", "compat_not_declared", "toml_preview_not_implemented"]));
    expect(base.constraints.join(" ")).toContain("Uyumluluk (bu hedef için kayıt): unknown");
    expect(codes(plan({ entry: mcp({ reviewState: "quarantined" }) }))).toContain("quarantined");
    expect(codes(plan({ entry: mcp({ removedAt: "2026-10-01T00:00:00Z" }) }))).toContain("accept_block");
    expect(codes(plan({ entry: mcp({ enabled: false }) }))).toContain("accept_block");
    expect(codes(plan({ entry: mcp({ executionPolicy: "blocked-until-scoped-authorization" }) }))).toContain("accept_block");
    expect(codes(plan({ entry: makeEntry("plain", { record: { category: "python-cli" } }) }))).toContain("not_mcp");
    // every one of them still ends in the TOML status, never in a preview
    for (const e of [mcp({ reviewState: "quarantined" }), mcp({ enabled: false }), makeEntry("plain", { record: { category: "python-cli" } })]) expect(plan({ entry: e }).status).toBe("TOML_PREVIEW_NOT_IMPLEMENTED");
    // an explicit Codex vendor-declared row is a catalog fact, not a license: still no preview
    const declared = mcp({ record: { category: "mcp-server", targetEvidence: [ev(WIN, CODEX, "vendor-declared")] } });
    const p = plan({ entry: declared });
    expect(p.status).toBe("TOML_PREVIEW_NOT_IMPLEMENTED");
    expect(p.diff).toBeNull();
  });

  it("identity is still never defaulted for Codex: INVALID_INPUT outranks the TOML status", () => {
    for (const over of [{ alias: "" }, { platform: "linux-x64" }, { entry: mcp({ entryId: "" }) }]) expect(plan(over as Partial<PlanRequest>).status).toBe("INVALID_INPUT");
  });

  it("near-miss surface names are UNSUPPORTED_SURFACE, not Codex: no provider brand, no other Codex product, no wrong case", () => {
    for (const surface of ["codex", "Codex-CLI", "codex-desktop", "codex-ide", "chatgpt-desktop", "gemini-cli", "deepseek", "grok", "groq", "xai", "openai", ""]) {
      const p = plan({ surface });
      expect(p.status, surface).toBe("UNSUPPORTED_SURFACE");
      expect(p.relativePath, surface).toBeNull();
      expect(p.configFormat, surface).toBeNull();
      expect(p.guidance, surface).toEqual([]);
    }
    // and the unsupported message names the Codex distinction instead of silently redirecting
    expect(plan({ surface: "codex" }).problems[0].text).toMatch(/Codex CLI/);
  });
});

describe("format mismatch fails closed", () => {
  it("Codex TOML text is never parsed by the JSON parser, and the JSON hosts reject TOML text as an invalid current configuration", () => {
    const toml = `[mcp_servers.synthetic]\ncommand = "npx"\nargs = ["-y", "pkg"]\ntoken = "${SENTINEL}"\n`;
    expect(parseConfigText(toml).ok).toBe(false);
    for (const surface of [CC, AG]) {
      const p = planConfigPreview(req({ surface, source: { kind: "text", text: toml } }), DECLS);
      expect(p.status, surface).toBe("INVALID_CURRENT_CONFIG");
      expect(p.proposed, surface).toBeNull();
      expect(p.diff, surface).toBeNull();
      expect(everything(p), surface).not.toContain(SENTINEL);
    }
    // for Codex itself the text is not looked at at all
    const c = planConfigPreview(req({ source: { kind: "text", text: toml } }), DECLS);
    expect(c.status).toBe("TOML_PREVIEW_NOT_IMPLEMENTED");
    expect(c.problems).toEqual([]);
    expect(everything(c)).not.toContain(SENTINEL);
  });

  it("JSON text with secrets pasted for a Codex target is ignored: nothing of it is echoed, hashed or masked", () => {
    const json = JSON.stringify({ mcpServers: { a: { command: "x", env: { API_KEY: SENTINEL, other: SENTINEL2 } } } });
    const c = planConfigPreview(req({ source: { kind: "text", text: json } }), DECLS);
    expect(c.status).toBe("TOML_PREVIEW_NOT_IMPLEMENTED");
    const all = everything(c);
    for (const secret of [SENTINEL, SENTINEL2, "API_KEY", "mcpServers"]) expect(all, secret).not.toContain(secret);
    expect(c.sourceDigest).toBeNull();
  });

  it("untrusted, hostile and oversized text for a Codex target cannot change the answer", () => {
    const outputs = new Set<string>();
    for (const t of ["", "__proto__ = 1\n[__proto__]\npolluted = true", "[[mcp_servers]]\n".repeat(50), "x".repeat(1_000_000), "﻿[a]\n", "{".repeat(100_000)]) {
      const p = planConfigPreview(req({ source: { kind: "text", text: t } }), DECLS);
      outputs.add(JSON.stringify({ ...p, inputKey: "" }));
    }
    expect(outputs.size).toBe(1);
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
  });
});

describe("stale invalidation is unaffected and still covers Codex", () => {
  it("the input key changes with project, OS, surface, entry and catalog state; an unchanged request is not stale", () => {
    const p = plan();
    expect(isPlanStale(p, req(), DECLS)).toBe(false);
    for (const over of [{ alias: "baska" }, { platform: MAC }, { surface: CC }, { surface: AG }, { entry: mcp({ entryId: "other" }) }, { entry: mcp({ reviewState: "quarantined" }) }, { entry: mcp({ revision: "r2" }) }] as Partial<PlanRequest>[]) {
      expect(isPlanStale(p, req(over), DECLS), JSON.stringify(Object.keys(over))).toBe(true);
    }
    expect(planInputKey(req({ surface: CODEX }), DECLS)).not.toBe(planInputKey(req({ surface: CC }), DECLS));
    expect(planInputKey(req({ surface: CODEX }), DECLS)).not.toBe(planInputKey(req({ surface: AG }), DECLS));
  });

  it("is deterministic", () => {
    expect(JSON.stringify(plan())).toBe(JSON.stringify(plan()));
  });
});
