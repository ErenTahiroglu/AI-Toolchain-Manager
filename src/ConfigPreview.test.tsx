// CONFIG-01 - "Yapılandırma önizlemesi" in a matrix row: read-only, no file read or written, no IPC, no clipboard, no network, no apply, no wish. Mocked IPC and
// jsdom only: no real window, no real file, no agent. All "secrets" are obviously fake sentinels; the registry of reviewed declarations the application
// ships is EMPTY, so every positive path below injects a synthetic one.
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import capabilitySource from "../src-tauri/capabilities/default.json?raw";
import { ConfigPreview } from "./components/ConfigPreview";
import previewSource from "./components/ConfigPreview.tsx?raw";
import { REVIEWED_DECLARATIONS, type ReviewedDeclaration } from "./configDeclarations";
import { DRAFT_NOTICE } from "./configPlan";
import planSource from "./configPlan.ts?raw";
import declarationsSource from "./configDeclarations.ts?raw";
import { ProjectsPage } from "./pages/Projects";
import { createBackend, makeEntry, type Backend } from "./test/backend";
import type { CatalogEntry } from "./types";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const WIN = "windows-x64";
const MAC = "macos-arm64";
const CC = "claude-code-terminal";
const AG = "antigravity-ide";
const SENTINEL = "ghp_SENTINELSENTINELSENTINELSENTINEL12";
const PIN = "0123456789abcdef0123456789abcdef01234567";
const ev = (platformTarget: string, agentSurface: string, compatibilityState: string) => ({ platformTarget, agentSurface, compatibilityState, limitation: "kayıt" });

const mcp = (over: Partial<CatalogEntry> = {}) =>
  makeEntry("synthetic-mcp", { displayName: "Sentetik MCP", kind: "agent_plugin", record: { category: "mcp-server", sourceSnapshot: { immutableCommit: PIN }, targetEvidence: [ev(WIN, CC, "vendor-declared"), ev(MAC, AG, "vendor-declared")] }, ...over });
const DECLS: ReviewedDeclaration[] = [
  { hostId: "claude-code-project-mcp", entryId: "synthetic-mcp", serverName: "synthetic", declaration: { command: "npx", args: ["-y", "@example/synthetic-mcp@1.2.3"] }, review: { reviewId: "REV-SYN-1", evidenceIds: ["R99"], reviewedAgainst: PIN } },
  { hostId: "antigravity-workspace-mcp", entryId: "synthetic-mcp", serverName: "synthetic", declaration: { serverUrl: "https://synthetic.example.invalid/mcp" }, review: { reviewId: "REV-SYN-2", evidenceIds: ["R99"], reviewedAgainst: PIN } },
];
const FORBIDDEN = ["run_discovery_scan", "evaluate_task", "set_project_preference", "reset_project_preference", "prepare_install", "confirm_install", "cancel_install", "recover_installations", "plan_install", "get_installed_details", "preview_backup_cleanup", "set_setting", "set_startup_enabled", "set_catalog_entry_enabled", "add_catalog_entry", "remove_catalog_entry", "restore_catalog_entry", "preview_github_url", "add_project_reference", "remove_project_reference"];

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
const renderPreview = (props: Partial<Parameters<typeof ConfigPreview>[0]> = {}) =>
  render(<ConfigPreview entry={mcp()} alias="benim-sitem" projectName="Benim sitem" platform={WIN} surface={CC} declarations={DECLS} {...props} />);
const openDetails = async () => {
  await userEvent.click(screen.getByText(/Yapılandırma önizlemesi — salt okunur/));
};
const previewButton = () => screen.getByRole("button", { name: /^Değişiklik önizle/ });
const result = () => screen.queryByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ });

