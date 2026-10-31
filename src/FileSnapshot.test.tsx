// CONFIG-02C (Issue #40) - the opt-in "Dosyadan oku" section, in jsdom with a MOCKED backend. No real window, no real file, no real folder: every answer is made up here, and
// "secrets" are unique sentinels. jsdom proves component logic only; real-window focus order, screen-reader output and WebView2 behaviour are NOT RUN (#25).
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import capabilitySource from "../src-tauri/capabilities/default.json?raw";
import { ConfigPreview } from "./components/ConfigPreview";
import { FileSnapshot } from "./components/FileSnapshot";
import snapshotSource from "./components/FileSnapshot.tsx?raw";
import requestDoc from "../docs/config-02c/REAL_READ_AUTHORIZATION_REQUEST.md?raw";
import { sha256Hex } from "./configPlan";
import { ALL_STATUS_SAMPLES } from "./test/snapshotSamples";
import {
  BRIDGE_DIFFERENT, BRIDGE_EQUAL, DECLINED_TEXT, DECLINE_BUTTON, INVENTORY_LABEL, NOT_REQUESTED_TEXT, READ_BUTTON, READ_CONSENT, SECTION_TITLE, SNAPSHOT_LABEL, SNAPSHOT_TTL_MS,
  GATE_POINTS, STATUSES, UNEXPECTED_TEXT, allFixedText, describeResult, errorText,
} from "./fileSnapshot";
import type { ReviewedDeclaration } from "./configDeclarations";
import { caseIt } from "./test/config02cResult";
import { createBackend, makeEntry, type Backend } from "./test/backend";

let backend: Backend;
let nextRead: (args: Record<string, unknown> | undefined) => unknown;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "read_project_mcp_config") {
      backend.calls.push({ cmd, args });
      return Promise.resolve().then(() => nextRead(args));
    }
    return backend.handle(cmd, args);
  },
}));

const WIN = "windows-x64";
const CC = "claude-code-terminal";
const AG = "antigravity-ide";
const PIN = "0123456789abcdef0123456789abcdef01234567";
const ALIAS = "benim-sitem";
const FOLDER = "C:\\Projeler\\sitem";
const ev = (platformTarget: string, agentSurface: string) => ({ platformTarget, agentSurface, compatibilityState: "vendor-declared", limitation: "kayıt" });
const mcp = () =>
  makeEntry("synthetic-mcp", { displayName: "Sentetik MCP", kind: "agent_plugin", record: { category: "mcp-server", sourceSnapshot: { immutableCommit: PIN }, targetEvidence: [ev(WIN, CC), ev("macos-arm64", AG)] } });
const DECLS: ReviewedDeclaration[] = [
  { hostId: "claude-code-project-mcp", entryId: "synthetic-mcp", serverName: "synthetic", declaration: { command: "npx", args: ["-y", "@example/synthetic-mcp@1.2.3"] }, review: { reviewId: "REV-SYN-1", evidenceIds: ["R99"], reviewedAgainst: PIN } },
];

const result = (over: Record<string, unknown> = {}) => ({
  schema: "config02c.read.v1", status: "present_summarized", reason: null, alias: ALIAS, file: ".mcp.json", scope: "project", surface: CC, maxBytes: 1048576,
  sizeBytes: 51, sha256: "ab".repeat(32), serverCount: 2, reviewedDeclarations: [{ id: "exa-mcp-server", nameState: "present", endpointState: "present_under_this_name" }],
  notClaims: ["not_loaded", "not_trusted", "not_installed", "not_enabled", "not_reachable", "not_verified"], ...over,
});

