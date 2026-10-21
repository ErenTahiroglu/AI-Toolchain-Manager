# Earlier tests that changed in Phase 3D-6, and why

Only tests that pinned the allowance to **inactive** (`B3_ARMED_DATA_DIR_SHA256 = None`, reason `pilot_not_armed`) changed. None was weakened: each now pins the *armed* state, which is stricter about the one authorized scope. No test was deleted, ignored or loosened.

| Test / check | Before | After | Why |
| --- | --- | --- | --- |
| `b2_the_unlock_is_one_constant…` | armed constant is `None` | armed constant is exactly `Some(158c279e…e8a5)` | the allowance is armed for the one authorized directory |
| `b2_a_default_build_cannot_install_jq_and_the_pilot_build_only_shows_it_locked` | jq-pilot build in a temporary directory: `pilot_not_armed` | `pilot_wrong_data_directory` (a temporary directory is not the armed one) | still refused before any preview |
| `b2_the_real_committed_supply_is_windows_only_unlocked_…` (renamed `…_unlocked_and_armed_for_one_directory_and_creates_nothing`; the B2 staging manifest follows the rename) | `armed_data_dir_sha256 == None` | `== Some(158c279e…e8a5)` | same supply, same "creates nothing, reads nothing" assertions |
| `b3_the_pilot_is_bound_to_…_inactive_until_armed` | unchanged: it builds its own supplies with explicit armed values | unchanged | independent of the constant |
| `b3_the_allowance_has_no_reset_path_…` | exactly one declaration `= None;` | exactly one declaration `= Some("158c279e…e8a5");` | the constant is declared once and used once |
| `b3_the_activation_candidate_is_unlocked_but_inactive_…` | real supply in a temporary directory: `pilot_not_armed` | `pilot_wrong_data_directory`; still **0** provider requests, no manager folder, no input folder, no allowance record | the candidate test now proves the *armed* supply refuses every other directory |
| `tests/test_phase3d5_contract.py` | armed constant is `None` | `None` or exactly the one authorized digest (the Phase 3D-6 contract requires the digest itself) | keeps the older contract valid for both states without letting any other digest through |
| `tools/gui_jq_pilot_locked.mjs` | reason code `pilot_not_armed` or `recipe_not_enabled`; reason text *"…etkinleştirilmedi"* | also reason code `pilot_wrong_data_directory` and its accurate text *"…başka bir veri klasörü için etkinleştirilmiş…"* | the driver runs in a temporary directory, which is not the armed one; the first armed run failed exactly this one text check (15/16) and the application was not changed |

The helper `expected_refusal_in_a_foreign_directory()` asserts the armed constant before it answers, so every test that uses it fails if the allowance is ever disarmed or armed for another digest.
