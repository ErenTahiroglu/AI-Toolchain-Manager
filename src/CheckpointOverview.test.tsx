// ATM-GOV-02 P2C-WP2 - the project checkpoint overview inside the real App, against the in-memory backend. jsdom + user-event only: no real window, no real
// network. The native assess command is a controllable fake; nothing here can reach GitHub.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import centerSource from "./pages/CheckpointCenter.tsx?raw";
import overviewSource from "./components/ProjectCheckpointOverview.tsx?raw";
import formSource from "./claimsForm.ts?raw";
import panelSource from "./components/CiEvidencePanel.tsx?raw";
import pasteViewerSource from "./pages/Checkpoints.tsx?raw";
import pasteLogicSource from "./checkpointView.ts?raw";
import { TARGET_REPOSITORY } from "./ciAssessment";
import { parseClaimsForm, EMPTY_FORM } from "./claimsForm";
import { goodReport } from "./test/ciReport";
// vitest serves CSS imports empty, so the stylesheet is read as text; @types/node is not installed (and not added), hence the expected type error
// @ts-expect-error TS2591: no Node type definitions in this project
import { readFileSync } from "node:fs";
import { createBackend, type Backend } from "./test/backend";
import type { ProjectView } from "./types";

const SHA = "dd37b04f730569c3a8ad1b929ee6b80052ef22e2";
const OTHER_SHA = "6ae8032000000000000000000000000000000000";
const RUN = "41217731446";

let backend: Backend;
type Pending = { args: Record<string, unknown>; resolve: (v: unknown) => void; reject: (e: unknown) => void };
let assessCalls: Pending[];
let listGate: Promise<void> | null; // when set, the project list answers only after it resolves

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "assess_atm_checkpoint") return new Promise((resolve, reject) => { assessCalls.push({ args: args ?? {}, resolve, reject }); });
    if (cmd === "list_projects" && listGate) return listGate.then(() => backend.handle(cmd, args));
    return backend.handle(cmd, args);
  },
}));

const ref = (id: string, name: string, at = "2026-10-17T14:00:00Z"): ProjectView =>
  ({ projectId: id, alias: `alias-${id}`, displayName: name, kind: "reference", localNote: "", createdAt: at, preferences: [] });

