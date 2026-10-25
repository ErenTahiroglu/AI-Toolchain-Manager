// CONFIG-01 - the pure configuration planner. Synthetic in-memory fixtures only: no file is read or written, nothing is spawned, no network, no IPC.
// Every "secret" below is an obviously fake sentinel. The real registry of reviewed declarations is EMPTY; these tests inject synthetic registries.
import { describe, expect, it } from "vitest";
import { REVIEWED_DECLARATIONS, type ReviewedDeclaration } from "./configDeclarations";
import {
  DRAFT_NOTICE, HOSTS, MASK, NO_FILE_TOUCHED, checkServersShape, findDuplicateKeys, hostForSurface, isMcpEntry, isPlanStale, lineDiff, parseConfigText, planConfigPreview, planInputKey,
  redactForDisplay, sha256Hex, validateDeclaration, type JsonValue, type PlanRequest, type PreviewPlan,
} from "./configPlan";
import { makeEntry } from "./test/backend";
import { acceptBlockReason } from "./workspace";

const WIN = "windows-x64";
const MAC = "macos-arm64";
const CC = "claude-code-terminal";
const AG = "antigravity-ide";
const SENTINEL = "ghp_SENTINELSENTINELSENTINELSENTINEL12";
const SENTINEL2 = "sk-SENTINEL-SECRET-VALUE-0123456789abcdef";
const PIN = "0123456789abcdef0123456789abcdef01234567";

const ev = (platformTarget: string, agentSurface: string, compatibilityState: string) => ({ platformTarget, agentSurface, compatibilityState, limitation: "x" });
/** A catalog entry that is an MCP server, declared for Claude Code on both OSes and for nothing else (the shape of the shipped Playwright MCP record). */
const mcp = (over: Record<string, unknown> = {}) =>
  makeEntry("synthetic-mcp", { displayName: "Sentetik MCP", kind: "agent_plugin", record: { category: "mcp-server", sourceSnapshot: { immutableCommit: PIN }, targetEvidence: [ev(WIN, CC, "vendor-declared"), ev(MAC, CC, "vendor-declared"), ev(WIN, AG, "unknown"), ev(MAC, AG, "vendor-declared")] }, ...over });

const CLAUDE_DECL: ReviewedDeclaration = {
  hostId: "claude-code-project-mcp", entryId: "synthetic-mcp", serverName: "synthetic",
  declaration: { command: "npx", args: ["-y", "@example/synthetic-mcp@1.2.3"] },
  review: { reviewId: "REV-SYN-1", evidenceIds: ["R99"], reviewedAgainst: PIN },
};
const AG_DECL: ReviewedDeclaration = {
  hostId: "antigravity-workspace-mcp", entryId: "synthetic-mcp", serverName: "synthetic",
  declaration: { serverUrl: "https://synthetic.example.invalid/mcp" },
  review: { reviewId: "REV-SYN-2", evidenceIds: ["R99"], reviewedAgainst: PIN },
};
const DECLS = [CLAUDE_DECL, AG_DECL];

const req = (over: Partial<PlanRequest> = {}): PlanRequest => ({ alias: "benim-sitem", platform: WIN, surface: CC, entry: mcp(), source: { kind: "new" }, ...over });
const text = (t: string): PlanRequest["source"] => ({ kind: "text", text: t });
const plan = (over: Partial<PlanRequest> = {}, decls: readonly ReviewedDeclaration[] = DECLS): PreviewPlan => planConfigPreview(req(over), decls);
const everything = (p: PreviewPlan) => JSON.stringify(p);

