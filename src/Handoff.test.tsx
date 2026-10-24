// PRODUCT-02 - Recommendations -> Project Tool Workspace context handoff, through the real App (a typed local callback, no router, no deep link).
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { createBackend, makeEntry, pref, rec, type Backend } from "./test/backend";
import type { ProjectView } from "./types";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const calls = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd);
const go = async (name: string) => {
  await userEvent.click(within(screen.getByRole("navigation")).getByRole("button", { name }));
  return screen.findByRole("heading", { level: 1, name });
};
const project = (alias: string, name: string, preferences: ProjectView["preferences"] = []): ProjectView => ({
  projectId: `id-${alias}`, alias, displayName: name, kind: "reference", localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences,
});
const matrix = async () => within(await screen.findByRole("region", { name: "Proje Araç Matrisi" }));
const OPEN = "Bu hedefi Proje Araç Matrisi’nde aç";

/** Chooses project / operating system / agent surface on the Recommendations page. */
const pick = async (alias: string, platform: string, surface: string) => {
  await screen.findByRole("option", { name: new RegExp(alias === "benim-sitem" ? "Benim sitem" : "Diğer proje") });
  await userEvent.selectOptions(screen.getByLabelText("Proje"), alias);
  await userEvent.selectOptions(screen.getByLabelText("İşletim sistemi"), platform);
  await userEvent.selectOptions(screen.getByLabelText("Ajan yüzeyi"), surface);
};
const shownTarget = async () => {
  const m = await matrix();
  return {
    project: (m.getByLabelText("Proje") as HTMLSelectElement).value,
    platform: (m.getByLabelText("İşletim sistemi") as HTMLSelectElement).value,
    surface: (m.getByLabelText("Ajan yüzeyi") as HTMLSelectElement).value,
    text: m.getByText(/Hedef:/).textContent,
  };
};

beforeEach(() => {
  backend = createBackend();
  backend.projects = [
    { ...project("ornek-web-sitesi", "Örnek: Web sitesi", [pref({ entryId: "gitleaks", entryName: "gitleaks", agentSurface: "local-cli", platformTarget: "windows-x64", decision: "accepted" })]), kind: "synthetic" },
    project("benim-sitem", "Benim sitem"),
    project("diger-proje", "Diğer proje"),
  ];
});

