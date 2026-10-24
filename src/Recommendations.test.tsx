// PRODUCT-03 / P02-RT-01 - Recommendations evaluation is context-safe: an answer belongs to the snapshot of inputs it was asked with and can never
// appear, finish "loading" or point anywhere under another context. Every race here uses deferred promises that the TEST settles, in a chosen order.
// This is evidence for the mocked IPC and jsdom only (no real window, WebView2 or screen reader).
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import recommendationsSource from "./pages/Recommendations.tsx?raw";
import App from "./App";
import { createBackend, pref, rec, type Backend } from "./test/backend";
import type { Evaluation, TaskInput } from "./types";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const calls = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd);
const go = async (name: string) => {
  await userEvent.click(within(screen.getByRole("navigation")).getByRole("button", { name }));
  return screen.findByRole("heading", { level: 1, name });
};

/** One held evaluate_task call. The test decides when and how it ends. */
interface Gate {
  input: TaskInput;
  resolve: (over?: Partial<Evaluation>, recs?: ReturnType<typeof rec>[]) => Promise<void>;
  reject: (error: unknown) => Promise<void>;
}
let gates: Gate[] = [];
function holdEvaluations() {
  const original = backend.handle;
  backend.handle = (cmd, args) => {
    if (cmd !== "evaluate_task") return original(cmd, args);
    backend.calls.push({ cmd, args });
    const input = args!.input as TaskInput;
    return new Promise((res, rej) => {
      gates.push({
        input,
        resolve: (over = {}, recs = []) => act(async () => res({ taskId: input.taskId, phase: input.phase ?? "start", recommendations: recs, suppressed: [], disclaimer: "Hiçbir şey kurulmadı.", ...over })),
        reject: (e) => act(async () => rej(e)),
      });
    });
  };
}

const RA = () => rec({ entryId: "anthropic-frontend-design", displayName: "Anthropic Frontend Design" });
const RB = () => rec({ entryId: "agent-browser", displayName: "Agent Browser" });
const cards = () => screen.queryAllByRole("heading", { level: 3 }).map((h) => h.textContent ?? "");
const hasCard = (name: string) => cards().some((t) => t.includes(name));
const LOADING = "Değerlendiriliyor…";

const openPage = async () => {
  render(<App />);
  await go("Öneriler");
  await screen.findByRole("option", { name: "Proje B" });
};
const typeTask = (text: string) => userEvent.type(screen.getByLabelText("Görev metni"), text);
const evaluate = () => userEvent.click(screen.getByRole("button", { name: "Değerlendir" }));
const waitGates = (n: number) => waitFor(() => expect(gates).toHaveLength(n));
const settle = () => act(async () => { await new Promise((r) => setTimeout(r, 25)); });

beforeEach(() => {
  backend = createBackend();
  gates = [];
  backend.projects = [
    { projectId: "p1", alias: "proje-a", displayName: "Proje A", kind: "reference", localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences: [pref({ entryId: "gitleaks" })] },
    { projectId: "p2", alias: "proje-b", displayName: "Proje B", kind: "reference", localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences: [] },
  ];
  holdEvaluations();
});

// Whatever happens on this page, it never stores or forgets a project preference by itself.
afterEach(() => {
  expect(calls("set_project_preference"), "set_project_preference").toHaveLength(0);
  expect(calls("reset_project_preference"), "reset_project_preference").toHaveLength(0);
  vi.restoreAllMocks();
});

