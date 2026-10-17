# Phase 2C — Safe backup inspection closure and passive artifact identity

Baseline `d62883d71c0304c112dc91689c6dc846e6fb0779` (accepted Phase 2B). Passive verification only: **nothing is executed, installed, updated, downloaded or sent anywhere**, no agent configuration is written, and no Windows security setting is touched. It is acceptable — and today's actual outcome — that every real tool remains unverified.

## First gate — Issue #8

### RT-2B-R3: a partial scan is never a complete scan
* The backup scan is now four **layers**, each ending `completed`, `partial` (a limit was reached), `unreadable` (could not be read, was refused, or changed while read), `unsupported` (e.g. image too large to load) or `not_present`:
  `observation_rows` (privacy-gate fields), `all_text` (every TEXT cell of every table), `raw_main` (streamed raw bytes), `raw_wal` (streamed raw bytes of the sidecar). Reasons are fixed codes (`cell_limit`, `size_limit`, `time_limit`, `read_error`, `redirected`, `changed_while_scanning`, …), never content.
* Limits are explicit and reported (`ScanLimits`: 500 000 cells, 256 MiB raw per file, 128 MiB loaded database image, 30 s per backup); the UI shows them.
* Status precedence: a finding is always reported (even after a partial scan: "hit preceding partial scan"); then anything unreadable/refused → `not_assessable`; then schema < 5 → `potentially_sensitive`; then any partial/unsupported layer → **`partially_scanned_no_findings`**; only a fully completed, finding-free scan is `none_detected_in_examined_content`. `coverage` = `complete | partial | incomplete`.
* Memory is bounded: raw bytes are streamed in 1 MiB chunks with a 512-byte overlap (a pattern split across two chunks is still found and not double-counted); the SQL layers load the image only up to the cap (`deserialize` from the confined handle) and the cell scan stops at its budget. A change of size/modification time during the scan makes the layers `unreadable (changed_while_scanning)`.
* Tests: 500 000+ cells with a secret after the budget (partial, no finding, never "none detected"); small-limit variants for complete / partial-clean / partial-with-later-secret / partial-with-early-finding; raw and image size caps and an expired deadline; exact-fit cap is complete; a pattern split across chunks; read failures (malformed, truncated, directory, redirected); a file changed between the main scan and the sidecar open; identical repeated scans give identical plans. Files above 256 MiB are not created in tests: the limits are exercised at small values and the defaults are asserted.

### RT-2B-R4: backup path confinement
* The backup folder and every backup file and `-wal` sidecar are opened through the **same confined layer as discovery** (`fsx::open_regular`): no symlink/junction/reparse point in the file or in any parent, no network or unrecognised file system, a handle-based check on Windows and `openat(O_NOFOLLOW)` from `/` on macOS. The SQL layers, the raw scan and the sidecar scan all read from the **one handle** (the SQLite image is loaded from that handle, not by re-opening a path).
* A refused or unreadable sidecar makes the whole backup `not_assessable`; a backup folder that is itself a link (or below one) is `refused_redirect` and no file in it is touched. Symlinked backup, symlinked WAL, parent junction, junction in place of a backup or WAL, and a swap between listing and opening are all tested with an **outside sentinel** (a database containing path- and secret-shaped text): its content is never counted.
* Local evidence: Windows ran the junction / parent / swap / changed-while-scanning cases and the mutation check (replacing the confined open by `File::open` makes two tests fail). The three **file-symlink** cases need a privilege this account does not have (they print "not exercised" locally) and run on macOS CI.
* **Remaining TOCTOU limitation:** between listing a folder and opening a name, a file can still be replaced; the replacement is opened (never a link target) and a size/time change during the scan is detected, but a same-size, same-timestamp replacement between listing and open cannot be told from the original. A hard link (not a reparse point) to an outside file is indistinguishable from a regular file.
* Nothing is deleted, rewritten or moved; deletion remains unavailable.

## Passive artifact hashing

