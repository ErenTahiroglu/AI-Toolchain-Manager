// CONFIG-02A - the first usable preview, end to end through the REAL Projects page and the application's OWN shipped declaration, for the REAL `exa-mcp-server` catalog
// record (read from the data file). Mocked IPC and jsdom only: no real window, no real file, no agent, no network. Every "secret" is an obviously fake sentinel.
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DRAFT_NOTICE } from "./configPlan";
import { ProjectsPage } from "./pages/Projects";
import { createBackend, type Backend } from "./test/backend";
import { exaEntry } from "./test/exaEntry";
import type { CatalogEntry } from "./types";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const MAC = "macos-arm64";
const SENTINEL = "ghp_SENTINELSENTINELSENTINELSENTINEL12";
const KEY = "exa-SENTINEL-KEY-0123456789abcdef";
const NAME = "Exa MCP Server";
const WRITES = ["run_discovery_scan", "evaluate_task", "set_project_preference", "reset_project_preference", "prepare_install", "confirm_install", "cancel_install", "recover_installations", "plan_install", "set_setting", "set_startup_enabled", "set_catalog_entry_enabled", "add_catalog_entry", "remove_catalog_entry", "restore_catalog_entry", "preview_github_url", "add_project_reference", "remove_project_reference"];

let writeText: ReturnType<typeof vi.fn>;
let fetchSpy: ReturnType<typeof vi.fn>;
beforeEach(() => {
  backend = createBackend();
  backend.entries = [exaEntry()];
  backend.projects = [{ projectId: "p1", alias: "benim-sitem", displayName: "Benim sitem", kind: "reference", localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences: [] }];
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
const text = (el: HTMLElement | null) => el?.textContent?.replace(/\s+/g, " ") ?? "";
/** The document's text EXCEPT the paste box (whose own value is, of course, what the user pasted). */
const outsideTheBox = () => {
  const clone = document.body.cloneNode(true) as HTMLElement;
  clone.querySelectorAll("textarea").forEach((t) => t.remove());
  return clone.textContent ?? "";
};
/** The row is re-found every time: a change of target re-creates it, and a reference to the old one would point at a detached node. */
const findRow = async () => {
  const region = within(await screen.findByRole("region", { name: "Proje Araç Matrisi" }));
  return within((await region.findByRole("heading", { level: 3, name: NAME })).closest("article") as HTMLElement);
};
const open = async (entry: CatalogEntry = exaEntry()) => {
  backend.entries = [entry];
  render(<ProjectsPage handoff={null} />);
  const region = within(await screen.findByRole("region", { name: "Proje Araç Matrisi" }));
  return { region, row: await findRow() };
};
const preview = async (row: ReturnType<typeof within>, mode: "new" | string) => {
  const details = row.getByText(/Yapılandırma önizlemesi — salt okunur/);
  if (!(details.closest("details") as HTMLDetailsElement).open) await userEvent.click(details);
  if (mode === "new") {
    await userEvent.click(row.getByRole("radio", { name: /Yeni dosya/ }));
  } else {
    await userEvent.click(row.getByRole("radio", { name: /Var olan dosyanın metnini/ }));
    const box = row.getByRole("textbox");
    await userEvent.clear(box);
    await userEvent.click(box);
    await userEvent.paste(mode);
  }
  await userEvent.click(row.getByRole("button", { name: /^Değişiklik önizle/ }));
  return within(await row.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ }));
};

