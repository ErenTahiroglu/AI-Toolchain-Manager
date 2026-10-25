# CONFIG-02A — sources and the one reviewed declaration

Issue #38. Status of everything below: **a syntax-only, preview-only proposal that the developer compared against the sources; runtime behaviour is NOT verified and nothing here is independently reviewed yet.** `REAL_INSTALLATION_NO_GO` and `REAL_CONFIG_WRITE_NO_GO` are unchanged.

## The declaration

| Field | Value |
|---|---|
| Catalog entry | `exa-mcp-server` (existing entry; not a new candidate) |
| Host | Claude Code, project scope, file `.mcp.json` at the selected project's root (surface `claude-code-terminal`) |
| Server name written | `exa` |
| Stanza | `{ "type": "http", "url": "https://mcp.exa.ai/mcp" }` (nothing else: no `headers`, no `env`, no query string, no `tools=`) |
| Bound to | publisher repository `https://github.com/exa-labs/exa-mcp-server` at the catalog's pinned commit `0b499f095c4ca145eee07b09137641c037d58449` |
| Review id / evidence | `SRC-CONFIG-02A-EXA-1`; R37 (catalog pin), R55 (publisher README at the pin), R56 (Claude Code MCP page) in `docs/RESEARCH-SOURCES.md` |
| Where it lives | `src/configDeclarations.ts` (`REVIEWED_DECLARATIONS`, exactly one entry) |

The planner refuses to use the declaration when the catalog record's `sourceSnapshot.immutableCommit` or `repository.url` differs from the ones above (`source_mismatch`): a re-pinned catalog entry needs a new review, it does not inherit this one.

## What each source establishes (and what it does not)

**Exa README at the pinned commit (R55).** Says: the hosted MCP server is `https://mcp.exa.ai/mcp`; it "works anonymously with rate limits"; OAuth is preferred; an API key can be given in the URL, an `Authorization: Bearer` header or `x-api-key`; `?tools=` selects tools. Its generic JSON example uses `"type": "streamable-http"` and, as published, has a trailing comma after the `url` line, so it is **not valid JSON** and is not copied. The publisher's own VS Code install link encodes `{"type":"http","url":"https://mcp.exa.ai/mcp"}`, which is the same shape as the declaration. The README gives no Claude Code project-file snippet (its Claude path is a plugin install command, which this application never runs).

**Claude Code documentation (R56).** Says: project scope is `.mcp.json` at the project root with a top-level `mcpServers` object; a remote server is written `"type": "http"` with a `url`; `"streamable-http"` is accepted as an alias; a `url` without a `type` is an error. Interactive sessions ask before using a project server; headless runs load project servers without asking — this is repeated on screen.

**Why `http` and not the README's `streamable-http`.** Claude Code's own page names `http` first and treats `streamable-http` as the alias, and the publisher's own VS Code link uses `http`. The draft uses the host's documented primary name instead of copying the generic example. The on-screen warnings say so.

**Not established by any source read here** (these are the unverified assumptions):
1. That `mcp.exa.ai` is operated by the publisher, is reachable today, is safe, or serves MCP over this transport. No request was made.
2. That the installed Claude Code version accepts this stanza (the docs were read as pages; Claude Code was never run).
3. That anonymous use is free or acceptable for the user's data, or what its limits are; privacy and billing terms were not read.
4. Anything about authentication flows (OAuth sign-in happens in Claude Code, not here).
5. That the documentation pages still say the same thing: they are living pages read on 2026-10-25; only the README is pinned to a commit.

## What the user sees

- The required notice, verbatim: "Yalnızca yapılandırma taslağı; uygulanmadı, bağlantı kurulmadı. Hizmetin ücretleri, gizliliği ve kimlik doğrulaması ayrıca değerlendirilmelidir."
- A "Gözden geçirilmiş bildirim" line saying the declaration is syntax-only, preview-only, runtime not verified, and awaiting independent review.
- Four constraint sentences (third-party hosted service/cost/privacy; no key, token, header or query parameter; endpoint identity unverified and no connection made; `http` versus the README's `streamable-http` and its invalid trailing comma).
- When the catalog compatibility for the chosen OS and surface is **unknown**, an extra sentence that this is a draft and that "kullanmak istiyorum" still cannot be chosen from it.

## Viewing a draft is not accepting it

CONFIG-02A changes exactly one gate. An UNKNOWN compatibility no longer stops the *text* draft from being shown for a JSON host; it is shown as a constraint. Everything that makes an entry unusable still stops it: removed, disabled, execution-blocked, quarantined, blocked, explicitly unsupported, not an MCP server. The matrix's own acceptance rule (`acceptBlockReason` in `src/workspace.ts`) is untouched: an unknown cell still cannot be accepted, and nothing in the preview can change a preference. Codex (TOML) keeps its old gate and its `TOML_PREVIEW_NOT_IMPLEMENTED` result; Antigravity and every other entry still get `BLOCKED_INSUFFICIENT_EVIDENCE`. Nothing falls back to the Claude JSON shape.
