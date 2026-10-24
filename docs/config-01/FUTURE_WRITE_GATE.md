# CONFIG-01 — the gate a future real write must pass (design only; nothing here is implemented)

**REAL_CONFIG_WRITE_NO_GO.** CONFIG-01 contains no write adapter, not even one disabled behind a flag, and no file-system permission in the Tauri capability. This document says what a *future*, separately reviewed and separately approved phase would have to satisfy before the first real byte is written to a user's project, so that the preview of today cannot be mistaken for permission to write.

**Issue #14 is not solved by anything in CONFIG-01.** Synthetic preview tests exercise a pure function over strings; they say nothing about real-filesystem races, link following, atomic replacement, deletion or cleanup, which is exactly what #14 is about. **Issue #25** concerns the WebView2 runtime of the GUI (outbound HTTPS, profile state) and is unrelated to a headless pure function, but it is still open and still gates any real GUI session that might one day host such a write. #29 (PRODUCT-07, evidence selection) is also open and orthogonal.

## 0. Preconditions that are not engineering
1. #14 resolved by independent review for the real write/cleanup primitives that the adapter would use; a fresh, explicit instruction to start a write phase (this one is not it).
2. At least one **reviewed declaration** exists in `src/configDeclarations.ts` for the target (host, entry) — today the registry is empty, so there is nothing to write.
3. The catalog gates for that entry are satisfied on their own terms (not quarantined, not execution-blocked, compatibility declared for the exact OS × surface) — a write never lifts a block.
4. A written threat model for the adapter, reviewed independently, and a decision on where the consent is rendered (a surface whose integrity does not depend on the unfixed items above).

## 1. One narrow capability per adapter — no generic "write a file" API
Three different things are configured in three different ways and must never share a write primitive:

| Future adapter | Target | What it may do | What it must never do |
|---|---|---|---|
| **MCP JSON adapter** | exactly one file per call: `.mcp.json` (Claude Code) or `.agents/mcp_config.json` (Antigravity IDE) under the selected project root | add **one** reviewed stanza under `mcpServers` | write any other key, any other file, a different host's file, or anything outside the project root |
| **Native plugin / skill manager adapter** | the agent's own plugin/skill commands (for example a marketplace add + plugin install) | invoke the host's documented manager with reviewed, version-pinned arguments, never a shell string | touch configuration files directly; run publisher scripts; accept `latest` |
| **Instruction-document adapter** | `CLAUDE.md` / `AGENTS.md` (instructions, not MCP configuration) | append or replace one fenced, owned block | treat instructions as configuration, or write them as a side effect of another adapter |

Each adapter exposes a typed, single-purpose command with its own consent record. There is no parameter that carries a path, a file name, a shell command, a URL to fetch or a free-form content blob from the UI.

## 2. Project-root identity
* The user selects the project root once, in a native directory chooser that returns an OS handle; the application stores the **identity** of that directory (volume + file id on Windows, device + inode on macOS), not just its text, in the project registry. An alias is not a path.
* Every later operation re-opens the root **by handle**, verifies its identity still matches, and performs all further operations **relative to that handle** (`openat`-style / `NtCreateFile` with a root directory handle), never by re-resolving a path string.
* The target file name is a compile-time constant of the adapter; the only variable path component is the project root, which is never concatenated into a command.