let fetchSpy: ReturnType<typeof vi.fn>;
beforeEach(() => {
  backend = createBackend();
  assessCalls = [];
  listGate = null;
  fetchSpy = vi.fn();
  vi.stubGlobal("fetch", fetchSpy);
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const nav = () => screen.getByRole("navigation", { name: "Ana gezinme" });
const goCheckpoints = async () => {
  render(<App />);
  await userEvent.click(within(nav()).getByRole("button", { name: "Checkpoint Merkezi" }));
  await screen.findByRole("heading", { level: 1, name: "Checkpoint Merkezi" });
  await screen.findByRole("heading", { level: 2, name: "Proje Checkpoint Özeti" });
};
const card = (name: string) => screen.getByRole("group", { name });
const cards = () => screen.queryAllByRole("group").filter((g) => g.classList.contains("ov-card"));
// element boundaries count as spaces (a <dt> and its <dd> are separate words)
const text = (el: Element | null) => el?.innerHTML.replace(/<[^>]*>/g, " ").replace(/\s+/g, " ").replace(/ ([.,;:])/g, "$1").trim() ?? "";
const shaBox = () => screen.getByRole("textbox", { name: /Tam commit SHA/ });
const runBox = () => screen.getByRole("textbox", { name: /çalıştırma kimliği/ });
const attemptBox = () => screen.getByRole("textbox", { name: /Deneme numarası/ });
const submitButton = () => screen.getByRole("button", { name: "GitHub CI Kanıtını Denetle" });
const fill = (sha = SHA, run = RUN, attempt = "1") => {
  fireEvent.change(shaBox(), { target: { value: sha } });
  fireEvent.change(runBox(), { target: { value: run } });
  fireEvent.change(attemptBox(), { target: { value: attempt } });
};
const resolveWith = async (i: number, report: unknown) => act(async () => assessCalls[i].resolve(report));

/** Types claims into one card's session-only form and applies them. */
const claim = async (name: string, v: { repo?: string; accepted?: string; head?: string; review?: boolean; auth?: boolean; blockers?: string }) => {
  const c = within(card(name));
  if (v.repo !== undefined) fireEvent.change(c.getByLabelText(/^Depo \(sahip\/ad\)/), { target: { value: v.repo } });
  if (v.accepted !== undefined) fireEvent.change(c.getByLabelText(/^Son kabul olduğu beyan edilen commit/), { target: { value: v.accepted } });
  if (v.head !== undefined) fireEvent.change(c.getByLabelText(/^Güncel dalın ucu/), { target: { value: v.head } });
  if (v.review) await userEvent.click(c.getByLabelText(/Bağımsız inceleme yapıldığını beyan/));
  if (v.auth) await userEvent.click(c.getByLabelText(/İnsan onayı verildiğini beyan/));
  if (v.blockers !== undefined) fireEvent.change(c.getByLabelText(/^Açık engeller — beyan/), { target: { value: v.blockers } });
  await userEvent.click(c.getByRole("button", { name: "Oturuma uygula" }));
};
const ATM_NAME = "Proje ATM";

describe("rows: real projects only, one ATM row", () => {
  it("synthetic projects are not rows; the fixed ATM row stands alone when no project claims the ATM repository", async () => {
    backend.projects.push(ref("r1", "Proje X"));
    await goCheckpoints();
    expect(cards().map((c) => text(c.querySelector("h3")))).toEqual(["ATM (sabit hedef)", "Proje X"]);
    expect(document.body.textContent).not.toContain("Örnek: Web sitesi"); // the backend's synthetic project
    expect(text(document.querySelector(".ov"))).toMatch(/1 örnek \(sentetik\) proje gerçek depo olmadığı için gösterilmiyor/);
  });

  it("claiming the ATM repository on a real project replaces the fixed row; a second project cannot claim it too", async () => {
    backend.projects.push(ref("a", ATM_NAME, "2026-01-01T00:00:00Z"), ref("b", "Proje B", "2026-02-01T00:00:00Z"));
    await goCheckpoints();
    expect(cards()).toHaveLength(3); // fixed ATM row + two foreign
    await claim(ATM_NAME, { repo: TARGET_REPOSITORY });
    expect(cards().map((c) => text(c.querySelector("h3")))).toEqual([ATM_NAME, "Proje B"]); // the fixed row is gone; no second ATM card
    await claim("Proje B", { repo: TARGET_REPOSITORY.toLowerCase() });
    expect(within(card("Proje B")).getByText(/zaten “Proje ATM” projesinde beyan edilmiş/)).toBeInTheDocument();
    expect(card("Proje B")).toHaveAttribute("data-role", "FOREIGN");
    expect(cards().filter((c) => c.getAttribute("data-role") === "ATM")).toHaveLength(1);
  });

  it("an alias or display name that looks like the ATM repository does not make an ATM row", async () => {
    backend.projects.push({ ...ref("r1", TARGET_REPOSITORY), alias: "ai-toolchain-manager" });
    await goCheckpoints();
    expect(card(TARGET_REPOSITORY)).toHaveAttribute("data-role", "FOREIGN");
    expect(cards().filter((c) => c.getAttribute("data-role") === "ATM")).toHaveLength(1);
    expect(text(cards()[0].querySelector("h3"))).toBe("ATM (sabit hedef)");
  });
});

describe("sources, claims versus observations", () => {
  it("typed values carry [beyan], missing ones [bilinmiyor]; a review/authorization claim never fills the observed fields", async () => {
    backend.projects.push(ref("a", ATM_NAME));
    await goCheckpoints();
    expect(text(card(ATM_NAME))).toMatch(/Gözlenen inceleme değerlendirilmedi \[bilinmiyor\]/);
    await claim(ATM_NAME, { repo: TARGET_REPOSITORY, accepted: SHA, head: OTHER_SHA, review: true, auth: true });
    const t = text(card(ATM_NAME));
    expect(t).toContain(`Beyan edilen son kabul ${SHA} [beyan]`);
    expect(t).toContain(`Beyan edilen HEAD ${OTHER_SHA} [beyan]`);
    expect(t).toMatch(/Güncel HEAD doğrulanmadı — bilinmiyor \[bilinmiyor\]/);
    expect(t).toMatch(/Beyan edilen inceleme bağımsız inceleme yapıldığı beyan edildi — doğrulanmadı \[beyan\]/);
    expect(t).toMatch(/Beyan edilen onay insan onayı verildiği beyan edildi — doğrulanmadı \[beyan\]/);
    expect(t).toMatch(/Gözlenen inceleme değerlendirilmedi \[bilinmiyor\]/); // the claim did not change what is observed
    expect(t).toMatch(/Gözlenen onay değerlendirilmedi \[bilinmiyor\]/);
    expect(t).not.toContain("[ECE]"); // no machine observation without a user-run ECE check
    expect(t).toMatch(/Bu oturumda ECE makine gözlemi yok \[bilinmiyor\]/);
  });

  it("a historical machine pass is [ECE], stays historical, and never becomes a HEAD, review or authorization statement", async () => {
    backend.projects.push(ref("a", ATM_NAME));
    await goCheckpoints();
    await claim(ATM_NAME, { repo: TARGET_REPOSITORY, accepted: SHA, head: SHA, review: true, auth: true });
    fill();
    await userEvent.click(submitButton());
    await resolveWith(0, goodReport());
    const t = text(card(ATM_NAME));
    expect(t).toContain(`Gözlenen commit ${SHA} [ECE]`);
    expect(t).toMatch(/Makine CI sonucu İki zorunlu iş de bu denemede başarılı sonuçlandı \(makine gözlemi\) \[ECE\]/);
    expect(t).toContain("TARİHSEL KANIT — GÜNCEL HEAD ONAYI DEĞİLDİR");
    expect(t).toMatch(/Güncel HEAD doğrulanmadı — bilinmiyor/);
    expect(t).toMatch(/Gözlenen inceleme BAĞIMSIZ İNCELEME: YAPILMADI \[ECE\]/);
    expect(t).toMatch(/Gözlenen onay İNSAN ONAYI: BEKLEMEDE \[ECE\]/);
    expect(t).toMatch(/Beyan edilen inceleme .*\[beyan\]/); // the claim stays a separate, labelled claim
    expect(t).toMatch(/Devam eden eksikler.*HEAD doğrulanmadı.*Bağımsız inceleme doğrulanmadı.*İnsan onayı doğrulanmadı/);
    expect(t).toMatch(/ATM içinde önerilecek başka bir adım yok\. Bu bir kabul ya da tamamlanma değildir/);
    expect(t).not.toMatch(/Beyan edilen HEAD farklı|farklı/); // equal SHAs: no mismatch note
  });

  it("an observation about another commit than the claimed accepted one says so and does not satisfy the claim", async () => {
    backend.projects.push(ref("a", ATM_NAME));
    await goCheckpoints();
    await claim(ATM_NAME, { repo: TARGET_REPOSITORY, accepted: OTHER_SHA, head: OTHER_SHA });
    fill();
    await userEvent.click(submitButton());
    await resolveWith(0, goodReport());
    const t = text(card(ATM_NAME));
    expect(t).toContain("Gözlem, beyan edilen son kabul commit’ine bağlı değil");
    expect(t).toContain("Beyan edilen HEAD, gözlemlenen commit’ten farklı");
    expect(t).toContain("ECE denetimi çalıştırın");
  });

  it("an assessment error is shown as an error, not as a failed CI, and recommends reviewing it", async () => {
    backend.projects.push(ref("a", ATM_NAME));
    await goCheckpoints();
    await claim(ATM_NAME, { repo: TARGET_REPOSITORY, accepted: SHA });
    fill();
    await userEvent.click(submitButton());
    await resolveWith(0, goodReport({
      outcome: "NOT_ASSESSED", ci: "NOT_EVALUATED", availability: "NOT_EVALUATED", review: "NOT_EVALUATED", freshness: "NOT_EVALUATED",
      limitations: ["NO_INDEPENDENT_REVIEW", "NO_HUMAN_AUTHORIZATION", "NOT_CURRENT_HEAD_ATTESTATION"], blocking: [], error: "ATTEMPT_NOT_LATEST",
    }));
    const t = text(card(ATM_NAME));
    expect(t).toMatch(/Bu, gözlenmiş bir CI başarısızlığı değildir/);
    expect(t).toContain("Değerlendirme hatasını inceleyin");
    expect(t).not.toContain("Düzeltin / yeniden çalıştırın");
    expect(t).not.toMatch(/başarısız ya da eksik/);
  });
});

describe("ECE only from the user's form submission", () => {
  it("opening the page, editing claims, filtering and opening details make no ECE call, no fetch and no storage write", async () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    backend.projects.push(ref("a", ATM_NAME), ref("f", "Proje F"));
    await goCheckpoints();
    await waitFor(() => expect(backend.calls.some((c) => c.cmd === "list_projects")).toBe(true));
    const settled = backend.calls.length;
    await claim(ATM_NAME, { repo: TARGET_REPOSITORY, accepted: SHA });
    await claim("Proje F", { repo: "someone/else", blockers: "bir engel #5" });
    await userEvent.click(screen.getAllByRole("button", { name: "ECE denetim formuna git" })[0]);
    fireEvent.change(screen.getByRole("combobox", { name: "Rol" }), { target: { value: "FOREIGN" } });
    fireEvent.change(screen.getByRole("textbox", { name: "Ara (ad ya da depo)" }), { target: { value: "else" } });
    expect(assessCalls).toHaveLength(0);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(setItem).not.toHaveBeenCalled();
    expect(localStorage.length + sessionStorage.length).toBe(0);
    // after the page has loaded its (read-only) project list, nothing it does reaches the native side at all
    expect(backend.calls.slice(settled)).toEqual([]);
  });

  it("the form submission is the only thing that calls the command, with the three typed fields", async () => {
    await goCheckpoints();
    fill();
    await userEvent.click(submitButton());
    expect(assessCalls).toHaveLength(1);
    expect(assessCalls[0].args).toEqual({ request: { commitSha: SHA, runId: Number(RUN), runAttempt: 1 } });
  });

  it("the virtual ATM row receives the observation when no project claims the ATM repository", async () => {
    await goCheckpoints();
    fill();
    await userEvent.click(submitButton());
    await resolveWith(0, goodReport());
    expect(text(card("ATM (sabit hedef)"))).toContain(`Gözlenen commit ${SHA} [ECE]`);
  });

  it("a foreign project gets no ECE button, no form and no observation, even right after an ATM check", async () => {
    backend.projects.push(ref("f", "Proje F"));
    await goCheckpoints();
    await claim("Proje F", { repo: "someone/else" });
    fill();
    await userEvent.click(submitButton());
    await resolveWith(0, goodReport());
    const f = card("Proje F");
    expect(within(f).queryByRole("button", { name: /ECE denetim formuna git/ })).toBeNull();
    expect(within(f).queryByRole("button", { name: "GitHub CI Kanıtını Denetle" })).toBeNull();
    expect(text(f)).toContain("Bu depo için ATM içinde ECE doğrulaması desteklenmiyor; doğrulamayı ATM dışında yapın.");
    expect(text(f)).not.toContain("[ECE]");
    expect(screen.getAllByRole("button", { name: "GitHub CI Kanıtını Denetle" })).toHaveLength(1); // one panel on the page
    expect(screen.getAllByRole("heading", { level: 2, name: "GitHub CI Kanıtını Denetle (ECE)" })).toHaveLength(1);
  });
});

describe("stale answers", () => {
  it("an answer for edited inputs is dropped, and a later submission is not invalidated by the older one", async () => {
    await goCheckpoints();
    fill();
    await userEvent.click(submitButton());
    fireEvent.change(attemptBox(), { target: { value: "2" } }); // the old answer now belongs to old inputs
    await resolveWith(0, goodReport());
    expect(text(card("ATM (sabit hedef)"))).toMatch(/Bu oturumda ECE makine gözlemi yok/);
    await userEvent.click(submitButton());
    expect(assessCalls).toHaveLength(2);
    await resolveWith(1, goodReport({ provenance: { ...goodReport().provenance, runAttempt: 2 } }));
    expect(text(card("ATM (sabit hedef)"))).toContain(`Gözlenen commit ${SHA} [ECE]`);
    expect(text(card("ATM (sabit hedef)"))).toMatch(/Çalıştırma · deneme 41217731446 · 2/);
  });

  it("editing the inputs after an answer clears the observation from the card", async () => {
    await goCheckpoints();
    fill();
    await userEvent.click(submitButton());
    await resolveWith(0, goodReport());
    expect(text(card("ATM (sabit hedef)"))).toContain("[ECE]");
    fireEvent.change(runBox(), { target: { value: "1" } });
    expect(text(card("ATM (sabit hedef)"))).not.toContain("[ECE]");
  });

  it("an answer that arrives after the ATM row changed is not carried to the new row", async () => {
    backend.projects.push(ref("a", ATM_NAME), ref("b", "Proje B", "2026-12-01T00:00:00Z"));
    await goCheckpoints();
    await claim(ATM_NAME, { repo: TARGET_REPOSITORY });
    fill();
    await userEvent.click(submitButton()); // bound to project "a"
    await claim(ATM_NAME, { repo: "someone/else" }); // "a" is no longer the ATM project
    await claim("Proje B", { repo: TARGET_REPOSITORY });
    await resolveWith(0, goodReport());
    expect(cards().every((c) => !text(c).includes("[ECE]"))).toBe(true);
    expect(card("Proje B")).toHaveAttribute("data-role", "ATM");
    expect(text(card("Proje B"))).toMatch(/Bu oturumda ECE makine gözlemi yok/);
  });

  it("an unrecognized or failed answer produces no observation", async () => {
    await goCheckpoints();
    fill();
    await userEvent.click(submitButton());
    await resolveWith(0, { schemaVersion: "other" });
    expect(text(card("ATM (sabit hedef)"))).toMatch(/Bu oturumda ECE makine gözlemi yok/);
    expect(text(card("ATM (sabit hedef)"))).not.toContain("[ECE]");
  });
});

describe("leaving the page keeps nothing", () => {
  it("claims and the observation vanish on navigation; nothing is stored", async () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    backend.projects.push(ref("a", ATM_NAME));
    await goCheckpoints();
    await claim(ATM_NAME, { repo: TARGET_REPOSITORY, accepted: SHA });
    fill();
    await userEvent.click(submitButton());
    await resolveWith(0, goodReport());
    expect(text(card(ATM_NAME))).toContain("[ECE]");
    await userEvent.click(within(nav()).getByRole("button", { name: "Ana Sayfa" }));
    await screen.findByRole("heading", { level: 1, name: "Ana Sayfa" });
    await userEvent.click(within(nav()).getByRole("button", { name: "Checkpoint Merkezi" }));
    await screen.findByRole("heading", { level: 2, name: "Proje Checkpoint Özeti" });
    const t = text(card("ATM (sabit hedef)")); // claims are gone, so the fixed row is back
    expect(t).not.toContain("[ECE]");
    expect(document.body.textContent).not.toContain(SHA);
    expect(setItem).not.toHaveBeenCalled();
    expect(localStorage.length + sessionStorage.length).toBe(0);
  });

  it("states that claims are not persistent", async () => {
    await goCheckpoints();
    expect(text(document.querySelector(".ov"))).toMatch(/Beyanlar kalıcı değildir; yalnız bu oturumda/);
  });
});