describe("hosts and paths: one fixed relative path per host, surfaces never guessed", () => {
  it("exactly two hosts, each tied to exactly one application surface, with fixed relative paths", () => {
    expect(HOSTS.map((h) => [h.surface, h.relativePath])).toEqual([[CC, ".mcp.json"], [AG, ".agents/mcp_config.json"]]);
    expect(hostForSurface(CC)?.id).toBe("claude-code-project-mcp");
    expect(hostForSurface(AG)?.id).toBe("antigravity-workspace-mcp");
    for (const h of HOSTS) {
      expect(h.relativePath).not.toMatch(/^[\\/]|\.\.|~|:|\\/); // relative, no traversal, no drive, no user-profile path
      expect(h.untouched.length).toBeGreaterThan(0);
    }
  });

  it("every other surface is UNSUPPORTED_SURFACE - Claude Desktop and the generic command line are not mapped onto Claude Code or Antigravity", () => {
    for (const surface of ["claude-desktop-code", "local-cli", "antigravity-cli", "antigravity-2", "", "Claude-Code-Terminal"]) {
      const p = plan({ surface });
      expect(p.status, surface).toBe("UNSUPPORTED_SURFACE");
      expect(p.relativePath, surface).toBeNull();
      expect(p.proposed, surface).toBeNull();
      expect(p.diff, surface).toBeNull();
    }
  });

  it("the file never depends on the operating system or the project: the same host gives the same path", () => {
    expect(plan({ platform: WIN }).relativePath).toBe(".mcp.json");
    expect(plan({ platform: MAC }).relativePath).toBe(".mcp.json");
    expect(plan({ alias: "baska", platform: MAC, surface: AG }).relativePath).toBe(".agents/mcp_config.json");
  });

  it("identity is never defaulted: an empty alias, an empty entry or an unknown operating system is INVALID_INPUT", () => {
    for (const over of [{ alias: "" }, { alias: "  " }, { platform: "" }, { platform: "linux-x64" }, { platform: "windows-arm64" }, { entry: mcp({ entryId: "" }) }]) {
      const p = plan(over as Partial<PlanRequest>);
      expect(p.status).toBe("INVALID_INPUT");
      expect(p.proposed).toBeNull();
    }
  });
});

describe("with no approved declaration: BLOCKED_INSUFFICIENT_EVIDENCE and no recipe, ever", () => {
  it("the registry the application ships holds exactly ONE declaration (CONFIG-02A): the real Exa catalog entry on Claude Code's .mcp.json", () => {
    expect(REVIEWED_DECLARATIONS.map((d) => [d.hostId, d.entryId, d.serverName])).toEqual([["claude-code-project-mcp", "exa-mcp-server", "exa"]]);
  });

  it("every host, OS, source mode and every OTHER entry gets BLOCKED_INSUFFICIENT_EVIDENCE from the shipped registry - no command is made up", () => {
    for (const surface of [CC, AG]) {
      for (const platform of [WIN, MAC]) {
        for (const source of [{ kind: "new" } as const, text('{"mcpServers":{}}')]) {
          const p = planConfigPreview(req({ surface, platform, source }));
          expect(p.status, `${surface}/${platform}/${source.kind}`).toBe("BLOCKED_INSUFFICIENT_EVIDENCE");
          expect(p.proposed).toBeNull();
          expect(p.diff).toBeNull();
          expect(p.declaration).toBeNull();
          expect(p.blockers.map((b) => b.code)).toContain("no_reviewed_declaration");
          expect(everything(p)).not.toMatch(/npx|"command"|"args"/); // the explanation may MENTION @latest as something rejected; no stanza exists
        }
      }
    }
  });

  it("an entry that is not an MCP server gets no preview either, and says so", () => {
    const p = plan({ entry: makeEntry("gitleaks") }, DECLS);
    expect(isMcpEntry(makeEntry("gitleaks"))).toBe(false);
    expect(p.blockers.map((b) => b.code)).toContain("not_mcp");
    expect(p.status).not.toBe("PREVIEW_ONLY_NOT_APPLIED");
  });

  it("a declaration for another entry or another host is not used", () => {
    expect(plan({ entry: mcp({ entryId: "other-mcp" }) }).status).toBe("BLOCKED_INSUFFICIENT_EVIDENCE");
    expect(plan({}, [AG_DECL]).status).toBe("BLOCKED_INSUFFICIENT_EVIDENCE"); // only an Antigravity declaration exists; the surface is Claude Code
  });
});

