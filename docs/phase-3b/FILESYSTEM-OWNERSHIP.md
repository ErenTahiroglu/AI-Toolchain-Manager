# Filesystem ownership and safe deletion — production design (Issue #14)

Status: **design + synthetic hardening** (Phase 3C added the isolated handle-relative layer described in `../phase-3c/FILESYSTEM-GUARANTEES.md`). Section 2 describes what was *implemented* in Phase 3B inside the synthetic engine (`src-tauri/src/lifecycle.rs`). Sections 3–8 are a *design proposal* for the isolated production adapter of Phase 3C; none of it is wired to the application, and the synthetic engine is still not a production writer. Platform API behaviour in this document is described from general platform documentation and is an **unverified assumption until Phase 3C proves it with tests on real Windows and macOS** (it is marked *assumption* where it matters).

## 1. The finding and the threat model

Issue #14 (RT-3A-R1): the Phase 3A cleanup removed a file because its **name** was an expected one (`versions/<v>/artifact.bin`, `pointer.json`, staging `artifact.bin` / `artifact.bin.part`) and it was a regular file. A different file placed under that name (by another program, an earlier crash, a person) would have been deleted. A digest check on only some paths, or a SHA-256 binding without authenticated provenance, is not ownership.

| | Defended | Not defended (disclosed) |
| --- | --- | --- |
| Accidental collision with another tool's or the user's file under an expected name | yes | |
| A link, junction, hard link or mount point put where the manager expects a plain file or folder | yes (refuse, never follow) | |
| Interrupted / crashed transactions leaving partial files | yes (journal + identity) | |
| A different local account | yes, by the per-user data directory permissions | |
| Malware or a person running **as the same user**, racing the manager between check and use | narrowed, not closed (§6) | it can also rewrite the manager's own database and binary |
| Administrator / SYSTEM, kernel-level actors | | out of scope |
| Offline modification of the disk | | out of scope |

## 2. Implemented in Phase 3B (synthetic engine only)

