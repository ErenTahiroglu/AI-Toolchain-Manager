# What changed in the gate pins of earlier phases (Phase 3D-4), and what did not

The acting adapter exists for tests and for explicit, non-default features only. Phase 3D-4 adds one more such feature, `jq-pilot`. Four earlier **source-text pins** named the feature set literally, so each had to be widened **in the same words**, deliberately and visibly, instead of being left to fail or silently relaxed. Nothing else in any earlier test changed.

| Pin (file → text) | Before | After |
| --- | --- | --- |
| `install_flow_tests.rs` `a_release_build_contains_no_acting_adapter…` (three `contains`) | `#[cfg(any(test, feature = "synthetic-pilot"))]` before `fn adapter` / `pub fn mvp_synthetic`, and the `#[cfg(not(any(test, feature = "synthetic-pilot")))]` twin | the same attributes with `feature = "jq-pilot"` added as the third member |
| `tests/test_phase3d2_contract.py` `test_the_acting_adapter_exists_only_for_tests_and_the_pilot_feature` | the same two strings | the same two strings, widened identically |
| `tests/test_phase3d3_contract.py` `test_the_acting_adapter_and_the_enablement_gate_exist_only_where_expected` | `with_pilot_gate`, `mvp_synthetic`, `fn adapter` strings | widened identically |

**Why this is not a weakening.** (1) The set of features that may compile an acting adapter is still *closed and exact*: `test`, `synthetic-pilot`, `jq-pilot` — a new test (`b2_the_pilot_module_has_no_environment…`) and `tests/test_phase3d4_contract.py` pin the new set literally, so adding a fourth would fail them. (2) The two pilot features cannot be combined (`compile_error!`), `jq-pilot` is a compile error off Windows, neither is a default feature, and CI's release build never names either (`tauri build` line scanned). (3) Real recipes are still refused unless listed in an enablement list that is **empty** in every build that exists today, including the `jq-pilot` build (`B3_UNLOCK = None`). (4) `Adapter::production` is untouched and still refuses everything; `check_authority_gated` is untouched. (5) The Phase 3D-0 guard (`test_production_lists_are_still_empty_and_no_reference_digest_was_added`) is **unchanged and passing**: the jq digest is a literal in no Rust source and no catalog seed; the pilot reads it from committed data files.

## Edits to existing code that are not tests

* `adapter.rs`: the two `cfg` attributes above (no logic change). `recipe.rs`: `RecipeRegistry::pilot` (tests and `jq-pilot` only; `RecipeRegistry::synthetic` still refuses a real recipe; `BUILT_IN_RECIPES_JSON` is still `[]`).
* `install_flow.rs`: `for_app` / `startup` get a `jq-pilot` arm; the two `adapter()` cfgs; `dead_code` allowances extended to `jq-pilot`; two new preview fields (`licenseScopeText`, `installedAsText`) that are part of the preview digest; one refusal text (`real_install_locked`).
* `lib.rs`: two `compile_error!` guards. `Cargo.toml`: the `jq-pilot = []` feature. `tools/run_staging.py`: an optional `--manifest` argument (default unchanged).
* `tools/verify_review_package.py`: its check *"commits after the candidate commit touch only `docs/phase-3d3b/`"* compared `candidate..HEAD`, which any later phase would break by design. It now compares `candidate..<the SHA the independent reviewer accepted>` (read from the B1 binding; `HEAD` when no binding exists). On the accepted SHA the result is unchanged (76/76); later commits are covered by the B2 contract and staging checks.
* CI: three new steps (Windows `jq-pilot` clippy+tests; macOS proves the refusal; B2 staging reproduction).
