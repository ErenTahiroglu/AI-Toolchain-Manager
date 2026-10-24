// PRODUCT-03 - Recommendations -> matrix handoff of ONE suggested tool: target + entry id. Navigation only: it brings the right row into view and
// decides nothing. App-level tests go through the real Recommendations page and the real sidebar; the component-level ones feed ProjectsPage the
// typed handoff directly for states the UI cannot produce on its own. Mocked IPC and jsdom only (no real window, focus ring or screen reader).
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { ProjectsPage } from "./pages/Projects";
import { createBackend, makeEntry, pref, rec, type Backend } from "./test/backend";
import type { ProjectView } from "./types";
import type { MatrixHandoff } from "./workspace";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const WIN = "windows-x64";
const MAC = "macos-arm64";
const CC = "claude-code-terminal";
const CLI = "local-cli";
const AG = "antigravity-ide";
const ev = (platformTarget: string, agentSurface: string, compatibilityState: string) => ({ platformTarget, agentSurface, compatibilityState, limitation: `kayıt: ${compatibilityState}` });

const calls = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd);
const go = async (name: string) => {
  await userEvent.click(within(screen.getByRole("navigation")).getByRole("button", { name }));
  return screen.findByRole("heading", { level: 1, name });
};
const project = (alias: string, name: string, preferences: ProjectView["preferences"] = []): ProjectView => ({
  projectId: `id-${alias}`, alias, displayName: name, kind: "reference", localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences,
});
const matrix = async () => within(await screen.findByRole("region", { name: "Proje Araç Matrisi" }));
const articleOf = async (name: string) => {
  const m = await matrix();
  return (await m.findByRole("heading", { level: 3, name })).closest("article") as HTMLElement;
};
const spots = () => document.querySelectorAll(".ws-spot");
const OPEN_ROW = /^Bu öneriyi matriste incele/;

beforeEach(() => {
  backend = createBackend();
  backend.entries.push(
    makeEntry("alfa", { displayName: "Alfa Araç", record: { targetEvidence: [ev(MAC, AG, "vendor-declared"), ev(WIN, CC, "vendor-declared")] } }),
    makeEntry("beta", { displayName: "Beta Eklenti", record: { targetEvidence: [ev(WIN, CLI, "vendor-declared")] } }), // nothing recorded for WIN/CC or MAC/AG: compatibility unknown there
    makeEntry("kilitli", { displayName: "Kilitli Araç", executionPolicy: "blocked-until-scoped-authorization", record: { targetEvidence: [ev(MAC, AG, "vendor-declared")] } }),
    makeEntry("eski", { displayName: "Eski Kayıt", removedAt: "2026-10-18T00:00:00Z", enabled: false, record: { targetEvidence: [ev(MAC, AG, "vendor-declared")] } }),
  );
  backend.projects = [
    { ...project("ornek-web-sitesi", "Örnek: Web sitesi", [pref({ entryId: "gitleaks", entryName: "gitleaks", agentSurface: CLI, platformTarget: WIN })]), kind: "synthetic" },
    project("benim-sitem", "Benim sitem"),
    project("diger-proje", "Diğer proje"),
  ];
});

afterEach(() => {
  // opening a row decides nothing; only the tests that press the matrix's own save button may write
  vi.restoreAllMocks();
});

/** Evaluates a task on the real Recommendations page for the given target and returns once the card of `entryId` is shown. */
async function recommend(entryId: string, displayName: string, t: { project: string; platform: string; surface: string }) {
  backend.nextEvaluation = { taskId: "t", phase: "start", recommendations: [rec({ entryId, displayName })], suppressed: [], disclaimer: "Hiçbir şey kurulmadı." };
  await go("Öneriler");
  await screen.findByRole("option", { name: "Diğer proje" });
  await userEvent.selectOptions(screen.getByLabelText("Proje"), t.project);
  await userEvent.selectOptions(screen.getByLabelText("İşletim sistemi"), t.platform);
  await userEvent.selectOptions(screen.getByLabelText("Ajan yüzeyi"), t.surface);
  await userEvent.type(screen.getByLabelText("Görev metni"), "görev");
  await userEvent.click(screen.getByRole("button", { name: "Değerlendir" }));
  await screen.findByRole("heading", { level: 3, name: new RegExp(displayName) });
}