describe("claim form validation", () => {
  it("explains invalid values and stores nothing", async () => {
    backend.projects.push(ref("a", ATM_NAME));
    await goCheckpoints();
    await claim(ATM_NAME, { repo: "a/b/c", accepted: "ABC", head: OTHER_SHA.toUpperCase(), blockers: "x #0" });
    const t = text(card(ATM_NAME));
    expect(t).toMatch(/Depo “sahip\/ad” biçiminde olmalı/);
    expect(t).toMatch(/Tam, küçük harfli, 40 karakterlik bir commit SHA olmalı/);
    expect(t).toMatch(/1. satır: Issue numarası/);
    expect(text(cards()[0].querySelector("h3"))).toBe("ATM (sabit hedef)"); // nothing was applied
  });

  it("parseClaimsForm: exact syntax, no silent correction, limits", () => {
    const ok = (over: Partial<typeof EMPTY_FORM>) => parseClaimsForm({ ...EMPTY_FORM, ...over });
    expect(ok({}).ok).toBe(true);
    expect(ok({ repository: "a-b/c_d.e", acceptedSha: SHA, claimedHeadSha: SHA, blockers: "engel #12\nbaşka" })).toMatchObject({ ok: true });
    for (const repository of [" a/b", "a/b ", "a/b/c", "/x", "x/", "a//b", "../x", "a/..", "./a", "a b/c", "a/b;", "a/b%2f", "a/b\n", "ö/b", "x".repeat(40) + "/b", "a/" + "x".repeat(101)]) {
      expect(ok({ repository }), repository).toMatchObject({ ok: false });
    }
    for (const acceptedSha of [SHA.toUpperCase(), SHA.slice(1), SHA + "0", " " + SHA.slice(1)]) expect(ok({ acceptedSha })).toMatchObject({ ok: false });
    expect(ok({ blockers: Array.from({ length: 11 }, () => "x").join("\n") })).toMatchObject({ ok: false });
    expect(ok({ blockers: "x".repeat(161) })).toMatchObject({ ok: false });
    expect(ok({ blockers: "x #1000000000" })).toMatchObject({ ok: false });
    expect(ok({ blockers: "" })).toMatchObject({ ok: true, claims: { blockers: [] } });
    expect(ok({ blockers: "a\n\nb" })).toMatchObject({ ok: false });
  });
});

