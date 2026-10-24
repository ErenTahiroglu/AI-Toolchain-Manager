// PRODUCT-06 - "Kanıtın kaynağı": where the evidence behind a stored status came from (computer-wide or project-specific; exact surface or `local-cli`
// fallback; its own time), shown under the matrix observation AND in the review queue. Real Projects page against the in-memory backend. Mocked IPC and
// jsdom only: no real window, no real database, no real scan. The Rust selection/descriptor is covered by the Rust tests; here its answers are fixtures.
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import originNoteSource from "./components/EvidenceOriginNote.tsx?raw";
import originLogicSource from "./origin.ts?raw";
import { formatTime } from "./labels";
import { ProjectsPage } from "./pages/Projects";
import { createBackend, makeEntry, makeRequest, originOf, pref, type Backend } from "./test/backend";
import type { EvidenceOrigin, Inventory, PreferenceView, ProjectView, RequestStatus, RequestVsObservation } from "./types";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const WIN = "windows-x64";
const MAC = "macos-arm64";
const CC = "claude-code-terminal";
const DESK = "claude-desktop-code";
const BENIM = "benim-sitem";
const DIGER = "diger-proje";
const ev = (platformTarget: string, agentSurface: string) => ({ platformTarget, agentSurface, compatibilityState: "vendor-declared", limitation: "kayıt: vendor-declared" });
const FORBIDDEN = ["run_discovery_scan", "evaluate_task", "set_project_preference", "reset_project_preference", "prepare_install", "confirm_install", "cancel_install", "recover_installations", "plan_install", "get_installed_details", "list_install_states", "preview_backup_cleanup", "set_setting", "set_startup_enabled", "set_catalog_entry_enabled", "add_catalog_entry", "remove_catalog_entry", "preview_github_url", "add_project_reference", "remove_project_reference"];

// the phrases the user reads (key words only, so a reworded sentence that keeps the meaning still passes, and a sentence that flips the meaning does not)
const COMPUTER_WIDE = /Kanıt kapsamı: bilgisayar geneli\. Projeden bağımsız bir kayıttır; bu projeye özgü bir kanıt değildir\./;
const PROJECT_SPECIFIC = /Kanıt kapsamı: bu proje\. Gözlem, yalnızca arka uçta kaydedilen proje takma adı kadarıyla/;
const NOT_AGENT_PROOF = /bir ajanın bu aracı kullandığını ya da kimliğinin onaylandığını göstermez/;
const EXACT = /Ajan yüzeyi: istenen yüzey için kaydedilmiş gözlem \(o yüzeyin bu aracı yapılandırdığı anlamına gelmez\)\./;
const FALLBACK = /Ajan yüzeyi: yedek eşleşme\..*local-cli.*istenen ajan yüzeyinin bu aracı yapılandırdığına kanıt değildir/;
const UNAVAILABLE = /Kanıtın kaynağı belirlenemedi; bilgisayar geneli mi, projeye özel mi olduğu söylenemez\./;
const NO_OBSERVATION = /Bu durumda seçilmiş bir gözlem yok\./;
const POSITIVE_CLAIMS = /kurulu olarak|Kuruldu|çalışıyor|doğrulandı|doğrulanmış olarak|etkinleştirildi|oturum açıldı|başarı|✓|ajanda etkin\b(?! mi)|destekleniyor|desteklenir/i;

const tool = (n: number) => `t${n}`;
const toolName = (n: number) => `Araç ${n}`;
const entries = (count: number) =>
  Array.from({ length: count }, (_, i) => makeEntry(tool(i + 1), { displayName: toolName(i + 1), record: { targetEvidence: [ev(WIN, CC), ev(MAC, CC), ev(WIN, DESK)] } }));
const wish = (n: number, over: Partial<PreferenceView> = {}): PreferenceView =>
  pref({ entryId: tool(n), entryName: toolName(n), agentSurface: CC, platformTarget: WIN, decision: "accepted", materialChange: true, ...over }); // materialChange: listed in the queue