describe("a recommendation opens the matrix on the suggested tool of the evaluated target", () => {
  it("selects project + system + surface, marks and focuses exactly that row, and says where it came from", async () => {
    render(<App />);
    await recommend("alfa", "Alfa Araç", { project: "benim-sitem", platform: MAC, surface: AG });
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    await screen.findByRole("heading", { level: 1, name: "Projeler" });
    const row = await articleOf("Alfa Araç");
    await waitFor(() => expect(row).toHaveFocus());
    const m = await matrix();
    expect(m.getByLabelText("Proje")).toHaveValue("benim-sitem");
    expect(m.getByLabelText("İşletim sistemi")).toHaveValue(MAC);
    expect(m.getByLabelText("Ajan yüzeyi")).toHaveValue(AG);
    expect(within(row).getByText("Öneriden gelen araç")).toBeInTheDocument();
    expect(spots()).toHaveLength(1);
    expect(spots()[0]).toContainElement(row);
    expect(await m.findByText(/Öneriden açıldı: Alfa Araç/)).toBeInTheDocument();
    expect(m.getAllByRole("heading", { level: 3 }).length).toBeGreaterThan(3); // the rest of the matrix is still there
  });

  it("is navigation only: no decision is selected or saved, nothing is evaluated again, installed, scanned or enabled", async () => {
    render(<App />);
    await recommend("alfa", "Alfa Araç", { project: "benim-sitem", platform: MAC, surface: AG });
    const before = backend.calls.length;
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    const row = within(await articleOf("Alfa Araç"));
    await waitFor(() => expect(row.getByText("Henüz karar yok")).toBeInTheDocument());
    expect(row.getByLabelText(/^Karar/)).toHaveValue(""); // nothing pre-selected, least of all "accepted"
    expect(row.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ })).toBeDisabled();
    const after = [...new Set(backend.calls.slice(before).map((c) => c.cmd))];
    for (const cmd of after) expect(["list_projects", "list_catalog", "list_inventory"], cmd).toContain(cmd); // list_inventory: the stored read of PRODUCT-04
    for (const bad of ["evaluate_task", "set_project_preference", "reset_project_preference", "prepare_install", "confirm_install", "plan_install", "run_discovery_scan", "set_catalog_entry_enabled", "list_install_states"]) {
      expect(calls(bad).filter((c) => backend.calls.indexOf(c) >= before), bad).toHaveLength(0);
    }
    expect(calls("evaluate_task")).toHaveLength(1);
  });

  it("a real save happens only on the matrix's own button, once, with the full key", async () => {
    render(<App />);
    await recommend("alfa", "Alfa Araç", { project: "benim-sitem", platform: MAC, surface: AG });
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    const row = within(await articleOf("Alfa Araç"));
    expect(calls("set_project_preference")).toHaveLength(0);
    await userEvent.selectOptions(row.getByLabelText(/^Karar/), "accepted");
    expect(calls("set_project_preference")).toHaveLength(0); // choosing is not saving
    await userEvent.click(row.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ }));
    await waitFor(() => expect(calls("set_project_preference")).toHaveLength(1));
    expect(calls("set_project_preference")[0].args).toMatchObject({ alias: "benim-sitem", entryId: "alfa", agentSurface: AG, platformTarget: MAC, decision: "accepted" });
    expect(await (await matrix()).findByText(/Alfa Araç: “Bu projede kullanmak istiyorum” isteği kaydedildi/)).toBeInTheDocument();
  });

  it("is keyboard operable end to end: Enter on the card button leaves focus on the suggested row", async () => {
    render(<App />);
    await recommend("beta", "Beta Eklenti", { project: "diger-proje", platform: WIN, surface: CLI });
    const button = screen.getByRole("button", { name: OPEN_ROW });
    button.focus();
    await userEvent.keyboard("{Enter}");
    const row = await articleOf("Beta Eklenti");
    await waitFor(() => expect(row).toHaveFocus());
    expect((await matrix()).getByLabelText("Proje")).toHaveValue("diger-proje");
  });

  it("the second handoff to the same tool brings focus back to its row again", async () => {
    render(<App />);
    await recommend("alfa", "Alfa Araç", { project: "benim-sitem", platform: MAC, surface: AG });
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    await waitFor(async () => expect(await articleOf("Alfa Araç")).toHaveFocus());
    await go("Öneriler");
    await recommend("alfa", "Alfa Araç", { project: "benim-sitem", platform: MAC, surface: AG });
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    await waitFor(async () => expect(await articleOf("Alfa Araç")).toHaveFocus());
    expect(spots()).toHaveLength(1);
  });
});