beforeEach(() => {
  backend = createBackend();
  nextRead = () => result();
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const reads = () => backend.calls.filter((c) => c.cmd === "read_project_mcp_config");
const renderPreview = (props: Partial<Parameters<typeof ConfigPreview>[0]> = {}) =>
  render(<ConfigPreview entry={mcp()} alias={ALIAS} projectName="Benim sitem" platform={WIN} surface={CC} declarations={DECLS} {...props} />);
const openPreview = () => userEvent.click(screen.getByText(/Yapılandırma önizlemesi — salt okunur/));
const openSection = async () => {
  await openPreview();
  await userEvent.click(screen.getByText(SECTION_TITLE));
};
const folderBox = () => screen.getByRole("textbox", { name: /Proje klasörünün tam yolu/ });
const readButton = () => screen.getByRole("button", { name: new RegExp(`^${READ_BUTTON.replace(/\./g, "\\.")}`) });
const declineButton = () => screen.getByRole("button", { name: DECLINE_BUTTON });
const typeFolder = async (text = FOLDER) => userEvent.type(folderBox(), text);
const readOnce = async () => {
  await typeFolder();
  await userEvent.click(readButton());
  await screen.findByRole("region", { name: new RegExp(SNAPSHOT_LABEL.replace(/[()]/g, "\\$&")) });
};

describe("the words on screen are the reviewed words", () => {
  caseIt("U4", "every fixed sentence the panel can show appears verbatim in the reviewed authorization request", () => {
    const norm = (x: string) => x.replace(/[*`>]/g, "").replace(/\s+/g, " ").trim();
    const doc = norm(requestDoc);
    const texts = allFixedText().map(norm).filter(Boolean);
    expect(texts.length).toBeGreaterThan(60);
    for (const text of texts) expect(doc, text.slice(0, 60)).toContain(text);
  });
});

describe("the network sentence of the gate stays within the evidence (R1-F2)", () => {
  caseIt("R2", "the gate says the read starts no network access on purpose, names the open WebView issue and promises no erasure or protection", () => {
    const local = GATE_POINTS.find((p) => p.lead === "Yerel:");
    expect(local).toBeDefined();
    const text = local!.text;
    expect(text).toContain("bu okuma işlemi");
    expect(text).toContain("bilerek ağ bağlantısı başlatmaz");
    expect(text).toContain("WebView çalışma ortamının ayrı ağ trafiği");
    expect(text).toContain("dışlanmış değildir");
    expect(text).toContain("(açık sorun #25)");
    expect(text).toContain("olmadığı da kanıtlanmamıştır");
    expect(text).toContain("vaadi değildir");
    expect(text).toContain("hiçbir program çalıştırılmaz, hiçbir dosyaya yazılmaz, hiçbir şey kurulmaz");
    // no absolute promise about the whole application or the runtime, and no claim of a confirmed leak
    for (const absolute of ["İnternete bağlanılmaz", "internete bağlanmaz", "ağa hiçbir", "hiçbir ağ", "sızdırıldı", "sızdı."]) expect(text, absolute).not.toContain(absolute);
    // the sentence is in the reviewed request exactly as shown
    expect(requestDoc.replace(/[*`>]/g, "").replace(/\s+/g, " ")).toContain(text);
  });
});

describe("the closed answer is accepted only in its exact shape", () => {
  caseIt("U2", "every status renders only fixed text; anything outside the contract is refused and never shown", () => {
    for (const sample of ALL_STATUS_SAMPLES) {
      const view = describeResult(sample, ALIAS);
      expect(view, JSON.stringify(sample.status)).not.toBeNull();
    }
    expect(STATUSES).toHaveLength(11);
    const secret = "ATM_SENTINEL_deadbeef_u2";
    const hostile: unknown[] = [
      null, "text", [], {}, { ...result(), path: `C:\\${secret}` }, { ...result(), status: `<img src=x onerror=1>${secret}` }, { ...result(), status: "not_configured" },
      { ...result(), reason: secret }, { ...result(), alias: "baska-proje" }, { ...result(), file: ".claude.json" }, { ...result(), maxBytes: 5 }, { ...result(), notClaims: [] },
      { ...result(), sha256: "AB".repeat(32) }, { ...result(), sha256: secret }, { ...result(), sizeBytes: -1 }, { ...result(), sizeBytes: 1048577 }, { ...result(), serverCount: 257 },
      { ...result(), reviewedDeclarations: [{ id: "exa-mcp-server", nameState: secret, endpointState: "absent" }] },
      { ...result(), reviewedDeclarations: [{ id: "exa-mcp-server", nameState: "present", endpointState: "absent", name: secret }] },
      { ...result(), reviewedDeclarations: [{ id: "other", nameState: "present", endpointState: "absent" }] },
      { ...result({ status: "absent" }), sizeBytes: 10, sha256: "ab".repeat(32), serverCount: null, reviewedDeclarations: null },
      { ...result({ status: "too_large", sizeBytes: null, sha256: null, serverCount: null, reviewedDeclarations: null }), serverCount: 1 },
      { ...result({ status: "present_metadata_only", reason: null, serverCount: null, reviewedDeclarations: null }) },
      { ...result({ status: "present_metadata_only", reason: "link", serverCount: null, reviewedDeclarations: null }) },
    ];
    for (const raw of hostile) {
      const view = describeResult(raw, ALIAS);
      expect(view, JSON.stringify(raw)).toBeNull();
    }
    expect(errorText("ANYTHING_ELSE")).not.toContain("ANYTHING");
    expect(errorText("PROJECT_NOT_FOUND")).toContain("bulunamadı");
  });

  caseIt("U2b", "a hostile answer in the live component shows only the fixed 'unexpected' sentence", async () => {
    const secret = "ATM_SENTINEL_cafef00d_u2b";
    nextRead = () => ({ ...result(), path: `C:\\Users\\${secret}`, status: `<b>${secret}</b>` });
    renderPreview();
    await openSection();
    await readOnce().catch(() => undefined);
    expect(await screen.findByText(UNEXPECTED_TEXT)).toBeInTheDocument();
    expect(document.body.textContent).not.toContain(secret);
    expect(document.body.innerHTML).not.toContain(secret);
  });
});

describe("opt-in, one click = one call, saying no", () => {
  caseIt("U1", "collapsed by default; no call until the click; the request is exactly {alias, directory, consent}; declining makes no call", async () => {
    renderPreview();
    await openPreview();
    expect(screen.getByText(SECTION_TITLE)).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: /Proje klasörünün/ })).not.toBeInTheDocument(); // the section is closed: no field, no buttons
    expect(reads()).toHaveLength(0);
    await userEvent.click(screen.getByText(SECTION_TITLE));
    expect(readButton()).toBeDisabled(); // nothing typed yet
    expect(reads()).toHaveLength(0);
    await typeFolder();
    await userEvent.click(declineButton());
    expect(reads()).toHaveLength(0);
    expect(screen.getByText(DECLINED_TEXT)).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: /Proje klasörünün/ })).not.toBeInTheDocument();
    // asking again: an empty field, the same gate, then exactly one call
    await userEvent.click(screen.getByText(SECTION_TITLE));
    expect(folderBox()).toHaveValue("");
    await typeFolder();
    await userEvent.click(readButton());
    await screen.findByRole("region", { name: /dosya anlık görüntüsü/ });
    expect(reads()).toHaveLength(1);
    expect(reads()[0].args).toEqual({ request: { alias: ALIAS, directory: FOLDER, consent: READ_CONSENT } });
    expect(Object.keys((reads()[0].args as { request: object }).request).sort()).toEqual(["alias", "consent", "directory"]);
  });

  it("shows the gate text with the file, the limit, the disclosures and the two labels before anything can be read", async () => {
    renderPreview();
    await openSection();
    const text = document.body.textContent ?? "";
    for (const needle of [".mcp.json", "1.048.576", "üst klasörlerin yalnızca türü", "harf büyüklüğü farkıyla", "gösterilmez ve saklanmaz", "güvenli silinmesi garanti edilmez", "5 dakika", INVENTORY_LABEL, SNAPSHOT_LABEL, "bilerek ağ bağlantısı başlatmaz", "WebView çalışma ortamının ayrı ağ trafiği", "#25", "kanıtlanmamıştır", NOT_REQUESTED_TEXT]) {
      expect(text, needle).toContain(needle);
    }
    expect(text).not.toContain("İnternete bağlanılmaz"); // the absolute promise is gone (Issue #25 is open)
    expect(document.activeElement).not.toBe(readButton()); // no default focus on the read button
    expect(folderBox()).toHaveAttribute("autocomplete", "off");
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
  });

  it("an error from the backend is shown as a fixed sentence chosen by its code, never its message", async () => {
    const secret = "ATM_SENTINEL_0badf00d_err";
    nextRead = () => {
      throw { code: "SOMETHING_ELSE", message: `C:\\${secret}` };
    };
    renderPreview();
    await openSection();
    await typeFolder();
    await userEvent.click(readButton());
    expect(await screen.findByText(errorText("SOMETHING_ELSE"))).toBeInTheDocument();
    expect(document.body.textContent).not.toContain(secret);
    expect(reads()).toHaveLength(1); // no automatic retry
  });

  it("a changed-during-read answer is shown with no data and is not retried", async () => {
    nextRead = () => result({ status: "changed_during_read", sizeBytes: null, sha256: null, serverCount: null, reviewedDeclarations: null });
    renderPreview();
    await openSection();
    await readOnce();
    expect(screen.getByText(/okunurken değişti/)).toBeInTheDocument();
    expect(screen.queryByText(/SHA-256 özeti/, { selector: "dt" })).not.toBeInTheDocument();
    await new Promise((r) => setTimeout(r, 50));
    expect(reads()).toHaveLength(1);
  });
});

