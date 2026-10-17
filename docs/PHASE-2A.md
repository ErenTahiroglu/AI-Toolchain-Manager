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
* **One read-only file layer** (`discovery/fsx.rs`): no write/create/delete/rename/execute call exists in the discovery code (a test scans the source for them and checks that no HTTP client crate is a dependency). The WHOLE chain from the volume root to the target is validated (not only the last folder; see *Red Team closure*): a link, junction or other reparse point anywhere in it is never followed, except that a link *as the final PATH file* is reported as present-but-unverified. UNC/network paths, remote or redirected drives and device paths are never touched. Reads are size-capped (1 MiB; 8 MiB for `~/.claude.json`) and must be UTF-8. Folder names come from constants or validated identifiers; versions are validated before use in a path.
* **Bounded.** At most 64 PATH entries, named files only, no directory listing, no crawling, no project source tree. Of a project folder exactly `.claude/settings.json`, `.claude/settings.local.json` and `.mcp.json` are read.
* **Configuration is untrusted data.** Only typed fields are extracted. Arguments, environment, URLs, headers, tokens and unrelated plugin or server names are never read into an observation; evidence strings are redacted again before storage; every stored field passes the privacy gate (closed grammars, see *Red Team closure*): no absolute path, no account or folder name chosen by someone else, no MCP server name, no undocumented version text.
* **Stored locally only** in the application's own SQLite database (migration 4, additive; migration 5 adds the privacy gate's `record_version`). Nothing is synced, logged with content, or committed. A scan is audited only as a schema-valid `inspect` event.
* No agent or MCP server is started, no package manager is touched, no network, no LLM, no administrator rights, no Windows security setting.

## Red Team closure (Issue #5, baseline `f424aa4`)