describe("the control: nothing is chosen, nothing happens, nothing is applied", () => {
  it("renders nothing for an entry that is not an MCP server", () => {
    const { container } = renderPreview({ entry: makeEntry("gitleaks") });
    expect(container).toBeEmptyDOMElement();
  });

  it("starts closed, with no mode chosen and no result; the only button is 'Değişiklik önizle'; there is no apply, save, copy or download", async () => {
    renderPreview();
    expect(screen.getByRole("group", { hidden: true, name: /Mevcut yapılandırma/ })).toBeInTheDocument();
    await openDetails();
    expect(screen.getByRole("radio", { name: /Yeni dosya/ })).not.toBeChecked();
    expect(screen.getByRole("radio", { name: /metnini ben yapıştırıyorum/ })).not.toBeChecked();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(result()).not.toBeInTheDocument();
    expect(screen.getAllByRole("button").map((b) => b.textContent?.replace(/\s+/g, " ").trim())).toEqual(["Değişiklik önizle (Sentetik MCP; hiçbir dosyaya yazmaz)"]);
    expect(screen.queryByRole("button", { name: /Uygula|Kaydet|Yaz|Kopyala|İndir|Dışa aktar|Kur|Etkinleştir/ })).not.toBeInTheDocument();
    expect(screen.getByText(/okumaz ve yazmaz/)).toBeInTheDocument();
  });

  it("asking for a preview before choosing the current configuration is refused with guidance and focus, and produces nothing", async () => {
    renderPreview();
    await openDetails();
    await userEvent.click(previewButton());
    expect(screen.getByText(/Önce mevcut yapılandırmayı seçin/)).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /Yeni dosya/ })).toHaveFocus();
    expect(result()).not.toBeInTheDocument();
  });
});

describe("a preview of a new file with a reviewed declaration", () => {
  it("shows the status, the exact identity and path, before and after, the focused diff and the no-file-touched statement - and no apply", async () => {
    renderPreview();
    await openDetails();
    await userEvent.click(screen.getByRole("radio", { name: /Yeni dosya/ }));
    await userEvent.click(previewButton());
    const region = within(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ }));
    expect(region.getByText("PREVIEW_ONLY_NOT_APPLIED")).toBeInTheDocument();
    expect(region.getByText(/YALNIZCA ÖNİZLEME — UYGULANMADI/)).toBeInTheDocument();
    expect(region.getByText(/Hiçbir dosyaya dokunulmadı ve hiçbir dosya okunmadı/)).toBeInTheDocument();
    expect(region.getByText("Benim sitem")).toBeInTheDocument();
    expect(region.getByText("Windows x64")).toBeInTheDocument();
    expect(region.getByText("Claude Code (terminal)")).toBeInTheDocument();
    expect(region.getByText(".mcp.json")).toBeInTheDocument();
    expect(region.getByText(/Proje \(paylaşılan\)/)).toBeInTheDocument();
    expect(region.getByLabelText("Önce")).toHaveTextContent("(dosya yok — yeni dosya)");
    expect(region.getByLabelText("Sonra")).toHaveTextContent('"synthetic"');
    expect(region.getByLabelText("Sonra")).toHaveTextContent("@example/synthetic-mcp@1.2.3");
    expect(region.getByLabelText("Odaklı fark").textContent).toMatch(/\+ {3}"mcpServers": \{/);
    expect(within(region.getByLabelText("Odaklı fark")).queryAllByText(/^- /)).toHaveLength(0);
    expect(region.getByText(/Bu önizlemenin hiçbir zaman dokunmadığı dosyalar/)).toHaveTextContent("CLAUDE.md");
    expect(screen.queryByRole("button", { name: /Uygula|Kaydet|Yaz|Kopyala|İndir|Kur/ })).not.toBeInTheDocument();
  });

  it("the result takes focus so a keyboard or screen-reader user lands on it", async () => {
    renderPreview();
    await openDetails();
    await userEvent.click(screen.getByRole("radio", { name: /Yeni dosya/ }));
    await userEvent.click(previewButton());
    expect(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ })).toHaveFocus();
  });

  it("works from the keyboard alone: radio with Space, textarea typed into, button with Enter", async () => {
    renderPreview();
    await openDetails();
    screen.getByRole("radio", { name: /metnini ben yapıştırıyorum/ }).focus();
    await userEvent.keyboard(" ");
    const box = await screen.findByRole("textbox", { name: /Dosya metni/ });
    box.focus();
    await userEvent.keyboard('{{"mcpServers":{{}}');
    previewButton().focus();
    await userEvent.keyboard("{Enter}");
    const region = within(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ }));
    expect(region.getByText("PREVIEW_ONLY_NOT_APPLIED")).toBeInTheDocument();
  });
});

