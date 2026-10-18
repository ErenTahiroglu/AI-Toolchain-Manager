# Archive and extraction security specification

Status: **specification + a metadata-only checker with synthetic negative tests** (`src-tauri/src/archive_check.rs`, `archive_check_tests.rs`). **No real artifact was extracted, and no extraction code exists**: the checker reads headers from bytes held in memory, never decompresses and never touches the file system (a test scans the source for file-system, process, network and decompression APIs). Extraction itself is a Phase 3C+ item and, for the recommended first pilot (jq: raw single executables), is **not needed at all**; it is specified now because the fallback candidates (ripgrep, Gitleaks) need it.

## 1. Two kinds of evidence, never one

| Evidence | Established by | Authenticates |
| --- | --- | --- |
| **Archive digest** — SHA-256 of the downloaded `.zip` / `.tar.gz` | compared with a reviewed reference | that these are the bytes that were reviewed |
| **Extracted-file digest** — SHA-256 (and size) of each file that will be installed | recorded in the reviewed recipe's manifest | that the file that ends up on disk is the reviewed one |

An archive digest does **not** authenticate what comes out of it (a different extractor, a different platform behaviour or a bug can produce other bytes), and the extracted-file digest is a **separate fact** that must be reviewed, not derived at install time from the archive. The checker enforces `archive_digest_reused_as_file_digest` (a manifest that uses the archive's digest for an inner file is refused) and the manifest sanity rules (well-formed lowercase SHA-256, no duplicates, safe names, size ≤ limit).

## 2. The expected-entry manifest

A reviewed recipe for an archive artifact carries, in addition to the archive digest: the **complete, exact** list of entries that may be extracted (`name`, extracted `size`, extracted `sha256`, `executable` bit). Everything else in the archive is a finding. Folders are accepted only as parents of expected files. Names are relative, ASCII, forward-slash separated.

## 3. Checks (all implemented as header checks and covered by tests)

| Area | Rule | Finding codes |
| --- | --- | --- |
| Containers | ZIP: end record present, single disk, **no ZIP64**, no multi-disk, compression method stored (0) or deflate (8) only, central directory in range and without trailing bytes, **every local header must exist and name the same entry** (classic confusion), data ranges must **not overlap** (overlapping-entry bombs). TAR: ustar only, checksum verified, base-256 numbers refused, **PAX / GNU long-name headers refused**, end-of-archive marker required, data in range | `zip_*`, `tar_*` (container errors) |
| Names | no `..`, no absolute path, no drive letter / UNC, no backslash, no `:` (NTFS alternate streams), no empty or `.` component, no NUL or control character, valid UTF-8 **and ASCII only** (so Unicode normalisation / look-alike collisions cannot occur), length ≤ 255, no Windows reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`, with or without extension), no trailing dot or space, none of `< > " \| ? *` | `path_traversal`, `absolute_path`, `drive_letter_path`, `backslash_in_name`, `colon_in_name`, `empty_path_component`, `dot_path_component`, `nul_in_name`, `control_character_in_name`, `name_not_utf8`, `non_ascii_name`, `name_too_long`, `windows_reserved_name`, `trailing_dot_or_space`, `windows_forbidden_character` |
| Uniqueness | exact duplicates; **case-insensitive collisions** (`A.bin` vs `a.bin`) | `duplicate_entry`, `case_collision` |
| Entry kinds | symlinks (including NTFS reparse points), hard links, devices/FIFOs/other special entries, encrypted entries are refused | `symlink_entry`, `hardlink_entry`, `special_file_entry`, `encrypted_entry` |
| Permissions | setuid / setgid / sticky, world-writable files; the executable bit must equal the manifest's | `setuid_setgid_or_sticky`, `world_writable_file`, `executable_bit_mismatch` |
| Content expectations | every entry must be expected; expected size must match the header; extra scripts / executables (by extension or execute bit) are flagged specifically | `unexpected_entry`, `size_mismatch`, `unexpected_executable`, `missing_expected_entry` |
| Limits (defaults) | ≤ 64 entries, ≤ 64 MiB per entry, ≤ 128 MiB in total, unpacked/packed ratio ≤ 20 for entries larger than 64 KiB | `entry_count_limit`, `entry_size_limit`, `total_size_limit`, `compression_ratio_limit` |

Findings carry an entry **index**, never the entry name (names come from an untrusted archive), and never a path.

## 4. Gzip layer for `.tar.gz` (design only)

`.tar.gz` needs a decompressor before the TAR reader can see headers. Requirements for the Phase 3C+ implementation, none of which exist today: a streaming, bounded decompressor that counts **output** bytes against `max_total_size` and against a ratio limit *while* decompressing, aborts at the first excess, accepts exactly one gzip member with no trailing data, and never buffers more than the limit. The TAR reader then runs on the bounded output. Without that, `.tar.gz` remains unsupported (fail closed). Use a vetted decompression crate; do not hand-write DEFLATE.

## 5. Extraction procedure (design only; applies only after §3 returned no finding)

1. Fetch the archive into the staging folder created by the transaction (manifest entry recorded **before** writing; see `FILESYSTEM-OWNERSHIP.md`). Verify the archive digest against the reference **before** parsing anything.
2. Run `read_zip` / `read_tar` + `check`. Any finding → refuse, delete only manifest-owned staging files.
3. Extract **each expected entry** to a fresh path built by the manager from the manifest name (never from the archive's name), with exclusive create, no link following, permissions set by the manager (never from the archive), and a hard cap on bytes written equal to the manifest size. Write to `…/staging/<txn>/extract/<name>.part`, fsync, re-hash through the handle, compare with the **extracted-file digest**, then rename to its final staging name. A mismatch or extra byte → refuse.
4. Only files that passed step 3 are eligible for the atomic placement step of the install transaction. The archive itself is deleted from staging (manifest-owned) after success, and kept for diagnosis only if the user opted in.
5. **Interrupted extraction:** the journal lists every `.part` / final staging file with identity; recovery deletes only those, unchanged, and reports anything else.
6. **Cleanup / rollback** use the same all-or-nothing verified deletion as the lifecycle engine.
7. Unexpected executables or scripts in the archive never reach the disk: they are findings in step 2, not files to delete later.

## 6. What the checker does not and cannot cover

* It validates headers, not content: an archive whose headers lie about sizes is only caught during bounded extraction (step 3).
* Deflate streams and CRC are not verified (no decompression exists); the extracted-file digest is the real integrity check.
* Behaviour of any particular OS or extractor is irrelevant on purpose: the manager writes the files itself.

## 7. Tests (synthetic, hand-built bytes)

Accepted baseline ZIP and TAR; 20 hostile names in both formats; NUL byte; duplicates and case collisions; symlink (Unix mode and NTFS reparse), hard link, devices, setuid/setgid/sticky, world-writable, encrypted; unexpected entries and executables, executable-bit mismatch, wrong size, missing entry; entry-count, size, total-size and compression-ratio limits (including a header claiming 200 MB from 1 KB); ZIP64 marker, multi-disk, out-of-range central directory, unsupported method, **local-header name mismatch**, **overlapping entries**; TAR bad checksum, not ustar, missing end marker, truncated data, PAX/GNU extension headers, base-256 size; **every prefix and every single-byte flip** of valid archives never panics; archive-digest reuse; unsafe manifests; findings never contain a name; the module source contains no file-system, process, network or decompression API. Mutation checks (traversal rule, local-name equality, overlap check, digest-reuse rule disabled) each fail a test.