* `fsx::hash_file` streams one **already-open handle** through SHA-256: bounded chunks, size cap (512 MiB), deadline (20 s), cancel flag, refusal of links/redirects/non-local volumes (same layer), and `ChangedWhileReading` when the handle's size or modification time moved or the byte count disagrees with the size seen at the start. Every failure is `Unknown(<code>)`, never "verified". A test swaps the path during hashing and shows the digest is that of the file that was opened. Limitation: a rewrite that keeps both size and (coarse) timestamp between two chunks is not detectable.
* **Permission:** reading file content is a new kind of reading, so it has its own off-by-default checkbox ("parmak izini (SHA-256) hesapla"), only available together with the PATH permission; the request is dropped (not widened) without it. The permission is stored per scan (`discovery_scans.authorized_fingerprint`) and shown per datum.
* Scope: only the **one file that would run** for a PATH hit, and Claude Code's documented native install file. Plugins are directories: no digest of a directory or skill collection is defined or accepted (`plugin_directory_digest` is listed as not performed). No crawling; no hash of anything not approved by a typed detector.
* No shell, PowerShell or helper program is used; the digest is computed in-process.

## The trust contract (validation policy v1)

Versioned contract: [`schemas/artifact-provenance.schema.json`](../schemas/artifact-provenance.schema.json) (validated by the Python contract tests against the built binary's output, including negative cases that must be rejected). Evidence order: 1 discovered · 2 fingerprint computed · 3 installer/package record · 4 record linked to the exact artifact · 5 reference digest available · 6 reference independently authenticated · 7 exact artifact matches that reference · 8 functional verification (**never performed**).

* A local SHA-256 is a **fingerprint**, not proof of origin.
* `identity_verified` / identity `validated` require 2, 5, 6 and 7 together with a reference that is **independently authenticated** and of an **exact-file kind** (`executable`, `script`, `plugin_file`). That decision is made in one place (`Observation::finalize_provenance`) and re-derived on every read from the stored digest and the code-embedded references.
* Reference sources: **authenticated** = the compiled-in list `BUILT_IN_AUTHENTICATED` (code-reviewed; **empty today**: no authoritative, independently signed digest evidence for an installed file exists and the application fetches none). **Unauthenticated** = rows of `reference_digests` and anything copied from a README, `package.json`, a repository or typed by a user: the table's CHECK constraint allows only `authentication = 'none'`, so even a hand-edited database cannot create an authenticated reference, and a README-sourced digest that equals the local hash is shown as "matches an unauthenticated claim", never as verification.
* An **archive, installer or package** digest is never compared with an extracted file (`incomparable`), even when the values are equal; one file digest never authenticates a plugin directory.
* Per-reference fields: entry, artifact kind, format, OS/architecture (`platform_target`), exact version, algorithm (`sha256`), digest, source, authentication. Different platforms and different versions are separate references (a Windows digest never applies on macOS; with several versions the matching one is reported). No digest is created for any catalog entry, none is derived from a local file, none is fetched.
* **Invalidation:** every verdict stores the policy version and the *epoch* (a SHA-256 over the whole reference set). On read, a changed policy or reference set voids the verdict (the identity is lowered and flagged `reclassified`, a version that came only from an authenticated reference is withdrawn). A verdict from a scan older than 24 h or superseded by a newer scan is not "current", and `identity_verified` is exposed only for current, non-invalidated observations: **an old fingerprint never stands for the file as it is now**, and the application has no code path that executes a file anyway.

## Package record → artifact linkage (findings)

| Source | Documented, read-only linkage to the exact installed file? | Result |
| --- | --- | --- |
| Claude Code plugin registry (`installed_plugins.json`) | The record names marketplace/plugin/version, and the cache folder `cache/<market>/<plugin>/<version>` can be checked. That ties the record to a **folder name**, not to file content; the registry and the folder are user-writable. | `record_link = folder_by_name`; identity at most `record_corroborated` |
| npm global package (`package.json` next to a PATH hit) | Shims (`.cmd`, symlinks into `lib/node_modules`) are wrappers; parsing a shim would only tie two user-writable files together. A matching `package.json` is not linkage. | `record_link = unverifiable`; the shim itself is fingerprinted as the exact file that runs; identity `evidence_available` |
| Claude Code native install file | No installer record exists for the file | filename + fingerprint + embedded-signature-block presence only |
| Homebrew / Scoop / winget / pip / uv / pipx / cargo | Layouts depend on links/junctions (Homebrew bin entries, Scoop `current`) that the no-follow layer refuses, or on undocumented receipts | **Unsupported**; not guessed |

## UI

The Computer Inventory page shows separate lines — file found; fingerprint computed (short prefix only, "identifies bytes, does not prove origin"); source metadata available; reference digest: **none / available but unauthenticated (self-declared) / available and authenticated**; match / mismatch / "cannot compare" / "verification void"; record link; identity verified / not verified; functionality not tested — plus the checks not performed with reasons. Nothing turns green because a hash exists. No path is shown; the backup panel lists layer outcomes, coverage, limits and the folder status.

## Database

Migration 8 (additive, transactional): `discovery_scans.authorized_fingerprint` (older scans: 0) and the always-unauthenticated `reference_digests` table. Fingerprints and verdicts live in the existing `provenance_json` (closed vocabulary + one hex digest; the self-audit rejects anything else). Preferences, history, audit chain and earlier trust downgrades are preserved (tested through all migrations 1–8, including failure rollback).

## Not done / limitations

* No authenticated reference exists, so **no real tool can be verified**. The compiled-in list is empty by test. The synthetic probe in `--self-check` injects its own synthetic authenticated references (and removes its scans afterwards) only to exercise the full path in the built binary.
* No signature-chain validation (`WinVerifyTrust`-class calls need their own review: revocation lookups can use the network); macOS code signatures are not read.
* No record-to-file linkage for any package manager.
* Hash-time TOCTOU and hard-link limits as described above; no live network-volume test.
* macOS behaviour is exercised only by CI; Windows file-symlink cases are exercised only on macOS CI.

## Safety gates before any active version probe

1. At least one **authenticated, signed reference-digest source** with its own review (who signs, how the public key is pinned in code, rotation/revocation) — nothing in this phase provides one.
2. Verified record→file linkage (or probing only exact files that match an authenticated reference).
3. The probe policy ([ACTIVE-PROBE-POLICY-PROPOSAL](ACTIVE-PROBE-POLICY-PROPOSAL.md)) reviewed independently, with job-object/process-tree handling, environment isolation and an adversarial suite.
4. Cross-platform exact-SHA CI, a manual macOS pass, and explicit user authorization separate from this document.

## Red Team closure (Issue #9)

Baseline `8f60ad1`. Two findings; nothing executable was added and the production authenticated reference list is still **empty**.

### RT-2C-01 — transactions committed only to the SQLite WAL

* **Root cause.** The SQL-visible layers (`observation_rows`, `all_text`) load the **main database file** from the confined handle. A `-wal` sidecar can hold committed transactions that were never copied into that file, so those rows were invisible to the SQL layers while the entry could still read `completed` / "no violation detected" if the raw WAL bytes held no path-shaped text.
* **Fix (conservative; no replay, no checkpoint, no VFS, nothing written).** A new layer `wal_logical_sql` answers "could the SQL layers have seen what the WAL holds?" separately from `raw_wal` ("a bounded pattern scan over the WAL's bytes"):
  * no sidecar → `not_present`; empty sidecar (`wal_empty`) or a valid 32-byte header without frames (`wal_no_frames`) → `completed`;
  * a valid header followed by frames → `partial / wal_not_replayed` (committed data may exist only there);
  * a non-empty sidecar whose header is truncated or foreign → `partial / wal_header_unrecognised` (never assumed empty);
  * a sidecar that cannot be opened (link, junction, directory, non-local volume, access) → `unreadable` for both WAL layers, so the backup is `not_assessable`; a sidecar whose size/time changed during the scan → `unreadable / changed_while_scanning`.
  Any `partial` layer lowers the coverage to `partial`, so such a backup can be `partially_scanned_no_findings` at best, never `none_detected_in_examined_content`.
* The raw layers (main file and WAL) now also report whether a **credential, e-mail or private-path shape** occurs anywhere in the bytes (`rawSensitiveShapeLayers`: a count of layers, never content), so a credential committed only to the WAL is a reported finding (`violations_observed`) instead of being invisible. This is a bounded pattern scan, not a SQL inspection.
* A WAL-mode main file declares journal mode WAL in header bytes 18/19, which an in-memory image cannot honour; the two bytes are rewritten **in memory** while the image is loaded (the file is never touched) so the main-file SQL layers still work for such backups.
* **Tests.** A real WAL-mode database with the value committed only to the WAL (auto-checkpoint off; fixture asserts the value is in the `-wal` and not in the main file): SQL layers see nothing, `wal_logical_sql` is `partial`, coverage `partial`, never "none detected", and the raw scan reports it. Clean WAL with frames → `partially_scanned_no_findings`. Absent / empty / header-only WAL do not lower coverage. Truncated inside the header, inside the first frame, and foreign bytes are never trusted to be empty. A directory in the place of the sidecar and the existing symlink/junction cases make both WAL layers unreadable. Inspecting a WAL backup leaves every byte identical and creates no `-shm`/`-journal`/checkpoint. Mutation: treating `wal_not_replayed` as `completed` fails three tests.
* **Remaining limitation.** The WAL's frames are not replayed, so what the WAL holds is only pattern-scanned. A secret that matches no pattern, or lies beyond the raw budget, is not found; a WAL replacing a main-file row is not applied.

### RT-2C-02 — type binding of an authenticated reference (validation policy v2)

* **Root cause.** Policy v1 compared the digest of the opened file with exact-file references of the right entry and platform. It trusted the reference's *claim* about its own kind and format and never checked what the opened file actually is, so a digest equal to the file's bytes but belonging to a script, a plugin file or a file of another format could still have matched.
* **Fix.** The hashed handle's first bytes (≤ 4 KiB, same handle as the digest, never the file name) establish the **verified format** (`pe-x64`, `pe-arm64`, `pe-x86`, `macho-arm64`, `macho-x64`, `macho-universal`, `elf-x64`, `elf-arm64`, `script-shebang`, `zip`, `gzip`, otherwise `unrecognised`). A reference binds to the opened file only if **all** agree: catalog entry, `platform_target` (and the reference's own format must be possible on that OS/architecture), artifact kind (the role implied by the verified format: executable / script / archive; a plugin file role is never produced because no plugin file is hashed), canonical format identifier, and the exact SHA-256. Otherwise the result is `incomparable` (never a "mismatch", which would wrongly mark a different artifact as conflicting) and the identity stays unverified. Two authenticated references that give one digest two versions, or one version two digests, make the set ambiguous (`incomparable`, nothing verifies). Free-text or missing format evidence in a reference is malformed and binds nothing.
* The verdict is re-derived on every read from the stored digest **and the stored verified format**; a hand-edited format cannot create a verification (the re-derived verdict differs from the stored one → `invalidated`). `finalize_provenance` additionally requires the verified format's role to equal the authenticated reference kind. `POLICY_VERSION` is now 2, so verdicts stored under v1 are invalidated on read. The JSON schema, the TypeScript types and the Python contract tests carry `verifiedFormat` (the Python test re-implements the synthetic headers independently).
* **Tests (synthetic references only).** Script ↔ executable, plugin file ↔ standalone executable, PE ↔ Mach-O (incl. a Mach-O format on a Windows target and the reverse), authenticated archive/installer/package digest ↔ extracted executable, conflicting versions, missing/malformed/truncated format evidence (reference and file), executable-named script, hand-edited stored format, and the correctly matching exact artifact. Mutation: removing the kind and format checks fails eight tests.
* **Format knowledge is not authenticity.** A matching verified format narrows what a reference may describe; it proves nothing about origin. The production list is empty, so no real tool is `identity_verified`.