describe("P02-RT-01 reproduction (this test failed before the fix: A's card appeared under B and its action wrote for B)", () => {
  it("A evaluates -> user selects B -> A resolves -> no A recommendation and no action under B", async () => {
    await openPage();
    await typeTask("React bileşeni tasarla");
    await evaluate();
    await waitGates(1);
    expect(gates[0].input.projectAlias).toBe("proje-a");
    await userEvent.selectOptions(screen.getByLabelText("Proje"), "proje-b");
    await gates[0].resolve({}, [RA()]);
    await settle();
    expect(hasCard("Anthropic Frontend Design")).toBe(false);
    expect(screen.queryByText("Öneri", { selector: ".badge" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Bu öneriyi matriste incele/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 2, name: "Sonuç" })).not.toBeInTheDocument();
    expect(screen.queryByText(LOADING)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Değerlendir" })).toBeEnabled();
  });
});

describe("overlapping evaluations of two projects, in both settlement orders", () => {
  const twoProjects = async () => {
    await openPage();
    await typeTask("React bileşeni tasarla");
    await evaluate(); // A
    await waitGates(1);
    await userEvent.selectOptions(screen.getByLabelText("Proje"), "proje-b");
    await evaluate(); // B (same text, other project)
    await waitGates(2);
    expect([gates[0].input.projectAlias, gates[1].input.projectAlias]).toEqual(["proje-a", "proje-b"]);
    expect(gates[1].input.taskId).not.toBe(gates[0].input.taskId); // a new project is a new task
  };

  it("A settles first, then B: A is never shown, A cannot end B's 'loading', only B's answer is on screen", async () => {
    await twoProjects();
    await gates[0].resolve({}, [RA()]);
    await settle();
    expect(hasCard("Anthropic Frontend Design")).toBe(false);
    expect(screen.getByText(LOADING)).toBeInTheDocument(); // B is still running and still owns the indicator
    await gates[1].resolve({}, [RB()]);
    expect(await screen.findByRole("heading", { level: 3, name: /Agent Browser/ })).toBeInTheDocument();
    expect(hasCard("Anthropic Frontend Design")).toBe(false);
    expect(screen.queryByText(LOADING)).not.toBeInTheDocument();
    expect(screen.getByText(/Değerlendirilen hedef:/)).toHaveTextContent("Proje B");
  });

  it("B settles first, then A: B stays, A's late answer changes nothing", async () => {
    await twoProjects();
    await gates[1].resolve({}, [RB()]);
    expect(await screen.findByRole("heading", { level: 3, name: /Agent Browser/ })).toBeInTheDocument();
    await gates[0].resolve({}, [RA()]);
    await settle();
    expect(cards().filter((t) => t.trim() !== "")).toHaveLength(1);
    expect(hasCard("Agent Browser")).toBe(true);
    expect(hasCard("Anthropic Frontend Design")).toBe(false);
    expect(screen.getByText(/Değerlendirilen hedef:/)).toHaveTextContent("Proje B");
  });
});

describe("stale errors and stale successes", () => {
  it("a rejection that arrives after the target changed shows no error, triggers no project reload and leaves the page usable", async () => {
    await openPage();
    await typeTask("React bileşeni tasarla");
    await evaluate();
    await waitGates(1);
    const listsBefore = calls("list_projects").length;
    await userEvent.selectOptions(screen.getByLabelText("Proje"), "proje-b");
    await gates[0].reject({ code: "PROJECT_NOT_FOUND", message: "Böyle bir proje kaydı yok." });
    await settle();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(calls("list_projects")).toHaveLength(listsBefore);
    expect(screen.getByLabelText("Proje")).toHaveValue("proje-b");
    expect(screen.getByRole("button", { name: "Değerlendir" })).toBeEnabled();
  });

  it("a stale error after a newer success does not replace or hide the newer result", async () => {
    await openPage();
    await typeTask("React bileşeni tasarla");
    await evaluate(); // A
    await waitGates(1);
    await userEvent.selectOptions(screen.getByLabelText("Proje"), "proje-b");
    await evaluate(); // B
    await waitGates(2);
    await gates[1].resolve({}, [RB()]);
    await screen.findByRole("heading", { level: 3, name: /Agent Browser/ });
    await gates[0].reject({ code: "EVAL_STALE_FAILURE", message: "eski hata" });
    await settle();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByText(/EVAL_STALE_FAILURE/)).not.toBeInTheDocument();
    expect(hasCard("Agent Browser")).toBe(true);
  });

  it("a stale success after a newer failure leaves the newer error on screen and shows nothing of A", async () => {
    await openPage();
    await typeTask("React bileşeni tasarla");
    await evaluate(); // A
    await waitGates(1);
    await userEvent.selectOptions(screen.getByLabelText("Proje"), "proje-b");
    await evaluate(); // B
    await waitGates(2);
    await gates[1].reject({ code: "DB_BUSY", message: "Veritabanı meşgul." });
    expect(await screen.findByRole("alert")).toHaveTextContent("DB_BUSY");
    await gates[0].resolve({}, [RA()]);
    await settle();
    expect(screen.getByRole("alert")).toHaveTextContent("DB_BUSY");
    expect(hasCard("Anthropic Frontend Design")).toBe(false);
    expect(screen.queryByRole("heading", { level: 2, name: "Sonuç" })).not.toBeInTheDocument();
  });

  it("an answer for another task or phase than the one asked is refused with a readable error and shows no suggestion", async () => {
    await openPage();
    await typeTask("React bileşeni tasarla");
    await evaluate();
    await waitGates(1);
    await gates[0].resolve({ taskId: "baska-gorev" }, [RA()]);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("EVALUATION_ANSWER_MISMATCH");
    expect(alert).toHaveTextContent(/Sonuç gösterilmedi/);
    expect(hasCard("Anthropic Frontend Design")).toBe(false);
  });
});

