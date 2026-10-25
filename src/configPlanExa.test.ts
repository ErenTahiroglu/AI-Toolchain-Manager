// CONFIG-02A - the first USABLE preview: the real `exa-mcp-server` catalog entry on Claude Code's project `.mcp.json`, through the application's OWN shipped registry
// (no injected declaration). Pure and deterministic: nothing is read, written, connected or executed; every "secret" is an obviously fake sentinel.
import { describe, expect, it } from "vitest";
import sourceRegistry from "./configDeclarations.ts?raw";
import { REVIEWED_DECLARATIONS } from "./configDeclarations";
import {
  DRAFT_NOTICE, HOSTS, hostForSurface, isPlanStale, parseConfigText, planConfigPreview, planInputKey, validateDeclaration, type PlanRequest, type PreviewPlan,
} from "./configPlan";
import { EXA_WRAPPER, exaEntry } from "./test/exaEntry";

const WIN = "windows-x64";
const MAC = "macos-arm64";
const CC = "claude-code-terminal";
const SENTINEL = "ghp_SENTINELSENTINELSENTINELSENTINEL12";
const KEY = "exa-SENTINEL-KEY-0123456789abcdef";
const EXPECTED_URL = "https://mcp.exa.ai/mcp";
const PINNED = "0b499f095c4ca145eee07b09137641c037d58449";

const req = (over: Partial<PlanRequest> = {}): PlanRequest => ({ alias: "benim-sitem", platform: WIN, surface: CC, entry: exaEntry(), source: { kind: "new" }, ...over });
const text = (t: string): PlanRequest["source"] => ({ kind: "text", text: t });
const plan = (over: Partial<PlanRequest> = {}): PreviewPlan => planConfigPreview(req(over)); // the SHIPPED registry
const everything = (p: PreviewPlan) => JSON.stringify(p);