describe("the real Exa entry gives a positive, inert draft in the real matrix row", () => {
  it("explicit new file: PREVIEW_ONLY_NOT_APPLIED, the exact notice, the stanza, a diff of additions only - and nothing else changes", async () => {
    const { row } = await open();
    expect(row.getByText("Henüz karar yok")).toBeInTheDocument();
    const r = await preview(row, "new");
    expect(r.getByText("PREVIEW_ONLY_NOT_APPLIED")).toBeInTheDocument();
    expect(r.getByText("Yalnızca yapılandırma taslağı; uygulanmadı, bağlantı kurulmadı. Hizmetin ücretleri, gizliliği ve kimlik doğrulaması ayrıca değerlendirilmelidir.")).toBeInTheDocument();
    expect(r.getByText(DRAFT_NOTICE)).toBeInTheDocument();
    expect(r.getByText(".mcp.json")).toBeInTheDocument();
    expect(r.getByText("Claude Code (terminal)")).toBeInTheDocument();
    expect(r.getByLabelText("Önce")).toHaveTextContent("(dosya yok — yeni dosya)");
    const after = r.getByLabelText("Sonra").textContent!;
    expect(JSON.parse(after)).toEqual({ mcpServers: { exa: { type: "http", url: "https://mcp.exa.ai/mcp" } } });
    const diff = r.getByLabelText("Odaklı fark").textContent!;
    expect(diff).toMatch(/\+ +"url": "https:\/\/mcp\.exa\.ai\/mcp"/);
    expect(diff).not.toMatch(/^- /m);
    expect(r.getByText(/Hiçbir dosyaya dokunulmadı/)).toBeInTheDocument();
    expect(r.getByText("exa")).toBeInTheDocument(); // the server name of the reviewed declaration
    expect(text(r.getByText(/Gözden geçirilmiş bildirim/).closest("dl"))).toMatch(/Yalnızca sözdizimi, yalnızca önizleme için gözden geçirildi; çalışma zamanı doğrulanmadı/);
    // the matrix is unchanged: still no decision, and the activation rule for an unknown cell still refuses 'kullanmak istiyorum'
    expect(row.getByText("Henüz karar yok")).toBeInTheDocument();
    const select = row.getByLabelText(/^Karar/) as HTMLSelectElement;
    expect(within(select).getByRole("option", { name: /Bu projede kullanmak istiyorum \(şu an seçilemez\)$/ })).toBeDisabled();
    expect(select.value).toBe("");
  });

  it("explains in the UI that a draft is not an acceptance, and that service cost, privacy and authentication are separate matters", async () => {
    const { row } = await open();
    const r = await preview(row, "new");
    const constraints = text(r.getByText(/Kaynak, uyumluluk ve güven kısıtları/).closest("details"));
    expect(constraints).toMatch(/Uyumluluk \(bu hedef için kayıt\): unknown/);
    expect(constraints).toMatch(/Bu bir yapılandırma TASLAĞIDIR: aracın kabul edilmesi, etkinleştirilmesi ya da kurulmasıyla ilgisi yoktur/);
    expect(constraints).toMatch(/seçilemez; bu önizleme bunu değiştirmez/);
    expect(constraints).toMatch(/Barındırılan bir arama hizmeti/);
    expect(constraints).toMatch(/ücret, kullanım sınırı ve mahremiyet koşulları ayrıca değerlendirilmelidir/);
    expect(constraints).toMatch(/OAuth ya da API anahtarı gerektiğini söyler/);
    expect(constraints).toMatch(/“-p” ve Agent SDK çalıştırmalarında bu istem gösterilemez/);
  });

  it("works on macOS as well, and an existing file keeps its other servers and keys; secrets in it stay off the screen", async () => {
    const { region } = await open();
    await userEvent.selectOptions(region.getByLabelText("İşletim sistemi"), MAC);
    const macRow = await findRow();
    const current = JSON.stringify({ "$schema": "x", mcpServers: { zeta: { command: "node", env: { TOKEN: SENTINEL } }, alfa: { type: "http", url: "https://a.example.invalid/mcp" } }, other: { n: 1 } });
    const r = await preview(macRow, current);
    expect(r.getByText("PREVIEW_ONLY_NOT_APPLIED")).toBeInTheDocument();
    expect(r.getByText("macOS Apple Silicon")).toBeInTheDocument();
    const after = JSON.parse(r.getByLabelText("Sonra").textContent!);
    expect(Object.keys(after.mcpServers)).toEqual(["zeta", "alfa", "exa"]);
    expect(after.other).toEqual({ n: 1 });
    expect(outsideTheBox()).not.toContain(SENTINEL);
    expect(text(r.getByText(/Mevcut metinde tanımlı sunucu adları/))).toContain("zeta");
    expect(r.getByText(/değer gizli olabileceği için maskelendi/)).toBeInTheDocument();
  });

  it("a name collision (also by letter case) or the same endpoint under another name is a CONFLICT: shown, never overwritten", async () => {
    const { row } = await open();
    for (const [servers, expected] of [
      [{ exa: { type: "http", url: "https://x.example.invalid/mcp" } }, /zaten tanımlı; üzerine yazılmaz/],
      [{ EXA: { type: "http", url: "https://x.example.invalid/mcp" } }, /Büyük\/küçük harf farkı dışında/],
      [{ aramam: { type: "http", url: `https://mcp.exa.ai/mcp?exaApiKey=${KEY}` } }, /Aynı komut\/adres başka bir ad altında zaten tanımlı/],
    ] as [object, RegExp][]) {
      const r = await preview(row, JSON.stringify({ mcpServers: servers }));
      expect(r.getByText("CONFLICT")).toBeInTheDocument();
      expect(r.getByLabelText("Sorunlar")).toHaveTextContent(expected);
      expect(r.queryByLabelText("Sonra")).not.toBeInTheDocument();
      expect(r.queryByText(DRAFT_NOTICE)).not.toBeInTheDocument();
      expect(outsideTheBox()).not.toContain(KEY);
    }
  });

  it("malformed, commented, duplicate-key or wrongly shaped pasted JSON is refused as the current configuration", async () => {
    const { row } = await open();
    for (const bad of ["{", '{"mcpServers":{} // c\n}', '{"mcpServers":{},"mcpServers":{}}', '{"mcpServers":[]}', "[]"]) {
      const r = await preview(row, bad);
      expect(r.getByText("INVALID_CURRENT_CONFIG"), bad).toBeInTheDocument();
      expect(r.queryByLabelText("Sonra")).not.toBeInTheDocument();
      expect(r.queryByText(DRAFT_NOTICE)).not.toBeInTheDocument();
    }
  });
});

