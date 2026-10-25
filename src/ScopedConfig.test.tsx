// PRODUCT-07 H - the scoped MCP-configuration block beside the stored presence result, in the real matrix and the real review queue, against the in-memory
// backend. Mocked IPC and jsdom only: no real window, no real database, no real scan. The Rust derivation is covered by the Rust tests; here its answers are fixtures.
import { render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ScopedConfigNote } from "./components/ScopedConfigNote";
import { ProjectsPage } from "./pages/Projects";
import { deriveReview, reviewHeadline } from "./review";
import { COVERAGE_TEXT, CONFIGURATION_TEXT, NOT_IMPLEMENTED_TEXT, PRESENCE_LABEL, SCOPED_DISCLAIMER, SCOPED_HEADING, SCOPED_NOT_READ_NOTE, SCOPED_SEPARATE } from "./scopedDeclarations";
import { createBackend, makeEntry, makeRequest, originOf, pref, type Backend } from "./test/backend";
import type { Inventory, PreferenceView, ProjectView, RequestStatus, RequestVsObservation } from "./types";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const WIN = "windows-x64";
const MAC = "macos-arm64";
const CC = "claude-code-terminal";
const BENIM = "benim-sitem";
const T0 = "2026-10-23T09:00:00Z";
const ev = (platformTarget: string, agentSurface: string) => ({ platformTarget, agentSurface, compatibilityState: "vendor-declared", limitation: "kayıt: vendor-declared" });
const FORBIDDEN = ["run_discovery_scan", "evaluate_task", "set_project_preference", "reset_project_preference", "prepare_install", "confirm_install", "cancel_install", "recover_installations", "plan_install", "set_setting", "add_project_reference", "remove_project_reference"];

const NONE = { project: "no_record", local: "no_record", global: "no_record" };
const reading = (scope: string, over: Record<string, unknown> = {}) => ({ scope, configuration: "configured", freshness: "current", observedAt: T0, ageHours: 3, ...over });
/** The two members PRODUCT-07 H adds to a request, in the wire shape of declarations.rs. */
const scoped = (readings: unknown[], coverage: Record<string, string> = NONE) => ({
  declarationDetector: { kind: "claude_mcp", coverage: { ...coverage, ...Object.fromEntries((readings as { scope: string }[]).map((r) => [r.scope, "read"])) } },
  declarations: readings,
});
const notImplemented = () => ({ declarationDetector: { kind: "not_implemented", coverage: null }, declarations: [] });

const tool = (n: number) => `t${n}`;
const toolName = (n: number) => `Araç ${n}`;
const entries = (count: number) => Array.from({ length: count }, (_, i) => makeEntry(tool(i + 1), { displayName: toolName(i + 1), record: { targetEvidence: [ev(WIN, CC), ev(MAC, CC)] } }));
const wish = (n: number, over: Partial<PreferenceView> = {}): PreferenceView => pref({ entryId: tool(n), entryName: toolName(n), agentSurface: CC, platformTarget: WIN, decision: "accepted", ...over });
const project = (preferences: PreferenceView[]): ProjectView => ({ projectId: `id-${BENIM}`, alias: BENIM, displayName: "Benim sitem", kind: "reference", localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences });
const req = (n: number, status: RequestStatus, over: Partial<RequestVsObservation> & Record<string, unknown> = {}) =>
  makeRequest({ entryId: tool(n), entryName: toolName(n), projectAlias: BENIM, projectName: "Benim sitem", status, ...over });

