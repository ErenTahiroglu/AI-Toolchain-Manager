# Phase 2A — Read-Only Local Tool Discovery and Installation Inventory

**Status:** implemented on `development`; awaiting independent Red Team review. Not merged to `main`. Phase 2B has not started.
**Baseline:** Phase 1B closure `312316f595da7bb251de4eff73785fdd30911769` (CI green on Windows x64 and macOS ARM64).

Phase 2A observes this computer. It never installs, downloads, updates, executes, verifies or configures anything.

## Four facts that are never merged

| Fact | Meaning in the app | Phase 2A |
| --- | --- | --- |
| **Presence** | Something with that name or identity was found in a documented place | observed |
| **Configuration** | A configuration file refers to it (enabled, disabled, or referenced but absent) | observed |
| **Identity** | It is exactly the catalogued tool (not just a similar name) | only for an exact `plugin@marketplace` match |
| **Functionality** | It was run and tested | **never** (`functionality_tested = 0` is a SQL constraint; no program is run) |

A user's preference ("I want this enabled") lives in its own tables and is never changed by a scan; the inventory shows the two side by side.

## Evidence: where each detector's method comes from

Methods were taken from official documentation read on 2026-10-17 (R27–R28 in [RESEARCH-SOURCES](RESEARCH-SOURCES.md)); nothing was guessed.

| Detector | Documented basis | Result vocabulary |
| --- | --- | --- |
| Claude Code program | Native installer location `~/.local/bin/claude` (macOS: a symlink by design) and `%USERPROFILE%\.local\bin\claude.exe` (Windows); legacy `~/.claude/local/`; otherwise a `claude` file in a PATH folder | present (medium/low confidence) · absent (only those places were searched) |
| Claude Code settings files | `~/.claude/settings.json` (user), `.claude/settings.json` (project), `.claude/settings.local.json` (local) | configured · not configured · malformed · unreadable |
| Plugins (Frontend Design, Superpowers, Ponytail, Caveman) | Claude Code's three stages: **declared** (`enabledPlugins`, key `plugin@marketplace`) → **fetched** (`~/.claude/plugins/installed_plugins.json` record + `cache/<marketplace>/<plugin>/<version>/` on disk) → **loaded** (running session). The third stage cannot be observed and is never claimed | present+configured · referenced-but-not-present · record without files (unknown) · disabled · not configured. Version only from the registry |
| MCP references (`codebase-memory-mcp`) | `.mcp.json` (project), `~/.claude.json` `mcpServers` (user) and `projects.<dir>.mcpServers` (local) | configured / not configured. Presence stays *unknown*: nothing is started and the command's existence is not verified |
| Local CLIs (gitleaks, betterleaks, agent-browser, strix, markitdown, graphify, codebase-memory-mcp) | Command names from each project's README | present = a file of that name is in a PATH folder (low confidence, identity unverified); absent = not in PATH |
| Antigravity desktop IDE | No documented detection method was verified | always **unknown**; look-alike folders and the `agy` CLI are never used as evidence |
| `cloudflare-security-audit-skill`, `agentic-awesome-skills` | none documented yet | `no-detector` (unknown) |

Not detected, by design: the VS Code extension's private Claude Code copy, running sessions, program versions (nothing is executed — running an unverified program found by name would be arbitrary code execution; `claude --version` is documented but a PATH file of that name cannot be trusted), Antigravity IDE.

## Safety model

* **Authorization per scan.** Three independent permissions, all off by default and never pre-ticked: PATH names; Claude Code user files; one project folder (typed by the user, used for that scan only, never stored). A scan with nothing authorized is refused. Nothing starts by itself.
* **One read-only file layer** (`discovery/fsx.rs`): no write/create/delete/rename/execute call exists in the discovery code (a test scans the source for them and checks that no HTTP client crate is a dependency). Links and Windows reparse points (junctions, app-execution aliases) are never followed, except that a link *as the final PATH file* is reported as present-but-unverified. UNC/network paths and device paths are never touched. Reads are size-capped (1 MiB; 8 MiB for `~/.claude.json`) and must be UTF-8. Folder names come from constants or validated identifiers; versions are validated before use in a path.
* **Bounded.** At most 64 PATH entries, named files only, no directory listing, no crawling, no project source tree. Of a project folder exactly `.claude/settings.json`, `.claude/settings.local.json` and `.mcp.json` are read.
* **Configuration is untrusted data.** Only typed fields are extracted. Arguments, environment, URLs, headers, tokens and unrelated plugin or server names are never read into an observation; evidence strings are redacted again before storage; locations collapse the home folder to `~`.
* **Stored locally only** in the application's own SQLite database (migration 4, additive). Nothing is synced, logged with content, or committed. A scan is audited only as a schema-valid `inspect` event.
* No agent or MCP server is started, no package manager is touched, no network, no LLM, no administrator rights, no Windows security setting.

## Self-audit

A new `discovery` check is *unknown* before any scan, *unknown* when the last scan is older than 24 h, *failed* if any stored observation is not already redacted or claims a test, otherwise *passed* with the explicit wording that observations are presence/configuration evidence, not verification. A passing scan cannot make the overall health "passed".

## Evidence (exact commit in the final report)

| Evidence | Result |
| --- | --- |
| Rust tests | 161 passed (45 new discovery tests, 4 for Issue #4, 5 for migration/self-audit) |
| Issue #4 (global `rowid` used as a per-project counter) | Fixed: retention ranks each project's own rows. Reproduction (A 5+1 rows around B 700) now keeps all 6; a project over 500 keeps exactly its newest 500, B untouched; mutation back to the old query fails 2 tests |
| Frontend tests, type check, build | 49 passed |
| Python contract tests | 53 passed with the built binary's self-check output (23 skip honestly without it) |
| Real Phase 1A and Phase 1B databases | Both migrate to schema 4 and keep working (audit chain verifies) |
| Windows GUI run on a real schema-v2 database | Passed: app migrated it, scan only on request, permissions explained, network and `..` project paths refused, project scan read exactly the named files and left the folder byte- and timestamp-identical, no dummy secret stored |
| Real Windows scan (`--discover-local`) | Done; results summarised in the final report; output contained no account name, path or token |
| Windows junction tests | Passed (a junction in place of `~/.claude`, a PATH folder or the project folder is not followed) |
| Windows file-permission (access-denied) test | **Not run** (the equivalent test runs on macOS CI) |
| macOS | CI compiles, runs the same tests (symlink, permission, read-only) and the headless synthetic probe. **No real macOS installation was inspected**; detector paths for macOS follow the documentation but are unverified on a real Mac |

## Known limitations

* A found file or a plugin record does not mean the tool works; there is no functional check, no version for programs, and identity for programs rests on a name.
* PATH-only search can report *absent* for tools installed elsewhere; the text says so.
* `~/.claude.json` is read in full (up to 8 MiB) to find MCP definitions; only matching server names are kept.
* Project-scope plugin installs cannot be tied to a project by the registry; they are reported as declaration-only with files "on disk somewhere".
* Antigravity IDE and the two skill entries have no detector.
* The project folder is typed, not picked with a native dialog (no extra permission or dependency); the OS file dialog can be added later.
* Local release runtime on Windows remains unverified (Smart App Control, see [WINDOWS-SMART-APP-CONTROL](WINDOWS-SMART-APP-CONTROL.md)).
