// CATALOG-02 - "Playwright seçenekleri" on the Catalog page: three plain-language options, a target the user picks explicitly, and ONE action that only
// hands an exact project + OS + agent surface + variant to the matrix. Mocked IPC and jsdom only: no real window, no real database, nothing installed
// or connected. The Rust side (data, seeding, collisions, upgrade) is covered by the Rust tests; here its answers are fixtures.
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import playwrightChoiceSource from "./components/PlaywrightChoice.tsx?raw";
import playwrightLogicSource from "./playwrightChoice.ts?raw";
import { CatalogPage } from "./pages/Catalog";
import { createBackend, makeEntry, pref, type Backend } from "./test/backend";
import type { CatalogEntry, ProjectView } from "./types";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const WIN = "windows-x64";
const MAC = "macos-arm64";
const CC = "claude-code-terminal";
const AG = "antigravity-ide";
const CLI = "local-cli";
const FORBIDDEN = ["run_discovery_scan", "evaluate_task", "set_project_preference", "reset_project_preference", "prepare_install", "confirm_install", "cancel_install", "recover_installations", "plan_install", "get_installed_details", "preview_backup_cleanup", "set_setting", "set_startup_enabled", "set_catalog_entry_enabled", "add_catalog_entry", "remove_catalog_entry", "restore_catalog_entry", "preview_github_url", "add_project_reference", "remove_project_reference", "list_inventory"];

const row = (platformTarget: string, agentSurface: string, compatibilityState: string, limitation = "yayıncı kaynağı") => ({ platformTarget, agentSurface, compatibilityState, limitation });
const both = (surface: string, state: string) => [row(WIN, surface, state), row(MAC, surface, state)];
// exactly the labels the shipped data files carry
const cli = (over: Partial<CatalogEntry> = {}) =>
  makeEntry("microsoft-playwright-cli", { displayName: "Microsoft Playwright CLI", record: { targetEvidence: [...both(CLI, "unknown"), ...both(CC, "unknown"), ...both(AG, "unknown")] }, ...over });
const lib = (over: Partial<CatalogEntry> = {}) =>
  makeEntry("microsoft-playwright", { displayName: "Microsoft Playwright", record: { targetEvidence: [...both(CLI, "vendor-declared"), ...both(CC, "unknown"), ...both(AG, "unknown")] }, ...over });
const mcp = (over: Partial<CatalogEntry> = {}) =>
  makeEntry("microsoft-playwright-mcp", { displayName: "Microsoft Playwright MCP", kind: "agent_plugin", record: { targetEvidence: [...both(CLI, "unknown"), ...both(CC, "vendor-declared"), ...both(AG, "unknown")] }, ...over });
const project = (alias: string, name: string, kind: ProjectView["kind"] = "reference"): ProjectView => ({
  projectId: `id-${alias}`, alias, displayName: name, kind, localNote: "GİZLİ-NOT", createdAt: "2026-10-17T00:00:00Z", preferences: [pref()],
});

const calls = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd);
const group = async () => within(await screen.findByRole("group", { name: "Playwright seçenekleri" }));
const optionButtons = async () => (await group()).getAllByRole("button", { name: /^Matriste aç/ });
/** The project list arrives through an async (mocked) IPC read; selecting before it is there is a race that only shows under load. */
const selectProject = async (g: ReturnType<typeof within>, alias: string) => {
  const projectSelect = g.getByLabelText("Proje") as HTMLSelectElement;
  await waitFor(() => expect([...projectSelect.options].some((o) => o.value === alias)).toBe(true));
  await userEvent.selectOptions(projectSelect, alias);
};
const pick = async (g: ReturnType<typeof within>, alias = "benim", os = WIN, surface = CC) => {
  await selectProject(g, alias);
  await userEvent.selectOptions(g.getByLabelText("İşletim sistemi"), os);
  await userEvent.selectOptions(g.getByLabelText("Ajan yüzeyi"), surface);
};
const open = async (onOpen = vi.fn()) => {
  render(<CatalogPage onOpenInMatrix={onOpen} />);
  await group();
  return onOpen;
};

