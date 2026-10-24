# CONFIG-01 — project MCP configuration diff preview: behaviour and scope

Tracking issue: [#30](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/30) (stays open for independent review). Baseline (verified before any edit): `development` = `origin/development` = `dbd035a0708329d3cc0e3a75f4f617a71ed5b76c`, `origin/main` = `68eda3f0407490fabccb517dcf822ab4818fc0bf` (untouched), clean tree, one worktree. Issues #14, #25 and #29 stay open. **REAL_INSTALLATION_NO_GO. REAL_CONFIG_WRITE_NO_GO.**

The commit's own SHA and the exact-SHA CI result are not written in this file (a commit cannot contain its own hash or a result that does not exist yet; no documentation-only follow-up commit was made). They are in the hand-off reply and in the comment on Issue #30.

## What this phase delivers
A deterministic, **read-only** preview of what a project's MCP configuration file would look like if one *reviewed* declaration were added — and an honest "blocked" answer when it must not be shown as a recipe. It is a **preview, not availability and not execution**: nothing is applied, installed, started, connected, authenticated or written, and no file is read.

* **A pure planner** (`src/configPlan.ts`, no I/O, no IPC, no clock, no randomness). Input: the selected project alias, operating system, agent surface and catalog entry; the configuration **text the user pastes** (or an explicit "new file"); and a registry of exact, independently reviewed declarations. Output: a `PreviewPlan`.
* **A registry of reviewed declarations** (`src/configDeclarations.ts`) that is **empty** in this phase. Nothing qualifies yet (see below). Adding an entry is a reviewed *data* change, never a code path.
* **A control in the matrix row** (`src/components/ConfigPreview.tsx`, mounted by the Project Tool Workspace row for catalog entries of category `mcp-server` only): "Yapılandırma önizlemesi — salt okunur, hiçbir dosyaya yazmaz" with a "Değişiklik önizle" button, side-by-side before/after, a focused diff, the target path, constraints, and the sentence that no file was touched. The row already fixes project, OS, surface and entry, so nothing is chosen for the user and no second selector exists.

## What it deliberately is not
No "Uygula", no write, no export, no download, no clipboard, no automatic preference or wish, no new preference-writing surface (the matrix's own save button stays the only one), no IPC command, no Tauri capability, no dependency, no migration, no Rust change. No `.claude/settings.json`, `.claude/settings.local.json`, `CLAUDE.md`, `AGENTS.md`, hook, plugin-manager or global/user file is touched, planned or previewed. No jq-pilot, offline-preflight, WebView2, GUI or CDP use.

## The plan
| Field | Meaning |
|---|---|
| `status` | one of `PREVIEW_ONLY_NOT_APPLIED`, `BLOCKED_INSUFFICIENT_EVIDENCE`, `BLOCKED_BY_CATALOG`, `UNSUPPORTED_SURFACE`, `INVALID_INPUT`, `INVALID_CURRENT_CONFIG`, `CONFLICT`; shown as a label **and** the code, and the words "ÖNİZLEME — UYGULANMADI" are on every result |
| `applied: false`, `touchedFiles: 0` | constants of the type; the notice "Hiçbir dosyaya dokunulmadı ve hiçbir dosya okunmadı …" is on every result |
| identity | alias, platform, surface, entry id, host id; the plan's `inputKey` covers them plus the entry's catalog state, the text digest and the declaration |
| `relativePath`, `scope` | one fixed relative path per host (below), never built from input |
| `blockers`, `problems`, `constraints` | why there is no recipe; what is wrong with the supplied text; source, compatibility and trust constraints |
| `original`, `proposed`, `diff`, `hypothetical` | redacted, normalised before/after and a focused diff — only when a reviewed declaration exists and the text is usable; `hypothetical` marks an explanatory diff for a blocked entry |
| `maskedValues`, `sourceDigest` | how many values were masked; SHA-256 of the *pasted text* (not of any file) for staleness |

### Status precedence
`UNSUPPORTED_SURFACE` → `INVALID_INPUT` → `INVALID_CURRENT_CONFIG` (the supplied text itself is bad and must be fixed first) → `BLOCKED_INSUFFICIENT_EVIDENCE` (no reviewed declaration, or the registry entry failed the hygiene rules) → `BLOCKED_BY_CATALOG` (a reviewed declaration exists but the catalog state forbids it: explanatory diff only) → `CONFLICT` → `PREVIEW_ONLY_NOT_APPLIED`. Several reasons can be listed at once; the first applicable status is the headline.

## Targets
| Application surface | Host file (relative to the selected project root) | Notes |
|---|---|---|
| `claude-code-terminal` | `.mcp.json` | project-shared scope; `mcpServers` object |
| `antigravity-ide` | `.agents/mcp_config.json` | workspace scope; `mcpServers` object; the IDE only |
| `claude-desktop-code`, `local-cli`, anything else | — | `UNSUPPORTED_SURFACE`; no file is guessed, no other host is substituted |

Both operating systems use the same relative path; OS, project and entry are part of the plan's identity and its staleness key. See `SOURCE_AND_TARGET_MAP.md` for the evidence.

## Gates the preview cannot lift
Quarantined or `blocked`-state entries, `blocked-until-scoped-authorization`, removed or disabled entries, entries whose compatibility for the exact OS × surface is not declared (unknown stays unknown), and entries that are not MCP servers all stay blocked. For them a diff exists only if a reviewed declaration exists, and then it is labelled "AÇIKLAYICI — bu bir tarif değildir". The matrix's existing "kullanmak istiyorum" rules are untouched and tested as unchanged. "Yayıncı beyan etti" is shown as exactly that, never as supported, working or installed.

## The current configuration: pasted text, strictly validated
The application reads **no** file; the user pastes the text or chooses "Yeni dosya". The text is refused (no repair) when it is empty, over 200 000 characters, not valid JSON (JSONC / comments / trailing commas included — the hosts' documentation describes strict JSON), not an object at the top level, has duplicate keys in any object, has `mcpServers` that is not an object, a server that is not an object, or more than 200 servers. The JSON engine's own message is never shown (it can quote the text). A same-named server, or one with the same command line or URL under another name, is a `CONFLICT` and is never overwritten. Unrelated keys and servers are preserved semantically and in order; the only difference in the proposal is one added stanza (the display is re-formatted, and says so).

## Secrets
Every value shown is parsed, redacted and re-serialised; the raw text is never put in a plan, a log or a test output. Masked: values under keys that look secret-bearing, `--api-key`-style flags and their next argument, URL credentials, queries and fragments, and well-known token shapes (including long opaque strings). `${NAME}` references are kept (they are references, not values). The textarea content lives in component state only — no storage, no clipboard, no export. If masked values exist the plan says so and that nothing can be produced from the masked view.

## Staleness
A displayed result is withdrawn the moment any input differs from the one it was made from: the pasted text, the mode, the project, OS, surface, entry or its catalog state, or the declaration. The user must press "Değişiklik önizle" again.

## Documentation caveat that matters
Claude Code's documentation says project-scoped `.mcp.json` servers are approved interactively, but that `claude -p` and Agent SDK runs "load project-scoped servers without asking". The preview says so in its constraints: a file that is eventually committed to a project is not only a personal setting.
