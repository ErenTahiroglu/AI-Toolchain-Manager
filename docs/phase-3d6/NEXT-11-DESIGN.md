# NEXT-11 design record: bounded filesystem hardening (AUTHOR EVIDENCE, local and uncommitted)

**Status of this record.** The author of this change also wrote the earlier implementation, the CI delta and the NEXT-05 to NEXT-09 evidence. Nothing here is independent Red Team acceptance. The Rust changes and Rust tests described below were written in an environment where local Rust builds were blocked (Smart App Control, `os error 4551`, and missing dependency artifacts for the `ece` crate). They are **NOT COMPILED and NOT RUN**. Nobody may treat them as passed until a CI or local run exists.

## Result per task

| Task | Result |
| --- | --- |
| 1. Journal replacement (H3) | **DESIGN_BLOCKED.** Not eliminated. The residual is documented and pinned by tests. |
| 2. Empty-folder identity binding | Implemented (not compiled). |
| 3. Capability-probe cleanup | Implemented (not compiled). |
| 4. `Created` phase | Unchanged on purpose. The residual is documented and pinned by a test. |
| 5. Tests | Written (not run). See "Test coverage". |
| 6. Documents | Done (`REMAINING-STEPS.tr.md`, `NETWORK-OBSERVATION.md`). |

## Task 1: why the journal replacement is DESIGN_BLOCKED

`Adapter::save` writes a fresh temporary file (exclusive create, fsync) and then moves it onto the journal name. The first write uses `move_file` (no replace, atomic refusal). Every later write calls `replace_owned_file`, which on Windows opens the destination, checks identity, and then renames with replace-if-exists. Windows cannot pin a destination against deletion and replace it in one operation, so a same-account process that deletes the journal and creates a file under its exact name between the check and the rename gets that file replaced. Blast radius: that one name inside the owner-only `journal` folder of the manager's own root. macOS exchanges, verifies and swaps back.

Options that were examined and why none was implemented:

1. **Versioned, exclusive-create journal records** (`<txn>.<seq>.json`). This removes replace-over-existing completely, and it is the recommended direction. It is not bounded: it changes the journal naming and the recovery format (`read_txn`, `recover`, `committed_receipts`, lock release), needs a rule to pick the newest valid record and fall back from a torn newest one, needs verified deletion of superseded records (a crash leaves several), and changes what every crash-point test expects. About 100 existing adapter and rehearsal tests exercise those paths. None of that could be compiled or run here.
2. **Rename the old journal away, create the new one with no-replace, delete the old one.** No replace remains, but a crash between the first two steps leaves no journal under its name, so an unfinished transaction becomes invisible to recovery. Making recovery adopt the renamed file is the same redesign as option 1.
3. **Two alternating slots overwritten in place through held handles.** A new on-disk format, and recovery would need the slot identities that only the journal itself records.
4. **Hold the destination open to pin it.** Windows then refuses the rename with a sharing violation. This was tried earlier (see `managed_fs_windows.rs`, `rename_file`). It must not be used as a fix.
5. **Check after the rename.** The renamed file keeps its identity, so nothing shows what it replaced.

What changed instead: `Adapter::save` now documents the exact residual, and tests pin it (below). H3 is **not** claimed eliminated. A fresh installation still creates the active pointer with a no-replace move (`adapter.rs`, the `move_file` branch); only update paths use `replace_owned_file` for the pointer.

To unblock: a dedicated design for option 1 with the full crash matrix run on Windows and macOS CI.

## Task 2: folder removal by identity

New contract: `Dir::remove_dir_verified(name, expect: Identity)` in `managed_fs.rs`, with `remove_empty_dir_checked(name, Some(identity))` in the Windows and Unix layers.

* **Windows.** The folder is opened with DELETE and `FILE_OPEN_REPARSE_POINT` (never follows a link), kind and identity are read from that handle, a link gives `Link`, a non-directory gives `NotDir`, a different identity gives `Changed`, and only then the deletion goes through that handle. The operating system refuses a non-empty folder, so there is no recursion.
* **macOS.** There is no inode-based `rmdir`. The name is read with `fstatat(AT_SYMLINK_NOFOLLOW)`, compared, and read again immediately before `unlinkat(AT_REMOVEDIR)`. A window of microseconds remains (the same documented limit as the verified file deletion).
* **Callers.** All eight folder removals in the transaction code of `adapter.rs` now use it, with the identity of the handle that was inspected and emptied: `remove_owned_folder`, `remove_snapshot` (version folders, `versions`, the snapshot folder), `run_uninstall` (version folder and version root), `undo_version_dir`, `undo_version_tool_dir`. A source-scan test keeps `.remove_dir(` out of the adapter. The identity-less `Dir::remove_dir` still exists for tests and is documented as unbound.
* **Residual.** The identity is the one of the folder that was inspected, not of "the folder this transaction created", because the journal records identities of files only. A different empty folder put under the same name **before** the inspection is therefore treated as the transaction's folder and removed if the inspection finds it empty (or filled only with manifest-matched files). Closing that needs directory identities in the journal, which is a journal-format change and is out of scope here.