describe("pasted text: preserved, validated, masked, never kept", () => {
  const paste = async (value: string) => {
    await userEvent.click(screen.getByRole("radio", { name: /metnini ben yapıştırıyorum/ }));
    const box = screen.getByRole("textbox", { name: /Dosya metni/ });
    await userEvent.click(box);
    await userEvent.paste(value);
  };

  it("an existing file keeps its other servers and keys; only the new stanza is added", async () => {
    renderPreview();
    await openDetails();
    await paste(JSON.stringify({ mcpServers: { other: { command: "node", args: ["a.js"] } }, keep: { me: true } }));
    await userEvent.click(previewButton());
    const region = within(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ }));
    expect(region.getByLabelText("Önce")).toHaveTextContent('"other"');
    expect(region.getByLabelText("Sonra")).toHaveTextContent('"other"');
    expect(region.getByLabelText("Sonra")).toHaveTextContent('"synthetic"');
    expect(region.getByLabelText("Sonra")).toHaveTextContent('"keep"');
    expect(region.getByText(/Mevcut metinde tanımlı sunucu adları/)).toHaveTextContent("other");
    expect(region.getByText(/verdiğiniz metnin SHA-256 özeti/i)).toBeInTheDocument();
  });

  it("a secret in the pasted file never appears on screen - not in the textbox echo of the result, not in the digest line, not in an error", async () => {
    renderPreview();
    await openDetails();
    await paste(JSON.stringify({ mcpServers: { other: { command: "node", env: { API_TOKEN: SENTINEL } } } }));
    await userEvent.click(previewButton());
    const region = await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ });
    expect(region.textContent).not.toContain(SENTINEL);
    expect(region.textContent).toContain("«gizlendi»");
    expect(within(region).getByText(/değer gizli olabileceği için maskelendi/)).toBeInTheDocument();
    // a broken file that contains a secret: the engine's message is not shown either
    await userEvent.clear(screen.getByRole("textbox", { name: /Dosya metni/ }));
    await userEvent.click(screen.getByRole("textbox", { name: /Dosya metni/ }));
    await userEvent.paste(`{"token": "${SENTINEL}" oops`);
    await userEvent.click(previewButton());
    const broken = await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ });
    expect(within(broken).getByText("INVALID_CURRENT_CONFIG")).toBeInTheDocument();
    expect(broken.textContent).not.toContain(SENTINEL);
    expect(broken.textContent).not.toContain("oops");
  });

  it("malformed JSON, comments, duplicate keys and wrong types are refused with a plain reason and no before/after", async () => {
    renderPreview();
    await openDetails();
    for (const [value, reason] of [
      ["{", /geçerli bir JSON değil/],
      ['{"a":1} // yorum', /JSONC/],
      ['{"mcpServers":{},"mcpServers":{}}', /yinelenen anahtar/],
      ['{"mcpServers":[]}', /“mcpServers” bir nesne olmalı/],
      ['{"mcpServers":{"a":"x"}}', /nesne değil/],
    ] as const) {
      await userEvent.click(screen.getByRole("radio", { name: /metnini ben yapıştırıyorum/ }));
      const box = screen.getByRole("textbox", { name: /Dosya metni/ });
      await userEvent.clear(box);
      await userEvent.click(box);
      await userEvent.paste(value);
      await userEvent.click(previewButton());
      const region = within(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ }));
      expect(region.getByText("INVALID_CURRENT_CONFIG"), value).toBeInTheDocument();
      expect(region.getByLabelText("Sorunlar")).toHaveTextContent(reason);
      expect(region.queryByLabelText("Sonra")).not.toBeInTheDocument();
    }
  });

  it("an existing server of the same name is a conflict that is shown, never overwritten", async () => {
    renderPreview();
    await openDetails();
    await paste(JSON.stringify({ mcpServers: { synthetic: { command: "node" } } }));
    await userEvent.click(previewButton());
    const region = within(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ }));
    expect(region.getByText("CONFLICT")).toBeInTheDocument();
    expect(region.getByLabelText("Sorunlar")).toHaveTextContent("üzerine yazılmaz");
    expect(region.queryByLabelText("Sonra")).not.toBeInTheDocument();
  });
});

