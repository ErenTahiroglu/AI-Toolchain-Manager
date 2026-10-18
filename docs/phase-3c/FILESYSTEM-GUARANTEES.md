# Production-oriented filesystem layer — what is guaranteed, on which platform, with what evidence

Scope: `src-tauri/src/managed_fs.rs` (+ `managed_fs_windows.rs`, `managed_fs_unix.rs`), `managed_root.rs`, and the test-mode `adapter.rs` built on them. **Nothing in the application calls any of it**: no command, no screen, no start-up code. The production recipe list and the authenticated-reference set are empty, `Adapter::production` refuses every request, and the only acting constructor (`Adapter::test_mode`) exists in test builds only.

Evidence labels: **[W-local]** executed on the developer's Windows 11 PC; **[CI-win]** / **[CI-mac]** executed by the exact-SHA CI jobs (`windows-latest`, `macos-14` Apple Silicon *runner* — not a Mac desktop session); **[type-check]** compiled for `aarch64-apple-darwin` on Windows without running; **[simulated]** the decision logic is exercised with a test hook because the real condition cannot be created in an unprivileged test; **[design]** not implemented.

## 1. Guarantees by platform

| Property | Windows (`NtCreateFile` relative to a directory handle) | macOS (`openat` family relative to a directory descriptor) |
| --- | --- | --- |
| Anchor reached without following links | each component of the chain opened relative to the previous handle with `FILE_OPEN_REPARSE_POINT`; any reparse point (junction, symlink, mount point) → refused. **[W-local] [CI-win]** | each component opened with `O_NOFOLLOW \| O_DIRECTORY`; any symlink → refused. **[type-check]** → **[CI-mac]** |
| Network / unknown volume | `GetDriveTypeW` must be fixed/removable/RAM; UNC refused. **[W-local]** (local path only; a remote volume was not available to test) | `fstatfs` file-system type must be a known local type (reuses the discovery layer's list). **[CI-mac]** (local APFS only) |
| Links below the anchor never followed | every `open`/`stat` uses `FILE_OPEN_REPARSE_POINT` and checks `FILE_ATTRIBUTE_REPARSE_POINT`. **[W-local]** junction tested; file symlink tested only where the privilege exists | `O_NOFOLLOW` / `AT_SYMLINK_NOFOLLOW`. **[CI-mac]** |
| Mount transitions | volume serial of each opened directory must equal its parent's; a Windows mount point is a reparse point and is refused. **[simulated]** (volume-serial mismatch via hook) + **[W-local]** junction | `st_dev` must equal the parent's. **[simulated]** + **[CI-mac]** |
| Exclusive creation | `FILE_CREATE`; an existing file/folder → `Exists`, never adopted. **[W-local]** | `O_CREAT \| O_EXCL \| O_NOFOLLOW`, `mkdirat`. **[CI-mac]** |
| Identity from the open handle | `GetFileInformationByHandle` (volume serial + 64-bit file index, link count). **[W-local]** | `fstat` (`st_dev`, `st_ino`, `st_nlink`). **[CI-mac]** |
| Owner-only root | protected DACL `D:P(A;OICI;FA;;;OW)` set at creation and **read back** (`GetSecurityInfo` → SDDL) and compared. **[W-local]** | mode `0700` + owner = effective user, verified with `fstat`. **[CI-mac]** |
| Verified deletion | opens the file with `DELETE`, checks kind / one link / identity / size / SHA-256 **through that handle**, then marks **that handle** for deletion (`FileDispositionInformationEx`, POSIX semantics): the object deleted is exactly the one verified; a newcomer under the name is never touched. **[W-local]** (swap test) | `fstat` + hash on the opened descriptor, then `fstatat` on the NAME must show the same `(dev, ino)` immediately before `unlinkat`. **A window of microseconds remains**: no inode-based unlink exists. A swap *before* the final check is refused. **[CI-mac]** |
| No-clobber move | `FileRenameInformationEx` without the replace flag: an existing destination of any kind is refused by the OS atomically (`STATUS_OBJECT_NAME_COLLISION`). **[W-local]** (destination created in the gap tested) | `renameatx_np(RENAME_EXCL)`: refused atomically with `EEXIST`; unsupported file system -> `Unsupported`, **never** a fallback to `renameat`. **[CI-mac]** |
| Replacement (only of ONE known transaction-owned file) | destination opened without following links and its identity/link count checked immediately before the replacing rename; **a window remains** (a destination cannot be pinned against deletion and replaced at once). **[W-local]** (window recorded by a test) | `RENAME_SWAP` exchange, then verification of the displaced and moved-in objects, swapped back if either is not as expected. **[CI-mac]** |
| Files in use | a program that holds a file without delete sharing → `Locked`, operation fails honestly, rollback restores. **[W-local]** | unlink of an open file succeeds and the holder keeps its content; reported as "name removed", never as "program stopped". **[CI-mac]** |
| Held folder cannot be swapped | folders are held open without `FILE_SHARE_DELETE`: renaming/deleting the held folder fails with a sharing violation during a transaction. **[W-local]** | a swap succeeds, but the held descriptor keeps designating the original folder and the link is never followed; the rollback then refuses the replaced name. **[CI-mac]** |
| Durability | `FlushFileBuffers` on files; directory flush best effort (reported). | `F_FULLFSYNC` (fallback `fsync`) on files and folders. |
| No recursion | no recursive delete exists; folders are removed only when empty (asserted by a source-scan test). | same |
| Bounded | reads, hashes, listings, names: explicit limits (`read_bounded`, 512 MiB hash cap, 4096 entries, names ≤ 100 ASCII chars). | same |

## 2. What is NOT guaranteed (disclosed)

1. **macOS TOCTOU window**: between the final `fstatat` and `unlinkat` a same-user process can swap the name; the worst outcome is deleting a *file with the expected name inside the manager-owned, owner-only folder*. It cannot be removed without a new platform capability. The Windows path has no such window for the deleted object, but Windows can still lose a race on the *name* if a process replaces the file *after* deletion — irrelevant to what was deleted.
2. **Same-user malware**: it can rewrite the manager's database, the journal (its digest is unkeyed evidence against accidents, not authentication — a forged journal that claims a foreign file with the file's *real* identity and digest would be believed) and the manager's own binary. The design protects against accidents, other programs, other accounts, crashes and path redirection, not against a hostile process with the user's own rights.
3. **Directory flush on Windows** is best effort; NTFS journals metadata, other file systems may not.
4. **macOS device numbers** are not stable across mounts, so across restarts only the inode of the managed root is compared (the device must equal the anchor's within a session).
5. **No real mount test**: a real mount transition needs a second volume (macOS disk image / Windows volume mount point). It is covered by a simulated volume mismatch and, on Windows, by the junction/mount-point refusal. Manual test: see `PORTABILITY-AND-MANUAL-TESTS` (3B) and `PHASE-3C.md`.
6. **Cloud-sync placeholders, case-insensitive Unicode normalisation**: names are a strict ASCII subset; placeholders were not tested.
7. **Windows share modes of other programs**: an anti-virus scanner holding a file briefly can cause `Locked`; the adapter reports it and retries on the next recovery, it does not retry in a loop.

## 3. Crash consistency (Issue #15)

Every file a transaction creates goes through three durable stages: **intent** (journalled before the file exists) → **exclusive create + identity read from the handle** (journalled before any data is written) → **finalized content identity** (size + SHA-256, journalled after the data is flushed). Between the exclusive create and the durable identity record there is an **irreducible window**: a crash there leaves a file whose ownership cannot be proven. The engine treats it as an **ambiguous orphan**: it is never deleted, never adopted, reported on every recovery (`NeedsAttention`), the tool stays locked, and normal work resumes only after the owner removes it. A plain *failure* (not a crash) in that window still records the identity the engine holds in memory, so the in-process rollback can prove ownership. The same discipline applies in the synthetic engine (`lifecycle.rs`) and in the adapter.

## 4. Manager-owned root

Derived only from the application's own data directory (`AppDataAnchor`, constructed from the resolved data directory; no command or IPC argument can name a root — asserted by a source-scan test). Its identity (volume + file index) is recorded in the manager's own SQLite database (`managed_roots`, migration 9) *before* the folder exists (`creating`) and activated only after the identity was read from the created folder (`active`); an unfinished `creating` row blocks adoption. A folder with the right names but a different identity, a replaced/linked root, extra top-level entries, or non-owner-only permissions is refused.

## 5. Differences between platforms that tests assert explicitly

* swap of a held folder: prevented on Windows, survived (not followed) on macOS;
* delete of a swapped name: original deleted by handle, newcomer untouched (Windows) vs. refusal `Changed` (macOS);
* open file: `Locked` (Windows) vs. name removed, holder unaffected (macOS);
* read-only file: delete refused (Windows) vs. permitted by folder permission (macOS).