## 3. Links, reparse points, hard links, escapes
* Every component between the root and the target is opened **without following** links (`O_NOFOLLOW` / `FILE_FLAG_OPEN_REPARSE_POINT`), and refused if it is a symlink, junction, mount point or any other reparse point. A final-path-of-handle comparison confirms the opened object is inside the root.
* The target, if it exists, must be a regular file with exactly one hard link (`nNumberOfLinks == 1` / `st_nlink == 1`); otherwise the write is refused — a hard link could point outside the project or at a shared file.
* `.agents` must already be a real directory or be created by the adapter, never a link; no recursion, no deletion of anything not created by the same transaction (#14).
* Case-insensitive and short-name aliases (`MCP~1.JSO`, trailing dots/spaces, alternate data streams on Windows) are rejected by comparing the final opened name to the constant.

## 4. Expected identity and SHA-256 at preview and at commit
The preview (today) already records a SHA-256 of the **pasted text**. A future write replaces this with the digest and identity of the **real file as the adapter reads it**:
1. At preview time the adapter reads the file through the handle above and records: exists/absent, file id, size, modification time, SHA-256 of the exact bytes.
2. The user's consent is bound to that tuple, the reviewed declaration's id and digest, the project-root identity, the host, the OS and the entry.
3. At commit time the adapter **re-reads under an exclusive lock and re-verifies the whole tuple**. Any difference — a concurrent edit, an editor's autosave, a formatter, a `git checkout` — aborts with no write and shows a **new** preview. There is no "apply anyway".

## 5. TOCTOU and concurrent edits
Verification and replacement happen on the same open handle where the OS allows it. For "no file existed at preview" the commit uses an exclusive-create primitive, so a file that appeared in between is never clobbered. The adapter assumes the editor, the agent itself and a version-control tool may write at the same moment; it holds a lock for the shortest possible time and never waits for the user while holding it.

## 6. Atomic, no-clobber replacement per OS
* **Windows:** write the new content to a temporary file created in the **same directory** (exclusive create, final-path verified, no sharing), flush, then replace with a rename that **fails if the destination changed** (`ReplaceFileW` with the identity checked first, or `SetFileInformationByHandle(FileRenameInfoEx)` with the replace-if-same-file semantics once verified); preserve ACLs/attributes; handle the delete-pending states that already make lifecycle tests intermittent on this machine.
* **macOS:** temporary file in the same directory, `fsync`, `renamex_np` / `renameatx_np` with `RENAME_EXCL` for creation, or `rename` after verifying the destination's inode/size/digest under the lock for replacement; preserve mode and extended attributes as appropriate.
* Never write in place; never truncate-then-write.

## 7. Rollback and crash recovery
* Before replacement the adapter writes a **recovery record** (in the application's own data folder, not in the project): the tuple from §4, the backup copy's digest and location, the intended new digest and a state (`prepared` → `replaced` → `verified`). The backup is a copy of the previous bytes, created before any change.
* After replacement it re-reads the file and checks the digest; a mismatch rolls back from the backup.
* On the next start, any record not in `verified`/`rolled_back` is reconciled: if the file equals the intended digest it is verified, if it equals the backup it is rolled back, otherwise the state is reported as `needs_attention` and **nothing is changed automatically**.
* Rollback is a restore of the backup **only if the current file still equals the digest the adapter wrote** — never over a later edit by the user.
* No recursive deletion anywhere; the only thing ever removed is the adapter's own temporary and backup files, identified by recorded identity (the #14 rule).

## 8. Permission checks scoped to the root
The Tauri capability for the write command would grant only that one typed command; the command itself checks that the process can open the root and the target for exactly the needed access (read + replace), and **fails closed** on any elevation request. It never requests administrator rights, never changes ACLs or ownership, and never touches the user-profile files (`~/.claude.json`, `~/.claude/settings*.json`, `~/.gemini/config/mcp_config.json`).

## 9. Stanza ownership and conflicts
* The adapter owns exactly one key it adds (`mcpServers.<reviewed name>`) and records that ownership (name + digest of the written stanza) in its own store. Update/remove later touch only a stanza whose current digest equals the recorded one; a user-edited stanza is left alone and reported.
* A same-named server, or an equivalent command/URL under another name, is a conflict that blocks the write (as the preview already does).
* Existing MCP servers, skills, plugins, hooks and permissions configured elsewhere (settings files, a plugin manager, another adapter) are detected as **context** and shown; they are never merged away or overridden. Claude Code's documented behaviour that headless runs load project servers without asking is shown at consent time.

## 10. Secrets
Preview and consent text mask secret-bearing values (as today). The adapter never writes a literal credential: only reviewed `${NAME}` references where the host documents expansion; for a host that does not (Antigravity today) a declaration needing a credential cannot be written at all. Nothing the adapter handles is logged; error messages never quote file contents; the audit record stores digests, never text.

## 11. Consent
Explicit, per project, per file, per action, per declaration version. The consent screen shows: the project and its resolved root (by display name and identity check result), the host, the OS, the exact relative path, the full redacted before/after, the digest tuple, the catalog state of the entry and every block that is still in force, the headless-approval caveat, and the rollback story. It offers "Uygula" and "Vazgeç" only after the preview is fresh (§4); changing anything invalidates it. Consent is single-use: applying consumes it, as the installer's approval already does. A write is an audited event (`config_write`) that fails closed if the audit append fails (the Issue #22 rule).

## 12. What the first real write phase would still have to prove
Tests on real temporary directories (junctions, symlinks, hard links, case aliases, concurrent writers, crash injection between every step, read-only files, locked files, long paths, non-ASCII roots) on both Windows and macOS CI; a failure-injection suite for the recovery record; and an independent review before any flag could exist. None of that is claimed here.

## 13. Added by AGENT-01: a TOML target (Codex CLI) is a separate adapter
Codex CLI's project file is `.codex/config.toml` (TOML, loaded only for a trusted project; see `docs/agent-01/CODEX_SOURCE_AND_SCOPE.md`). If a write phase ever covers it, it is a **fourth, separate adapter** with its own restricted-TOML reader, its own refuse-on-unsupported-construct rule (dotted keys, inline tables, arrays of tables, multi-line strings, comments, quoted names), golden tests for each, TOML-aware redaction that never echoes values, and its own independent review - never a branch of the JSON adapter and never a JSON parser pointed at TOML. The same gates as above apply (handle-based root identity, no-follow opens, digest tuple at preview and commit, atomic no-clobber replace, recovery record, stanza ownership, single-use consent). Trust for a project is decided inside Codex; this application would have to show it cannot know it. The user-level `~/.codex/config.toml` (model providers, credentials) stays out of scope for every adapter. AGENT-01 implements none of this.
