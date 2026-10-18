# Process safety and reversible installation — design for the first pilot adapter

Status: **design proposal for Phase 3C.** Nothing here is implemented; the synthetic engine is not connected to any command or screen and stays that way. The adapter described here is for **one** pilot (recommended: jq, a raw single executable per platform), one target, user scope.

## 1. Fixed boundaries of the first pilot adapter

| Rule | Reason |
| --- | --- |
| Writes only under the manager-owned root derived from the OS app-data directory (`FILESYSTEM-OWNERSHIP.md` §3); no path from the UI, IPC, a recipe or a plan is ever writable | no arbitrary write primitive |
| **No PATH change**, no shell-profile edit, no registry write, no shim in a folder the user did not choose | PATH edits are invisible, global and hard to undo |
| **No administrator rights**, no elevation prompt, no service, no scheduled task, no startup entry | blast radius |
| **No agent configuration writes** (Claude Code, Antigravity, MCP settings) | out of scope until a separate phase |
| **No shell, no package manager, no lifecycle script** (`postinstall`, `.msi`, installers) | the artifact is a single file placed by the manager |
| **Never runs the installed program** — not for a version probe, a self-test or "verification" | installation is not execution; running third-party code is a separate authorization |
| Network only for the download step the user approved (exact URL from the recipe, HTTPS, digest check before writing) | no telemetry, no side requests |
| The tool's own user data (`data/<tool>/`) is never touched by update/rollback/uninstall | protect user data |

How a person then runs the tool (a user-chosen shim folder, an explicit "copy command" button, a "run" feature) is an **open design question that must not default to PATH changes**; the pilot may end at "installed, here is its managed location".

## 2. Transaction shape (reuses the proven synthetic sequence)

lock → snapshot (pointer) → stage (exclusive create, manifest entry first) → verify digest (staged bytes vs authenticated reference) → create version folder → **second verification right before activation** → atomic pointer switch → receipt → journal commit → cleanup of staging/snapshot by manifest. Every step is a write-ahead journal entry; recovery at every start finishes or undoes it (`recover()`), and anything it cannot prove is its own is kept and reported.

## 3. In-use files and processes

The manager does not start the tool, so it cannot know whether someone else did. It must never "probe" by launching or enumerating third-party processes' memory.

* **Windows.** An executable that is running is mapped and cannot be deleted or overwritten; a rename of the **version folder** containing it also fails. Consequences: an *update* writes a **new** version folder and switches the pointer (no overwrite — works while the old version runs); an *uninstall* of a running version fails with a sharing violation on `DeleteFile`/`RemoveDirectory` → the engine reports `partial`/`failed` with a precise reason, keeps the journal and the lock, and **retries on the next start**; it never terminates a process. Anti-virus/indexer handles cause transient delays → bounded retry of the same single operation (already used by the synthetic engine).
* **macOS.** `unlink` of a running executable succeeds (the inode lives until the process exits); the tool keeps running from a file that no longer has a name. The manager reports the removal as done and states that a running instance of the old version may still exist; it never kills it. A pointer switch never affects a running process.
* **Detection without launching:** none is proposed for the first pilot. The user-facing text says "close the tool before updating or removing it"; failures are reported honestly. A later phase may add process-image checks only with its own review.
* **Smart App Control / Defender / Gatekeeper** may block a first launch of an unsigned file. The manager must not attempt to alter them, strip quarantine attributes, or add exclusions, and must report a block without claiming the installation is broken.

## 4. Reversibility

* **Rollback target:** the previous version folder is kept (`kept_for`) until the user removes it; rollback = switch the pointer back after **re-verifying that version's artifact against the receipt that vouches for it**.
* **Failed install:** nothing activated; staging removed by manifest; the report lists what was and was not undone (counts and kinds, never names).
* **Failed uninstall / interrupted uninstall:** the pointer and version folders are *moved* into the snapshot first (reversible), the receipt is written, then the snapshot is cleaned by manifest. A crash in between is rolled forward or back by recovery; a locked file leaves a `RollbackFailed` journal and the lock, and new work on that tool is refused until resolved.
* **Partial failure reporting:** every transaction ends with `restored`, `partial` or `failed` plus `undone` / `not_undone` / `not_covered` lists. The adapter must not collapse these into success.
* **Data safety:** nothing under `data/<tool>/` or outside the managed root is created, changed or removed; the canary test pattern from the synthetic suite is mandatory.

## 5. Receipts

A receipt records tool, exact versions, platform, artifact digest, approval id, recipe digest, timestamps and the kept versions — no paths, no account names, no secrets. It is written before the journal commit and removed if the transaction does not commit. Uninstall preflight accepts installed content only when a valid committed receipt vouches for its digest.

## 6. Acceptance tests for the adapter (Phase 3C, to be written)

All with **synthetic** artifacts and fixtures served from a local, test-only source (no real download): the synthetic suite's cases against the adapter; Windows open-handle/lock simulation (a test holds a handle without share-delete on the installed file); macOS unlink-while-open; kill-the-process-at-every-journal-point (fault injection as in `Point`); full-disk and permission-denied simulations; case-insensitive filesystem; path-length limits; a user data canary; an assertion that no process is spawned and no environment/PATH change occurs (the existing source-scan tests, extended to the adapter module).