describe("a usable preview: new file, existing file, unrelated keys preserved", () => {
  it("new file: the proposed content is exactly one server, labelled PREVIEW ONLY - NOT APPLIED, with the file untouched", () => {
    const p = plan();
    expect(p.status).toBe("PREVIEW_ONLY_NOT_APPLIED");
    expect(p.applied).toBe(false);
    expect(p.touchedFiles).toBe(0);
    expect(p.notice).toBe(NO_FILE_TOUCHED);
    expect(p.statusLabel).toContain("UYGULANMADI");
    expect(p.original).toBeNull();
    expect(p.hypothetical).toBe(false);
    expect(JSON.parse(p.proposed!)).toEqual({ mcpServers: { synthetic: { command: "npx", args: ["-y", "@example/synthetic-mcp@1.2.3"] } } });
    expect(p.diff!.filter((l) => l.kind === "del")).toEqual([]);
    expect(p.diff!.filter((l) => l.kind === "add").length).toBeGreaterThan(3);
    expect(p.relativePath).toBe(".mcp.json");
    expect(p.declaration).toEqual({ serverName: "synthetic", reviewId: "REV-SYN-1", evidenceIds: ["R99"], note: null });
    expect(p.draftNotice).toBe(DRAFT_NOTICE);
    expect(p.sourceDigest).toBeNull();
  });

  it("existing file: unrelated top-level keys and unrelated servers are preserved semantically, in their order, and only one stanza is added", () => {
    const current = { "$schema": "x", mcpServers: { zeta: { command: "node", args: ["a.js"] }, alfa: { type: "http", url: "https://a.example.invalid/mcp" } }, other: { n: 1, list: [1, 2, { k: null }] } };
    const p = plan({ source: text(JSON.stringify(current)) });
    expect(p.status).toBe("PREVIEW_ONLY_NOT_APPLIED");
    const after = JSON.parse(p.proposed!);
    expect(after.$schema).toBe("x");
    expect(after.other).toEqual(current.other);
    expect(Object.keys(after.mcpServers)).toEqual(["zeta", "alfa", "synthetic"]);
    expect(after.mcpServers.zeta).toEqual(current.mcpServers.zeta);
    expect(after.mcpServers.alfa).toEqual(current.mcpServers.alfa);
    const removed = p.diff!.filter((l) => l.kind === "del");
    expect(removed.every((l) => l.text.trim() === "}" || l.text.trim().endsWith("}"))).toBe(true); // only a closing brace gains a comma
    expect(p.diff!.filter((l) => l.kind === "add").map((l) => l.text).join("\n")).toContain('"synthetic"');
    expect(p.existingServerNames).toEqual(["zeta", "alfa"]);
  });

  it("a file with no mcpServers key gets one; an empty object is a valid starting point", () => {
    expect(JSON.parse(plan({ source: text("{}") }).proposed!)).toEqual({ mcpServers: { synthetic: CLAUDE_DECL.declaration } });
    const p = plan({ source: text('{"permissions":{"allow":["x"]}}') });
    expect(JSON.parse(p.proposed!)).toEqual({ permissions: { allow: ["x"] }, mcpServers: { synthetic: CLAUDE_DECL.declaration } });
  });

  it("Antigravity: the workspace file gets the remote stanza under its own path, and nothing about Claude Code", () => {
    const p = plan({ surface: AG, platform: MAC });
    expect(p.status).toBe("PREVIEW_ONLY_NOT_APPLIED");
    expect(p.relativePath).toBe(".agents/mcp_config.json");
    expect(JSON.parse(p.proposed!)).toEqual({ mcpServers: { synthetic: { serverUrl: "https://synthetic.example.invalid/mcp" } } });
    expect(p.identity.hostId).toBe("antigravity-workspace-mcp");
    expect(p.untouched.join(" ")).toContain("~/.gemini/config/mcp_config.json");
    expect(p.constraints.join(" ")).toContain("Yalnızca Antigravity IDE");
  });

  it("formatting is normalised and said so; a BOM is accepted and said so", () => {
    const messy = '\uFEFF{"mcpServers":{"a":{"command":"node"}}}';
    const p = plan({ source: text(messy) });
    expect(p.status).toBe("PREVIEW_ONLY_NOT_APPLIED");
    expect(p.formattingNote).toMatch(/normalize/);
    const canonical = JSON.stringify({ mcpServers: { a: { command: "node" } } }, null, 2);
    expect(plan({ source: text(canonical) }).formattingNote).toBeNull();
  });
});

