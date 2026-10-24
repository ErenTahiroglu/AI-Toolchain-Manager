// PRODUCT-05 - "İncelenmesi Gerekenler": the read-only review queue on the Projects page and its Dashboard summary, through the real App and the real
// sidebar against the in-memory backend. Mocked IPC and jsdom only: no real window, focus ring, screen reader or database. Only `Date` is faked (a fixed
// "now" for the reminder dates); no timer is faked and the queue owns none.
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import reviewQueueSource from "./components/ReviewQueue.tsx?raw";
import reviewSummarySource from "./components/ReviewSummary.tsx?raw";
import { platformLabel, surfaceLabel } from "./labels";
import reviewLogicSource from "./review.ts?raw";
import { createBackend, makeEntry, makeRequest, obs, pref, type Backend } from "./test/backend";
import type { Inventory, PreferenceView, ProjectView, RequestStatus } from "./types";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const WIN = "windows-x64";
const MAC = "macos-arm64";
const CC = "claude-code-terminal";
const DESK = "claude-desktop-code";
const CLI = "local-cli";
const ORNEK = "ornek-web-sitesi";
const BENIM = "benim-sitem";
const ev = (platformTarget: string, agentSurface: string) => ({ platformTarget, agentSurface, compatibilityState: "vendor-declared", limitation: "kayıt: vendor-declared" });

const wish = (entryId: string, over: Partial<PreferenceView> = {}): PreferenceView =>
  pref({ entryId, entryName: entryId, agentSurface: CC, platformTarget: WIN, decision: "accepted", ...over });
const project = (alias: string, name: string, preferences: PreferenceView[], kind: ProjectView["kind"] = "reference"): ProjectView => ({
  projectId: `id-${alias}`, alias, displayName: name, kind, localNote: "GİZLİ-PROJE-NOTU", createdAt: "2026-10-17T00:00:00Z", preferences,
});
const request = (entryId: string, status: RequestStatus, over: Partial<Parameters<typeof makeRequest>[0]> = {}) =>
  makeRequest({ entryId, entryName: entryId, status, ...over });

const calls = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd);
const go = async (name: string) => {
  await userEvent.click(within(screen.getByRole("navigation")).getByRole("button", { name }));
  return screen.findByRole("heading", { level: 1, name });
};
const queue = async () => within(await screen.findByRole("region", { name: "İncelenmesi Gerekenler" }));
const tile = async () => within(await screen.findByRole("region", { name: "İncelenmesi gerekenler" }));
const matrix = async () => within(await screen.findByRole("region", { name: "Proje Araç Matrisi" }));
const where = (projectName: string, platform: string, surface: string) => `${projectName} · ${platformLabel(platform)} · ${surfaceLabel(surface)}`;
const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
// the accessible name carries the tool NAME, the tool ID (same-named tools differ by it) and the exact target
const openButton = async (entryName: string, projectName: string, platform = WIN, surface = CC, entryId?: string) =>
  (await queue()).getByRole("button", { name: new RegExp(`^Matriste incele \\(${escape(entryName)} · ${entryId ? escape(entryId) : "[^;]+"}; ${escape(where(projectName, platform, surface))}\\)$`) });
const itemOf = (button: HTMLElement) => within(button.closest("li.rq-item") as HTMLElement);
const spot = () => document.querySelector(".ws-spot article") as HTMLElement | null;
const FORBIDDEN = ["run_discovery_scan", "evaluate_task", "set_project_preference", "reset_project_preference", "prepare_install", "confirm_install", "cancel_install", "recover_installations", "plan_install", "get_installed_details", "list_install_states", "preview_backup_cleanup", "set_setting", "set_startup_enabled", "set_catalog_entry_enabled", "add_catalog_entry", "remove_catalog_entry", "preview_github_url", "add_project_reference", "remove_project_reference"];