describe("what the matrix says when the suggested tool cannot simply be acted on", () => {
  it("unknown compatibility: the row is shown, 'kullanmak istiyorum' is not selectable, the compatibility stays 'unknown' - a recommendation never makes it supported", async () => {
    render(<App />);
    await recommend("beta", "Beta Eklenti", { project: "benim-sitem", platform: WIN, surface: CC });
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    const m = await matrix();
    const row = within(await articleOf("Beta Eklenti"));
    expect(await m.findByText(/“kullanmak istiyorum” seçilemez/, { selector: ".ws-focus-note p" })).toBeInTheDocument();
    expect(row.getByRole("option", { name: /Bu projede kullanmak istiyorum \(şu an seçilemez\)/ })).toBeDisabled();
    expect(row.getByText(/Bu birleşim için kayıtlı kanıt yok/)).toBeInTheDocument();
    expect(row.getByText("Bilinmiyor")).toBeInTheDocument();
    expect(row.queryByText(/Desteklenir|Destekleniyor|Üretici beyanı/)).not.toBeInTheDocument();
  });

  it("a tool blocked until scoped authorization is shown with that reason", async () => {
    render(<App />);
    await recommend("kilitli", "Kilitli Araç", { project: "benim-sitem", platform: MAC, surface: AG });
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    const m = await matrix();
    expect(await m.findByText(/Kapsamlı yazılı yetki olmadan/, { selector: ".ws-focus-note p" })).toBeInTheDocument();
    expect(within(await articleOf("Kilitli Araç")).getByRole("option", { name: /kullanmak istiyorum \(şu an seçilemez\)/ })).toBeDisabled();
  });

  it("a tool that was removed from the catalog and holds no decision is NOT replaced by another row: only a note", async () => {
    render(<App />);
    await recommend("eski", "Eski Kayıt", { project: "benim-sitem", platform: MAC, surface: AG });
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    const m = await matrix();
    expect(await m.findByText(/katalogdan kaldırılmış ve bu projede bu hedef için kayıtlı bir kararı yok/)).toBeInTheDocument();
    expect(m.getByText("Önerilen araç satırı vurgulanamadı.")).toBeInTheDocument();
    expect(spots()).toHaveLength(0);
    expect(document.activeElement?.closest(".ws-row")).toBeNull();
    expect(m.queryByRole("heading", { level: 3, name: "Eski Kayıt" })).not.toBeInTheDocument();
  });

  it("a removed tool that still holds a decision is shown, marked removed, and cannot take a new decision", async () => {
    backend.projects[1].preferences = [pref({ entryId: "eski", entryName: "eski", entryRemoved: true, agentSurface: AG, platformTarget: MAC, decision: "rejected" })];
    render(<App />);
    await recommend("eski", "Eski Kayıt", { project: "benim-sitem", platform: MAC, surface: AG });
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    const row = await articleOf("Eski Kayıt");
    await waitFor(() => expect(row).toHaveFocus());
    expect(within(row).getByText(/Bu kayıt katalogdan kaldırıldı: yeni karar kaydedilemez/)).toBeInTheDocument();
    expect(within(row).queryByRole("button", { name: /^Yalnızca tercihimi kaydet/ })).not.toBeInTheDocument();
    expect((await matrix()).getByText(/katalogdan kaldırılmış: yeni karar kaydedilemez/, { selector: ".ws-focus-note p" })).toBeInTheDocument();
  });

  it("a tool id the catalog does not contain is reported by name; no other tool is highlighted or focused", async () => {
    render(<App />);
    await recommend("yok-boyle-bir-arac", "Hayalet", { project: "benim-sitem", platform: MAC, surface: AG });
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    const m = await matrix();
    expect(await m.findByText(/\(“yok-boyle-bir-arac”\) katalogda bulunamadı; başka bir araç seçilmedi/)).toBeInTheDocument();
    expect(spots()).toHaveLength(0);
    expect(document.activeElement?.closest(".ws-row")).toBeNull();
    expect(m.getByLabelText("Proje")).toHaveValue("benim-sitem"); // the target itself was still honoured
  });

  it("a project that is gone: the matrix falls back WITH the existing note, and says the suggestion's target was not found; nothing is highlighted", async () => {
    render(<App />);
    await recommend("alfa", "Alfa Araç", { project: "benim-sitem", platform: MAC, surface: AG });
    backend.projects = backend.projects.filter((p) => p.alias !== "benim-sitem");
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    const m = await matrix();
    expect(await m.findByText(/İstenen proje artık yok veya bulunamadı/)).toBeInTheDocument();
    expect(await m.findByText(/istenen hedef .* bulunamadı/)).toBeInTheDocument();
    expect(spots()).toHaveLength(0);
    expect(m.getByLabelText("Proje")).toHaveValue("ornek-web-sitesi");
  });
});