describe("the current text is rejected, not repaired", () => {
  const bad = (t: string) => plan({ source: text(t) });

  it("malformed JSON, comments, trailing commas, empty and non-object content: INVALID_CURRENT_CONFIG, no diff", () => {
    for (const t of ["", "   ", "{", "{'a':1}", '{"a":1,}', '{"a":1} // c', '/* c */ {"a":1}', "[]", "null", '"text"', "42", "not json at all"]) {
      const p = bad(t);
      expect(p.status, t).toBe("INVALID_CURRENT_CONFIG");
      expect(p.proposed, t).toBeNull();
      expect(p.diff, t).toBeNull();
      expect(p.original, t).toBeNull();
    }
    expect(bad('{"a":1} // c').problems.map((q) => q.text).join(" ")).toMatch(/JSONC/);
    expect(bad("{").problems[0].code).toBe("malformed_json");
  });

  it("the JSON engine's own message is never shown: it can quote the text, and the text may hold a secret", () => {
    const p = bad(`{"token": "${SENTINEL}" oops`);
    expect(p.status).toBe("INVALID_CURRENT_CONFIG");
    expect(everything(p)).not.toContain(SENTINEL);
    expect(everything(p)).not.toContain("oops");
  });

  it("duplicate keys, at any depth, are refused: the last one would silently win", () => {
    expect(bad('{"mcpServers":{},"mcpServers":{}}').problems[0].code).toBe("duplicate_keys");
    expect(bad('{"mcpServers":{"a":{"command":"x","command":"y"}}}').problems[0].code).toBe("duplicate_keys");
    expect(bad('{"mcpServers":{"a":{},"a":{}}}').status).toBe("INVALID_CURRENT_CONFIG");
    expect(findDuplicateKeys('{"a":{"b":1},"c":{"b":2},"d":[{"x":1},{"x":2}]}')).toEqual([]); // the same key in different objects is fine
    expect(findDuplicateKeys('{"a":"a","b":"a"}')).toEqual([]); // equal VALUES are fine
  });

  it("wrong structural types are refused: mcpServers must be an object of objects", () => {
    for (const t of ['{"mcpServers":[]}', '{"mcpServers":"x"}', '{"mcpServers":null}', '{"mcpServers":{"a":"node"}}', '{"mcpServers":{"a":[1]}}', '{"mcpServers":{"a":null}}', '{"mcpServers":{"ok":{},"b":1}}']) {
      const p = bad(t);
      expect(p.status, t).toBe("INVALID_CURRENT_CONFIG");
      expect(p.proposed, t).toBeNull();
    }
    expect(checkServersShape({ mcpServers: { a: {} } })).toEqual([]);
    expect(checkServersShape({})).toEqual([]);
  });

  it("an enormous text is refused", () => {
    expect(bad(JSON.stringify({ mcpServers: {}, pad: "x".repeat(250_000) })).problems[0].code).toBe("too_large");
    const many = Object.fromEntries(Array.from({ length: 201 }, (_, i) => [`s${i}`, {}]));
    expect(bad(JSON.stringify({ mcpServers: many })).problems[0].code).toBe("too_many_servers");
  });

  it("an existing server of the same name is a CONFLICT and is never overwritten; an equivalent one under another name is too", () => {
    const p = bad(JSON.stringify({ mcpServers: { synthetic: { command: "node" } } }));
    expect(p.status).toBe("CONFLICT");
    expect(p.proposed).toBeNull();
    expect(p.problems.map((q) => q.code)).toContain("server_name_exists");
    const same = bad(JSON.stringify({ mcpServers: { renamed: { command: "npx", args: ["-y", "@example/synthetic-mcp@1.2.3"] } } }));
    expect(same.status).toBe("CONFLICT");
    expect(same.problems.map((q) => q.code)).toContain("equivalent_exists");
    const remote = plan({ surface: AG, platform: MAC, source: text(JSON.stringify({ mcpServers: { other: { serverUrl: "https://SYNTHETIC.example.invalid/mcp/" } } })) });
    expect(remote.status).toBe("CONFLICT");
  });

  it("the shape of the document is the host's shape: a key that happens to be named mcpServers deeper down is not it", () => {
    const p = bad('{"nested":{"mcpServers":[]}}');
    expect(p.status).toBe("PREVIEW_ONLY_NOT_APPLIED");
    expect(JSON.parse(p.proposed!).nested).toEqual({ mcpServers: [] });
  });

  it("prototype-pollution style keys stay plain data", () => {
    const p = bad('{"__proto__":{"polluted":true},"mcpServers":{}}');
    expect(p.status).toBe("PREVIEW_ONLY_NOT_APPLIED");
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
  });
});

