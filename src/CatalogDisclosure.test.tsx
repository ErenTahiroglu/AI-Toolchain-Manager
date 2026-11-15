// CATALOG-UNSLOP-01 R1 - the Catalog card must SHOW the security and privacy statements that a record carries (privilegeClaims, networkClaims,
// privacy.notes, privacy.telemetry), not only keep them in metadata. The fixture is the REAL CATALOG-05 record read from the data file (no
// hand-written warning text), mocked IPC and jsdom only: no real window, no real database, nothing installed, fetched or run.
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import candidateSource from "../data/catalog-candidates-c05.json?raw";
import catalogSource from "./pages/Catalog.tsx?raw";
import { CatalogPage } from "./pages/Catalog";
import { createBackend, makeEntry, type Backend } from "./test/backend";
import { toolState } from "./test/installMock";
import type { CatalogEntry, ToolRecord } from "./types";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const FORBIDDEN = ["run_discovery_scan", "evaluate_task", "set_project_preference", "reset_project_preference", "prepare_install", "confirm_install", "cancel_install", "recover_installations", "plan_install", "get_installed_details", "preview_backup_cleanup", "set_setting", "set_startup_enabled", "set_catalog_entry_enabled", "add_catalog_entry", "remove_catalog_entry", "restore_catalog_entry", "preview_github_url", "add_project_reference", "remove_project_reference"];

const wrapper = JSON.parse(candidateSource).records[0] as { entryKind: CatalogEntry["kind"]; purpose: string; executionPolicy: CatalogEntry["executionPolicy"]; record: ToolRecord & { toolId: string; displayName: string; license: { spdxId: string } } };
const rec = wrapper.record;
const unslop = (record: unknown = rec, over: Partial<CatalogEntry> = {}) =>
  makeEntry(rec.toolId, { displayName: rec.displayName, kind: wrapper.entryKind, purpose: wrapper.purpose, executionPolicy: wrapper.executionPolicy, reviewState: "unreviewed", licenseSpdx: rec.license.spdxId, record: record as ToolRecord, ...over });

const PASSIVE = "Bu güvenlik bilgileri aracın olası kullanım risklerini açıklar. Katalog kaydını görüntülemek kurulum veya çalıştırma başlatmaz. Mevcut kurulum durumunu Kurulum bölümünden kontrol edin.";
const UNKNOWN_TELEMETRY = "bilinmiyor (kayıt bir şey belirtmiyor)";
const HEADING = "Güvenlik ve gizlilik açıklamaları";
/** any sentence that would deny a past installation or run (the old wording: "kurmadı ve çalıştırmadı") */
const DENIAL = /kurmadı|çalıştırmadı|hiç kurulmadı|kurulu değil|bilgisayarda yok/i;

const show = async (entry: CatalogEntry) => {
  backend.entries = [entry];
  render(<CatalogPage />);
  return screen.findByRole("heading", { level: 2, name: entry.displayName });
};
const card = async (entry: CatalogEntry) => within((await show(entry)).closest("section") as HTMLElement);
/** the single record string that matches; fails loudly if the data file ever loses it */
const claim = (list: string[] | undefined, re: RegExp) => {
  const hit = (list ?? []).filter((s) => re.test(s));
  expect(hit.length, `record has no string for ${re}`).toBeGreaterThan(0);
  return hit[0];
};

beforeEach(() => {
  backend = createBackend();
  vi.stubGlobal("fetch", vi.fn());
});

describe("the fixture is the real candidate record", () => {
  it("is Unslop with the safe states and unknown telemetry", () => {
    expect(rec.toolId).toBe("heydamir-unslop");
    expect(wrapper.executionPolicy).toBe("blocked-until-scoped-authorization");
    expect(rec.privacy?.telemetry).toBe("unknown");
    expect(rec.privacy?.mayTransmitProjectContent).toBe(true);
  });
});