describe("the highlight is bound to its target and to its visit", () => {
  it("choosing another target removes the mark and the note, and returning to the first target does not bring them back", async () => {
    render(<App />);
    await recommend("alfa", "Alfa Araç", { project: "benim-sitem", platform: MAC, surface: AG });
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    const m = await matrix();
    await m.findByText(/Öneriden açıldı: Alfa Araç/);
    await userEvent.selectOptions(m.getByLabelText("Proje"), "diger-proje");
    await waitFor(() => expect(m.queryByText(/Öneriden açıldı/)).not.toBeInTheDocument());
    expect(spots()).toHaveLength(0);
    await userEvent.selectOptions(m.getByLabelText("Proje"), "benim-sitem");
    expect(m.queryByText(/Öneriden açıldı/)).not.toBeInTheDocument();
    expect(spots()).toHaveLength(0);
    expect(m.queryByText("Öneriden gelen araç")).not.toBeInTheDocument();
  });

  it("ordinary sidebar navigation clears the handoff: the next visit has no mark, no note and no focus on a row - also after a detour", async () => {
    render(<App />);
    await recommend("alfa", "Alfa Araç", { project: "benim-sitem", platform: MAC, surface: AG });
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    await waitFor(async () => expect(await articleOf("Alfa Araç")).toHaveFocus());
    await go("Ana Sayfa");
    await go("Projeler");
    const m = await matrix();
    await articleOf("Alfa Araç");
    expect(spots()).toHaveLength(0);
    expect(m.queryByText(/Öneriden açıldı/)).not.toBeInTheDocument();
    expect(m.getByLabelText("İşletim sistemi")).toHaveValue(WIN); // the default target, not the handoff's
    expect(document.activeElement?.closest(".ws-row")).toBeNull();
    await go("Öneriler");
    await go("Projeler");
    expect(spots()).toHaveLength(0);
  });

  it("a second handoff to another project carries neither the first row's mark nor the first confirmation", async () => {
    render(<App />);
    await recommend("alfa", "Alfa Araç", { project: "benim-sitem", platform: MAC, surface: AG });
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    const first = within(await articleOf("Alfa Araç"));
    await userEvent.selectOptions(first.getByLabelText(/^Karar/), "rejected");
    await userEvent.click(first.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ }));
    expect(await (await matrix()).findByText(/Alfa Araç: “Reddet \(tekrar sorma\)” isteği kaydedildi/)).toBeInTheDocument();

    await go("Öneriler");
    await recommend("beta", "Beta Eklenti", { project: "diger-proje", platform: WIN, surface: CLI });
    await userEvent.click(screen.getByRole("button", { name: OPEN_ROW }));
    const m = await matrix();
    const row = await articleOf("Beta Eklenti");
    await waitFor(() => expect(row).toHaveFocus());
    expect(m.getByLabelText("Proje")).toHaveValue("diger-proje");
    expect(m.queryByText(/isteği kaydedildi/)).not.toBeInTheDocument();
    expect(spots()).toHaveLength(1);
    expect(spots()[0]).toContainElement(row);
    expect(within(await articleOf("Alfa Araç")).getByText("Henüz karar yok")).toBeInTheDocument(); // the first project's wish is not this project's
  });
});

