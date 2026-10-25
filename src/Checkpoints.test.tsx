// ATM-GOV-01 - the "Checkpoint Merkezi" page inside the real App, against the in-memory backend. jsdom + user-event only: no real window, no screen reader.
// Pasted records are made up; "secrets" are obviously fake sentinels. The page must show claims as claims, fail closed, hold nothing after leaving, and cause no side effect.
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { CheckpointsPage } from "./pages/Checkpoints";
import pageSource from "./pages/Checkpoints.tsx?raw";
import { ERROR_TEXT, NOT_CHECKED_SENTENCE, SCHEMA_VERSION, SCOPE_SENTENCE, TRUST_LABEL } from "./checkpointView";
import { createBackend, type Backend } from "./test/backend";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const SHA = "ba2fb81af4cd8bd802dfb0b34fff1af4f24bf71b";
const SHA2 = "68eda3f0407490fabccb517dcf822ab4818fc0bf";
const DIGEST = "b".repeat(64);
const ECHO = "SENTINEL_PASTE_7c1d";

const record = (over: Record<string, unknown> = {}) => ({
  repository: "ornek/depo", phase: "ORNEK-01", sourceSha: SHA,
  workflows: [{ name: "CI", runId: 40175302645, runAttempt: 1, ref: "development", expectedJobs: ["windows-x64", "macos-arm64"],
    jobs: [{ name: "windows-x64", claimedConclusion: "success" }, { name: "macos-arm64", claimedConclusion: "skipped" }] }],
  artifacts: [{ id: 99, name: "paket", sha256: DIGEST, expiresAt: "2026-11-01T00:00:00Z", availabilityClaim: "retained" }],
  issues: [33, 32], pullRequests: [5],
  reviewerClaim: { role: "independent_reviewer", assessment: "accepted", onSha: SHA },
  limitations: ["Yalnızca beyan"], missingEvidence: ["branch_head_not_checked"],
  ...over,
});
const doc = (...records: Record<string, unknown>[]) => JSON.stringify({ schemaVersion: SCHEMA_VERSION, checkpoints: records.length ? records : [record()] });
const VALID = doc();

const textbox = () => screen.getByRole("textbox", { name: /Checkpoint metaverisi/ });
const paste = (text: string) => fireEvent.change(textbox(), { target: { value: text } });
const showButton = () => screen.getByRole("button", { name: /^Göster/ });
const clearButton = () => screen.getByRole("button", { name: /^Temizle/ });
const summary = () => screen.queryByRole("region", { name: "Kayıt özeti" });
const go = async () => {
  render(<App />);
  await userEvent.click(within(screen.getByRole("navigation", { name: "Ana gezinme" })).getByRole("button", { name: "Checkpoint Merkezi" }));
  await screen.findByRole("heading", { level: 1, name: "Checkpoint Merkezi" });
};
const text = (el: HTMLElement | null) => el?.textContent?.replace(/\s+/g, " ") ?? "";
/** The text of the whole document EXCEPT the paste box (whose own value is, of course, what the user typed). */
const outsideTheBox = () => {
  const clone = document.body.cloneNode(true) as HTMLElement;
  clone.querySelectorAll("textarea").forEach((t) => t.remove());
  return clone.textContent ?? "";
};