## Task 3: capability-probe cleanup

`Dir::probe_atomic_rename` now records the identity of each scratch file the moment it exists and cleans up only entries that still carry one of those identities, deleting through a handle checked against that identity. Anything else under the probe's names is kept. The probe folder is removed with `remove_dir_verified` and the identity of the folder the probe created. Cleanup failures are returned as errors even when the capability itself was proven. The probe's checks are unchanged.

Consequence to review: the probe runs when a managed root is created and each time it is opened. A scratch file that another program holds open at that instant (for example a virus scanner) now makes the probe, and so the root check, fail closed instead of leaving a stray `capprobe-<random>` folder. That is deliberate and fail-closed, but it can refuse a root on a transient condition.

## Task 4: `Created` versus `Final`

Unchanged. A file in phase `Created` is recorded right after its exclusive create, before any data is written, so no content digest exists yet. Cleanup of such a file checks identity and a link count of 1 only. A `Final` file also checks size and SHA-256. An `Intended` file has no recorded identity and is `Ambiguous`: never deleted. Strengthening `Created` needs a second durable record between the create and the first write, which is a transaction-format change. The code comment on `classify`, the module header of `adapter.rs` and the operating document now state this. Nobody may claim that every deletion verifies content.

## Test coverage (all NOT RUN)

New tests:

* `managed_fs_tests.rs`: `a_verified_folder_removal_needs_the_expected_identity_and_is_never_recursive`, `a_folder_swapped_in_the_gap_before_a_verified_removal_is_never_deleted`, `a_link_in_place_of_a_folder_is_refused_by_the_verified_removal_and_its_target_stays`, `a_file_is_not_a_folder_for_the_verified_removal`, `the_probe_keeps_a_substituted_scratch_file_and_reports_the_leftover`, `the_probe_cleanup_is_bound_to_identities_in_the_source`.
* `adapter_tests.rs`: `a_created_file_is_known_by_identity_only_a_final_file_by_content_too_and_an_intended_one_is_never_deleted`, `the_adapter_removes_folders_only_by_identity_and_never_by_a_blank_expectation`, `a_journal_swapped_in_the_gap_before_a_replacing_save_touches_nothing_but_that_one_name`, `a_partial_journal_write_leaves_a_temporary_file_that_recovery_reports_and_never_deletes`.

Mapping to the ten required areas:

1. Journal destination substitution in the gap: new adapter test, plus the existing `a_destination_swapped_in_the_gap_before_a_replacement_is_not_clobbered_where_the_platform_can_tell` (`managed_fs_tests.rs`). The Windows outcome is the documented residual, so the tests assert what must hold and accept the documented outcome.
2. Partial journal write and recovery: new adapter test (temporary file kept and reported). A full crash matrix is covered by the existing `a_crash_at_any_point_of_an_install_is_recovered_to_a_consistent_state_or_reported_honestly` and its update and uninstall siblings.
3. Foreign journal file preservation: new adapter test (canary beside the journals), plus the existing `a_journal_that_was_replaced_by_another_file_is_never_overwritten_by_the_next_save` and `a_forged_journal_cannot_make_the_adapter_delete_a_foreign_file_and_a_corrupt_one_blocks_new_work`.
4. Ordinary empty-folder substitution: new `managed_fs` test (hook `before-dir-remove`). **No adapter-level test** exists for the substitution between inspection and removal inside a rollback, because the hook cannot be aimed at one specific removal deterministically. This is a gap to close in CI work.
5. Folder replaced by a junction or symlink: new `managed_fs` test, plus the existing `symlinks_junctions_and_parent_redirection_never_lead_outside_the_root`.
6. Probe-file substitution during cleanup: new `managed_fs` test.
7. Probe cleanup failure and retained leftovers: the same new test (a leftover makes the probe fail and stay). There is no test for a locked scratch file.
8. `Created` versus `Final`: new adapter test.
9. Crash recovery and automatic rollback: existing tests named above, plus `a_failure_at_any_point_of_an_install_is_rolled_back_completely_and_a_retry_succeeds`.
10. Foreign and out-of-root canary preservation: `outside_is_untouched` in the new adapter tests and in the existing ones.

No existing test was edited, loosened or removed. All fixtures use temporary directories. None touches the authorized B3 root.

## What a CI or local run must show before anyone relies on this

1. The `ai-toolchain-manager` library and its tests compile on Windows and macOS (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, with and without `--features jq-pilot`).
2. The ten new tests pass on Windows. The macOS-only expectations (`#[cfg(unix)]`) pass on macOS.
3. The whole existing Rust suite passes unchanged (in particular `managed_fs_tests`, `adapter_tests`, `managed_root` and the install-flow tests).
4. The identity-based removal behaves as described on a real NTFS volume, including a directory file-index that is not reused after a delete and re-create.
5. The default release build and the `jq-pilot` build still produce what they produced before; `B3_UNLOCK` and `B3_ARMED_DATA_DIR_SHA256` are untouched.