describe("secrets are masked, never echoed, never kept", () => {
  const withSecrets = JSON.stringify({
    mcpServers: {
      one: { command: "node", args: ["server.js", "--api-key", SENTINEL2, `--token=${SENTINEL}`], env: { API_TOKEN: SENTINEL, PLAIN: "visible", FROM_ENV: "${KEEP_ME}" } },
      two: { type: "http", url: `https://user:pw-${SENTINEL}@host.example.invalid/mcp?key=${SENTINEL2}#frag`, headers: { Authorization: `Bearer ${SENTINEL}`, "X-Ref": "Bearer ${TOKEN_REF}" } },
    },
    note: "ordinary text",
    blob: "A".repeat(40),
  });

  it("none of the sentinels appears anywhere in the plan, in any status", () => {
    for (const decls of [DECLS, []]) {
      const p = planConfigPreview(req({ source: text(withSecrets) }), decls);
      const all = everything(p);
      for (const s of [SENTINEL, SENTINEL2, "pw-", "A".repeat(40), "frag"]) expect(all, `${s} in ${p.status}`).not.toContain(s);
      expect(p.maskedValues).toBeGreaterThanOrEqual(6);
      expect(p.original).toContain(MASK);
    }
  });

  it("references to environment variables and ordinary values are kept, so the preview stays readable", () => {
    const p = plan({ source: text(withSecrets) });
    expect(p.original).toContain("${KEEP_ME}");
    expect(p.original).toContain("${TOKEN_REF}");
    expect(p.original).toContain('"PLAIN": "visible"');
    expect(p.original).toContain("ordinary text");
    expect(p.original).toContain("https://host.example.invalid/mcp");
    expect(p.constraints.join(" ")).toMatch(/maskelendi/);
  });

  it("redaction is deterministic and total over keys, flags, URLs and token shapes", () => {
    const { value, masked } = redactForDisplay({ a: { password: "x" }, b: ["--secret", "y", "plain"], c: "https://h.example.invalid/p?token=1", d: SENTINEL, e: "short", f: ["tokens", SENTINEL] });
    expect(JSON.stringify(value)).not.toMatch(/"x"|"y"|token=1|SENTINEL/);
    expect(masked).toBe(5);
    expect(JSON.stringify(redactForDisplay({ a: { password: "x" } }))).toBe(JSON.stringify(redactForDisplay({ a: { password: "x" } })));
  });
});