describe("default state: a visible heading and summary, details still folded", () => {
  it("shows the heading, telemetry unknown and possible project-content transmission without opening anything", async () => {
    const c = await card(unslop());
    expect(c.getByRole("heading", { level: 3, name: "Güvenlik ve gizlilik açıklamaları" })).toBeInTheDocument();
    expect(c.getByText("Telemetri:").parentElement).toHaveTextContent("bilinmiyor (kayıt bir şey belirtmiyor)");
    expect(c.getByText("Proje içeriği iletilebilir mi:").parentElement).toHaveTextContent("evet, olabilir");
    const summary = c.getByText(/Yetki, ağ ve gizlilik ayrıntıları \(\d+\)/);
    expect(summary.tagName).toBe("SUMMARY");
    expect(summary.closest("details")).not.toHaveAttribute("open");
  });

  it("describes passive viewing, not an installation history", async () => {
    const c = await card(unslop());
    // the pointer sentence appears together with the install panel (it needs the installation state), so wait for the panel, not for luck
    await waitFor(() => expect(c.getByRole("group", { name: `Kurulum: ${rec.toolId}` })).toBeInTheDocument());
    const box = c.getByRole("heading", { level: 3, name: HEADING }).parentElement as HTMLElement;
    expect(within(box).getByText(PASSIVE)).toBeInTheDocument();
    expect(within(box).queryByText(DENIAL)).toBeNull();
  });
});

