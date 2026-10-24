// PRODUCT-04 - "Son kayıtlı envanter gözlemi" in the Project Tool Workspace: what the last STORED discovery observation said about an accepted wish,
// next to (and apart from) the wish. Rendered inside the real Projects page against the in-memory backend. Mocked IPC and jsdom only: no real window,
// no real database, no real scan. The Rust classifier (store.rs::classify_request) is covered by the Rust tests; here its answers are fixtures.
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import observationComponentSource from "./components/ObservationSection.tsx?raw";
import observationLogicSource from "./observation.ts?raw";
import { ProjectsPage } from "./pages/Projects";
import { createBackend, makeEntry, makeRequest, obs, pref, scanRef, type Backend } from "./test/backend";
import type { Inventory, PreferenceView, ProjectView, RequestStatus } from "./types";
import { formatTime } from "./labels";
import type { MatrixHandoff } from "./workspace";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const WIN = "windows-x64";
const MAC = "macos-arm64";
const CC = "claude-code-terminal";
const CLI = "local-cli";
const ORNEK = "ornek-web-sitesi";
const BENIM = "benim-sitem";
const ev = (platformTarget: string, agentSurface: string) => ({ platformTarget, agentSurface, compatibilityState: "vendor-declared", limitation: "kayıt: vendor-declared" });
const FOUND = "Son taramada bulundu";
const NOT_FOUND = "Son taramada bulunamadı";
const UNDETERMINED = "Son taramada belirlenemedi";
const WAS_FOUND = "Eski taramada bulunmuştu (güncel değil)";
const NOT_SCANNED = "Henüz taranmadı";
const ANY_STATUS = /Son taramada bulundu|Son taramada bulunamadı|Son taramada belirlenemedi|Eski taramada|Başka bir işletim sistemi için gözlem var|Henüz taranmadı/;

const alfa = () => makeEntry("alfa", { displayName: "Alfa Araç", record: { targetEvidence: [ev(WIN, CC), ev(MAC, CC), ev(WIN, CLI)] } });
const beta = () => makeEntry("beta", { displayName: "Beta Eklenti", kind: "agent_plugin", record: { targetEvidence: [ev(WIN, CC)] } });
const gama = () => makeEntry("gama", { displayName: "Gama Kapalı", enabled: false, record: { targetEvidence: [ev(WIN, CC)] } });
const delta = () => makeEntry("delta", { displayName: "Delta Kısıtlı", executionPolicy: "blocked-until-scoped-authorization", reviewState: "quarantined", record: { targetEvidence: [ev(WIN, CC)] } });
const eski = () => makeEntry("eski", { displayName: "Eski Kayıt", removedAt: "2026-10-18T00:00:00Z", enabled: false, record: { targetEvidence: [ev(WIN, CC)] } });
const yeni = () => makeEntry("yeni", { displayName: "Yeni Kayıt", record: { targetEvidence: [ev(WIN, CC)] } });
const mystery = () => makeEntry("gizemli", { displayName: "Gizemli Araç", record: {} }); // no compatibility evidence at all: unknown

const wish = (entryId: string, decision: PreferenceView["decision"] | string, platformTarget = WIN, agentSurface = CC) =>
  pref({ entryId, entryName: entryId, agentSurface, platformTarget, decision: decision as PreferenceView["decision"] });
const project = (alias: string, name: string, preferences: PreferenceView[], kind: ProjectView["kind"] = "reference"): ProjectView => ({
  projectId: `id-${alias}`, alias, displayName: name, kind, localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences,
});
const inventoryWith = (...requests: Inventory["requests"]): Inventory => ({ ...backend.inventory, requests });

const calls = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd);
const region = async () => within(await screen.findByRole("region", { name: "Proje Araç Matrisi" }));
const rowOf = async (name: string) => {
  const r = await region();
  return within((await r.findByRole("heading", { level: 3, name })).closest("article") as HTMLElement);
};
const obsOf = async (name: string) => (await rowOf(name)).getByRole("region", { name: /^Son kayıtlı envanter gözlemi/ });
const obsText = async (name: string) => within(await obsOf(name));
const open = async (handoff: MatrixHandoff | null = null) => {
  render(<ProjectsPage handoff={handoff} />);
  await rowOf("Alfa Araç");
  return region();
};
/** The text of the observation block without the collapsed "what this does not show" list (which names the very words a positive claim would use). */
const visibleText = async (name: string) => {
  const clone = (await obsOf(name)).cloneNode(true) as HTMLElement;
  clone.querySelectorAll("details").forEach((d) => d.remove());
  return clone.textContent ?? "";
};

/** Holds every `list_inventory` call until the test answers it, in any order. */
function deferInventory() {
  const original = backend.handle;
  const pending: { resolve: (inv: Inventory) => void; reject: (e: unknown) => void }[] = [];
  backend.handle = (cmd, args) => {
    if (cmd !== "list_inventory") return original(cmd, args);
    backend.calls.push({ cmd, args });
    return new Promise((resolve, reject) => pending.push({ resolve: (inv) => resolve(JSON.parse(JSON.stringify(inv))), reject }));
  };
  return pending;
}