describe("a stale preview is withdrawn", () => {
  it("editing the pasted text, switching the mode or changing the target hides the result and asks for a new preview", async () => {
    const { rerender } = renderPreview();
    await openDetails();
    await userEvent.click(screen.getByRole("radio", { name: /Yeni dosya/ }));
    await userEvent.click(previewButton());
    expect(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ })).toBeInTheDocument();
    // mode changes
    await userEvent.click(screen.getByRole("radio", { name: /metnini ben yapıştırıyorum/ }));
    expect(result()).not.toBeInTheDocument();
    expect(screen.getByText(/önceki önizleme eskidi ve gizlendi/)).toBeInTheDocument();
    // a new preview, then the text changes
    const box = screen.getByRole("textbox", { name: /Dosya metni/ });
    await userEvent.click(box);
    await userEvent.paste("{}");
    await userEvent.click(previewButton());
    expect(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ })).toBeInTheDocument();
    expect(screen.queryByText(/eskidi/)).not.toBeInTheDocument();
    await userEvent.type(box, " ");
    expect(result()).not.toBeInTheDocument();
    expect(screen.getByText(/eskidi/)).toBeInTheDocument();
    // an explicit new request makes a new preview from the current inputs
    await userEvent.click(previewButton());
    expect(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ })).toBeInTheDocument();
    // the project / OS / surface / entry of the row change
    rerender(<ConfigPreview entry={mcp()} alias="benim-sitem" projectName="Benim sitem" platform={MAC} surface={CC} declarations={DECLS} />);
    expect(result()).not.toBeInTheDocument();
    expect(screen.getByText(/eskidi/)).toBeInTheDocument();
  });
});

