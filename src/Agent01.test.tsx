// AGENT-01 - the Codex CLI target and the shared manual guide, rendered against the in-memory backend. jsdom only: no real window, no real file, no agent, no network.
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import capabilitySource from "../src-tauri/capabilities/default.json?raw";
import agentGuideSource from "./agentGuide.ts?raw";
import agentHostsSource from "./agentHosts.ts?raw";
import guideComponentSource from "./components/AgentGuide.tsx?raw";
import { ConfigPreview } from "./components/ConfigPreview";
import { PlanPage } from "./pages/Plan";
import { ProjectsPage } from "./pages/Projects";
import { deriveReview } from "./review";
import { createBackend, emptyInventory, makeEntry, makeRequest, originOf, pref, type Backend } from "./test/backend";
import type { CatalogEntry, Inventory, PreferenceView, ProjectView } from "./types";
import { TARGET_SURFACES, resolveTarget, targetCounts } from "./workspace";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const WIN = "windows-x64";
const MAC = "macos-arm64";
const CC = "claude-code-terminal";
const AG = "antigravity-ide";
const CODEX = "codex-cli";
const CLI = "local-cli";
const ev = (platformTarget: string, agentSurface: string, compatibilityState: string) => ({ platformTarget, agentSurface, compatibilityState, limitation: "kayıt" });
const mcp = (over: Partial<CatalogEntry> = {}) =>
  makeEntry("synthetic-mcp", { displayName: "Sentetik MCP", kind: "agent_plugin", record: { category: "mcp-server", targetEvidence: [ev(WIN, CC, "vendor-declared"), ev(MAC, CC, "vendor-declared")] }, ...over });
const plain = () => makeEntry("gitleaks", { displayName: "Gitleaks", record: { targetEvidence: [ev(WIN, CLI, "vendor-declared")] } });
const wish = (entryId: string, decision: PreferenceView["decision"], over: Partial<PreferenceView> = {}) =>
  pref({ entryId, entryName: entryId, agentSurface: CC, platformTarget: WIN, decision, ...over });
const project = (preferences: PreferenceView[] = []): ProjectView => ({ projectId: "p1", alias: "benim-sitem", displayName: "Benim sitem", kind: "reference", localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences });

const FORBIDDEN_WRITES = ["run_discovery_scan", "evaluate_task", "reset_project_preference", "prepare_install", "confirm_install", "cancel_install", "recover_installations", "plan_install", "get_installed_details", "preview_backup_cleanup", "set_setting", "set_startup_enabled", "set_catalog_entry_enabled", "add_catalog_entry", "remove_catalog_entry", "restore_catalog_entry", "preview_github_url", "add_project_reference", "remove_project_reference"];