let fetchSpy: ReturnType<typeof vi.fn>;
let writeText: ReturnType<typeof vi.fn>;
beforeEach(() => {
  backend = createBackend();
  fetchSpy = vi.fn();
  writeText = vi.fn();
  vi.stubGlobal("fetch", fetchSpy);
  Object.defineProperty(navigator, "clipboard", { value: { writeText, readText: vi.fn() }, configurable: true });
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("navigation, empty state and the trust boundary", () => {
  it("is a separate Turkish navigation entry; the page opens empty with the boundary note and instructions", async () => {
    await go();
    const nav = screen.getByRole("navigation", { name: "Ana gezinme" });
    expect(within(nav).getByRole("button", { name: "Checkpoint Merkezi" })).toHaveAttribute("aria-current", "page");
    expect(within(nav).getAllByRole("button")).toHaveLength(10);
    const note = screen.getAllByRole("note")[0];
    expect(note).toHaveTextContent(TRUST_LABEL);
    expect(note).toHaveTextContent(/Ne bir doğrulayıcıdır ne de bir sertifika/);
    expect(note).toHaveTextContent(/yalnızca bu sayfanın belleğinde durur, sayfadan ayrılınca silinir/);
    expect(textbox()).toHaveValue("");
    expect(summary()).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent(/Henüz bir şey gösterilmiyor/);
    expect(screen.getByText(SCHEMA_VERSION)).toBeInTheDocument();
    expect(screen.getByText(/Ayrı bir doğrulayıcı henüz yoktur/)).toBeInTheDocument();
  });

  it("pressing Göster on an empty box says so, shows nothing and does not call it an error of the data", async () => {
    await go();
    await userEvent.click(showButton());
    expect(screen.getByRole("alert")).toHaveTextContent(ERROR_TEXT.empty);
    expect(summary()).toBeNull();
  });
});

describe("a pasted record is shown as a claim, field by field, never as a result", () => {
  beforeEach(async () => {
    await go();
    paste(VALID);
    await userEvent.click(showButton());
  });

  it("shows the exact commit, run id and attempt together, the claimed job conclusions, artifact metadata, inert issue numbers and the reviewer claim", async () => {
    const region = screen.getByRole("region", { name: "Kayıt özeti" });
    const card = within(within(region).getByRole("article"));
    expect(card.getByRole("heading", { level: 2 })).toHaveTextContent("ornek/depo");
    expect(card.getAllByText(SHA)).toHaveLength(2); // the full 40 characters: the source commit and the commit the reviewer claim names
    expect(text(region)).toMatch(/Çalıştırma kimliği \(bildirilen\)40175302645/);
    expect(text(region)).toMatch(/Deneme \(bildirilen\)1/);
    expect(text(region)).toMatch(/Tetikleyen ref \(yalnızca beyan\)development/);
    expect(card.getByRole("list", { name: "CI: bildirilen işler" })).toHaveTextContent(/windows-x64 — bildirilen sonuç: başarılı \(denetlenmedi\)/);
    expect(card.getByRole("list", { name: "CI: bildirilen işler" })).toHaveTextContent(/macos-arm64 — bildirilen sonuç: atlandı \(denetlenmedi\)/);
    expect(text(region)).toMatch(/Beklenen işler \(bildirilen\): windows-x64, macos-arm64/);
    expect(card.getByText("Çıktı kimliği 99")).toBeInTheDocument();
    expect(card.getByText(DIGEST)).toBeInTheDocument();
    expect(text(region)).toMatch(/Durum \(bildirilen\)saklanıyor/);
    expect(text(region)).toMatch(/Issue: #33, #32\./);
    expect(text(region)).toMatch(/Çekme isteği: #5\./);
    expect(text(region)).toMatch(/Yalnızca metin; bağlantı değildir, hiçbir şey açılmaz ya da getirilmez\./);
    expect(text(region)).toMatch(/Kaynağın bildirdiği değerlendirme: kabul \(bağımsız inceleyici olduğunu söyleyen\)\. Bu bir beyandır; bağımsız bir onay değildir ve doğrulanmadı\./);
    expect(text(region)).toMatch(/Yalnızca beyan/);
    expect(text(region)).toMatch(/Dal ucunun \(HEAD\) bu commit olup olmadığı denetlenmedi/);
  });

  it("labels every record AND the page with the exact trust label, and says nothing was checked remotely", () => {
    const region = screen.getByRole("region", { name: "Kayıt özeti" });
    const article = within(region).getByRole("article");
    expect(article).toHaveTextContent(TRUST_LABEL);
    expect(article).toHaveTextContent(NOT_CHECKED_SENTENCE);
    expect(screen.getAllByRole("note")[0]).toHaveTextContent(TRUST_LABEL);
    expect(document.body.textContent!.split(TRUST_LABEL).length - 1).toBeGreaterThanOrEqual(2);
  });

  it("never claims the commit is the current branch head, and states the scope of the claim", () => {
    const region = screen.getByRole("region", { name: "Kayıt özeti" });
    expect(region).toHaveTextContent(SCOPE_SENTENCE);
    expect(text(region)).not.toMatch(/güncel dal ucu(dur| olduğu)|en son commit|latest|current head/i);
  });

  it("uses no success styling, icon, badge, link, image, control or progress total, and never says verified/accepted in its own voice", () => {
    const region = screen.getByRole("region", { name: "Kayıt özeti" });
    const t = text(region);
    expect(t).not.toMatch(/doğrulandı|onaylandı|başarıyla|✓|✔|\bPASS(ED)?\b|\bVERIFIED\b|\bACCEPTED\b|geçti\b/);
    expect(t).not.toMatch(/\d+\s*\/\s*\d+|%\s|yüzde/); // no progress total
    expect(region.querySelector("a, img, svg, progress, meter, [class*='badge'], [class*='passed'], [class*='success']")).toBeNull();
    expect(within(region).queryAllByRole("link")).toHaveLength(0);
    expect(within(region).queryAllByRole("button")).toHaveLength(0);
    expect(within(region).queryByRole("img")).toBeNull();
  });

  it("interprets the declared artifact expiry only against the clock read when 'Göster' was pressed, and says so", () => {
    const region = screen.getByRole("region", { name: "Kayıt özeti" });
    expect(text(region)).toMatch(/Göster”e bastığınız andaki bu cihaz saatiyle \(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z\) karşılaştırılmasıdır; çıktının şimdi var olup olmadığı denetlenmedi/);
  });

  it("has a sensible heading structure and labelled sections", () => {
    const region = screen.getByRole("region", { name: "Kayıt özeti" });
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Checkpoint Merkezi");
    expect(within(region).getAllByRole("heading", { level: 2 })).toHaveLength(1);
    const h3 = within(region).getAllByRole("heading", { level: 3 }).map((h) => text(h));
    expect(h3).toEqual(["İş akışı beyanı: CI", "Çıktı beyanları", "İlgili kayıt numaraları", "İnceleyici beyanı", "Sınırlar ve eksik kanıt (bildirilen)"]);
    expect(region).toHaveFocus(); // a successful "Göster" lands on the result
  });
});

describe("forged and inconsistent records stay claims", () => {
  it("a record that claims success, an independent acceptance and a retained artifact is shown with the same label and the same 'denetlenmedi' wording", async () => {
    await go();
    paste(doc(record({ reviewerClaim: { role: "independent_reviewer", assessment: "accepted" } })));
    await userEvent.click(showButton());
    const region = screen.getByRole("region", { name: "Kayıt özeti" });
    expect(region).toHaveTextContent(TRUST_LABEL);
    expect(region).toHaveTextContent(/bildirdiği değerlendirme: kabul/);
    expect(region).toHaveTextContent(/bağımsız bir onay değildir ve doğrulanmadı/);
    expect(region.querySelectorAll("strong").length).toBeGreaterThan(0);
  });

  it("a reviewer claim about another commit, a missing expected job and a retained-but-expired artifact are noted neutrally", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-12-01T00:00:00Z")); // after the declared expiry
    await go();
    paste(doc(record({ reviewerClaim: { role: "author", assessment: "accepted", onSha: SHA2 }, workflows: [{ name: "CI", runId: 40175302645, runAttempt: 2, expectedJobs: ["a", "b"], jobs: [{ name: "a", claimedConclusion: "failure" }] }] })));
    await userEvent.click(showButton());
    const region = screen.getByRole("region", { name: "Kayıt özeti" });
    expect(region).toHaveTextContent(/farklı bir commit için yazılmış; bu kayıt için geçerli sayılmaz/);
    expect(region).toHaveTextContent(/Beklenen bir iş, bildirilen işler arasında yok\. Bu, işin başarısız olduğunu ya da hiç çalışmadığını göstermez/);
    expect(region).toHaveTextContent(/bildirilen son kullanma zamanı bu cihazın saatine göre geçmiş görünüyor\. Hiçbiri denetlenmedi\./);
    expect(region).toHaveTextContent(/bildirilen zaman, bu cihazın saatine göre geçmiş görünüyor/);
    expect(region).toHaveTextContent("2026-12-01T00:00:00.000Z");
  });

  it("the same pasted record is read differently only through the clock, deterministically", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-10-30T00:00:00Z")); // before the declared expiry of 2026-11-01
    await go();
    paste(VALID);
    await userEvent.click(showButton());
    expect(screen.getByRole("region", { name: "Kayıt özeti" })).toHaveTextContent(/bildirilen zaman, bu cihazın saatine göre henüz gelmemiş görünüyor/);
    expect(screen.getByRole("region", { name: "Kayıt özeti" })).not.toHaveTextContent(/Hiçbiri denetlenmedi\./);
  });

  it("several records are listed one by one, none is combined into a total, and a stale historical commit is still just a claim", async () => {
    await go();
    paste(doc(record(), record({ phase: "ESKI-00", sourceSha: SHA2, workflows: [{ name: "CI", runId: 11, runAttempt: 1, jobs: [] }], artifacts: [], issues: [], pullRequests: [], reviewerClaim: undefined })));
    await userEvent.click(showButton());
    const region = screen.getByRole("region", { name: "Kayıt özeti" });
    expect(within(region).getAllByRole("article")).toHaveLength(2);
    expect(region).toHaveTextContent(/2 kayıt gösteriliyor\. Hepsi KAYNAK BEYANI — DOĞRULANMADI\./);
    expect(region).toHaveTextContent(/kayıtlar toplanıp bir sonuca çevrilmez/);
    for (const a of within(region).getAllByRole("article")) {
      expect(a).toHaveTextContent(TRUST_LABEL);
      expect(a).toHaveTextContent(SCOPE_SENTENCE);
    }
  });
});

describe("hostile or invalid input fails closed, with feedback that does not echo it", () => {
  const cases: [string, string][] = [
    ["HTML in a field", doc(record({ phase: `<img src=x onerror=alert('${ECHO}')>` }))],
    ["a script tag", doc(record({ limitations: [`<script>alert('${ECHO}')</script>`] }))],
    ["a markdown link", doc(record({ phase: `[x](https://evil.example/${ECHO})` }))],
    ["a javascript: URL", doc(record({ limitations: [`javascript:alert('${ECHO}')`] }))],
    ["a token-shaped value", doc(record({ limitations: ["ghp_" + "Ab1".repeat(10) + ECHO] }))],
    ["a secret word", doc(record({ limitations: [`token ${ECHO}`] }))],
    ["a local path", doc(record({ phase: "C:\\Users\\kisi\\gizli" }))],
    ["an unknown schema version", JSON.stringify({ schemaVersion: `v-${ECHO}`, checkpoints: [] })],
    ["an unexpected key", doc(record({ [ECHO]: true }))],
    ["a forged status key", doc(record({ verified: true }))],
    ["malformed JSON", `{ "schemaVersion": "${ECHO}", `],
    ["a run id of zero", doc(record({ workflows: [{ name: "CI", runId: 0, runAttempt: 1, jobs: [] }] }))],
    ["an uppercase commit", doc(record({ sourceSha: SHA.toUpperCase() }))],
    ["too much text", "x".repeat(40_001)],
  ];
  it.each(cases)("%s: an alert with a closed code, nothing rendered, nothing echoed", async (_label, input) => {
    await go();
    paste(input);
    await userEvent.click(showButton());
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Kayıt gösterilemedi.");
    expect(alert).toHaveTextContent(/Hiçbir şey gösterilmedi, kaydedilmedi ya da gönderilmedi/);
    expect(alert.textContent).not.toContain(ECHO);
    expect(alert.textContent).not.toMatch(/evil|alert\(|onerror|kisi|Users/);
    expect(summary()).toBeNull();
    expect(screen.queryByRole("article")).toBeNull();
    // nothing injected anywhere in the document
    expect(document.querySelector("img[src='x'], script, iframe, object, embed")).toBeNull();
    expect(document.body.innerHTML.includes("<img src=\"x\"")).toBe(false);
    const rendered = document.body.textContent!.replace(textbox().textContent ?? "", "");
    expect(rendered).not.toContain(ECHO);
  });

  it("a failed attempt discards the result of an earlier valid one, and the box keeps the text so it can be fixed", async () => {
    await go();
    paste(VALID);
    await userEvent.click(showButton());
    expect(summary()).not.toBeNull();
    paste("{ not json");
    expect(summary()).toBeNull(); // editing already hides the old result
    await userEvent.click(showButton());
    expect(screen.getByRole("alert")).toHaveTextContent(ERROR_TEXT.invalid_json);
    expect(summary()).toBeNull();
    expect(textbox()).toHaveValue("{ not json");
  });

  it("a hostile record that is VALID syntax (text only) renders as inert text: no element is created from it", async () => {
    await go();
    paste(doc(record({ phase: "Faz (ön) · #33 + devam", limitations: ["Yalnızca beyan; liste (1) ve (2)"] })));
    await userEvent.click(showButton());
    const region = screen.getByRole("region", { name: "Kayıt özeti" });
    expect(region).toHaveTextContent("Faz (ön) · #33 + devam");
    expect(region.querySelector("a, img, script, style, iframe")).toBeNull();
  });
});

describe("lifecycle: nothing survives clearing, editing or leaving the page", () => {
  it("'Temizle' empties the box and the result and returns focus to the box", async () => {
    await go();
    paste(VALID);
    await userEvent.click(showButton());
    expect(summary()).not.toBeNull();
    await userEvent.click(clearButton());
    expect(textbox()).toHaveValue("");
    expect(summary()).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(textbox()).toHaveFocus();
    expect(document.body.textContent).not.toContain(SHA);
    expect(screen.getByRole("status")).toHaveTextContent(/Henüz bir şey gösterilmiyor/);
  });

  it("'Temizle' also clears an error", async () => {
    await go();
    paste("{");
    await userEvent.click(showButton());
    expect(screen.getByRole("alert")).toBeInTheDocument();
    await userEvent.click(clearButton());
    expect(screen.queryByRole("alert")).toBeNull();
    expect(textbox()).toHaveValue("");
  });

  it("editing the text hides the previous result at once (the screen never shows a result for text that is no longer in the box)", async () => {
    await go();
    paste(VALID);
    await userEvent.click(showButton());
    expect(summary()).not.toBeNull();
    paste(VALID + " ");
    expect(summary()).toBeNull();
    expect(outsideTheBox()).not.toContain(SHA);
  });

  it("leaving the page destroys the pasted text and the result: coming back finds an empty page, and the old content is gone from the document", async () => {
    await go();
    paste(doc(record({ phase: "GIZLI-NOT-91" })));
    await userEvent.click(showButton());
    expect(document.body.textContent).toContain("GIZLI-NOT-91");
    const nav = screen.getByRole("navigation", { name: "Ana gezinme" });
    await userEvent.click(within(nav).getByRole("button", { name: "Ayarlar" }));
    await screen.findByRole("heading", { level: 1, name: "Ayarlar" });
    expect(document.body.textContent).not.toContain("GIZLI-NOT-91");
    expect(document.body.textContent).not.toContain(SHA);
    await userEvent.click(within(nav).getByRole("button", { name: "Checkpoint Merkezi" }));
    await screen.findByRole("heading", { level: 1, name: "Checkpoint Merkezi" });
    expect(textbox()).toHaveValue("");
    expect(summary()).toBeNull();
    expect(document.body.textContent).not.toContain("GIZLI-NOT-91");
  });

  it("unmounting the page leaves nothing behind in the document", () => {
    const { unmount, container } = render(<CheckpointsPage />);
    paste(VALID);
    fireEvent.click(showButton());
    expect(container.textContent).toContain(SHA);
    unmount();
    expect(container.textContent).toBe("");
    expect(document.body.textContent).not.toContain(SHA);
  });
});

describe("no side effect of any kind", () => {
  it("pasting, showing, failing and clearing make no IPC call, no network request, no clipboard access, no storage write and no window.open", async () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    const open = vi.spyOn(window, "open").mockImplementation(() => null);
    await go();
    await waitFor(() => expect(backend.calls.length).toBeGreaterThan(0));
    const before = backend.calls.length;
    paste(VALID);
    await userEvent.click(showButton());
    paste("{");
    await userEvent.click(showButton());
    paste(VALID);
    await userEvent.click(showButton());
    await userEvent.click(clearButton());
    expect(backend.calls.length, "no IPC call from this page").toBe(before);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(writeText).not.toHaveBeenCalled();
    expect(setItem).not.toHaveBeenCalled();
    expect(open).not.toHaveBeenCalled();
    expect(localStorage.length).toBe(0);
    expect(sessionStorage.length).toBe(0);
  });

  it("the page source has no IPC, network, storage, clipboard, timer, file, link or HTML-injection API", () => {
    const forbidden = /invoke\(|@tauri-apps|\bapi\b\.|from "\.\.\/api"|fetch\(|XMLHttpRequest|WebSocket|navigator\.|localStorage|sessionStorage|indexedDB|setTimeout|setInterval|queueMicrotask|createObjectURL|FileReader|window\.open|child_process|dangerouslySetInnerHTML|innerHTML|<a\s|href=|eval\(|new Function|useLoad/;
    expect(pageSource).not.toMatch(forbidden);
    // exactly two modules are imported: React and the pure view module
    const specifiers = [...pageSource.matchAll(/\sfrom "([^"]+)";/g)].map((m) => m[1]);
    expect(specifiers).toEqual(["react", "../checkpointView"]);
  });
});

describe("keyboard use", () => {
  it("the whole flow works from the keyboard: textarea, Göster, Temizle in tab order; Enter and Space activate; focus lands on the result", async () => {
    await go();
    await userEvent.click(textbox());
    await userEvent.paste(VALID);
    expect(textbox()).toHaveValue(VALID);
    await userEvent.tab();
    expect(showButton()).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    const region = screen.getByRole("region", { name: "Kayıt özeti" });
    expect(region).toHaveFocus();
    await userEvent.tab({ shift: true });
    expect(document.activeElement).not.toBe(document.body);
    clearButton().focus();
    await userEvent.keyboard(" ");
    expect(textbox()).toHaveValue("");
    expect(textbox()).toHaveFocus();
    expect(summary()).toBeNull();
  });

  it("errors are announced (role alert), the status area is polite, and controls have explicit names", async () => {
    await go();
    expect(screen.getByRole("status")).toHaveAttribute("aria-live", "polite");
    expect(textbox()).toHaveAccessibleName("Checkpoint metaverisi (JSON)");
    expect(showButton()).toHaveAccessibleName(/^Göster/);
    expect(clearButton()).toHaveAccessibleName(/^Temizle/);
    expect(textbox()).toHaveAttribute("autocomplete", "off");
    expect(textbox()).toHaveAttribute("spellcheck", "false");
    paste("{");
    await userEvent.click(showButton());
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("the example record is made-up static text inside a collapsed section, with no copy control", async () => {
    await go();
    const details = screen.getByText(/Beklenen biçim \(uydurma örnek\) ve sınırlar/).closest("details")!;
    expect(details).not.toHaveAttribute("open");
    const example = within(details).getByLabelText("Uydurma örnek kayıt");
    expect(example).toHaveAttribute("tabindex", "0");
    expect(example.textContent).toContain("ornek-sahip/ornek-depo");
    expect(within(details).queryAllByRole("button")).toHaveLength(0);
  });
});
