# AGENT-02 — source evidence (Issue #53)

Retrieval date for every row below: **2026-11-14**. Method: a targeted quotation through a scrape tool, i.e. the publisher's page text as the tool returned it; no
program was installed, started or probed and no real configuration file was read. The full register rows (with their limits) are **R58, R59 and R60** in
`docs/RESEARCH-SOURCES.md`; earlier rows are not edited (R23–R25 were already duplicated before this phase and are left as they were).

| Register id | Publisher page | What was quoted / recorded | What it does **not** show |
|---|---|---|---|
| R58 | Codex IDE extension (`developers.openai.com/codex/ide`, redirected to `learn.chatgpt.com/docs/codex/ide`) | "VS Code and compatible editors use the Codex extension; Xcode and JetBrains IDEs provide their own integrations." Lists VS Code, Cursor, Windsurf, VS Code Insiders, Xcode, JetBrains IDEs. | Anything about configuration files, MCP, skills, trusted projects or operating systems for the extension. |
| R59 | Cline MCP overview (`docs.cline.bot/mcp/mcp-overview`, site `dateModified` 2026-10-07) | CLI user-level file `~/.cline/data/settings/cline_mcp_settings.json`, override `CLINE_MCP_SETTINGS_PATH`; transports Streamable HTTP, SSE (legacy, default when `type` omitted), STDIO; "MCP also works in Cline CLI". | A project-scoped MCP file; any VS Code or JetBrains settings path; skills, plugins, rules, Desktop and IDE pages (not read). |
| R60 | Kiro MCP configuration (`kiro.dev/docs/mcp/configuration/`) | Workspace `.kiro/settings/mcp.json`, user `~/.kiro/settings/mcp.json`; priority agent config > workspace > global; saved changes apply automatically; `mcpServers` with `command`/`args`/`env` or `url`/`headers`/`oauth`; `autoApprove` (`"*"` = all tools); advice to use env-var references for secrets. | Which statement belongs to the IDE tab and which to the CLI tab (the page has an IDE / CLI / Web selector); skills, operating systems. |

## Documentation conflicts and unknowns carried forward

* Cline CLI Node.js requirement: the design review saw two pages that disagree; **not re-checked** and therefore unknown.
* Codex CLI and IDE extension sharing `~/.codex/config.toml`: noted in an earlier session, **not re-verified** on 2026-11-14 and not used for `codex-vscode`.
* Cline VS Code and Cline JetBrains MCP settings paths: **unknown** (no page read).
* Kiro page content is shared across IDE/CLI/Web; recorded for both Kiro surfaces as the publisher's shared page, not as per-surface proof.
* Cline Desktop and the Codex desktop experience are **not** surfaces in this phase; they are committed work in Issue #54.