describe("the typed handoff given straight to the Projects page (states the sidebar flow cannot produce)", () => {
  const handoff = (over: Partial<MatrixHandoff> = {}): MatrixHandoff => ({ alias: "benim-sitem", platform: MAC, surface: AG, entryId: "alfa", nonce: 1, ...over });

  it("a filter that hides the suggested row is cleared, and the user is told", async () => {
    const view = render(<ProjectsPage handoff={null} />);
    const m = await matrix();
    await m.findByRole("heading", { level: 3, name: "Beta Eklenti" });
    await userEvent.type(m.getByLabelText("Araç ara"), "sadece-beta-eslesmez");
    expect(await m.findByText(/Bu süzgeçle eşleşen araç yok/)).toBeInTheDocument();
    view.rerender(<ProjectsPage handoff={handoff()} />);
    const row = await articleOf("Alfa Araç");
    await waitFor(() => expect(row).toHaveFocus());
    expect(m.getByLabelText("Araç ara")).toHaveValue("");
    expect(await m.findByText(/süzgeçleri bu aracı gizlediği için temizlendi/)).toBeInTheDocument();
  });

  it("a decision filter that would hide the row is cleared as well", async () => {
    const view = render(<ProjectsPage handoff={null} />);
    const m = await matrix();
    await m.findByRole("heading", { level: 3, name: "Beta Eklenti" });
    await userEvent.selectOptions(m.getByLabelText("Karar durumu"), "rejected");
    view.rerender(<ProjectsPage handoff={handoff({ alias: "benim-sitem" })} />);
    await waitFor(async () => expect(await articleOf("Alfa Araç")).toHaveFocus());
    expect(m.getByLabelText("Karar durumu")).toHaveValue("all");
  });

  it("an unknown operating system or surface in the request is replaced WITH the existing note, and no row is highlighted", async () => {
    render(<ProjectsPage handoff={handoff({ platform: "linux-x64" })} />);
    const m = await matrix();
    expect(await m.findByText(/İstenen işletim sistemi tanınmıyor/)).toBeInTheDocument();
    expect(await m.findByText(/istenen hedef .* bulunamadı/)).toBeInTheDocument();
    expect(spots()).toHaveLength(0);
  });

  it("a request without a tool (the target-only handoff) marks nothing and says nothing about a tool", async () => {
    render(<ProjectsPage handoff={handoff({ entryId: null })} />);
    const m = await matrix();
    await articleOf("Alfa Araç");
    expect(spots()).toHaveLength(0);
    expect(m.queryByText(/Öneriden açıldı|vurgulanamadı/)).not.toBeInTheDocument();
    expect(m.getByLabelText("Proje")).toHaveValue("benim-sitem");
  });

  it("while the catalog is still loading the request waits; when it arrives the row is found and focused", async () => {
    const original = backend.handle;
    let release!: () => void;
    const gate = new Promise<void>((r) => { release = r; });
    backend.handle = async (c, a) => {
      if (c === "list_catalog") await gate;
      return original(c, a);
    };
    render(<ProjectsPage handoff={handoff()} />);
    await screen.findByText("Yükleniyor…");
    expect(spots()).toHaveLength(0);
    await act(async () => release());
    const row = await articleOf("Alfa Araç");
    await waitFor(() => expect(row).toHaveFocus());
  });

  it("if the catalog cannot be read, the page says it could not point at the tool and selects nothing", async () => {
    backend.failures.list_catalog = { code: "DB_UNAVAILABLE", message: "Katalog okunamadı." };
    render(<ProjectsPage handoff={handoff()} />);
    const m = await matrix();
    expect(await m.findByText("Önerilen araç satırı vurgulanamadı.")).toBeInTheDocument();
    expect(spots()).toHaveLength(0);
  });

  it("the user picking another target while the request is still waiting cancels it: it never lands on a different project", async () => {
    const original = backend.handle;
    let release!: () => void;
    const gate = new Promise<void>((r) => { release = r; });
    backend.handle = async (c, a) => {
      if (c === "list_catalog") await gate;
      return original(c, a);
    };
    render(<ProjectsPage handoff={handoff()} />);
    const m = await matrix();
    await waitFor(() => expect(m.getByLabelText("Proje")).toBeInTheDocument());
    await userEvent.selectOptions(m.getByLabelText("Proje"), "diger-proje");
    await act(async () => release());
    await articleOf("Alfa Araç");
    expect(spots()).toHaveLength(0);
    expect(m.queryByText(/Öneriden açıldı/)).not.toBeInTheDocument();
    expect(m.getByLabelText("Proje")).toHaveValue("diger-proje");
  });
});