beforeEach(() => {
  backend = createBackend();
  backend.entries = [alfa(), beta(), gama(), delta(), eski(), yeni(), mystery()];
  backend.projects = [
    project(ORNEK, "Örnek: Web sitesi", [
      wish("alfa", "accepted"), wish("alfa", "accepted", MAC), wish("alfa", "accepted", WIN, CLI),
      wish("beta", "rejected"), wish("gama", "postponed"), wish("delta", "disabled"), wish("eski", "accepted"),
      wish("gizemli", "accepted"),
    ], "synthetic"),
    project(BENIM, "Benim sitem", [wish("alfa", "accepted")]),
  ];
  backend.inventory = inventoryWith(
    makeRequest({ status: "present_current" }),
    makeRequest({ status: "absent_current", projectAlias: BENIM, projectName: "Benim sitem" }),
    makeRequest({ status: "previously_present", platformTarget: MAC }),
    makeRequest({ status: "unknown_current", agentSurface: CLI }),
  );
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("the row shows the wish and the last stored observation as two separate things", () => {
  it("an accepted wish with a matching stored request shows the backend's status, its note and the scan it rests on", async () => {
    await open();
    const o = await obsText("Alfa Araç");
    expect(await o.findByText(FOUND, { selector: "strong" })).toBeInTheDocument();
    expect(o.getByText(/bulunması, çalıştığı veya projede etkin olduğu anlamına gelmez/)).toBeInTheDocument();
    expect(o.getByText(new RegExp(`Dayanak tarama: ${formatTime(scanRef().finishedAt).replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`))).toBeInTheDocument();
    expect(o.getByText(/taramaya verilen izinler: program adları \(PATH\)/)).toBeInTheDocument();
    // the wish is its own block in the same row
    const a = await rowOf("Alfa Araç");
    expect(a.getByText("Bu projede kullanmak istiyorum", { selector: "strong" })).toBeInTheDocument();
  });

  it("says in plain Turkish that this is not a live check and that no scan is started", async () => {
    await open();
    const o = await obsText("Alfa Araç");
    expect(await o.findByText(/son kaydedilen tarama sonucudur; şu anki bir denetim değildir/)).toBeInTheDocument();
    expect(o.getByText(/hiçbir tarama başlatmaz ve bilgisayara bakmaz/)).toBeInTheDocument();
  });

  it("explains what a name match, an observation, a catalog switch and a wish do NOT mean", async () => {
    await open();
    const o = await obsText("Alfa Araç");
    const summary = await o.findByText("Bu gözlem neyi göstermez?");
    await userEvent.click(summary);
    expect(o.getByText(/Ad eşleşmesi kimlik doğrulaması değildir/)).toBeInTheDocument();
    expect(o.getByText(/aracın kurulu, çalışıyor, oturum açılmış veya doğrulanmış olduğu anlamına gelmez; işlev hiç denenmedi/)).toBeInTheDocument();
    expect(o.getByText(/Katalogdaki “etkin” işareti, ajanın bu aracı kullandığı anlamına gelmez/)).toBeInTheDocument();
    expect(o.getByText(/İsteğiniz de aracın kurulu olduğunu göstermez/)).toBeInTheDocument();
    expect(o.getByText(/projeden bağımsız olarak her proje için geçerli sayılabilir/)).toBeInTheDocument();
  });

  it("'found' is never worded as installed, working, authenticated, activated or verified", async () => {
    await open();
    await (await obsText("Alfa Araç")).findByText(FOUND);
    const text = await visibleText("Alfa Araç");
    expect(text).toMatch(/bulundu/);
    expect(text).not.toMatch(/Kuruldu|kurulu olarak|Kurulu\b|çalışıyor|doğrulandı|doğrulanmış olarak|etkinleştirildi|oturum açıldı|başarı/i);
    expect(screen.queryByText(/✓|✔|başarılı/)).not.toBeInTheDocument(); // no success badge anywhere on the page
  });

  it("every row has a named observation region under its own heading, so the blocks are not confused with one another", async () => {
    await open();
    const r = await region();
    await waitFor(() => expect(r.getAllByRole("heading", { level: 4 })).toHaveLength(r.getAllByRole("heading", { level: 3 }).length));
    const names = r.getAllByRole("region", { name: /^Son kayıtlı envanter gözlemi/ }).map((s) => s.getAttribute("aria-labelledby"));
    expect(new Set(names).size).toBe(names.length);
    expect(r.getByRole("region", { name: "Son kayıtlı envanter gözlemi (Alfa Araç)" })).toBeInTheDocument();
  });
});

describe("the key is project + tool + operating system + agent surface", () => {
  it("shows a different outcome for the same tool in each target and never carries one over to another", async () => {
    await open();
    const r = await region();
    expect(await (await obsText("Alfa Araç")).findByText(FOUND)).toBeInTheDocument(); // Örnek · Windows · Claude Code terminal

    await userEvent.selectOptions(r.getByLabelText("Proje"), BENIM);
    const benim = await obsText("Alfa Araç");
    expect(await benim.findByText(NOT_FOUND)).toBeInTheDocument();
    expect(benim.queryByText(FOUND)).not.toBeInTheDocument();

    await userEvent.selectOptions(r.getByLabelText("Proje"), ORNEK);
    await userEvent.selectOptions(r.getByLabelText("İşletim sistemi"), MAC);
    const mac = await obsText("Alfa Araç");
    expect(await mac.findByText(WAS_FOUND)).toBeInTheDocument();
    expect(mac.queryByText(FOUND)).not.toBeInTheDocument();

    await userEvent.selectOptions(r.getByLabelText("İşletim sistemi"), WIN);
    await userEvent.selectOptions(r.getByLabelText("Ajan yüzeyi"), CLI);
    const cli = await obsText("Alfa Araç");
    expect(await cli.findByText(UNDETERMINED)).toBeInTheDocument();
    expect(cli.queryByText(WAS_FOUND)).not.toBeInTheDocument();

    expect(calls("list_inventory")).toHaveLength(1); // moving between targets re-reads nothing
    expect(calls("run_discovery_scan")).toHaveLength(0);
  });

  it("a target with an accepted wish but no stored request for exactly that key is explained, not filled from a neighbour", async () => {
    backend.inventory = inventoryWith(makeRequest({ status: "present_current", platformTarget: MAC }), makeRequest({ status: "present_current", projectAlias: BENIM, projectName: "Benim sitem" }));
    await open();
    const o = await obsText("Alfa Araç"); // Örnek · Windows · CC: only a macOS row and another project's row exist
    // "tam hedef" is a <strong> in between, so the text node before and after it is matched
    expect(await o.findByText(/okunan envanterde bu\s+için eşleşen satır yok/)).toBeInTheDocument();
    expect(o.queryByText(ANY_STATUS)).not.toBeInTheDocument();
  });

  it("a backend row that names the right tool and project but another surface is not shown under this surface", async () => {
    backend.inventory = inventoryWith(makeRequest({ status: "present_current", agentSurface: CLI }));
    await open();
    const o = await obsText("Alfa Araç");
    expect(await o.findByText(/eşleşen satır yok/)).toBeInTheDocument();
    expect(o.queryByText(FOUND)).not.toBeInTheDocument();
  });
});

describe("no accepted wish, no comparison", () => {
  it("rejected, postponed, disabled and undecided rows say there is no accepted wish - even where the inventory holds a row for that key", async () => {
    backend.inventory = inventoryWith(
      ...["beta", "gama", "delta", "yeni"].map((entryId) => makeRequest({ entryId, status: "present_current" })), // an old read can still carry rows for wishes that changed
    );
    await open();
    for (const name of ["Beta Eklenti", "Gama Kapalı", "Delta Kısıtlı", "Yeni Kayıt"]) {
      const o = await obsText(name);
      expect(o.getByText(/kabul edilmiş bir isteğiniz yok/), name).toBeInTheDocument();
      expect(o.getByText(/aracın bilgisayarda olmadığı ya da hiç taranmadığı anlamına/), name).toBeInTheDocument();
      expect(o.queryByText(ANY_STATUS), name).not.toBeInTheDocument();
    }
  });

  it("'no accepted wish' is not worded as 'absent' or 'not scanned'", async () => {
    await open();
    expect(await visibleText("Beta Eklenti")).not.toMatch(/Henüz taranmadı|bulunamadı\b(?!\s*anlamına)|Taranmadı/);
  });
});

describe("a catalog entry that was removed", () => {
  it("keeps its old wish visible, gets no comparison and no invented observation - even if a hostile row is present", async () => {
    backend.inventory = inventoryWith(makeRequest({ entryId: "eski", entryName: "Eski Kayıt", status: "present_current" }));
    await open();
    const o = await obsText("Eski Kayıt");
    expect(o.getByText(/Bu araç katalogdan kaldırıldı/)).toBeInTheDocument();
    expect(o.queryByText(ANY_STATUS)).not.toBeInTheDocument();
    expect((await rowOf("Eski Kayıt")).getByText("katalogdan kaldırıldı", { selector: "span" })).toBeInTheDocument();
  });
});

describe("every status the backend defines", () => {
  const cases: [RequestStatus, string][] = [
    ["present_current", FOUND], ["absent_current", NOT_FOUND], ["unknown_current", UNDETERMINED],
    ["previously_present", WAS_FOUND], ["previously_absent", "Eski taramada bulunamamıştı (güncel değil)"], ["previously_unknown", "Eski taramada belirlenememişti (güncel değil)"],
    ["platform_mismatch", "Başka bir işletim sistemi için gözlem var (bu tercih için geçerli değil)"], ["not_scanned", NOT_SCANNED],
  ];

  it.each(cases)("%s -> %s, with the backend's own note", async (status, label) => {
    const request = makeRequest({ status });
    backend.inventory = inventoryWith(request);
    await open();
    const o = await obsText("Alfa Araç");
    expect(await o.findByText(label, { selector: "strong" })).toBeInTheDocument();
    expect(o.getByText(request.note)).toBeInTheDocument();
    // exactly one status label, and it is that one
    expect(o.getAllByText(ANY_STATUS, { selector: "strong" })).toHaveLength(1);
    if (request.basis) expect(o.getByText(/Dayanak tarama:/)).toBeInTheDocument();
    else expect(o.getByText(/dayanak tarama kaydı gelmedi/)).toBeInTheDocument();
  });

  it("a stale result is labelled old and does not use the wording of a current one", async () => {
    backend.inventory = inventoryWith(makeRequest({ status: "previously_present" }));
    await open();
    const text = await visibleText("Alfa Araç");
    await waitFor(async () => expect(await visibleText("Alfa Araç")).toContain("güncel değil"));
    expect(text).not.toContain(FOUND);
    expect(await visibleText("Alfa Araç")).toMatch(/hiçbir işlem için dayanak sayılmaz/);
  });

  it("the permission scope is the scan's: a fingerprint permission is reported as a permission, not as verification", async () => {
    backend.inventory = inventoryWith(makeRequest({ status: "present_current", basis: scanRef({ authorizedFingerprint: true, authorizedUserConfig: true }) }));
    await open();
    const o = await obsText("Alfa Araç");
    expect(await o.findByText(/taramaya verilen izinler: program adları \(PATH\) \+ Claude Code kullanıcı dosyaları \+ dosya parmak izi \(SHA-256\)/)).toBeInTheDocument();
    expect(await visibleText("Alfa Araç")).not.toMatch(/doğrulandı|doğrulanmış/i);
  });

  it("a status this version does not know is shown as unknown and never as a success", async () => {
    backend.inventory = inventoryWith(makeRequest({ status: "verified_installed" as RequestStatus }));
    await open();
    const o = await obsText("Alfa Araç");
    expect(await o.findByText(/tanımadığı bir gözlem durumu geldi/)).toBeInTheDocument();
    expect(o.getByText(/Başarı sayılmaz/)).toBeInTheDocument();
    expect(o.queryByText(ANY_STATUS)).not.toBeInTheDocument();
  });

  it("two contradictory rows for one key show neither", async () => {
    backend.inventory = inventoryWith(makeRequest({ status: "present_current" }), makeRequest({ status: "absent_current" }));
    await open();
    const o = await obsText("Alfa Araç");
    expect(await o.findByText(/birden fazla satır var/)).toBeInTheDocument();
    expect(o.queryByText(ANY_STATUS)).not.toBeInTheDocument();
  });
});

describe("nothing stored yet", () => {
  it("no scans and no requests: an accepted wish is explained as 'no matching row' - never as 'not scanned', 'not found' or a reassuring zero", async () => {
    backend.inventory = { ...backend.inventory, lastScan: null, observations: [], requests: [] };
    await open();
    const o = await obsText("Alfa Araç");
    expect(await o.findByText(/eşleşen satır yok/)).toBeInTheDocument();
    expect(o.getByText(/Bir gözlem varsayılmaz/)).toBeInTheDocument();
    expect(o.queryByText(ANY_STATUS)).not.toBeInTheDocument();
  });

  it("an observation in the inventory does not make a row appear by itself: only the backend's request classification is used", async () => {
    backend.inventory = { ...backend.inventory, requests: [], observations: [obs({ subject: "alfa", platformTarget: WIN, agentSurface: CC, presence: "present", current: true })] };
    await open();
    const o = await obsText("Alfa Araç");
    expect(await o.findByText(/eşleşen satır yok/)).toBeInTheDocument();
    expect(o.queryByText(FOUND)).not.toBeInTheDocument();
  });

  it("unknown compatibility stays unknown and 'kullanmak istiyorum' stays unavailable, whatever the observation says", async () => {
    backend.inventory = inventoryWith(makeRequest({ entryId: "gizemli", entryName: "Gizemli Araç", status: "present_current" }));
    await open();
    const g = await rowOf("Gizemli Araç");
    expect(await within(await obsOf("Gizemli Araç")).findByText(FOUND)).toBeInTheDocument();
    expect(g.getByText("Bilinmiyor", { selector: "strong" })).toBeInTheDocument();
    expect(g.getByRole("option", { name: /Bu projede kullanmak istiyorum \(şu an seçilemez\)/ })).toBeDisabled();
  });
});

describe("loading, failure, retry and age of the stored read", () => {
  it("while the read is pending no observation is shown; the matrix is usable meanwhile", async () => {
    const pending = deferInventory();
    await open();
    await waitFor(() => expect(pending).toHaveLength(1));
    const o = await obsText("Alfa Araç");
    expect(o.getByText(/Kayıtlı envanter okunuyor/)).toBeInTheDocument();
    expect(o.queryByText(ANY_STATUS)).not.toBeInTheDocument();
    const y = await rowOf("Yeni Kayıt");
    expect(y.getByLabelText(/^Karar/)).toBeEnabled(); // the wish part works without the inventory
    await act(async () => pending[0].resolve(backend.inventory));
    expect(await o.findByText(FOUND)).toBeInTheDocument();
    expect(o.queryByText(/okunuyor/)).not.toBeInTheDocument();
  });

  it("a failed read is shown as an error and as 'unknown' - not as 'not scanned', 'not found' or empty - and can be retried", async () => {
    backend.failures.list_inventory = { code: "DB_BUSY", message: "Veritabanı meşgul." };
    await open();
    const r = await region();
    expect(await r.findByText(/Veritabanı meşgul\./)).toBeInTheDocument();
    const o = await obsText("Alfa Araç");
    expect(await o.findByText(/Kayıtlı envanter okunamadı; bu araç için gözlem/)).toBeInTheDocument();
    expect(o.getByText(/“taranmadı” veya “bulunamadı” anlamına gelmez/)).toBeInTheDocument();
    expect(o.queryByText(ANY_STATUS)).not.toBeInTheDocument();
    expect(r.queryByText(ANY_STATUS)).not.toBeInTheDocument(); // no row anywhere shows a positive or negative observation
    // rows that never depended on the inventory keep saying so
    expect((await obsText("Beta Eklenti")).getByText(/kabul edilmiş bir isteğiniz yok/)).toBeInTheDocument();

    delete backend.failures.list_inventory;
    await userEvent.click(r.getByRole("button", { name: "Yeniden dene" }));
    expect(await (await obsText("Alfa Araç")).findByText(FOUND)).toBeInTheDocument();
    expect(r.queryByText(/Veritabanı meşgul/)).not.toBeInTheDocument();
    expect(calls("list_inventory")).toHaveLength(2);
  });

  it("a failed refresh after a good read keeps the old result but marks it old; a successful retry clears the mark", async () => {
    await open();
    const r = await region();
    expect(await (await obsText("Alfa Araç")).findByText(FOUND)).toBeInTheDocument();
    backend.failures.list_inventory = { code: "DB_BUSY", message: "Veritabanı meşgul." };
    await userEvent.click(r.getByRole("button", { name: "Listeyi yenile" }));
    expect(await r.findByText(/Veritabanı meşgul\./)).toBeInTheDocument();
    const o = await obsText("Alfa Araç");
    expect(o.getByText(FOUND)).toBeInTheDocument(); // still the earlier answer ...
    expect(o.getByText(/Envanter yenilenemedi; bu, önceki başarılı okumadan ve güncel olmayabilir/)).toBeInTheDocument(); // ... and it says so
    expect(r.getByText(/Araç satırlarındaki gözlemler önceki başarılı okumadan ve eski olabilir/)).toBeInTheDocument();

    delete backend.failures.list_inventory;
    backend.inventory = inventoryWith(makeRequest({ status: "absent_current" }));
    await userEvent.click(r.getByRole("button", { name: "Yeniden dene" }));
    await waitFor(() => expect(o.getByText(NOT_FOUND)).toBeInTheDocument());
    expect(o.queryByText(/yenilenemedi/)).not.toBeInTheDocument();
    expect(o.queryByText(FOUND)).not.toBeInTheDocument();
  });

  it("a failed refresh never turns an earlier 'no matching row' into a claim either", async () => {
    backend.inventory = inventoryWith();
    await open();
    const r = await region();
    expect(await (await obsText("Alfa Araç")).findByText(/eşleşen satır yok/)).toBeInTheDocument();
    backend.failures.list_inventory = { code: "DB_BUSY", message: "Veritabanı meşgul." };
    await userEvent.click(r.getByRole("button", { name: "Listeyi yenile" }));
    expect(await (await obsText("Alfa Araç")).findByText(/Envanter yenilenemedi; elinizdeki okuma eski olabilir/)).toBeInTheDocument();
  });

  it("out-of-order reads: the NEWER read wins when the older one answers last", async () => {
    const pending = deferInventory();
    await open();
    await waitFor(() => expect(pending).toHaveLength(1));
    // the "Listeyi yenile" button is disabled while a read runs, so a second read is started by a save (which re-reads the stored inventory)
    const y = await rowOf("Yeni Kayıt");
    await userEvent.selectOptions(y.getByLabelText(/^Karar/), "rejected");
    await userEvent.click(y.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ }));
    await waitFor(() => expect(pending).toHaveLength(2));
    const older = inventoryWith(makeRequest({ status: "present_current" }));
    const newer = inventoryWith(makeRequest({ status: "absent_current" }));
    await act(async () => pending[1].resolve(newer));
    await act(async () => pending[0].resolve(older)); // late
    const o = await obsText("Alfa Araç");
    expect(o.getByText(NOT_FOUND)).toBeInTheDocument();
    expect(o.queryByText(FOUND)).not.toBeInTheDocument();
  });

  it("out-of-order reads: an older answer that arrives first is not shown while the newer read is still running", async () => {
    const pending = deferInventory();
    await open();
    await waitFor(() => expect(pending).toHaveLength(1));
    const y = await rowOf("Yeni Kayıt");
    await userEvent.selectOptions(y.getByLabelText(/^Karar/), "rejected");
    await userEvent.click(y.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ }));
    await waitFor(() => expect(pending).toHaveLength(2));
    await act(async () => pending[0].resolve(inventoryWith(makeRequest({ status: "present_current" })))); // superseded
    const o = await obsText("Alfa Araç");
    expect(o.queryByText(FOUND)).not.toBeInTheDocument();
    expect(o.getByText(/Kayıtlı envanter okunuyor/)).toBeInTheDocument();
    await act(async () => pending[1].resolve(inventoryWith(makeRequest({ status: "unknown_current" }))));
    expect(await o.findByText(UNDETERMINED)).toBeInTheDocument();
  });

  it("a late failure of an older read does not mark a newer good read as failed", async () => {
    const pending = deferInventory();
    await open();
    await waitFor(() => expect(pending).toHaveLength(1));
    const y = await rowOf("Yeni Kayıt");
    await userEvent.selectOptions(y.getByLabelText(/^Karar/), "rejected");
    await userEvent.click(y.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ }));
    await waitFor(() => expect(pending).toHaveLength(2));
    await act(async () => pending[1].resolve(inventoryWith(makeRequest({ status: "present_current" }))));
    await act(async () => pending[0].reject({ code: "DB_BUSY", message: "Geç gelen hata." }));
    const r = await region();
    expect(r.queryByText(/Geç gelen hata/)).not.toBeInTheDocument();
    expect((await obsText("Alfa Araç")).getByText(FOUND)).toBeInTheDocument();
    expect((await obsText("Alfa Araç")).queryByText(/yenilenemedi/)).not.toBeInTheDocument();
  });
});

