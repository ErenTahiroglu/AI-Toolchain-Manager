// AGENT-01 - the declarative host registry, the Codex target, and the shared guide's classification. Pure tests: no rendering, no backend, no file, no network.
import { describe, expect, it } from "vitest";
import planRustSource from "../src-tauri/src/plan.rs?raw";
import projectsRustSource from "../src-tauri/src/projects.rs?raw";
import { AGENT_HOST_SURFACES, CAPABILITIES, agentHostFor, planSurfaces } from "./agentHosts";
import { CONCEPTS, GUIDE_EXAMPLES, GUIDE_STEPS, NO_UNIVERSAL_FORMAT, classifyName } from "./agentGuide";
import { HOSTS, hostForSurface } from "./configPlan";
import { surfaceLabel } from "./labels";
import { TARGET_SURFACES } from "./workspace";

const rustList = (source: string, name: string) => {
  const m = source.match(new RegExp(`pub const ${name}: &\\[&str\\] =\\s*&\\[([^\\]]*)\\]`));
  expect(m, name).not.toBeNull();
  return [...m![1].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
};

describe("the host registry", () => {
  it("has exactly one row per selectable surface, in the same order, and is pinned to the Rust allowlist", () => {
    expect(AGENT_HOST_SURFACES.map((h) => h.agentSurface)).toEqual([...TARGET_SURFACES]);
    expect(rustList(projectsRustSource, "SURFACES")).toEqual([...TARGET_SURFACES]);
    expect(TARGET_SURFACES).toContain("codex-cli");
    expect(new Set(AGENT_HOST_SURFACES.map((h) => h.agentSurface)).size).toBe(AGENT_HOST_SURFACES.length);
  });

  it("the install-plan layer's surface list is untouched: a project target is not an installer target", () => {
    const planList = rustList(planRustSource, "SURFACES");
    expect(planList).not.toContain("codex-cli");
    expect(planSurfaces(TARGET_SURFACES).sort()).toEqual(TARGET_SURFACES.filter((s) => planList.includes(s)).sort());
    expect(planSurfaces(TARGET_SURFACES)).not.toContain("codex-cli");
    expect(planSurfaces(TARGET_SURFACES)).toEqual(["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "local-cli"]);
  });

  it("Codex CLI is its own surface: TOML, a fixed relative path, project scope only for trusted projects, no JSON preview, not an installer target", () => {
    const codex = agentHostFor("codex-cli")!;
    expect(codex.clientHostId).toBe("codex");
    expect(codex.kind).toBe("agent-host");
    expect(codex.configFormat).toBe("toml");
    expect(codex.relativeTargetPath).toBe(".codex/config.toml");
    expect(codex.configScope).toBe("project");
    expect(codex.scopeCondition).toMatch(/GÜVENİLİR/);
    expect(codex.previewKind).toBe("toml-not-implemented");
    expect(codex.planSupported).toBe(false);
    expect(codex.untouched.join(" ")).toContain("~/.codex/config.toml");
    expect(codex.evidenceIds).toEqual(["R48", "R49"]);
    // it is NOT a JSON host and has no JSON preview host
    expect(hostForSurface("codex-cli")).toBeNull();
    expect(HOSTS.map((h) => h.surface)).not.toContain("codex-cli");
    expect(surfaceLabel("codex-cli")).toBe("Codex CLI");
  });

  it("the registry and the JSON preview hosts agree about the two JSON surfaces (one source of truth per fact)", () => {
    for (const h of HOSTS) {
      const r = agentHostFor(h.surface)!;
      expect(r.configFormat, h.id).toBe("json");
      expect(r.relativeTargetPath, h.id).toBe(h.relativePath);
      expect(r.previewKind, h.id).toBe("json-diff");
      expect(r.untouched, h.id).toEqual(h.untouched);
    }
  });

  it("only surfaces with a documented project file carry a path and format; nothing is guessed for the others", () => {
    for (const h of AGENT_HOST_SURFACES) {
      if (h.configScope === "none-documented") {
        expect([h.relativeTargetPath, h.configFormat, h.scopeCondition, h.previewKind]).toEqual([null, null, null, "none"]);
      } else {
        expect(h.relativeTargetPath).toMatch(/^[.\w-]+(\/[.\w-]+)*$/);
        expect(h.relativeTargetPath).not.toMatch(/(^\/|\.\.|~|:|\\)/);
      }
    }
    expect(agentHostFor("local-cli")!.kind).toBe("program-lookup");
    expect(agentHostFor("claude-desktop-code")!.relativeTargetPath).toBeNull();
  });

  it("capabilities are exactly the five named ones; 'not-assessed' is the default and 'documented' needs a source id", () => {
    expect([...CAPABILITIES]).toEqual(["mcp_stdio", "mcp_http", "skills", "plugins", "project_instructions"]);
    for (const h of AGENT_HOST_SURFACES) {
      expect(Object.keys(h.capabilities).sort()).toEqual([...CAPABILITIES].sort());
      const documented = CAPABILITIES.filter((c) => h.capabilities[c] === "documented");
      if (documented.length > 0) expect(h.evidenceIds.length, h.agentSurface).toBeGreaterThan(0);
    }
    expect(agentHostFor("codex-cli")!.capabilities.skills).toBe("not-assessed");
    expect(agentHostFor("local-cli")!.capabilities.mcp_stdio).toBe("not-assessed");
  });

  it("no provider/brand name appears as a surface, a client host id, or a path", () => {
    // the identifying fields only: a documented file path of another client (the user-global Antigravity file lives under ~/.gemini) is not a brand claim
    const text = JSON.stringify(AGENT_HOST_SURFACES.map((h) => [h.agentSurface, h.clientHostId, h.relativeTargetPath, h.configFormat])).toLowerCase();
    for (const brand of ["deepseek", "groq", "grok", "xai", "x.ai", "gemini"]) expect(text, brand).not.toContain(brand);
    expect(TARGET_SURFACES.join(" ")).not.toMatch(/deepseek|groq|grok|xai|gemini|openai/);
  });

  it("an unknown surface has no host and no plan support", () => {
    for (const s of ["", "codex", "Codex-CLI", "codex-desktop", "codex-ide", "gemini-cli", "deepseek", "grok", "groq"]) {
      expect(agentHostFor(s), s).toBeNull();
      expect(planSurfaces([s]), s).toEqual([]);
    }
  });
});

describe("the shared guide: classification is an exact lookup of documented examples, anything else is MANUAL_VERIFY", () => {
  it("separates hosts from providers, and similar names from each other", () => {
    expect(classifyName("Codex CLI").kind).toBe("managed-agent-host");
    expect(classifyName("codex-cli").kind).toBe("managed-agent-host");
    expect(classifyName("Claude Code").kind).toBe("managed-agent-host");
    expect(classifyName("Antigravity IDE").kind).toBe("managed-agent-host");
    expect(classifyName("Gemini CLI").kind).toBe("example-agent-host");
    expect(classifyName("Gemini API").kind).toBe("model-api-provider");
    expect(classifyName("DeepSeek").kind).toBe("model-api-provider");
    expect(classifyName("Grok").kind).toBe("model-api-provider");
    expect(classifyName("xAI").kind).toBe("model-api-provider");
    expect(classifyName("Groq").kind).toBe("model-api-provider");
    expect(classifyName("GroqCloud").kind).toBe("model-api-provider");
    expect(classifyName("grok").example!.id).toBe("grok");
    expect(classifyName("groq").example!.id).toBe("groq");
    expect(classifyName("grok").example!.id).not.toBe(classifyName("groq").example!.id);
  });

  it("is case-, diacritic- and whitespace-insensitive, but never fuzzy", () => {
    expect(classifyName("  GEMINI   cli ").example!.id).toBe("gemini-cli");
    for (const near of ["groc", "grokk", "grq", "gemini", "gemini c l i", "deep-seek-cli", "codexx", "codex desktop", "openai", "claude", "anti gravity"]) {
      const c = classifyName(near);
      expect(c.kind, near).toBe("uncertain");
      expect(c.verdict, near).toBe("MANUAL_VERIFY");
      expect(c.example, near).toBeNull();
    }
  });

  it("an empty or unknown name is MANUAL_VERIFY and never yields a path, a format, 'compatible' or 'installed'", () => {
    for (const unknown of ["", "   ", "Cursor", "Zed", "bilinmeyen-ajan-3000", "<script>alert(1)</script>", "x".repeat(500)]) {
      const c = classifyName(unknown);
      expect(c.verdict).toBe("MANUAL_VERIFY");
      expect(c.kind).toBe("uncertain");
      expect(c.message).not.toMatch(/\.json|\.toml|settings|config\.|uyumlu\b|kurulu\b|compatible|installed/i);
    }
    for (const known of GUIDE_EXAMPLES) expect(classifyName(known.name).verdict).toBe("KNOWN_EXAMPLE");
  });

  it("the example table: every managed host row mirrors the registry; providers carry no config path; every row cites a register id", () => {
    for (const e of GUIDE_EXAMPLES) {
      expect(e.evidenceIds.length, e.id).toBeGreaterThan(0);
      for (const id of e.evidenceIds) expect(id).toMatch(/^R\d+$/);
      if (e.kind === "model-api-provider") expect(e.note, e.id).not.toMatch(/\.mcp\.json|mcp_config|settings\.json/);
    }
    const managed = GUIDE_EXAMPLES.filter((e) => e.kind === "managed-agent-host").map((e) => e.id).sort();
    expect(managed).toEqual(["antigravity", "claude-code", "codex"]);
    // Gemini CLI is only an example: it is not a selectable surface
    expect(TARGET_SURFACES as readonly string[]).not.toContain("gemini-cli");
    expect(GUIDE_EXAMPLES.find((e) => e.id === "gemini-cli")!.note).toMatch(/yönetmez/);
  });

  it("the workflow has the six steps in order, ends in MANUAL_VERIFY semantics, and never claims one universal format", () => {
    expect(GUIDE_STEPS.map((s) => s.id)).toEqual(["classify", "verify-support", "separate-things", "scope-and-risk", "manual-verify", "examples"]);
    expect(GUIDE_STEPS.find((s) => s.id === "manual-verify")!.body).toContain("MANUAL_VERIFY");
    expect(NO_UNIVERSAL_FORMAT).toMatch(/tek bir ortak settings\.json/);
    expect(NO_UNIVERSAL_FORMAT).toContain(".codex/config.toml");
    expect(JSON.stringify(GUIDE_STEPS)).not.toMatch(/tüm istemciler (aynı|ortak)/i);
    expect(CONCEPTS.map((c) => c.thing)).toEqual(["MCP sunucusu", "Skill (SKILL.md)", "Ajan eklentisi / pazaryeri", "Proje yönerge dosyası", "Model / API sağlayıcısı", "Çalışma ortamı (runtime)"]);
  });
});
