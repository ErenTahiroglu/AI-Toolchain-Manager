# CONFIG-02B — one-file transaction contract, state machine and limits

Issue #39. **Status: a test-only prototype on synthetic directories. It is not a feature, not an installer and not permission to write anything.** `REAL_CONFIG_WRITE_NO_GO` and `REAL_INSTALLATION_NO_GO` stand. Nothing here can be reached from the application: the code is compiled for tests only (`#[cfg(test)]`), no IPC command, Tauri capability, migration, dependency, GUI control or user-selected path was added, and no test reads or writes outside directories the test itself creates. Issues #14 (pre-production filesystem gate) and #25 (WebView2 runtime) stay open and are separate problems; nothing in this milestone resolves either.

A word on wording: *design condition* means something a future, separately authorised phase must satisfy; *implemented in tests* means a test in this repository exercises it on a synthetic root. The two are kept apart in every table below.

## 1. The one operation

| | |
|---|---|
| Host / surface | Claude Code (`claude-code-project-mcp`), surface `claude-code-terminal` |
| File | `.mcp.json` directly in the selected project root. The name is a compile-time constant; no function takes a file name or a path inside the root |
| Change | add exactly one member, `mcpServers.exa = {"type":"http","url":"https://mcp.exa.ai/mcp"}` (the CONFIG-02A reviewed declaration: syntax only, runtime **not** verified) |
| Catalog basis | entry `exa-mcp-server` pinned at `0b499f095c4ca145eee07b09137641c037d58449` in `https://github.com/exa-labs/exa-mcp-server`. Compatibility stays *unknown*; this does not make the entry accepted or supported, and the write path does not lift any catalog block |
| Never touched | `.claude/settings.json`, `CLAUDE.md`, `~/.claude.json`, `.agents/mcp_config.json`, `.codex/config.toml`, any other file (a test checks that every other byte of the test area is unchanged) |
| Does not decide | endpoint identity, availability, cost, privacy, credentials, or whether Claude Code accepts the stanza |

## 2. What an operation is bound to

A plan is a typed value whose SHA-256 digest covers every fact below; an approval is bound to that digest, to one use and to a lifetime of at most 600 logical seconds (the prototype has no clock; the caller passes the time).

| Bound fact | In this milestone | A real phase would add |
|---|---|---|
| Project | the alias string, validated by the existing `projects::valid_alias`. **An alias is metadata, not a path** — the project registry stores no path at all | the registry/identity decision of §7 |
| Actual root directory | the identity (volume + file id; device + inode) of the directory *handle* the plan was made from. The root is a test-created directory | a root selected by the user through a native chooser, stored as an identity, re-opened by handle |
| OS and surface | `windows-x64` or `macos-arm64`; `claude-code-terminal`. A plan for the other OS, or any other surface, is refused | — |
| Exact relative file | the constant `.mcp.json` | — |
| Expected prior state | `Absent`, or `Present` with file identity, size and SHA-256 of the bytes read through the handle | the same |
| Proposed full text | SHA-256 of the complete new file, built by the writer from the bytes just read (never from preview text) | the same, shown to the user |
| Catalog pin | entry id, pinned commit and repository URL must equal the reviewed ones | the same, plus re-validation against the live catalog |
| Approval | a test fixture (`test_grant`): one-shot nonce + plan digest + expiry. **There is no consent screen, nothing is persisted, no production code can construct one** | a real per-file consent (§6) |

**The React preview is never the source of a write.** Its before/after text is redacted (`«gizlendi»`) and normalised. The writer reads the original bytes from the authorised file through the root handle, parses them strictly, merges one member structurally and writes the result. A file that already contains the preview's mask text is refused (`redaction_marker_present`). Values in other servers (including real secrets) are preserved exactly in meaning and are never shown, logged or quoted in a refusal (refusals carry a code only).

## 3. Reuse of existing primitives (and why nothing was built in parallel)

The Phase 3C layer `managed_fs` (`managed_fs_windows.rs`, `managed_fs_unix.rs`) already provides: root opened by walking its chain with no link followed (`NtCreateFile` with a root handle and `FILE_OPEN_REPARSE_POINT` / `openat(O_NOFOLLOW)`); exclusive creation with identity read from the created handle; link-count and kind checks; an atomic no-clobber rename decided by the OS (`FILE_RENAME_INFORMATION_EX` without the replace flag / `renameatx_np(RENAME_EXCL)`) with **no fallback to an ordinary rename**; an identity-checked replace of one known file; verified deletion; owner-only folders. The prototype is a **child module of `managed_fs`** so that it can call those primitives directly instead of copying them. It needs exactly one thing the public `Dir` API refuses: the name `.mcp.json` (the shared validator rejects a leading dot). Rather than widening the shared validator, the child module addresses that one constant name itself. Nothing in `managed_fs.rs` changed except the `#[cfg(test)]` module declaration.