describe("blocked and unsupported states", () => {
  it("an entry the application's own registry does not hold: BLOCKED_INSUFFICIENT_EVIDENCE with the reason, no before/after, no diff", async () => {
    expect(REVIEWED_DECLARATIONS.map((d) => d.entryId)).toEqual(["exa-mcp-server"]); // CONFIG-02A: the registry holds exactly one real entry, not this synthetic one
    renderPreview({ declarations: undefined });
    await openDetails();
    await userEvent.click(screen.getByRole("radio", { name: /Yeni dosya/ }));
    await userEvent.click(previewButton());
    const region = within(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ }));
    expect(region.getByText("BLOCKED_INSUFFICIENT_EVIDENCE")).toBeInTheDocument();
    expect(region.getByLabelText("Engeller")).toHaveTextContent(/onaylanmadı/);
    expect(region.getByLabelText("Engeller")).toHaveTextContent(/uydurulmaz/);
    expect(region.queryByLabelText("Sonra")).not.toBeInTheDocument();
    expect(region.queryByLabelText("Odaklı fark")).not.toBeInTheDocument();
    expect(region.getByText(/Hiçbir dosyaya dokunulmadı/)).toBeInTheDocument();
  });

  it("a quarantined entry is shown only as an explanatory preview that cannot be mistaken for a recipe", async () => {
    renderPreview({ entry: mcp({ reviewState: "quarantined", executionPolicy: "blocked-until-scoped-authorization" }) });
    await openDetails();
    await userEvent.click(screen.getByRole("radio", { name: /Yeni dosya/ }));
    await userEvent.click(previewButton());
    const region = within(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ }));
    expect(region.getByText("BLOCKED_BY_CATALOG")).toBeInTheDocument();
    expect(region.getByText(/AÇIKLAYICI — bu bir tarif değildir/)).toBeInTheDocument();
    expect(region.getByLabelText("Engeller")).toHaveTextContent(/Kapsamlı yazılı yetki olmadan/);
    expect(region.getByLabelText("Engeller")).toHaveTextContent(/quarantined/);
    expect(screen.queryByRole("button", { name: /Uygula|Kopyala/ })).not.toBeInTheDocument();
  });

  it("CONFIG-02A: unknown compatibility for the exact OS + surface no longer blocks the TEXT draft, and says plainly that the draft is not an acceptance", async () => {
    renderPreview({ platform: MAC, surface: CC }); // the fixture declares Windows x Claude Code and macOS x Antigravity only: macOS x Claude Code is unknown
    await openDetails();
    await userEvent.click(screen.getByRole("radio", { name: /Yeni dosya/ }));
    await userEvent.click(previewButton());
    const region = within(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ }));
    expect(region.getByText("PREVIEW_ONLY_NOT_APPLIED")).toBeInTheDocument();
    expect(region.queryByLabelText("Engeller")).not.toBeInTheDocument();
    expect(region.getByText(DRAFT_NOTICE)).toBeInTheDocument();
    expect(screen.getByText(/Bu bir yapılandırma TASLAĞIDIR/)).toBeInTheDocument();
  });

  it("an explicitly UNSUPPORTED combination still blocks the draft", async () => {
    const entry = mcp({ record: { category: "mcp-server", sourceSnapshot: { immutableCommit: PIN }, targetEvidence: [ev(MAC, CC, "unsupported")] } });
    renderPreview({ entry, platform: MAC, surface: CC });
    await openDetails();
    await userEvent.click(screen.getByRole("radio", { name: /Yeni dosya/ }));
    await userEvent.click(previewButton());
    const region = within(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ }));
    expect(region.getByText("BLOCKED_BY_CATALOG")).toBeInTheDocument();
    expect(region.getByLabelText("Engeller")).toHaveTextContent(/desteklenmiyor/);
    expect(region.queryByText(DRAFT_NOTICE)).not.toBeInTheDocument();
  });

  it("Claude Desktop (Code) and the generic command line are UNSUPPORTED_SURFACE - no file is guessed", async () => {
    for (const surface of ["claude-desktop-code", "local-cli"]) {
      const { unmount } = renderPreview({ surface });
      await openDetails();
      await userEvent.click(screen.getByRole("radio", { name: /Yeni dosya/ }));
      await userEvent.click(previewButton());
      const region = within(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ }));
      expect(region.getByText("UNSUPPORTED_SURFACE"), surface).toBeInTheDocument();
      expect(region.getByText(/dosyası tahmin edilmez/)).toBeInTheDocument();
      expect(region.queryByText(".mcp.json")).not.toBeInTheDocument();
      expect(region.queryByText(".agents/mcp_config.json")).not.toBeInTheDocument();
      unmount();
    }
  });

  it("Antigravity IDE gets its own workspace path and says the user-global file and other Antigravity interfaces are not touched", async () => {
    renderPreview({ platform: MAC, surface: AG });
    await openDetails();
    await userEvent.click(screen.getByRole("radio", { name: /Yeni dosya/ }));
    await userEvent.click(previewButton());
    const region = within(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ }));
    expect(region.getByText(".agents/mcp_config.json")).toBeInTheDocument();
    expect(region.getByText(/Çalışma alanı: çalışma alanı klasöründeki/)).toBeInTheDocument();
    expect(region.getByText(/Bu önizlemenin hiçbir zaman dokunmadığı dosyalar/)).toHaveTextContent("~/.gemini/config/mcp_config.json");
    expect(region.getByLabelText("Sonra")).toHaveTextContent("serverUrl");
  });
});