describe("expanded state: every required disclosure is discoverable, taken from the record itself", () => {
  it("shows the four groups and the seven disclosure categories", async () => {
    const c = await card(unslop());
    await userEvent.click(c.getByText(/Yetki, ağ ve gizlilik ayrıntıları/));
    expect(c.getByText(/Yetki, ağ ve gizlilik ayrıntıları/).closest("details")).toHaveAttribute("open");
    for (const g of ["Yetkiler: dosya, süreç ve tarayıcı davranışı", "Ağ erişimi ve dış hizmetler", "Gizlilik ve proje içeriğinin iletilmesi"]) {
      expect(c.getByRole("region", { name: g })).toBeInTheDocument();
      expect(c.getByRole("heading", { level: 4, name: g })).toBeInTheDocument();
    }
    const both = [...(rec.privilegeClaims ?? []), ...(rec.networkClaims ?? []), ...(rec.privacy?.notes ?? [])];
    const wanted: [string, string][] = [
      ["reads and modifies project source files", claim(rec.privilegeClaims, /edits the project's front-end source files/)],
      ["Playwright and Chromium run", claim(rec.privilegeClaims, /Chromium through Playwright/)],
      ["visits sites and fetches their resources", claim(rec.networkClaims, /fetches its style sheets, images and fonts/)],
      ["starts a loopback server", claim(rec.privilegeClaims, /127\.0\.0\.1/)],
      ["runs ffmpeg", claim(rec.privilegeClaims, /ffmpeg/)],
      ["content goes through the host and its provider", claim(rec.privacy?.notes, /model provider/)],
      ["optional paid image services", claim(rec.networkClaims, /image service[\s\S]*paid generation/)],
    ];
    for (const [what, text] of wanted) {
      expect(both, what).toContain(text);
      expect(c.getByText(text), what).toBeInTheDocument();
    }
    // telemetry is stated in the always-visible summary and in the record's own network claim
    expect(c.getByText(claim(rec.networkClaims, /no telemetry statement/))).toBeInTheDocument();
  });

  it("the folded part is a native details/summary: reachable in the keyboard focus order and toggles", async () => {
    const c = await card(unslop());
    const summary = c.getByText(/Yetki, ağ ve gizlilik ayrıntıları/);
    expect(summary.tagName).toBe("SUMMARY");
    expect(summary).not.toHaveAttribute("tabindex", "-1");
    await userEvent.tab();
    // walk the focus order to this summary: the page only uses native elements, so it is reachable
    for (let i = 0; i < 40 && document.activeElement !== summary; i++) await userEvent.tab();
    expect(document.activeElement).toBe(summary);
    // jsdom does not implement the browser's Enter/Space activation of <summary>, so only the native element and its focus order are
    // checked here; the toggle itself is exercised by a click. Real-browser keyboard toggling is native behaviour and is NOT tested here.
    await userEvent.click(summary);
    expect(summary.closest("details")).toHaveAttribute("open");
    await userEvent.click(summary);
    expect(summary.closest("details")).not.toHaveAttribute("open");
  });
});

describe("the disclosure is passive and creates no action", () => {
  it("adds no button, link, input or form inside the disclosure, and sends no write or install command", async () => {
    const c = await card(unslop());
    await userEvent.click(c.getByText(/Yetki, ağ ve gizlilik ayrıntıları/));
    const box = c.getByRole("heading", { level: 3, name: "Güvenlik ve gizlilik açıklamaları" }).parentElement as HTMLElement;
    expect(box.querySelectorAll("button, a, input, select, textarea, form, [role=button], [onclick]").length).toBe(0);
    expect(backend.calls.filter((x) => FORBIDDEN.includes(x.cmd))).toEqual([]);
    expect(screen.queryByRole("button", { name: /kur$|install|run/i })).toBeNull();
  });

  it("keeps the existing controls, the unavailable run button and the compatibility section", async () => {
    const c = await card(unslop());
    expect(c.getByRole("checkbox", { name: /Katalogda etkin/ })).toBeInTheDocument();
    expect(c.getByRole("button", { name: "Katalogdan kaldır" })).toBeEnabled();
    const run = c.getByRole("button", { name: /Kur \/ çalıştır/ });
    expect(run).toHaveAttribute("aria-disabled", "true");
    const compat = c.getByText(/Platform ve ajan uyumluluğu \(6\)/);
    await userEvent.click(compat);
    expect(c.getAllByRole("row").length).toBe(7);
    expect(c.getByText(/Sınırlamalar ve bilinmeyenler/)).toBeInTheDocument();
  });
});

describe("untrusted and incomplete metadata", () => {
  it("renders hostile text as inert text and creates no element, link or handler from it", async () => {
    const hostile = { ...rec, privilegeClaims: ["<img src=x onerror=alert(1)>", "[tıkla](https://evil.example/x)", "javascript:alert(1)"], networkClaims: ["<script>alert(2)</script>"], privacy: { telemetry: "<b>x</b>", mayTransmitProjectContent: null, notes: ["`rm -rf /`"] } };
    const c = await card(unslop(hostile));
    await userEvent.click(c.getByText(/Yetki, ağ ve gizlilik ayrıntıları/));
    expect(c.getByText("<img src=x onerror=alert(1)>")).toBeInTheDocument();
    expect(c.getByText("[tıkla](https://evil.example/x)")).toBeInTheDocument();
    expect(c.getByText("`rm -rf /`")).toBeInTheDocument();
    const box = c.getByRole("heading", { level: 3, name: "Güvenlik ve gizlilik açıklamaları" }).parentElement as HTMLElement;
    expect(box.querySelector("img, script, a, b")).toBeNull();
    expect(box).toHaveTextContent(UNKNOWN_TELEMETRY);
    expect(c.getByText("Proje içeriği iletilebilir mi:").parentElement).toHaveTextContent("bilinmiyor");
    expect(catalogSource).not.toMatch(/dangerouslySetInnerHTML|innerHTML|eval\(|new Function/);
  });

  it.each([
    ["no privacy and no claims", { targetEvidence: [] }],
    ["empty lists", { privilegeClaims: [], networkClaims: [], dependencies: [], privacy: { notes: [] } }],
    ["wrong types", { privilegeClaims: "x", networkClaims: [1, null, {}, ""], dependencies: 5, privacy: "x" }],
  ])("%s: no crash and no empty disclosure heading", async (_name, record) => {
    const c = await card(unslop(record));
    expect(c.queryByRole("heading", { level: 3, name: "Güvenlik ve gizlilik açıklamaları" })).toBeNull();
    expect(c.getByRole("checkbox", { name: /Katalogda etkin/ })).toBeInTheDocument();
  });

  it("a record with only a telemetry value still shows a labelled summary and no empty details", async () => {
    const c = await card(unslop({ privacy: { telemetry: "none-declared", mayTransmitProjectContent: false } }));
    expect(c.getByRole("heading", { level: 3, name: "Güvenlik ve gizlilik açıklamaları" })).toBeInTheDocument();
    expect(c.getByText("Telemetri:").parentElement).toHaveTextContent("yayıncı telemetri beyan etmiyor");
    expect(c.getByText("Proje içeriği iletilebilir mi:").parentElement).toHaveTextContent("hayır (yayıncı beyanı)");
    expect(c.queryByText(/Yetki, ağ ve gizlilik ayrıntıları/)).toBeNull();
  });

  it("a well-formed entry that names nothing else is not special-cased by id", async () => {
    expect(catalogSource).not.toMatch(/heydamir|unslop/i);
    await waitFor(() => expect(true).toBe(true));
  });
});

describe("R2 P2-A: the passive wording is true for every installation status, including an installed tool", () => {
  it.each([
    ["installed_verified", "Kurulu ve doğrulandı"],
    ["verification_incomplete", "Doğrulama tamamlanmadı"],
    ["available", "Kurulabilir"],
    ["installing", "Kuruluyor…"],
    ["failed_needs_attention", "Başarısız / dikkat gerekiyor"],
    ["removed", "Kaldırıldı"],
  ] as const)("%s: the status is shown, the disclosure stays, and nothing denies an installation", async (status, label) => {
    backend.install.states.set(rec.toolId, toolState(rec.toolId, status));
    const c = await card(unslop());
    await waitFor(() => expect(c.getAllByText(label, { exact: false }).length).toBeGreaterThan(0));
    const box = c.getByRole("heading", { level: 3, name: HEADING }).parentElement as HTMLElement;
    expect(within(box).getByText(PASSIVE)).toBeInTheDocument();
    expect(within(box).getByText("Telemetri:").parentElement).toHaveTextContent(UNKNOWN_TELEMETRY);
    expect(within(box).queryByText(DENIAL)).toBeNull();
    await userEvent.click(within(box).getByText(/Yetki, ağ ve gizlilik ayrıntıları/));
    expect(within(box).getByText(claim(rec.privilegeClaims, /Chromium through Playwright/))).toBeInTheDocument();
    expect(backend.calls.filter((x) => FORBIDDEN.includes(x.cmd))).toEqual([]);
  });
});

describe("R2 P2-B: telemetry lookup is safe for every inherited-property key and every malformed value", () => {
  const LABELS: [string, string][] = [
    ["unknown", UNKNOWN_TELEMETRY],
    ["none-declared", "yayıncı telemetri beyan etmiyor"],
    ["opt-in", "yayıncıya göre isteğe bağlı (açık onayla)"],
    ["opt-out", "yayıncıya göre varsayılan olarak açık"],
  ];
  /** parsed from JSON text, as a real record arrives: "__proto__" becomes an OWN key here, which an object literal could not express */
  const withTelemetry = (json: string) => JSON.parse(`{"privilegeClaims":["Bir yetki ifadesi"],"privacy":{"telemetry":${json},"mayTransmitProjectContent":true,"notes":["Bir not"]}}`);
  let errors: ReturnType<typeof vi.spyOn>;
  beforeEach(() => { errors = vi.spyOn(console, "error").mockImplementation(() => {}); });
  afterEach(() => { expect(errors).not.toHaveBeenCalled(); errors.mockRestore(); });

  it.each(LABELS)("the supported value %s gets its own label", async (value, label) => {
    const c = await card(unslop(withTelemetry(JSON.stringify(value))));
    expect(c.getByText("Telemetri:").parentElement).toHaveTextContent(label);
  });

  it.each([
    ['"__proto__"'], ['"constructor"'], ['"prototype"'], ['"toString"'], ['"valueOf"'], ['"hasOwnProperty"'], ['"isPrototypeOf"'], ['"__defineGetter__"'],
    ['"bilinmeyen-değer"'], ['""'], ['"UNKNOWN"'], ["null"], ["42"], ["true"], ["[]"], ['["opt-in"]'], ["{}"], ['{"unknown":1}'], ['{"__proto__":{}}'],
  ])("telemetry %s: the page keeps rendering, the disclosure stays and the label is the unknown label", async (json) => {
    const c = await card(unslop(withTelemetry(json)));
    await waitFor(() => expect(c.getByRole("group", { name: `Kurulum: ${rec.toolId}` })).toBeInTheDocument());
    const box = c.getByRole("heading", { level: 3, name: HEADING }).parentElement as HTMLElement;
    expect(within(box).getByText("Telemetri:").parentElement).toHaveTextContent(`Telemetri: ${UNKNOWN_TELEMETRY}.`);
    expect(within(box).getByText(PASSIVE)).toBeInTheDocument();
    await userEvent.click(within(box).getByText(/Yetki, ağ ve gizlilik ayrıntıları/));
    expect(within(box).getByText("Bir yetki ifadesi")).toBeInTheDocument();
    expect(c.getByRole("checkbox", { name: /Katalogda etkin/ })).toBeInTheDocument();
    expect(box.querySelectorAll("button, a, input, select, textarea, form, img, script").length).toBe(0);
    expect(backend.calls.filter((x) => FORBIDDEN.includes(x.cmd))).toEqual([]);
  });

  it.each([['"x"'], ["[]"], ["null"], ["7"]])("a malformed privacy block %s does not crash and does not invent a telemetry value", async (privacy) => {
    const c = await card(unslop(JSON.parse(`{"privilegeClaims":["Bir yetki ifadesi"],"privacy":${privacy}}`)));
    expect(c.getByRole("heading", { level: 3, name: HEADING })).toBeInTheDocument();
    expect(c.getByText("Telemetri:").parentElement).toHaveTextContent(UNKNOWN_TELEMETRY);
  });

  it("an inherited (not own) telemetry value is ignored", async () => {
    const inherited = Object.create({ telemetry: "opt-in", mayTransmitProjectContent: false });
    const c = await card(unslop({ privilegeClaims: ["Bir yetki ifadesi"], privacy: inherited }));
    expect(c.getByText("Telemetri:").parentElement).toHaveTextContent(UNKNOWN_TELEMETRY);
    expect(c.getByText("Proje içeriği iletilebilir mi:").parentElement).toHaveTextContent("bilinmiyor");
  });

  it("the source has no object-literal label table that a key could index", () => {
    expect(catalogSource).not.toMatch(/TELEMETRY_LABEL\[/);
  });
});

// CATALOG-UX-P3 (#57): the pointer to the "Kurulum" section must exist exactly when that section is on the card.
// `entry.removedAt !== null` = the CATALOG ENTRY was removed (the install panel is not rendered); `install.status === "removed"` = an INSTALLATION
// was removed (the entry is still live and its install panel is shown). The two must never be confused.
describe("#57: the installation pointer follows the installation panel", () => {
  const GUIDE = "Mevcut kurulum durumunu Kurulum bölümünden kontrol edin.";
  const PASSIVE_CORE = "Bu güvenlik bilgileri aracın olası kullanım risklerini açıklar. Katalog kaydını görüntülemek kurulum veya çalıştırma başlatmaz.";
  const REMOVED_AT = "2026-11-01T10:00:00Z";
  const disclosureOf = (c: ReturnType<typeof within>) => c.getByRole("heading", { level: 3, name: HEADING }).parentElement as HTMLElement;
  const installPanel = (c: ReturnType<typeof within>) => c.queryByRole("group", { name: `Kurulum: ${rec.toolId}` });
  /** the fixture entry rendered under the "removed entries" section, as the page really does it */
  const removedCard = async (over: Partial<CatalogEntry> = {}) => {
    backend.entries = [unslop(rec, { removedAt: REMOVED_AT, enabled: false, ...over })];
    render(<CatalogPage />);
    const section = (await screen.findByRole("heading", { level: 2, name: /Katalogdan kaldırılanlar \(1\)/ })).closest("section") as HTMLElement;
    return within(within(section).getByRole("heading", { level: 2, name: rec.displayName }).closest("section") as HTMLElement);
  };
  const expectDisclosureComplete = async (c: ReturnType<typeof within>) => {
    const box = disclosureOf(c);
    expect(within(box).getByText("Telemetri:").parentElement).toHaveTextContent(UNKNOWN_TELEMETRY);
    expect(within(box).getByText("Proje içeriği iletilebilir mi:").parentElement).toHaveTextContent("evet, olabilir");
    const summary = within(box).getByText(/Yetki, ağ ve gizlilik ayrıntıları/);
    expect(summary.tagName).toBe("SUMMARY");
    expect(summary.closest("details")).not.toHaveAttribute("open");
    await userEvent.click(summary);
    expect(summary.closest("details")).toHaveAttribute("open");
    for (const g of ["Yetkiler: dosya, süreç ve tarayıcı davranışı", "Ağ erişimi ve dış hizmetler", "Gizlilik ve proje içeriğinin iletilmesi", "Gerekli yan araçlar (bu uygulama kurmaz)"]) {
      expect(within(box).getByRole("region", { name: g })).toBeInTheDocument();
    }
    for (const text of [claim(rec.privilegeClaims, /edits the project's front-end source files/), claim(rec.privilegeClaims, /Chromium through Playwright/), claim(rec.privilegeClaims, /127\.0\.0\.1/), claim(rec.privilegeClaims, /ffmpeg/), claim(rec.networkClaims, /fetches its style sheets/), claim(rec.networkClaims, /paid generation/), claim(rec.privacy?.notes, /model provider/), ...(rec.dependencies ?? [])]) {
      expect(within(box).getByText(text)).toBeInTheDocument();
    }
    expect(within(box).queryByText(DENIAL)).toBeNull();
    expect(box.querySelectorAll("button, a, input, select, textarea, form, img, script, [onclick]").length).toBe(0);
    expect(backend.calls.filter((x) => FORBIDDEN.includes(x.cmd))).toEqual([]);
  };

  it("A: a normal catalog card shows its install panel, the pointer to it, the passive statement and the full disclosure", async () => {
    const c = await card(unslop());
    await waitFor(() => expect(installPanel(c)).not.toBeNull());
    expect(c.getByRole("heading", { level: 3, name: "Kurulum" })).toBeInTheDocument();
    expect(within(disclosureOf(c)).getByText(PASSIVE)).toBeInTheDocument();
    expect(disclosureOf(c)).toHaveTextContent(GUIDE);
    expect(c.queryByText("Katalogdan kaldırıldı")).toBeNull();
    await expectDisclosureComplete(c);
  });

  it("B: a removed catalog card sits under the removed section with no install panel, no pointer to one, and the full disclosure", async () => {
    const c = await removedCard();
    expect(c.getByText("Katalogdan kaldırıldı")).toBeInTheDocument();
    expect(installPanel(c)).toBeNull();
    expect(c.queryByRole("heading", { level: 3, name: "Kurulum" })).toBeNull();
    const box = disclosureOf(c);
    expect(within(box).getByText(PASSIVE_CORE)).toBeInTheDocument();
    expect(box).not.toHaveTextContent("Kurulum bölümünden");
    expect(box).not.toHaveTextContent(GUIDE);
    expect(c.getByRole("button", { name: "Geri ekle (kapalı olarak)" })).toBeEnabled();
    expect(c.queryByRole("checkbox", { name: /Katalogda etkin/ })).toBeNull();
    await expectDisclosureComplete(c);
  });

  it("B2: the removed card says nothing about a Kurulum section even when an installation exists for the entry", async () => {
    backend.install.states.set(rec.toolId, toolState(rec.toolId, "installed_verified"));
    const c = await removedCard();
    expect(installPanel(c)).toBeNull();
    expect(disclosureOf(c)).not.toHaveTextContent("Kurulum bölümünden");
    expect(within(disclosureOf(c)).getByText(PASSIVE_CORE)).toBeInTheDocument();
  });

  it("C: a live card with an installed_verified installation shows that status, its panel, the pointer and no denial", async () => {
    backend.install.states.set(rec.toolId, toolState(rec.toolId, "installed_verified"));
    const c = await card(unslop());
    await waitFor(() => expect(c.getAllByText("Kurulu ve doğrulandı", { exact: false }).length).toBeGreaterThan(0));
    expect(installPanel(c)).not.toBeNull();
    expect(within(disclosureOf(c)).getByText(PASSIVE)).toBeInTheDocument();
    expect(disclosureOf(c)).not.toHaveTextContent(DENIAL);
    await expectDisclosureComplete(c);
  });

  it("D: an installation with status \"removed\" is not a removed catalog entry: the card stays live with its panel and its pointer", async () => {
    backend.install.states.set(rec.toolId, toolState(rec.toolId, "removed"));
    const entry = unslop();
    expect(entry.removedAt).toBeNull();
    const c = await card(entry);
    await waitFor(() => expect(c.getAllByText("Kaldırıldı", { exact: false }).length).toBeGreaterThan(0));
    expect(installPanel(c)).not.toBeNull();
    expect(screen.queryByRole("heading", { level: 2, name: /Katalogdan kaldırılanlar/ })).toBeNull();
    expect(c.queryByText("Katalogdan kaldırıldı")).toBeNull();
    expect(c.getByRole("checkbox", { name: /Katalogda etkin/ })).toBeInTheDocument();
    expect(within(disclosureOf(c)).getByText(PASSIVE)).toBeInTheDocument();
    expect(disclosureOf(c)).toHaveTextContent(GUIDE);
  });

  it("E: changing only the catalog-removal flag changes nothing in the risk information, only the pointer sentence", async () => {
    const live = await card(unslop());
    await waitFor(() => expect(installPanel(live)).not.toBeNull());
    const liveBox = disclosureOf(live);
    await userEvent.click(within(liveBox).getByText(/Yetki, ağ ve gizlilik ayrıntıları/));
    const liveText = (liveBox.textContent ?? "").replace(` ${GUIDE}`, "").replace(/\s+/g, " ").trim();
    document.body.innerHTML = "";
    const gone = await removedCard();
    const goneBox = disclosureOf(gone);
    await userEvent.click(within(goneBox).getByText(/Yetki, ağ ve gizlilik ayrıntıları/));
    expect((goneBox.textContent ?? "").replace(/\s+/g, " ").trim()).toBe(liveText.replace(/\s+/g, " ").trim().replace(/\s{2,}/g, " "));
  });
});

// CATALOG-UX-P3 R2: InstallPanel renders nothing without a state object, so the pointer needs `!removed && !!install`. An entry can be live and
// still have no state: the list was read while the entry was removed (the list holds live entries only) and a restore does not re-read it.
describe("#57 R2: no pointer to an install panel that cannot render", () => {
  const GUIDE = "Mevcut kurulum durumunu Kurulum bölümünden kontrol edin.";
  const PASSIVE_CORE = "Bu güvenlik bilgileri aracın olası kullanım risklerini açıklar. Katalog kaydını görüntülemek kurulum veya çalıştırma başlatmaz.";
  const REMOVED_AT = "2026-11-01T10:00:00Z";
  const panel = (c: ReturnType<typeof within>) => c.queryByRole("group", { name: `Kurulum: ${rec.toolId}` });
  const boxOf = (c: ReturnType<typeof within>) => c.getByRole("heading", { level: 3, name: HEADING }).parentElement as HTMLElement;
  const installReads = () => backend.calls.filter((x) => x.cmd === "list_install_states").length;
  /** the IPC answer for the installation-state list is empty: a COMPLETED read with no matching entry (not a loading state) */
  const noInstallStates = () => {
    const original = backend.handle.bind(backend);
    backend.handle = async (cmd, args) => (cmd === "list_install_states" ? (backend.calls.push({ cmd, args }), []) : original(cmd, args));
  };
  /** wait until the read has really finished and React applied it: a call was made AND a following tick has run */
  const settled = async () => {
    await waitFor(() => expect(installReads()).toBeGreaterThan(0));
    await act(async () => { await Promise.resolve(); await new Promise((r) => setTimeout(r, 0)); });
  };
  const disclosureComplete = async (c: ReturnType<typeof within>) => {
    const box = boxOf(c);
    expect(within(box).getByText(PASSIVE_CORE)).toBeInTheDocument();
    expect(within(box).getByText("Telemetri:").parentElement).toHaveTextContent(UNKNOWN_TELEMETRY);
    expect(within(box).getByText("Proje içeriği iletilebilir mi:").parentElement).toHaveTextContent("evet, olabilir");
    await userEvent.click(within(box).getByText(/Yetki, ağ ve gizlilik ayrıntıları/));
    for (const t of [claim(rec.privilegeClaims, /edits the project's front-end source files/), claim(rec.privilegeClaims, /Chromium through Playwright/), claim(rec.privilegeClaims, /ffmpeg/), claim(rec.networkClaims, /fetches its style sheets/), claim(rec.privacy?.notes, /model provider/), ...(rec.dependencies ?? [])]) {
      expect(within(box).getByText(t)).toBeInTheDocument();
    }
    expect(within(box).queryByText(DENIAL)).toBeNull();
    expect(box).not.toHaveTextContent("Kurulum bölümünden");
    expect(box.querySelectorAll("button, a, input, select, textarea, form, img, script, [onclick]").length).toBe(0);
  };
  const removedCardIn = async () => {
    render(<CatalogPage />);
    return (await screen.findByRole("heading", { level: 2, name: /Katalogdan kaldırılanlar \(1\)/ })).closest("section") as HTMLElement;
  };

  it("R2-A: a live card whose installation-state read found no entry shows no panel and no pointer, and the full disclosure", async () => {
    noInstallStates();
    const entry = unslop();
    expect(entry.removedAt).toBeNull();
    const c = await card(entry);
    await settled();
    expect(c.getByRole("checkbox", { name: /Katalogda etkin/ })).toBeInTheDocument();
    expect(c.queryByText("Katalogdan kaldırıldı")).toBeNull();
    expect(panel(c)).toBeNull();
    await disclosureComplete(c);
    expect(backend.calls.filter((x) => FORBIDDEN.includes(x.cmd))).toEqual([]);
  });

  it("R2-B: restoring a removed entry in the same mounted page leaves a live card with no panel and no pointer", async () => {
    backend.entries = [unslop(rec, { removedAt: REMOVED_AT, enabled: false })];
    const removedSection = await removedCardIn();
    await settled();
    // the list the page read holds live entries only, so it has no state for this (then removed) entry
    expect(backend.entries.filter((e) => e.removedAt === null)).toHaveLength(0);
    const reads = installReads();
    await userEvent.click(within(removedSection).getByRole("button", { name: "Geri ekle (kapalı olarak)" }));
    // same mounted page: the catalog reloads and the entry becomes a live card
    const live = await waitFor(() => {
      expect(screen.queryByRole("heading", { level: 2, name: /Katalogdan kaldırılanlar/ })).toBeNull();
      return within(screen.getByRole("heading", { level: 2, name: rec.displayName }).closest("section") as HTMLElement);
    });
    expect(backend.calls.filter((x) => x.cmd === "restore_catalog_entry")).toHaveLength(1);
    expect(installReads()).toBe(reads); // the state list was NOT re-read: the stale-state situation the reviewer found
    expect(live.getByRole("checkbox", { name: /Katalogda etkin/ })).not.toBeChecked();
    expect(live.queryByText("Katalogdan kaldırıldı")).toBeNull();
    expect(panel(live)).toBeNull();
    await disclosureComplete(live);
    expect(backend.calls.filter((x) => FORBIDDEN.includes(x.cmd) && x.cmd !== "restore_catalog_entry")).toEqual([]);
    expect(backend.calls.filter((x) => /install|run|execute|apply|confirm/i.test(x.cmd) && x.cmd !== "list_install_states")).toEqual([]);
  });

  it("R2-C: when the state IS present on a fresh page load, the panel and the pointer appear together (the page has no refresh path to test in place)", async () => {
    backend.entries = [unslop()];
    backend.install.states.set(rec.toolId, toolState(rec.toolId, "available"));
    const c = await card(backend.entries[0]);
    await waitFor(() => expect(panel(c)).not.toBeNull());
    await waitFor(() => expect(boxOf(c)).toHaveTextContent(GUIDE));
    expect(within(boxOf(c)).getByText(PASSIVE)).toBeInTheDocument();
  });

  it.each([
    ["installed_verified", "Kurulu ve doğrulandı"],
    ["removed", "Kaldırıldı"],
  ] as const)("R2-D: a live entry with installation status %s keeps its panel and its pointer", async (status, label) => {
    backend.install.states.set(rec.toolId, toolState(rec.toolId, status));
    const c = await card(unslop());
    await waitFor(() => expect(panel(c)).not.toBeNull());
    expect(c.getAllByText(label, { exact: false }).length).toBeGreaterThan(0);
    await waitFor(() => expect(boxOf(c)).toHaveTextContent(GUIDE));
    expect(within(boxOf(c)).getByText(PASSIVE)).toBeInTheDocument();
    expect(c.queryByText("Katalogdan kaldırıldı")).toBeNull();
  });

  it("R2-D: a removed catalog entry shows no panel and no pointer even if an installation state object exists for it", async () => {
    backend.install.states.set(rec.toolId, toolState(rec.toolId, "installed_verified"));
    backend.entries = [unslop(rec, { removedAt: REMOVED_AT, enabled: false })];
    const section = await removedCardIn();
    await settled();
    const c = within(within(section).getByRole("heading", { level: 2, name: rec.displayName }).closest("section") as HTMLElement);
    expect(panel(c)).toBeNull();
    await disclosureComplete(c);
  });
});