const project = (alias: string, name: string, preferences: PreferenceView[]): ProjectView => ({
  projectId: `id-${alias}`, alias, displayName: name, kind: "reference", localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences,
});
const req = (n: number, status: RequestStatus, origin: RequestVsObservation["origin"] | "default", over: Partial<RequestVsObservation> = {}) =>
  makeRequest({ entryId: tool(n), entryName: toolName(n), projectAlias: BENIM, projectName: "Benim sitem", status, ...(origin === "default" ? {} : { origin }), ...over });
const garbage = (o: object) => o as unknown as RequestVsObservation["origin"];

const calls = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd);
const setWorld = (wishes: PreferenceView[], requests: RequestVsObservation[], extraProjects: ProjectView[] = []) => {
  backend.entries = entries(10);
  backend.projects = [project(BENIM, "Benim sitem", wishes), ...extraProjects];
  backend.inventory = { ...backend.inventory, requests } as Inventory;
};
const open = async (onOpen = vi.fn()) => {
  render(<ProjectsPage handoff={null} onOpenInMatrix={onOpen} />);
  await screen.findByRole("region", { name: "İncelenmesi Gerekenler" });
  await screen.findByRole("region", { name: "Proje Araç Matrisi" });
  return onOpen;
};
const matrixRow = async (name: string, project?: string) => {
  const region = within(await screen.findByRole("region", { name: "Proje Araç Matrisi" }));
  const rows = (await region.findAllByRole("heading", { level: 3, name })).map((h) => h.closest("article") as HTMLElement);
  expect(rows.length).toBeGreaterThan(0);
  return project ? rows.find((r) => within(r).queryByText(project)) ?? rows[0] : rows[0];
};
const originInMatrix = async (name: string) => {
  const section = within(await matrixRow(name)).getByRole("region", { name: /^Son kayıtlı envanter gözlemi/ });
  return within(section).queryByTestId("evidence-origin");
};
const queueItem = async (name: string, alias = "Benim sitem") => {
  const q = within(await screen.findByRole("region", { name: "İncelenmesi Gerekenler" }));
  const button = await q.findByRole("button", { name: new RegExp(`^Matriste incele \\(${name} · [^;]+; ${alias} ·`) });
  return button.closest("li.rq-item") as HTMLElement;
};
const originInQueue = async (name: string, alias?: string) => within(await queueItem(name, alias)).queryByTestId("evidence-origin");
const text = (el: HTMLElement | null) => el?.textContent?.replace(/\s+/g, " ") ?? "";

