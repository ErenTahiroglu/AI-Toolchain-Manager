// PRODUCT-01 - component and safety tests of the Project Tool Workspace, rendered inside the real Projects page against the in-memory backend.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import workspaceComponentSource from "./components/ProjectToolWorkspace.tsx?raw";
import workspaceLogicSource from "./workspace.ts?raw";
import { ProjectsPage } from "./pages/Projects";
import { createBackend, makeEntry, pref, type Backend } from "./test/backend";
import type { PreferenceView, ProjectView } from "./types";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const WIN = "windows-x64";
const MAC = "macos-arm64";
const CC = "claude-code-terminal";
const CLI = "local-cli";
const ev = (platformTarget: string, agentSurface: string, compatibilityState: string, limitation = `kayıt: ${compatibilityState}`) => ({ platformTarget, agentSurface, compatibilityState, limitation });

const alfa = () =>
  makeEntry("alfa", {
    displayName: "Alfa Araç", purpose: "Alfa'nın amacı", record: { targetEvidence: [ev(WIN, CC, "vendor-declared", "yalnızca üretici beyanı"), ev(WIN, CLI, "independently-tested"), ev(MAC, CC, "unsupported", "bu platformda yok")] },
  });
const beta = () => makeEntry("beta", { displayName: "Beta Eklenti", kind: "agent_plugin", purpose: "Beta'nın amacı", record: { targetEvidence: [ev(WIN, CLI, "vendor-declared")] } }); // nothing recorded for WIN/CC
const gama = () => makeEntry("gama", { displayName: "Gama Kapalı", enabled: false, purpose: "Katalogda kapalı bir araç", record: { targetEvidence: [ev(WIN, CC, "vendor-declared")] } });
const delta = () => makeEntry("delta", { displayName: "Delta Kısıtlı", executionPolicy: "blocked-until-scoped-authorization", reviewState: "quarantined", record: { targetEvidence: [ev(WIN, CC, "vendor-declared")] } });
const eski = () => makeEntry("eski", { displayName: "Eski Kayıt", removedAt: "2026-10-18T00:00:00Z", enabled: false, record: { targetEvidence: [ev(WIN, CC, "vendor-declared")] } });
const yeni = () => makeEntry("yeni", { displayName: "Yeni Kayıt", record: { targetEvidence: [ev(WIN, CC, "vendor-declared")] } }); // never has a wish

const project = (alias: string, name: string, preferences: PreferenceView[] = [], kind: ProjectView["kind"] = "reference"): ProjectView => ({
  projectId: `id-${alias}`, alias, displayName: name, kind, localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences,
});
const wish = (entryId: string, decision: PreferenceView["decision"], over: Partial<PreferenceView> = {}) =>
  pref({ entryId, entryName: entryId, agentSurface: CC, platformTarget: WIN, decision, ...over });

const calls = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd);
const region = async () => within(await screen.findByRole("region", { name: "Proje Araç Matrisi" }));
const rowOf = async (name: string) => {
  const r = await region();
  return within((await r.findByRole("heading", { level: 3, name })).closest("article") as HTMLElement);
};
const open = async () => {
  render(<ProjectsPage />);
  await rowOf("Alfa Araç");
  return region();
};
const saveButton = (row: ReturnType<typeof within>) => row.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ });
const choose = async (row: ReturnType<typeof within>, value: string) => userEvent.selectOptions(row.getByLabelText(/^Karar/), value);

/** Holds the next backend call of `cmd` until released, and records when it STARTED. */
function hold(cmd: string) {
  const original = backend.handle;
  let release!: () => void;
  const gate = new Promise<void>((r) => { release = r; });
  const started: unknown[] = [];
  backend.handle = async (c, a) => {
    if (c === cmd) {
      started.push(a);
      await gate;
    }
    return original(c, a);
  };
  return { release, started };
}