beforeEach(() => {
  backend = createBackend();
  backend.entries = [makeEntry("gitleaks"), lib(), cli(), mcp()];
  backend.projects = [project("ornek-web-sitesi", "Örnek: Web sitesi", "synthetic"), project("benim", "Benim sitem")];
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("the comparison", () => {
  it("shows the three options in the asked order, explains each, and marks only CLI + Skills as the publisher's starting point", async () => {
    await open();
    const g = await group();
    expect(g.getAllByRole("heading", { level: 3 }).map((h) => h.textContent)).toEqual([
      "CLI + Skills — kodlama ajanı için kısa komutlar", "Test / Library — projenin kendi uçtan uca testleri", "MCP — ajana zengin, durumlu tarayıcı denetimi",
    ]);
    expect(g.getByText(/Hiçbiri kurulu değil, hiçbiri çalıştırılmadı, hiçbirine bağlanılmadı/)).toBeInTheDocument();
    expect(g.getByText(/aralarında teknik bir dışlama yoktur/)).toBeInTheDocument();
    expect(g.getAllByText(/Yayıncı belgelerinin başlangıç noktası \(kodlama ajanları\)/)).toHaveLength(1);
    expect(g.getByText(/Bu, “her depoda çalışır”, “kurulu” ya da “uyumluluğu doğrulandı” demek değildir/)).toBeInTheDocument();
    expect(g.getByText(/projenin içinde kendi tarayıcı testlerinizi/)).toBeInTheDocument();
    expect(g.getByText(/jeton maliyeti|bağlam ve jeton maliyeti CLI’dan yüksektir/)).toBeInTheDocument();
    // each option carries its catalog id so same-looking names cannot be confused
    for (const id of ["microsoft-playwright-cli", "microsoft-playwright", "microsoft-playwright-mcp"]) expect(g.getByText(id)).toBeInTheDocument();
  });

  it("the cautions are one click away (expandable), and the future shared-runtime idea is labelled an architecture proposal only", async () => {
    await open();
    const g = await group();
    const summaries = g.getAllByText("Dikkat edilecekler");
    expect(summaries).toHaveLength(3);
    await userEvent.click(summaries[0]);
    expect(g.getByText(/Ortak bir kurulum, ortak ya da ayrı bir tarayıcı profili demek değildir/)).toBeVisible();
    await userEvent.click(g.getByText(/Gelecekte ortak bir CLI çalışma zamanı \(yalnızca öneri\)/));
    expect(g.getByText(/Bu bir mimari öneridir; kurulu bir özellik değildir/)).toBeVisible();
    expect(g.getByText(/tarayıcı oturumları ile profilleri ayrı kalır/)).toBeInTheDocument();
    expect(g.getByText(/çerez veya oturum paylaşımına izin vermez ve hiçbir dosya yazmaz/)).toBeInTheDocument();
  });

  it("nothing is selected, saved, scanned, installed or opened by merely showing it", async () => {
    const onOpen = await open();
    const g = await group();
    expect(g.getByLabelText("Proje")).toHaveValue("");
    expect(g.getByLabelText("İşletim sistemi")).toHaveValue("");
    expect(g.getByLabelText("Ajan yüzeyi")).toHaveValue("");
    expect(onOpen).not.toHaveBeenCalled();
    for (const bad of FORBIDDEN) expect(calls(bad), bad).toHaveLength(0);
    expect(calls("list_projects")).toHaveLength(1); // the one existing read-only call it uses
    expect(g.queryByText(/Seçtiğiniz hedef için yayıncı beyanı/)).not.toBeInTheDocument(); // no target, no compatibility claim
    expect(g.queryByRole("checkbox")).not.toBeInTheDocument();
    expect(g.queryByRole("button", { name: /^(Kaydet|Kur|Etkinleştir|Kabul)/ })).not.toBeInTheDocument();
  });

  it("is not shown at all when the catalog holds none of the three", async () => {
    backend.entries = [makeEntry("gitleaks")];
    render(<CatalogPage onOpenInMatrix={vi.fn()} />);
    await screen.findByRole("heading", { name: "gitleaks" });
    expect(screen.queryByRole("group", { name: "Playwright seçenekleri" })).not.toBeInTheDocument();
    expect(calls("list_projects")).toHaveLength(0);
  });

  it("without a way to open the matrix it is a plain read-only comparison", async () => {
    render(<CatalogPage />);
    const g = await group();
    expect(g.queryByLabelText("Proje")).not.toBeInTheDocument();
    expect(g.queryByRole("button", { name: /^Matriste aç/ })).not.toBeInTheDocument();
    expect(calls("list_projects")).toHaveLength(1);
  });
});

describe("opening the matrix: an exact target and an exact variant, or guidance", () => {
  it("with no target the button explains what is missing, focuses the first missing field and opens nothing", async () => {
    const onOpen = await open();
    const g = await group();
    await userEvent.click((await optionButtons())[0]);
    expect(g.getByText("Önce proje, işletim sistemi ve ajan yüzeyini seçin. Hiçbir şey açılmadı, hiçbir kayıt yapılmadı.")).toBeInTheDocument();
    expect(g.getByLabelText("Proje")).toHaveFocus();
    expect(onOpen).not.toHaveBeenCalled();
    await selectProject(g, "benim");
    expect(g.queryByText(/Önce proje, işletim sistemi/)).not.toBeInTheDocument(); // the message clears when the user acts
    await userEvent.click((await optionButtons())[1]);
    expect(g.getByLabelText("İşletim sistemi")).toHaveFocus(); // the next missing one
    await userEvent.selectOptions(g.getByLabelText("İşletim sistemi"), MAC);
    await userEvent.click((await optionButtons())[2]);
    expect(g.getByLabelText("Ajan yüzeyi")).toHaveFocus();
    expect(onOpen).not.toHaveBeenCalled();
  });

  it("each variant opens exactly its own entry for exactly the chosen project, OS and agent surface - no fallback to another variant", async () => {
    const onOpen = await open();
    const g = await group();
    await pick(g, "benim", MAC, AG);
    const [b1, b2, b3] = await optionButtons();
    await userEvent.click(b3);
    await userEvent.click(b1);
    await userEvent.click(b2);
    expect(onOpen.mock.calls.map((c) => c[0])).toEqual([
      { alias: "benim", platform: MAC, surface: AG, entryId: "microsoft-playwright-mcp" },
      { alias: "benim", platform: MAC, surface: AG, entryId: "microsoft-playwright-cli" },
      { alias: "benim", platform: MAC, surface: AG, entryId: "microsoft-playwright" },
    ]);
    // opening the matrix is navigation: still nothing was written, scanned or installed
    for (const bad of FORBIDDEN) expect(calls(bad), bad).toHaveLength(0);
  });

  it("the buttons name the variant AND the chosen target, so they cannot be mistaken for one another", async () => {
    await open();
    const g = await group();
    await pick(g, "ornek-web-sitesi", WIN, CC);
    const names = (await optionButtons()).map((b) => b.textContent);
    expect(names).toEqual([
      "Matriste aç: CLI + Skills (Örnek: Web sitesi · Windows x64 · Claude Code (terminal))",
      "Matriste aç: Test / Library (Örnek: Web sitesi · Windows x64 · Claude Code (terminal))",
      "Matriste aç: MCP (Örnek: Web sitesi · Windows x64 · Claude Code (terminal))",
    ]);
  });

  it("more than one option can be opened independently: no exclusivity, no remembered selection", async () => {
    const onOpen = await open();
    const g = await group();
    await pick(g);
    const [b1, , b3] = await optionButtons();
    await userEvent.click(b1);
    await userEvent.click(b3);
    expect(onOpen).toHaveBeenCalledTimes(2);
    expect(g.queryAllByRole("button", { pressed: true })).toHaveLength(0);
    expect(g.queryByText(/seçildi|seçili/i)).not.toBeInTheDocument();
  });

  it("works from the keyboard: labelled fields, focusable buttons, Enter opens the focused variant", async () => {
    const onOpen = await open();
    const g = await group();
    // the fields are reachable by label and by Tab, in order
    g.getByLabelText("Proje").focus();
    await userEvent.tab();
    expect(g.getByLabelText("İşletim sistemi")).toHaveFocus();
    await userEvent.tab();
    expect(g.getByLabelText("Ajan yüzeyi")).toHaveFocus();
    await selectProject(g, "benim");
    await userEvent.selectOptions(g.getByLabelText("İşletim sistemi"), WIN);
    await userEvent.selectOptions(g.getByLabelText("Ajan yüzeyi"), CLI);
    const [, , mcpButton] = await optionButtons();
    mcpButton.focus();
    expect(mcpButton).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    expect(onOpen).toHaveBeenCalledWith({ alias: "benim", platform: WIN, surface: CLI, entryId: "microsoft-playwright-mcp" });
  });
});

describe("compatibility is shown for the chosen target and unknown stays unknown", () => {
  it("CLI is unknown everywhere (also on Claude Code), the MCP variant is publisher-declared for Claude Code only, and nothing is called tested", async () => {
    await open();
    const g = await group();
    await pick(g, "benim", WIN, CC);
    const [cliBtn, libBtn, mcpBtn] = await optionButtons();
    const optionOf = (b: HTMLElement) => within(b.closest("li") as HTMLElement);
    expect(optionOf(cliBtn).getByText(/Seçtiğiniz hedef için yayıncı beyanı:/)).toHaveTextContent("Bilinmiyor");
    expect(optionOf(cliBtn).getByText(/Matriste “kullanmak istiyorum” seçilemez: Bu işletim sistemi ve ajan yüzeyi için uyumluluk bilinmiyor/)).toBeInTheDocument();
    expect(optionOf(libBtn).getByText(/Seçtiğiniz hedef için yayıncı beyanı:/)).toHaveTextContent("Bilinmiyor");
    expect(optionOf(mcpBtn).getByText(/Seçtiğiniz hedef için yayıncı beyanı:/)).toHaveTextContent("Üretici belirtmiş (test edilmedi)");
    expect(optionOf(mcpBtn).getByText(/Bu, test edildiği veya çalıştığı anlamına gelmez; yalnızca yayıncının kendi beyanıdır/)).toBeInTheDocument();
    expect(optionOf(mcpBtn).queryByText(/seçilemez/)).not.toBeInTheDocument();
    // another target: Test / Library is declared for the local command line, MCP no longer is
    await userEvent.selectOptions(g.getByLabelText("Ajan yüzeyi"), CLI);
    expect(optionOf(libBtn).getByText(/Seçtiğiniz hedef için yayıncı beyanı:/)).toHaveTextContent("Üretici belirtmiş (test edilmedi)");
    expect(optionOf(mcpBtn).getByText(/Seçtiğiniz hedef için yayıncı beyanı:/)).toHaveTextContent("Bilinmiyor");
    // opening is still allowed for an unknown combination (the matrix then refuses 'accepted' and explains why; 'not now' / 'reject' stay possible)
    expect(cliBtn).toBeEnabled();
  });
});

describe("removed, disabled, user-owned and missing records are described honestly", () => {
  it("a removed record says so and still opens the matrix (which shows an old decision or explains)", async () => {
    backend.entries = [lib(), cli(), mcp({ removedAt: "2026-10-20T00:00:00Z", enabled: false })];
    const onOpen = await open();
    const g = await group();
    const mcpOption = within(g.getByText("microsoft-playwright-mcp").closest("li") as HTMLElement);
    expect(mcpOption.getByText(/Katalogdan kaldırılmış: yeni karar kaydedilemez/)).toBeInTheDocument();
    await pick(g);
    expect(mcpOption.getByText(/Matriste “kullanmak istiyorum” seçilemez: Katalogdan kaldırılmış bir kayıt için yeni karar kaydedilemez/)).toBeInTheDocument();
    await userEvent.click(mcpOption.getByRole("button", { name: /^Matriste aç/ }));
    expect(onOpen).toHaveBeenCalledWith({ alias: "benim", platform: WIN, surface: CC, entryId: "microsoft-playwright-mcp" });
  });

  it("a disabled record says how to enable it and that 'not now' / 'reject' remain possible", async () => {
    backend.entries = [lib({ enabled: false }), cli(), mcp()];
    await open();
    const g = await group();
    const libOption = within(g.getByText("microsoft-playwright", { selector: "code" }).closest("li") as HTMLElement);
    expect(libOption.getByText(/Katalogda kapalı: “kullanmak istiyorum” seçilemez; önce aşağıdaki kayıtta katalogda etkinleştirin/)).toBeInTheDocument();
  });

  it("a record the user added from a link is labelled as theirs, carries no publisher evidence and is never treated as the shipped one", async () => {
    backend.entries = [lib(), cli({ origin: "user", record: { targetEvidence: [] }, displayName: "microsoft/playwright-cli" }), mcp()];
    await open();
    const g = await group();
    const cliOption = within(g.getByText("microsoft-playwright-cli").closest("li") as HTMLElement);
    expect(cliOption.getByText(/sizin eklediğiniz bir aday: uygulama bu depo için yayıncı künyesini uygulamadı/)).toBeInTheDocument();
    await pick(g, "benim", WIN, CC);
    expect(cliOption.getByText(/Seçtiğiniz hedef için yayıncı beyanı:/)).toHaveTextContent("Bilinmiyor");
  });

  it("a missing record gets no button and no substitute, and the other options still work", async () => {
    backend.entries = [lib(), mcp()];
    const onOpen = await open();
    const g = await group();
    expect(g.getByText("Bu kayıt katalogda bulunamadı; yerine başka bir seçenek seçilmedi.")).toBeInTheDocument();
    expect(g.getAllByRole("button", { name: /^Matriste aç/ })).toHaveLength(2);
    expect(g.queryByRole("button", { name: /CLI \+ Skills/ })).not.toBeInTheDocument();
    await pick(g);
    await userEvent.click(g.getByRole("button", { name: /Test \/ Library/ }));
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(onOpen.mock.calls[0][0].entryId).toBe("microsoft-playwright");
  });
});

describe("no projects, an unreadable project list, and nothing leaking", () => {
  it("with no project it says what to do first, offers no invented project, and refuses to open without one", async () => {
    backend.projects = [];
    const onOpen = await open();
    const g = await group();
    expect(await g.findByText(/Henüz proje yok. Önce Projeler sayfasında bir proje ekleyin/)).toBeInTheDocument();
    expect(within(g.getByLabelText("Proje")).getAllByRole("option")).toHaveLength(1); // only the placeholder
    await userEvent.selectOptions(g.getByLabelText("İşletim sistemi"), WIN);
    await userEvent.selectOptions(g.getByLabelText("Ajan yüzeyi"), CC);
    await userEvent.click((await optionButtons())[0]);
    expect(g.getByText(/Önce proje, işletim sistemi ve ajan yüzeyini seçin/)).toBeInTheDocument();
    expect(onOpen).not.toHaveBeenCalled();
  });

  it("an unreadable project list shows the error with a retry; the comparison itself stays readable; retry recovers", async () => {
    backend.failures.list_projects = { code: "DB_BUSY", message: "Veritabanı meşgul." };
    const onOpen = await open();
    const g = await group();
    expect(await screen.findByText(/Veritabanı meşgul/)).toBeInTheDocument();
    expect(g.getAllByRole("heading", { level: 3 })).toHaveLength(3);
    await userEvent.click((await optionButtons())[0]);
    expect(onOpen).not.toHaveBeenCalled();
    delete backend.failures.list_projects;
    await userEvent.click(screen.getByRole("button", { name: "Yeniden dene" }));
    await waitFor(() => expect(within(g.getByLabelText("Proje")).getAllByRole("option")).toHaveLength(3));
  });

  it("never shows a project's private note and marks example projects as examples", async () => {
    await open();
    const g = await group();
    expect(document.body.textContent).not.toContain("GİZLİ-NOT");
    expect(within(g.getByLabelText("Proje")).getByRole("option", { name: "Örnek: Web sitesi (örnek)" })).toBeInTheDocument();
  });
});

describe("what the screen never claims", () => {
  it("has no success or status icon and no positive installation, activation or test claim anywhere in the comparison", async () => {
    await open();
    const g = await group();
    await pick(g);
    const text = (await screen.findByRole("group", { name: "Playwright seçenekleri" })).textContent ?? "";
    expect(text).not.toMatch(/[✓✕⚠⛔]/);
    expect(text).not.toMatch(/Kuruldu|Etkinleştirildi|Test edildi|Doğrulandı|Çalışıyor|kurulu ve|başarıyla/);
    expect(g.queryByRole("img")).not.toBeInTheDocument();
  });

  it("the new modules use only the existing read-only project list: no other IPC, timer, storage or network", () => {
    const apiCalls = [...playwrightChoiceSource.matchAll(/api\.(\w+)/g)].map((m) => m[1]);
    expect(apiCalls).toEqual(["listProjects"]);
    for (const source of [playwrightChoiceSource, playwrightLogicSource]) {
      expect(source).not.toMatch(/invoke\(|@tauri-apps|fetch\(|XMLHttpRequest|setInterval|setTimeout|localStorage|sessionStorage|WebSocket/);
    }
  });
});

describe("through the real App: the handoff reaches the matrix, exactly, and writes nothing", () => {
  const go = async (name: string) => {
    await userEvent.click(within(screen.getByRole("navigation")).getByRole("button", { name }));
    return screen.findByRole("heading", { level: 1, name });
  };
  const spots = () => document.querySelectorAll(".ws-spot");

  it("MCP for Claude Code on Windows: the Projects page opens that project and target with the MCP row brought into view, and nothing was saved", async () => {
    render(<App />);
    await go("Araç Kütüphanesi");
    const g = await group();
    await pick(g, "benim", WIN, CC);
    await userEvent.click(g.getByRole("button", { name: /Matriste aç: MCP/ }));
    await screen.findByRole("heading", { level: 1, name: "Projeler" });
    const matrix = within(await screen.findByRole("region", { name: "Proje Araç Matrisi" }));
    const row = (await matrix.findByRole("heading", { level: 3, name: "Microsoft Playwright MCP" })).closest("article") as HTMLElement;
    await waitFor(() => expect(spots()).toHaveLength(1));
    expect(spots()[0]).toContainElement(row);
    expect(matrix.getByLabelText("Proje")).toHaveValue("benim");
    expect(matrix.getByLabelText("İşletim sistemi")).toHaveValue(WIN);
    expect(matrix.getByLabelText("Ajan yüzeyi")).toHaveValue(CC);
    for (const bad of FORBIDDEN.filter((c) => c !== "list_inventory")) expect(calls(bad), bad).toHaveLength(0);
  });

  it("CLI + Skills where nothing is declared: the matrix shows the CLI row and explains that 'use' cannot be saved there - the choice stays the user's", async () => {
    render(<App />);
    await go("Araç Kütüphanesi");
    const g = await group();
    await pick(g, "benim", MAC, CLI);
    await userEvent.click(g.getByRole("button", { name: /Matriste aç: CLI \+ Skills/ }));
    const matrix = within(await screen.findByRole("region", { name: "Proje Araç Matrisi" }));
    const row = (await matrix.findByRole("heading", { level: 3, name: "Microsoft Playwright CLI" })).closest("article") as HTMLElement;
    await waitFor(() => expect(spots()).toHaveLength(1));
    expect(spots()[0]).toContainElement(row);
    expect(within(row).getByText(/uyumluluk bilinmiyor/i)).toBeInTheDocument();
    expect(calls("set_project_preference")).toHaveLength(0);
  });
});

// keep the unused-import linter honest
void act;