let writeText: ReturnType<typeof vi.fn>;
let fetchSpy: ReturnType<typeof vi.fn>;
beforeEach(() => {
  backend = createBackend();
  writeText = vi.fn();
  fetchSpy = vi.fn();
  Object.defineProperty(navigator, "clipboard", { value: { writeText, readText: vi.fn() }, configurable: true });
  vi.stubGlobal("fetch", fetchSpy);
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const calls = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd);
const region = async () => within(await screen.findByRole("region", { name: "Proje Araç Matrisi" }));
const rowOf = async (name: string) => {
  const r = await region();
  return within((await r.findByRole("heading", { level: 3, name })).closest("article") as HTMLElement);
};

describe("the Codex CLI preview control: a TOML target takes no pasted text and shows only the honest status and static guidance", () => {
  const renderCodex = (props: Partial<Parameters<typeof ConfigPreview>[0]> = {}) =>
    render(<ConfigPreview entry={mcp()} alias="benim-sitem" projectName="Benim sitem" platform={WIN} surface={CODEX} {...props} />);

  it("has no mode choice and no text field, says the file is TOML, and shows TOML_PREVIEW_NOT_IMPLEMENTED with the manual steps", async () => {
    renderCodex();
    await userEvent.click(screen.getByText(/Yapılandırma önizlemesi — salt okunur/));
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.getByRole("note")).toHaveTextContent(/TOML/);
    expect(screen.getByRole("note")).toHaveTextContent(/\.codex\/config\.toml/);
    expect(screen.getByRole("note")).toHaveTextContent(/yapıştırma alanı bu yüzden sunulmaz/);
    await userEvent.click(screen.getByRole("button", { name: /^Değişiklik önizle/ }));
    const result = await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ });
    const r = within(result);
    expect(r.getByText("TOML_PREVIEW_NOT_IMPLEMENTED")).toBeInTheDocument();
    expect(r.getByText(".codex/config.toml")).toBeInTheDocument();
    expect(r.getByText("TOML")).toBeInTheDocument();
    expect(r.getByText("Codex CLI")).toBeInTheDocument();
    expect(r.getByRole("list", { name: "Elle yönerge" }).children.length).toBeGreaterThanOrEqual(5);
    expect(result).toHaveFocus();
    expect(result).toHaveTextContent(/hiçbir metin de işlenmedi/);
    // no before/after/diff panes: nothing was composed
    expect(r.queryByLabelText("Önce")).not.toBeInTheDocument();
    expect(r.queryByLabelText("Sonra")).not.toBeInTheDocument();
    expect(r.queryByLabelText("Odaklı fark")).not.toBeInTheDocument();
    // the JSON file is only ever named to say that this is NOT it
    expect(result).not.toHaveTextContent(/mcpServers/);
    expect(result.textContent!.match(/\.mcp\.json/g) ?? []).toHaveLength(1);
    expect(result.textContent).toMatch(/\.mcp\.json veya Antigravity'nin JSON dosyası DEĞİLDİR/);
  });

  it("offers no apply, save, copy, export or download control, in any state", async () => {
    renderCodex();
    await userEvent.click(screen.getByText(/Yapılandırma önizlemesi — salt okunur/));
    await userEvent.click(screen.getByRole("button", { name: /^Değişiklik önizle/ }));
    await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ });
    const names = screen.getAllByRole("button").map((b) => b.textContent ?? "");
    expect(names).toHaveLength(1);
    for (const bad of [/uygula\b/i, /kaydet/i, /kopyala/i, /dışa aktar/i, /indir/i, /yaz\b/i, /kur\b/i, /çalıştır/i, /bağlan/i]) expect(names.join(" ")).not.toMatch(bad);
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
  });

  it("makes no IPC call, no clipboard write and no network request", async () => {
    renderCodex();
    await userEvent.click(screen.getByText(/Yapılandırma önizlemesi — salt okunur/));
    await userEvent.click(screen.getByRole("button", { name: /^Değişiklik önizle/ }));
    await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ });
    expect(backend.calls).toHaveLength(0);
    expect(writeText).not.toHaveBeenCalled();
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("only MCP entries get the control, also for Codex", () => {
    renderCodex({ entry: plain() });
    expect(screen.queryByText(/Yapılandırma önizlemesi/)).not.toBeInTheDocument();
  });

  it("the JSON surfaces keep their controls (mode choice, text field) exactly as before", async () => {
    for (const surface of [CC, AG]) {
      const { unmount } = render(<ConfigPreview entry={mcp()} alias="benim-sitem" projectName="Benim sitem" platform={WIN} surface={surface} />);
      await userEvent.click(screen.getByText(/Yapılandırma önizlemesi — salt okunur/));
      expect(screen.getAllByRole("radio")).toHaveLength(2);
      await userEvent.click(screen.getByRole("radio", { name: /Var olan dosyanın metnini/ }));
      expect(screen.getByRole("textbox")).toBeInTheDocument();
      unmount();
    }
  });

  it("changing the surface withdraws a shown Codex result (stale invalidation), and the JSON result is withdrawn when switching to Codex", async () => {
    const { rerender } = renderCodex();
    await userEvent.click(screen.getByText(/Yapılandırma önizlemesi — salt okunur/));
    await userEvent.click(screen.getByRole("button", { name: /^Değişiklik önizle/ }));
    await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ });
    rerender(<ConfigPreview entry={mcp()} alias="benim-sitem" projectName="Benim sitem" platform={WIN} surface={CC} />);
    expect(screen.queryByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ })).not.toBeInTheDocument();
    expect(screen.getByText(/önceki önizleme eskidi ve gizlendi/)).toBeInTheDocument();
    rerender(<ConfigPreview entry={mcp()} alias="benim-sitem" projectName="Benim sitem" platform={MAC} surface={CODEX} />);
    expect(screen.queryByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ })).not.toBeInTheDocument();
  });
});

