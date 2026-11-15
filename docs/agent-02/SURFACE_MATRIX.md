# AGENT-02 — surface identity and capability matrix

Owner decisions D1–D3 and D5 (Issue #53) fix the set: the existing `codex-cli` plus six new project target surfaces. A surface is only what its id says; nothing
is copied from a sibling surface of the same client. "documented" means a publisher page read on 2026-11-14 says it (see `SOURCE_EVIDENCE.md`); it is **not** a test.

| Surface id | Label | Client family | Config scope | Documented path | Format | Preview | MCP stdio / HTTP | Skills / plugins / instructions | Evidence |
|---|---|---|---|---|---|---|---|---|---|
| `codex-cli` (existing) | Codex CLI | codex | project | `.codex/config.toml` | TOML | `toml-not-implemented` (`TOML_PREVIEW_NOT_IMPLEMENTED`) | documented / documented | plugins and instructions documented (AGENT-01), skills not-assessed | R48, R49 |
| `codex-vscode` | Codex VS Code | codex | none-documented | — (null) | — | none | not-assessed | not-assessed | R58 (identity only) |
| `cline-cli` | Cline CLI | cline | none-documented | — (null; the documented file is user-level) | — | none | documented / documented | not-assessed | R59 |
| `cline-vscode` | Cline VS Code | cline | none-documented | — (null) | — | none | not-assessed | not-assessed | none read |
| `cline-jetbrains` | Cline JetBrains | cline | none-documented | — (null) | — | none | not-assessed | not-assessed | none read |
| `kiro-cli` | Kiro CLI | kiro | project | `.kiro/settings/mcp.json` | JSON | **none** | documented / documented | not-assessed | R60 |
| `kiro-ide` | Kiro IDE | kiro | project | `.kiro/settings/mcp.json` | JSON | **none** | documented / documented | not-assessed | R60 |

## CLI versus IDE
* `codex-cli` and `codex-vscode` are separate ids, preferences and evidence rows. `codex-vscode` means the VS Code extension only — not Cursor, Windsurf, Xcode or JetBrains.
* `cline-cli`, `cline-vscode`, `cline-jetbrains` are three ids. A preference for one is never displayed for another (pinned by `src/Agent02.test.ts` and the Rust tests).
* `kiro-cli` and `kiro-ide` share a documented workspace path because the publisher page is shared; they remain separate ids and separate preferences.
* No generic `codex-ide`, no `cline`, no `kiro`, no desktop id exists.

## What a surface id does not prove
Installed, running, configured, verified, authorized to execute — none. Compatibility of a catalog entry with a new surface is **unknown** until a catalog evidence row
exists (none does), so the existing matrix refuses "use it in this project" for every entry on every new surface; postpone / reject / reset stay available.
A documented JSON path (Kiro) does not authorize reading, previewing or writing it: `previewKind` is `none` and `src/configPlan.ts` answers `UNSUPPORTED_SURFACE`.

## Files ATM never touches (recorded per surface in `src/agentHosts.ts`, `untouched`)
`~/.codex/config.toml`, `.codex/config.toml` for the VS Code surface, `~/.codex/auth.json`; `~/.cline/data/settings/cline_mcp_settings.json`; `~/.kiro/settings/mcp.json`,
Kiro agent JSON files, and the `autoApprove`, `oauth` and `headers` fields; editor settings of VS Code and JetBrains.

## Providers are not hosts
Gemini API, Grok (xAI), Groq, DeepSeek and every other model/API provider stay classified as providers (`src/agentGuide.ts`); none is a surface.