describe("the pasted-text preview", () => {
  caseIt("F3", "still works when the read is never opened, is declined, fails, or succeeds", async () => {
    const paste = async (value: string) => {
      const box = screen.getByRole("textbox", { name: /Dosya metni/ });
      await userEvent.clear(box);
      await userEvent.click(box);
      await userEvent.paste(value);
      await userEvent.click(screen.getByRole("button", { name: /^Değişiklik önizle/ }));
    };
    renderPreview();
    await openPreview();
    await userEvent.click(screen.getByRole("radio", { name: /metnini ben yapıştırıyorum/ }));
    await paste("{}");
    expect(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ })).toBeInTheDocument();
    // decline
    await userEvent.click(screen.getByText(SECTION_TITLE));
    await userEvent.click(declineButton());
    expect(screen.getByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ })).toBeInTheDocument();
    expect(reads()).toHaveLength(0);
    // a failing read
    nextRead = () => {
      throw { code: "PROJECT_NOT_FOUND", message: "x" };
    };
    await userEvent.click(screen.getByText(SECTION_TITLE));
    await typeFolder();
    await userEvent.click(readButton());
    await screen.findByText(errorText("PROJECT_NOT_FOUND"));
    await paste("{ \"mcpServers\": {} }");
    expect(await screen.findByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ })).toBeInTheDocument();
    // a successful read neither pre-fills nor changes the pasted text
    nextRead = () => result();
    await typeFolder();
    await userEvent.click(readButton());
    await screen.findByRole("region", { name: /dosya anlık görüntüsü/ });
    expect(screen.getByRole("textbox", { name: /Dosya metni/ })).toHaveValue("{ \"mcpServers\": {} }");
    expect(screen.getAllByRole("region", { name: /ÖNİZLEME — UYGULANMADI/ })).toHaveLength(1);
  });

  caseIt("U3", "the digest comparison says 'byte for byte the same' only for equal digests and never claims more", async () => {
    const pasted = "{ \"mcpServers\": {} }";
    nextRead = () => result({ sha256: sha256Hex(pasted) });
    renderPreview();
    await openPreview();
    await userEvent.click(screen.getByRole("radio", { name: /metnini ben yapıştırıyorum/ }));
    const box = screen.getByRole("textbox", { name: /Dosya metni/ });
    await userEvent.click(box);
    await userEvent.paste(pasted);
    await userEvent.click(screen.getByText(SECTION_TITLE));
    await readOnce();
    expect(screen.getByText(BRIDGE_EQUAL)).toBeInTheDocument();
    expect(screen.queryByText(BRIDGE_DIFFERENT)).not.toBeInTheDocument();
    // different text: "digests differ", with the caveat, and no claim about the file
    await userEvent.type(box, " ");
    expect(screen.getByText(BRIDGE_DIFFERENT)).toBeInTheDocument();
    expect(screen.queryByText(BRIDGE_EQUAL)).not.toBeInTheDocument();
  });
});