describe("saving or resetting a wish and the observation stay consistent", () => {
  it("after a save the stored inventory is read again, and the new accepted wish shows the backend's row for it", async () => {
    await open();
    const y = await rowOf("Yeni Kayıt");
    expect((await obsText("Yeni Kayıt")).getByText(/kabul edilmiş bir isteğiniz yok/)).toBeInTheDocument();
    const original = backend.handle;
    backend.handle = async (cmd, args) => {
      const out = await original(cmd, args);
      if (cmd === "set_project_preference") backend.inventory.requests.push(makeRequest({ entryId: "yeni", entryName: "Yeni Kayıt", status: "not_scanned" })); // what the Rust side would now return
      return out;
    };
    await userEvent.selectOptions(y.getByLabelText(/^Karar/), "accepted");
    await userEvent.click(y.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ }));
    const o = await obsText("Yeni Kayıt");
    expect(await o.findByText(NOT_SCANNED, { selector: "strong" })).toBeInTheDocument();
    expect(o.getByText(/aracın olmadığı anlamına gelmez/)).toBeInTheDocument();
    const order = backend.calls.map((c) => c.cmd);
    expect(order.lastIndexOf("list_inventory")).toBeGreaterThan(order.indexOf("set_project_preference"));
    expect(calls("set_project_preference")).toHaveLength(1);
  });

  it("a fresh accepted wish that the inventory read does not know yet is explained conservatively, not filled in", async () => {
    await open();
    const y = await rowOf("Yeni Kayıt");
    await userEvent.selectOptions(y.getByLabelText(/^Karar/), "accepted");
    await userEvent.click(y.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ }));
    const o = await obsText("Yeni Kayıt");
    expect(await o.findByText(/eşleşen satır yok/)).toBeInTheDocument();
    expect(o.getByText(/Karar bu envanter okumasından sonra kaydedilmiş olabilir/)).toBeInTheDocument();
    expect(o.queryByText(ANY_STATUS)).not.toBeInTheDocument();
  });

  it("resetting the wish removes the comparison at once, even though the stored inventory still holds the old row", async () => {
    await open();
    const a = await rowOf("Alfa Araç");
    expect(await (await obsText("Alfa Araç")).findByText(FOUND)).toBeInTheDocument();
    await userEvent.click(a.getByRole("button", { name: /^Kararı sıfırla/ }));
    const o = await obsText("Alfa Araç");
    expect(await o.findByText(/kabul edilmiş bir isteğiniz yok/)).toBeInTheDocument();
    expect(o.queryByText(ANY_STATUS)).not.toBeInTheDocument();
    expect(calls("reset_project_preference")).toHaveLength(1);
    expect(calls("list_inventory").length).toBeGreaterThanOrEqual(2);
  });

  it("changing an accepted wish to 'rejected' drops the comparison too (the wish decides, not the cached row)", async () => {
    await open();
    const a = await rowOf("Alfa Araç");
    await within(await obsOf("Alfa Araç")).findByText(FOUND);
    await userEvent.selectOptions(a.getByLabelText(/^Karar/), "rejected");
    await userEvent.click(a.getByRole("button", { name: /^Yalnızca tercihimi kaydet/ }));
    expect(await (await obsText("Alfa Araç")).findByText(/kabul edilmiş bir isteğiniz yok/)).toBeInTheDocument();
    expect(calls("set_project_preference")[0].args).toMatchObject({ decision: "rejected" });
  });
});

