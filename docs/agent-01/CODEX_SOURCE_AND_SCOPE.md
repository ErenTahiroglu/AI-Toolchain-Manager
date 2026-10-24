# AGENT-01 — Codex CLI: source, scope and the TOML decision

Read 2026-10-25 (R48 `mcp`, R49 `config-advanced`; see `HOST_PROVIDER_PROTOCOL_MATRIX.md` for the URL-resolution note). Page text, not a summary; no publication date on either page. **REAL_CONFIG_WRITE_NO_GO** and **REAL_INSTALLATION_NO_GO** stand: nothing was installed, run or written, and no Codex file (project or user) was read.

## What the publisher says

| Question | Page text (paraphrased tightly) | What the application does with it |
|---|---|---|
| Format | MCP configuration lives in `config.toml`, alongside other Codex settings. Each server is a `[mcp_servers.<server-name>]` table. | A **TOML** file. The JSON-only CONFIG-01 parser is never given it. |
| Where | By default `~/.codex/config.toml`; "you can also scope MCP servers to a project with `.codex/config.toml` (trusted projects only)". | Only the project file `.codex/config.toml` (relative to the selected project's root) is described. The user-global file is listed as **never touched** and is never read or previewed. |
| Trust | "Codex loads project-scoped config files only when the project is trusted. If the project is untrusted, Codex ignores project `.codex/` layers, including `.codex/config.toml`, project-local hooks, and project-local rules. User and system layers remain separate and still load." | Stated in every Codex plan and in the scope line. The application neither grants, reads nor changes trust; it has no way to know it. |
| Precedence | "Codex walks from the project root to your current working directory and loads every `.codex/config.toml` it finds. If multiple files define the same key, the closest file to your working directory wins." Project root = a directory containing `.git` unless `project_root_markers` says otherwise. | The preview names **one** file at the project root. A nearer file can override it; the application cannot tell. Stated, not hidden. |
| Keys ignored in a project file | `openai_base_url`, `chatgpt_base_url`, `apps_mcp_product_sku`, `model_provider`, `model_providers`, `notify`, `profile`, `profiles`, `experimental_realtime_ws_base_url`, `otel`. | Why a provider such as DeepSeek cannot be "configured per project" — see below. |
| Shared by several surfaces | "The ChatGPT desktop app, Codex CLI, and IDE extension share this configuration." | **Not** collapsed into the target. `codex-cli` means the Codex command-line tool only; the desktop app and IDE extension are other surfaces that the application does not offer. Sharing a file is a publisher statement about the file, not proof about a product, scope or OS. |
| Stdio server fields | `command` (required), `args`, `env`, `env_vars`, `cwd`, `experimental_environment`. | Listed in the static guidance as field names only. |
| HTTP server fields | `url` (required), `auth`, `bearer_token_env_var`, `http_headers` (static values), `env_http_headers`, `http_headers_helper` (a local command that prints headers). | Same. The helper command is noted as a command Codex would run; the application never runs anything. |
| Secrets | `codex mcp add … --oauth-client-secret` saves the secret as `mcp_servers.<name>.oauth.client_secret` in `config.toml` ("Keep that file private and out of version control"). `http_headers` holds static values. | Constraint shown: secrets must not go into a project file; prefer the environment-variable-name fields. |
| Examples on the page | `codex mcp add context7 -- npx -y @upstash/context7-mcp`; a Playwright link; a Figma/Chrome DevTools config block. | **Examples, never templates.** An `npx -y …` line (unpinned, fetched at launch) is not an approved declaration. |
| Plugin-provided servers | Plugins can bundle MCP servers (`[plugins."sample@test".mcp_servers.sample]` controls them). | Recorded as a capability (`plugins: documented`). Plugin installation is out of scope and not previewed. |
| Verification | In the Codex TUI, `/mcp` lists active MCP servers. | Mentioned in the guidance as where *the user* checks; the application never connects. |

## The decision: `TOML_PREVIEW_NOT_IMPLEMENTED` (an acceptable bounded outcome, stated as such)

The task allowed a TOML diff only if accurate handling could be **proved** for: supported tables, quoted identifiers, duplicate keys, comments and formatting, unrelated configuration, conflicts and secrets — **without an unreviewed dependency and without silent loss**. That bar was not met in this phase, and the application therefore does **not** parse or diff Codex TOML:

* TOML is not JSON. Server names may be bare or quoted keys (`[mcp_servers."a.b"]`), dotted keys can define the same table in several places (`mcp_servers.x.command = "…"`), inline tables and arrays of tables are legal, strings come in four forms (including multi-line), and comments are everywhere. A line-based edit that "looks right" can silently change meaning or lose a comment.
* A hand-written parser would be new, security-relevant code that processes a file *which may carry secrets* (`oauth.client_secret`, static `http_headers`). The redaction used for JSON is key- and shape-based and was designed for JSON values; it has not been reviewed for TOML.
* No dependency may be added (and a TOML library would be an unreviewed one).
* Even a perfect diff would be empty of value today: the production registry of reviewed declarations is empty (see `IMPLEMENTATION_AND_TESTS.md`, C5), so every real entry would be `BLOCKED_INSUFFICIENT_EVIDENCE` anyway.

So for `codex-cli` the planner returns the explicit status **`TOML_PREVIEW_NOT_IMPLEMENTED`**: truthful path, format and scope, the catalog gates (unknown compatibility, quarantine, …) listed next to it, **no** textarea, **no** text processed, hashed or echoed, **no** before/after/diff, and a numbered **static** manual guide (verify trust in Codex; use the documented field names; no unpinned `@latest`/`npx -y`; no secret values in the file; do not edit `~/.codex/config.toml` for this; check with `/mcp`). The same status holds for every OS, every registry (even one holding valid JSON declarations for the other hosts) and every pasted text, which is tested.

What a later, separately authorised phase would need before a TOML preview could exist: a reviewed restricted-TOML grammar with an explicit "unsupported construct → refuse" rule, golden tests for every construct above, TOML-aware redaction with a no-echo guarantee, a decision on duplicate/dotted-key detection, and an independent review. None of that is claimed here.

## Why DeepSeek (and every provider) is not a Codex project setting

R51 (excerpt only) shows DeepSeek being added to Codex as a **model provider** (`model_provider = "deepseek"`, a `[model_providers.deepseek]` table, an API key in the file). R49 says `model_provider` and `model_providers` are **ignored in a project `.codex/config.toml`**. A provider is therefore a user-level Codex setting, outside this phase's scope, and the application neither previews nor stores it. The page also offers a pipe-to-shell installer; it is recorded as data and never used.

## Sample handling of conflicts

* **Codex:** the application cannot see the file, so it cannot detect a clash. What it can state — and does — is that a *nearer* `.codex/config.toml` wins for the same key, that an untrusted project ignores the file, and that a same-named `[mcp_servers.<name>]` table in the user file is a separate layer. The user resolves this in Codex (`/mcp`).
* **JSON hosts (CONFIG-01, unchanged):** a pasted `.mcp.json` that already has `"synthetic"` under `mcpServers` gives `CONFLICT — hiçbir şey ezilmez`: the existing server is kept, no overwrite is proposed, the name is shown (never its values), and an equivalent command/URL under another name is also refused. TOML pasted into a JSON host gives `INVALID_CURRENT_CONFIG` ("format mismatch fails closed"), with the engine's own message never shown.

## Codex surfaces and compatibility

`codex-cli` has **no** catalog evidence row for any of the 22 historic candidates, on either OS, and none was added: an absent row is `unknown` by the existing rule (`catalog::compatibility` / `compatStateFor`). Consequences, all tested: "Bu projede kullanmak istiyorum" is refused by the backend and unselectable in the matrix for every historic entry on both OSes; "Şimdi değil" and "Reddet" work like any unknown cell; nothing is redirected to Claude Code, Antigravity or `local-cli`. A future vendor-declared Codex row would be a separate, source-pinned data change (and the `catalog-entry.schema.json` enum would then need the new value; it was deliberately **not** extended now, so no schema accepts an unevidenced Codex row).