describe("Recommendations -> matrix handoff", () => {
  it("opens exactly the project, operating system and agent surface chosen in Recommendations", async () => {
    render(<App />);
    await go("Öneriler");
    await pick("benim-sitem", "macos-arm64", "antigravity-ide");
    await userEvent.click(screen.getByRole("button", { name: OPEN }));
    expect(await screen.findByRole("heading", { level: 1, name: "Projeler" })).toBeInTheDocument();
    await waitFor(async () => expect((await shownTarget()).project).toBe("benim-sitem"));
    expect(await shownTarget()).toMatchObject({ project: "benim-sitem", platform: "macos-arm64", surface: "antigravity-ide" });
    expect((await shownTarget()).text).toContain("Benim sitem · macOS Apple Silicon · Antigravity IDE");
  });

  it("only moves the user: no task is evaluated, nothing is written or installed, and the only extra read is the stored inventory (never a scan)", async () => {
    render(<App />);
    await go("Öneriler");
    await pick("benim-sitem", "macos-arm64", "local-cli");
    const before = backend.calls.length;
    await userEvent.click(screen.getByRole("button", { name: OPEN }));
    await screen.findByRole("heading", { level: 1, name: "Projeler" });
    await matrix();
    const after = backend.calls.slice(before).map((c) => c.cmd);
    for (const cmd of new Set(after)) expect(["list_projects", "list_catalog", "list_inventory"], cmd).toContain(cmd);
    for (const bad of ["evaluate_task", "set_project_preference", "reset_project_preference", "prepare_install", "confirm_install", "list_install_states", "plan_install", "run_discovery_scan", "set_catalog_entry_enabled"]) {
      expect(calls(bad), bad).toHaveLength(0);
    }
  });

  it("is keyboard operable: Enter on the focused button hands the context over", async () => {
    render(<App />);
    await go("Öneriler");
    await pick("diger-proje", "windows-x64", "claude-desktop-code");
    const button = screen.getByRole("button", { name: OPEN });
    button.focus();
    expect(button).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    await screen.findByRole("heading", { level: 1, name: "Projeler" });
    await waitFor(async () => expect(await shownTarget()).toMatchObject({ project: "diger-proje", surface: "claude-desktop-code" }));
  });

  it("is disabled while there is no project to hand over", async () => {
    backend.projects = [];
    render(<App />);
    await go("Öneriler");
    expect(screen.getByRole("button", { name: OPEN })).toBeDisabled();
  });

  it("back and forth: a second visit with a different target replaces the first, and nothing of the first leaks into the second", async () => {
    render(<App />);
    await go("Öneriler");
    await pick("benim-sitem", "macos-arm64", "antigravity-ide");
    await userEvent.click(screen.getByRole("button", { name: OPEN }));
    await waitFor(async () => expect(await shownTarget()).toMatchObject({ project: "benim-sitem", platform: "macos-arm64", surface: "antigravity-ide" }));

    // save something in the first target, so that a confirmation exists
    const m = await matrix();
    await userEvent.selectOptions(m.getByLabelText("Ajan yüzeyi"), "local-cli");
    const row = within((await m.findByRole("heading", { level: 3, name: "agent-browser" })).closest("article")!);
    await userEvent.selectOptions(row.getByLabelText(/^Karar/), "rejected");
    await userEvent.click(row.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ }));
    expect(await m.findByText(/agent-browser: “Reddet \(tekrar sorma\)” isteği kaydedildi/)).toBeInTheDocument();

    await go("Öneriler");
    await pick("diger-proje", "windows-x64", "claude-code-terminal");
    await userEvent.click(screen.getByRole("button", { name: OPEN }));
    await waitFor(async () => expect(await shownTarget()).toMatchObject({ project: "diger-proje", platform: "windows-x64", surface: "claude-code-terminal" }));
    expect((await matrix()).queryByText(/isteği kaydedildi/)).not.toBeInTheDocument(); // the earlier confirmation belongs to another target
    const other = within(((await (await matrix()).findByRole("heading", { level: 3, name: "agent-browser" }))).closest("article")!);
    expect(other.getByText("Henüz karar yok")).toBeInTheDocument(); // the first project's wish is not the second's
  });

  it("a normal visit to Projeler after a handoff shows the default target, not the old handoff", async () => {
    render(<App />);
    await go("Öneriler");
    await pick("benim-sitem", "macos-arm64", "antigravity-ide");
    await userEvent.click(screen.getByRole("button", { name: OPEN }));
    await waitFor(async () => expect((await shownTarget()).platform).toBe("macos-arm64"));
    await go("Ana Sayfa");
    await go("Projeler");
    await waitFor(async () => expect(await shownTarget()).toMatchObject({ platform: "windows-x64", surface: "claude-code-terminal" }));
    // ... and a visit to Öneriler and back through the sidebar does not resurrect it either
    await go("Öneriler");
    await go("Projeler");
    expect(await shownTarget()).toMatchObject({ platform: "windows-x64", surface: "claude-code-terminal" });
  });

  it("a project that was removed meanwhile is handled safely: the first project is shown and the page says so", async () => {
    render(<App />);
    await go("Öneriler");
    await pick("benim-sitem", "macos-arm64", "local-cli");
    backend.projects = backend.projects.filter((p) => p.alias !== "benim-sitem"); // removed after the user chose it
    await userEvent.click(screen.getByRole("button", { name: OPEN }));
    await screen.findByRole("heading", { level: 1, name: "Projeler" });
    const m = await matrix();
    expect(await m.findByText(/İstenen proje artık yok veya bulunamadı/)).toBeInTheDocument();
    expect(await shownTarget()).toMatchObject({ project: "ornek-web-sitesi", platform: "macos-arm64", surface: "local-cli" }); // OS and surface are kept
    expect(m.getAllByRole("heading", { level: 3 }).length).toBeGreaterThan(0); // the matrix still works
  });

  it("a recommendation card is not a second place to decide: it opens the matrix on that tool and records nothing", async () => {
    backend.nextEvaluation = { taskId: "t", phase: "start", recommendations: [rec()], suppressed: [], disclaimer: "Hiçbir şey kurulmadı." };
    backend.entries.push(makeEntry("anthropic-frontend-design"));
    render(<App />);
    await go("Öneriler");
    await pick("benim-sitem", "windows-x64", "claude-code-terminal");
    await userEvent.type(screen.getByLabelText("Görev metni"), "React bileşeni tasarla");
    await userEvent.click(screen.getByRole("button", { name: "Değerlendir" }));
    await userEvent.click(await screen.findByRole("button", { name: /^Bu öneriyi matriste incele/ }));
    await waitFor(async () => expect(await shownTarget()).toMatchObject({ project: "benim-sitem", platform: "windows-x64", surface: "claude-code-terminal" }));
    expect(calls("set_project_preference")).toHaveLength(0);
    expect(calls("reset_project_preference")).toHaveLength(0);
  });
});