describe("Codex CLI in the real matrix: its own target, its own decisions, never another target's", () => {
  beforeEach(() => {
    backend.entries = [mcp(), plain()];
    backend.projects = [project([wish("synthetic-mcp", "accepted"), wish("gitleaks", "rejected", { agentSurface: CLI })])];
  });

  const selectSurface = async (label: RegExp | string) => {
    const r = await region();
    await userEvent.selectOptions(r.getByLabelText("Ajan yüzeyi"), label as string);
    return r;
  };

  it("lists Codex CLI as a fifth, separate surface next to the existing four", async () => {
    render(<ProjectsPage />);
    const r = await region();
    const options = within(r.getByLabelText("Ajan yüzeyi")).getAllByRole("option").map((o) => [o.getAttribute("value"), o.textContent]);
    expect(options).toEqual([
      [CC, "Claude Code (terminal)"],
      ["claude-desktop-code", "Claude Desktop (Code)"],
      [AG, "Antigravity IDE"],
      [CODEX, "Codex CLI"],
      [CLI, "Yerel komut satırı"],
    ]);
  });

  it("shows no decision of another surface under Codex, keeps those decisions intact, and says compatibility is unknown", async () => {
    render(<ProjectsPage />);
    await rowOf("Sentetik MCP");
    const before = JSON.stringify(backend.projects[0].preferences);
    const r = await selectSurface(CODEX);
    const row = await rowOf("Sentetik MCP");
    expect(row.getByText("Henüz karar yok")).toBeInTheDocument();
    expect(r.getByText(/Hedef:/).textContent).toMatch(/Codex CLI/);
    expect(row.getAllByText(/unknown|bilinmiyor/i).length).toBeGreaterThan(0);
    expect(JSON.stringify(backend.projects[0].preferences)).toBe(before);
    // the Claude Code wish is exactly where it was
    await selectSurface(CC);
    const back = await rowOf("Sentetik MCP");
    expect(back.queryByText("Henüz karar yok")).not.toBeInTheDocument();
  });

  it("'kullanmak istiyorum' is not selectable for Codex while its compatibility is unknown; postponing is, and is stored for exactly Codex + the chosen OS", async () => {
    render(<ProjectsPage />);
    await rowOf("Sentetik MCP");
    await selectSurface(CODEX);
    const row = await rowOf("Sentetik MCP");
    const select = row.getByLabelText(/^Karar/) as HTMLSelectElement;
    expect(within(select).getByRole("option", { name: /Bu projede kullanmak istiyorum \(şu an seçilemez\)$/ })).toBeDisabled();
    await userEvent.selectOptions(select, "postponed");
    await userEvent.click(row.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ }));
    await waitFor(() => expect(calls("set_project_preference")).toHaveLength(1));
    expect(calls("set_project_preference")[0].args).toMatchObject({ alias: "benim-sitem", entryId: "synthetic-mcp", agentSurface: CODEX, platformTarget: WIN, decision: "postponed" });
    const stored = backend.projects[0].preferences.filter((p) => p.entryId === "synthetic-mcp");
    expect(stored.map((p) => [p.agentSurface, p.platformTarget, p.decision]).sort()).toEqual([[CC, WIN, "accepted"], [CODEX, WIN, "postponed"]].sort());
    for (const bad of FORBIDDEN_WRITES) expect(calls(bad), bad).toHaveLength(0);
  });

  it("a hand-off for codex-cli opens the Codex target, not Claude Code, Antigravity or the generic command line", async () => {
    render(<ProjectsPage handoff={{ alias: "benim-sitem", platform: WIN, surface: CODEX, entryId: "synthetic-mcp", nonce: 1 }} />);
    await rowOf("Sentetik MCP");
    const r = await region();
    expect((r.getByLabelText("Ajan yüzeyi") as HTMLSelectElement).value).toBe(CODEX);
    expect(r.getByText(/Hedef:/).textContent).toMatch(/Codex CLI/);
  });

  it("a Codex row's configuration control is the TOML one; the same entry under Claude Code keeps the JSON one", async () => {
    render(<ProjectsPage />);
    await rowOf("Sentetik MCP");
    await selectSurface(CODEX);
    let row = await rowOf("Sentetik MCP");
    await userEvent.click(row.getByText(/Yapılandırma önizlemesi — salt okunur/));
    expect(row.queryByRole("radio")).not.toBeInTheDocument();
    await selectSurface(CC);
    row = await rowOf("Sentetik MCP");
    expect(row.getByText(/Yapılandırma önizlemesi — salt okunur/)).toBeInTheDocument();
  });
});

