# CATALOG-04 source evidence (evidence id R57)

## 1 Scope
What was read to write `data/catalog-candidates-c04.json`. Metadata only: not a review, a test or an authorisation. Nothing was installed or run by ATM.

## 2 Files read
| File (at commit e9e1f06aa4e26e95cd4ed37210a0632202c429e9) | Statement taken |
|---|---|
| `.claude-plugin/plugin.json` | plugin `egm`, version 0.1.1, licence field Apache-2.0, repository URL |
| `.claude-plugin/marketplace.json` | plugin `egm`, `"source": "./"` (the plugin is the repository root) |
| `.claude-plugin/agents/*.md` (10 files) | reviewer agents declare `tools: [Read, Grep, Glob]` and `omitClaudeMd: true`; the bootstrap orchestrator `[Agent, Bash, Read, Grep, Glob, Write, Edit]`; the synthesizer `[Agent, Read, Grep, Glob, Write]` |
| `LICENSE` | Apache License 2.0 text |
| `docs/claude.md` | Claude Code 2.1.271 or later for `omitClaudeMd` support; older versions are not treated as verified isolation environments; the synthesizer's `Write` is not path-restricted by the host; Windows path syntax for permission rules; no support statement for Windows, macOS or a CPU architecture |
| `docs/privacy.md` | no hosted service, database, analytics client or telemetry endpoint; the host processes files under its own account, terms and retention; bootstrap can run local discovery commands and a project-configured formatter; no MCP server and no permission-approval hook declared |

The tree also has folders for other hosts (`.codex-plugin`, `.cursor-plugin`, `.gemini` appear in the marketplace clone); their content was not read.

How the files were obtained and compared (2026-11-08): read from the locally installed copy `~/.claude/plugins/cache/egm/egm/0.1.1`, whose commit is the `gitCommitSha` of `installed_plugins.json`. Read-only `gh` calls showed that the commit exists (committed 2026-11-07T03:11:51Z), that `main` was identical to it, and that the git blob ids of these 15 files (14 files of the copy and `marketplace.json` of the marketplace clone) equal the blob ids of the same paths at that commit after line endings are normalised to LF.

## 3 Hooks and .mcp.json search
The tree at the commit has 401 paths; none is `hooks`, `hooks.json`, `.mcp.json` or `mcp.json` (listing through the GitHub API). A text search of the manifests, agent files and skill files of the copy found no `hooks` or `mcpServers` key.

## 4 Not checked
Runtime telemetry behaviour; how the host fetches the marketplace; support on Windows, macOS or any CPU architecture; the adapters for other hosts (for example Codex); other revisions of the repository; the exact number of `SEED_SOURCE_TAKEN` diagnostics per run (the diagnostics layer was not read).

## 5 Corrections to earlier drafts
Revision 1 recorded telemetry as `unknown`; the publisher declares none (`none-declared`). Revision 1 said a user-added EGM row leaves the total at 22; the total is 23 in both cases. The first test-pin register missed the inline tests of `catalog.rs` and `db/migration_tests.rs`.