1. **A transaction-owned manifest.** The journal gains `owned`: one record per file the transaction creates (`role`, every sandbox-relative place the file may legitimately be at, `sha256`, `size`, `identity`). It is written to the journal **before** the file is needed:
   * the staged artifact is created with `create_new` (exclusive), its identity is read **from the open handle**, the record is saved, and only then is data written — so even a crash in the middle of the write leaves a partial file that recovery can prove is its own;
   * the saved pointer copy (`snapshots/<txn>/pointer.json`) was, in Phase 3B, created and written BEFORE its identity reached the journal (Red Team finding RT-3B-01, Issue #15): that overstated what was implemented. Phase 3C corrected it: intent, then exclusive create with identity, then data, each durable in turn; an irreducible window between the create and the identity record remains and is handled by treating a file found there as an ambiguous orphan (see `../phase-3c/FILESYSTEM-GUARANTEES.md` §3);
   * an uninstall records the active pointer and every installed version artifact (identity + digest) **before the first move**.
2. **Identity** = (volume serial, file index) on Windows via `GetFileInformationByHandle`, (`st_dev`, `st_ino`) on Unix, together with the **link count** from the same call. A file is deleted only if it is a plain file (never a link or junction), has exactly one link, the same identity as recorded, and — once content is final — the same size and SHA-256 read **through the handle that was identity-checked**.
3. **All or nothing.** `remove_verified_dir` / `remove_snapshot` first verify *every* entry; one unrecorded entry, replaced file, alias, folder or link keeps everything in place. Removal is `remove_file` of verified names and non-recursive `remove_dir`; no recursive deletion exists (a test asserts it).
4. **Re-verification at use.** The staged file is re-verified (identity + digest) right before activation and again right before the rename into the version folder; the saved pointer is verified before it is restored as the active pointer; an uninstall refuses (`unrecognised_installed_content`, `installed_content_has_no_matching_receipt`) before anything moves unless every installed version folder holds exactly one `artifact.bin`, a single link, whose digest a valid committed receipt vouches for.
5. **No dangling pointer.** If the pointer could not be restored, the version folder it may still name is kept and reported.
6. **Tests** (`lifecycle_tests.rs`): same-content replacement (new inode) refused; hard-linked alias refused (the alias is untouched); in-place edit with unchanged size refused by digest; staging expected-name substitution; substitution between the first check and the activation; crashed install with an owned partial file removed vs a substituted one kept across repeated recoveries; staging folder replaced by a junction/symlink (outside folder byte-identical); substituted `pointer.json` / version artifact in a committed update's or uninstall's snapshot kept; a forged saved pointer never restored; uninstall preflight refusals; out-of-root canary unchanged. Mutation checks (identity, link count, digest, pointer verification each disabled) each fail at least one test.

What this does **not** do (and never claims): it is still path-based. Between the check and the unlink another process of the same user can still swap the name (§6). Journals written before Phase 3B have no `owned` field and are not readable by the new digest rule — there are none outside tests, because the engine has never been connected to the application.

## 3. Manager-owned data root (design)

* The root is derived **only** from the operating system's per-user application data directory (Tauri `app_data_dir`, or the existing `ATM_DATA_DIR` override for tests), never from an IPC argument, a recipe field, a plan field or a UI value. No command accepts a path that becomes writable.
* Layout (all below one root `…/AI Toolchain Manager/managed/<root-id>/`): `versions/<tool>/<version>/`, `state/`, `journal/`, `receipts/`, `staging/`, `snapshots/`, `locks/`, `data/<tool>/` (the tool's own user data — never touched by rollback/uninstall).
* **Ownership is recorded independently of the folder itself.** At creation the manager writes into **its own database** `(root_id, computer_id, created_at, root identity = volume serial + file index / dev + inode, creation journal digest)`. A marker file inside the folder is only a hint. A folder that merely carries a copied marker and the right sub-folder names is refused unless its recorded identity matches. (The database is user-writable; same-user tampering is outside the threat model.)
* Created with owner-only permissions: on Windows an explicit DACL for the current user only (inheritance off); on macOS mode `0700`. Existing folders are never adopted.

## 4. Safe ancestor traversal (design)

1. Open the **known-good anchor** (the application data directory) once, by handle / descriptor, and record its identity.
2. Walk down **component by component**, each opened relative to the previous handle, never from a full path string:
   * **Windows:** `NtCreateFile`/`CreateFile` relative to the parent directory handle with reparse-point handling set to *open the point itself* (`FILE_OPEN_REPARSE_POINT`), then reject any result whose attributes include `FILE_ATTRIBUTE_REPARSE_POINT` (junctions, symlinks, mount points, cloud placeholders). Hold directory handles without `FILE_SHARE_DELETE` so that renaming/replacing the held directory fails while the transaction runs (*assumption: to be confirmed by a Phase 3C test*).
   * **macOS:** `openat(dirfd, name, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)` per component; `O_NOFOLLOW_ANY` and, on versions that have it, `O_RESOLVE_BENEATH` as additional protection (*availability per macOS version must be checked*); `fstatat(…, AT_SYMLINK_NOFOLLOW)` for existence checks.
3. **Mount transitions:** compare the device (`st_dev` / volume serial) of each opened directory with its parent's and with the root's recorded device; a change is a refusal. Mount points and volume junctions on Windows are reparse points and already refused.
4. Every handle stays open for the whole operation; after the last component the engine compares the opened identity with the manifest.

## 5. Handle-relative creation and deletion (design)

* **Create** with exclusive-create semantics relative to the held directory handle (`CREATE_NEW` / `O_CREAT | O_EXCL | O_NOFOLLOW`), read the identity **from the new handle**, record it in the journal, fsync file and directory, then write.
* **Delete** only through a handle that was just identity- and digest-verified:
  * **Windows:** open the file with `DELETE` access and `FILE_FLAG_OPEN_REPARSE_POINT`, compare identity/link count/digest through that handle, then mark it for deletion **on that handle** (`SetFileInformationByHandle` with `FileDispositionInfoEx`, flags `DELETE | POSIX_SEMANTICS`, or `FILE_FLAG_DELETE_ON_CLOSE`). The deleted object is exactly the verified one; a name swapped in the meantime is not touched.
  * **macOS/POSIX:** there is no "delete this inode" call. The best available is `fstat` on an `openat` descriptor followed immediately by `fstatat(dirfd, name, AT_SYMLINK_NOFOLLOW)` to confirm the same `(dev, ino)`, then `unlinkat(dirfd, name, 0)`. A residual window of microseconds remains (§6).
* **Never recursive.** Directories are removed with `rmdir`/`RemoveDirectory`/`unlinkat(AT_REMOVEDIR)` only after they were listed through the held handle and found to contain nothing but verified, already-deleted entries.

## 6. Concurrency and TOCTOU boundary (disclosed)

* One writer per tool (the lock file with the transaction id) and one transaction journal; the manager cannot stop **another process of the same user** from touching files in its folder.
* With Windows delete-by-handle and non-shared directory handles the check and the deletion refer to the same object. With POSIX the final `unlinkat` is name-based: a same-user attacker who wins a microsecond race can make the manager delete **a file with the expected name inside the manager-owned, owner-only folder** — never a file elsewhere (the directory descriptor is fixed, links are not followed, the folder removal is non-recursive). This is the documented residual risk and it needs no new platform capability to understand, but it **cannot be eliminated without one** (no inode-based unlink on macOS).
* Quarantine-by-rename (`renameat` into a manager-owned trash folder, re-verify there, then delete) narrows the window further but mutates a foreign file's location if the check fails; the design therefore prefers refuse-and-report.

## 7. Interrupted transactions, fail-closed recovery, unknown files

* Recovery reads the journal (digest-protected) and the manifest; it deletes only manifest-verified files, otherwise reports `NeedsAttention` / `LeftoversRetained` **on every start** without changing a byte, and keeps the lock so no new work starts on that tool.
* **Unknown files are preserved**: anything not in the manifest, any folder, link, hard-linked or substituted file, any entry on a different device. Nothing is quarantined or "cleaned up" on a guess.
* A failed removal is reported with counts and kinds, never names or paths, never secrets.

## 8. What is impossible without new platform capabilities

| Wanted | Why not | Fallback |
| --- | --- | --- |
| Atomic "delete exactly this inode" on macOS | no such system call | verified `unlinkat`, residual window disclosed |
| Defence against a same-user attacker | the attacker has the manager's own privileges | owner-only folder; detection + refusal, not prevention |
| Proof that a file was created by the manager (rather than adopted) after the fact | filesystems keep no creator | identity recorded at creation in the manager's database + journal |
| Preventing Windows from deleting a file open elsewhere without sharing | by design the OS refuses; in-use files block uninstall | see `ADAPTER-PROCESS-SAFETY.md` |

## 9. Phase 3C acceptance tests for this design (to be written, not done)

Substitution under every expected name; junction (Windows) and symlink (macOS) at every path component including the anchor's descendants; hard-link alias; concurrent swap using a barrier-synchronised second thread/process; mount transition (macOS disk image mounted over a sub-folder; Windows volume mount point); unexpected entries; crashed-and-resumed cleanup at every journal state; out-of-root canary byte comparison; cloud-sync placeholder files; case-insensitive and Unicode-normalising names.