describe("target resolution, counts and the review queue never substitute a target for Codex", () => {
  it("resolveTarget keeps codex-cli, and an unknown spelling falls back loudly to the first surface - never to Codex or the command line", () => {
    const projects = [project()];
    const ok = resolveTarget({ alias: "benim-sitem", platform: WIN, surface: CODEX }, projects);
    expect([ok.surface, ok.exact, ok.notes]).toEqual([CODEX, true, []]);
    for (const bad of ["codex", "Codex-CLI", "codex-desktop", "gemini-cli", "deepseek", "groq", "grok"]) {
      const r = resolveTarget({ alias: "benim-sitem", platform: WIN, surface: bad }, projects);
      expect(r.surface, bad).toBe(TARGET_SURFACES[0]);
      expect(r.exact, bad).toBe(false);
      expect(r.notes.join(" "), bad).toMatch(/tanınmıyor/);
    }
  });

  it("targetCounts places Codex decisions in their own bucket, in canonical order, and counts nothing as unreachable", () => {
    const p = project([wish("a", "rejected", { agentSurface: CLI }), wish("b", "rejected", { agentSurface: CODEX }), wish("c", "rejected", { agentSurface: CC }), wish("d", "rejected", { agentSurface: CODEX }), wish("e", "rejected", { agentSurface: AG })]);
    const t = targetCounts(p);
    expect(t.targets.map((x) => [x.surface, x.count])).toEqual([[CC, 1], [AG, 1], [CODEX, 2], [CLI, 1]]);
    expect(t.unreachable).toBe(0);
    expect(t.total).toBe(5);
  });

  it("the review queue treats Codex as its own four-field key: Claude Code evidence is never attributed to it", () => {
    const projects = [project([wish("alfa", "accepted"), wish("alfa", "accepted", { agentSurface: CODEX })])];
    const request = makeRequest({ entryId: "alfa", projectAlias: "benim-sitem", platformTarget: WIN, agentSurface: CC, status: "not_scanned", origin: originOf() });
    const inventory: Inventory = { ...emptyInventory(), requests: [request] };
    const q = deriveReview({ data: projects, error: null, loading: false }, { data: inventory, error: null, loading: false }, Date.UTC(2026, 9, 24));
    const all = [...q.items, ...q.uncertain];
    expect(all).toHaveLength(2);
    const codex = all.find((i) => i.surface === CODEX)!;
    expect(codex.openable).toBe(true);
    expect(codex.uncertain).toEqual(["no_row"]);
    expect(q.items.map((i) => i.surface)).toEqual([CC]);
    expect(new Set(all.map((i) => i.key)).size).toBe(2);
  });
});