/** Holds every call of `cmd` until the test answers it. */
function defer(cmd: string) {
  const original = backend.handle;
  const pending: { resolve: (v: unknown) => void; reject: (e: unknown) => void }[] = [];
  backend.handle = (c, a) => {
    if (c !== cmd) return original(c, a);
    backend.calls.push({ cmd: c, args: a });
    return new Promise((resolve, reject) => pending.push({ resolve: (v) => resolve(JSON.parse(JSON.stringify(v))), reject }));
  };
  return pending;
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-10-24T12:00:00Z"));
  backend = createBackend();
  backend.entries = [
    makeEntry("alfa", { displayName: "Alfa Araç", record: { targetEvidence: [ev(WIN, CC), ev(MAC, CC), ev(WIN, DESK)] } }),
    makeEntry("beta", { displayName: "Beta Eklenti", kind: "agent_plugin", record: { targetEvidence: [ev(WIN, CC)] } }),
    makeEntry("ikiz-bir", { displayName: "İkiz Araç", record: { targetEvidence: [ev(WIN, CC)] } }),
    makeEntry("ikiz-iki", { displayName: "İkiz Araç", record: { targetEvidence: [ev(WIN, CC)] } }),
    makeEntry("eski", { displayName: "Eski Kayıt", removedAt: "2026-10-18T00:00:00Z", enabled: false, record: { targetEvidence: [ev(WIN, CC)] } }),
  ];
  backend.projects = [
    project(ORNEK, "Örnek: Web sitesi", [
      wish("alfa", { entryName: "Alfa Araç", materialChange: true }), // + not_scanned below: two reasons, one item
      wish("alfa", { entryName: "Alfa Araç", platformTarget: MAC }), // previously_present
      wish("alfa", { entryName: "Alfa Araç", agentSurface: DESK }), // accepted, no backend row: uncertain
      wish("beta", { entryName: "Beta Eklenti", decision: "postponed", remindAt: "2026-10-24T12:00:00Z" }), // due exactly now
      wish("ikiz-bir", { entryName: "İkiz Araç" }), // present_current: nothing to review
      wish("ikiz-iki", { entryName: "İkiz Araç", decision: "rejected", materialChange: true }),
      wish("eski", { entryName: "Eski Kayıt", entryRemoved: true }),
    ], "synthetic"),
    project(BENIM, "Benim sitem", [
      wish("alfa", { entryName: "Alfa Araç" }), // absent_current
      wish("beta", { entryName: "Beta Eklenti", decision: "postponed", remindAt: "2026-10-25T12:00:00Z" }), // not yet due
    ]),
  ];
  backend.inventory = {
    ...backend.inventory,
    requests: [
      request("alfa", "not_scanned"),
      request("alfa", "previously_present", { platformTarget: MAC }),
      request("ikiz-bir", "present_current"),
      request("alfa", "absent_current", { projectAlias: BENIM, projectName: "Benim sitem" }),
    ],
  };
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("what the queue lists", () => {
  it("shows exactly the items with a reason, one per exact key, grouped by project, with tool, project, target, wish, reasons and provenance", async () => {
    render(<App />);
    await go("Projeler");
    const q = await queue();
    expect(await q.findByText("6 öğe incelenmeyi bekliyor.")).toBeInTheDocument();
    expect(q.getAllByRole("heading", { level: 3 }).map((h) => h.textContent?.replace(/\s+/g, " "))).toEqual(["Benim sitem (1)", "Örnek: Web sitesi örnek veri (5)", "Kanıtı belirsiz kalan kararlar (1)"]);

    // two reasons on one key -> ONE item
    const alfa = itemOf(await openButton("Alfa Araç", "Örnek: Web sitesi"));
    expect(alfa.getByText(/Karardan sonra aracın yetenekleri veya uyumluluğu katalogda önemli ölçüde değişti/)).toBeInTheDocument();
    expect(alfa.getByText(/Bu hedef için kayıtlı envanterde henüz gözlem yok; bu, aracın olmadığı anlamına gelmez/)).toBeInTheDocument();
    expect(alfa.getByText("Bu projede kullanmak istiyorum", { selector: "strong" })).toBeInTheDocument();
    expect(alfa.getByText(/Kayıtlı gözlem: Henüz taranmadı/)).toBeInTheDocument();
    expect(alfa.getByText("alfa")).toBeInTheDocument(); // the tool id tells same-named tools apart
    expect(q.getAllByRole("button", { name: /^Matriste incele \(Alfa Araç · alfa;/ })).toHaveLength(4); // Örnek WIN/CC, Örnek MAC/CC, Benim WIN/CC, plus the uncertain-only Örnek DESK listed below

    // stored classification of the other keys
    expect(itemOf(await openButton("Alfa Araç", "Örnek: Web sitesi", MAC)).getByText(/Kayıtlı gözlem: Eski taramada bulunmuştu \(güncel değil\)/)).toBeInTheDocument();
    expect(itemOf(await openButton("Alfa Araç", "Benim sitem")).getByText(/Kayıtlı gözlem: Son taramada bulunamadı/)).toBeInTheDocument();
    // the due reminder (now == remindAt) is listed, the future one is not
    expect(itemOf(await openButton("Beta Eklenti", "Örnek: Web sitesi")).getByText(/Hatırlatma tarihi geldi/)).toBeInTheDocument();
    expect(q.queryByRole("button", { name: /Beta Eklenti; Benim sitem/ })).not.toBeInTheDocument();
    // present_current alone is not on the list; a rejected decision with a material change is
    expect(q.getAllByRole("button", { name: /^Matriste incele \(İkiz Araç ·/ })).toHaveLength(1);
    expect(itemOf(q.getByRole("button", { name: /^Matriste incele \(İkiz Araç ·/ })).getByText("Reddet (tekrar sorma)", { selector: "strong" })).toBeInTheDocument();
    // removed tool with a wish
    expect(itemOf(await openButton("Eski Kayıt", "Örnek: Web sitesi")).getByText(/Araç katalogdan kaldırıldı; eski karar duruyor/)).toBeInTheDocument();
  });

  it("an accepted wish whose evidence cannot be matched is listed as UNCERTAIN, in its own section, with its own wording", async () => {
    render(<App />);
    await go("Projeler");
    const q = await queue();
    const heading = await q.findByRole("heading", { level: 3, name: /Kanıtı belirsiz kalan kararlar/ });
    const section = within(heading.parentElement as HTMLElement);
    expect(section.getByText(/Kanıt belirsiz: Kayıtlı envanterde bu kabul edilmiş istek için eşleşen satır yok; gözlem bilinmiyor/)).toBeInTheDocument();
    expect(section.getByText(/“sorun var” ya da “sorun yok” denmiyor/)).toBeInTheDocument();
    expect(section.queryByText(/Henüz taranmadı|bulunamadı\b(?!;)/)).not.toBeInTheDocument();
    // not counted as 'needs review'
    expect(q.getByText("6 öğe incelenmeyi bekliyor.")).toBeInTheDocument();
  });

  it("distinct tools with the same display name are two separate entries with distinguishable controls", async () => {
    backend.projects[0].preferences.push(wish("ikiz-bir", { entryName: "İkiz Araç", agentSurface: DESK, materialChange: true }));
    backend.projects[0].preferences.find((p) => p.entryId === "ikiz-iki")!.agentSurface = DESK; // same display name, different tool and surface
    backend.entries.find((e) => e.entryId === "ikiz-bir")!.record = { targetEvidence: [ev(WIN, CC), ev(WIN, DESK)] };
    render(<App />);
    await go("Projeler");
    const q = await queue();
    const buttons = await q.findAllByRole("button", { name: /^Matriste incele \(İkiz Araç ·/ });
    expect(buttons).toHaveLength(2);
    expect(new Set(buttons.map((b) => b.getAttribute("aria-label") ?? b.textContent)).size).toBe(2);
    expect(itemOf(buttons[0]).getByText(/^ikiz-(bir|iki)$/)).toBeInTheDocument();
    expect(itemOf(buttons[1]).getByText(/^ikiz-(bir|iki)$/)).toBeInTheDocument();
    expect(itemOf(buttons[0]).getByText(/^ikiz-(bir|iki)$/).textContent).not.toBe(itemOf(buttons[1]).getByText(/^ikiz-(bir|iki)$/).textContent);
  });

  it("filters by project and by reason, says how many are shown, and does not change what exists", async () => {
    render(<App />);
    await go("Projeler");
    const q = await queue();
    await q.findByText("6 öğe incelenmeyi bekliyor.");
    await userEvent.selectOptions(q.getByLabelText("Kuyrukta proje süz"), BENIM);
    expect(q.getByText("1 / 6 öğe gösteriliyor.")).toBeInTheDocument();
    expect(q.getAllByRole("button", { name: /^Matriste incele/ })).toHaveLength(1); // the uncertain-only decision belongs to the other project
    expect(q.queryByRole("heading", { level: 3, name: /Kanıtı belirsiz/ })).not.toBeInTheDocument();
    await userEvent.selectOptions(q.getByLabelText("Kuyrukta proje süz"), "all");
    await userEvent.selectOptions(q.getByLabelText("Kuyrukta neden süz"), "entry_removed");
    expect(q.getByText("1 / 6 öğe gösteriliyor.")).toBeInTheDocument();
    expect(q.getByRole("button", { name: /Eski Kayıt/ })).toBeInTheDocument();
    await userEvent.selectOptions(q.getByLabelText("Kuyrukta neden süz"), "evidence");
    expect(q.getByText("3 / 6 öğe gösteriliyor.")).toBeInTheDocument(); // alfa: not_scanned, previously_present, absent_current
    expect(q.getByText(/Neden süzgeci açıkken kanıtı belirsiz kalan 1 karar listelenmez/)).toBeInTheDocument();
    expect(q.queryByRole("heading", { level: 3, name: /Kanıtı belirsiz/ })).not.toBeInTheDocument();
    expect(q.getByText("6 öğe incelenmeyi bekliyor.")).toBeInTheDocument(); // the headline is not a filter result
  });

  it("filtering and the matrix's own project select are separate controls", async () => {
    render(<App />);
    await go("Projeler");
    const m = await matrix();
    expect(m.getByLabelText("Proje")).toBeInTheDocument();
    expect((await queue()).getByLabelText("Kuyrukta proje süz")).not.toBe(m.getByLabelText("Proje"));
  });
});

describe("stored findings are not installations", () => {
  it("present_current next to a real reason is plain information - never 'resolved', 'installed', 'verified' or 'working'", async () => {
    backend.projects[0].preferences.find((p) => p.entryId === "ikiz-bir")!.materialChange = true;
    render(<App />);
    await go("Projeler");
    const item = itemOf(await openButton("İkiz Araç", "Örnek: Web sitesi", WIN, CC, "ikiz-bir"));
    await waitFor(() => expect(item.getByText(/Kayıtlı gözlem: Son taramada bulundu/)).toBeInTheDocument());
    const text = (await queue()).getAllByRole("button", { name: /^Matriste incele \(İkiz Araç/ }).map((b) => (b.closest("li.rq-item") as HTMLElement).textContent ?? "").join(" ");
    expect(text).not.toMatch(/çözüldü|tamamlandı|doğrulandı|doğrulanmış olarak|kuruldu|kurulu olarak|çalışıyor|etkinleştirildi|✓|başarı/i);
  });

  it("a queue with nothing to review says so only after both reads finished, and says what that does not mean", async () => {
    backend.projects = [project(ORNEK, "Örnek: Web sitesi", [wish("alfa", { entryName: "Alfa Araç" })], "synthetic")];
    backend.inventory = { ...backend.inventory, requests: [request("alfa", "present_current")] };
    render(<App />);
    await go("Projeler");
    const q = await queue();
    expect(await q.findByText(/Bu ölçütlere göre incelenmesi gereken öğe yok/)).toBeInTheDocument();
    expect(q.getByText(/herhangi bir aracın kurulu ve çalışır olduğu anlamına gelmez/)).toBeInTheDocument();
  });
});

describe("pending, failed and old reads never produce a reassuring empty queue", () => {
  it("first read of the decisions failed: an error with a retry, no number, no 'nothing to review'; the retry works", async () => {
    backend.failures.list_projects = { code: "DB_BUSY", message: "Veritabanı meşgul." };
    render(<App />);
    await go("Projeler");
    const q = await queue();
    expect(await q.findByText(/Kararlar okunamadı; incelenecek öğelerin sayısı bilinmiyor/)).toBeInTheDocument();
    expect(q.queryByText(/öğe incelenmeyi bekliyor|incelenmesi gereken öğe yok/)).not.toBeInTheDocument();
    delete backend.failures.list_projects;
    await userEvent.click(q.getByRole("button", { name: "Listeyi yeniden oku" }));
    expect(await q.findByText("6 öğe incelenmeyi bekliyor.")).toBeInTheDocument();
  });

  it("decisions pending: 'okunuyor', no number", async () => {
    const pending = defer("list_projects");
    render(<App />);
    await go("Projeler");
    expect(await (await queue()).findByText("Kararlar okunuyor…")).toBeInTheDocument();
    expect((await queue()).queryByText(/incelenmeyi bekliyor|öğe yok/)).not.toBeInTheDocument();
    await act(async () => pending.forEach((p) => p.resolve(backend.projects)));
  });

  it("inventory pending: decision-based reasons are listed as 'at least', evidence-based ones are not yet included, and there is no clean message", async () => {
    const pending = defer("list_inventory");
    render(<App />);
    await go("Projeler");
    const q = await queue();
    expect(await q.findByText(/Eldeki geçici görüntüde 4 öğe var; liste eksik veya eski olabilir, sayı artabilir veya azalabilir \(sonuç kesin değil\)/)).toBeInTheDocument(); // material, rejected+material, reminder, removed
    expect(q.getByText(/Kayıtlı envanter okunuyor; gözlemden doğan nedenler henüz dahil değil/)).toBeInTheDocument();
    expect(q.queryByText(/Kayıtlı gözlem: /)).not.toBeInTheDocument();
    await act(async () => pending.forEach((p) => p.resolve(backend.inventory)));
    expect(await q.findByText("6 öğe incelenmeyi bekliyor.")).toBeInTheDocument();
  });

  it("inventory pending with nothing decision-based to list is NOT 'nothing to review'", async () => {
    backend.projects = [project(ORNEK, "Örnek: Web sitesi", [wish("alfa", { entryName: "Alfa Araç" })], "synthetic")];
    const pending = defer("list_inventory");
    render(<App />);
    await go("Projeler");
    const q = await queue();
    expect(await q.findByText(/Eldeki geçici görüntüde 0 öğe var/)).toBeInTheDocument();
    expect(q.queryByText(/incelenmesi gereken öğe yok/)).not.toBeInTheDocument();
    await act(async () => pending.forEach((p) => p.resolve(backend.inventory)));
  });

  it("inventory failed on the first read: states how many accepted wishes have unknown evidence - not 'not scanned' - and a retry fixes it", async () => {
    backend.failures.list_inventory = { code: "DB_BUSY", message: "Veritabanı meşgul." };
    render(<App />);
    await go("Projeler");
    const q = await queue();
    expect(await q.findByText(/Eldeki geçici görüntüde 4 öğe var/)).toBeInTheDocument();
    expect(q.getByText(/Kayıtlı envanter okunamadı; gözlemden doğan nedenler dahil değil \(5 kabul edilmiş istek için kayıtlı gözlem bilinmiyor\)/)).toBeInTheDocument();
    expect(q.getByText(/“taranmadı” veya “bulunamadı” anlamına gelmez/)).toBeInTheDocument();
    expect(q.queryByText(/Henüz taranmadı|Son taramada bulunamadı/)).not.toBeInTheDocument();
    delete backend.failures.list_inventory;
    await userEvent.click(q.getByRole("button", { name: "Listeyi yeniden oku" }));
    expect(await q.findByText("6 öğe incelenmeyi bekliyor.")).toBeInTheDocument();
  });

  it("a failed refresh after a good read keeps the earlier list but marks it old, for the decisions and for the evidence", async () => {
    render(<App />);
    await go("Projeler");
    const q = await queue();
    await q.findByText("6 öğe incelenmeyi bekliyor.");
    backend.failures.list_inventory = { code: "DB_BUSY", message: "Veritabanı meşgul." };
    await userEvent.click((await matrix()).getByRole("button", { name: "Listeyi yenile" }));
    expect(await q.findByText(/Kayıtlı envanter yenilenemedi; gözlemden doğan nedenler önceki başarılı okumadan ve eski olabilir/)).toBeInTheDocument();
    expect(q.getByText(/^Eldeki geçici görüntüde 6 öğe var/)).toBeInTheDocument();
    expect(q.getAllByRole("button", { name: /^Matriste incele/ })).toHaveLength(7); // the earlier answer is still there: 6 items + 1 uncertain-only decision
    delete backend.failures.list_inventory;
    backend.failures.list_projects = { code: "DB_BUSY", message: "Veritabanı meşgul." };
    await userEvent.click(q.getByRole("button", { name: "Listeyi yeniden oku" }));
    expect(await q.findByText(/Kararlar yenilenemedi; gösterilenler önceki başarılı okumadan ve eski olabilir/)).toBeInTheDocument();
  });
});

describe("the Dashboard summary uses the same derivation and never shows a bare zero for unfinished evidence", () => {
  it("shows the same headline as the queue and links to it", async () => {
    render(<App />);
    const t = await tile();
    expect(await t.findByText("6 öğe incelenmeyi bekliyor.")).toBeInTheDocument();
    expect(t.getByText(/⚠ 1 karar için kayıtlı kanıt belirsiz/)).toBeInTheDocument();
    await userEvent.click(t.getByRole("button", { name: "İnceleme listesini aç" }));
    await screen.findByRole("heading", { level: 1, name: "Projeler" });
    expect(await (await queue()).findByText("6 öğe incelenmeyi bekliyor.")).toBeInTheDocument();
  });

  it("pending, failed and old reads are stated as such", async () => {
    const pending = defer("list_inventory");
    render(<App />);
    const t = await tile();
    expect(await t.findByText(/^Eldeki geçici görüntüde 4 öğe var/)).toBeInTheDocument();
    expect(t.queryByText(/incelenmesi gereken öğe yok/)).not.toBeInTheDocument();
    await act(async () => pending[0].reject({ code: "DB_BUSY", message: "meşgul" }));
    expect(await t.findByText(/Kayıtlı envanter okunamadı veya yenilenemedi/)).toBeInTheDocument();
    expect(t.getByText(/^Eldeki geçici görüntüde 4 öğe var/)).toBeInTheDocument();
  });

  it("decisions failed: no number at all", async () => {
    backend.failures.list_projects = { code: "DB_BUSY", message: "meşgul" };
    render(<App />);
    expect(await (await tile()).findByText(/Kararlar okunamadı; incelenecek öğelerin sayısı bilinmiyor/)).toBeInTheDocument();
  });
});

describe("'Matriste incele' is navigation to the exact row and nothing else", () => {
  it("opens the exact project, system, surface and tool row, focuses it, marks it, and decides / saves / resets / scans nothing", async () => {
    render(<App />);
    await go("Projeler");
    const before = backend.calls.length;
    await userEvent.click(await openButton("Alfa Araç", "Örnek: Web sitesi", MAC));
    await waitFor(() => expect(spot()).not.toBeNull());
    const m = await matrix();
    expect(m.getByLabelText("Proje")).toHaveValue(ORNEK);
    expect(m.getByLabelText("İşletim sistemi")).toHaveValue(MAC);
    expect(m.getByLabelText("Ajan yüzeyi")).toHaveValue(CC);
    await waitFor(() => expect(spot()).toHaveFocus());
    expect(within(spot()!).getByText("Öneriden gelen araç")).toBeInTheDocument();
    expect(within(spot()!).getByRole("heading", { level: 3, name: "Alfa Araç" })).toBeInTheDocument();
    expect(document.querySelectorAll(".ws-spot")).toHaveLength(1);
    // the row shows the saved decision as it is: the queue preselects nothing and changes nothing
    expect(within(spot()!).getByLabelText(/^Karar/)).toHaveValue("accepted");
    expect(within(spot()!).getByRole("button", { name: /^Yalnızca tercihimi kaydet/ })).toBeDisabled();
    for (const bad of FORBIDDEN) expect(backend.calls.slice(before).filter((c) => c.cmd === bad), bad).toHaveLength(0);
    // PRODUCT-04: the stored observation of that very row is shown next to the wish
    expect(await within(spot()!).findByText("Eski taramada bulunmuştu (güncel değil)")).toBeInTheDocument();
  });

  it("is keyboard operable: Enter on the focused button hands the target over", async () => {
    render(<App />);
    await go("Projeler");
    const b = await openButton("Alfa Araç", "Benim sitem");
    b.focus();
    expect(b).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(spot()).toHaveFocus());
    expect((await matrix()).getByLabelText("Proje")).toHaveValue(BENIM);
    expect(calls("set_project_preference")).toHaveLength(0);
  });

  it("each item goes to ITS OWN target: pressing several in turn never lands on another project, system, surface or tool", async () => {
    render(<App />);
    await go("Projeler");
    for (const [tool, proj, projAlias, platform, surface] of [
      ["Alfa Araç", "Benim sitem", BENIM, WIN, CC],
      ["Alfa Araç", "Örnek: Web sitesi", ORNEK, MAC, CC],
      ["Beta Eklenti", "Örnek: Web sitesi", ORNEK, WIN, CC],
    ] as const) {
      await userEvent.click(await openButton(tool, proj, platform, surface));
      const m = await matrix();
      await waitFor(() => expect(m.getByLabelText("Proje")).toHaveValue(projAlias));
      expect(m.getByLabelText("İşletim sistemi")).toHaveValue(platform);
      expect(m.getByLabelText("Ajan yüzeyi")).toHaveValue(surface);
      await waitFor(() => expect(within(spot()!).getByRole("heading", { level: 3 })).toHaveTextContent(tool));
      expect(document.querySelectorAll(".ws-spot")).toHaveLength(1);
    }
    expect(calls("set_project_preference")).toHaveLength(0);
    expect(calls("reset_project_preference")).toHaveLength(0);
  });

  it("a removed tool opens as removed and reset-only: no save control, nothing is reset by opening it", async () => {
    render(<App />);
    await go("Projeler");
    await userEvent.click(await openButton("Eski Kayıt", "Örnek: Web sitesi"));
    await waitFor(() => expect(spot()).not.toBeNull());
    const row = within(spot()!);
    expect(row.getByText("katalogdan kaldırıldı", { selector: "span" })).toBeInTheDocument();
    expect(row.getByRole("button", { name: /^Eski kararı sıfırla/ })).toBeEnabled();
    expect(row.queryByRole("button", { name: /^Yalnızca tercihimi kaydet/ })).not.toBeInTheDocument();
    expect(calls("reset_project_preference")).toHaveLength(0);
    expect(backend.projects[0].preferences.some((p) => p.entryId === "eski")).toBe(true);
  });

  it("a removed tool WITHOUT a decision is not in the queue at all", async () => {
    backend.projects[0].preferences = backend.projects[0].preferences.filter((p) => p.entryId !== "eski");
    render(<App />);
    await go("Projeler");
    const q = await queue();
    await q.findByText(/öğe incelenmeyi bekliyor/);
    expect(q.queryByText("Eski Kayıt")).not.toBeInTheDocument();
  });

  it("a target this version cannot show is listed with a clear note and NO button; no other target is chosen", async () => {
    backend.projects[1].preferences.push(wish("alfa", { entryName: "Alfa Araç", platformTarget: "linux-x64", materialChange: true }), wish("beta", { entryName: "Beta Eklenti", agentSurface: "vim-eklentisi", materialChange: true }));
    render(<App />);
    await go("Projeler");
    const q = await queue();
    await q.findByText(/öğe incelenmeyi bekliyor/);
    const notes = q.getAllByText(/bu sürümde matriste açılamıyor; başka bir hedef seçilmedi/);
    expect(notes).toHaveLength(2);
    expect(q.queryByRole("button", { name: /linux-x64|vim-eklentisi/ })).not.toBeInTheDocument();
    expect(spot()).toBeNull();
  });

  it("a project that is gone after a refresh leaves the queue with its items; nothing is opened elsewhere", async () => {
    render(<App />);
    await go("Projeler");
    const q = await queue();
    await q.findByText("6 öğe incelenmeyi bekliyor.");
    backend.projects = backend.projects.filter((p) => p.alias !== BENIM);
    await userEvent.click((await matrix()).getByRole("button", { name: "Listeyi yenile" }));
    expect(await q.findByText("5 öğe incelenmeyi bekliyor.")).toBeInTheDocument();
    expect(q.queryByRole("button", { name: /Benim sitem/ })).not.toBeInTheDocument();
    expect(spot()).toBeNull();
  });

  it("ordinary navigation clears the handoff: coming back to Projects does not re-focus the row", async () => {
    render(<App />);
    await go("Projeler");
    await userEvent.click(await openButton("Alfa Araç", "Benim sitem"));
    await waitFor(() => expect(spot()).toHaveFocus());
    await go("Ana Sayfa");
    await go("Projeler");
    await (await queue()).findByText(/öğe incelenmeyi bekliyor/);
    expect(spot()).toBeNull();
  });
});

describe("viewing never changes the list: there is no acknowledge, dismiss or auto-clear", () => {
  it("opening every item leaves the count and the items exactly as they were; only a changed saved decision or catalog state changes them", async () => {
    render(<App />);
    await go("Projeler");
    const q = await queue();
    await q.findByText("6 öğe incelenmeyi bekliyor.");
    const names = () => q.getAllByRole("button", { name: /^Matriste incele/ }).map((b) => b.textContent);
    const before = names();
    for (const b of q.getAllByRole("button", { name: /^Matriste incele/ })) await userEvent.click(b);
    expect(q.getByText("6 öğe incelenmeyi bekliyor.")).toBeInTheDocument();
    expect(names()).toEqual(before);
    // a real change does: the user rejects the Benim sitem wish in the matrix, which really changes the saved decision
    await userEvent.click(await openButton("Alfa Araç", "Benim sitem"));
    await waitFor(() => expect(spot()).toHaveFocus());
    await userEvent.selectOptions(within(spot()!).getByLabelText(/^Karar/), "rejected");
    await userEvent.click(within(spot()!).getByRole("button", { name: /^Yalnızca tercihimi kaydet/ }));
    await waitFor(() => expect(q.getByText("5 öğe incelenmeyi bekliyor.")).toBeInTheDocument());
    expect(calls("set_project_preference")).toHaveLength(1);
  });

  it("the new code names no acknowledge / dismiss / reviewed state, no storage, no timer and no write or scan API", () => {
    for (const [label, text] of [["review.ts", reviewLogicSource], ["ReviewQueue.tsx", reviewQueueSource], ["ReviewSummary.tsx", reviewSummarySource]] as const) {
      for (const banned of ["acknowledge", "dismiss", "markReviewed", "localStorage", "sessionStorage", "indexedDB", "document.cookie", "setInterval", "setTimeout", "requestAnimationFrame", "fetch(", "XMLHttpRequest", "WebSocket", "window.open", "navigator.sendBeacon", "@tauri-apps", "invoke(", "dangerouslySetInnerHTML", "href=", "runDiscoveryScan", "setProjectPreference", "resetProjectPreference", "evaluateTask", "prepareInstall", "confirmInstall"]) {
        expect(text.includes(banned), `${label}: ${banned}`).toBe(false);
      }
    }
    const used = (t: string) => [...new Set([...t.matchAll(/\bapi\.([A-Za-z]+)/g)].map((m) => m[1]))].sort();
    expect(used(reviewLogicSource)).toEqual([]);
    expect(used(reviewQueueSource)).toEqual([]); // the queue reads through the loads the Projects page already has
    expect(used(reviewSummarySource)).toEqual(["listInventory", "listProjects"]);
  });
});

describe("privacy and safety", () => {
  it("no project note, observation location, evidence text or version ever appears; hostile names are plain text", async () => {
    backend.inventory = {
      ...backend.inventory,
      observations: [obs({ subject: "alfa", location: "C:\\Users\\gizli-kisi\\bin\\alfa.exe", evidence: "GIZLI-KANIT-METNI", version: "9.9.9-gizli" })],
    };
    backend.projects[0].preferences.push(wish("beta", { entryName: "<img src=x onerror=alert(1)> <b>kalın</b>", agentSurface: CLI, materialChange: true }));
    render(<App />);
    await go("Projeler");
    const q = await queue();
    await q.findByText(/öğe incelenmeyi bekliyor/);
    expect((await q.findAllByText(/<img src=x onerror=alert\(1\)>/)).length).toBeGreaterThan(0);
    expect(document.querySelector("img")).toBeNull();
    expect(document.querySelector(".rq-item b")).toBeNull();
    const text = document.body.textContent ?? "";
    for (const secret of ["gizli-kisi", "alfa.exe", "GIZLI-KANIT-METNI", "9.9.9-gizli", "C:\\Users"]) expect(text.includes(secret), secret).toBe(false);
    // the project note is shown by the existing project cards (unchanged), but never by the queue or the dashboard tile
    expect(q.getByRole("heading", { level: 2, name: "İncelenmesi Gerekenler" }).closest("section")!.textContent).not.toContain("GİZLİ-PROJE-NOTU");
    await go("Ana Sayfa");
    expect((await tile()).getByRole("heading", { level: 2 }).closest("section")!.textContent).not.toContain("GİZLİ-PROJE-NOTU");
  });

  it("a whole session - dashboard, queue, filters, every item, refresh - issues only the existing reads; no scan, no write, no install, no network, no window, no polling", async () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal("fetch", fetchSpy);
    const openSpy = vi.spyOn(window, "open").mockImplementation(() => null);
    render(<App />);
    await (await tile()).findByText(/öğe incelenmeyi bekliyor/);
    await go("Projeler");
    const q = await queue();
    await q.findByText("6 öğe incelenmeyi bekliyor.");
    await userEvent.selectOptions(q.getByLabelText("Kuyrukta proje süz"), ORNEK);
    await userEvent.selectOptions(q.getByLabelText("Kuyrukta neden süz"), "material_change");
    for (const b of q.getAllByRole("button", { name: /^Matriste incele/ })) await userEvent.click(b);
    await userEvent.click((await matrix()).getByRole("button", { name: "Listeyi yenile" }));
    await waitFor(() => expect(calls("list_inventory").length).toBeGreaterThanOrEqual(3));

    const used = new Set(backend.calls.map((c) => c.cmd));
    for (const cmd of used) expect(["get_app_info", "get_settings", "get_dashboard", "list_projects", "list_catalog", "list_inventory"], cmd).toContain(cmd);
    for (const bad of FORBIDDEN) expect(used.has(bad), bad).toBe(false);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(openSpy).not.toHaveBeenCalled();

    // nothing keeps reading by itself
    const settled = backend.calls.length;
    await act(async () => { await new Promise((r) => setTimeout(r, 120)); });
    expect(backend.calls.length).toBe(settled);
  });

  it("opening the queue itself (Projects page) issues no read beyond the matrix's own three; the dashboard tile adds exactly two", async () => {
    render(<App />);
    await (await tile()).findByText(/öğe incelenmeyi bekliyor/);
    expect(calls("list_projects")).toHaveLength(1);
    expect(calls("list_inventory")).toHaveLength(1);
    await go("Projeler");
    await (await queue()).findByText("6 öğe incelenmeyi bekliyor.");
    expect(calls("list_projects")).toHaveLength(2); // one for the Projects page, shared by the queue and the matrix
    expect(calls("list_inventory")).toHaveLength(2);
    expect(calls("list_catalog")).toHaveLength(1);
  });
});

describe("existing flows stay intact", () => {
  it("the matrix still saves a decision through its own button and the queue follows the saved state", async () => {
    render(<App />);
    await go("Projeler");
    const m = await matrix();
    await userEvent.selectOptions(m.getByLabelText("Proje"), BENIM);
    const row = within((await m.findByRole("heading", { level: 3, name: "Beta Eklenti" })).closest("article") as HTMLElement);
    await userEvent.selectOptions(row.getByLabelText(/^Karar/), "rejected");
    await userEvent.click(row.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ }));
    await waitFor(() => expect(calls("set_project_preference")).toHaveLength(1));
    expect(calls("set_project_preference")[0].args).toMatchObject({ alias: BENIM, entryId: "beta", decision: "rejected" });
    expect((await queue()).getByText("6 öğe incelenmeyi bekliyor.")).toBeInTheDocument(); // a rejected, unchanged wish adds no reason
  });

  it("project creation and removal still work and refresh the queue", async () => {
    render(<App />);
    await go("Projeler");
    await userEvent.type(screen.getByLabelText(/^Takma ad/), "yeni-proje");
    await userEvent.type(screen.getByLabelText("Görünen ad"), "Yeni proje");
    await userEvent.click(screen.getByRole("button", { name: "Proje kaydını ekle" }));
    await waitFor(() => expect(calls("add_project_reference")).toHaveLength(1));
    expect(await screen.findByRole("heading", { level: 2, name: /Yeni proje/ })).toBeInTheDocument();
    expect((await queue()).getByText("6 öğe incelenmeyi bekliyor.")).toBeInTheDocument();
  });

  it("the other pages are reachable and unaffected (library, plan, prompts, inventory)", async () => {
    render(<App />);
    for (const name of ["Araç Kütüphanesi", "Kurulum Planı", "İstem Kütüphanesi", "Bilgisayar Envanteri"]) {
      await go(name);
    }
    expect(calls("run_discovery_scan")).toHaveLength(0);
  });
});

describe("P05-RT-01: a read that is still running is never final (through the real App)", () => {
  const NONE = /incelenmesi gereken öğe yok/;
  const PROVISIONAL0 = /^Eldeki geçici görüntüde 0 öğe var; okuma sürüyor, sayı artabilir veya azalabilir \(sonuç kesin değil\)\./;
  const cleanWorld = () => {
    backend.projects = [project(ORNEK, "Örnek: Web sitesi", [wish("alfa", { entryName: "Alfa Araç" })], "synthetic")];
    backend.inventory = { ...backend.inventory, requests: [request("alfa", "present_current")] };
  };
  const clone = <T,>(v: T): T => JSON.parse(JSON.stringify(v));
  const refresh = async () => userEvent.click((await matrix()).getByRole("button", { name: "Listeyi yenile" }));
  const betaRow = async () => within(((await (await matrix()).findByRole("heading", { level: 3, name: "Beta Eklenti" })).closest("article")) as HTMLElement);

  it.each([["list_projects"], ["list_inventory"], ["both"]])("clean queue, then a refresh with %s pending: provisional - never 'nothing to review' - and final again once it settles", async (which) => {
    cleanWorld();
    render(<App />);
    await go("Projeler");
    const q = await queue();
    expect(await q.findByText(NONE)).toBeInTheDocument();
    const gates = { list_projects: which !== "list_inventory" ? defer("list_projects") : [], list_inventory: which !== "list_projects" ? defer("list_inventory") : [] };
    await refresh();
    await waitFor(() => {
      if (which !== "list_inventory") expect(gates.list_projects).toHaveLength(1);
      if (which !== "list_projects") expect(gates.list_inventory).toHaveLength(1);
    });
    expect(q.getByText(PROVISIONAL0)).toBeInTheDocument();
    expect(q.queryByText(NONE)).not.toBeInTheDocument();
    if (which !== "list_inventory") expect(q.getByText(/Kararlar yeniden okunuyor; gösterilenler kesin değil/)).toBeInTheDocument();
    if (which !== "list_projects") expect(q.getByText(/Kayıtlı envanter yeniden okunuyor; gözlemden doğan nedenler kesin değil/)).toBeInTheDocument();
    expect(q.queryByRole("button", { name: "Listeyi yeniden oku" })).not.toBeInTheDocument(); // nothing failed: no retry to offer
    await act(async () => {
      gates.list_projects.forEach((g) => g.resolve(backend.projects));
      gates.list_inventory.forEach((g) => g.resolve(backend.inventory));
    });
    expect(await q.findByText(NONE)).toBeInTheDocument();
    expect(q.queryByText(/kesin değil/)).not.toBeInTheDocument();
    const settled = backend.calls.length;
    await act(async () => { await new Promise((r) => setTimeout(r, 80)); });
    expect(backend.calls.length).toBe(settled); // nothing polls
  });

  it("retained NON-empty queue during a refresh: still listed, but a provisional snapshot count and marked not final; the answer replaces it", async () => {
    render(<App />);
    await go("Projeler");
    const q = await queue();
    await q.findByText("6 öğe incelenmeyi bekliyor.");
    const projectsGate = defer("list_projects");
    const inventoryGate = defer("list_inventory");
    await refresh();
    await waitFor(() => { expect(projectsGate).toHaveLength(1); expect(inventoryGate).toHaveLength(1); });
    expect(q.getByText("Eldeki geçici görüntüde 6 öğe var; okuma sürüyor, sayı artabilir veya azalabilir (sonuç kesin değil).")).toBeInTheDocument();
    expect(q.queryByText("6 öğe incelenmeyi bekliyor.")).not.toBeInTheDocument();
    expect(q.getAllByRole("button", { name: /^Matriste incele/ })).toHaveLength(7); // the retained answer is still shown
    backend.projects = backend.projects.filter((p) => p.alias !== BENIM);
    await act(async () => { projectsGate[0].resolve(backend.projects); inventoryGate[0].resolve(backend.inventory); });
    expect(await q.findByText("5 öğe incelenmeyi bekliyor.")).toBeInTheDocument();
  });

  it("P05-RT-02: six cached items, the deferred answer has five - the count during the refresh is no lower bound, and the final count is exact", async () => {
    render(<App />);
    await go("Projeler");
    const q = await queue();
    await q.findByText("6 öğe incelenmeyi bekliyor.");
    const projectsGate = defer("list_projects");
    const inventoryGate = defer("list_inventory");
    await refresh();
    await waitFor(() => { expect(projectsGate).toHaveLength(1); expect(inventoryGate).toHaveLength(1); });
    expect(q.getByText(/^Eldeki geçici görüntüde 6 öğe var; okuma sürüyor, sayı artabilir veya azalabilir \(sonuç kesin değil\)\./)).toBeInTheDocument();
    expect(q.queryByText(/En az/)).not.toBeInTheDocument();
    expect(q.queryByText(/^6 öğe incelenmeyi bekliyor\./)).not.toBeInTheDocument();
    backend.projects = backend.projects.filter((p) => p.alias !== BENIM);
    await act(async () => { projectsGate[0].resolve(backend.projects); inventoryGate[0].resolve(backend.inventory); });
    expect(await q.findByText("5 öğe incelenmeyi bekliyor.")).toBeInTheDocument();
    expect(q.queryByText(/geçici görüntü|En az|kesin değil/)).not.toBeInTheDocument();
  });

  it("overlapping reads, the NEWER one settles first: it is final at once, and the late older answer changes nothing", async () => {
    cleanWorld();
    render(<App />);
    await go("Projeler");
    const q = await queue();
    await q.findByText(NONE);
    const fresh = clone(backend.projects);
    const stale = clone(backend.projects);
    stale[0].preferences[0].materialChange = true; // what the older read would have said: one item
    const projectsGate = defer("list_projects");
    const inventoryGate = defer("list_inventory");
    await refresh(); // older request
    await waitFor(() => expect(projectsGate).toHaveLength(1));
    const beta = await betaRow();
    await userEvent.selectOptions(beta.getByLabelText(/^Karar/), "rejected");
    await userEvent.click(beta.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ })); // a save re-reads: the newer request
    await waitFor(() => { expect(projectsGate).toHaveLength(2); expect(inventoryGate).toHaveLength(2); });
    expect(q.getByText(PROVISIONAL0)).toBeInTheDocument();
    await act(async () => { projectsGate[1].resolve(fresh); inventoryGate[1].resolve(backend.inventory); });
    expect(await q.findByText(NONE)).toBeInTheDocument();
    await act(async () => { projectsGate[0].resolve(stale); inventoryGate[0].resolve(backend.inventory); }); // late, older
    expect(q.getByText(NONE)).toBeInTheDocument();
    expect(q.queryByRole("button", { name: /^Matriste incele/ })).not.toBeInTheDocument();
    expect(q.queryByText(/kesin değil|geçici görüntü/)).not.toBeInTheDocument();
  });

  it("overlapping reads, the OLDER one settles first: it is ignored and the queue stays provisional until the newer one answers", async () => {
    cleanWorld();
    render(<App />);
    await go("Projeler");
    const q = await queue();
    await q.findByText(NONE);
    const fresh = clone(backend.projects);
    const stale = clone(backend.projects);
    stale[0].preferences[0].materialChange = true;
    const projectsGate = defer("list_projects");
    const inventoryGate = defer("list_inventory");
    await refresh();
    await waitFor(() => expect(projectsGate).toHaveLength(1));
    const beta = await betaRow();
    await userEvent.selectOptions(beta.getByLabelText(/^Karar/), "rejected");
    await userEvent.click(beta.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ }));
    await waitFor(() => { expect(projectsGate).toHaveLength(2); expect(inventoryGate).toHaveLength(2); });
    await act(async () => { projectsGate[0].resolve(stale); inventoryGate[0].resolve(backend.inventory); }); // older answers first
    expect(q.getByText(PROVISIONAL0)).toBeInTheDocument(); // still waiting for the newer one
    expect(q.queryByText(NONE)).not.toBeInTheDocument();
    expect(q.queryByRole("button", { name: /^Matriste incele/ })).not.toBeInTheDocument(); // the older answer's item never shows
    await act(async () => { projectsGate[1].resolve(fresh); inventoryGate[1].resolve(backend.inventory); });
    expect(await q.findByText(NONE)).toBeInTheDocument();
  });

  it("a refresh that FAILS keeps the retained answer as 'old' with a retry (not as 'refreshing' and never as clean), and a successful retry makes it final", async () => {
    cleanWorld();
    render(<App />);
    await go("Projeler");
    const q = await queue();
    await q.findByText(NONE);
    const projectsGate = defer("list_projects");
    await refresh();
    await waitFor(() => expect(projectsGate).toHaveLength(1));
    expect(q.getByText(PROVISIONAL0)).toBeInTheDocument();
    await act(async () => projectsGate[0].reject({ code: "DB_BUSY", message: "Veritabanı meşgul." }));
    expect(await q.findByText(/Kararlar yenilenemedi; gösterilenler önceki başarılı okumadan ve eski olabilir/)).toBeInTheDocument();
    expect(q.getByText(/^Eldeki geçici görüntüde 0 öğe var; liste eksik veya eski olabilir, sayı artabilir veya azalabilir \(sonuç kesin değil\)\./)).toBeInTheDocument();
    expect(q.queryByText(NONE)).not.toBeInTheDocument();
    expect(q.queryByText(/okuma sürüyor/)).not.toBeInTheDocument();
    await userEvent.click(q.getByRole("button", { name: "Listeyi yeniden oku" }));
    await waitFor(() => expect(projectsGate).toHaveLength(2));
    await act(async () => projectsGate[1].resolve(backend.projects));
    expect(await q.findByText(NONE)).toBeInTheDocument();
  });

  it("the Dashboard summary and the queue print the same headline for the same final state", async () => {
    cleanWorld();
    render(<App />);
    const t = await tile();
    const zeroText = (await t.findByText(NONE)).textContent;
    await go("Projeler");
    expect((await (await queue()).findByText(NONE)).textContent).toBe(zeroText);
  });
});

// keep the unused-import linter honest about helpers used only in some branches
void ({} as Inventory);