describe("filters, refresh and the Recommendations handoff are unaffected", () => {
  it("a search filter persists across a refresh and the observation behind it updates", async () => {
    await open();
    const r = await region();
    await userEvent.type(r.getByLabelText("Araç ara"), "alfa");
    await waitFor(() => expect(r.getAllByRole("heading", { level: 3 }).map((h) => h.textContent)).toEqual(["Alfa Araç"]));
    expect(await (await obsText("Alfa Araç")).findByText(FOUND)).toBeInTheDocument();
    backend.inventory = inventoryWith(makeRequest({ status: "absent_current" }));
    await userEvent.click(r.getByRole("button", { name: "Listeyi yenile" }));
    await waitFor(async () => expect((await obsText("Alfa Araç")).getByText(NOT_FOUND)).toBeInTheDocument());
    expect(r.getByLabelText("Araç ara")).toHaveValue("alfa");
    expect(r.getAllByRole("heading", { level: 3 }).map((h) => h.textContent)).toEqual(["Alfa Araç"]);
  });

  const handoff = (over: Partial<MatrixHandoff> = {}): MatrixHandoff => ({ alias: ORNEK, platform: WIN, surface: CC, entryId: "alfa", nonce: 1, ...over });

  it("the handed-over row keeps focus, its mark and its note, and shows wish and observation side by side - with no decision made", async () => {
    await open(handoff());
    const article = (await rowOf("Alfa Araç")) && (document.querySelector(".ws-spot article") as HTMLElement);
    await waitFor(() => expect(document.querySelector(".ws-spot article")).toHaveFocus());
    const a = within(article);
    expect(a.getByText("Öneriden gelen araç")).toBeInTheDocument();
    expect(a.getByText("Bu projede kullanmak istiyorum", { selector: "strong" })).toBeInTheDocument();
    expect(await within(a.getByRole("region", { name: /^Son kayıtlı envanter gözlemi/ })).findByText(FOUND)).toBeInTheDocument();
    expect(await (await region()).findByText(/Öneriden açıldı: Alfa Araç/)).toBeInTheDocument();
    expect(calls("set_project_preference")).toHaveLength(0);
    expect(calls("run_discovery_scan")).toHaveLength(0);
  });

  it("the handed-over row of a tool without an accepted wish shows 'no comparison', and nothing is pre-selected", async () => {
    await open(handoff({ entryId: "yeni" }));
    await waitFor(() => expect(document.querySelector(".ws-spot article")).toHaveFocus());
    const y = await rowOf("Yeni Kayıt");
    expect((await obsText("Yeni Kayıt")).getByText(/kabul edilmiş bir isteğiniz yok/)).toBeInTheDocument();
    expect(y.getByLabelText(/^Karar/)).toHaveValue("");
    expect(calls("set_project_preference")).toHaveLength(0);
  });

  it("a hiding filter is still cleared for the handed-over row, and the row still gets its observation", async () => {
    const view = render(<ProjectsPage handoff={null} />);
    await rowOf("Alfa Araç");
    const r = await region();
    await userEvent.type(r.getByLabelText("Araç ara"), "beta");
    await waitFor(() => expect(r.getAllByRole("heading", { level: 3 }).map((h) => h.textContent)).toEqual(["Beta Eklenti"]));
    view.rerender(<ProjectsPage handoff={handoff()} />);
    await waitFor(() => expect(document.querySelector(".ws-spot article")).toHaveFocus());
    expect(r.getByLabelText("Araç ara")).toHaveValue("");
    expect(await within(await obsOf("Alfa Araç")).findByText(FOUND)).toBeInTheDocument();
    expect(await r.findByText(/Araç ve karar süzgeçleri bu aracı gizlediği için temizlendi/)).toBeInTheDocument();
  });

  it("focus does not wait for the inventory, and a failing inventory does not block the handoff", async () => {
    const pending = deferInventory();
    await open(handoff());
    await waitFor(() => expect(document.querySelector(".ws-spot article")).toHaveFocus());
    expect((await obsText("Alfa Araç")).getByText(/Kayıtlı envanter okunuyor/)).toBeInTheDocument();
    await act(async () => pending[0].reject({ code: "DB_BUSY", message: "Veritabanı meşgul." }));
    expect(await (await obsText("Alfa Araç")).findByText(/Kayıtlı envanter okunamadı/)).toBeInTheDocument();
    expect(document.querySelector(".ws-spot article")).toHaveFocus();
    expect(document.querySelectorAll(".ws-spot")).toHaveLength(1);
  });

  it("a removed or unknown handed-over tool still gets the existing note and no observation", async () => {
    await open(handoff({ entryId: "eski" }));
    expect(await (await region()).findByText(/Önerilen araç satırı vurgulanamadı|Öneriden açıldı: Eski Kayıt/)).toBeInTheDocument();
    expect(within(await obsOf("Eski Kayıt")).queryByText(ANY_STATUS)).not.toBeInTheDocument();
  });
});