describe("the installer-side plan page does not offer Codex CLI (a project target is not an installer target)", () => {
  it("lists the four historic surfaces only and never sends codex-cli to the plan command", async () => {
    render(<PlanPage />);
    const select = await screen.findByLabelText("Hedef ajan yüzeyi");
    const values = within(select).getAllByRole("option").map((o) => o.getAttribute("value"));
    expect(values).toEqual([CC, "claude-desktop-code", AG, CLI]);
    expect(values).not.toContain(CODEX);
    expect(calls("plan_install")).toHaveLength(0);
  });
});

describe("the shared manual guide on the Projects page", () => {
  beforeEach(() => {
    backend.entries = [mcp()];
    backend.projects = [project()];
  });
  const openGuide = async () => {
    render(<ProjectsPage />);
    await userEvent.click(await screen.findByText(/Diğer ajanlar ve MCP\/Skills kullanım kılavuzu/));
    return within(document.querySelector("details.agent-guide") as HTMLElement);
  };

  it("states that it is a guide, the six-step workflow, the no-universal-format rule and the registry of managed surfaces", async () => {
    const g = await openGuide();
    expect(g.getByText(/dosya okumaz veya yazmaz, ağa bağlanmaz, hesap veya anahtar istemez/)).toBeInTheDocument();
    expect(g.getByText(/tek bir ortak settings\.json/)).toBeInTheDocument();
    const steps = g.getAllByRole("listitem").map((li) => li.textContent ?? "");
    for (const needle of ["Adı sınıflandırın", "Resmî desteği doğrulayın", "Farklı şeyleri birbirinden ayırın", "Kapsamı ve riski belirleyin", "Kanıt eksikse MANUAL_VERIFY", "Örnekler yalnızca açıklayıcıdır"]) {
      expect(steps.some((s) => s.includes(needle)), needle).toBe(true);
    }
    const managed = g.getByRole("list", { name: /Seçilebilir ajan yüzeyleri/ });
    expect(within(managed).getAllByRole("listitem")).toHaveLength(4); // claude-code, desktop, antigravity, codex (the generic program lookup is not an agent)
    expect(within(managed).getByText(".codex/config.toml")).toBeInTheDocument();
    expect(within(managed).getByText("TOML")).toBeInTheDocument();
    expect(within(managed).getAllByText(/TOML önizlemesi uygulanmadı/).length).toBe(1);
  });

  it("classifies Grok and Groq separately, Gemini CLI as a host and Gemini API as a provider, and anything else as MANUAL_VERIFY", async () => {
    const g = await openGuide();
    const ask = async (name: string) => {
      const box = g.getByLabelText(/^Ad \(/);
      await userEvent.clear(box);
      await userEvent.type(box, name);
      await userEvent.click(g.getByRole("button", { name: /^Sınıflandır/ }));
      return within(g.getByRole("status"));
    };
    let s = await ask("Grok");
    expect(s.getByText("KNOWN_EXAMPLE")).toBeInTheDocument();
    expect(s.getByText(/Model \/ API sağlayıcısı/)).toBeInTheDocument();
    expect(s.getByText(/Groq ile aynı değildir/)).toBeInTheDocument();
    s = await ask("Groq");
    expect(s.getByText(/Grok \(xAI\) ile aynı değildir/)).toBeInTheDocument();
    s = await ask("Gemini CLI");
    expect(s.getByText(/Ajan istemcisi — yalnızca örnek/)).toBeInTheDocument();
    s = await ask("Gemini API");
    expect(s.getByText(/Model \/ API sağlayıcısı/)).toBeInTheDocument();
    s = await ask("DeepSeek");
    expect(s.getByText(/Model \/ API sağlayıcısı/)).toBeInTheDocument();
    s = await ask("Codex");
    expect(s.getByText(/uygulamada seçilebilir hedef/)).toBeInTheDocument();
    s = await ask("Cursor");
    expect(s.getByText("MANUAL_VERIFY")).toBeInTheDocument();
    expect(s.getByText(/Belirsiz/)).toBeInTheDocument();
    s = await ask("gemini");
    expect(s.getByText("MANUAL_VERIFY")).toBeInTheDocument();
  });

  it("is read-only: no IPC beyond the page's own reads, no clipboard, no network, one text box, one button", async () => {
    const g = await openGuide();
    expect(g.getAllByRole("textbox")).toHaveLength(1);
    expect(g.getAllByRole("button")).toHaveLength(1);
    await userEvent.type(g.getByLabelText(/^Ad \(/), "Groq");
    await userEvent.click(g.getByRole("button", { name: /^Sınıflandır/ }));
    await waitFor(() => expect(backend.calls.every((c) => /^(list_|get_)/.test(c.cmd))).toBe(true));
    for (const bad of FORBIDDEN_WRITES) expect(calls(bad), bad).toHaveLength(0);
    expect(calls("set_project_preference")).toHaveLength(0);
    expect(writeText).not.toHaveBeenCalled();
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(g.queryByRole("link")).not.toBeInTheDocument();
  });

  it("never promises a config file, a compatibility or an installation for a provider", async () => {
    const g = await openGuide();
    // the only place these words may appear is the sentence that says the output is NOT "uyumlu" / "kurulu"
    const raw = (document.querySelector("details.agent-guide") as HTMLElement).textContent ?? "";
    const text = raw.replace("ne yapılandırma farkı, ne “uyumlu”, ne “kurulu”.", "");
    expect(text.length).toBeLessThan(raw.length);
    expect(text).toMatch(/Gemini CLI/);
    expect(text).not.toMatch(/\buyumlu\b|\bkurulu\b|çalışıyor|\bdesteklenir\b/);
    expect(g.getByText(/Marka adı bunu belirlemez/)).toBeInTheDocument();
  });
});

describe("no new privilege, no new I/O", () => {
  it("the Tauri capability still grants no filesystem, shell, HTTP, dialog, clipboard or configuration permission", () => {
    const permissions = (JSON.parse(capabilitySource) as { permissions: string[] }).permissions;
    expect(permissions.length).toBeGreaterThan(20);
    // CONFIG-02C (Issue #40, owner-authorized): exactly ONE permission names the one-file read command; nothing else may name a configuration, MCP or Codex command.
    expect(permissions.filter((p) => p === "allow-read-project-mcp-config")).toHaveLength(1);
    for (const p of permissions.filter((x) => x !== "allow-read-project-mcp-config")) {
      expect(p, p).not.toMatch(/fs|shell|http|opener|dialog|clipboard|process|os:|path|window/i);
      expect(p, p).not.toMatch(/config|mcp|codex|write-file|apply/i);
    }
  });

  it("the new modules contain no IPC, file, clipboard, network, storage, timer or process access", () => {
    const forbidden = /invoke\(|@tauri-apps|\bapi\.|fetch\(|XMLHttpRequest|WebSocket|navigator\.clipboard|localStorage|sessionStorage|indexedDB|setTimeout|setInterval|createObjectURL|FileReader|showSaveFilePicker|showOpenFilePicker|window\.open|child_process|require\(|import\.meta\.env|process\.env/;
    for (const [name, source] of [["agentHosts.ts", agentHostsSource], ["agentGuide.ts", agentGuideSource], ["AgentGuide.tsx", guideComponentSource]] as const) {
      expect(source, name).not.toMatch(forbidden);
    }
  });
});