describe("the entry is the real catalog record, and the shipped declaration is bound to it", () => {
  it("the data file's record is the one the declaration was reviewed against (same repository, same pinned commit)", () => {
    expect(EXA_WRAPPER.record.repository.url).toBe("https://github.com/exa-labs/exa-mcp-server");
    expect(EXA_WRAPPER.record.sourceSnapshot.immutableCommit).toBe(PINNED);
    const [d] = REVIEWED_DECLARATIONS;
    expect(d.review.reviewedAgainst).toBe(PINNED);
    expect(d.review.repositoryUrl).toBe(EXA_WRAPPER.record.repository.url);
    expect(EXA_WRAPPER.record.category).toBe("mcp-server");
    expect(EXA_WRAPPER.record.reviewState).toBe("unreviewed"); // not promoted: the preview does not change the catalog's own state
  });

  it("the declaration is exactly {type: http, url: <the publisher's endpoint>} - nothing else, in a fixed order - and passes the hygiene rules", () => {
    expect(REVIEWED_DECLARATIONS).toHaveLength(1);
    const [d] = REVIEWED_DECLARATIONS;
    expect(JSON.stringify(d.declaration)).toBe(JSON.stringify({ type: "http", url: EXPECTED_URL }));
    expect(d).toMatchObject({ hostId: "claude-code-project-mcp", entryId: "exa-mcp-server", serverName: "exa" });
    expect(validateDeclaration(HOSTS[0], d)).toEqual([]);
    const url = new URL(d.declaration.url as string);
    expect([url.protocol, url.host, url.pathname, url.search, url.hash, url.username, url.password]).toEqual(["https:", "mcp.exa.ai", "/mcp", "", "", "", ""]);
  });

  it("carries a review note that says what it is and is not, and warnings about cost, privacy, authentication and source limits", () => {
    const [d] = REVIEWED_DECLARATIONS;
    expect(d.review.note).toMatch(/Yalnızca sözdizimi, yalnızca önizleme için gözden geçirildi; çalışma zamanı doğrulanmadı/);
    expect(d.review.note).toMatch(/bağımsız inceleme bekliyor/);
    const w = (d.warnings ?? []).join(" ");
    expect(w).toMatch(/üçüncü taraf bir hizmete gider/);
    expect(w).toMatch(/ücret, kullanım sınırı ve mahremiyet/);
    expect(w).toMatch(/anahtar, belirteç, başlık ya da sorgu parametresi içermez/);
    expect(w).toMatch(/doğrulanmadı ve bu uygulama ona bağlanmaz/);
    expect(w).toMatch(/streamable-http/);
  });

  it("the registry source contains no I/O, no key and no query string", () => {
    expect(sourceRegistry).not.toMatch(/fetch\(|invoke\(|localStorage|XMLHttpRequest|exaApiKey=|x-api-key|Bearer [A-Za-z0-9]|\?tools=/);
    expect(sourceRegistry.match(/^import .*$/gm)).toEqual(['import type { HostId, JsonValue } from "./configPlan";']);
  });
});

describe("a real catalog entry yields a positive, inert PREVIEW_ONLY_NOT_APPLIED - on both operating systems, with unknown compatibility", () => {
  it.each([WIN, MAC])("%s: explicit new file", (platform) => {
    const p = plan({ platform });
    expect(p.status).toBe("PREVIEW_ONLY_NOT_APPLIED");
    expect(p.applied).toBe(false);
    expect(p.touchedFiles).toBe(0);
    expect(p.hypothetical).toBe(false);
    expect(p.blockers).toEqual([]);
    expect(p.problems).toEqual([]);
    expect(p.relativePath).toBe(".mcp.json");
    expect(p.identity).toEqual({ alias: "benim-sitem", platform, surface: CC, entryId: "exa-mcp-server", hostId: "claude-code-project-mcp" });
    expect(JSON.parse(p.proposed!)).toEqual({ mcpServers: { exa: { type: "http", url: EXPECTED_URL } } });
    expect(p.original).toBeNull();
    expect(p.diff!.every((l) => l.kind === "add")).toBe(true);
    expect(p.diff!.map((l) => l.text).join("\n")).toContain(`"url": "${EXPECTED_URL}"`);
    expect(p.draftNotice).toBe("Yalnızca yapılandırma taslağı; uygulanmadı, bağlantı kurulmadı. Hizmetin ücretleri, gizliliği ve kimlik doğrulaması ayrıca değerlendirilmelidir.");
    expect(p.draftNotice).toBe(DRAFT_NOTICE);
    expect(p.declaration).toMatchObject({ serverName: "exa", reviewId: "SRC-CONFIG-02A-EXA-1", evidenceIds: ["R37", "R55", "R56"] });
    expect(p.declaration!.note).toMatch(/çalışma zamanı doğrulanmadı/);
    const c = p.constraints.join(" ");
    expect(c).toMatch(/Uyumluluk \(bu hedef için kayıt\): unknown/); // the catalog still says unknown
    expect(c).toMatch(/Bu bir yapılandırma TASLAĞIDIR/);
    expect(c).toMatch(/“-p” ve Agent SDK çalıştırmalarında bu istem gösterilemez/); // the documented headless loading of project servers
    expect(c).toMatch(/üçüncü taraf bir hizmete gider/);
  });

  it("an existing file's unrelated keys and servers are preserved in order; exactly one stanza is added; pasted secrets are masked in the display", () => {
    const current = {
      "$schema": "https://example.invalid/schema.json",
      mcpServers: {
        zeta: { command: "node", args: ["a.js"], env: { TOKEN: SENTINEL } },
        alfa: { type: "http", url: "https://a.example.invalid/mcp" },
      },
      otherSection: { n: 1, list: [1, 2, { k: null }] },
    };
    const p = plan({ source: text(JSON.stringify(current)) });
    expect(p.status).toBe("PREVIEW_ONLY_NOT_APPLIED");
    const after = JSON.parse(p.proposed!);
    expect(Object.keys(after)).toEqual(["$schema", "mcpServers", "otherSection"]);
    expect(Object.keys(after.mcpServers)).toEqual(["zeta", "alfa", "exa"]);
    expect(after.otherSection).toEqual(current.otherSection);
    expect(after.mcpServers.alfa).toEqual(current.mcpServers.alfa);
    expect(after.mcpServers.zeta.command).toBe("node");
    expect(everything(p)).not.toContain(SENTINEL);
    expect(p.maskedValues).toBeGreaterThan(0);
    expect(p.diff!.filter((l) => l.kind === "add").length).toBeGreaterThan(3);
    expect(p.diff!.filter((l) => l.kind === "del")).toEqual([]);
    expect(p.existingServerNames).toEqual(["zeta", "alfa"]);
  });

  it("an empty object and a file without mcpServers are valid starting points", () => {
    for (const t of ["{}", '{"permissions":{"allow":[]}}', '{"mcpServers":{}}']) {
      const p = plan({ source: text(t) });
      expect(p.status, t).toBe("PREVIEW_ONLY_NOT_APPLIED");
      expect(JSON.parse(p.proposed!).mcpServers.exa).toEqual({ type: "http", url: EXPECTED_URL });
    }
  });

  it("is deterministic and idempotent: the same request gives the same plan, and a plan never changes its inputs", () => {
    const r = req({ source: text('{"mcpServers":{"a":{"command":"x"}}}') });
    const snapshot = JSON.stringify(r);
    expect(JSON.stringify(planConfigPreview(r))).toBe(JSON.stringify(planConfigPreview(r)));
    expect(JSON.stringify(r)).toBe(snapshot);
    expect(planInputKey(r)).toBe(planInputKey(JSON.parse(JSON.stringify(r))));
  });

  it("nothing in the plan is a credential, a query string, a header or a package-runner call", () => {
    const p = plan();
    expect(p.proposed).not.toMatch(/exaApiKey|x-api-key|Authorization|Bearer|\?|tools=|npx|"command"|"args"|"env"|"headers"/);
    expect(everything(p)).not.toContain(KEY);
  });
});

describe("collisions are refused, never overwritten", () => {
  const conflict = (servers: object) => plan({ source: text(JSON.stringify({ mcpServers: servers })) });

  it("the same name, and names that differ only in letter case, are a CONFLICT with no proposal", () => {
    for (const name of ["exa", "Exa", "EXA", "eXa"]) {
      const p = conflict({ [name]: { type: "http", url: "https://other.example.invalid/mcp" } });
      expect(p.status, name).toBe("CONFLICT");
      expect(p.proposed, name).toBeNull();
      expect(p.diff, name).toBeNull();
      expect(p.draftNotice, name).toBeNull();
      expect(p.existingServerNames).toEqual([name]);
    }
  });

  it("the same endpoint under another name is a CONFLICT, however it is spelled (case, trailing slash, query string, fragment)", () => {
    for (const url of [EXPECTED_URL, "https://MCP.EXA.AI/mcp", `${EXPECTED_URL}/`, `${EXPECTED_URL}?tools=web_search_exa`, `${EXPECTED_URL}#x`, `${EXPECTED_URL}?exaApiKey=${KEY}`]) {
      const p = conflict({ search: { type: "http", url } });
      expect(p.status, url).toBe("CONFLICT");
      expect(p.problems.map((x) => x.code), url).toContain("equivalent_exists");
      expect(everything(p), url).not.toContain(KEY); // a key in the existing URL is never shown
    }
  });

  it("a different host that merely looks similar is not a collision", () => {
    for (const url of ["https://mcp.exa.ai.attacker.example/mcp", "https://mcp.exa.ai/other", "http://mcp.exa.ai/mcp".replace("http:", "https:").replace("mcp.exa.ai", "mcp-exa.ai")]) {
      expect(conflict({ other: { type: "http", url } }).status, url).toBe("PREVIEW_ONLY_NOT_APPLIED");
    }
  });
});

describe("unsafe, malformed or unexpected pasted text is refused as the current configuration", () => {
  const bad = (t: string) => plan({ source: text(t) });
  it.each([
    ["malformed JSON", "{"], ["a comment", '{"mcpServers":{} /* c */}'], ["a trailing comma", '{"mcpServers":{},}'], ["single quotes", "{'mcpServers':{}}"], ["an array at the top", "[]"], ["a scalar", "5"], ["empty text", ""],
    ["a duplicate key", '{"mcpServers":{},"mcpServers":{}}'], ["a nested duplicate key", '{"mcpServers":{"a":{"command":"x","command":"y"}}}'], ["mcpServers of the wrong type", '{"mcpServers":[]}'], ["a server that is not an object", '{"mcpServers":{"a":"x"}}'],
  ])("%s", (_n, t) => {
    const p = bad(t);
    expect(p.status).toBe("INVALID_CURRENT_CONFIG");
    expect(p.proposed).toBeNull();
    expect(p.diff).toBeNull();
    expect(p.draftNotice).toBeNull();
  });
  it("a pasted secret in an unrelated field never reaches the screen, also when the text is refused", () => {
    expect(everything(bad(`{"mcpServers":{"a":{"headers":{"Authorization":"Bearer ${SENTINEL}"}}}`))).not.toContain(SENTINEL);
    expect(everything(bad(`{"x": "${SENTINEL}", `))).not.toContain(SENTINEL);
    expect(everything(plan({ source: text(`{"mcpServers":{"a":{"type":"http","url":"https://u:${KEY}@a.example.invalid/mcp?k=${KEY}"}}}`) }))).not.toContain(KEY);
  });
  it("TOML or prose pasted for the JSON file is an invalid current configuration, never converted", () => {
    expect(bad('[mcp_servers.exa]\nurl = "https://mcp.exa.ai/mcp"\n').status).toBe("INVALID_CURRENT_CONFIG");
  });
});

describe("no other host is served from this declaration; there is no fallback", () => {
  it("Antigravity IDE has no declaration for the entry: BLOCKED_INSUFFICIENT_EVIDENCE with no diff (Claude Code's JSON is not borrowed)", () => {
    const p = plan({ surface: "antigravity-ide" });
    expect(p.status).toBe("BLOCKED_INSUFFICIENT_EVIDENCE");
    expect(p.relativePath).toBe(".agents/mcp_config.json");
    expect([p.proposed, p.diff, p.declaration, p.draftNotice]).toEqual([null, null, null, null]);
  });
  it("Codex CLI gets the TOML status, Claude Desktop and the bare command line are unsupported surfaces - none produces a JSON draft", () => {
    expect(plan({ surface: "codex-cli" }).status).toBe("TOML_PREVIEW_NOT_IMPLEMENTED");
    expect(plan({ surface: "codex-cli" }).proposed).toBeNull();
    for (const surface of ["claude-desktop-code", "local-cli", "codex", "gemini-cli", ""]) {
      const p = plan({ surface });
      expect(p.status, surface).toBe("UNSUPPORTED_SURFACE");
      expect([p.proposed, p.draftNotice]).toEqual([null, null]);
    }
    expect(hostForSurface(CC)!.id).toBe("claude-code-project-mcp");
  });
  it("another catalog entry gets nothing from the shipped registry", () => {
    for (const id of ["microsoft-playwright-mcp", "apify-mcp-server", "gitleaks", "x"]) {
      const p = plan({ entry: exaEntry({ entryId: id }) });
      expect(p.status, id).toBe("BLOCKED_INSUFFICIENT_EVIDENCE");
      expect(p.proposed, id).toBeNull();
    }
  });
});

describe("the catalog's own gates still stop the draft; only UNKNOWN compatibility stopped being one", () => {
  const status = (over: Parameters<typeof exaEntry>[0]) => plan({ entry: exaEntry(over) });
  it.each([
    ["removed", { removedAt: "2026-10-20T00:00:00Z" }, "accept_block"],
    ["disabled", { enabled: false }, "accept_block"],
    ["execution-blocked", { executionPolicy: "blocked-until-scoped-authorization" as const }, "accept_block"],
    ["quarantined", { reviewState: "quarantined" }, "quarantined"],
    ["blocked", { reviewState: "blocked" }, "quarantined"],
  ])("%s: a clear block, an explanatory-only diff, no draft notice", (_n, over, code) => {
    const p = status(over);
    expect(p.status).toBe("BLOCKED_BY_CATALOG");
    expect(p.blockers.map((b) => b.code)).toContain(code);
    expect(p.hypothetical).toBe(true);
    expect(p.draftNotice).toBeNull();
    expect(p.statusLabel).toContain("açıklayıcı");
  });
  it("not an MCP server in the catalog, or explicitly unsupported for this OS + surface: blocked", () => {
    const notMcp = exaEntry();
    (notMcp.record as { category: string }).category = "python-cli";
    expect(plan({ entry: notMcp }).blockers.map((b) => b.code)).toContain("not_mcp");
    const unsupported = exaEntry();
    unsupported.record.targetEvidence = [{ platformTarget: WIN, agentSurface: CC, compatibilityState: "unsupported", limitation: "x" }];
    const p = plan({ entry: unsupported });
    expect(p.status).toBe("BLOCKED_BY_CATALOG");
    expect(p.blockers.map((b) => b.code)).toContain("compat_not_declared");
  });
  it("a vendor-declared cell would still be a draft (the declaration, not the compatibility, makes it one) and unknown stays unknown in the record", () => {
    const declared = exaEntry();
    declared.record.targetEvidence = [{ platformTarget: WIN, agentSurface: CC, compatibilityState: "vendor-declared", limitation: "x" }];
    expect(plan({ entry: declared }).status).toBe("PREVIEW_ONLY_NOT_APPLIED");
    expect(plan().constraints.join(" ")).toContain("Uyumluluk (bu hedef için kayıt): unknown");
  });
});

describe("untrusted or changed catalog data: a security-significant mismatch is a clear block, never a silent draft", () => {
  const mismatch = (entry: ReturnType<typeof exaEntry>) => {
    const p = plan({ entry });
    expect(p.status).toBe("BLOCKED_INSUFFICIENT_EVIDENCE");
    expect(p.problems.map((x) => x.code)).toContain("source_mismatch");
    expect([p.proposed, p.diff, p.declaration, p.draftNotice]).toEqual([null, null, null, null]);
    return p;
  };
  it("a different pinned commit (one character), a missing pin, a malformed pin", () => {
    const flip = exaEntry();
    (flip.record as { sourceSnapshot: { immutableCommit: string } }).sourceSnapshot.immutableCommit = PINNED.replace(/.$/, "0");
    mismatch(flip);
    const none = exaEntry();
    delete (none.record as { sourceSnapshot?: unknown }).sourceSnapshot;
    mismatch(none);
    const upper = exaEntry();
    (upper.record as { sourceSnapshot: { immutableCommit: string } }).sourceSnapshot.immutableCommit = PINNED.toUpperCase();
    mismatch(upper);
    const num = exaEntry();
    (num.record as { sourceSnapshot: { immutableCommit: unknown } }).sourceSnapshot.immutableCommit = 12345;
    mismatch(num);
  });
  it("a different repository, a lookalike repository, or no repository", () => {
    for (const url of ["https://github.com/exa-labs/exa-mcp-server.git", "https://github.com/exa-labs/exa-mcp-servers", "https://github.com/evil/exa-mcp-server", "http://github.com/exa-labs/exa-mcp-server", "https://github.com/Exa-Labs/exa-mcp-server"]) {
      const e = exaEntry();
      (e.record as { repository: { url: string } }).repository.url = url;
      mismatch(e);
    }
    const none = exaEntry();
    delete (none.record as { repository?: unknown }).repository;
    mismatch(none);
  });
  it("an old or empty backend payload (no record at all) is refused, not guessed", () => {
    const old = exaEntry();
    (old as unknown as { record: unknown }).record = undefined;
    const p = plan({ entry: old });
    expect(p.status).not.toBe("PREVIEW_ONLY_NOT_APPLIED");
    expect(p.proposed).toBeNull();
    const emptyRecord = exaEntry();
    (emptyRecord as unknown as { record: unknown }).record = {};
    expect(plan({ entry: emptyRecord }).status).not.toBe("PREVIEW_ONLY_NOT_APPLIED");
  });
  it("a tampered registry entry (key in the URL, bearer header, wrong type, unknown field) fails the hygiene rules and is not used", () => {
    const base = REVIEWED_DECLARATIONS[0];
    const tamper = (declaration: Record<string, unknown>) => planConfigPreview(req(), [{ ...base, declaration: declaration as never }]);
    for (const declaration of [
      { type: "http", url: `${EXPECTED_URL}?exaApiKey=${KEY}` }, { type: "http", url: EXPECTED_URL, headers: { Authorization: `Bearer ${KEY}` } }, { type: "streamable-http", url: EXPECTED_URL },
      { url: EXPECTED_URL }, { type: "http", url: "http://mcp.exa.ai/mcp" }, { type: "http", url: `https://user:${KEY}@mcp.exa.ai/mcp` }, { type: "http", url: EXPECTED_URL, extra: true },
      { type: "sse", url: EXPECTED_URL }, { command: "npx", args: ["-y", "exa-mcp-server@latest"] },
    ]) {
      const p = tamper(declaration);
      expect(p.status, JSON.stringify(declaration)).toBe("BLOCKED_INSUFFICIENT_EVIDENCE");
      expect(p.problems.map((x) => x.code)).toContain("declaration_rejected");
      expect(p.proposed).toBeNull();
      expect(everything(p)).not.toContain(KEY);
    }
  });
});

describe("a shown preview is withdrawn when anything it depends on changes", () => {
  it("project, operating system, surface, catalog pin, catalog state and pasted text each change the input key", () => {
    const base = plan({ source: text('{"mcpServers":{}}') });
    const r = req({ source: text('{"mcpServers":{}}') });
    expect(isPlanStale(base, r)).toBe(false);
    const pinned = exaEntry();
    (pinned.record as { sourceSnapshot: { immutableCommit: string } }).sourceSnapshot.immutableCommit = "1".repeat(40);
    const moved = exaEntry({ revision: "v2@other" });
    for (const [label, over] of [
      ["project", { alias: "baska-proje" }], ["os", { platform: MAC }], ["surface", { surface: "antigravity-ide" }], ["pin", { entry: pinned }], ["revision", { entry: moved }],
      ["disabled", { entry: exaEntry({ enabled: false }) }], ["text", { source: text('{"mcpServers":{"a":{"command":"x"}}}') }], ["mode", { source: { kind: "new" } }],
    ] as [string, Partial<PlanRequest>][]) {
      expect(isPlanStale(base, { ...r, ...over }), label).toBe(true);
    }
  });
});

describe("the words never claim more than a text draft", () => {
  it("no status label, notice, blocker, constraint, warning or note says installed, connected, verified, accepted, tested, working or successful", () => {
    const banned = /\b(kuruldu|kuruldu\.|bağlandı|doğrulandı|doğrulanmış olarak|onaylandı|kabul edildi|test edildi|çalışıyor|çalıştı|başarılı|başarıyla|etkinleştirildi)\b/i;
    for (const p of [plan(), plan({ source: text('{"mcpServers":{"exa":{}}}') }), plan({ surface: "antigravity-ide" }), plan({ entry: exaEntry({ enabled: false }) })]) {
      const strings = [p.statusLabel, p.notice, p.draftNotice ?? "", p.scope ?? "", ...p.blockers.map((b) => b.text), ...p.problems.map((b) => b.text), ...p.constraints, p.declaration?.note ?? ""];
      for (const s of strings) expect(s, s).not.toMatch(banned);
    }
  });
  it("the parser used for the pasted text is the strict one (no comments, no repair)", () => {
    expect(parseConfigText('{"a":1} // c').ok).toBe(false);
  });
});