const calls = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd);
const setWorld = (wishes: PreferenceView[], requests: RequestVsObservation[]) => {
  backend.entries = entries(6);
  backend.projects = [project(wishes)];
  backend.inventory = { ...backend.inventory, requests } as Inventory;
};
const open = async () => {
  render(<ProjectsPage handoff={null} onOpenInMatrix={vi.fn()} />);
  await screen.findByRole("region", { name: "İncelenmesi Gerekenler" });
  await screen.findByRole("region", { name: "Proje Araç Matrisi" });
};
const observation = async (name: string) => {
  const region = within(await screen.findByRole("region", { name: "Proje Araç Matrisi" }));
  const row = (await region.findAllByRole("heading", { level: 3, name }))[0].closest("article") as HTMLElement;
  return within(within(row).getByRole("region", { name: /^Son kayıtlı envanter gözlemi/ }));
};
const text = (el: HTMLElement | null) => el?.textContent?.replace(/\s+/g, " ") ?? "";

let writeText: ReturnType<typeof vi.fn>;
let fetchSpy: ReturnType<typeof vi.fn>;
beforeEach(() => {
  backend = createBackend();
  writeText = vi.fn();
  fetchSpy = vi.fn();
  Object.defineProperty(navigator, "clipboard", { value: { writeText, readText: vi.fn() }, configurable: true });
  vi.stubGlobal("fetch", fetchSpy);
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("in the matrix: two separate blocks, the stored presence result unchanged", () => {
  it("a program found by name and a project file without a matching definition are both shown, labelled, neither hiding the other, and not called a conflict", async () => {
    setWorld([wish(1)], [req(1, "present_current", { origin: originOf({ method: "path-executable", surfaceRelation: "local_cli_fallback" }), ...scoped([reading("project", { configuration: "not_configured" }), reading("global", { configuration: "not_configured" })], { local: "no_record" }) })]);
    await open();
    const o = await observation(toolName(1));
    // (a) the stored presence result: label + unchanged headline + unchanged backend note
    expect(o.getByText(PRESENCE_LABEL)).toBeInTheDocument();
    expect(o.getByText("Son taramada bulundu")).toBeInTheDocument();
    expect(o.getByText(/Son taramada bu bilgisayarda bulundu \(bulunması, çalıştığı veya projede etkin olduğu anlamına gelmez\)\./)).toBeInTheDocument();
    // (b) the separate scoped block
    const block = o.getByRole("region", { name: SCOPED_HEADING });
    const b = within(block);
    expect(b.getByText(SCOPED_SEPARATE)).toBeInTheDocument();
    const lines = b.getAllByRole("listitem").map((li) => text(li));
    expect(lines).toHaveLength(3);
    expect(lines[0]).toMatch(/^Bu projenin \.mcp\.json dosyası: Son taramada okundu: araçla eşleşen bir MCP tanımı bulunamadı \(yalnızca bu kapsamda\)\./);
    expect(lines[1]).toMatch(/^Bu proje için yerel Claude Code ayarı.*: Bu kapsam için kayıtlı bir okuma yok\./);
    expect(lines[2]).toMatch(/^Kullanıcı geneli Claude Code ayarı.*tüm projelere uygulanır\): Son taramada okundu: araçla eşleşen bir MCP tanımı bulunamadı/);
    expect(b.getByText(SCOPED_DISCLAIMER)).toBeInTheDocument();
    expect(b.getByText(SCOPED_NOT_READ_NOTE)).toBeInTheDocument();
    // no conflict banner, no alarm styling, no badge, no icon
    expect(block.querySelector(".bad-text")).toBeNull();
    expect(within(block).queryByRole("alert")).toBeNull();
    expect(within(block).queryByRole("img")).toBeNull();
    expect(text(block)).not.toMatch(/çelişki|çatışma|uyuşmazlık|\bhata\b|[✓✕⚠⛔]/);
    // the origin block names the technique of the presence winner
    expect(text(o.getByTestId("evidence-origin"))).toMatch(/Kanıt türü: PATH klasörlerinde dosya adı eşleşmesi \(bilgisayar düzeyinde bir arama/);
  });

  it("a missing program and a configured project file are both shown (the reverse coexistence)", async () => {
    setWorld([wish(1)], [req(1, "absent_current", scoped([reading("project", { configuration: "configured" })]))]);
    await open();
    const o = await observation(toolName(1));
    expect(o.getByText("Son taramada bulunamadı")).toBeInTheDocument();
    const lines = within(o.getByRole("region", { name: SCOPED_HEADING })).getAllByRole("listitem").map((li) => text(li));
    expect(lines[0]).toContain(CONFIGURATION_TEXT.configured);
    expect(text(o.getByRole("region", { name: SCOPED_HEADING }))).toMatch(/kurulu, onaylı, yüklenmiş, çalışan ya da doğrulanmış olduğunu göstermez/);
  });

  it("one scope 'not configured' and another 'configured' stay two separate lines; nothing is aggregated into 'not configured anywhere'", async () => {
    setWorld([wish(1)], [req(1, "unknown_current", scoped([reading("project", { configuration: "not_configured" }), reading("local", { configuration: "configured" }), reading("global", { configuration: "not_configured" })]))]);
    await open();
    const o = await observation(toolName(1));
    const lines = within(o.getByRole("region", { name: SCOPED_HEADING })).getAllByRole("listitem").map((li) => text(li));
    expect(lines[0]).toContain(CONFIGURATION_TEXT.not_configured);
    expect(lines[1]).toContain(CONFIGURATION_TEXT.configured);
    expect(lines[2]).toContain(CONFIGURATION_TEXT.not_configured);
    expect(text(o.getByRole("region", { name: SCOPED_HEADING }))).not.toMatch(/hiçbir kapsamda|hiçbir yerde|her yerde|tamamen/);
  });

  it("an older reading is labelled old (with its reason class), never shown as today's state, even when it says configured", async () => {
    setWorld([wish(1)], [req(1, "present_current", scoped([reading("project", { freshness: "previous_area_not_checked", configuration: "configured" }), reading("global", { freshness: "previous_stale", configuration: "not_configured" })]))]);
    await open();
    const o = await observation(toolName(1));
    const lines = within(o.getByRole("region", { name: SCOPED_HEADING })).getAllByRole("listitem").map((li) => text(li));
    expect(lines[0]).toMatch(/: Eski bir taramadan \(güncel değil; son taramada bu alana bakılmadı\): araçla eşleşen bir MCP tanımı kaydedilmiş/);
    expect(lines[2]).toMatch(/: Eski bir taramadan \(güncel değil; son tarama 24 saatten eski\):/);
    expect(lines.join(" ")).not.toMatch(/Son taramada okundu/);
  });

  it("a scope with no reading says why it was not read and never says 'no definition'", async () => {
    setWorld([wish(1)], [req(1, "present_current", scoped([], { project: "not_authorized", local: "project_not_in_latest_scan", global: "no_scan" }))]);
    await open();
    const o = await observation(toolName(1));
    const lines = within(o.getByRole("region", { name: SCOPED_HEADING })).getAllByRole("listitem").map((li) => text(li));
    expect(lines[0]).toContain(COVERAGE_TEXT.not_authorized);
    expect(lines[1]).toContain(COVERAGE_TEXT.project_not_in_latest_scan);
    expect(lines[2]).toContain(COVERAGE_TEXT.no_scan);
    expect(lines.join(" ")).not.toMatch(/MCP tanımı bulunamadı|kaydedilmemiş/);
  });

  it("an entry or surface without a reader says exactly that, and nothing else", async () => {
    setWorld([wish(1)], [req(1, "present_current", notImplemented())]);
    await open();
    const o = await observation(toolName(1));
    const b = within(o.getByRole("region", { name: SCOPED_HEADING }));
    expect(b.getByText(NOT_IMPLEMENTED_TEXT)).toBeInTheDocument();
    expect(b.queryByRole("list")).toBeNull();
    expect(b.queryByText(SCOPED_DISCLAIMER)).toBeNull();
  });

  it("an older backend (no new members) looks exactly as before: no scoped block and no coverage claim", async () => {
    setWorld([wish(1)], [req(1, "present_current")]);
    await open();
    const o = await observation(toolName(1));
    expect(o.getByText("Son taramada bulundu")).toBeInTheDocument();
    expect(o.queryByRole("region", { name: SCOPED_HEADING })).toBeNull();
    expect(o.queryByTestId("scoped-config")).toBeNull();
    expect(text(o.getByTestId("evidence-origin"))).not.toMatch(/Kanıt türü/);
  });

  it("a payload that is not believed (unknown value, wrong order, detector on another surface) shows nothing", async () => {
    setWorld(
      [wish(1), wish(2), wish(3)],
      [
        req(1, "present_current", scoped([reading("project", { configuration: "disabled" })])),
        req(2, "present_current", scoped([reading("global"), reading("project")])),
        req(3, "present_current", { agentSurface: "antigravity-ide", ...scoped([reading("project")]) }),
      ],
    );
    await open();
    expect((await observation(toolName(1))).queryByTestId("scoped-config")).toBeNull();
    expect((await observation(toolName(2))).queryByTestId("scoped-config")).toBeNull();
  });

  it("plugin presence from the user cache is explained as such (D7) while the stored origin sentences stay as they were", async () => {
    setWorld([wish(1)], [req(1, "present_current", { origin: originOf({ projectRelation: "project_specific", surfaceRelation: "exact_surface", method: "claude-plugin-declaration" }), ...notImplemented() })]);
    await open();
    const o = await observation(toolName(1));
    const origin = text(o.getByTestId("evidence-origin"));
    expect(origin).toMatch(/Kanıt kapsamı: bu proje\. Gözlem, yalnızca arka uçta kaydedilen proje takma adı kadarıyla/); // the stored-origin sentence is untouched
    expect(origin).toMatch(/Kanıt türü: Claude Code eklenti kaydı ve önbelleği \(kullanıcı düzeyinde\)/);
    expect(origin).toMatch(/yalnızca hangi ayar kapsamının denetlendiğini gösterir/);
    expect(origin).toMatch(/projeye özel bir kurulum değildir/);
    expect(o.getByText(/Son taramada bu bilgisayarda bulundu/)).toBeInTheDocument(); // the backend note is untouched
  });

  it("reading the matrix makes no write, scan, install, clipboard or network call", async () => {
    setWorld([wish(1)], [req(1, "present_current", scoped([reading("project", { configuration: "not_configured" })]))]);
    await open();
    await observation(toolName(1));
    for (const bad of FORBIDDEN) expect(calls(bad), bad).toHaveLength(0);
    await waitFor(() => expect(backend.calls.every((c) => /^(list_|get_)/.test(c.cmd))).toBe(true));
    expect(writeText).not.toHaveBeenCalled();
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

describe("in the review queue: display only - no new reason, no changed count, no new status", () => {
  const readQueue = (requests: RequestVsObservation[], wishes: PreferenceView[]) =>
    deriveReview({ data: [project(wishes)], error: null, loading: false }, { data: { ...backend.inventory, requests } as Inventory, error: null, loading: false }, Date.UTC(2026, 9, 24));

  it("reasons, counts and the headline are identical with and without the new members, for every status and every kind of reading", () => {
    const statuses: RequestStatus[] = ["present_current", "absent_current", "unknown_current", "previously_present", "previously_absent", "previously_unknown", "platform_mismatch", "not_scanned"];
    const variants = [
      {},
      scoped([reading("project", { configuration: "not_configured" })]),
      scoped([reading("project"), reading("local"), reading("global", { freshness: "previous_stale" })]),
      scoped([], { project: "not_authorized", local: "no_scan", global: "platform_not_scanned" }),
      notImplemented(),
      { declarationDetector: { kind: "nonsense" }, declarations: "nonsense" },
    ];
    for (const status of statuses) {
      const wishes = [wish(1, { materialChange: false })];
      const base = readQueue([req(1, status)], wishes);
      for (const v of variants) {
        const q = readQueue([req(1, status, v)], wishes);
        expect(q.items.map((i) => i.reasons), `${status}`).toEqual(base.items.map((i) => i.reasons));
        expect(q.uncertain.length, `${status}`).toBe(base.uncertain.length);
        expect(reviewHeadline(q), `${status}`).toBe(reviewHeadline(base));
        expect(q.items.map((i) => i.key)).toEqual(base.items.map((i) => i.key));
      }
    }
  });

  it("the scoped members are carried only when they were sent, so an old payload yields exactly the old item", () => {
    const wishes = [wish(1, { materialChange: true })];
    const old = readQueue([req(1, "present_current")], wishes).items[0].observation!;
    expect("scoped" in old).toBe(false);
    const fresh = readQueue([req(1, "present_current", scoped([reading("project")]))], wishes).items[0].observation!;
    expect(fresh.scoped).toBeDefined();
    expect({ status: fresh.status, scanFinishedAt: fresh.scanFinishedAt, origin: fresh.origin }).toEqual({ status: old.status, scanFinishedAt: old.scanFinishedAt, origin: old.origin });
  });

  it("a queue item shows the same block, and nothing in it is a button", async () => {
    setWorld([wish(1, { materialChange: true })], [req(1, "present_current", scoped([reading("project", { configuration: "not_configured" })]))]);
    await open();
    const q = within(await screen.findByRole("region", { name: "İncelenmesi Gerekenler" }));
    const block = await q.findByRole("region", { name: SCOPED_HEADING });
    expect(within(block).getAllByRole("listitem")).toHaveLength(3);
    expect(within(block).queryByRole("button")).toBeNull();
    // the only action of the item is still "Matriste incele"
    expect(q.getAllByRole("button", { name: /^Matriste incele/ })).toHaveLength(1);
  });
});

describe("the block on its own", () => {
  const surfaceReq = (surface: string, extra: object) => ({ agentSurface: surface, ...extra });

  it("renders nothing for an older or untrusted payload and a labelled region with list semantics otherwise", () => {
    const { container, rerender } = render(<ScopedConfigNote request={{ agentSurface: CC }} />);
    expect(container).toBeEmptyDOMElement();
    rerender(<ScopedConfigNote request={surfaceReq(CC, scoped([reading("project")]))} />);
    expect(screen.getByRole("region", { name: SCOPED_HEADING })).toBeInTheDocument();
    expect(screen.getByRole("list", { name: "Kapsamlara göre MCP tanımı okumaları" })).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
    expect(screen.getByRole("heading", { level: 5, name: SCOPED_HEADING })).toBeInTheDocument();
    expect(screen.queryByRole("link")).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it.each(["codex-cli", "antigravity-ide", "claude-desktop-code", "local-cli"])("%s: states that nothing is read, never claims a scan, a reading or support", (surface) => {
    render(<ScopedConfigNote request={surfaceReq(surface, notImplemented())} />);
    const region = screen.getByRole("region", { name: SCOPED_HEADING });
    expect(within(region).getByText(NOT_IMPLEMENTED_TEXT)).toBeInTheDocument();
    expect(within(region).queryByRole("list")).toBeNull();
    expect(text(region)).not.toMatch(/Son taramada okundu|araçla eşleşen/);
    if (surface === "codex-cli") expect(text(region)).toMatch(/Codex'e özgü değildir/);
  });

  it("a long Turkish sentence wraps inside the block instead of widening the page", () => {
    render(<ScopedConfigNote request={surfaceReq(CC, scoped([reading("global", { freshness: "previous_area_not_checked" })]))} />);
    const region = screen.getByRole("region", { name: SCOPED_HEADING });
    expect(region.className).toContain("ws-scoped");
  });
});