describe("blocked by the catalog: the preview cannot lift an existing block", () => {
  it("quarantined and execution-blocked entries stay blocked; any diff shown is explanatory only", () => {
    const entry = mcp({ reviewState: "quarantined", executionPolicy: "blocked-until-scoped-authorization" });
    const p = plan({ entry });
    expect(p.status).toBe("BLOCKED_BY_CATALOG");
    expect(p.hypothetical).toBe(true);
    expect(p.blockers.map((b) => b.code)).toEqual(expect.arrayContaining(["accept_block", "quarantined"]));
    expect(p.statusLabel).toContain("açıklayıcı");
    expect(p.constraints.join(" ")).toMatch(/kabul\/etkinleştirme engellerini kaldırmaz/);
  });

  it("CONFIG-02A: an explicitly UNSUPPORTED combination still blocks the draft; UNKNOWN or missing compatibility no longer does - it is stated beside the draft and activation stays refused elsewhere", () => {
    const withState = (state: string | null) =>
      mcp({ record: { category: "mcp-server", sourceSnapshot: { immutableCommit: PIN }, targetEvidence: state === null ? [] : [ev(WIN, CC, state)] } });
    const unsupported = plan({ entry: withState("unsupported") });
    expect(unsupported.status).toBe("BLOCKED_BY_CATALOG");
    expect(unsupported.blockers.map((b) => b.code)).toContain("compat_not_declared");
    expect(unsupported.hypothetical).toBe(true);
    expect(unsupported.draftNotice).toBeNull();
    for (const entry of [withState("unknown"), withState(null)]) {
      const p = plan({ entry });
      expect(p.status).toBe("PREVIEW_ONLY_NOT_APPLIED");
      expect(p.hypothetical).toBe(false);
      expect(p.blockers).toEqual([]);
      expect(p.draftNotice).toBe(DRAFT_NOTICE);
      expect(p.constraints.join(" ")).toMatch(/uyumluluk bilinmiyor\. Bu bir yapılandırma TASLAĞIDIR/);
      expect(p.constraints.join(" ")).toMatch(/seçilemez; bu önizleme bunu değiştirmez/);
    }
    // the matrix's own acceptance rule is untouched: an unknown cell still cannot be 'accepted'
    expect(acceptBlockReason(withState("unknown") as unknown as Parameters<typeof acceptBlockReason>[0], "unknown")).toMatch(/uyumluluk bilinmiyor veya desteklenmiyor/);
    // a combination the fixture declares for another surface is not borrowed
    expect(plan({ platform: WIN, surface: AG }).status).toBe("PREVIEW_ONLY_NOT_APPLIED"); // Antigravity has its own declaration in the fixture registry; only its compatibility is unknown
  });

  it("removed and disabled entries are blocked", () => {
    expect(plan({ entry: mcp({ removedAt: "2026-10-20T00:00:00Z" }) }).status).toBe("BLOCKED_BY_CATALOG");
    expect(plan({ entry: mcp({ enabled: false }) }).status).toBe("BLOCKED_BY_CATALOG");
  });

  it("a catalog-blocked entry with no declaration is BLOCKED_INSUFFICIENT_EVIDENCE with no diff at all, and lists both reasons", () => {
    const p = plan({ entry: mcp({ reviewState: "quarantined" }) }, []);
    expect(p.status).toBe("BLOCKED_INSUFFICIENT_EVIDENCE");
    expect(p.diff).toBeNull();
    expect(p.hypothetical).toBe(false);
    expect(p.blockers.map((b) => b.code)).toEqual(expect.arrayContaining(["quarantined", "no_reviewed_declaration"]));
  });

  it("only a declared, enabled, non-quarantined, reviewed combination reaches PREVIEW_ONLY_NOT_APPLIED - and even then nothing is applied", () => {
    const p = plan();
    expect(p.status).toBe("PREVIEW_ONLY_NOT_APPLIED");
    expect(p.blockers).toEqual([]);
    expect(p.constraints.join(" ")).toMatch(/desteklendiği, çalıştığı, kurulu olduğu veya güvenli olduğu anlamına gelmez/);
    expect(p.applied).toBe(false);
  });
});