describe("clearing", () => {
  caseIt("F4", "Temizle, a new read, a context change, unmount and the 5-minute timer clear the result; the typed folder is cleared right after the call", async () => {
    const view = renderPreview();
    await openSection();
    await typeFolder();
    await userEvent.click(readButton());
    await screen.findByRole("region", { name: /dosya anlık görüntüsü/ });
    expect(folderBox()).toHaveValue(""); // cleared as soon as the call returned
    // Temizle
    await userEvent.click(screen.getByRole("button", { name: "Temizle" }));
    expect(screen.queryByRole("region", { name: /dosya anlık görüntüsü/ })).not.toBeInTheDocument();
    // a new read replaces the previous one (one region at a time)
    await readOnce();
    await readOnce();
    expect(screen.getAllByRole("region", { name: /dosya anlık görüntüsü/ })).toHaveLength(1);
    // a change of project clears
    view.rerender(<ConfigPreview entry={mcp()} alias="baska-proje" projectName="Başka" platform={WIN} surface={CC} declarations={DECLS} />);
    expect(screen.queryByRole("region", { name: /dosya anlık görüntüsü/ })).not.toBeInTheDocument();
    // a change of operating system and of catalog entry clears
    view.rerender(<ConfigPreview entry={mcp()} alias={ALIAS} projectName="Benim sitem" platform={WIN} surface={CC} declarations={DECLS} />);
    nextRead = () => result({ alias: ALIAS });
    await readOnce();
    view.rerender(<ConfigPreview entry={mcp()} alias={ALIAS} projectName="Benim sitem" platform="macos-arm64" surface={CC} declarations={DECLS} />);
    expect(screen.queryByRole("region", { name: /dosya anlık görüntüsü/ })).not.toBeInTheDocument();
    // closing the section clears too (the section is still open after the context change)
    await readOnce();
    await userEvent.click(screen.getByText(SECTION_TITLE));
    await userEvent.click(screen.getByText(SECTION_TITLE));
    expect(screen.queryByRole("region", { name: /dosya anlık görüntüsü/ })).not.toBeInTheDocument();
    expect(folderBox()).toHaveValue("");
    // unmount: an answer that arrives afterwards is dropped (no state update after unmount, no crash)
    let release: (v: unknown) => void = () => undefined;
    const late = new Promise((r) => {
      release = r;
    });
    nextRead = () => late;
    await typeFolder();
    await userEvent.click(readButton());
    view.unmount();
    release(result());
    await act(async () => {
      await Promise.resolve();
    });
    expect(document.body.textContent ?? "").not.toContain(SNAPSHOT_LABEL);
  });

  it("the result expires after five minutes", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    render(<FileSnapshot alias={ALIAS} projectName="Benim sitem" platform={WIN} surface={CC} entryId="synthetic-mcp" pastedText={null} />);
    await userEvent.click(screen.getByText(SECTION_TITLE));
    await userEvent.type(folderBox(), FOLDER);
    await userEvent.click(readButton());
    await screen.findByRole("region", { name: /dosya anlık görüntüsü/ });
    await act(async () => {
      vi.advanceTimersByTime(SNAPSHOT_TTL_MS - 1000);
    });
    expect(screen.getByRole("region", { name: /dosya anlık görüntüsü/ })).toBeInTheDocument();
    await act(async () => {
      vi.advanceTimersByTime(2000);
    });
    expect(screen.queryByRole("region", { name: /dosya anlık görüntüsü/ })).not.toBeInTheDocument();
  });

  it("an answer for a project the user has since left is dropped", async () => {
    let release: (v: unknown) => void = () => undefined;
    nextRead = () => new Promise((r) => { release = r; });
    const view = renderPreview();
    await openSection();
    await typeFolder();
    await userEvent.click(readButton());
    view.rerender(<ConfigPreview entry={mcp()} alias="baska-proje" projectName="Başka" platform={WIN} surface={CC} declarations={DECLS} />);
    release(result());
    await act(async () => {
      await Promise.resolve();
    });
    expect(screen.queryByRole("region", { name: /dosya anlık görüntüsü/ })).not.toBeInTheDocument();
  });
});

