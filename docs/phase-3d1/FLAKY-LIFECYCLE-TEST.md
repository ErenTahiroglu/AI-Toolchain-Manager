# The intermittent failure of `lifecycle::tests::a_failed_update_puts_the_previous_version_back_exactly` (Phase 3D-1 closure)

The failure was first seen on the development PC (Windows 11) as "one test failed on the first run after a build, passed afterwards". The Red Team asked for an investigation **without assuming antivirus is the cause**, with the first-failure evidence captured, and **without weakening or retrying the test away**. This is what was found.

## 1. Method

* The test file keeps its assertions **unchanged in strength**; only the failure *messages* were extended. On the first failing observation (never after a retry) they now include: the engine's report, every entry of the three work folders and one level below with the raw operating-system error of a metadata query and of an open, the same listing again after 50 ms / 0.25 s / 1 s / 3 s, and — in test builds only — what the engine's own classifier recorded for every entry it called "other" (`lifecycle::OTHER_LOG`).
* A reproduction loop rebuilt the test binary (a comment line changed so that every run is a **fresh** executable) and ran only this test, one cycle at a time, on the development PC: five loops before the fix, one after.
* Runs that were blocked by Smart App Control (`exit 126`, "never executed", the known `os error 4551` behaviour for fresh unsigned binaries) are counted separately and were not worked around.

## 2. Results before any fix

| Loop | Executed runs | Failures |
| --- | --- | --- |
| 1 | 9 | 2 |
| 2 | 14 | 1 |
| 3 | 20 | 3 |
| 4 | 20 | 2 |
| 5 | 18 | 2 |
| **Total** | **81** | **10 (about 12 %)** |

Two different symptoms were captured with their evidence:

**Symptom A (7 of 10) — the engine's rollback reports `failed`/`partial` for the staging folder**: `1 unexpected entry (0 file(s), 0 folder(s), 0 link(s), 1 other) were left untouched; nothing was deleted`. At the failure the staging folder still held the staged `artifact.bin` as an ordinary readable file that stayed there for more than 3 s (so a *persisting* leftover, caused by the engine refusing to clean). **In the one failure observed after the classifier instrumentation was added, the engine's own classifier had recorded: `ARTIFACT.BIN.PART.tmp: metadata ERROR raw_os_error=Some(2) kind=NotFound`** — an entry that the directory listing returned but that no longer existed one instant later: the temporary file the engine itself had just removed, still listed under an upper-case temporary name while Windows finished the deletion. The engine counted that vanished entry as unknown content and, by design, deleted nothing.

**Symptom B (3 of 10) — a post-condition read raced the same state**: the receipts folder listing taken immediately after a rollback that had correctly reported "uncommitted receipt removed" contained one extra transient entry (once as `TXN-….JSON.tmp`, upper-case), gone in the next listing a few milliseconds later; the engine itself had done nothing wrong.

## 3. What is established, and what is not

* **Established (direct evidence):** the failures are caused by a Windows file that is *being deleted* and is still visible in a directory listing. A metadata query on such an entry answers **"not found" (2)** or **"access denied" (5)**. The second answer was reproduced *in a unit test without any other program involved* (a thread creating and deleting files while another lists the folder: `churn-2: metadata ERROR raw_os_error=Some(5)` and `NotFound`). The engine turned that transient state into "unknown content" (symptom A), and a test read the state while it lasted (symptom B).
* **Not established:** *which program* keeps the handle that makes the deletion linger in the failing real runs. Antivirus is plausible (every failing run was a fresh unsigned binary), but it is **not proven and not excluded**; a second program's handle is not needed to explain the in-process reproduction (the lister's own metadata query holds a handle for a moment). The reason for the upper-case temporary name is not known. The observation that failures cluster after a fresh build cannot be separated from the loop design (every loop run was a fresh build); earlier six consecutive full-suite runs of the same binary had passed.

## 4. Changes made (and not made)

* **Engine (`lifecycle.rs`, the Phase 3A path-based engine; not used by the production adapter):** (1) an entry that no longer exists when it is inspected is not "unexpected content" (`entry_state` → `None`); (2) a metadata error other than "not found" is re-queried a few times (the same read-only query, about 0.3 s in total) before the entry is given up as unknown — an entry that stays unreadable is still `other` and is **never deleted**; (3) after a successful removal of the uncommitted receipt the engine waits (bounded, about 1 s) until the name is really gone and reports honestly if it is not. No deletion rule was widened.
* **Tests:** none weakened or retried. A new regression test (`an_entry_that_vanished_between_the_listing_and_its_inspection_is_not_unexpected_content`) lists a folder 1 500 times against a concurrent create/delete thread and requires that no entry is ever called unknown. The extended failure messages stay.
* **After the fix:** one loop of 30 fresh-binary cycles on the same PC: **27 executed, 0 failures** (3 blocked by Smart App Control, not executed). With the earlier rate (12 %), 27 clean runs in a row would occur by chance about 3 % of the time: strong evidence, **not proof** that the failure is gone.

## 5. Residual risk, stated

* The production adapter (`adapter.rs` on `managed_fs`) does not share this code; it passed every full-suite run, but this investigation did not stress it the same way. The same Windows behaviour (a deleted file still listed) is a theoretical exposure of its cleanup checks; its handle-relative design and identity checks make a wrong *deletion* impossible, the exposure would be a spurious refusal.
* The cause of the lingering entry on the failing machine remains unproven (item 3). Should the failure reappear after this change, the message now carries the first-failure evidence needed to continue.
* A different reading of symptom B is possible: the receipts post-condition in the *test* observes the OS state without waiting. It was left unchanged on purpose (the engine now waits before reporting success), so that no test was made to tolerate the condition.