describe("the reviewed registry is held to hygiene rules too (defence in depth)", () => {
  const claude = HOSTS[0];
  const ag = HOSTS[1];
  const base = (decl: Record<string, JsonValue>, over: Partial<ReviewedDeclaration> = {}): ReviewedDeclaration => ({ ...CLAUDE_DECL, declaration: decl, ...over });

  it("accepts the synthetic fixtures", () => {
    expect(validateDeclaration(claude, CLAUDE_DECL)).toEqual([]);
    expect(validateDeclaration(ag, AG_DECL)).toEqual([]);
    expect(validateDeclaration(claude, base({ type: "http", url: "https://x.example.invalid/mcp", headers: { Authorization: "Bearer ${EXAMPLE_TOKEN}" } }))).toEqual([]);
  });

  it("rejects unpinned runners, dist-tags, shell metacharacters, paths and credentials in URLs", () => {
    const cases: [string, Record<string, JsonValue>][] = [
      ["unpinned_tag", { command: "npx", args: ["-y", "@example/x@latest"] }],
      ["runner_needs_exact_version", { command: "npx", args: ["-y", "@example/x"] }],
      ["runner_needs_exact_version", { command: "uvx", args: ["pkg"] }],
      ["command", { command: "npx -y evil@1.0.0" }],
      ["command", { command: "node;rm" }],
      ["command", { command: "/usr/bin/node" }],
      ["command", { command: "C:\\tools\\node.exe" }],
      ["path_in_args", { command: "node", args: ["../server.js"] }],
      ["path_in_args", { command: "node", args: ["/etc/passwd"] }],
      ["unknown_field", { command: "node", args: [], shell: true }],
      ["unknown_field", { command: "node", url: "https://a.example.invalid/m" }],
      ["url", { type: "http", url: "http://a.example.invalid/m" }],
      ["url", { type: "http", url: "https://user:pw@a.example.invalid/m" }],
      ["url", { type: "http", url: "https://a.example.invalid/m?key=1" }],
      ["url", { type: "http", url: "https://localhost/m" }],
      ["claude_remote_needs_type_http", { url: "https://a.example.invalid/m" }],
      ["env_literal_value", { command: "node", env: { API_KEY: "literal-value" } }],
      ["headers_literal_value", { type: "http", url: "https://a.example.invalid/m", headers: { Authorization: "Bearer literal" } }],
      ["env_literal_value", { command: "node", env: { OTHER: "x".repeat(80) } }],
    ];
    for (const [code, decl] of cases) expect(validateDeclaration(claude, base(decl)), JSON.stringify(decl)).toContain(code);
  });

  it("Antigravity: no env or headers at all (expansion is not documented), no httpUrl, no cwd outside the workspace", () => {
    const agBase = (decl: Record<string, JsonValue>): ReviewedDeclaration => ({ ...AG_DECL, declaration: decl });
    expect(validateDeclaration(ag, agBase({ serverUrl: "https://a.example.invalid/m", headers: { A: "b" } }))).toContain("headers_not_expressible");
    expect(validateDeclaration(ag, agBase({ command: "node", args: [], env: { A: "b" } }))).toContain("env_not_expressible");
    expect(validateDeclaration(ag, agBase({ httpUrl: "https://a.example.invalid/m" }))).toEqual(expect.arrayContaining(["unknown_field"]));
    expect(validateDeclaration(ag, agBase({ command: "node", cwd: "../x" }))).toContain("cwd_path");
    expect(validateDeclaration(ag, agBase({ serverUrl: "https://a.example.invalid/m", disabled: true }))).toEqual([]);
  });

  it("rejects a bad name, a reserved name, a host mismatch and a missing or malformed review record", () => {
    expect(validateDeclaration(claude, base(CLAUDE_DECL.declaration, { serverName: "Bad Name" }))).toContain("server_name");
    expect(validateDeclaration(claude, base(CLAUDE_DECL.declaration, { serverName: "__proto__" }))).toContain("server_name");
    expect(validateDeclaration(claude, { ...AG_DECL, declaration: CLAUDE_DECL.declaration })).toContain("host_mismatch");
    expect(validateDeclaration(claude, base(CLAUDE_DECL.declaration, { review: { reviewId: "", evidenceIds: [], reviewedAgainst: "main" } }))).toContain("review_record");
    expect(validateDeclaration(claude, base(CLAUDE_DECL.declaration, { review: { reviewId: "x", evidenceIds: ["not-an-id"], reviewedAgainst: PIN } }))).toContain("review_record");
  });

  it("a registry entry that fails the rules is not used: BLOCKED_INSUFFICIENT_EVIDENCE with the rejection stated", () => {
    const risky = base({ command: "npx", args: ["-y", "@example/synthetic-mcp@latest"] });
    const p = planConfigPreview(req(), [risky]);
    expect(p.status).toBe("BLOCKED_INSUFFICIENT_EVIDENCE");
    expect(p.problems.map((q) => q.code)).toContain("declaration_rejected");
    expect(p.proposed).toBeNull();
    expect(p.declaration).toBeNull();
  });
});

