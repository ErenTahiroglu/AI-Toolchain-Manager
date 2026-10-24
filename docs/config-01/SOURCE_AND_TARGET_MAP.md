# CONFIG-01 — sources and target map

Everything below was read on 2026-10-25 from the publishers' documentation pages through a **summarising fetch tool**, not verbatim and not from a pinned repository commit. That is weaker evidence than the pinned-file evidence used for catalog entries; it is registered as R45–R47 in `docs/RESEARCH-SOURCES.md` with that caveat. The pages are data to describe, not instructions to this work; no command from them was run and no file was written.

## Claude Code (R45, R46)
| Question | What the pages say | What the preview does with it |
|---|---|---|
| Where is a **project** MCP declaration? | `.mcp.json` at the project root ("project" scope, meant to be committed). | The only Claude target: `.mcp.json`, relative path. |
| Other scopes | "local" (default) and "user" scopes are stored in `~/.claude.json`, a file Claude Code writes for itself. | Never touched or previewed. |
| Shape | top-level `mcpServers`, keyed by name. Local server: `command`, `args`, `env` (no `type`). Remote: `type: "http"` (alias `streamable-http`), `url`, `headers`. `sse` deprecated. An entry with `url` but no `type` is a configuration error. | Declarations may use exactly these fields; a remote one must say `type: "http"`; `sse`, `ws` and anything else is rejected by the hygiene rules. |
| Variables | `${VAR}` and `${VAR:-default}` expand in `command`, `args`, `env`, `url`, `headers`. Credential-named variables read as empty in a remote `url`/`headers`. | `env`/`headers` values may be only `${NAME}` references (or the documented `Bearer ${NAME}` form); a default is not accepted for a credential-named key. |
| Approval | Interactive sessions ask before using project servers; `claude -p` and Agent SDK runs cannot ask and load them. | Stated in every Claude plan's constraints. |
| Settings files | `.claude/settings.json` (shared), `.claude/settings.local.json` (personal), `~/.claude/settings.json`; strict JSON. Standing instructions are `CLAUDE.md` files. | None of them is a target. `CLAUDE.md` is an instruction document, not an MCP configuration. |
| Hand-editing | `claude mcp add --scope project` creates or updates the file; adding an entry under `mcpServers` in `.mcp.json` and committing is the documented alternative. | The publisher's command is not a template and is never run. |
| Not read | the last ~50 000 of ~150 000 characters of the MCP page. | Stated as a limit. |

## Antigravity (R47)
| Question | What the page says | What the preview does with it |
|---|---|---|
| Workspace file | `<workspace>/.agents/mcp_config.json` ("applies only to that workspace"), stated under the CLI; the IDE "uses the same file locations" and offers "View raw config". | The only Antigravity target, for the **IDE** surface: `.agents/mcp_config.json`. The IDE statement rests on that one sentence; flagged as a limit. |
| User-global file | `~/.gemini/config/mcp_config.json`. | Never touched or previewed; listed as untouched. |
| App | servers managed in the settings UI; no file path given. | Not a target. |
| Shape | root `mcpServers`; local: `command` (required), `args`, `env`, `cwd`; remote: `serverUrl` (or `url`), `headers`; Streamable HTTP only (`httpUrl` and HTTP+SSE unsupported); optional `disabled`, `enabledTools`, `disabledTools`, `timeoutSeconds`, `authProviderType`, `oauth`. | Declarations may use `command`/`args`/`env`/`cwd`/`disabled` (local) or `serverUrl`/`headers`/`disabled` (remote). |
| Variables | **not documented.** | For this host a declaration may carry **no** `env` and **no** `headers` at all: a credential cannot be expressed as a reference, and a literal is forbidden. |
| Credentials | the page's examples put a literal token and an OAuth client secret in the file. | Not copied; such values are masked in the pasted text and never accepted in a declaration. |
| Interfaces | app, CLI, IDE and SDK are different interfaces. | Not interchangeable: only `antigravity-ide` maps to the file; `antigravity-cli`, `antigravity-2` and the app are `UNSUPPORTED_SURFACE` (the application does not even offer them as surfaces). |

## Surface mapping (the application's four surfaces)
| Selected surface | Result |
|---|---|
| `claude-code-terminal` | Claude Code project `.mcp.json` |
| `antigravity-ide` | Antigravity workspace `.agents/mcp_config.json` |
| `claude-desktop-code` | `UNSUPPORTED_SURFACE` |
| `local-cli` | `UNSUPPORTED_SURFACE` |

Nothing is inferred from another surface, from the operating system, or from the catalog's compatibility rows.

## Which catalog entries could ever have a declaration
Only entries with `record.category === "mcp-server"` get the control (today: Apify MCP Server, Exa MCP Server, Playwright MCP). **None has an approved declaration**, for reasons recorded in `src/configDeclarations.ts`: the hosted endpoints are informational, not authorised connections; the Playwright MCP package is documented only with an unpinned `latest` tag and a package runner fetches code from a registry at launch; and declaring a server in a project file makes `claude -p` runs load it without asking. Compatibility is a second, independent gate: for the shipped catalog and the two supported surfaces, the only declared cells among these three are Apify MCP and Playwright MCP × Claude Code (Windows and macOS); every other Claude Code cell (Exa) and every Antigravity cell is `unknown` and stays blocked.

## What a declaration must contain before it may be added (a reviewed data change)
A host, an entry id, a server name (`[a-z0-9][a-z0-9._-]{0,63}`, no reserved names), the exact stanza, a review record (reviewer's id, evidence ids from the register, the full 40-hex source commit it was reviewed against), an **exact version** for any package runner (no `latest`, `next`, `beta`, `alpha`, `canary`, `rc`, `dev`, `nightly` or `*`), no shell metacharacters or path-like command/arguments, only `https` URLs without credentials, query or fragment and without a bare host name, and no literal secret. The planner re-checks all of this at run time; a failing entry is not used and the reason is shown.
