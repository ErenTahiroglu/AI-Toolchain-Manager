# AGENT-01 — hosts, providers and protocols: what is what

Tracking issue: [#31](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/31) (stays open for independent review). Baseline (verified before any edit): `development` = `origin/development` = `51c4661cdd4dbf773cc01449195db2cf437240a5`, `origin/main` = `68eda3f0407490fabccb517dcf822ab4818fc0bf` (untouched), clean tree, one worktree. Issues #14, #25 and #29 stay open. **REAL_INSTALLATION_NO_GO. REAL_CONFIG_WRITE_NO_GO.**

The commit's own SHA and the exact-SHA CI result are not written in this file (a commit cannot contain its own hash or a result that does not exist yet; no documentation-only follow-up commit was made). They are in the hand-off reply and in the comment on Issue #31.

All sources below were read on **2026-10-25**, as page text through a page-to-markdown scrape (a live fetch, not a summary; see the limits per row). They are registered as R48–R54 in `docs/RESEARCH-SOURCES.md`. The pages are data to describe, not instructions to this work: no command from them was run, nothing was installed, no key or account was used, and no file was written.

## The four things that must never be mixed

| Kind | What it is | Examples here | What it is **not** |
|---|---|---|---|
| **Agent client / host surface** | A program (CLI, IDE extension, desktop app) that can discover and use MCP tools and/or skills. The scope (project / user / system) differs per host **and per surface** of the same product. | Claude Code (terminal), Antigravity IDE, **Codex CLI**, Gemini CLI | Not a model; not a service you call |
| **Model / API provider** | A service a host calls with an API key. | DeepSeek API, GroqCloud, xAI (Grok), Gemini API | Not a host; has no project MCP file of its own; a brand name proves nothing about a config path |
| **Protocol / capability** | MCP over stdio or HTTP; skills (`SKILL.md`); host plugins / marketplaces; text instruction files (`AGENTS.md`, `CLAUDE.md`); provider/model API configuration. | — | **Not interchangeable formats.** There is no universal `settings.json`. |
| **Platform** | Windows x64 or macOS Apple Silicon. | — | Independent of host, provider and demonstrated compatibility; part of the stored preference key and of the catalog evidence, not of this registry |

Easy mistakes the application now refuses to make (and tests pin):
**Grok ≠ Groq** (xAI vs GroqCloud). **Gemini CLI ≠ Gemini API** (a client vs a provider). **DeepSeek ≠ a fixed project-MCP client** (a provider). **Codex CLI ≠ a generic `local-cli` preference**, and **Codex CLI ≠ an automatically approved Codex IDE / desktop app**.

## Examples, classified (what the guide's lookup answers)

| Name | Class | Facts used | Source | Treated by the application as |
|---|---|---|---|---|
| Claude Code | agent host | project MCP file `.mcp.json` (JSON) | R45, R46 | selectable surface `claude-code-terminal` (CONFIG-01 JSON preview) |
| Antigravity IDE | agent host | workspace file `.agents/mcp_config.json` (JSON); app / CLI / IDE / SDK are different interfaces | R47 | selectable surface `antigravity-ide` (CONFIG-01 JSON preview) |
| **Codex CLI** | agent host | project file `.codex/config.toml` (**TOML**), **trusted projects only** | R48, R49 | **new** selectable surface `codex-cli`; TOML preview **not implemented**, static manual guidance only |
| Gemini CLI | agent host (example only) | MCP servers in `settings.json`: user `~/.gemini/settings.json`, project `.gemini/settings.json`; the page says it was replaced by Antigravity CLI for unpaid-tier and Google One users on 2026-06-18 | R50 | **not a surface**; worked example in the manual guide |
| Gemini API | model/API provider | — | R50 (distinction only) | guide example; no host, no file |
| DeepSeek | model/API provider | the publisher's page sets DeepSeek up *as a model provider inside Codex* (`model_provider`, `[model_providers.deepseek]`, an API key in the file) and offers a one-click pipe-to-shell script | R51 | guide example; **never** a project-MCP client; script never used |
| Grok (xAI) | model/API provider | Responses API at `api.x.ai`, API key; no local MCP host documented | R52 | guide example |
| Groq (GroqCloud) | model/API provider | OpenAI-compatible inference API; documents its own *remote-MCP tool use* (a provider-side feature) | R53 | guide example; not a local client configuration |
| anything else | uncertain | — | — | `MANUAL_VERIFY`: no path, no format, no "compatible", no "installed" |

The classification in the application is an **exact, normalised lookup** of this table (`src/agentGuide.ts`). It is deliberately not fuzzy: "gemini" alone, "groc", "grokk", "codex" (without `-cli`), "codex desktop" and "openai" are all *uncertain*.

## Capabilities documented per managed surface (`src/agentHosts.ts`)

`documented` = a publisher page read **for this phase** states it. `not-assessed` is **not** a claim of absence.

| Surface | mcp_stdio | mcp_http | skills | plugins | project_instructions | Project file / format | Preview |
|---|---|---|---|---|---|---|---|
| `claude-code-terminal` | documented (R45) | documented (R45) | not-assessed | not-assessed | documented (`CLAUDE.md`, R46) | `.mcp.json` · JSON | JSON diff (CONFIG-01) |
| `claude-desktop-code` | not-assessed | not-assessed | not-assessed | not-assessed | not-assessed | none documented | none |
| `antigravity-ide` | documented (R47) | documented (Streamable HTTP only, R47) | not-assessed | not-assessed | not-assessed | `.agents/mcp_config.json` · JSON | JSON diff (CONFIG-01) |
| `codex-cli` | documented (R48) | documented (R48) | not-assessed | documented (R48: plugins may bundle MCP servers) | documented (`AGENTS.md`, R49) | `.codex/config.toml` · **TOML** | **TOML_PREVIEW_NOT_IMPLEMENTED** |
| `local-cli` | not-assessed | not-assessed | not-assessed | not-assessed | not-assessed | none (a computer-level program lookup, not an agent) | none |

Registry fields per surface: `clientHostId`, `agentSurface`, `configScope`, `configFormat`, `relativeTargetPath` (fixed text, only where a publisher documents it), `scopeCondition`, `previewKind`, `capabilities`, `planSupported`, `evidenceIds`, `untouched`. The **platform** is intentionally *not* a field: it stays a separate dimension of the preference key and of the catalog's compatibility rows. A provider's brand name never sets or overrides any field (a test scans the identifying fields for provider names).

## Discrepancies found while reading (logged, not silently resolved)

1. The Codex URLs in the task (`developers.openai.com/codex/mcp`, `…/codex/config-advanced`) resolved to `learn.chatgpt.com/docs/extend/mcp?surface=cli` and `…/config-file/config-advanced`. Neither page shows a publication date; only the retrieval date is recorded.
2. The DeepSeek URL in the task (`api-docs.deepseek.com/guides/codex`) returned the site's **"Page Not Found"**. The real page, `…/quick_start/agent_integrations/codex/`, was found by a domain-restricted search and **read only as an excerpt** (R51). Nothing here depends on text outside that excerpt.
3. The xAI documentation now calls itself "SpaceXSI Docs" (R52); the product is still the Grok API at `api.x.ai`.
4. The Gemini CLI page carries a banner that unpaid-tier and Google One users' Gemini CLI was replaced by Antigravity CLI on 2026-06-18 (R50). That is why Gemini CLI stays *an example*, not a surface.
5. GroqCloud's official documentation is `console.groq.com/docs` (R53), validated by reading it; it is a different company and product from xAI's Grok.
6. **Not read:** `developers.openai.com/resources/docs-mcp` (the OpenAI Docs MCP server) and `developers.openai.com/plugins/build/plugins` (building plugins). The MCP page (R48) links to both; nothing in this phase depends on them. Plugin packaging for Codex is therefore *not assessed*.

## What is shared and what is not (for the guide)

An MCP **server** (a program or a hosted address) can be the same for several clients. Everything around it is client-specific: the file, its format, the key names, the trust/approval rule, how secrets may be referenced, whether `${VAR}`-style expansion exists. Skills, plugins/marketplaces and instruction files are different mechanisms again. The application therefore keeps **one** typed registry for the surfaces it manages and **one** manual workflow for everything else — it does not grow a brand-named adapter per vendor.