describe("what may be said", () => {
  const forbiddenTokens = ["doğrulandı", "yüklü", "güvenli", "çalışıyor", "bağlı", "yapılandırılmamış", "verified", "loaded", "safe", "connected"];
  // a forbidden word may appear only inside a sentence that denies the claim ("yapılandırılmamış" anlamına gelmez; "güvenli" silinmesi garanti edilmez)
  const DENIED_IN: Record<string, RegExp> = { yapılandırılmamış: /anlamına gelmez/, güvenli: /garanti edilmez/ };
  // "isteğe bağlı" means "optional" here, not "connected"
  const tokens = (text: string) => text.toLowerCase().replaceAll("isteğe bağlı", "").split(/[^\p{L}\p{N}]+/u).filter(Boolean);

  caseIt("F5", "no status, reason or fixed sentence contains a claim of verification, loading, safety, running, connection or 'not configured'", () => {
    for (const sample of ALL_STATUS_SAMPLES) {
      const view = describeResult(sample, ALIAS)!;
      for (const sentence of [view.headline, view.reasonLine ?? "", ...view.declarationLines]) {
        for (const t of tokens(sentence)) {
          if (t in DENIED_IN) expect(sentence, "only inside a sentence that denies it").toMatch(DENIED_IN[t]);
          else expect(forbiddenTokens, `${sample.status}: ${t}`).not.toContain(t);
        }
      }
    }
    for (const sentence of allFixedText()) {
      for (const t of tokens(sentence)) {
        if (t in DENIED_IN) {
          expect(sentence, "only inside a sentence that denies it").toMatch(DENIED_IN[t]);
        } else {
          expect(forbiddenTokens, `${sentence.slice(0, 40)}: ${t}`).not.toContain(t);
        }
      }
    }
  });

  caseIt("F6ui", "absent, denied, unsafe_path and root_not_found never render a verified badge or a not-configured value", async () => {
    for (const status of ["absent", "denied", "unsafe_path", "root_not_found"]) {
      nextRead = () => result({ status, reason: status === "denied" ? "access_denied" : status === "unsafe_path" ? "link" : null, sizeBytes: null, sha256: null, serverCount: null, reviewedDeclarations: null });
      const { unmount } = renderPreview();
      await openSection();
      await readOnce();
      const region = screen.getByRole("region", { name: /dosya anlık görüntüsü/ });
      expect(region.querySelector(".tag, .badge, [class*=ok], [class*=verified]")).toBeNull();
      expect(region.textContent).not.toMatch(/NotConfigured|not_configured/i);
      if (/yapılandırılmamış/i.test(region.textContent ?? "")) expect(region.textContent).toMatch(/“yapılandırılmamış” anlamına gelmez/); // only as a denial
      expect(within(region).queryByText(/SHA-256 özeti/, { selector: "dt" })).not.toBeInTheDocument();
      unmount();
    }
  });

  it("renders the Exa lines as a syntax comparison and the footer under every result", async () => {
    renderPreview();
    await openSection();
    await readOnce();
    const region = screen.getByRole("region", { name: /dosya anlık görüntüsü/ });
    expect(within(region).getByText(/çalıştığı doğrulanmadı/)).toBeInTheDocument();
    expect(within(region).getByText(/yalnızca karşılaştırma; erişilebilirlik denenmedi/)).toBeInTheDocument();
    expect(within(region).getByText(/Okuma anındaki anlık görüntüdür/)).toBeInTheDocument();
    expect(within(region).getByText("2")).toBeInTheDocument(); // the count
    expect(within(region).getByText(/abababababababab…/)).toBeInTheDocument(); // only the first 16 digits
    expect(region.textContent).not.toContain("ab".repeat(32));
  });
});

describe("no new reach in the component", () => {
  caseIt("G4", "the component source contains no console, storage, clipboard, network, window.open, HTML injection, link, export or apply", () => {
    const forbidden = /console\.|localStorage|sessionStorage|indexedDB|navigator\.clipboard|\bfetch\(|XMLHttpRequest|WebSocket|window\.open|dangerouslySetInnerHTML|<a\s|href=|createObjectURL|\bBlob\b|FileReader|showOpenFilePicker|showSaveFilePicker|@tauri-apps|\binvoke\(/;
    expect(snapshotSource).not.toMatch(forbidden);
    expect(snapshotSource).not.toMatch(/(?:>|\s)(Uygula|Kopyala|Dışa aktar|İndir|Kaydet)(?:<|\s)/);
    const permissions: string[] = JSON.parse(capabilitySource).permissions;
    expect(permissions.filter((p) => /mcp|config/.test(p))).toEqual(["allow-read-project-mcp-config"]);
    expect(permissions).toHaveLength(37);
  });

  it("the panel is shown for Claude Code rows only", async () => {
    renderPreview({ surface: AG });
    await openPreview();
    expect(screen.queryByText(SECTION_TITLE)).not.toBeInTheDocument();
  });
});