beforeEach(() => {
  backend = createBackend();
  backend.entries = [alfa(), beta(), gama(), delta(), eski(), yeni()];
  backend.projects = [
    project("ornek-web-sitesi", "Örnek: Web sitesi", [wish("alfa", "accepted")], "synthetic"),
    project("benim-sitem", "Benim sitem", []),
  ];
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("what the workspace shows for one project and one target", () => {
  it("lists every active catalog entry (and a removed one only when a wish exists), with catalog data, source, license and compatibility", async () => {
    await open();
    const r = await region();
    expect(r.getAllByRole("heading", { level: 3 }).map((h) => h.textContent)).toEqual(["Alfa Araç", "Beta Eklenti", "Delta Kısıtlı", "Gama Kapalı", "Yeni Kayıt"]);
    const a = await rowOf("Alfa Araç");
    expect(a.getByText("Çalıştırılabilir araç")).toBeInTheDocument();
    expect(a.getByText("Alfa'nın amacı")).toBeInTheDocument();
    expect(a.getByText(/https:\/\/github\.com\/example\/alfa/)).toBeInTheDocument();
    expect(a.getByText(/Herkese açık belgelerden anlık görüntü \(çevrimdışı\)/)).toBeInTheDocument();
    expect(a.getByText(/Lisans bilinen: MIT/)).toBeInTheDocument();
    expect(a.getByText("Üretici belirtmiş (test edilmedi)")).toBeInTheDocument();
    expect(a.getByText(/yalnızca üretici beyanı/)).toBeInTheDocument();
    expect(a.getByText(/Sınırlamalar ve bilinmeyenler \(2\)/)).toBeInTheDocument();
    expect(r.getByText(/Hedef:/)).toHaveTextContent("Örnek: Web sitesi · Windows x64 · Claude Code (terminal)");
  });

  it("states that it is a planner and does not inspect this computer", async () => {
    await open();
    const r = await region();
    expect(r.getByText(/planlama ekranıdır/)).toBeInTheDocument();
    expect(r.getByText(/bilgisayarı denetlediği anlamına gelmez/)).toBeInTheDocument();
  });

  it("a catalog entry that is switched off in the catalog is a catalog mark - not 'not installed', not 'unavailable on this computer'", async () => {
    await open();
    const g = await rowOf("Gama Kapalı");
    expect(g.getByText("Katalogda kapalı")).toBeInTheDocument();
    expect(g.getByText(/yalnızca bir katalog işaretidir; kurulum veya ajan durumu değildir/)).toBeInTheDocument();
    expect(g.getByText(/Kurulum: Kurulu değil \/ yönetilmiyor/)).toBeInTheDocument(); // the separate record, unchanged by the catalog switch
    const accepted = g.getByRole("option", { name: /Bu projede kullanmak istiyorum/ });
    expect(accepted).toBeDisabled();
    expect(g.getByText(/Katalogda kapalı: önce Araç Kütüphanesi sayfasında/)).toBeInTheDocument();
  });

  it("unknown compatibility stays unknown: no invented support, 'accepted' unavailable, 'not now' and 'reject' still possible", async () => {
    await open();
    const b = await rowOf("Beta Eklenti"); // no evidence for Windows + Claude Code terminal
    expect(b.getByText("Bilinmiyor", { selector: "strong" })).toBeInTheDocument();
    expect(b.getByText(/kayıtlı kanıt yok; “desteklenmiyor” değil/)).toBeInTheDocument();
    expect(b.getByRole("option", { name: /Bu projede kullanmak istiyorum/ })).toBeDisabled();
    expect(b.getByRole("option", { name: /Şimdi değil/ })).toBeEnabled();
    expect(b.getByRole("option", { name: /Reddet/ })).toBeEnabled();
    expect(b.getByRole("option", { name: /Bu projede kapalı tut/ })).toBeEnabled();
    expect(b.getByText(/uyumluluk bilinmiyor veya desteklenmiyor/)).toBeInTheDocument();
    // the same entry IS declared for another surface, and only there
    await userEvent.selectOptions((await region()).getByLabelText("Ajan yüzeyi"), CLI);
    const b2 = await rowOf("Beta Eklenti");
    expect(b2.getByText("Üretici belirtmiş (test edilmedi)")).toBeInTheDocument();
    expect(b2.getByRole("option", { name: /Bu projede kullanmak istiyorum/ })).toBeEnabled();
  });

  it("shows 'unsupported' as unsupported and 'independently tested' with its own wording", async () => {
    await open();
    const r = await region();
    await userEvent.selectOptions(r.getByLabelText("İşletim sistemi"), MAC);
    expect((await rowOf("Alfa Araç")).getByText("Desteklenmiyor")).toBeInTheDocument();
    await userEvent.selectOptions(r.getByLabelText("İşletim sistemi"), WIN);
    await userEvent.selectOptions(r.getByLabelText("Ajan yüzeyi"), CLI);
    expect((await rowOf("Alfa Araç")).getByText("Bağımsız olarak test edilmiş")).toBeInTheDocument();
  });

  it("an execution-policy entry can be read and rejected but never 'wanted', and says why", async () => {
    await open();
    const d = await rowOf("Delta Kısıtlı");
    expect(d.getByText("Karantinada")).toBeInTheDocument();
    expect(d.getByRole("option", { name: /Bu projede kullanmak istiyorum/ })).toBeDisabled();
    expect(d.getByText(/Kapsamlı yazılı yetki olmadan bu araç için “kullanmak istiyorum” seçilemez/)).toBeInTheDocument();
    expect(d.getByRole("option", { name: /Reddet/ })).toBeEnabled();
  });
});

describe("a wish is not a state", () => {
  it("'accepted' reads as a wish, and the real state stays separate: not installed, not verified, not active in any agent", async () => {
    await open();
    const a = await rowOf("Alfa Araç");
    expect(a.getByText("Bu projede kullanmak istiyorum", { selector: "strong" })).toBeInTheDocument();
    expect(a.getByText("Kurulum: Kurulu değil / yönetilmiyor")).toBeInTheDocument();
    expect(a.getByText("Doğrulama: Doğrulanmadı")).toBeInTheDocument();
    expect(a.getByText(/Kapsam: Kapsam bilinmiyor/)).toBeInTheDocument();
    expect(a.getByText(/Ajanda etkin mi: Bilinmiyor — bu uygulama ajan ayarlarına bakmaz/)).toBeInTheDocument();
    expect(a.queryByText(/Kurulu ve doğrulandı|Kuruldu|Doğrulandı|Etkinleştirildi|ajanda etkin\b(?! mi)/i)).not.toBeInTheDocument();
    const r = await region();
    expect(r.getByText(/1 kullanmak istiyorum/)).toBeInTheDocument();
    expect(r.getByText(/5 kurulu değil \/ yönetilmiyor · 5 doğrulanmadı/)).toBeInTheDocument();
    expect(r.queryByText(/kurulu olarak kaydedilmiş|doğrulanmış olarak kaydedilmiş|tanınmayan durum/)).not.toBeInTheDocument();
  });

  it("saving 'accepted' stores a wish through the typed command and says nothing was installed, verified or activated", async () => {
    await open();
    await userEvent.selectOptions((await region()).getByLabelText("Proje"), "benim-sitem");
    const a = await rowOf("Alfa Araç");
    expect(a.getByText("Henüz karar yok")).toBeInTheDocument();
    await choose(a, "accepted");
    await userEvent.click(saveButton(a));
    expect(calls("set_project_preference")).toEqual([
      { cmd: "set_project_preference", args: { alias: "benim-sitem", entryId: "alfa", agentSurface: CC, platformTarget: WIN, decision: "accepted", remindDays: null } },
    ]);
    const r = await region();
    expect(await r.findByText(/Alfa Araç: “Bu projede kullanmak istiyorum” isteği kaydedildi/)).toHaveTextContent(/hiçbir şey kurulmadı, doğrulanmadı veya bir ajanda etkinleştirilmedi/);
    await waitFor(() => expect(within(screen.getByRole("heading", { level: 3, name: "Alfa Araç" }).closest("article")!).getByText("Bu projede kullanmak istiyorum", { selector: "strong" })).toBeInTheDocument());
    const after = await rowOf("Alfa Araç");
    expect(after.getByText("Kurulum: Kurulu değil / yönetilmiyor")).toBeInTheDocument(); // still
    expect(calls("set_catalog_entry_enabled")).toHaveLength(0);
  });

  it("'disabled' is only a recorded wish: it deletes nothing, can be reset, and says it does not touch an agent plugin", async () => {
    backend.projects[0].preferences = [wish("alfa", "disabled")];
    await open();
    const a = await rowOf("Alfa Araç");
    expect(a.getByText("Bu projede kapalı tut", { selector: "strong" })).toBeInTheDocument();
    await choose(a, "rejected");
    await choose(a, "disabled");
    expect(a.getByText(/Kayıt silinmez ve ajandaki bir eklenti kapatılmaz/)).toBeInTheDocument();
    expect(a.getByRole("button", { name: /^Kararı sıfırla/ })).toBeEnabled();
    expect(calls("remove_catalog_entry")).toHaveLength(0);
    expect(calls("set_catalog_entry_enabled")).toHaveLength(0);
    expect(calls("reset_project_preference")).toHaveLength(0);
  });

  it("'postponed' sends a bounded number of days; 'accepted' sends none", async () => {
    await open();
    const y = await rowOf("Yeni Kayıt");
    await choose(y, "postponed");
    const days = y.getByLabelText(/^Kaç gün sonra hatırlat/);
    expect(days).toHaveAttribute("min", "1");
    expect(days).toHaveAttribute("max", "365");
    await userEvent.clear(days);
    await userEvent.type(days, "999");
    await userEvent.click(saveButton(y));
    expect(calls("set_project_preference").at(-1)!.args).toMatchObject({ entryId: "yeni", decision: "postponed", remindDays: 365 });
  });

  it("flags a decision made before a material capability change, and treats a plain version bump as information only", async () => {
    backend.projects[0].preferences = [wish("alfa", "rejected", { materialChange: true }), wish("yeni", "rejected", { revisionChanged: true })];
    await open();
    expect((await rowOf("Alfa Araç")).getByText(/Aracın yetenekleri veya uyumluluğu bu karardan sonra önemli biçimde değişti/)).toBeInTheDocument();
    const y = await rowOf("Yeni Kayıt");
    expect(y.getByText(/bu tek başına kararınızı geçersiz kılmaz/)).toBeInTheDocument();
    expect(y.queryByText(/önemli biçimde değişti/)).not.toBeInTheDocument();
    expect((await region()).getByText(/Karardan sonra önemli biçimde değişen: 1/)).toBeInTheDocument();
  });

  it("a removed catalog entry keeps its old wish visible, labelled removed, with no way to set a new one", async () => {
    backend.projects[0].preferences = [wish("alfa", "accepted"), wish("eski", "rejected")];
    await open();
    const e = await rowOf("Eski Kayıt");
    expect(e.getAllByText("katalogdan kaldırıldı").length).toBeGreaterThan(0);
    expect(e.getByText("Reddet (tekrar sorma)", { selector: "strong" })).toBeInTheDocument();
    expect(e.queryByLabelText(/^Karar/)).not.toBeInTheDocument();
    expect(e.queryByRole("button", { name: /Yalnızca tercihimi kaydet/ })).not.toBeInTheDocument();
    expect(e.getByText(/yeni karar kaydedilemez/)).toBeInTheDocument();
    await userEvent.click(e.getByRole("button", { name: /^Eski kararı sıfırla/ }));
    expect(calls("reset_project_preference")).toEqual([{ cmd: "reset_project_preference", args: { alias: "ornek-web-sitesi", entryId: "eski", agentSurface: CC, platformTarget: WIN } }]);
    expect(calls("set_project_preference")).toHaveLength(0);
    // once the wish is gone a removed entry is no longer listed at all
    await waitFor(() => expect(screen.queryByRole("heading", { level: 3, name: "Eski Kayıt" })).not.toBeInTheDocument());
  });
});

describe("scoping: project + tool + operating system + agent surface", () => {
  it("one project's wish never appears for another project, and each save carries its own alias", async () => {
    await open();
    expect((await rowOf("Alfa Araç")).getByText("Bu projede kullanmak istiyorum", { selector: "strong" })).toBeInTheDocument();
    const r = await region();
    await userEvent.selectOptions(r.getByLabelText("Proje"), "benim-sitem");
    const other = await rowOf("Alfa Araç");
    expect(other.getByText("Henüz karar yok")).toBeInTheDocument();
    expect(other.queryByText("Bu projede kullanmak istiyorum", { selector: "strong" })).not.toBeInTheDocument();
    expect(r.getByText(/0 kullanmak istiyorum/)).toBeInTheDocument();
    await choose(other, "rejected");
    await userEvent.click(saveButton(other));
    expect(calls("set_project_preference")[0].args).toMatchObject({ alias: "benim-sitem", entryId: "alfa" });
    expect(backend.projects[0].preferences.map((p) => [p.entryId, p.decision])).toEqual([["alfa", "accepted"]]); // the first project is untouched
    await userEvent.selectOptions(r.getByLabelText("Proje"), "ornek-web-sitesi");
    expect((await rowOf("Alfa Araç")).getByText("Bu projede kullanmak istiyorum", { selector: "strong" })).toBeInTheDocument();
  });

  it("the same project and tool on another surface or operating system is a separate wish with its own key", async () => {
    backend.projects[0].preferences = [
      wish("alfa", "accepted"),
      wish("alfa", "rejected", { agentSurface: CLI }),
      wish("alfa", "postponed", { platformTarget: MAC }),
    ];
    await open();
    const r = await region();
    const decision = async () => (await rowOf("Alfa Araç")).getByText(/Bu projede kullanmak istiyorum|Reddet \(tekrar sorma\)|Şimdi değil \(ertele\)|Henüz karar yok/, { selector: "strong" }).textContent;
    expect(await decision()).toBe("Bu projede kullanmak istiyorum");
    await userEvent.selectOptions(r.getByLabelText("Ajan yüzeyi"), CLI);
    expect(await decision()).toBe("Reddet (tekrar sorma)");
    await userEvent.selectOptions(r.getByLabelText("İşletim sistemi"), MAC);
    expect(await decision()).toBe("Henüz karar yok"); // (macOS, CLI) has no wish
    await userEvent.selectOptions(r.getByLabelText("Ajan yüzeyi"), CC);
    expect(await decision()).toBe("Şimdi değil (ertele)");
    // a save in this context carries exactly this context
    const a = await rowOf("Alfa Araç");
    await choose(a, "rejected");
    await userEvent.click(saveButton(a));
    expect(calls("set_project_preference").at(-1)!.args).toMatchObject({ alias: "ornek-web-sitesi", entryId: "alfa", agentSurface: CC, platformTarget: MAC, decision: "rejected" });
    await waitFor(async () => expect(await decision()).toBe("Reddet (tekrar sorma)"));
    // ... and the other two keys did not move
    expect(backend.projects[0].preferences.filter((p) => p.entryId === "alfa").map((p) => `${p.platformTarget}/${p.agentSurface}/${p.decision}`).sort()).toEqual([
      `${MAC}/${CC}/rejected`, `${WIN}/${CC}/accepted`, `${WIN}/${CLI}/rejected`,
    ]);
  });

  it("an answer for another key than the one asked for is refused, not shown as success", async () => {
    await open();
    const original = backend.handle;
    backend.handle = async (c, a) => {
      const r = await original(c, a);
      return c === "set_project_preference" ? { ...(r as PreferenceView), entryId: "yeni" } : r;
    };
    const b = await rowOf("Beta Eklenti");
    await choose(b, "rejected");
    await userEvent.click(saveButton(b));
    expect(await b.findByRole("alert")).toHaveTextContent("PREFERENCE_ANSWER_MISMATCH");
    expect((await region()).queryByText(/isteği kaydedildi/)).not.toBeInTheDocument();
  });
});

describe("loading, empty, failure, retry and rapid changes never leave a stale positive outcome", () => {
  it("shows a loading state first and no matrix until both lists are there", async () => {
    const gate = hold("list_catalog");
    render(<ProjectsPage />);
    const r = await region();
    expect(r.getAllByText("Yükleniyor…").length).toBeGreaterThan(0);
    expect(r.queryByRole("heading", { level: 3 })).not.toBeInTheDocument();
    expect(r.queryByText(/Katalogda hiç kayıt yok/)).not.toBeInTheDocument(); // loading is not "empty"
    await act(async () => gate.release());
    expect(await r.findByRole("heading", { level: 3, name: "Alfa Araç" })).toBeInTheDocument();
  });

  it("says so when the catalog is empty, and when a project has no wishes at all", async () => {
    backend.entries = [];
    render(<ProjectsPage />);
    expect(await (await region()).findByText(/Katalogda hiç kayıt yok/)).toBeInTheDocument();
  });

  it("with no wishes every row says 'no decision yet' and the summary counts them as undecided", async () => {
    backend.projects = [project("benim-sitem", "Benim sitem")];
    await open();
    const r = await region();
    expect(r.getAllByText("Henüz karar yok")).toHaveLength(5);
    expect(r.getByText(/5 karar yok/)).toBeInTheDocument();
  });

  it("no project at all: says so and offers no matrix", async () => {
    backend.projects = [];
    render(<ProjectsPage />);
    expect(await (await region()).findByText(/Henüz proje kaydı yok/)).toBeInTheDocument();
    expect((await region()).queryByRole("list", { name: "Araçlar" })).not.toBeInTheDocument();
  });

  it("a failing catalog is an error with retry - never an 'empty catalog' - and retry recovers", async () => {
    backend.failures.list_catalog = { code: "DB_UNAVAILABLE", message: "Katalog okunamadı." };
    render(<ProjectsPage />);
    const r = await region();
    const alert = await r.findByRole("alert");
    expect(alert).toHaveTextContent("DB_UNAVAILABLE");
    expect(r.queryByText(/Katalogda hiç kayıt yok/)).not.toBeInTheDocument();
    expect(r.queryByRole("heading", { level: 3 })).not.toBeInTheDocument();
    delete backend.failures.list_catalog;
    await userEvent.click(within(alert).getByRole("button", { name: "Yeniden dene" }));
    expect(await r.findByRole("heading", { level: 3, name: "Alfa Araç" })).toBeInTheDocument();
    expect(r.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("a failing project list is an error with retry, and retry recovers", async () => {
    backend.failures.list_projects = { code: "DB_UNAVAILABLE", message: "Projeler okunamadı." };
    render(<ProjectsPage />);
    const r = await region();
    const alert = await r.findByRole("alert");
    expect(alert).toHaveTextContent("DB_UNAVAILABLE");
    expect(r.queryByRole("heading", { level: 3 })).not.toBeInTheDocument();
    delete backend.failures.list_projects;
    await userEvent.click(within(alert).getByRole("button", { name: "Yeniden dene" }));
    expect(await r.findByRole("heading", { level: 3, name: "Alfa Araç" })).toBeInTheDocument();
  });

  it("a refused save shows the backend's code, keeps the old state, shows no success, and a retry works", async () => {
    await open();
    const y = await rowOf("Yeni Kayıt");
    backend.failures.set_project_preference = { code: "PREFERENCE_NOT_ACTIVATABLE", message: "Uyumluluk bilinmiyor." };
    await choose(y, "accepted");
    await userEvent.click(saveButton(y));
    expect(await y.findByRole("alert")).toHaveTextContent("PREFERENCE_NOT_ACTIVATABLE");
    expect(y.getByText("Henüz karar yok")).toBeInTheDocument();
    expect((await region()).queryByText(/isteği kaydedildi/)).not.toBeInTheDocument();
    expect(saveButton(y)).toBeEnabled(); // honest recovery: the user can try again
    delete backend.failures.set_project_preference;
    await userEvent.click(saveButton(y));
    expect(await (await region()).findByText(/Yeni Kayıt: “Bu projede kullanmak istiyorum” isteği kaydedildi/)).toBeInTheDocument();
    await waitFor(() => expect(y.queryByRole("alert")).not.toBeInTheDocument());
  });

  it("a refused reset is shown and the wish stays", async () => {
    await open();
    const a = await rowOf("Alfa Araç");
    backend.failures.reset_project_preference = { code: "PREFERENCE_NOT_FOUND", message: "Kayıtlı karar yok." };
    await userEvent.click(a.getByRole("button", { name: /^Kararı sıfırla/ }));
    expect(await a.findByRole("alert")).toHaveTextContent("PREFERENCE_NOT_FOUND");
    expect(a.getByText("Bu projede kullanmak istiyorum", { selector: "strong" })).toBeInTheDocument();
    expect((await region()).queryByText(/sıfırlandı/)).not.toBeInTheDocument();
  });

  it("a double click on save sends one request, and the controls are locked while it runs", async () => {
    await open();
    const y = await rowOf("Yeni Kayıt");
    await choose(y, "rejected");
    const gate = hold("set_project_preference");
    const button = saveButton(y);
    fireEvent.click(button);
    fireEvent.click(button);
    await waitFor(() => expect(gate.started).toHaveLength(1));
    expect(y.getByRole("button", { name: /^Kaydediliyor/ })).toBeDisabled();
    expect(y.getByLabelText(/^Karar/)).toBeDisabled();
    await act(async () => gate.release());
    await waitFor(() => expect(calls("set_project_preference")).toHaveLength(1));
    expect(gate.started).toHaveLength(1);
  });

  it("changing the target while a save is in flight: the late answer shows no success under the new target", async () => {
    await open();
    const r = await region();
    const y = await rowOf("Yeni Kayıt");
    await choose(y, "rejected");
    const gate = hold("set_project_preference");
    await userEvent.click(saveButton(y));
    await waitFor(() => expect(gate.started).toHaveLength(1));
    // the user moves on to another project, another OS and another surface before the answer arrives
    await userEvent.selectOptions(r.getByLabelText("Proje"), "benim-sitem");
    await userEvent.selectOptions(r.getByLabelText("İşletim sistemi"), MAC);
    await userEvent.selectOptions(r.getByLabelText("Ajan yüzeyi"), CLI);
    await act(async () => gate.release());
    await waitFor(() => expect(calls("set_project_preference")).toHaveLength(1));
    expect(calls("set_project_preference")[0].args).toMatchObject({ alias: "ornek-web-sitesi", entryId: "yeni", agentSurface: CC, platformTarget: WIN });
    await new Promise((res) => setTimeout(res, 50));
    expect(r.queryByText(/isteği kaydedildi/)).not.toBeInTheDocument();
    const other = await rowOf("Yeni Kayıt");
    expect(other.getByText("Henüz karar yok")).toBeInTheDocument();
    expect(r.getByText(/Hedef:/)).toHaveTextContent("Benim sitem · macOS Apple Silicon · Yerel komut satırı");
  });

  it("the refresh button re-reads the catalog and the projects (a removed entry leaves the list)", async () => {
    await open();
    expect(screen.getByRole("heading", { level: 3, name: "Yeni Kayıt" })).toBeInTheDocument();
    backend.entries.find((e) => e.entryId === "yeni")!.removedAt = "2026-10-18T00:00:00Z";
    const before = calls("list_catalog").length;
    await userEvent.click((await region()).getByRole("button", { name: "Listeyi yenile" }));
    await waitFor(() => expect(screen.queryByRole("heading", { level: 3, name: "Yeni Kayıt" })).not.toBeInTheDocument());
    expect(calls("list_catalog").length).toBe(before + 1);
    // a restored entry comes back, but off in the catalog: still not 'wanted'-able
    const back = backend.entries.find((e) => e.entryId === "yeni")!;
    back.removedAt = null; back.enabled = false;
    const again = (await region()).getByRole("button", { name: "Listeyi yenile" });
    await waitFor(() => expect(again).toBeEnabled());
    await userEvent.click(again);
    const y = await rowOf("Yeni Kayıt");
    expect(y.getByText("Katalogda kapalı")).toBeInTheDocument();
    expect(y.getByRole("option", { name: /Bu projede kullanmak istiyorum/ })).toBeDisabled();
  });
});

describe("search and filters", () => {
  it("narrow the list, combine, report the count, and can be cleared", async () => {
    backend.projects[0].preferences = [wish("alfa", "accepted"), wish("gama", "rejected")];
    await open();
    const r = await region();
    const names = () => r.getAllByRole("heading", { level: 3 }).map((h) => h.textContent);
    expect(names()).toEqual(["Alfa Araç", "Beta Eklenti", "Delta Kısıtlı", "Gama Kapalı", "Yeni Kayıt"]);
    await userEvent.type(r.getByLabelText("Araç ara"), "ALFA");
    expect(names()).toEqual(["Alfa Araç"]);
    expect(r.getByText("1", { selector: "strong" })).toBeInTheDocument();
    await userEvent.clear(r.getByLabelText("Araç ara"));
    await userEvent.selectOptions(r.getByLabelText("Karar durumu"), "undecided");
    expect(names()).toEqual(["Beta Eklenti", "Delta Kısıtlı", "Yeni Kayıt"]);
    await userEvent.selectOptions(r.getByLabelText("Karar durumu"), "rejected");
    expect(names()).toEqual(["Gama Kapalı"]);
    await userEvent.selectOptions(r.getByLabelText("Karar durumu"), "all");
    await userEvent.selectOptions(r.getByLabelText("Tür"), "agent_plugin");
    expect(names()).toEqual(["Beta Eklenti"]);
    await userEvent.selectOptions(r.getByLabelText("Uyumluluk"), "declared");
    expect(r.getByText(/Bu süzgeçle eşleşen araç yok/)).toBeInTheDocument(); // Beta has no record for this target
    await userEvent.click(r.getByRole("button", { name: "Süzgeçleri temizle" }));
    expect(names()).toEqual(["Alfa Araç", "Beta Eklenti", "Delta Kısıtlı", "Gama Kapalı", "Yeni Kayıt"]);
  });

  it("the compatibility filter never counts an unknown or unsupported tool as supported", async () => {
    await open();
    const r = await region();
    await userEvent.selectOptions(r.getByLabelText("Uyumluluk"), "declared");
    expect(r.getAllByRole("heading", { level: 3 }).map((h) => h.textContent)).toEqual(["Alfa Araç", "Delta Kısıtlı", "Gama Kapalı", "Yeni Kayıt"]);
    await userEvent.selectOptions(r.getByLabelText("Uyumluluk"), "unknown");
    expect(r.getAllByRole("heading", { level: 3 }).map((h) => h.textContent)).toEqual(["Beta Eklenti"]);
  });

  it("filters survive a refresh and a save (they are the user's view, not data)", async () => {
    await open();
    const r = await region();
    await userEvent.selectOptions(r.getByLabelText("Karar durumu"), "undecided");
    const y = await rowOf("Yeni Kayıt");
    await choose(y, "rejected");
    await userEvent.click(saveButton(y));
    // the saved row has a decision now, so the 'undecided' view no longer lists it - and the confirmation is still there
    await waitFor(() => expect(r.queryByRole("heading", { level: 3, name: "Yeni Kayıt" })).not.toBeInTheDocument());
    expect(r.getByText(/Yeni Kayıt: “Reddet \(tekrar sorma\)” isteği kaydedildi/)).toBeInTheDocument();
    expect(r.getByLabelText("Karar durumu")).toHaveValue("undecided");
  });
});

describe("accessibility", () => {
  it("has a named region, labelled controls, a named list, one labelled article per tool and live status areas", async () => {
    await open();
    const r = await region();
    for (const label of ["Proje", "İşletim sistemi", "Ajan yüzeyi", "Araç ara", "Tür", "Karar durumu", "Uyumluluk"]) expect(r.getByLabelText(label)).toBeInTheDocument();
    expect(r.getByRole("search", { name: "Araçları ara ve süz" })).toBeInTheDocument();
    const list = r.getByRole("list", { name: "Araçlar" });
    expect(within(list).getAllByRole("article")).toHaveLength(5);
    for (const name of ["Alfa Araç", "Beta Eklenti", "Gama Kapalı"]) expect(within(list).getByRole("article", { name })).toBeInTheDocument();
    expect(r.getAllByRole("status").length).toBeGreaterThanOrEqual(2); // summary + confirmation line
    // every control inside a row is named, including which tool it belongs to
    const a = await rowOf("Alfa Araç");
    expect(a.getByRole("button", { name: /^Yalnızca tercihimi kaydet \(Alfa Araç; kurulum yapmaz\)/ })).toBeInTheDocument();
    expect(a.getByRole("button", { name: /^Kararı sıfırla \(Alfa Araç\)/ })).toBeInTheDocument();
    expect(a.getByLabelText("Karar (Alfa Araç)")).toBeInTheDocument();
  });

  it("is operable by keyboard: native controls, focusable in order, Enter on the focused button saves", async () => {
    await open();
    const y = await rowOf("Yeni Kayıt");
    await choose(y, "rejected");
    const button = saveButton(y);
    button.focus();
    expect(button).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(calls("set_project_preference")).toHaveLength(1));
    const select = y.getByLabelText(/^Karar/) as HTMLSelectElement;
    select.focus();
    expect(select).toHaveFocus();
    expect(select.tagName).toBe("SELECT");
  });

  it("never relies on colour alone: every state is words, every warning carries a symbol, unavailable choices say why", async () => {
    backend.projects[0].preferences = [wish("alfa", "rejected", { materialChange: true })];
    await open();
    const a = await rowOf("Alfa Araç");
    expect(a.getByText(/⚠ Aracın yetenekleri/)).toBeInTheDocument();
    const b = await rowOf("Beta Eklenti");
    expect(b.getByText(/⚠ Bu işletim sistemi ve ajan yüzeyi için uyumluluk bilinmiyor/)).toBeInTheDocument();
    expect(b.getByRole("option", { name: /Bu projede kullanmak istiyorum \(şu an seçilemez\)/ })).toBeDisabled();
  });

  it("a disabled save button is really disabled until a different, allowed decision is chosen", async () => {
    await open();
    const y = await rowOf("Yeni Kayıt");
    expect(saveButton(y)).toBeDisabled();
    await choose(y, "rejected");
    expect(saveButton(y)).toBeEnabled();
    const a = await rowOf("Alfa Araç"); // already 'accepted': re-saving the same decision is pointless
    expect(saveButton(a)).toBeDisabled();
  });
});

describe("safety: the workspace only records or forgets a wish", () => {
  const FORBIDDEN = [
    "prepare_install", "confirm_install", "cancel_install", "get_installed_details", "recover_installations", "list_install_states", "run_discovery_scan", "list_inventory",
    "plan_install", "preview_github_url", "add_catalog_entry", "remove_catalog_entry", "restore_catalog_entry", "set_catalog_entry_enabled", "explain_execution_block",
    "evaluate_task", "add_project_reference", "remove_project_reference", "set_setting", "set_startup_enabled", "preview_backup_cleanup",
  ];

  it("a full session issues only the four existing read / wish commands, and never touches the network, a new window or another command", async () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal("fetch", fetchSpy);
    const openSpy = vi.spyOn(window, "open").mockImplementation(() => null);
    await open();
    const r = await region();
    const y = await rowOf("Yeni Kayıt");
    await choose(y, "postponed");
    await userEvent.click(saveButton(y));
    await userEvent.click((await rowOf("Alfa Araç")).getByRole("button", { name: /^Kararı sıfırla/ }));
    await userEvent.selectOptions(r.getByLabelText("Proje"), "benim-sitem");
    await userEvent.selectOptions(r.getByLabelText("İşletim sistemi"), MAC);
    await userEvent.click(r.getByRole("button", { name: "Listeyi yenile" }));
    await waitFor(() => expect(calls("list_catalog").length).toBeGreaterThanOrEqual(2));
    const used = new Set(backend.calls.map((c) => c.cmd));
    for (const cmd of used) expect(["list_projects", "list_catalog", "set_project_preference", "reset_project_preference"], cmd).toContain(cmd);
    for (const cmd of FORBIDDEN) expect(used.has(cmd), cmd).toBe(false);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(openSpy).not.toHaveBeenCalled();
    expect(document.querySelector("a[href]")).toBeNull(); // not even a link a click could follow
  });

  it("the source of the new view names no network, file, shell, window or plugin API and calls only setProjectPreference / resetProjectPreference", () => {
    const text = `${workspaceComponentSource}\n${workspaceLogicSource}`;
    for (const banned of ["fetch(", "XMLHttpRequest", "WebSocket", "window.open", "navigator.sendBeacon", "EventSource", "localStorage", "sessionStorage", "indexedDB", "document.cookie", "@tauri-apps/plugin", "@tauri-apps/api/core", "invoke(", "child_process", "node:fs", "installMock", "dangerouslySetInnerHTML", "href="]) {
      expect(text.includes(banned), banned).toBe(false);
    }
    const apiCalls = [...text.matchAll(/\bapi\.([A-Za-z]+)/g)].map((m) => m[1]);
    expect([...new Set(apiCalls)].sort()).toEqual(["resetProjectPreference", "setProjectPreference"]);
  });

  it("the Projects page and the handoff code name no network, file, shell, window or plugin API and use only the project / catalog reads and the project-reference commands", async () => {
    const projectsSource = (await import("./pages/Projects.tsx?raw")).default;
    const appSource = (await import("./App.tsx?raw")).default;
    for (const [label, text] of [["Projects.tsx", projectsSource], ["App.tsx", appSource]] as const) {
      for (const banned of ["fetch(", "XMLHttpRequest", "WebSocket", "window.open", "navigator.sendBeacon", "EventSource", "localStorage", "sessionStorage", "indexedDB", "document.cookie", "@tauri-apps/plugin", "@tauri-apps/api/core", "invoke(", "dangerouslySetInnerHTML", "href="]) {
        // App.tsx keeps its existing skip link (href="#main"); nothing else may carry a link
        if (label === "App.tsx" && banned === "href=") continue;
        expect(text.includes(banned), `${label}: ${banned}`).toBe(false);
      }
    }
    const used = (text: string) => [...new Set([...text.matchAll(/\bapi\.([A-Za-z]+)/g)].map((m) => m[1]))].sort();
    expect(used(projectsSource)).toEqual(["addProjectReference", "listCatalog", "listProjects", "removeProjectReference"]);
    expect(used(appSource)).toEqual(["getAppInfo", "getSettings"]);
  });

  it("hostile text from a catalog entry is rendered as plain text", async () => {
    backend.entries.push(makeEntry("evil", { displayName: "evil/evil", purpose: "<img src=x onerror=alert(1)> <b>kalın</b>", record: {} }));
    await open();
    expect(await (await region()).findByText(/<img src=x onerror=alert\(1\)>/)).toBeInTheDocument();
    expect(document.querySelector("img")).toBeNull();
    expect(document.querySelector("article b")).toBeNull();
  });
});

describe("the Projects page has exactly one place that stores or forgets a wish", () => {
  it("keeps the project cards, the add form and the sample banner, and no legacy decision table or form", async () => {
    render(<ProjectsPage />);
    expect(await screen.findByText("ÖRNEK VERİ")).toBeInTheDocument();
    const card = (await screen.findByRole("heading", { name: /Örnek: Web sitesi/ })).closest("section")!;
    expect(within(card).getByText("örnek")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Proje kaydını ekle" })).toBeInTheDocument();
    for (const gone of [/Karar ekle/, /Kararı kaydet/, /için kayıtlı kararlar/]) expect(screen.queryByText(gone)).not.toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    expect(within(card).queryByLabelText("Kayıt")).not.toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: /Kararı (sıfırla|kaydet)/ })).not.toBeInTheDocument();
  });

  it("every control that writes a wish sits inside the workspace region; the project cards only show a count and open the matrix", async () => {
    await open();
    const r = await region();
    const all = screen.getAllByRole("button", { name: /Yalnızca tercihimi kaydet|Kararı sıfırla|Eski kararı sıfırla/ });
    expect(all.length).toBeGreaterThan(0);
    for (const b of all) expect(r.getAllByRole("button").includes(b)).toBe(true);
    const card = screen.getByRole("heading", { name: /Örnek: Web sitesi/ }).closest("section")!;
    expect(within(card).getByText(/Kayıtlı karar:/)).toHaveTextContent("Kayıtlı karar: 1");
    expect(within(card).getAllByRole("button").map((b) => b.textContent)).toEqual(["Araç matrisinde aç (Örnek: Web sitesi)"]);
  });

  it("the Projects page source never calls the preference write / reset commands itself", async () => {
    const mod = await import("./pages/Projects.tsx?raw");
    expect(mod.default).not.toMatch(/setProjectPreference|resetProjectPreference/);
    expect(mod.default).not.toMatch(/PreferenceForm|PreferenceRow/);
  });
});

describe("every recorded decision stays reachable: the read-only target navigator", () => {
  const manyTargets = () => [
    wish("alfa", "accepted"),
    wish("eski", "rejected", { entryRemoved: true }), // a decision on an entry that has since been removed from the catalog (the backend sets the flag from the same row)
    wish("alfa", "rejected", { agentSurface: CLI }),
    wish("alfa", "postponed", { platformTarget: MAC }),
  ];

  it("lists each target that holds decisions with its count (removed entries included) and marks the open one", async () => {
    backend.projects[0].preferences = manyTargets();
    await open();
    const r = await region();
    const nav = within(r.getByRole("group", { name: /Bu projedeki kayıtlı kararlar/ }));
    expect(nav.getAllByRole("button").map((b) => b.textContent)).toEqual([
      "Windows x64 · Claude Code (terminal): 2 karar (1 katalogdan kaldırılmış kayıt) — şu an açık",
      "Windows x64 · Yerel komut satırı: 1 karar",
      "macOS Apple Silicon · Claude Code (terminal): 1 karar",
    ]);
    expect(nav.getAllByRole("button")[0]).toHaveAttribute("aria-current", "true");
    expect(nav.getAllByRole("button")[1]).not.toHaveAttribute("aria-current");
  });

  it("selecting a target opens exactly it - decisions, removed entries included - and makes no backend call", async () => {
    backend.projects[0].preferences = manyTargets();
    await open();
    const r = await region();
    expect((await rowOf("Eski Kayıt")).getAllByText("katalogdan kaldırıldı").length).toBeGreaterThan(0); // reachable at the default target
    const before = backend.calls.length;
    await userEvent.click(r.getByRole("button", { name: /Windows x64 · Yerel komut satırı: 1 karar/ }));
    expect(r.getByLabelText("Ajan yüzeyi")).toHaveValue(CLI);
    expect(r.getByLabelText("İşletim sistemi")).toHaveValue(WIN);
    expect((await rowOf("Alfa Araç")).getByText("Reddet (tekrar sorma)", { selector: "strong" })).toBeInTheDocument();
    await userEvent.click(r.getByRole("button", { name: /macOS Apple Silicon · Claude Code \(terminal\): 1 karar/ }));
    expect((await rowOf("Alfa Araç")).getByText("Şimdi değil (ertele)", { selector: "strong" })).toBeInTheDocument();
    expect(backend.calls.length).toBe(before); // a navigator click reads nothing new and writes nothing
  });

  it("says plainly when the project has no recorded decision", async () => {
    await open();
    const r = await region();
    await userEvent.selectOptions(r.getByLabelText("Proje"), "benim-sitem");
    expect(await r.findByText("Bu projede henüz kayıtlı karar yok.")).toBeInTheDocument();
    expect(r.queryByRole("group", { name: /Bu projedeki kayıtlı kararlar/ })?.querySelectorAll("button")).toHaveLength(0);
  });

  it("a decision on a target this build does not offer is counted and reported, never deleted or hidden silently", async () => {
    backend.projects[0].preferences = [wish("alfa", "accepted"), wish("alfa", "rejected", { platformTarget: "linux-x64" })];
    await open();
    const r = await region();
    expect(r.getByText(/1 karar, bu sürümün tanımadığı bir işletim sistemi veya ajan yüzeyine ait/)).toBeInTheDocument();
    expect(calls("reset_project_preference")).toHaveLength(0);
    expect(backend.projects[0].preferences).toHaveLength(2);
  });

  it("the project card's shortcut selects that project in the matrix and keeps the chosen operating system and surface", async () => {
    await open();
    const r = await region();
    await userEvent.selectOptions(r.getByLabelText("İşletim sistemi"), MAC);
    await userEvent.selectOptions(r.getByLabelText("Ajan yüzeyi"), CLI);
    const card = screen.getByRole("heading", { name: /Benim sitem/ }).closest("section")!;
    await userEvent.click(within(card).getByRole("button", { name: /^Araç matrisinde aç/ }));
    expect(r.getByLabelText("Proje")).toHaveValue("benim-sitem");
    expect(r.getByLabelText("İşletim sistemi")).toHaveValue(MAC);
    expect(r.getByLabelText("Ajan yüzeyi")).toHaveValue(CLI);
    await waitFor(() => expect(r.getByRole("heading", { level: 2, name: "Proje Araç Matrisi" })).toHaveFocus());
  });

  it("add / remove project flows are untouched, and removing the selected project falls back to another one with a note-free list", async () => {
    await open();
    const card = () => screen.getByRole("heading", { name: /Benim sitem/ }).closest("section")!;
    await userEvent.selectOptions((await region()).getByLabelText("Proje"), "benim-sitem");
    await userEvent.click(within(card()).getByRole("button", { name: "Proje kaydını kaldır" }));
    await userEvent.click(within(card()).getByRole("button", { name: "Evet, kaldır" }));
    expect(calls("remove_project_reference")).toEqual([{ cmd: "remove_project_reference", args: { alias: "benim-sitem" } }]);
    await waitFor(() => expect(screen.queryByRole("heading", { name: /Benim sitem/ })).not.toBeInTheDocument());
    expect((await region()).getByLabelText("Proje")).toHaveValue("ornek-web-sitesi");
  });
});

describe("the application's record: an unfamiliar status is never shown or counted as success", () => {
  it("a wish row or a catalog row with a corrupt, future or positive-looking status string reads 'unrecognised - not a success'", async () => {
    backend.entries.find((e) => e.entryId === "beta")!.installState = "installed";
    backend.entries.find((e) => e.entryId === "yeni")!.verificationState = "passed";
    backend.projects[0].preferences = [wish("alfa", "accepted", { actualInstallState: "installed_verified", actualVerificationState: "verified" })];
    await open();
    const a = await rowOf("Alfa Araç");
    expect(a.getAllByText(/Tanınmayan durum .* başarı sayılmaz/).length).toBe(2);
    expect(a.getByText(/Kurulum: Tanınmayan durum \(“installed_verified”\)/)).toBeInTheDocument();
    expect((await rowOf("Beta Eklenti")).getByText(/Kurulum: Tanınmayan durum \(“installed”\)/)).toBeInTheDocument();
    expect((await rowOf("Yeni Kayıt")).getByText(/Doğrulama: Tanınmayan durum \(“passed”\)/)).toBeInTheDocument();
    const r = await region();
    expect(r.getByText(/3 tanınmayan durum \(başarı sayılmaz\)/)).toBeInTheDocument();
    expect(r.queryByText(/kurulu olarak kaydedilmiş|doğrulanmış olarak kaydedilmiş/)).not.toBeInTheDocument();
    for (const row of [a, await rowOf("Beta Eklenti"), await rowOf("Yeni Kayıt")]) expect(row.queryByText(/Kurulu ve doğrulandı|Doğrulandı|Kuruldu|Etkinleştirildi/)).not.toBeInTheDocument();
  });
});

describe("hostile project text is inert", () => {
  it("a project name with markup is rendered as text in the selector, the target line and the card", async () => {
    backend.projects[1].displayName = "<img src=x onerror=alert(1)><b>kalın</b>";
    backend.projects[1].localNote = "<script>alert(1)</script>";
    await open();
    await userEvent.selectOptions((await region()).getByLabelText("Proje"), "benim-sitem");
    expect((await region()).getByText(/Hedef:/)).toHaveTextContent("<img src=x onerror=alert(1)><b>kalın</b>");
    expect(document.querySelector("img")).toBeNull();
    expect(document.querySelector("script")).toBeNull();
    expect(document.querySelector("section b, article b")).toBeNull();
  });
});

describe("out-of-order reads never put an old saved preference back on the screen", () => {
  it("a slow, stale project list that arrives after a newer one is ignored", async () => {
    await open();
    const original = backend.handle;
    const gates: (() => void)[] = [];
    let holdNext = false;
    backend.handle = async (c, a) => {
      const snapshot = await original(c, a); // the answer is taken NOW, as a real read would be
      if (c === "list_projects" && holdNext) {
        holdNext = false;
        await new Promise<void>((res) => gates.push(res)); // ... but delivered late
      }
      return snapshot;
    };
    const alfa1 = await rowOf("Alfa Araç"); // currently 'accepted'
    holdNext = true;
    await userEvent.click(alfa1.getByRole("button", { name: /^Kararı sıfırla/ })); // read #1 will say: no decision (and is held)
    await waitFor(() => expect(calls("reset_project_preference")).toHaveLength(1));
    await waitFor(() => expect(gates).toHaveLength(1));
    await userEvent.selectOptions(alfa1.getByLabelText(/^Karar/), "rejected");
    await userEvent.click(saveButton(alfa1)); // read #2 says: rejected, and arrives first
    await waitFor(() => expect(alfa1.getByText("Reddet (tekrar sorma)", { selector: "strong" })).toBeInTheDocument());
    await act(async () => gates[0]()); // the stale read #1 now arrives
    await new Promise((res) => setTimeout(res, 30));
    expect(alfa1.getByText("Reddet (tekrar sorma)", { selector: "strong" })).toBeInTheDocument();
    expect(alfa1.queryByText("Henüz karar yok")).not.toBeInTheDocument();
    expect((await region()).getByText(/0 kullanmak istiyorum · 0 ertelendi · 1 reddedildi/)).toBeInTheDocument();
  });
});