New code: `src-tauri/src/config_txn.rs` (pure: strict JSON, merge, plan, approval fixture, refusals), `src-tauri/src/managed_fs_mcp.rs` (the file-system half), their two test files, one shared golden data file and one TypeScript test. Cargo dependencies: none added (`sha2`, `uuid`, `tempfile` were already there).

## 4. State machine

```
Inspected --prepare--> Prepared --authorize--> Authorized --(target == expected)--> [BackedUp] --> Staged --final recheck--> Swapped --verify--> Verified
     \_ any refusal: nothing changed (or only what the table in §5 says) \_ rollback(Verified) -> RolledBack  |  failure after Swapped -> NeedsAttention
```

| Operation | What it may do | What it may not do |
|---|---|---|
| **read** (`inspect`) | list the root, stat the constant name, open it without following links, read at most 1 MiB, hash | write, create, delete, follow a link, accept a case-variant or hard-linked file |
| **prepare** | build the new text and the plan from what was read | read or write anything itself; accept text from the UI |
| **confirm** (`authorize`) | check the approval against the plan digest, the lifetime and the one-shot ledger | be retried: a spent approval is never reusable and a failed commit needs a new plan *and* a new approval |
| **atomic replace** (`commit`) | copy the original to a **separate private folder**; stage the new file exclusively in the root; re-check the target; swap by no-clobber rename (absent) or identity-checked replace (present); verify | overwrite anything that is not the exact inspected object; delete anything it cannot prove it created |
| **abort** | remove the staging file it created, only if it is still the same object (identity from the creation handle, one link) | delete any other file, however it is named |
| **rollback** | restore the backed-up bytes (or remove the created file) **only while the target is still exactly the file this commit wrote** | overwrite a later edit by anybody |

## 5. Failure table

`reached` is the last state completed. "Stray" = a staging file the prototype cannot prove is its own; `inspect` reports it and nothing deletes it.

| Step | Failure | On disk afterwards | Code | reached |
|---|---|---|---|---|
| plan | wrong/invalid project, OS, surface, entry, pin, repository; lifetime 0 or > 600 s; unsupported host | nothing | `invalid_project` `wrong_os` `wrong_surface` `wrong_entry` `catalog_pin_mismatch` `ttl_too_long` `unsupported_host` | – |
| plan | malformed JSON, comments, trailing comma, duplicate key (also via escapes), BOM, lone surrogate, too deep/large, root or `mcpServers` not an object, preview mask present | nothing | `malformed_json` `duplicate_key` `byte_order_mark` `lone_surrogate` `too_deep` `file_too_large` `root_not_object` `mcp_servers_not_object` `redaction_marker_present` | – |
| plan | `exa` already present in any letter case; same endpoint (scheme+host+path, ignoring case, default port, query, fragment, user info, trailing slash) under another name | nothing | `server_name_conflict` `endpoint_already_configured` | – |
| inspect | target is a link/junction, a directory, hard-linked, or a case-variant name exists | nothing | `target_is_link` `target_not_regular_file` `target_hard_linked` `case_alias_present` | – |
| commit, before approval | other project, other root, backup folder is the project or not private, approval missing/other plan/expired/reused | nothing | `wrong_project` `root_identity_changed` `backup_is_project_root` `backup_not_private` `approval_*` `plan_expired` | Prepared |
| commit, first check | target changed (content or identity), appeared, vanished | nothing; the approval is spent | `target_changed` `target_appeared` `target_vanished` | Authorized |
| backup | backup name exists, write/verify fails | a backup file may exist in the private folder (never overwritten, never deleted); the project is untouched | `backup_exists` `backup_mismatch` | Authorized |
| stage | create/write/verify fails | staging file removed if provably ours, else stray; target untouched | `stage_mismatch` `fs_*` | Authorized / BackedUp |
| final recheck, or a change inside the last window | file edited, replaced, created, or swapped for a link between the last check and the swap | staging file removed; target is whatever the other party made it; nothing of theirs is overwritten | `target_changed` `target_appeared` `target_is_link` / `target_not_regular_file` | Staged |
| swap | volume or system lacks the atomic no-clobber rename | staging file removed; nothing created, **no fallback** | `WRITE_ADAPTER_BLOCKED` | Staged |
| swap | **in-place edit inside the replace window**, macOS | new file in place; the displaced original, which holds the edit, is kept as a stray; nothing deleted | `target_changed` | NeedsAttention |
| swap | **in-place edit inside the replace window**, Windows | **not detected**: new file in place, the edit is lost (the backup holds the pre-edit original) | – (success) | Verified |
| verify | what is at the target is not the file that was written | left as it is; backup intact | `verify_failed_needs_attention` | NeedsAttention |
| any step | process or machine dies | possibly a stray staging file and/or a backup; no recovery record exists, so nothing is reconciled automatically | – | – |

## 6. How a later, minimal per-file consent would be displayed (design only — nothing is built)

A future approval screen would be rendered by the application itself, in Turkish like the rest of the application, and would show — all read from the **same root handle** that the write will use, not from pasted text:

1. the project's display name and *the result of the identity check* of the selected root (not just a path string);
2. the exact relative file `.mcp.json` and whether it exists, with its size and SHA-256 (the digest the approval is bound to);
3. the exact stanza and the redacted before/after of the real file, with the count of masked values;
4. the verbatim notice: "Yalnızca yapılandırma taslağı; uygulanmadı, bağlantı kurulmadı. Hizmetin ücretleri, gizliliği ve kimlik doğrulaması ayrıca değerlendirilmelidir." — followed by what is different at that point (this screen *would* write) and that the service's cost, privacy and terms are not evaluated by the application;
5. every catalog block still in force for the entry (compatibility *unknown* stays unknown) and Claude Code's documented behaviour that headless runs load project servers without asking;
6. where the backup will be kept and what "geri al" (rollback) does and does not do;
7. two buttons only — approve **this one operation** / cancel — with no "apply anyway", no "retry", no default focus on approve. A stale plan is rejected and a *new* preview is shown, never silently retried.

None of this exists: there is no Apply control and no persisted token. Reading a real user file (a later "CONFIG-02C"-style step) and writing one are separate privileges that must be separately authorised.

## 7. Implemented in tests versus design only

| Item | State |
|---|---|
| Strict JSON reader/writer, order and number text preserved, one-member merge, TypeScript/Rust golden parity | **implemented in tests** |
| Plan binding (project alias, OS, surface, entry, pin, root identity, expected state, new digest, lifetime) | **implemented in tests** |
| One-shot approval bound to the plan digest | **implemented in tests as a fixture only** |
| Root opened with no link followed; target opened no-follow, single link, no case alias | **implemented in tests**; Windows junction and macOS symlink cases exercised where the OS lets a test create them |
| Exclusive creation of an absent target (OS no-clobber rename) | **implemented in tests** |
| Identity-checked replace of a present target | **implemented in tests**; the window between the check and the rename remains (§8) |
| Backup of the original to a separate private folder before the swap; rollback only of the file that was written | **implemented in tests** |
| Cleanup limited to objects the call created | **implemented in tests** |
| Exclusive lock held on the target | **design only** (not provided by the primitives) |
| Persisted recovery record and crash reconciliation | **design only** |
| Preservation of the old file's mode / ACL / extended attributes | **design only** (the new file has default permissions) |
| Root chosen by the user, identity stored in the project registry | **design only** |
| Real consent screen, real approval token, any IPC command, any capability | **not built; not authorised** |
| Reading or writing any real project directory | **not done; `REAL_CONFIG_WRITE_NO_GO`** |

## 8. What each platform actually guarantees here (`GUARANTEES`, asserted by a test)

| | Windows (x64) | macOS (arm64) |
|---|---|---|
| Create an absent target: atomic, no clobber | yes (`FILE_RENAME_INFORMATION_EX` without replace) | yes (`renameatx_np` with `RENAME_EXCL`); a volume without it → `WRITE_ADAPTER_BLOCKED` |
| Replace: destination identity checked just before the rename | yes, through an open handle | yes (`RENAME_SWAP`, then the displaced object is verified) |
| Window between that check and the rename | **remains** | **remains**, but see next row |
| In-place edit inside that window | **not detected**; the replaced file is gone | **detected afterwards**: the displaced original is kept and compared; nothing deleted, `NeedsAttention` |
| Exclusive lock held on the target | no | no |
| Old file's mode/ACL carried over | no | no |
| Persistent recovery record | no | no |

An editor, a formatter, `git checkout` or the agent itself writing `.mcp.json` in that window is exactly the case an exclusive lock or a content-checked swap would address; on Windows the available primitives cannot close it, and this is a **blocker for any real use** until an independent review decides how to close or accept it. `docs/phase-3c/FILESYSTEM-GUARANTEES.md` documents the same class of window for the underlying layer.

Also unchanged from the layer below: ancestors of the root must be real directories (a project reached through a symlinked parent, for example `~/code -> /elsewhere`, is refused, conservatively); network and unknown file systems are refused; the macOS temporary folder lives below the `/var` symlink, so tests canonicalise first.

## 9. Non-goals and remaining trust boundaries

Not done and not authorised here: any real project read or write; user-selected paths; a working Apply button; a persisted approval; a recovery journal; other hosts' files (Antigravity, Codex TOML, settings, `CLAUDE.md`, `~/.claude.json`); any network access, process launch, package-manager call, MCP-client connection or Claude/Codex/Exa execution; CONFIG-02C; Evidence-Checkpoint-Engine integration (#34); GitHub issue publication (#35); the jq pilot.

Trust boundaries that remain: the declaration is the developer's own comparison with the sources and awaits independent review as a runtime claim; the approval is a fixture; a same-user process can race the last check (§8); SHA-256 binding of the expected content is not an authorisation by itself; the machine's own file-system semantics (network shares, unusual file systems) are outside what was exercised; test results here are developer reports on the machines named in the Issue, not independent review, and a green CI run is not acceptance.
