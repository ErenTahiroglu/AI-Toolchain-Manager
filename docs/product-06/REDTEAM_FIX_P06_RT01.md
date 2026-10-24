# PRODUCT-06 Red Team repair — P06-RT-01: a sub-hour future observation read as "0 hours ago"

Baseline (verified before any edit): `development` = `origin/development` = `566f73d621c5db4b6ed00175ae80d3a62d59547c`, `origin/main` = `68eda3f0407490fabccb517dcf822ab4818fc0bf` (untouched), clean tracked/untracked tree, one worktree, no stash. Rust only (plus tests and this note). Nothing under `.github/`, `package*.json`, `docs/phase-3d*` or any migration changed; no IPC command, permission, feature, dependency, filesystem/network operation or agent write was added. No app, GUI, WebView2, CDP, jq, offline harness or installer was run. The commit's own SHA and the exact-SHA CI result are not written here; both are in the hand-off reply.

## The defect (pre-fix path)
1. `store.rs::age_hours(observed_at)` parsed the stamp and returned `(Utc::now() - t).num_hours()`. `num_hours()` truncates toward zero, so a stamp 1–59 minutes **ahead** gave `0` (and 1 h 1 min ahead gave `-1`).
2. `latest_observations_with()` stored it as `StoredObservation.age_hours = age.min(i32::MAX)` and `stale = age >= STALE_AFTER_HOURS`.
3. `classify_request` → `origin_of(o, surface)` emitted `age_hours: (valid && o.age_hours >= 0).then_some(o.age_hours)`. `0` passes `>= 0`, so the serialized `RequestVsObservation.origin.ageHours` was `0` for an observation from the future. The documented contract says it must be `null`.
4. The earlier far-future test set `age_hours = -9000` by hand, so it never exercised step 1.

Other consumers of `age_hours`: `stale` (`age >= 24`: a future stamp is not stale before and after — unchanged), the Inventory page (`Eski (N saat)` only when `stale`, so it never shows a future stamp's value), and `tests.rs` (`age_hours < 1`, `>= 47` for fresh and 48-hour-old rows — unchanged). An unreadable stamp still returns `i64::MAX` (stale, "not current"), exactly as before. Status ranking, relevance filtering and winner selection were not touched.

## The repair (one function)
`age_hours()` now delegates to a pure `age_hours_at(observed_at, now)`: the ordering against the reference instant is decided **before** truncating. An instant after `now` returns `elapsed.num_hours().min(-1)` — at most `-1`, so `origin_of`'s existing `>= 0` test and every other `age >= …` test treat it as "no age". Equal and past instants are unchanged (`num_hours()`), unreadable stamps are unchanged (`i64::MAX`). `origin_of` and the frontend/wire contract are unchanged: `observedAt` stays (it is valid), `ageHours` is `null`. The only observable change anywhere is the stored `age_hours` of a stamp **under one hour in the future**: `0` → `-1`.

## Tests (red before, green after)
Written first, run against the unfixed baseline code:
* `an_observation_from_the_near_future_has_no_age_through_the_real_computation` — **FAILED** on the baseline: a real scan is stored, `observed_at` is set to now+15 min, the inventory is read through `store::inventory`; `assertion left == right failed: 15 min ahead: it must not say '0 hours ago'  left: Some(0)  right: None`. (Margins of 15 and 30 minutes against a test that takes seconds: no sub-second clock edge.)
* `past_observations_keep_their_whole_hour_age_and_invalid_ones_none` — **passed** on the baseline and after (the preserved behaviour: 30 min ago → 0, 2 h → 2, 49 h → 49 with `previously_present`, 50.5 h → 50, four kinds of unreadable stamp → `null`/`null`).

Added with the repair (pure, injected reference instant, no clock): `age_at_a_given_instant_orders_before_it_truncates` (equal, past under/at an hour, older past, offset forms, one second / 15 min / 30 min / 59:59 / exactly 1 h / 2 h 30 / far future ahead, unreadable stamps); `only_the_sub_hour_future_case_differs_from_the_previous_computation` (a 7-minute sweep over eight days against a copy of the previous formula: identical everywhere except the under-an-hour-future case, which goes `0` → `-1`); `the_winner_decides_whether_an_age_is_shown_not_a_competing_row` (computer-wide vs project-specific competitors in both winner orders, a historical winner, and status/note/basis independent of the stamp). The 12 earlier origin tests, including the 8 000-case selection differential, pass unchanged; no assertion was deleted, weakened or skipped.

## Mutation checks of the repair
Y1 `min(-1)` dropped (the defect again) → 4 failing; Y2 equal instants treated as future → 1 failing (**only** the pure test: a real clock cannot produce an exact tie); Y3 every future stamp reads `0` → 4 failing. All caught; restore SHA-256-verified.

## Commands run locally (Windows)
| Command | Exit | Result |
|---|---|---|
| `cargo fmt --check` | 0 | |
| `cargo clippy --all-targets -j 2 -- -D warnings` | 0 | (one `unnecessary_cast` in a new test was found and removed first) |
| `cargo test -j 2 --lib` | 0 | 719 passed, 0 failed, 2 ignored (714 + 5 new) |
| `cargo test` bin target (`main.rs`, 0 tests) | NOT RUN | blocked by Smart App Control in the PRODUCT-06 run; not attempted again, nothing renamed, copied or bypassed |
| `npm run typecheck` / `npx vitest run` / `npm run build` | 0 / 0 / 0 | 17 files, 488 passed, 0 skipped (frontend unchanged) |
| `python -X utf8 -m unittest discover -s tests` | 0 | 286 ran, OK, 58 skipped (unchanged) |

## Limitations
* A stamp exactly equal to the reference instant or a few seconds ahead is "future" only by comparison with the read-time clock; a scan whose own clock is slightly ahead of the reading clock will show no age (fail-closed by design).
* Clock skew between machines is not modelled.
* The Rust bin test target is not run locally; CI is separate, complementary evidence.