beforeEach(() => {
  backend = createBackend();
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const COMBOS: { n: number; origin: EvidenceOrigin; project: RegExp; surface: RegExp; notProject: RegExp; notSurface: RegExp }[] = [
  { n: 1, origin: originOf({ projectRelation: "project_specific", surfaceRelation: "exact_surface" }), project: PROJECT_SPECIFIC, surface: EXACT, notProject: COMPUTER_WIDE, notSurface: FALLBACK },
  { n: 2, origin: originOf({ projectRelation: "project_specific", surfaceRelation: "local_cli_fallback" }), project: PROJECT_SPECIFIC, surface: FALLBACK, notProject: COMPUTER_WIDE, notSurface: EXACT },
  { n: 3, origin: originOf({ projectRelation: "computer_wide", surfaceRelation: "exact_surface" }), project: COMPUTER_WIDE, surface: EXACT, notProject: PROJECT_SPECIFIC, notSurface: FALLBACK },
  { n: 4, origin: originOf({ projectRelation: "computer_wide", surfaceRelation: "local_cli_fallback" }), project: COMPUTER_WIDE, surface: FALLBACK, notProject: PROJECT_SPECIFIC, notSurface: EXACT },
];

describe("all four combinations, in the matrix and in the queue, say exactly what the backend said", () => {
  beforeEach(() => {
    setWorld(COMBOS.map((c) => wish(c.n)), COMBOS.map((c) => req(c.n, "absent_current", c.origin)));
  });

  it.each(COMBOS)("tool $n ($origin.projectRelation / $origin.surfaceRelation): matrix", async (c) => {
    await open();
    const note = await originInMatrix(toolName(c.n));
    expect(note).not.toBeNull();
    const t = text(note);
    expect(t).toMatch(c.project);
    expect(t).toMatch(c.surface);
    expect(t).not.toMatch(c.notProject);
    expect(t).not.toMatch(c.notSurface);
    expect(t).toMatch(/Kanıtın kaynağı/);
    expect(t).toMatch(/kayıtlı isteğinizden ve uygulamanın kendi kurulum kaydından ayrıdır/);
  });

  it.each(COMBOS)("tool $n ($origin.projectRelation / $origin.surfaceRelation): queue", async (c) => {
    await open();
    const t = text(await originInQueue(toolName(c.n)));
    expect(t).toMatch(c.project);
    expect(t).toMatch(c.surface);
    expect(t).not.toMatch(c.notProject);
    expect(t).not.toMatch(c.notSurface);
  });

  it("a project-specific note never claims agent activation or identity, a computer-wide note never claims project proof", async () => {
    await open();
    expect(text(await originInMatrix(toolName(1)))).toMatch(NOT_AGENT_PROOF);
    expect(text(await originInMatrix(toolName(3)))).not.toMatch(/bu projeyle ilişkilendirildi/);
    expect(text(await originInMatrix(toolName(4)))).toMatch(/local-cli/);
    expect(text(await originInMatrix(toolName(3)))).not.toMatch(/local-cli/);
  });

  it("the matrix and the queue show the very same note for the same row", async () => {
    await open();
    for (const c of COMBOS) expect(text(await originInQueue(toolName(c.n)))).toBe(text(await originInMatrix(toolName(c.n))));
  });
});

describe("time and age", () => {
  const stamp = "2026-10-24T09:00:00Z";
  it("shows the stored time, the age at reading, and that it is not a live check", async () => {
    setWorld([wish(1)], [req(1, "absent_current", originOf({ observedAt: stamp, ageHours: 7 }))]);
    await open();
    for (const note of [await originInMatrix(toolName(1)), await originInQueue(toolName(1))]) {
      const t = text(note);
      expect(t).toContain(`Gözlem zamanı: ${formatTime(stamp)}`);
      expect(t).toContain("envanter okunduğunda yaklaşık 7 saat önce");
      expect(t).toMatch(/kaydedilmiş bir tarama sonucuna aittir; canlı bir doğrulama değildir/);
    }
  });

  it("an invalid timestamp, a missing age, a negative age: nothing is made up", async () => {
    setWorld(
      [wish(1), wish(2), wish(3), wish(4)],
      [
        req(1, "absent_current", originOf({ observedAt: null, ageHours: null })),
        req(2, "absent_current", originOf({ observedAt: "dün akşam", ageHours: 5 })),
        req(3, "absent_current", originOf({ observedAt: stamp, ageHours: null })),
        req(4, "absent_current", originOf({ observedAt: stamp, ageHours: -4 })),
      ],
    );
    await open();
    for (const n of [1, 2]) {
      for (const note of [await originInMatrix(toolName(n)), await originInQueue(toolName(n))]) {
        expect(text(note), `tool ${n}`).not.toMatch(/Gözlem zamanı|saat önce|dün akşam/);
        expect(text(note)).toMatch(COMPUTER_WIDE); // the rest of the descriptor is still shown
      }
    }
    for (const n of [3, 4]) {
      for (const note of [await originInMatrix(toolName(n)), await originInQueue(toolName(n))]) {
        expect(text(note), `tool ${n}`).toContain(`Gözlem zamanı: ${formatTime(stamp)}`);
        expect(text(note), `tool ${n}`).not.toMatch(/saat önce|-4/);
      }
    }
  });
});

describe("unavailable provenance is stated, never guessed", () => {
  it("a null origin, an origin from a newer backend, a missing member: 'kaynağı belirlenemedi' and no scope or surface sentence", async () => {
    setWorld(
      [wish(1), wish(2), wish(3), wish(4)],
      [
        req(1, "absent_current", null),
        req(2, "absent_current", garbage({ projectRelation: "organisation_wide", surfaceRelation: "exact_surface", observedAt: null, ageHours: null })),
        req(3, "absent_current", garbage({ projectRelation: "computer_wide" })),
        req(4, "absent_current", "default", { origin: undefined } as unknown as Partial<RequestVsObservation>), // an older backend: the member is absent altogether
      ],
    );
    await open();
    for (const n of [1, 2, 3, 4]) {
      for (const note of [await originInMatrix(toolName(n)), await originInQueue(toolName(n))]) {
        const t = text(note);
        expect(t, `tool ${n}`).toMatch(UNAVAILABLE);
        expect(t, `tool ${n}`).not.toMatch(/Kanıt kapsamı:|Ajan yüzeyi:|Gözlem zamanı/);
        expect(t, `tool ${n}`).not.toMatch(NO_OBSERVATION); // there is a basis scan, so this is not the "no observation" case
      }
    }
  });

  it("not_scanned and platform_mismatch (no selected observation): the same words plus the reason, and no scope", async () => {
    setWorld([wish(1), wish(2)], [req(1, "not_scanned", null), req(2, "platform_mismatch", null)]);
    await open();
    for (const n of [1, 2]) {
      for (const note of [await originInMatrix(toolName(n)), await originInQueue(toolName(n))]) {
        const t = text(note);
        expect(t, `tool ${n}`).toMatch(UNAVAILABLE);
        expect(t, `tool ${n}`).toMatch(NO_OBSERVATION);
        expect(t, `tool ${n}`).not.toMatch(/Kanıt kapsamı:|Ajan yüzeyi:|Gözlem zamanı/);
      }
    }
  });
});

describe("old, ambiguous, missing and unrecognised results keep the PRODUCT-04/05 handling", () => {
  it("a historical result still reads as old, and its origin describes that old observation without making it current", async () => {
    setWorld([wish(1)], [req(1, "previously_present", originOf({ projectRelation: "project_specific", observedAt: "2026-10-01T00:00:00Z", ageHours: 552 }))]);
    await open();
    const row = within(await matrixRow(toolName(1)));
    expect(row.getByText("Eski taramada bulunmuştu (güncel değil)", { selector: "strong" })).toBeInTheDocument();
    const t = text(await originInMatrix(toolName(1)));
    expect(t).toMatch(PROJECT_SPECIFIC);
    expect(t).toContain("envanter okunduğunda yaklaşık 552 saat önce");
    expect(text(await originInQueue(toolName(1)))).toBe(t);
    expect(within(await queueItem(toolName(1))).getByText(/Kayıtlı gözlem: Eski taramada bulunmuştu \(güncel değil\)/)).toBeInTheDocument();
  });

  it("two rows for one key (ambiguous): neither origin is shown, in the matrix or in the queue", async () => {
    setWorld(
      [wish(1)],
      [req(1, "absent_current", originOf({ projectRelation: "project_specific" })), req(1, "absent_current", originOf({ projectRelation: "computer_wide" }))],
    );
    await open();
    expect(await originInMatrix(toolName(1))).toBeNull();
    expect(screen.queryByTestId("evidence-origin")).not.toBeInTheDocument();
    expect(screen.queryByText(/Kanıt kapsamı:/)).not.toBeInTheDocument();
  });

  it("no matching row, and an unrecognised status: no origin is rendered even if the row carries one", async () => {
    setWorld([wish(1), wish(2)], [req(2, "absent_current", originOf({ projectRelation: "project_specific" }), { status: "future_status" as RequestStatus })]);
    await open();
    expect(await originInMatrix(toolName(1))).toBeNull(); // no row for the exact key
    expect(await originInMatrix(toolName(2))).toBeNull(); // not a known status: not a success, nothing inferred
    expect(screen.queryByText(/Kanıt kapsamı:/)).not.toBeInTheDocument();
  });

  it("a decision that is not an accepted wish has no comparison and no origin", async () => {
    setWorld([wish(1, { decision: "rejected" })], [req(1, "absent_current", originOf())]);
    await open();
    expect(await originInMatrix(toolName(1))).toBeNull();
  });
});

describe("the exact four-field key: no leak across project, platform or agent surface", () => {
  it("two projects asking for the same tool each get only their own row's origin", async () => {
    const other = project(DIGER, "Diğer proje", [wish(1)]);
    setWorld(
      [wish(1)],
      [
        req(1, "absent_current", originOf({ projectRelation: "project_specific", surfaceRelation: "exact_surface" })),
        req(1, "absent_current", originOf({ projectRelation: "computer_wide", surfaceRelation: "local_cli_fallback" }), { projectAlias: DIGER, projectName: "Diğer proje" }),
      ],
      [other],
    );
    await open();
    const mine = text(await originInQueue(toolName(1), "Benim sitem"));
    const theirs = text(await originInQueue(toolName(1), "Diğer proje"));
    expect(mine).toMatch(PROJECT_SPECIFIC);
    expect(mine).toMatch(EXACT);
    expect(mine).not.toMatch(COMPUTER_WIDE);
    expect(theirs).toMatch(COMPUTER_WIDE);
    expect(theirs).toMatch(FALLBACK);
    expect(theirs).not.toMatch(PROJECT_SPECIFIC);
  });

  it("a row for another platform or another agent surface is never shown for this one", async () => {
    setWorld(
      [wish(1), wish(2, { agentSurface: DESK }), wish(3, { platformTarget: MAC })],
      [
        req(1, "absent_current", originOf({ projectRelation: "computer_wide" }), { platformTarget: MAC }), // wrong platform for wish 1
        req(2, "absent_current", originOf({ projectRelation: "computer_wide" })), // wrong surface for wish 2 (row is for CC)
        req(3, "absent_current", originOf({ projectRelation: "project_specific" })), // wrong platform for wish 3 (row is for WIN)
      ],
    );
    await open();
    // the three wishes have no row for their own key: the queue says the evidence is unknown, it does not borrow a neighbour's origin
    expect(screen.queryByText(/Kanıt kapsamı:/)).not.toBeInTheDocument();
    expect(screen.queryByTestId("evidence-origin")).not.toBeInTheDocument();
  });
});

describe("reading and opening is read-only", () => {
  it("no write, scan, install or evaluate command is called by rendering, by opening a queue item, or by expanding details", async () => {
    setWorld(COMBOS.map((c) => wish(c.n)), COMBOS.map((c) => req(c.n, "absent_current", c.origin)));
    const onOpen = await open();
    await userEvent.click(await within(await queueItem(toolName(1))).findByRole("button", { name: /^Matriste incele/ }));
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(onOpen).toHaveBeenCalledWith({ alias: BENIM, platform: WIN, surface: CC, entryId: "t1" }); // navigation only: the exact target, nothing else
    for (const bad of FORBIDDEN) expect(calls(bad), bad).toHaveLength(0);
    expect(backend.calls.filter((c) => /scan|install|save|write|export|set_|add_|remove_|reset_|confirm|prepare|evaluate/i.test(c.cmd)), "no command that could change anything").toEqual([]);
    expect(calls("list_inventory").length).toBeGreaterThan(0); // the stored read it relies on
  });

  it("the new modules contain no IPC, timer, storage or network access", () => {
    for (const source of [originNoteSource, originLogicSource]) {
      expect(source).not.toMatch(/invoke\(|@tauri-apps|fetch\(|XMLHttpRequest|setInterval|setTimeout|localStorage|sessionStorage|WebSocket/);
    }
  });
});

describe("what the note never says", () => {
  it("no visible origin text, for any combination or for unavailable provenance, makes a positive claim", async () => {
    setWorld(
      [...COMBOS.map((c) => wish(c.n)), wish(5), wish(6)],
      [...COMBOS.map((c) => req(c.n, "absent_current", c.origin)), req(5, "absent_current", null), req(6, "not_scanned", null)],
    );
    await open();
    for (const n of [1, 2, 3, 4, 5, 6]) {
      for (const note of [await originInMatrix(toolName(n)), await originInQueue(toolName(n))]) expect(text(note), `tool ${n}`).not.toMatch(POSITIVE_CLAIMS);
    }
  });

  it("fields the wire may carry beyond the four (a path, raw evidence, a version, a fingerprint) never reach the screen", async () => {
    const leaky = garbage({ ...originOf(), location: "C:\\Users\\GİZLİ\\araç.exe", evidence: "HAM-KANIT", version: "9.9.9-GİZLİ", fingerprint: "ABCDEF-GİZLİ", scope: "global" });
    setWorld([wish(1)], [req(1, "absent_current", leaky)]);
    await open();
    const page = document.body.textContent ?? "";
    expect(page).not.toMatch(/GİZLİ|HAM-KANIT|9\.9\.9|ABCDEF/);
    expect(text(await originInMatrix(toolName(1)))).toMatch(COMPUTER_WIDE); // the descriptor itself is still read
  });
});
