// ATM-GOV-02 P2B - "GitHub CI Kanıtını Denetle (ECE)" inside the real App, against the in-memory backend. jsdom + user-event only: no real window, no real network.
// The native command is replaced by a controllable fake: nothing here can reach GitHub.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import panelSource from "./components/CiEvidencePanel.tsx?raw";
import apiSource from "./api.ts?raw";
import { TRUST_LABEL } from "./checkpointView";
import { LABEL_AUTHORIZATION, LABEL_HISTORICAL, LABEL_MACHINE, LABEL_REVIEW } from "./ciAssessment";
import { goodReport } from "./test/ciReport";
import { createBackend, type Backend } from "./test/backend";
import type { CiAssessmentReport } from "./types";

const SHA = "dd37b04f730569c3a8ad1b929ee6b80052ef22e2";
const RUN = "41217731446";

let backend: Backend;
type Pending = { args: Record<string, unknown>; resolve: (v: unknown) => void; reject: (e: unknown) => void };
let assessCalls: Pending[];
let assessImpl: ((args: Record<string, unknown>) => Promise<unknown>) | null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "assess_atm_checkpoint") {
      return new Promise((resolve, reject) => {
        assessCalls.push({ args: args ?? {}, resolve, reject });
        if (assessImpl) assessImpl(args ?? {}).then(resolve, reject);
      });
    }
    return backend.handle(cmd, args);
  },
}));