describe("privacy: the matrix shows the backend's classification and nothing underneath it", () => {
  it("no location, path, evidence text or version from the raw observations ever reaches the matrix", async () => {
    backend.inventory = {
      ...inventoryWith(makeRequest({ status: "present_current" })),
      observations: [obs({ subject: "alfa", platformTarget: WIN, agentSurface: CC, location: "C:\\Users\\gizli-kisi\\bin\\alfa.exe", evidence: "GIZLI-KANIT-METNI", version: "9.9.9-gizli" })],
    };
    await open();
    await (await obsText("Alfa Araç")).findByText(FOUND);
    const text = (await region()).getByRole("heading", { level: 2, name: "Proje Araç Matrisi" }).closest("section")!.textContent ?? "";
    for (const secret of ["gizli-kisi", "alfa.exe", "GIZLI-KANIT-METNI", "9.9.9-gizli", "C:\\Users"]) expect(text.includes(secret), secret).toBe(false);
  });

  it("hostile text in a note is rendered as plain text", async () => {
    backend.inventory = inventoryWith(makeRequest({ status: "present_current", note: "<img src=x onerror=alert(1)> <b>kalın</b>" }));
    await open();
    expect(await (await obsText("Alfa Araç")).findByText(/<img src=x onerror=alert\(1\)>/)).toBeInTheDocument();
    expect(document.querySelector("img")).toBeNull();
    expect(document.querySelector("article b")).toBeNull();
  });
});

