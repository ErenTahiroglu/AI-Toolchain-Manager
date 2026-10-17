# Phase 2B — Security closure, provenance and passive version verification

Baseline `8f82df9ba75dcb27351b7fbad6cc634b2f5e8817` (Phase 2A closure). Issue #6 is closed first; then the discovery model gains a **passive** evidence ladder. **Nothing is installed, updated, started or probed.** Active version probing is only *proposed* ([ACTIVE-PROBE-POLICY-PROPOSAL](ACTIVE-PROBE-POLICY-PROPOSAL.md)).

## Issue #6 — RT-2A-R1: macOS file-system checks fail closed and cover every depth

* One decision point, `judge_fstype`: a **failed `fstatfs` is never "local"** (`UnknownFilesystem`); documented network types (smbfs, nfs, afpfs, webdav, cifs, autofs, …) are `Network`; only types known to be local storage (apfs, hfs, msdos, exfat, ntfs, udf, cd9660, tmpfs, ufs, zfs) pass; **anything else, including FUSE/macFUSE, is refused**.
* The judgement runs on **every directory handle**: `/`, each component of the selected root, and each child below it (`open_root` and `open_child_dir`). A volume mounted at `project/.claude` or any deeper level is therefore refused before one entry of it is opened. The earlier "root only" check is gone.
* Tests: pure table of the judgement (runs on Windows and macOS); on Unix, a simulated `fstatfs` seam keyed by inode covers failure on the root, failure below the root, a network type at a nested folder (its local sibling still reads), an unrecognised type, a network type on a PATH folder / home folder, and the normal local case through the **real** lookup. A hook assertion proves no file below a refused folder is opened.
* **What was NOT done:** no network volume was mounted or accessed; the Unix seam tests run only on macOS CI. No live-mount claim is made.
* **Remaining macOS limitations:** (1) opening an *automount trigger* directory may itself make macOS mount the share before the type can be read; `autofs` is refused after the fact, not prevented. (2) A mount that appears between the type check of a directory and a later read of a file in it is limited to the same handle (files are opened relative to the already-judged handle), but the OS may still redirect an *entry* of that directory only if it is itself a mount point — that child is judged when opened as a directory. (3) Local-but-remote-backed types not in the allowlist (e.g. a cloud-sync provider's FUSE view) are refused, which can hide a legitimate install; this is deliberate. (4) Only the file-system *type* is judged, not whether an APFS volume is an encrypted/disk-image mount of remote content. (5) Other Unix systems are not a supported target and are not judged. Windows nested mounts: any mount point/junction below the root is a reparse point and already refused by the component walk; remote drives and UNC paths are refused at the root.

## Issue #6 — RT-2A-R2: backups, free pages and what "sanitized" means

Three different things, kept apart (and tested separately):

| Layer | State after migrating a Phase 2A database | Evidence |
| --- | --- | --- |
| **SQL-visible rows** of the live database | Scrubbed by migration 5 (paths, server names, undocumented versions) | `count_failing_rows == 0`; privacy-gate tests |
| **Raw bytes of the live database file** (+ WAL) | Migration runs with `secure_delete = ON`, then `wal_checkpoint(TRUNCATE)`, `VACUUM`, checkpoint again, **only when the privacy migration runs**. A separate mechanism test shows that a plain `DELETE` leaves text in free pages (control) and that this sequence removes it. On the synthetic v4 fixture the in-place `UPDATE` already overwrote the text, so that particular check passes with or without compaction; the compaction is defence in depth. | `backups::tests` |
| **Pre-migration backups** (`backups/atm-v<N>-<stamp>.sqlite`) | **Unchanged on purpose**: a verbatim rollback copy still holds the old text. | test reads the backup bytes and finds the old text |

Beyond that, no claim is made about SSD wear levelling, OS volume snapshots, shadow copies, the Recycle Bin/Trash or cloud-synced folders. Logical compaction of one file is not physical erasure.

**Backup retention decision.** Backups are never deleted automatically. A new read-only command `preview_backup_cleanup` (the 29th typed command; allowlisted in `build.rs`, `capabilities/default.json` and the self-audit) and the *Yedekler ve gizlilik* panel:

* list recognised backups by **file name only** (schema number + time stamp; no folder, no account name), size and time; unrecognised files are counted, never named;
* open each backup **read-only** and count rows that today's privacy gate would reject (`yes` / `no` / `unknown` if the file cannot be opened); only counts are returned;
* check preconditions on the live database: latest schema, `PRAGMA integrity_check`, audit-chain verification, no rows failing the privacy gate;
* state the trade-off: deleting a backup removes the ability to roll back to that version, and file deletion does not guarantee physical erasure;
* **`deletionAvailable = false`, `executed = false` always.** The preview is the whole feature; execution needs a separate, explicit authorization and is not implemented. Nothing is uploaded; backups stay in the application's own data folder.

## Passive identity ladder

Six separate facts per observation, never merged into one "ok":

| Rung | Meaning | Where it can come from |
| --- | --- | --- |
| 1 Filename detected | A file with a catalogued name exists (PATH, documented native/legacy location) | existing PATH / install-location detectors |
| 2 Installation record found | An installer or package manager recorded it | Claude Code plugin registry (`installed_plugins.json` + cache folder); npm global `package.json` next to a PATH hit |
| 3 Publisher / source metadata | The record names a marketplace or repository | registry marketplace key (`marketplace`); `package.json` `repository` (`self_declared`) |
| 4 Identity validated | **Only** an installer's own record that names the **catalog-pinned** marketplace, with the record present | plugins from a pinned marketplace (today `frontend-design@claude-plugins-official`) |
| 5 Version from trusted metadata | `trusted` = Claude Code's own registry; `declared` = a package's own metadata; never from running anything | registry, `package.json` |
| 6 Functionality tested | **Always no** | — |

States: identity `not_assessed | unknown | evidence_available | validated | conflicted`; version `unknown | known | conflicted`. Invariants are enforced in `Observation::finalize` regardless of what a detector asks for (validated needs a record **and** a marketplace publisher; a version that fails the privacy gate becomes unknown; `identity_verified` is derived, never set separately). A settings declaration alone is configuration, **not** provenance (a tightening of the Phase 2A rule).

* **Conflicts are reported as conflicts:** a package whose own name differs from the expected name; a source repository that differs from the catalog's; a same-named plugin from a second marketplace next to the genuine one; registry or package records that name different versions; PATH copies where only some have records (→ `unknown`).
* **Embedded signature blocks** (Windows `.exe`): only the PE header (first 4 KiB, read through the same no-follow file layer) is parsed to see whether a certificate table exists. That is `present_unvalidated`: the chain, signer, revocation and file hash are **not** checked, and it never raises identity or yields a version. macOS code-signature parsing is not implemented (`signature_parsing_unsupported` is listed as a check not performed).
* **Not supported (listed per observation as checks not performed):** Scoop/winget/Homebrew/pip/uv/pipx/cargo records (only npm global packages are read; Scoop's `current` is a junction and Homebrew's bin entries are links, which the no-follow layer refuses); installed-file digests (the catalog pins archive digests, never the digest of an installed binary, and an archive digest cannot be compared with an extracted file); linkage between a package record and the PATH file (the shim is not parsed).
* Catalog pins used: the entry's source repository (`owner/repo`, from its link). The npm package name for `agent-browser` is the one its README documents (R28); it is a mapping, not proof.
* Persistence: migration 6 (additive) adds `identity_state`, `version_state`, `provenance_json` (closed enumerations only; no path, no name, no free text). Existing rows keep every fact; their ladder is marked **not assessed**, except that an identity Phase 2A had already accepted (`identity_verified = 1`) and a shown version stay represented. The self-audit fails if a stored ladder is not closed vocabulary or disagrees with the verified flag.

## UI

The *Bilgisayar Envanteri* table gains a column of separate lines — Tespit edildi / Yapılandırılmış / Kimlik kanıtı mevcut / Kimlik doğrulandı / Sürüm biliniyor|bilinmiyor|çelişkili / İşlev test edilmedi / Çelişkili gözlemler / Eski gözlem — each with a link to that row's sanitized evidence text, and a collapsible list of **checks not performed, with the reason**. Nothing is green; a merely detected tool shows "Kimlik doğrulanmadı" and "İşlev test edilmedi". Rows from before Phase 2B say their ladder was not assessed. The backup panel is described above. No install/update/run/delete button exists.

## Still not allowed (unchanged)

No execution of discovered programs, no `--version` on PATH entries, no agent/MCP/Strix start, no installation or update, no configuration writes, no browser automation, no network, no telemetry, no change to Windows Smart App Control or Defender.

## Blockers before any active probe

1. No pinned digest of installed files in the catalog; no signature-chain validation (`WinVerifyTrust`-class API use needs its own review: network revocation lookups, policy interaction).
2. No record-to-file linkage for package managers.
3. No sandbox/job-object/process-tree implementation or tests; no per-entry probe allowlist.
4. macOS GUI and real-mount behaviour untested manually.
5. Independent review of [ACTIVE-PROBE-POLICY-PROPOSAL](ACTIVE-PROBE-POLICY-PROPOSAL.md).
