# What changed in the old "nothing is reachable" guards (Phase 3D-2), and what did not

Phase 3C/3D-1 pinned, by source-scan tests, that **no command, no start-up code and no screen could reach the installation engine**. Phase 3D-2 deliberately connects the application workflow to the handle-relative adapter (product decision, Issue #21). That is a change of *boundary*, not a weakening: every old test still exists, is byte-identical and passes; each old guard is complemented by a stricter test about the new boundary.

| Old guard (unchanged, still passing) | Still true because | New guard that covers the new boundary |
| --- | --- | --- |
| `lifecycle_tests.rs` `nothing_connects_the_synthetic_engine_to_a_command_a_permission_or_the_user_interface` | the **path-based synthetic engine** (`lifecycle.rs`) is still unreachable; the workflow uses only the handle-relative adapter | `install_flow_tests.rs` `the_webview_reaches_the_installation_engine_only_through_install_flow_and_never_with_a_path_a_url_or_bytes` |
| `managed_root_tests.rs` `no_command_or_start_up_code_can_name_a_writable_root` | `commands.rs`, `lib.rs` and `state.rs` still contain none of `managed_root`, `managed_fs`, `AppDataAnchor`, `ManagedRoot`, `adapter::` — they mention only `install_flow::` | same new test (it also forbids `Adapter`, `device_trust`, `lifecycle::`, `os_approval`, `os_hello`, `approval_store`, `consent::`, `ConsentGrant`) |
| `approval_store_tests.rs` `the_store_is_reachable_from_no_command_and_the_application_never_creates_one` | the durable approval ledger / Windows Hello path is still unreachable | `install_flow_tests.rs` `the_workflow_modules_never_touch_windows_hello_the_approval_ledger_or_a_key_store` (also covers `install_flow.rs`, `consent.rs`, `pilot_supply.rs`, `commands.rs`) |
| `tests/test_phase3c_contract.py` `test_no_command_or_start_up_code_can_name_a_root_or_reach_the_adapter`; `…acting_adapter_constructor_exists_in_test_builds_only_and_production_is_closed` | `test_mode` is still `#[cfg(test)]` only (count of `mode: Mode::TestSynthetic` is 1); `production_adapter_not_enabled` is still present | `tests/test_phase3d2_contract.py` pins the new gate text exactly (`#[cfg(any(test, feature = "synthetic-pilot"))]` before `mvp_synthetic` and before the service's acting `adapter()`; the `#[cfg(not(…))]` twin returns `production_adapter_not_enabled`) |
| `health.rs` `shipped_manifest_is_exactly_the_allowlist` | grant list and allow-list are still identical | the six new commands were added **to both lists together**, as the rule in `health.rs` requires: `allow-list-install-states`, `allow-prepare-install`, `allow-cancel-install`, `allow-confirm-install`, `allow-get-installed-details`, `allow-recover-installations` |

## Edits to existing code that are not tests

* `audit.rs`: `NewEvent` gained an optional `EventContext` (recipe id, tool version, approval reference, approved recipe digest, artifact digest, evidence digest) so `install`/`approve` rows satisfy the Phase 0 audit schema; plain events are unchanged.
* `approval.rs`: a second accepted actor kind, `in_app_confirmation`, constructible only from a `ConsentGrant` (`pub(crate)` constructor used in `consent.rs`); `ApprovalPolicy::production()` still accepts **no** actor (tested), `synthetic_only()` is unchanged.
* `adapter.rs`: `trust` became optional (`None` only in the in-app workflow); a consent path (`*_with_consent`) next to the device-approval path; read-only `inspect` and `history`. `Adapter::production` is untouched.
* `lifecycle.rs`: `check_artifact` delegates to `check_artifact_for` (same checks, no approval needed for a preview).

## Honest statement

The application can now, in a build made with `--features synthetic-pilot`, **install and remove synthetic tools in its own folder** through its real window. No release build can install anything, and no real tool can pass the recipe/registry/adapter gates in any build.
