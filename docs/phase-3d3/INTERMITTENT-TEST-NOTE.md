# An intermittent failure seen once in Phase 3D-3 (not explained, not hidden)

**Observed.** The first full `cargo test` run of the final test binary reported two failures out of 655 library tests, both in code that works on real files of the manager's folder:

* `adapter::tests::a_crash_at_any_point_of_an_install_is_recovered_to_a_consistent_state_or_reported_honestly` — `Refused("fs_not_found")` from a re-installation right after a recovery (`adapter_tests.rs`, the loop over crash points);
* `install_flow::tests::an_installed_tool_is_still_installed_and_verified_after_a_restart` — `recover()` returned a note ("left untouched") where none was expected.

**Reproduction.** Not reproducible: the same binary passed the two tests 6 times in a row when run alone, and then **5 further full runs** (655 passed each, 48–67 s). Both failures happened on the **first run after the binary was built**, under the full parallel load of the suite — the same pattern as the Phase 3D-1 investigation ([FLAKY-LIFECYCLE-TEST](../phase-3d1/FLAKY-LIFECYCLE-TEST.md): a Windows file that is being deleted stays visible in a directory listing and answers "not found" / "access denied" for a moment; a freshly built executable and its temporary files are a plausible reason for the delay, but **no program was proven to hold the handle** and antivirus is neither proven nor excluded).

**What is established and what is not.** Established: neither assertion was weakened, neither test was retried inside the code, and no failure was seen in the 11 later executions. Not established: the cause. The Phase 3D-1 note listed as a residual risk that the handle-relative adapter (used by the application workflow) "was not stress-tested the same way"; this observation is **new evidence that the adapter path can see the same transient state under load**. Consequences for the application are fail-closed: an install that meets the transient state is refused or rolled back with a report (`fs_not_found` → rollback), a recovery leaves the entry alone and says so; nothing is deleted on a guess.

**Not done here.** No engine change for the adapter was made in this phase (it would be a change to the file-system layer that this phase does not review), and no loop of 80+ fresh-binary runs like the 3D-1 one was run. Should the failure repeat, the next step is that same instrumented loop against `adapter.rs`.
