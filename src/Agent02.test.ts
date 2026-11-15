// AGENT-02 (Issue #53): six more read-only project surfaces. Pure data/registry tests: no I/O, no IPC, no network, no clock.
import { describe, expect, it } from "vitest";
import catalogSchemaSource from "../schemas/catalog-entry.schema.json?raw";
import migrationSource from "../src-tauri/migrations/0011_agent02_surfaces.sql?raw";
import planRustSource from "../src-tauri/src/plan.rs?raw";
import projectsRustSource from "../src-tauri/src/projects.rs?raw";
import researchSource from "../docs/RESEARCH-SOURCES.md?raw";
import { AGENT_HOST_SURFACES, CAPABILITIES, agentHostFor, planSurfaces } from "./agentHosts";
import { classifyName } from "./agentGuide";
import { HOSTS, hostForSurface, planConfigPreview } from "./configPlan";
import { surfaceLabel } from "./labels";
import { makeEntry, pref } from "./test/backend";
import { TARGET_SURFACES, acceptBlockReason, buildRows, compatStateFor, targetCounts } from "./workspace";
import type { CatalogEntry, ProjectView } from "./types";

const NEW = ["codex-vscode", "cline-cli", "cline-vscode", "cline-jetbrains", "kiro-cli", "kiro-ide"] as const;
const OLD = ["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "codex-cli", "local-cli"] as const;
const WIN = "windows-x64";
const rustList = (src: string, name: string) => {
  const m = src.match(new RegExp(`pub const ${name}: &\\[&str\\] =\\s*&\\[([^\\]]*)\\]`));
  expect(m, name).not.toBeNull();
  return [...m![1].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
};

describe("AGENT-02 registries agree", () => {
  it("TypeScript list, Rust project allowlist, host registry and migration 0011 carry the same eleven ids in the same order", () => {
    const all = [...TARGET_SURFACES];
    expect(all).toHaveLength(11);
    expect(rustList(projectsRustSource, "SURFACES")).toEqual(all);
    expect(AGENT_HOST_SURFACES.map((h) => h.agentSurface)).toEqual(all);
    const check = migrationSource.match(/agent_surface\s+TEXT NOT NULL CHECK \(agent_surface IN \(([^)]*)\)\)/);
    expect([...check![1].matchAll(/'([^']+)'/g)].map((m) => m[1])).toEqual(all);
    for (const s of OLD) expect(all, s).toContain(s);
    for (const s of NEW) expect(all, s).toContain(s);
  });

  it("the catalog schema's targetEvidence.agentSurface accepts codex-cli and the six new ids, keeps the old ones, and is an exact enum", () => {
    const schema = JSON.parse(catalogSchemaSource);
    const find = (o: unknown): unknown[] | null => {
      if (o && typeof o === "object") {
        const rec = o as Record<string, unknown>;
        const a = rec.agentSurface as { enum?: unknown[] } | undefined;
        if (a && Array.isArray(a.enum)) return a.enum;
        for (const v of Object.values(rec)) {
          const r = find(v);
          if (r) return r;
        }
      }
      return null;
    };
    const en = find(schema)!;
    expect(en).toEqual(["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "antigravity-cli", "antigravity-2", "codex-cli", "codex-vscode", "cline-cli", "cline-vscode", "cline-jetbrains", "kiro-cli", "kiro-ide", "local-cli", null]);
  });

  it("the install-plan list is untouched: no new surface is an installer target", () => {
    const plan = rustList(planRustSource, "SURFACES");
    expect(plan).toEqual(["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "antigravity-cli", "antigravity-2", "local-cli"]);
    for (const s of NEW) expect(plan, s).not.toContain(s);
    expect(planSurfaces(TARGET_SURFACES)).toEqual(["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "local-cli"]);
  });

  it("no alias, no desktop surface and no provider is a surface", () => {
    const all = TARGET_SURFACES as readonly string[];
    for (const bad of ["codex-ide", "codex-desktop", "codex-desktop-experience", "codex-app", "cline-desktop", "cline", "kiro", "cursor", "windsurf", "deepseek", "groq", "grok", "gemini-api", "gemini-cli"]) {
      expect(all, bad).not.toContain(bad);
    }
  });
});

describe("each new surface is its own row with honest, conservative fields", () => {
  it("labels are distinct and exact", () => {
    expect(NEW.map(surfaceLabel)).toEqual(["Codex VS Code", "Cline CLI", "Cline VS Code", "Cline JetBrains", "Kiro CLI", "Kiro IDE"]);
    expect(new Set(TARGET_SURFACES.map(surfaceLabel)).size).toBe(TARGET_SURFACES.length);
  });

  it("no new surface has a preview, none is plan-supported, and none is in the JSON host list", () => {
    for (const s of NEW) {
      const h = agentHostFor(s)!;
      expect(h.kind, s).toBe("agent-host");
      expect(h.previewKind, s).toBe("none");
      expect(h.planSupported, s).toBe(false);
      expect(hostForSurface(s), s).toBeNull();
      expect(HOSTS.map((x) => x.surface), s).not.toContain(s);
    }
    expect(agentHostFor("codex-cli")!.previewKind).toBe("toml-not-implemented");
  });

  it("Cline surfaces and Codex VS Code document no project file: none-documented, null path, no format", () => {
    for (const s of ["codex-vscode", "cline-cli", "cline-vscode", "cline-jetbrains"] as const) {
      const h = agentHostFor(s)!;
      expect(h.configScope, s).toBe("none-documented");
      expect(h.relativeTargetPath, s).toBeNull();
      expect(h.configFormat, s).toBeNull();
      expect(h.scopeCondition, s).toBeNull();
    }
  });

  it("Kiro: the documented JSON path is recorded but grants no preview", () => {
    for (const s of ["kiro-cli", "kiro-ide"] as const) {
      const h = agentHostFor(s)!;
      expect(h.configScope).toBe("project");
      expect(h.relativeTargetPath).toBe(".kiro/settings/mcp.json");
      expect(h.configFormat).toBe("json");
      expect(h.previewKind).toBe("none");
      expect(h.untouched.join(" ")).toContain("~/.kiro/settings/mcp.json");
      expect(h.untouched.join(" ")).toContain("autoApprove");
    }
  });

  it("no capability is inherited from a sibling surface of the same client", () => {
    for (const s of ["codex-vscode", "cline-vscode", "cline-jetbrains"] as const) {
      for (const c of CAPABILITIES) expect(agentHostFor(s)!.capabilities[c], `${s}/${c}`).toBe("not-assessed");
    }
    for (const s of NEW) {
      for (const c of ["skills", "plugins", "project_instructions"] as const) expect(agentHostFor(s)!.capabilities[c], `${s}/${c}`).toBe("not-assessed");
    }
    expect(agentHostFor("cline-cli")!.capabilities.mcp_stdio).toBe("documented");
    expect(agentHostFor("kiro-cli")!.capabilities.mcp_http).toBe("documented");
  });

  it("the capability vocabulary is unchanged: no runtime-verification state exists", () => {
    const states = new Set(AGENT_HOST_SURFACES.flatMap((h) => Object.values(h.capabilities)));
    expect([...states].sort()).toEqual(["documented", "not-assessed"]);
  });

  it("clients: one family per client", () => {
    expect(new Set(["codex-cli", "codex-vscode"].map((s) => agentHostFor(s)!.clientHostId))).toEqual(new Set(["codex"]));
    expect(new Set(["cline-cli", "cline-vscode", "cline-jetbrains"].map((s) => agentHostFor(s)!.clientHostId))).toEqual(new Set(["cline"]));
    expect(new Set(["kiro-cli", "kiro-ide"].map((s) => agentHostFor(s)!.clientHostId))).toEqual(new Set(["kiro"]));
  });

  it("every evidence id used by a new surface exists as a row in the research register", () => {
    for (const s of NEW) {
      for (const id of agentHostFor(s)!.evidenceIds) expect(researchSource, `${s}/${id}`).toMatch(new RegExp(`^\\| ${id} \\|`, "m"));
    }
  });

  it("the guide knows Cline and Kiro as hosts, still treats providers as non-hosts, and does not invent a desktop surface", () => {
    expect(classifyName("Cline").example?.kind).toBe("managed-agent-host");
    expect(classifyName("kiro ide").example?.kind).toBe("managed-agent-host");
    expect(classifyName("groq").example?.kind).toBe("model-api-provider");
    expect(classifyName("cline desktop").verdict).toBe("MANUAL_VERIFY");
  });
});

describe("fail-closed behaviour of the existing entry points for every new surface", () => {
  const ev = (surface: string, state = "vendor-declared") => ({ platformTarget: WIN, agentSurface: surface, compatibilityState: state, limitation: "x" });
  const mcp = (surfaces: string[] = ["claude-code-terminal"]): CatalogEntry =>
    makeEntry("synthetic-mcp", { kind: "agent_plugin", record: { category: "mcp-server", targetEvidence: surfaces.map((s) => ev(s)) } });

  it("the configuration preview answers UNSUPPORTED_SURFACE and shows no path, no diff, no text", () => {
    for (const s of NEW) {
      const p = planConfigPreview({ alias: "benim-sitem", platform: WIN, surface: s, entry: mcp(), source: { kind: "new" } });
      expect(p.status, s).toBe("UNSUPPORTED_SURFACE");
      expect(p.applied).toBe(false);
      expect(p.touchedFiles).toBe(0);
      expect(p.relativePath, s).toBeNull();
      expect(p.diff, s).toBeNull();
      expect(p.proposed, s).toBeNull();
    }
  });

  it("evidence for one surface never counts for another surface of the same client; unknown stays unknown and acceptance stays blocked", () => {
    const e = mcp(["claude-code-terminal", "cline-cli"]);
    expect(compatStateFor(e, WIN, "cline-cli")).toBe("vendor-declared");
    for (const s of ["cline-vscode", "cline-jetbrains", "codex-cli", "codex-vscode", "kiro-cli", "kiro-ide"]) {
      expect(compatStateFor(e, WIN, s), s).toBe("unknown");
      expect(acceptBlockReason(e, compatStateFor(e, WIN, s)), s).not.toBeNull();
    }
  });

  it("a stored preference is shown only under its own surface", () => {
    const e = mcp();
    for (const [a, b] of [["cline-cli", "cline-vscode"], ["cline-vscode", "cline-jetbrains"], ["codex-cli", "codex-vscode"], ["kiro-cli", "kiro-ide"]] as const) {
      const project: ProjectView = { projectId: "p1", alias: "benim-sitem", displayName: "x", kind: "reference", localNote: "", createdAt: "2026-11-14T00:00:00Z", preferences: [pref({ entryId: e.entryId, agentSurface: a, platformTarget: WIN, decision: "rejected" })] };
      expect(buildRows([e], project, WIN, a).rows[0].pref?.decision, a).toBe("rejected");
      expect(buildRows([e], project, WIN, b).rows[0].pref, b).toBeNull();
      const counts = targetCounts(project);
      expect(counts.targets.map((t) => t.surface)).toEqual([a]);
      expect(counts.unreachable).toBe(0);
    }
  });

  it("a stored preference with an unlisted surface stays unreachable rather than being mapped onto a new one", () => {
    const project: ProjectView = { projectId: "p1", alias: "a", displayName: "x", kind: "reference", localNote: "", createdAt: "2026-11-14T00:00:00Z", preferences: [pref({ agentSurface: "cline", platformTarget: WIN }), pref({ agentSurface: "Kiro-IDE", platformTarget: WIN })] };
    expect(targetCounts(project).unreachable).toBe(2);
  });
});
