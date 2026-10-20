# What changed in earlier tests (Phase 3D-5), and what did not

Only tests **written in Phase 3D-4 (B2)** changed, and only where the new one-installation rule makes their old expectation false. **No test of Phases 0–3D-3B changed.** `tests/test_phase3d2_contract.py` / `test_phase3d3_contract.py` and the Rust gate pins are untouched in this phase.

| B2 test (`install_flow_b2_tests.rs`) | Old expectation | New expectation (why) |
| --- | --- | --- |
| helpers `b2_lab`, `b2_restart`, rollback loop, unlocked-real-spec test | a pilot supply is usable as soon as it is unlocked | the supply is **armed for the lab's data directory** first (`lab_armed`); an unarmed pilot is inactive by design (tested) |
| `closed()` | registry/references/gate/notices/evidence empty | additionally `one_time` is `None` |
| `b2_the_whole_pilot_path…` | uninstall, then **reinstall and remove again** | after the removal a second installation is **refused** (`INSTALL_PILOT_ALLOWANCE_CONSUMED`), state *Removed, can_install = false*; one `install passed` row instead of two |
| `b2_audit_write_failure…` | install/reconcile/remove, then a refused-pre-row install attempt | the **pre-installation audit failure** is exercised first (and shown not to touch the allowance); at the end a second install is refused |
| `b2_unexpected_files_links_and_changed_identity…` | ownership-conflict install attempt **after** a removal | the conflict is exercised **first** (it fails closed, touches nothing of theirs and — having changed nothing — does not spend the allowance), then the install proceeds; at the end the removal does not reopen installation |

**Activation-candidate commit** (`B3_UNLOCK` becomes the reviewed recipe digest): `b2_the_unlock_is_one_constant…` now asserts the constant names exactly that digest (and that arming is `None`); the module-scan test matches the new declaration; `b2_the_real_committed_supply_…` expects gate 1 with the one-time rule present and unarmed, and was renamed (`…unlocked_for_the_candidate_but_inactive…`); in the jq-pilot configuration `b2_a_default_build_cannot_install_jq…` expects reason `pilot_not_armed` instead of `recipe_not_enabled`; `tests/test_phase3d4_contract.py` accepts `None` **or exactly the reviewed digest** (no other value) for `B3_UNLOCK`; `tools/gui_jq_pilot_locked.mjs` additionally checks the reason code. The B2 staging manifest follows the one rename.

Unchanged on purpose: the clean retry after an injected failure (`b2_a_failure_at_any_point…`) still installs, because a rolled-back attempt that is read back as *not installed* releases the allowance (audited first) — the test now also exercises that release path.

New in this phase: `pilot_allowance.rs` unit tests (6), `install_flow_b3_tests.rs` (10), `tests/test_phase3d5_contract.py`, `docs/phase-3d5/b3-staging-manifest.json` and one CI step. `tools/run_staging.py` is unchanged.