describe("filters", () => {
  it("filter by role, action and text; no new call", async () => {
    backend.projects.push(ref("f", "Proje F"));
    await goCheckpoints();
    const before = backend.calls.length;
    fireEvent.change(screen.getByRole("combobox", { name: "Rol" }), { target: { value: "FOREIGN" } });
    expect(cards().map((c) => text(c.querySelector("h3")))).toEqual(["Proje F"]);
    fireEvent.change(screen.getByRole("combobox", { name: "Rol" }), { target: { value: "ATM" } });
    expect(cards().map((c) => text(c.querySelector("h3")))).toEqual(["ATM (sabit hedef)"]);
    fireEvent.change(screen.getByRole("combobox", { name: "Rol" }), { target: { value: "all" } });
    fireEvent.change(screen.getByRole("combobox", { name: "Önerilen eylem" }), { target: { value: "ENTER_CLAIMS" } });
    expect(cards().map((c) => text(c.querySelector("h3")))).toEqual(["Proje F"]);
    fireEvent.change(screen.getByRole("textbox", { name: "Ara (ad ya da depo)" }), { target: { value: "yok-böyle" } });
    expect(cards()).toHaveLength(0);
    expect(screen.getByText("Süzgece uyan kart yok.")).toBeInTheDocument();
    await waitFor(() => expect(backend.calls.length).toBe(before));
    expect(assessCalls).toHaveLength(0);
  });
});

