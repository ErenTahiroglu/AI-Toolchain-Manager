# PRODUCT-05 Red Team repair — P05-RT-01: a pending refresh was treated as a finished read

Baseline (verified before any edit): `development` = `origin/development` = `3a8e17ffcca332180d80d62ac5f6401241548864`, clean tracked and untracked tree, one worktree; `main` = `68eda3f0407490fabccb517dcf822ab4818fc0bf`. Frontend only. Nothing under `src-tauri/`, `.github/`, `package*.json` or `docs/phase-3d*` changed; no IPC command, permission, migration, feature flag or dependency was added; no app, GUI, WebView2, jq, offline harness or installer was run or touched. The commit's own SHA and the exact-SHA CI result are **not** written here (a commit cannot contain its own hash, and CI cannot be known before the push); both are in the hand-off reply.

## The defect
`src/review.ts`: `phaseOf` looked only at `data` and `error`. `useLoad` keeps the previous good data while a new request runs, so a source in the state `{data: previousGoodData, error: null, loading: true}` was classified `"ready"`. With both sources in that state `complete` became `true`, and an earlier clean zero was shown again as the final statement "Bu ölçütlere göre incelenmesi gereken öğe yok" while a refresh was still in flight (a false zero on an unfinished read, which PRODUCT-05 forbids). `ReviewQueue` and `ReviewSummary` both use this derivation, so both were affected.

## Proof on the pinned baseline (tests written first, run against the unpatched code)
15 tests were added (7 pure, 8 through the real `App`). On `3a8e17f` **12 of them failed** and 3 passed (the control "only a finished clean pair says nothing to review", the phase test for already-correct states — `old` / `failed` / `loading` — and the Dashboard-vs-queue final-headline test). Failure evidence from the run:
* pure, projects pending with retained data: `AssertionError: expected 'ready' to be 'refreshing'` (received `"ready"`);
* pure, the 8 × 8 combination test: `complete` was `true` for a source with `loading: true` (expected `false`);
* App, clean queue + refresh held open (projects, inventory and both): the provisional headline was not found (`Unable to find an element with the text: /^En az 0 öğe incelenmeyi bekliyor \(okuma sürüyor; sonuç kesin değil\)\./`) because the page still showed the clean-zero statement;
* App, retained non-empty queue during a refresh, and both overlapping-request orders: same cause.
No timing or execution evidence beyond these assertion messages is claimed.

## The fix (minimal)
`src/review.ts`
* `ReadPhase` gains `"refreshing"`: data retained, no error, a read still running.
* `phaseOf` now receives `loading`: `data === null → (error ? "failed" : "loading")`, otherwise `error ? "old" : loading ? "refreshing" : "ready"`. A refresh that already failed stays `"old"` even while a retry runs; a first-read failure stays `"failed"`; a first read in progress stays `"loading"` — their wording and tests are unchanged.
* `complete` is unchanged in form (`projects === "ready" && evidence === "ready"`), so it now requires, for **both** sources, `data !== null`, `error === null` and `loading === false`.
* `reviewHeadline`: when not complete and a source is `refreshing`, the text is `En az N öğe incelenmeyi bekliyor (okuma sürüyor; sonuç kesin değil).` (the earlier `(liste eksik veya eski olabilir)` text is kept for every other incomplete state). A previously clean zero therefore reads "En az 0 … sonuç kesin değil", never "öğe yok". Retained items stay listed, marked by that headline and by the warnings below.
`src/components/ReviewQueue.tsx`: two warning lines — "Kararlar yeniden okunuyor; gösterilenler kesin değil, okuma bitince değişebilir." and "Kayıtlı envanter yeniden okunuyor; gözlemden doğan nedenler kesin değil." No retry button is offered while nothing has failed.
`src/components/ReviewSummary.tsx`: one line — "Okuma sürüyor; yukarıdaki sayı kesin değil." The tile and the queue still print the same `reviewHeadline` (tested for the final zero state).
Four-field identity, reason grouping, uncertainty, the navigation-only handoff, the clock handling (read once per answer, no polling) and the backend observation wording are untouched (the source diff is 3 files: 16 lines added, 3 removed).

## Tests
Added (existing tests are untouched: the test-file diffs are insertions only, no title was removed, renamed, skipped or turned into a snapshot):
* `src/review.test.ts` (+7): projects refreshing; inventory refreshing; both; retained non-empty data in each of the three combinations (count is "at least", marked not final); `old` / `failed` / `loading` keep their own phases (also `loading: true` on top of an error); **all 8 × 8 combinations** of `{data null/present, error none/set, loading true/false}` for the two sources: `complete` holds exactly when both are `data !== null && error === null && loading === false`, and no incomplete state ever prints "öğe yok"; the control.
* `src/ReviewQueue.test.tsx` (+8, through the real `App`, sidebar and Projects page; only `Date` is faked): a clean queue then a refresh held open with projects / inventory / both pending (provisional headline, warnings, no clean statement, no retry button, final again after it settles, no call after settling = no polling); retained non-empty queue during a refresh ("En az 6 … kesin değil", the 7 retained buttons still listed, replaced by the answer); overlapping reads with the **newer** one settling first (final at once, the late older answer changes nothing); overlapping reads with the **older** one settling first (ignored, queue stays provisional, the stale item never appears, final when the newer answers); a refresh that **fails** while retained data exists (`old`, retry offered, never clean, never "okuma sürüyor") and a successful retry makes it final; Dashboard and queue print the identical final headline.
Totals: 455 tests (440 + 15), 0 failed, 0 skipped.

## Mutation checks of the fix (one change at a time, SHA-256-verified restore)
R1 `phaseOf` ignores `loading` again → 12 failing; R2 a refreshing source counts as complete → 12; R3 provisional wording dropped → 10; R4 a retry on top of a failed refresh is called "refreshing" → 1 (pure test); R5 a retried first-read failure is called "loading" → 1 (pure test). All caught; no survivors identified. R4 and R5 are killed only by the pure test (the App fixtures do not retry on top of a failed refresh and then inspect the phase).

## Commands run locally (Windows)
| Command | Exit | Summary |
|---|---|---|
| `npm run typecheck` | 0 | no output |
| `npx vitest run` | 0 | 15 files, 455 passed, 0 failed, 0 skipped |
| `npm run build` | 0 | vite build ok |
| `python -X utf8 -m unittest discover -s tests` | 0 | 286 ran, OK, 58 skipped (unchanged; reasons not itemised) |
Not run: `cargo`, `tauri build`, the app, GUI, WebView2, CDP, jq, offline harness (the normal Rust jobs are covered by the repository's existing CI).

## Outstanding limitations (unchanged by this patch)
* The clock is read once per answer; a reminder that falls due while the page stays open appears at the next read.
* A status like `present_current` may rest on a computer-wide or `local-cli` name lookup; the contract cannot say which (backlog #1).
* The Dashboard tile has no refresh trigger of its own, so its `refreshing` state is covered by the shared pure function and the shared headline, not by a Dashboard-level interaction test.
* jsdom and mocked IPC are not evidence of a real window; the Turkish copy has had no editorial review.

## Next-phase recommendation
Do not widen this patch. After independent review, the next substantive value is `docs/product-05/NEXT_PRODUCT_BACKLOG.md` #1 (observation provenance in the contract, Rust, additive) — its own reviewed phase — or the human UX review in #6, which needs a person with the app open.