describe("determinism and staleness", () => {
  it("the same input gives the same plan, byte for byte, in any call order", () => {
    const r = req({ source: text('{"mcpServers":{"a":{"command":"node"}},"x":[1,2]}') });
    const first = JSON.stringify(planConfigPreview(r, DECLS));
    planConfigPreview(req({ surface: AG }), DECLS);
    expect(JSON.stringify(planConfigPreview(r, DECLS))).toBe(first);
    expect(planInputKey(r, DECLS)).toBe(planInputKey(r, [...DECLS].reverse()));
  });

  it("a plan goes stale when ANY input changes: project, OS, surface, entry, source text, source mode, catalog state, declaration", () => {
    const base = req({ source: text('{"mcpServers":{}}') });
    const p = planConfigPreview(base, DECLS);
    expect(isPlanStale(p, base, DECLS)).toBe(false);
    const changes: [string, PlanRequest, readonly ReviewedDeclaration[]][] = [
      ["project", { ...base, alias: "baska-proje" }, DECLS],
      ["os", { ...base, platform: MAC }, DECLS],
      ["surface", { ...base, surface: AG }, DECLS],
      ["entry", { ...base, entry: mcp({ entryId: "other-mcp" }) }, DECLS],
      ["text", { ...base, source: text('{"mcpServers":{} }') }, DECLS],
      ["mode", { ...base, source: { kind: "new" } }, DECLS],
      ["removed", { ...base, entry: mcp({ removedAt: "2026-10-20T00:00:00Z" }) }, DECLS],
      ["disabled", { ...base, entry: mcp({ enabled: false }) }, DECLS],
      ["review state", { ...base, entry: mcp({ reviewState: "quarantined" }) }, DECLS],
      ["policy", { ...base, entry: mcp({ executionPolicy: "blocked-until-scoped-authorization" }) }, DECLS],
      ["fingerprint", { ...base, entry: mcp({ capabilityFingerprint: "e".repeat(64) }) }, DECLS],
      ["declaration removed", base, []],
      ["declaration changed", base, [{ ...CLAUDE_DECL, declaration: { command: "npx", args: ["-y", "@example/synthetic-mcp@1.2.4"] } }, AG_DECL]],
    ];
    for (const [what, r, decls] of changes) expect(isPlanStale(p, r, decls), what).toBe(true);
  });
});

describe("sha256 and the line diff", () => {
  it("matches the published test vectors", async () => {
    expect(sha256Hex("")).toBe("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    expect(sha256Hex("abc")).toBe("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    expect(sha256Hex("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")).toBe("248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
    // cross-check against the platform's own implementation (test only) over every length around the block boundaries, plus non-ASCII text
    const platform = async (s: string) => [...new Uint8Array(await globalThis.crypto.subtle.digest("SHA-256", new TextEncoder().encode(s)))].map((b) => b.toString(16).padStart(2, "0")).join("");
    for (let n = 0; n <= 200; n++) expect(sha256Hex("x".repeat(n)), `length ${n}`).toBe(await platform("x".repeat(n)));
    for (const s of ["şğüİıÖç — ✓", "a".repeat(1000), String.fromCharCode(123, 10, 125, 10)]) expect(sha256Hex(s)).toBe(await platform(s));
    expect(sha256Hex("ş")).toBe(sha256Hex("\u015f")); // UTF-8, not UTF-16
    expect(sha256Hex("x")).not.toBe(sha256Hex("y"));
  });

  it("parseConfigText digests only what it is given", () => {
    const p = plan({ source: text('{"a":1}') });
    expect(p.sourceDigest).toBe(sha256Hex('{"a":1}'));
    expect(p.sourceDigest).toHaveLength(64);
  });

  it("lineDiff: pure insertion, pure deletion, replacement, no change, and context with gaps", () => {
    expect(lineDiff(["a", "b"], ["a", "b"])).toEqual([]); // nothing changed: no hunk
    expect(lineDiff([], ["x", "y"]).map((l) => l.kind)).toEqual(["add", "add"]);
    expect(lineDiff(["x"], []).map((l) => l.kind)).toEqual(["del"]);
    const d = lineDiff(["1", "2", "3", "4", "5", "6", "7", "8"], ["1", "2", "3", "4", "X", "5", "6", "7", "8"]);
    expect(d.filter((l) => l.kind === "add")).toEqual([{ kind: "add", text: "X" }]);
    expect(d.some((l) => l.kind === "gap")).toBe(true);
    expect(d.filter((l) => l.kind === "ctx").length).toBeLessThanOrEqual(4);
  });

  it("parseConfigText: a tab/newline-heavy but valid document parses; strictness matches the hosts' documentation (no comments, no trailing commas)", () => {
    expect(parseConfigText('{\n\t"mcpServers": {}\n}\n').ok).toBe(true);
    expect(parseConfigText('{"a":1,}').ok).toBe(false);
  });
});