describe("other targets and gates: no fallback, no widening", () => {
  it("Antigravity IDE and Codex CLI do not get Claude Code's draft", async () => {
    const { region } = await open();
    await userEvent.selectOptions(region.getByLabelText("Ajan yüzeyi"), "antigravity-ide");
    let r = await preview(await findRow(), "new");
    expect(r.getByText("BLOCKED_INSUFFICIENT_EVIDENCE")).toBeInTheDocument();
    expect(r.getByText(".agents/mcp_config.json")).toBeInTheDocument();
    expect(r.queryByLabelText("Sonra")).not.toBeInTheDocument();
    await userEvent.selectOptions(region.getByLabelText("Ajan yüzeyi"), "codex-cli");
    const codexRow = await findRow();
    await userEvent.click(codexRow.getByText(/Yapılandırma önizlemesi — salt okunur/));
    await userEvent.click(codexRow.getByRole("button", { name: /^Değişiklik önizle/ }));
    r = within(await codexRow.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ }));
    expect(r.getByText("TOML_PREVIEW_NOT_IMPLEMENTED")).toBeInTheDocument();
    expect(r.queryByLabelText("Sonra")).not.toBeInTheDocument();
    expect(r.queryByText(DRAFT_NOTICE)).not.toBeInTheDocument();
  });

  it("a quarantined, execution-blocked, disabled entry is blocked with an explanatory-only diff and no draft notice", async () => {
    for (const over of [{ reviewState: "quarantined" }, { executionPolicy: "blocked-until-scoped-authorization" as const }, { enabled: false }]) {
      const { row } = await open(exaEntry(over));
      const r = await preview(row, "new");
      expect(r.getByText("BLOCKED_BY_CATALOG"), JSON.stringify(over)).toBeInTheDocument();
      expect(r.getByText(/AÇIKLAYICI — bu bir tarif değildir/)).toBeInTheDocument();
      expect(r.queryByText(DRAFT_NOTICE)).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /Uygula|Kopyala|Kaydet|Dışa aktar|İndir/ })).not.toBeInTheDocument();
      document.body.innerHTML = "";
    }
  });

  it("untrusted catalog data (a changed pin, a different repository, an old payload without a record) gives a clear block and no draft", async () => {
    const pin = exaEntry();
    (pin.record as { sourceSnapshot: { immutableCommit: string } }).sourceSnapshot.immutableCommit = "0".repeat(40);
    const repo = exaEntry();
    (repo.record as { repository: { url: string } }).repository.url = "https://github.com/someone-else/exa-mcp-server";
    const old = exaEntry();
    delete (old.record as { sourceSnapshot?: unknown }).sourceSnapshot;
    for (const entry of [pin, repo, old]) {
      const { row } = await open(entry);
      const r = await preview(row, "new");
      expect(r.getByText("BLOCKED_INSUFFICIENT_EVIDENCE")).toBeInTheDocument();
      expect(r.getByLabelText("Sorunlar")).toHaveTextContent(/kaynak \(depo ve sabit commit\) bu katalog kaydının kendi sabitlenmiş kaynağıyla eşleşmiyor/);
      expect(r.queryByLabelText("Sonra")).not.toBeInTheDocument();
      expect(r.queryByText(DRAFT_NOTICE)).not.toBeInTheDocument();
      document.body.innerHTML = "";
    }
  });
});