describe("safety: a stored read, never a scan", () => {
  it("opening, navigating, filtering and refreshing read the stored inventory only - no scan, no install, no filesystem, no network, no window", async () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal("fetch", fetchSpy);
    const openSpy = vi.spyOn(window, "open").mockImplementation(() => null);
    await open();
    const r = await region();
    await (await obsText("Alfa Araç")).findByText(FOUND);
    expect(calls("list_inventory")).toHaveLength(1); // opening reads once
    await userEvent.selectOptions(r.getByLabelText("Proje"), BENIM);
    await userEvent.selectOptions(r.getByLabelText("İşletim sistemi"), MAC);
    await userEvent.selectOptions(r.getByLabelText("Ajan yüzeyi"), CLI);
    await userEvent.type(r.getByLabelText("Araç ara"), "alfa");
    await userEvent.selectOptions(r.getByLabelText("Karar durumu"), "undecided");
    expect(calls("list_inventory")).toHaveLength(1); // navigating and filtering read nothing more
    await userEvent.click(r.getByRole("button", { name: "Listeyi yenile" }));
    await waitFor(() => expect(calls("list_inventory")).toHaveLength(2)); // a refresh re-reads the stored data once

    const used = new Set(backend.calls.map((c) => c.cmd));
    for (const cmd of used) expect(["list_projects", "list_catalog", "list_inventory", "set_project_preference", "reset_project_preference"], cmd).toContain(cmd);
    for (const cmd of ["run_discovery_scan", "prepare_install", "confirm_install", "cancel_install", "recover_installations", "plan_install", "get_installed_details", "list_install_states", "preview_backup_cleanup", "evaluate_task", "set_setting", "set_startup_enabled", "set_catalog_entry_enabled", "add_catalog_entry", "preview_github_url"]) {
      expect(used.has(cmd), cmd).toBe(false);
    }
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(openSpy).not.toHaveBeenCalled();
    expect(document.querySelector("a[href]")).toBeNull();
    expect(screen.queryByRole("button", { name: /tara\b|taramayı başlat|yeniden tara/i })).not.toBeInTheDocument(); // no scan control in the matrix
  });

  it("the new code names no scan, network, file, shell, window, storage or plugin API, and has no write path of its own", () => {
    for (const [label, text] of [["observation.ts", observationLogicSource], ["ObservationSection.tsx", observationComponentSource]] as const) {
      for (const banned of ["fetch(", "XMLHttpRequest", "WebSocket", "window.open", "navigator.sendBeacon", "EventSource", "localStorage", "sessionStorage", "indexedDB", "document.cookie", "@tauri-apps/plugin", "@tauri-apps/api/core", "invoke(", "child_process", "node:fs", "installMock", "dangerouslySetInnerHTML", "href=", "runDiscoveryScan", "run_discovery_scan", "api."]) {
        expect(text.includes(banned), `${label}: ${banned}`).toBe(false);
      }
      expect(/<button|onClick|onSubmit/.test(text), `${label}: no control`).toBe(false);
    }
  });

  it("Projects.tsx and the workspace use exactly one inventory call, listInventory, and never the scan", async () => {
    const projectsSource = (await import("./pages/Projects.tsx?raw")).default;
    const workspaceSource = (await import("./components/ProjectToolWorkspace.tsx?raw")).default;
    expect(projectsSource).toMatch(/useLoad\(api\.listInventory\)/);
    for (const text of [projectsSource, workspaceSource]) {
      expect(/runDiscoveryScan|run_discovery_scan|pickProjectFolder|api\.(prepare|confirm|plan)/.test(text)).toBe(false);
    }
    const used = [...new Set([...workspaceSource.matchAll(/\bapi\.([A-Za-z]+)/g)].map((m) => m[1]))].sort();
    expect(used).toEqual(["resetProjectPreference", "setProjectPreference"]); // the workspace itself still has only the two wish commands
  });
});