describe("source boundary", () => {
  it("only the ECE panel produces observations; the paste viewer and the overview code never touch the producer", () => {
    expect(panelSource.match(/makeEceObservation\(/g)).toHaveLength(1);
    for (const src of [pasteViewerSource, pasteLogicSource, centerSource, overviewSource, formSource]) expect(src).not.toMatch(/makeEceObservation/);
    expect(pasteViewerSource).not.toMatch(/ciAssessment|CiEvidencePanel|projectCheckpoint/);
  });

  it("the new code has no storage, network, timer, link or native call except the read-only project list", () => {
    const forbidden = /setTimeout|setInterval|queueMicrotask|localStorage|sessionStorage|indexedDB|fetch\(|XMLHttpRequest|WebSocket|navigator\.|window\.open|dangerouslySet|innerHTML|eval\(|new Function|<a\s|href=|invoke\(|@tauri-apps/;
    for (const src of [centerSource, overviewSource, formSource]) expect(src).not.toMatch(forbidden);
    expect(centerSource.match(/api\.\w+/g)).toEqual(["api.listProjects"]);
    expect(overviewSource).not.toMatch(/\bapi\b/);
    expect(panelSource.match(/api\.assessAtmCheckpoint\(/g)).toHaveLength(1);
  });
});

const openCheckpoints = async () => {
  render(<App />);
  await userEvent.click(within(nav()).getByRole("button", { name: "Checkpoint Merkezi" }));
  await screen.findByRole("heading", { level: 1, name: "Checkpoint Merkezi" });
};
const summaryHeading = () => screen.queryByRole("heading", { level: 2, name: "Proje Checkpoint Özeti" });

describe("RT-WP2-01: the project list is loading or unreadable", () => {
  it("while the list is pending there is a loading state and no empty or partial summary (no cards, no fixed ATM row, no filter counts)", async () => {
    let open!: () => void;
    listGate = new Promise<void>((r) => { open = r; });
    backend.projects.push(ref("r1", "Proje X"));
    await openCheckpoints();
    expect(screen.getByText("Proje listesi yükleniyor…")).toBeInTheDocument();
    expect(cards()).toHaveLength(0);
    expect(document.body.textContent).not.toContain("ATM (sabit hedef)");
    expect(document.body.textContent).not.toMatch(/kart gösteriliyor/);
    // the independent parts stay usable
    expect(screen.getByRole("textbox", { name: /Checkpoint metaverisi/ })).toBeInTheDocument();
    expect(shaBox()).toBeInTheDocument();
    await act(async () => open());
    await waitFor(() => expect(cards()).toHaveLength(2));
    expect(screen.queryByText("Proje listesi yükleniyor…")).toBeNull();
  });

  it("a failed list read builds no summary and no card; the error offers a retry, and the paste viewer and the ECE form stay usable", async () => {
    backend.failures["list_projects"] = { code: "DB_UNAVAILABLE", message: "Veritabanı okunamadı." };
    backend.projects.push(ref("r1", "Proje X"));
    await openCheckpoints();
    expect(await screen.findByText(/Kayıtlı proje listesi okunamadığı için özet oluşturulmadı/)).toBeInTheDocument();
    expect(screen.getByText(/Veritabanı okunamadı/)).toBeInTheDocument();
    expect(cards()).toHaveLength(0);
    expect(document.body.textContent).not.toContain("ATM (sabit hedef)");
    expect(document.body.textContent).not.toMatch(/kart gösteriliyor|gerçek depo olmadığı için gösterilmiyor/);
    expect(screen.getByRole("textbox", { name: /Checkpoint metaverisi/ })).toBeInTheDocument();
    // the independent ECE panel still works (and still only on the user's submit)
    fill();
    await userEvent.click(submitButton());
    expect(assessCalls).toHaveLength(1);
    await resolveWith(0, goodReport());
    expect(screen.getByRole("region", { name: "GitHub CI denetim sonucu" })).toBeInTheDocument();
  });

  it("retry after a failure shows the correct rows", async () => {
    backend.failures["list_projects"] = { code: "DB_UNAVAILABLE", message: "Veritabanı okunamadı." };
    backend.projects.push(ref("r1", "Proje X"));
    await openCheckpoints();
    await screen.findByText(/özet oluşturulmadı/);
    delete backend.failures["list_projects"];
    await userEvent.click(screen.getByRole("button", { name: "Yeniden dene" }));
    await waitFor(() => expect(cards().map((c) => text(c.querySelector("h3")))).toEqual(["ATM (sabit hedef)", "Proje X"]));
    expect(screen.queryByText(/özet oluşturulmadı/)).toBeNull();
  });

  it("a successful but really empty list (only a synthetic project) still shows the fixed ATM row", async () => {
    await goCheckpoints();
    expect(cards().map((c) => text(c.querySelector("h3")))).toEqual(["ATM (sabit hedef)"]);
  });

  it("a failed read on re-entering the page shows the explanation, never an empty or old summary", async () => {
    backend.projects.push(ref("r1", "Proje X"));
    await goCheckpoints();
    expect(cards()).toHaveLength(2);
    backend.failures["list_projects"] = { code: "DB_UNAVAILABLE", message: "Veritabanı okunamadı." };
    await userEvent.click(within(nav()).getByRole("button", { name: "Ana Sayfa" }));
    await userEvent.click(within(nav()).getByRole("button", { name: "Checkpoint Merkezi" }));
    await screen.findByText(/özet oluşturulmadı/);
    expect(cards()).toHaveLength(0);
    expect(summaryHeading()).not.toBeNull();
  });
});

const stylesSource: string = readFileSync("src/styles.css", "utf8");

describe("RT-WP2-02: three columns at the normal content width (static contract; jsdom does not measure layout)", () => {
  const rule = (sel: string) => stylesSource.match(new RegExp(sel.replace(".", "\\.") + " \\{([^}]*)\\}"))![1];
  it("three minimum-width cards and two gaps fit the unchanged .content width, and the grid still collapses on narrow windows", () => {
    const content = rule(".content");
    const max = Number(/max-width: (\d+)px/.exec(content)![1]);
    const sidePadding = Number(/padding: [\d.]+rem ([\d.]+)rem/.exec(content)![1]) * 16;
    expect(max).toBe(1100); // the global width is not changed by this fix
    const grid = rule(".ov-grid");
    const minRem = Number(/minmax\(min\(100%, ([\d.]+)rem\), 1fr\)/.exec(grid)![1]);
    const gapRem = Number(/gap: ([\d.]+)rem/.exec(grid)![1]);
    expect(grid).toContain("repeat(auto-fit");
    expect((3 * minRem + 2 * gapRem) * 16).toBeLessThanOrEqual(max - 2 * sidePadding);
    expect(grid).toContain("min(100%,"); // the minimum is capped by the container: one column when the window is narrow
  });
});

describe("RT-WP2-03: the freshness line follows what ECE reported", () => {
  const goToAtm = async () => {
    backend.projects.push(ref("a", ATM_NAME));
    await goCheckpoints();
    await claim(ATM_NAME, { repo: TARGET_REPOSITORY, accepted: SHA });
    fill();
    await userEvent.click(submitButton());
  };
  const notAssessed = (over = {}) => goodReport({
    outcome: "NOT_ASSESSED", ci: "NOT_EVALUATED", availability: "NOT_EVALUATED", review: "NOT_EVALUATED", freshness: "NOT_EVALUATED",
    limitations: ["NO_INDEPENDENT_REVIEW", "NO_HUMAN_AUTHORIZATION", "NOT_CURRENT_HEAD_ATTESTATION"], blocking: [], error: "ATTEMPT_NOT_LATEST", ...over,
  });
  it("a completed historical assessment keeps the historical-evidence warning", async () => {
    await goToAtm();
    await resolveWith(0, goodReport());
    const t = text(card(ATM_NAME));
    expect(t).toContain("TARİHSEL KANIT — GÜNCEL HEAD ONAYI DEĞİLDİR");
    expect(t).not.toContain("Güncellik değerlendirilmedi");
  });
  it("NOT_ASSESSED / NOT_EVALUATED: neutral freshness text, no historical-evidence claim, no CI result claim", async () => {
    await goToAtm();
    await resolveWith(0, notAssessed());
    const t = text(card(ATM_NAME));
    expect(t).toContain("Güncellik değerlendirilmedi; güncel HEAD doğrulanmadı.");
    expect(t).not.toMatch(/TARİHSEL KANIT|tarihsel kanıt/i);
    expect(t).not.toMatch(/başarılı sonuçlandı|geçti/);
    expect(t).toContain("Değerlendirme hatasını inceleyin");
  });
  it("a provider failure is neither a CI failure nor an acceptance, and makes no historical-evidence claim", async () => {
    await goToAtm();
    await resolveWith(0, goodReport({ outcome: "INSUFFICIENT_EVIDENCE", ci: "NOT_EVALUATED", review: "NOT_EVALUATED", freshness: "NOT_EVALUATED", limitations: [], error: "PROVIDER_FAILURE", providerFailure: "RATE_LIMITED" }));
    const t = text(card(ATM_NAME));
    expect(t).toMatch(/Bu, gözlenmiş bir CI başarısızlığı değildir/);
    expect(t).not.toMatch(/TARİHSEL KANIT|başarılı sonuçlandı|başarısız ya da eksik|Düzeltin/);
    expect(t).toContain("Güncellik değerlendirilmedi; güncel HEAD doğrulanmadı.");
    expect(t).toContain("Sonra yeniden deneyin");
  });
  it("the trust separation is unchanged: HEAD unknown, review and authorization from ECE only, standing blockers listed", async () => {
    await goToAtm();
    await resolveWith(0, notAssessed({ limitations: ["NO_INDEPENDENT_REVIEW"] }));
    const t = text(card(ATM_NAME));
    expect(t).toMatch(/Güncel HEAD doğrulanmadı — bilinmiyor/);
    expect(t).toMatch(/Gözlenen inceleme değerlendirilmedi \[bilinmiyor\]/);
    expect(t).toMatch(/Gözlenen onay İNSAN ONAYI: BEKLEMEDE \[ECE\]/);
    expect(t).toMatch(/Devam eden eksikler.*HEAD doğrulanmadı.*Bağımsız inceleme doğrulanmadı.*İnsan onayı doğrulanmadı/);
  });
});