describe("a shown preview is withdrawn when the target or the input changes; nothing persists", () => {
  it("changing the operating system, the surface or the pasted text hides the old preview; a fresh mount starts empty", async () => {
    const { row, region } = await open();
    await preview(row, "new");
    // a different target re-creates the row: the old preview is gone (it is neither kept nor re-applied to the new target)
    await userEvent.selectOptions(region.getByLabelText("İşletim sistemi"), MAC);
    let current = await findRow();
    expect(current.queryByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ })).not.toBeInTheDocument();
    expect(screen.queryByText(DRAFT_NOTICE)).not.toBeInTheDocument();
    // within one target, changing the mode withdraws a shown preview and says why
    await preview(current, "new");
    await userEvent.click(current.getByRole("radio", { name: /Var olan dosyanın metnini/ }));
    expect(current.queryByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ })).not.toBeInTheDocument();
    expect(current.getByText(/önceki önizleme eskidi ve gizlendi/)).toBeInTheDocument();
    current = await findRow();
    document.body.innerHTML = "";
    const again = await open();
    expect(again.row.queryByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ })).not.toBeInTheDocument();
    expect(again.row.getByText("Henüz karar yok")).toBeInTheDocument();
  });

  it("the whole flow makes no write, scan, install, preference change, clipboard access or network request, and offers no apply, copy, export or save", async () => {
    const { row } = await open();
    await preview(row, "new");
    for (const bad of WRITES) expect(calls(bad), bad).toHaveLength(0);
    await waitFor(() => expect(backend.calls.every((c) => /^(list_|get_)/.test(c.cmd))).toBe(true));
    expect(writeText).not.toHaveBeenCalled();
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(localStorage.length).toBe(0);
    expect(row.queryByRole("button", { name: /Uygula|Kopyala|Dışa aktar|İndir|Bağlan|Kur\b|Çalıştır|Test et/ })).not.toBeInTheDocument();
    expect(backend.projects[0].preferences).toEqual([]);
  });

  it("the words on the draft never say installed, connected, verified, accepted, tested, working or successful", async () => {
    const { row } = await open();
    const r = await preview(row, "new");
    const banned = /\b(kuruldu|bağlandı|doğrulandı|onaylandı|kabul edildi|test edildi|çalışıyor|başarılı|başarıyla|etkinleştirildi)\b|✓|✔/i;
    expect(text(r.getByRole("heading", { level: 4 }).closest("section"))).not.toMatch(banned);
    expect(row.queryByRole("img")).not.toBeInTheDocument();
  });
});