describe("every relevant input invalidates a pending evaluation, one input at a time", () => {
  const starts = async () => {
    await openPage();
    await typeTask("React bileşeni tasarla");
    await evaluate();
    await waitGates(1);
    expect(screen.getByText(LOADING)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Değerlendir" })).toBeDisabled();
  };
  const afterStaleAnswer = async () => {
    await gates[0].resolve({}, [RA()]);
    await settle();
    expect(hasCard("Anthropic Frontend Design"), "stale card").toBe(false);
    expect(screen.queryByRole("heading", { level: 2, name: "Sonuç" })).not.toBeInTheDocument();
    expect(screen.queryByText(LOADING)).not.toBeInTheDocument();
    expect(screen.getByText(/süren değerlendirmenin sonucu atıldı/)).toBeInTheDocument();
    const evaluateAgain = screen.getByRole("button", { name: "Değerlendir" });
    expect(evaluateAgain).toBeEnabled();
    return evaluateAgain;
  };

  const cases: [string, () => Promise<unknown>][] = [
    ["task text", () => userEvent.type(screen.getByLabelText("Görev metni"), " ve ek")],
    ["evidence file names", () => userEvent.type(screen.getByLabelText(/İlgili dosya adları/), "Tablo.tsx")],
    ["hypothetical plugin scope", () => userEvent.selectOptions(screen.getByLabelText(/Frontend Design eklentisi zaten/), "global")],
    ["late phase", () => userEvent.click(screen.getByRole("checkbox", { name: /Görev ortasında yeni bir alan/ }))],
    ["agent surface", () => userEvent.selectOptions(screen.getByLabelText("Ajan yüzeyi"), "local-cli")],
    ["operating system", () => userEvent.selectOptions(screen.getByLabelText("İşletim sistemi"), "macos-arm64")],
    ["project", () => userEvent.selectOptions(screen.getByLabelText("Proje"), "proje-b")],
    ["task reset", () => userEvent.click(screen.getByRole("button", { name: "Yeni görev başlat" }))],
    ["example task", async () => { await screen.findByRole("option", { name: "React bileşeni tasarlamak" }); await userEvent.selectOptions(screen.getByLabelText(/Örnek görev/), "react-component"); }],
  ];
  for (const [name, change] of cases) {
    it(`changing only the ${name} discards the answer, ends 'loading' and allows a fresh request under a new task id`, async () => {
      await starts();
      await change();
      expect(screen.queryByText(LOADING)).not.toBeInTheDocument(); // invalidation ends the indicator at once
      const again = await afterStaleAnswer();
      const firstTask = gates[0].input.taskId;
      await userEvent.click(again);
      await waitGates(2);
      expect(gates[1].input.taskId, "a discarded evaluation may have logged suggestions nobody saw: its task id is not reused").not.toBe(firstTask);
      await gates[1].resolve({}, [RB()]);
      expect(await screen.findByRole("heading", { level: 3, name: /Agent Browser/ })).toBeInTheDocument();
    });
  }

  it("what the second request sends is the form as it is NOW, not the first request's values", async () => {
    await starts();
    await userEvent.type(screen.getByLabelText("Görev metni"), " ve ek");
    await userEvent.type(screen.getByLabelText(/İlgili dosya adları/), "Tablo.tsx");
    await userEvent.selectOptions(screen.getByLabelText(/Frontend Design eklentisi zaten/), "project");
    await userEvent.click(screen.getByRole("checkbox", { name: /Görev ortasında yeni bir alan/ }));
    await userEvent.click(screen.getByRole("button", { name: "Değerlendir" }));
    await waitGates(2);
    expect(gates[0].input).toMatchObject({ taskText: "React bileşeni tasarla", evidenceFiles: [], phase: "start", availability: [] });
    expect(gates[1].input).toMatchObject({
      taskText: "React bileşeni tasarla ve ek", evidenceFiles: ["Tablo.tsx"], phase: "late", availability: [{ entryId: "anthropic-frontend-design", scope: "project", enabled: true }],
    });
  });
});

describe("repeated clicks, indicator ownership, unmount", () => {
  it("a double click sends exactly one request; the button is unavailable until it ends; the answer is shown once", async () => {
    await openPage();
    await typeTask("React bileşeni tasarla");
    await userEvent.dblClick(screen.getByRole("button", { name: "Değerlendir" }));
    await waitGates(1);
    await userEvent.click(screen.getByRole("button", { name: "Değerlendir" })); // disabled: nothing happens
    await settle();
    expect(calls("evaluate_task")).toHaveLength(1);
    await gates[0].resolve({}, [RA()]);
    expect(await screen.findByRole("heading", { level: 3, name: /Anthropic Frontend Design/ })).toBeInTheDocument();
    expect(cards().filter((t) => t.includes("Anthropic Frontend Design"))).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Değerlendir" })).toBeEnabled();
    expect(calls("evaluate_task")).toHaveLength(1);
  });

  it("only the latest request can finish the loading indicator: a discarded request settling must not end the newer one's", async () => {
    await openPage();
    await typeTask("React bileşeni tasarla");
    await evaluate(); // 1
    await waitGates(1);
    await userEvent.type(screen.getByLabelText("Görev metni"), "!"); // discards 1
    await evaluate(); // 2
    await waitGates(2);
    expect(screen.getByText(LOADING)).toBeInTheDocument();
    await gates[0].resolve({}, [RA()]);
    await settle();
    expect(screen.getByText(LOADING)).toBeInTheDocument(); // still 2's
    expect(screen.getByRole("button", { name: "Değerlendir" })).toBeDisabled();
    await gates[1].resolve({}, [RB()]);
    expect(await screen.findByRole("heading", { level: 3, name: /Agent Browser/ })).toBeInTheDocument();
    expect(screen.queryByText(LOADING)).not.toBeInTheDocument();
    expect(hasCard("Anthropic Frontend Design")).toBe(false);
  });

  it("leaving the page with an evaluation pending: its answer later changes nothing and a fresh visit starts empty", async () => {
    const errors = vi.spyOn(console, "error").mockImplementation(() => undefined);
    await openPage();
    await typeTask("React bileşeni tasarla");
    await evaluate();
    await waitGates(1);
    await go("Ana Sayfa");
    await gates[0].resolve({}, [RA()]);
    await settle();
    expect(errors).not.toHaveBeenCalled();
    await go("Öneriler");
    expect(screen.queryByRole("heading", { level: 2, name: "Sonuç" })).not.toBeInTheDocument();
    expect(hasCard("Anthropic Frontend Design")).toBe(false);
    expect(screen.getByLabelText("Görev metni")).toHaveValue("");
    expect(calls("evaluate_task")).toHaveLength(1);
  });

  it("a rejection after leaving the page is silent too", async () => {
    const errors = vi.spyOn(console, "error").mockImplementation(() => undefined);
    await openPage();
    await typeTask("x");
    await evaluate();
    await waitGates(1);
    await go("Ayarlar");
    await gates[0].reject({ code: "GONE", message: "gitti" });
    await settle();
    expect(errors).not.toHaveBeenCalled();
    expect(screen.queryByText(/GONE/)).not.toBeInTheDocument();
  });
});

describe("a project that disappears", () => {
  it("when the backend says the project is gone, the list is re-read, the selection falls back WITH a visible note, and nothing from the old project stays", async () => {
    await openPage();
    await userEvent.selectOptions(screen.getByLabelText("Proje"), "proje-b");
    await typeTask("React bileşeni tasarla");
    await evaluate();
    await waitGates(1);
    backend.projects = backend.projects.filter((p) => p.alias !== "proje-b"); // removed elsewhere meanwhile
    const listsBefore = calls("list_projects").length;
    await gates[0].reject({ code: "PROJECT_NOT_FOUND", message: "Böyle bir proje kaydı yok." });
    await waitFor(() => expect(calls("list_projects").length).toBe(listsBefore + 1));
    expect(await screen.findByText(/Seçili proje .*artık listede yok/)).toBeInTheDocument();
    expect(screen.getByLabelText("Proje")).toHaveValue("proje-a");
    expect(screen.queryByRole("heading", { level: 2, name: "Sonuç" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Değerlendir" })).toBeEnabled();
  });

  it("a backend refusal with a project that still exists is shown as before and the selection is kept", async () => {
    await openPage();
    await typeTask("x");
    await evaluate();
    await waitGates(1);
    await gates[0].reject({ code: "PROJECT_NOT_FOUND", message: "Böyle bir proje kaydı yok." });
    expect(await screen.findByText(/PROJECT_NOT_FOUND/)).toBeInTheDocument();
    expect(screen.getByLabelText("Proje")).toHaveValue("proje-a");
  });
});

describe("a finished result stays bound to the inputs it answers", () => {
  const finished = async () => {
    await openPage();
    await typeTask("React bileşeni tasarla");
    await evaluate();
    await waitGates(1);
    await gates[0].resolve({}, [RA()]);
    await screen.findByRole("heading", { level: 3, name: /Anthropic Frontend Design/ });
  };

  it("a fresh result names the evaluated target and carries no 'outdated' warning", async () => {
    await finished();
    expect(screen.getByText(/Değerlendirilen hedef:/)).toHaveTextContent("Proje A · Windows x64 · Claude Code (terminal)");
    expect(screen.queryByText(/önceki girdilerle üretildi/)).not.toBeInTheDocument();
  });

  it("editing the task text afterwards marks the result as produced by earlier inputs - not as current - and its handoff still points at the evaluated target", async () => {
    await finished();
    await userEvent.type(screen.getByLabelText("Görev metni"), " değişti");
    const note = screen.getByText(/önceki girdilerle üretildi/);
    expect(note).toHaveTextContent("görev metni");
    expect(note).toHaveAttribute("role", "note");
    const sentBefore = calls("evaluate_task").length;
    await userEvent.click(screen.getByRole("button", { name: /^Bu öneriyi matriste incele/ }));
    expect(await screen.findByRole("heading", { level: 1, name: "Projeler" })).toBeInTheDocument();
    const matrix = within(await screen.findByRole("region", { name: "Proje Araç Matrisi" }));
    expect(matrix.getByLabelText("Proje")).toHaveValue("proje-a"); // the evaluated project, not whatever the form said later
    expect(calls("evaluate_task")).toHaveLength(sentBefore); // navigating re-evaluates nothing
  });

  it("changing the late phase or the assumed plugin scope marks the result outdated, and a different project / system / surface removes it", async () => {
    await finished();
    await userEvent.click(screen.getByRole("checkbox", { name: /Görev ortasında yeni bir alan/ }));
    expect(screen.getByText(/önceki girdilerle üretildi/)).toHaveTextContent("görev aşaması");
    await userEvent.selectOptions(screen.getByLabelText(/Frontend Design eklentisi zaten/), "global");
    expect(screen.getByText(/önceki girdilerle üretildi/)).toHaveTextContent("varsayımsal kurulum durumu");
    await userEvent.selectOptions(screen.getByLabelText("İşletim sistemi"), "macos-arm64");
    expect(screen.queryByRole("heading", { level: 2, name: "Sonuç" })).not.toBeInTheDocument();
  });

  it("the handoff of each card carries ITS evaluation's project, system, surface and tool (checked by what the matrix then shows)", async () => {
    backend.nextEvaluation = { taskId: "t", phase: "start", recommendations: [], suppressed: [], disclaimer: "x" };
    await openPage();
    await userEvent.selectOptions(screen.getByLabelText("Proje"), "proje-b");
    await userEvent.selectOptions(screen.getByLabelText("İşletim sistemi"), "macos-arm64");
    await userEvent.selectOptions(screen.getByLabelText("Ajan yüzeyi"), "antigravity-ide");
    await typeTask("React bileşeni tasarla");
    await evaluate();
    await waitGates(1);
    await gates[0].resolve({}, [RA(), RB()]);
    await screen.findByRole("heading", { level: 3, name: /Agent Browser/ });
    const buttons = screen.getAllByRole("button", { name: /^Bu öneriyi matriste incele/ });
    expect(buttons).toHaveLength(2);
    expect(buttons[1]).toHaveAccessibleName("Bu öneriyi matriste incele (Agent Browser; Proje B · macOS Apple Silicon · Antigravity IDE)");
  });
});

describe("the Recommendations page has no second write path", () => {
  it("its source names neither setProjectPreference nor resetProjectPreference nor their commands, and calls only the three read / evaluate APIs", () => {
    for (const banned of ["setProjectPreference", "resetProjectPreference", "set_project_preference", "reset_project_preference", "fetch(", "window.open", "localStorage", "@tauri-apps/api/core", "invoke(", "dangerouslySetInnerHTML"]) {
      expect(recommendationsSource.includes(banned), banned).toBe(false);
    }
    const used = [...new Set([...recommendationsSource.matchAll(/\bapi\.([A-Za-z]+)/g)].map((m) => m[1]))].sort();
    expect(used).toEqual(["evaluateTask", "listProjects", "listScenarios"]);
  });

  it("pressing every control on the page after a result - the card's too - never calls a preference command, an install command or the network", async () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal("fetch", fetchSpy);
    await openPage();
    await typeTask("React bileşeni tasarla");
    await evaluate();
    await waitGates(1);
    await gates[0].resolve({}, [RA(), RB()]);
    await screen.findByRole("heading", { level: 3, name: /Agent Browser/ });
    const main = within(screen.getByRole("main"));
    await userEvent.click(main.getByRole("button", { name: "Yeni görev başlat" }));
    await evaluate().catch(() => undefined);
    expect(main.queryByRole("button", { name: /kullanmak istiyorum|Şimdi değil|Reddet|Kaydet|Etkinleştir|Kur\b/ })).not.toBeInTheDocument();
    await userEvent.click(main.getByRole("button", { name: /^Bu hedefi Proje Araç Matrisi/ }));
    await screen.findByRole("region", { name: "Proje Araç Matrisi" });
    // the matrix (not this page) reads the stored inventory since PRODUCT-04; that is a database read and must not have happened before the handoff
    for (const bad of ["set_project_preference", "reset_project_preference", "prepare_install", "confirm_install", "plan_install", "run_discovery_scan", "set_catalog_entry_enabled", "list_install_states"]) {
      expect(calls(bad), bad).toHaveLength(0);
    }
    // the Dashboard tile (PRODUCT-05) and the matrix read the stored inventory; the Recommendations page itself never does: nothing between its own first read and the evaluation
    const order = backend.calls.map((c) => c.cmd);
    const between = order.slice(order.indexOf("list_scenarios"), order.indexOf("evaluate_task") + 1);
    expect(between).not.toContain("list_inventory");
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("the only commands the Recommendations page itself issues are listing projects and scenarios and evaluating the task", async () => {
    await openPage();
    await typeTask("React bileşeni tasarla");
    await evaluate();
    await waitGates(1);
    await gates[0].resolve({}, [RA()]);
    await screen.findByRole("heading", { level: 3, name: /Anthropic Frontend Design/ });
    const beyondTheShell = new Set(backend.calls.map((c) => c.cmd));
    // list_inventory comes only from the Dashboard's review tile (PRODUCT-05), which has finished before this page issues its own first command
    for (const cmd of beyondTheShell) expect(["get_app_info", "get_settings", "get_dashboard", "list_projects", "list_inventory", "list_scenarios", "evaluate_task"], cmd).toContain(cmd);
    const order = backend.calls.map((c) => c.cmd);
    const own = order.slice(order.indexOf("list_scenarios"));
    expect(own.filter((c) => c !== "list_projects" && c !== "list_scenarios" && c !== "evaluate_task")).toEqual([]);
  });
});