### RT-2A-01 — history is kept, but only a current scan describes today
* Every scan keeps its own rows (newest 20 scans). What is *shown as true now* is derived at read time. An observation is **current** only if it comes from the **latest** scan, that scan is **not older than 24 h**, and it is for **this computer's platform**. Otherwise it is an *earlier result*, with the reason: the latest scan did not look at that area (permission not given, or another/no project), produced no such observation, is stale, or belongs to another OS.
* "Did the latest scan look at it?" follows from the method and scope: PATH methods need the PATH permission; global settings/plugin/MCP data needs the user-files permission; project/local data needs the project permission **and** that very project alias in the latest scan (local MCP also needs the user-files permission); `no-detector`/`not-documented`/`unsupported-platform` rows are emitted by every scan.
* Each observation carries the scan id, the time and the three permissions of the scan that produced it. The UI shows *Güncel gözlemler* and *Önceki taramalardan — güncel değil* in separate sections.
* "Your request vs observation" is one row per accepted preference (entry, project, agent surface, platform) with a status: `present_current`, `absent_current`, `unknown_current`, `previously_present|absent|unknown`, `platform_mismatch`, `not_scanned`. Matching respects project (another project's rows never count), surface (a computer-wide `local-cli` program lookup applies to any surface) and platform. A current answer always outranks an old one; an old answer is labelled and "is never a basis for any action".

### RT-2A-02 — privacy gate
* `location` follows a closed grammar: a fixed documented place below `~` (e.g. `~/.local/bin/claude.exe`) or `PATH girdisi #N · <category> · <program file>` where the category is one of *kullanıcı klasörü altında*, *sistem veya paket yöneticisi klasörü*, *başka bir klasör*. No absolute path and no folder name ever appears.
* A version is shown only in a documented form (release number, optional alpha/beta/rc/pre/dev/preview/next tag, or a 7–40 hex commit id) and only with the source label *Claude Code kurulum kaydı*. Any other text is dropped; a path-safe but undocumented version text is still used internally to find the plugin cache folder, never shown or stored.
* MCP evidence states how many definitions matched; server names are not stored.
* The gate runs in `Observation::finalize` (when built), again before every database write, again on every read for the UI/headless output, and in the self-audit.
* **Migration 5** (additive, transactional, backed up first by the existing pre-migration snapshot): adds `record_version` (1 = written before the gate, 2 = under it) and scrubs every legacy row in place — unsafe locations become *eski kayıt: konum gizlilik nedeniyle kaldırıldı*, `(ad: …)` is removed, absolute paths in text are replaced, undocumented versions are dropped. Presence/configuration facts, scans, preferences and the audit chain are untouched; legacy rows are marked in the UI. **The pre-migration backup file intentionally keeps the old, unscrubbed data (it is a rollback copy in the application's own data folder, never in Git); delete it once the migration is confirmed.**

### RT-2A-03 — filesystem confinement
* **macOS:** the target is reached with `openat(O_NOFOLLOW | O_DIRECTORY)` component by component from `/`; a symbolic link in any ancestor fails the walk, and because every step is relative to the handle already opened there is no check-then-use gap. Files are opened with `O_NOFOLLOW | O_NONBLOCK` relative to the opened parent. Network file systems (smbfs, nfs, afpfs, webdav, …) are refused (`fstatfs`).
* **Windows:** every existing ancestor must be a plain directory (no junction, symlink, mount point or other reparse point); remote drives and UNC/device paths are refused; then the object is **opened and `GetFinalPathNameByHandle` on that very handle must name exactly the intended path** (8.3 short names are expanded with `GetLongPathName`, which never follows links). A folder swapped for a junction after the check therefore produces a different final path and the handle is discarded before a byte is read. `subst` drives and mapped drives fail this comparison and are refused.
* The project validator uses the same walk, so `C:\proj\link\sub\project` (a link in a **parent**) is refused with `PROJECT_DIR_LINK`.
* **Residual limitations (documented, fail closed where possible):** on Windows the *presence-only* checks (PATH file names, plugin cache folders) stat their last component by path, so a concurrent writer with write access to the user's own folders could swap that last component between check and `stat`; this can only change what a metadata query sees, never what is read. The Windows approach detects a swap at open time rather than preventing it (the std library has no handle-relative open). An unreadable ancestor is reported as unreadable (`PROJECT_DIR_UNREADABLE`) rather than skipped.

## Self-audit

A new `discovery` check is *unknown* before any scan, *unknown* when the last scan is older than 24 h, *failed* if any stored observation violates the privacy gate (location, version, version source, project alias, evidence, limitations) or claims a test, otherwise *passed* with the explicit wording that observations are presence/configuration evidence, not verification. A passing scan cannot make the overall health "passed".

## Evidence (exact commit in the final report)

| Evidence | Result |
| --- | --- |
| Rust tests | 197 passed on Windows (+1 ignored fixture generator): 161 baseline + closure tests for RT-2A-01 (10), RT-2A-02 (10 incl. v4→v5 migration and rollback) plus 4 privacy-grammar unit tests, RT-2A-03 (10 on Windows incl. junction parent, `subst` drive and swap-during-validation cases) plus 2 filesystem unit tests |
| Issue #4 (global `rowid` used as a per-project counter) | Fixed: retention ranks each project's own rows. Reproduction (A 5+1 rows around B 700) now keeps all 6; a project over 500 keeps exactly its newest 500, B untouched; mutation back to the old query fails 2 tests |
| Frontend tests, type check, build | 52 passed |
| Python contract tests | 57 passed with the built binary's self-check output (27 skip honestly without it) |
| Real Phase 1A and Phase 1B databases | Both migrate to schema 4 and keep working (audit chain verifies) |
| Windows GUI run on a real schema-v2 database | Passed: app migrated it, scan only on request, permissions explained, network and `..` project paths refused, project scan read exactly the named files and left the folder byte- and timestamp-identical, no dummy secret stored |
| Real Windows scan (`--discover-local`) | Done; results summarised in the final report; output contained no account name, path or token |
| Windows junction tests | Passed (a junction in place of `~/.claude`, a PATH folder or the project folder is not followed) |
| Windows file-permission (access-denied) test | **Not run** (the equivalent test runs on macOS CI) |
| macOS | CI compiles, runs the same tests (symlink, permission, read-only) and the headless synthetic probe. **No real macOS installation was inspected**; detector paths for macOS follow the documentation but are unverified on a real Mac |

## Known limitations

* A found file or a plugin record does not mean the tool works; there is no functional check, no version for programs, and identity for programs rests on a name.
* PATH-only search can report *absent* for tools installed elsewhere; the text says so.
* `~/.claude.json` is read in full (up to 8 MiB) to find MCP definitions; only the NUMBER of matching definitions is kept (server names are never stored).
* Project-scope plugin installs cannot be tied to a project by the registry; they are reported as declaration-only with files "on disk somewhere".
* Antigravity IDE and the two skill entries have no detector.
* The project folder is typed, not picked with a native dialog (no extra permission or dependency); the OS file dialog can be added later.
* Local release runtime on Windows remains unverified (Smart App Control, see [WINDOWS-SMART-APP-CONTROL](WINDOWS-SMART-APP-CONTROL.md)).