describe("no side effects: no IPC, clipboard, network, storage or timers", () => {
  it("a full session of previews makes no IPC call at all, touches no clipboard, fetch or storage", async () => {
    const before = backend.calls.length;
    renderPreview();
    await openDetails();
    await userEvent.click(screen.getByRole("radio", { name: /metnini ben yapıştırıyorum/ }));
    await userEvent.click(screen.getByRole("textbox", { name: /Dosya metni/ }));
    await userEvent.paste(JSON.stringify({ mcpServers: { a: { command: "node", env: { SECRET_KEY: SENTINEL } } } }));
    await userEvent.click(previewButton());
    await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ });
    expect(backend.calls.length).toBe(before);
    expect(writeText).not.toHaveBeenCalled();
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(window.localStorage.length).toBe(0);
    expect(window.sessionStorage.length).toBe(0);
  });

  it("the new modules contain no IPC, file, clipboard, network, storage, timer, dialog or process access", () => {
    for (const [name, source] of [["ConfigPreview", previewSource], ["configPlan", planSource], ["configDeclarations", declarationsSource]] as const) {
      expect(source, name).not.toMatch(/invoke\(|@tauri-apps|\bapi\.|fetch\(|XMLHttpRequest|WebSocket|navigator\.clipboard|localStorage|sessionStorage|indexedDB|setTimeout|setInterval|createObjectURL|FileReader|showSaveFilePicker|showOpenFilePicker|window\.open|document\.execCommand|child_process|require\(/);
    }
  });

  it("the Tauri capability file still grants no filesystem, shell, HTTP, opener, dialog or clipboard permission, and no configuration command exists", () => {
    const permissions: string[] = JSON.parse(capabilitySource).permissions;
    expect(permissions.length).toBeGreaterThan(20);
    for (const p of permissions) {
      expect(p, p).not.toMatch(/fs|shell|http|opener|dialog|clipboard|process|os:|path|window/i);
      expect(p, p).not.toMatch(/config|mcp|write-file|apply/i);
    }
  });
});

describe("in the real matrix row", () => {
  beforeEach(() => {
    backend.entries = [mcp(), makeEntry("gitleaks", { displayName: "Gitleaks" })];
    backend.projects = [{ projectId: "p1", alias: "benim-sitem", displayName: "Benim sitem", kind: "reference", localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences: [] }];
  });

  const row = async (name: string) => {
    const matrix = within(await screen.findByRole("region", { name: "Proje Araç Matrisi" }));
    return within((await matrix.findByRole("heading", { level: 3, name })).closest("article") as HTMLElement);
  };

  it("only the MCP server's row has the control; it opens blocked (the shipped registry is empty), writes no wish and calls no write command", async () => {
    render(<ProjectsPage handoff={null} />);
    const mcpRow = await row("Sentetik MCP");
    const other = await row("Gitleaks");
    expect(other.queryByText(/Yapılandırma önizlemesi/)).not.toBeInTheDocument();
    expect(mcpRow.getByText(/Yapılandırma önizlemesi — salt okunur/)).toBeInTheDocument();
    expect(mcpRow.getByText("Henüz karar yok")).toBeInTheDocument();
    await userEvent.click(mcpRow.getByText(/Yapılandırma önizlemesi — salt okunur/));
    await userEvent.click(mcpRow.getByRole("radio", { name: /Yeni dosya/ }));
    await userEvent.click(mcpRow.getByRole("button", { name: /^Değişiklik önizle/ }));
    const region = within(await mcpRow.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ }));
    expect(region.getByText("BLOCKED_INSUFFICIENT_EVIDENCE")).toBeInTheDocument();
    expect(region.getByText("Benim sitem")).toBeInTheDocument();
    expect(region.getByText("synthetic-mcp")).toBeInTheDocument();
    // still no wish, no write, no scan, no install
    expect(mcpRow.getByText("Henüz karar yok")).toBeInTheDocument();
    for (const bad of FORBIDDEN) expect(calls(bad), bad).toHaveLength(0);
    await waitFor(() => expect(backend.calls.every((c) => /^(list_|get_)/.test(c.cmd))).toBe(true));
    expect(writeText).not.toHaveBeenCalled();
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("the existing decision control keeps its own rules: the preview does not unlock 'kullanmak istiyorum' for an unknown combination", async () => {
    render(<ProjectsPage handoff={null} />);
    const mcpRow = await row("Sentetik MCP");
    // the fixture is vendor-declared for Windows x Claude Code, so 'accepted' is selectable there - the preview changes nothing about that
    const select = mcpRow.getByLabelText(/^Karar/) as HTMLSelectElement;
    expect(within(select).getByRole("option", { name: /Bu projede kullanmak istiyorum$/ })).not.toBeDisabled();
    await userEvent.click(mcpRow.getByText(/Yapılandırma önizlemesi — salt okunur/));
    await userEvent.click(mcpRow.getByRole("radio", { name: /Yeni dosya/ }));
    await userEvent.click(mcpRow.getByRole("button", { name: /^Değişiklik önizle/ }));
    await mcpRow.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ });
    expect(select.value).toBe("");
    expect(calls("set_project_preference")).toHaveLength(0);
  });
});
