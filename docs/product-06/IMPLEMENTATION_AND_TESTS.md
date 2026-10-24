# PRODUCT-06 — implementation, tests, evidence and limits

The contract and the wording are in `EVIDENCE_ORIGIN_CONTRACT.md`. This file lists what changed, what was run and what was **not**. The commit's own SHA and the exact-SHA CI result are not written here (a commit cannot contain its own hash and CI is unknown before the push); both are in the hand-off reply. Nothing here is an independent acceptance claim.

## What changed
| Area | Files |
|---|---|
| Rust (additive) | `src-tauri/src/discovery/store.rs` (`ProjectRelation`, `SurfaceRelation`, `EvidenceOrigin`, `origin_of`, `RequestVsObservation.origin`, a 4th member of `classify_request`'s result); `tests.rs` (+2 lines registering the new module) |
| Rust tests (new) | `src-tauri/src/discovery/tests_origin.rs` — 12 tests |
| Frontend | `src/types.ts` (`EvidenceOrigin`, `origin` on `RequestVsObservation`); new `src/origin.ts` (validation + copy), `src/components/EvidenceOriginNote.tsx`; `ObservationSection.tsx` (+1 element), `ReviewQueue.tsx` (+1 element, +1 import), `review.ts` (the item carries the origin; Deliverable 0 headline) |
| Frontend tests | new `src/origin.test.ts` (7), new `src/Origin.test.tsx` (24); P05-RT-02 tests added to `review.test.ts` (+1) and `ReviewQueue.test.tsx` (+1); fixtures: `test/backend.ts` (`originOf`, `makeRequest` carries an origin), `Inventory.test.tsx` (three literal rows got `origin`) |
| Docs | `docs/product-06/*`; `docs/product-05/BEHAVIOR_AND_SCOPE.md` (three table rows reworded to the new headline) |

No change under `.github/`, `package*.json`, `docs/phase-3d*`, no migration, no permission, no IPC command, no dependency, no feature flag, no new scanner, process, network request or write.

## Tests
**Rust** (`discovery::tests::origin`, all synthetic): the four project × surface combinations; a `local-cli` request on a `local-cli` row is `exact_surface`; several simultaneous candidates (rank beats specificity, equal rank → higher id wins whichever kind it is, a current answer beats an old project-specific one, rows of another platform/project/surface never take part) with the metadata describing the **winner**; `not_scanned` / `platform_mismatch` / other-project-only rows → `origin` and `basis` both `None`; a historical winner keeps its `previously_*` status and its own time (offset kept as stored); malformed (`yesterday`, empty, date only, no offset) and future timestamps → `null` time / `null` age, status unchanged; the winner's time and age (not the loser's); serialization (exact member set = the nine earlier members + `origin`, wire values, explicit `null`s); a response built from a row full of sentinels (path, evidence, version, fingerprint, limitation) contains none of them; an end-to-end test through the real store with a synthetic temp environment (`agent-browser` via PATH, a user-level plugin) — two projects asking for the same tool both get `computer_wide`; a differential test over 8 000 deterministic cases against a verbatim copy of the PRODUCT-05 selection (status, note, selected scan identical; origin = the selected row's); a source test that the origin/selection code has no I/O, process, network or database access.
**Frontend:** pure (`origin.test.ts`): known/unknown relations, timestamp and age validation, extra wire members not carried. Through the real Projects page (`Origin.test.tsx`; matrix **and** queue): all four combinations (sentences, absence of the opposite sentence, same text in both places), time/age shown/omitted/never invented, null / unknown / missing / older-backend origin, `not_scanned` / `platform_mismatch`, a historical result, two rows for one key (no origin shown), no row, unrecognised status, not-accepted wish, two projects each getting only their own row, wrong platform / wrong surface rows not borrowed, opening a queue item calls only the navigation callback (and no write/scan/install/evaluate command ever), the new modules contain no IPC/timer/storage/network, no positive-claim words, extra wire fields (path, raw evidence, version, fingerprint) never reach the screen. P05-RT-02: a pure test and a through-App test with six cached items and a deferred answer with five (no "En az", no exact-count sentence during the read; exactly "5 öğe incelenmeyi bekliyor." after).
Existing assertions were not weakened. Changed: the headline strings in existing assertions (same conditions, new words); `review.test.ts` one `toEqual` gained the `origin` member; two existing test **titles** that said "at least N" were reworded because they became false; fixtures got the new member.

## Commands run locally (Windows, this machine)
| Command | Exit | Result |
|---|---|---|
| `npm run typecheck` | 0 | no output |
| `npx vitest run` | 0 | 17 files, 488 passed, 0 failed, 0 skipped (455 + 33) |
| `npm run build` | 0 | vite build ok |
| `python -X utf8 -m unittest discover -s tests` | 0 | 286 ran, OK, 58 skipped (unchanged; reasons not itemised) |
| `cargo fmt --check` | 0 | |
| `cargo clippy --all-targets -j 2 -- -D warnings` | 0 | |
| `cargo test -j 2` (lib) | — | 714 passed, 0 failed, 2 ignored (704 + 12 new) |
| `cargo test -j 2` (bin `ai-toolchain-manager`, `main.rs`) | 101 | **BLOCKED / NOT RUN**: Windows Smart App Control refused the freshly built binary (`Uygulama Denetimi ilkesi bu dosyayı engelledi`, os error 4551). That target contains 0 tests (an earlier run in this session printed `running 0 tests`). The identical binary was re-run once with the same result; it was not rebuilt to get around the block and no setting was touched. The normal CI jobs build and run it on their own machines. |
Memory was checked before the Rust builds (≈3.5 GB free of 16 GB); builds and tests ran with `-j 2`, no parallel test runners, no retries under pressure.

## Mutation checks (one change at a time, SHA-256-verified restore)
**Frontend (15, all caught):** project text swapped; surface text swapped; unknown relation guessed; any string accepted as a timestamp; age kept without a valid timestamp; negative age accepted; queue shows no origin; matrix shows no origin; review item drops the origin; "no selected observation" sentence always shown; ambiguous rows show the first; a raw field leaks to the screen; copy turned into a positive claim; age described as live; fallback without naming `local-cli`. Thin spots: "age kept without a valid timestamp" is killed only by the pure test; "no selected observation sentence always shown" and "raw field leaks" each by a single through-the-page test.
**Rust (15 run, 13 caught, 2 BLOCKED / NOT RUN):** caught — project relation inverted; local-cli row always labelled exact; surface always exact; project always project-specific; origin of the first candidate instead of the winner (current and historical); timestamp passed unchecked; negative age accepted; lowest id wins ties; another project's rows count; wire name changed; `platform_mismatch` gets a guessed origin; an old `present` outranks a current `absent`. **Not run:** "age clamped to 0" and "project-specific wins before presence rank" — their freshly built test binaries were blocked by Smart App Control (os error 4551) and, as above, were not rebuilt to evade it. Neighbouring mutants (negative age accepted; tie-break and relevance changes; current/old ranking) cover the same lines, but these two specific mutants are **unverified**.

## Side-effect and privacy statements
No app, GUI, WebView2, CDP/9222, jq, `--features jq-pilot`, offline-preflight harness, installer, download, agent-configuration write, real-tool launch or network/process probe was started. The ignored `src-tauri/target/offline-preflight/`, the Rev 5 ZIP, the armed pilot directory and the real WebView2 profile were neither read nor written. The Rust end-to-end test uses a temporary directory with a text-file stand-in named like a program (content "not a real program", never executed) and synthetic config files only. The response/screen carry no path, location, evidence text, limitation, fingerprint or version. The only new copy of an observation's own data is its relations, its RFC 3339 timestamp and an integer age.

## Known limitations and test gaps
* The contract says **how** the selected row matched, not **whether the selection is right**. The existing precedence is untouched and now visible: a computer-wide `present` beats a project-specific `absent` (backlog #1). A reader who sees "computer-wide" next to "present" is told, truthfully, that it is not project evidence; the status word itself is unchanged.
* `age_hours` is the age when the inventory was read, not now; the screen says so.
* A project-specific row proves only that the backend recorded the project alias for it; there is no end-to-end Rust test that produces one through a real project scan (the unit tests construct it directly).
* Two Rust mutants and one bin test target are BLOCKED / NOT RUN on this machine (above); only CI covers them.
* jsdom and mocked IPC are not evidence of a real window; the Turkish copy (the origin sentences are long) has had no editorial or screen-reader review.
* The Dashboard tile has no refresh trigger, so its `refreshing` headline is covered through the shared pure function and formatter.
* The Inventory page does not show the origin (backlog #2).