let fetchSpy: ReturnType<typeof vi.fn>;
beforeEach(() => {
  backend = createBackend();
  assessCalls = [];
  assessImpl = null;
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
};
const shaBox = () => screen.getByRole("textbox", { name: /Tam commit SHA/ });
const runBox = () => screen.getByRole("textbox", { name: /çalıştırma kimliği/ });
const attemptBox = () => screen.getByRole("textbox", { name: /Deneme numarası/ });
const submitButton = () => screen.getByRole("button", { name: "GitHub CI Kanıtını Denetle" });
const fill = (sha: string, run: string, attempt = "1") => {
  fireEvent.change(shaBox(), { target: { value: sha } });
  fireEvent.change(runBox(), { target: { value: run } });
  fireEvent.change(attemptBox(), { target: { value: attempt } });
};
const result = () => screen.queryByRole("region", { name: "GitHub CI denetim sonucu" });
const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ") ?? "";
const resolveWith = async (i: number, report: unknown) => act(async () => assessCalls[i].resolve(report));

describe("no network, no command at mount, navigation or startup", () => {
  it("opening the page makes no assess call and no fetch; the manual viewer and the new section both exist and are separate", async () => {
    await goCheckpoints();
    await waitFor(() => expect(backend.calls.length).toBeGreaterThan(0));
    expect(assessCalls).toHaveLength(0);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(backend.calls.some((c) => c.cmd === "assess_atm_checkpoint")).toBe(false);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Checkpoint Merkezi");
    expect(screen.getByRole("heading", { level: 2, name: "GitHub CI Kanıtını Denetle (ECE)" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: /Checkpoint metaverisi/ })).toBeInTheDocument();
    expect(within(nav()).getAllByRole("button")).toHaveLength(10);
  });

  it("the manual viewer keeps its untrusted meaning: the first note on the page is still the source-claim label, and the new section says it is separate", async () => {
    await goCheckpoints();
    expect(screen.getAllByRole("note")[0]).toHaveTextContent(TRUST_LABEL);
    const section = screen.getByRole("heading", { level: 2, name: "GitHub CI Kanıtını Denetle (ECE)" }).closest("section")!;
    expect(text(section)).toMatch(/yapıştırma alanından ayrıdır/);
    // Typing in the paste box never reaches the native command.
    fireEvent.change(screen.getByRole("textbox", { name: /Checkpoint metaverisi/ }), { target: { value: `{"x":"${SHA}"}` } });
    expect(assessCalls).toHaveLength(0);
    expect(shaBox()).toHaveValue("");
  });

  it("typing in the three fields, tabbing and clearing make no command call", async () => {
    await goCheckpoints();
    fill(SHA, RUN, "2");
    await userEvent.tab();
    fill("", "", "1");
    expect(assessCalls).toHaveLength(0);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

describe("invalid input is rejected before anything is sent", () => {
  it("every malformed value shows the fixed message and sends nothing", async () => {
    await goCheckpoints();
    const bad: [string, string, string][] = [
      ["", RUN, "1"], ["dd37b04", RUN, "1"], [SHA.toUpperCase(), RUN, "1"], [SHA + "0", RUN, "1"], ["development", RUN, "1"],
      [SHA, "", "1"], [SHA, "0", "1"], [SHA, "-1", "1"], [SHA, "1e3", "1"], [SHA, "9007199254740992", "1"],
      [SHA, RUN, ""], [SHA, RUN, "0"], [SHA, RUN, "1001"], [SHA, RUN, "1.5"],
    ];
    for (const [sha, run, att] of bad) {
      fill(sha, run, att);
      await userEvent.click(submitButton());
      expect(screen.getAllByRole("alert").some((a) => /Hiçbir istek gönderilmedi/.test(a.textContent ?? "")), `${sha}|${run}|${att}`).toBe(true);
    }
    expect(assessCalls).toHaveLength(0);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

describe("the explicit submit", () => {
  it("sends exactly the three fields once, shows the machine evidence with the four fixed labels, and never an acceptance", async () => {
    await goCheckpoints();
    fill(SHA, RUN, "1");
    await userEvent.click(submitButton());
    expect(assessCalls).toHaveLength(1);
    expect(assessCalls[0].args).toEqual({ request: { commitSha: SHA, runId: 41217731446, runAttempt: 1 } });
    await resolveWith(0, goodReport());
    const region = await screen.findByRole("region", { name: "GitHub CI denetim sonucu" });
    const t = text(region);
    for (const label of [LABEL_MACHINE, LABEL_REVIEW, LABEL_AUTHORIZATION, LABEL_HISTORICAL]) expect(t, label).toContain(label);
    expect(t).toContain("ErenTahiroglu/AI-Toolchain-Manager");
    expect(t).toContain("1413039335");
    expect(t).toContain(SHA);
    expect(t).toContain("41217731446");
    expect(t).toContain("9f4e53e0a944a7681fb49957f0d5e30597c3148a");
    expect(t).toContain("atm-ci-two-runner-v1");
    expect(t).toContain("VERIFIED_PASS");
    expect(t).toContain("ASSESSED_LIMITATIONS");
    expect(t).toMatch(/kabul değildir/);
    // never a general accepted state, no success styling, no link, no image
    expect(t).not.toMatch(/kabul edildi|onaylandı|accepted|approved|checkpoint kabul/i);
    expect(region.querySelector("a, img, svg, progress, meter, [class*='success'], [class*='passed'], [class*='badge'], [class*='green']")).toBeNull();
    // the explanatory text states the real network behaviour and the unresolved WebView issue
    const section = screen.getByRole("heading", { level: 2, name: "GitHub CI Kanıtını Denetle (ECE)" }).closest("section")!;
    expect(text(section)).toMatch(/api\.github\.com/);
    expect(text(section)).toMatch(/Issue #25\) çözülmüş değildir/);
    expect(text(section)).toMatch(/en son denemesini/);
  });

  it("prevents duplicate submissions while one is pending", async () => {
    await goCheckpoints();
    fill(SHA, RUN);
    await userEvent.click(submitButton());
    expect(submitButton()).toBeDisabled();
    fireEvent.submit(submitButton().closest("form")!); // Enter / a forced submit
    await userEvent.click(submitButton());
    expect(assessCalls).toHaveLength(1);
    await resolveWith(0, goodReport());
    await screen.findByRole("region", { name: "GitHub CI denetim sonucu" });
    expect(submitButton()).toBeEnabled();
  });

  it("an answer that arrives after the inputs were edited is never shown for the new inputs", async () => {
    await goCheckpoints();
    fill(SHA, RUN, "1");
    await userEvent.click(submitButton());
    expect(assessCalls).toHaveLength(1);
    fireEvent.change(runBox(), { target: { value: "41217731447" } }); // edit while pending
    await resolveWith(0, goodReport()); // the OLD answer arrives
    expect(result()).toBeNull();
    expect(document.body.textContent).not.toContain("VERIFIED_PASS");
    expect(runBox()).toHaveValue("41217731447");
    expect(submitButton()).toBeEnabled();
    // a fresh submit for the new inputs works and shows only its own answer
    await userEvent.click(submitButton());
    expect(assessCalls).toHaveLength(2);
    expect(assessCalls[1].args).toEqual({ request: { commitSha: SHA, runId: 41217731447, runAttempt: 1 } });
    await resolveWith(1, goodReport({ provenance: { ...goodReport().provenance, runId: 41217731447 } }));
    expect(text(await screen.findByRole("region", { name: "GitHub CI denetim sonucu" }))).toContain("41217731447");
  });

  it("editing after a result removes the result; a late answer for the old inputs cannot come back", async () => {
    await goCheckpoints();
    fill(SHA, RUN);
    await userEvent.click(submitButton());
    await resolveWith(0, goodReport());
    await screen.findByRole("region", { name: "GitHub CI denetim sonucu" });
    fireEvent.change(attemptBox(), { target: { value: "2" } });
    expect(result()).toBeNull();
  });
});

describe("what is never shown as success", () => {
  it("an answer with an unknown value, another target or a contradiction is shown only as unrecognized", async () => {
    const bad: Partial<CiAssessmentReport>[] = [
      { review: "INDEPENDENT_REVIEW_PASS" }, { authorization: "EXPLICITLY_AUTHORIZED" }, { freshness: "CURRENT_HEAD" }, { outcome: "ACCEPTED" }, { ci: "FAILED" },
      { provenance: { ...goodReport().provenance, commitSha: "0".repeat(40) } },
    ];
    for (const over of bad) {
      const { unmount } = render(<App />);
      await userEvent.click(within(nav()).getByRole("button", { name: "Checkpoint Merkezi" }));
      await screen.findByRole("heading", { level: 1, name: "Checkpoint Merkezi" });
      fill(SHA, RUN);
      const before = assessCalls.length;
      await userEvent.click(submitButton());
      await resolveWith(before, goodReport(over));
      const region = await screen.findByRole("region", { name: "GitHub CI denetim sonucu" });
      expect(text(region), JSON.stringify(over)).toContain("Sonuç tanınmadı");
      expect(text(region)).not.toContain("VERIFIED_PASS");
      expect(text(region)).not.toContain(LABEL_MACHINE);
      unmount();
    }
  });

  it("a provider failure is shown as 'no evidence obtained', explicitly not a CI failure, and without a verdict about the checkpoint", async () => {
    await goCheckpoints();
    fill(SHA, RUN);
    await userEvent.click(submitButton());
    await resolveWith(
      0,
      goodReport({ outcome: "INSUFFICIENT_EVIDENCE", ci: "UNVERIFIED_RUN", availability: "UNREACHABLE", review: "NOT_EVALUATED", freshness: "NOT_EVALUATED", error: "PROVIDER_FAILURE", providerFailure: "NETWORK_UNAVAILABLE" }),
    );
    const t = text(await screen.findByRole("region", { name: "GitHub CI denetim sonucu" }));
    expect(t).toMatch(/Bu, bir CI başarısızlığı DEĞİLDİR/);
    expect(t).toMatch(/Ağa ulaşılamadı/);
    expect(t).toContain(LABEL_REVIEW);
    expect(t).toContain(LABEL_AUTHORIZATION);
    expect(t).not.toMatch(/VERIFIED_PASS|ASSESSED_LIMITATIONS|ASSESSED_FAIL/);
  });

  it("a superseded attempt is refused with its own explanation and no CI verdict", async () => {
    await goCheckpoints();
    fill(SHA, RUN, "1");
    await userEvent.click(submitButton());
    await resolveWith(0, goodReport({ outcome: "NOT_ASSESSED", ci: "NOT_EVALUATED", availability: "NOT_EVALUATED", review: "NOT_EVALUATED", freshness: "NOT_EVALUATED", error: "ATTEMPT_NOT_LATEST" }));
    const t = text(await screen.findByRole("region", { name: "GitHub CI denetim sonucu" }));
    expect(t).toMatch(/en son denemesi değil/);
    expect(t).toMatch(/karıştırılmaz/);
    expect(t).not.toContain("VERIFIED_PASS");
  });

  it("a native error (for example 'busy') is shown as a fixed sentence with its code and no CI result", async () => {
    await goCheckpoints();
    fill(SHA, RUN);
    await userEvent.click(submitButton());
    await act(async () => assessCalls[0].reject({ code: "CI_ASSESS_BUSY", message: "Bir GitHub CI denetimi zaten sürüyor; bitmesini bekleyin." }));
    const alert = await screen.findByRole("alert", { name: "GitHub CI denetim hatası" });
    expect(alert).toHaveTextContent("CI_ASSESS_BUSY");
    expect(alert).toHaveTextContent("Hiçbir CI sonucu üretilmedi");
    expect(result()).toBeNull();
    expect(submitButton()).toBeEnabled();
  });

  it("an unreadable IPC failure is shown as a generic fixed error, never as a CI result", async () => {
    await goCheckpoints();
    fill(SHA, RUN);
    await userEvent.click(submitButton());
    await act(async () => assessCalls[0].reject(new Error("raw C:\\Users\\someone\\secret path and token ghp_x")));
    const alert = await screen.findByRole("alert", { name: "GitHub CI denetim hatası" });
    expect(alert.textContent).not.toMatch(/Users|ghp_|secret/);
    expect(alert).toHaveTextContent("IPC_FAILED");
  });
});

describe("leaving the page keeps nothing", () => {
  it("a late answer after navigation is dropped, and nothing of the inputs or the result stays in the document or in storage", async () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    await goCheckpoints();
    fill(SHA, RUN);
    await userEvent.click(submitButton());
    await userEvent.click(within(nav()).getByRole("button", { name: "Ana Sayfa" }));
    await act(async () => assessCalls[0].resolve(goodReport()));
    expect(document.body.textContent).not.toContain(SHA);
    expect(document.body.textContent).not.toContain("VERIFIED_PASS");
    expect(setItem).not.toHaveBeenCalled();
    expect(localStorage.length).toBe(0);
    expect(sessionStorage.length).toBe(0);
  });
});

describe("source boundary", () => {
  it("the panel has no timer, storage, network, link, clipboard or HTML-injection API, and calls the command only from the submit handler", () => {
    const forbidden = /setTimeout|setInterval|queueMicrotask|localStorage|sessionStorage|indexedDB|fetch\(|XMLHttpRequest|WebSocket|navigator\.|window\.open|dangerouslySet|innerHTML|eval\(|new Function|<a\s|href=/;
    expect(panelSource).not.toMatch(forbidden);
    expect(panelSource.match(/api\.assessAtmCheckpoint\(/g)).toHaveLength(1);
    // the single call sits inside the submit handler, not in an effect or at render
    const submitBody = panelSource.split("const submit = ")[1].split("const invalidField")[0];
    expect(submitBody).toContain("api.assessAtmCheckpoint(");
    for (const effect of panelSource.matchAll(/useEffect\(\(\) => \{[\s\S]*?\n  \}, \[[^\]]*\]\);/g)) expect(effect[0]).not.toContain("api.");
  });

  it("the command is registered in api.ts once, with the request object as its only argument", () => {
    expect(apiSource.match(/"assess_atm_checkpoint"/g)).toHaveLength(1);
    expect(apiSource).toContain('call<CiAssessmentReport>("assess_atm_checkpoint", { request })');
  });
});

describe("privacy copy", () => {
  it("Settings no longer promises 'no network request' absolutely; it names the one command-scoped exception and keeps Issue #25 unresolved", async () => {
    render(<App />);
    await userEvent.click(within(nav()).getByRole("button", { name: "Ayarlar" }));
    await screen.findByRole("heading", { level: 1, name: "Ayarlar" });
    const privacy = screen.getByRole("heading", { level: 2, name: "Gizlilik" }).closest("section")!;
    const t = text(privacy);
    expect(t).not.toContain("hiçbir ağ isteği yapmaz");
    expect(t).toMatch(/bir tek istisna/);
    expect(t).toMatch(/GitHub CI Kanıtını Denetle/);
    expect(t).toMatch(/api\.github\.com/);
    expect(t).toMatch(/Issue #25\) bu ifadenin dışındadır, çözülmüş değildir/);
    expect(t).toMatch(/Telemetri yok/);
  });
});
