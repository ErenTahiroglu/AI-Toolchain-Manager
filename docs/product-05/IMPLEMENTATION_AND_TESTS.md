# PRODUCT-05 — implementation and tests

Baseline: `development` = `origin/development` = `b5ef0876cad6f91315dde49efcca30bf9cfa40f1` (PRODUCT-04), clean tracked and untracked tree, one worktree, one branch. `main` = `68eda3f0407490fabccb517dcf822ab4818fc0bf`, not touched. Frontend only: nothing under `src-tauri/`, `.github/`, `package*.json`, `docs/phase-3d*` or the offline-preflight folder changed; no IPC command, dependency, migration, capability or permission was added; no jq, pilot directory, WebView2 profile, GUI, scan or install was touched or run.
The code commit's own SHA and the exact-SHA CI result are **not** written here (a commit cannot contain its own hash and a second docs-only commit just to record CI was ruled out); both are in the hand-off report.

## Before → after
| | Before (PRODUCT-04) | After (PRODUCT-05) |
|---|---|---|
| Finding decisions that need a look | open every project / target / tool in the matrix | one list, grouped by project, filterable by project and reason, on the Projects page; a summary on the Dashboard |
| Reads | — | **none new on the Projects page** (the queue consumes the `listProjects` / `listInventory` loads the page already has); the Dashboard summary adds `listProjects` + `listInventory` (the same stored reads) |
| Actions | — | "Matriste incele" only (typed `MatrixHandoff`); the matrix keeps all decision controls |
| Backend / schema / capabilities / dependencies | — | unchanged |

## Changed files
| File | Change |
|---|---|
| `src/review.ts` (new, pure) | `deriveReview(projects, inventory, now)` → `ReviewQueue` (`items`, `uncertain`, phases, `complete`, `evidenceUnavailableFor`), `parseRfc3339`, `reviewKey`, `filterItems`, `reviewHeadline`, `EVIDENCE_REVIEW_STATUSES`. Imports `observationOfWish`, `normalize`, `TARGET_*` from existing modules. |
| `src/observation.ts` | `observationFor` now delegates to a new `observationOfWish(wish, entryId, target, inventory)` — same logic, same results, so the matrix row and the queue use **one** engine. |
| `src/components/ReviewQueue.tsx` (new) | The list, headline, warnings, filters, uncertain section, items and the handoff button. No `api` use. |
| `src/components/ReviewSummary.tsx` (new) | Dashboard section: two `useLoad` reads (`listProjects`, `listInventory`), the same derivation and headline, a link. |
| `src/pages/Projects.tsx`, `src/App.tsx`, `src/pages/Dashboard.tsx`, `src/styles.css` | Wiring: `ProjectsPage` takes the optional `onOpenInMatrix` (App's existing `openInMatrix`) and renders the queue only when it is given; Dashboard shows the summary; `.rq-*` styles. |
| `src/review.test.ts` (new, 49 tests), `src/ReviewQueue.test.tsx` (new, 32 tests) | See below. |
| `src/Recommendations.test.tsx` | Two allow-list tests re-expressed (same titles): `list_inventory` now also comes from the Dashboard tile, which has finished before the Recommendations page issues its own first command; the page's own command list is asserted from its first read on. |
| `docs/product-05/*` (3 files), `README.md` | Documentation; README feature row mentions the review queue. |

## Tests (frontend, mocked typed IPC, jsdom; only `Date` is faked, to a fixed instant)
`src/review.test.ts` (pure, the clock is a parameter): every reason alone; each of the seven review statuses; `present_current` as information only (alone: no item, next to a reason: carried, never a reason); several reasons on one key = **one** item in a fixed order; a removed entry never gets an evidence reason; non-accepted wishes never compared; the same tool in three projects × two systems × four surfaces = 8 keys with no evidence crossing keys; distinct tools with one display name; duplicate record for one key; `reviewKey` separates all four fields; deterministic order independent of input order; deep-frozen inputs (immutability) and identical output on a second derivation; reminder: exact boundary `now == remindAt`, ±1 s / ±1 ms, future, past, offsets (`+02:00`, `-05:00`, `+05:30`), fractions, 11 malformed forms (empty, free text, date only, no zone, Feb 30, month 13, hour 25, minute 60, space instead of `T`, bad offset, epoch digits) → never due and **listed as uncertain**; postponed without reminder; reminder on a non-postponed decision; strict-parser values; missing / ambiguous / unrecognised rows; inventory loading / failed / old; decisions loading / failed / old; headline wording per state (the "nothing to review" text only when both reads are clean); unsupported system or surface → listed, not openable; filters; example projects marked.
`src/ReviewQueue.test.tsx` (through the real `App` and sidebar):
* **what is listed**: 6 items + 1 uncertain-only decision from a fixture of two projects; group headings; two reasons on one key = one item; due-exactly-now reminder listed, future reminder not; present_current alone not listed; removed tool with a wish listed; same-named tools distinguishable by id in text and in the button name; filters and the "x / y" line; the uncertain section and its wording;
* **no false zero**: decisions failing (error + retry that works) and pending; inventory pending (with and without decision-based items — never "nothing to review"); inventory failing (states 5 unknown-evidence wishes, retry works); failed refresh after success (decisions and evidence) keeps the list and marks it old; the Dashboard tile in each state;
* **navigation**: exact project / system / surface / tool row focused and marked; no set / reset / evaluate / scan / install call from it; the saved decision shown as saved and not changed; keyboard Enter; three different items each land on their own target; removed tool opens reset-only and nothing is reset; removed tool without a wish absent; unsupported target → note, no button; vanished project leaves with its items after a refresh; sidebar navigation clears the handoff;
* **viewing changes nothing**: opening every item leaves count and list identical, a real saved decision in the matrix lowers it; source scan for acknowledge / dismiss / storage / timers / write-or-scan API names, and exact `api.*` use per file;
* **privacy / safety**: planted project note, path, evidence text and version never appear in the queue or tile; hostile names are plain text; a whole session issues only `get_app_info`, `get_settings`, `get_dashboard`, `list_projects`, `list_catalog`, `list_inventory`; 20 forbidden commands never; no `fetch`, `window.open`; no call after the session settles (no polling); exact read counts;
* **regression**: PRODUCT-04 observation block beside the opened row; matrix save through its own button; project creation; the other pages reachable.

### Migrated tests
Title comparison against `HEAD`: **no** test title removed or renamed. `Recommendations.test.tsx` has two assertions re-expressed under the same titles (see the table).

## Mutation checks (one change at a time, SHA-256-verified restore)
33 mutants; all made the suite fail; **no survivors, no equivalent mutants identified**. Counts are failing tests (the suite had 439 tests, 440 once the example-project test existed).
| # | Mutation | Failing |
|---|---|---|
| N1 / N2 | `reviewKey` drops the surface / the project | 10 / 20 |
| N3 / N4 | evidence join ignores the platform / the project | 32 / 31 |
| N5 | a failed decision refresh empties the list | 1 (pure test only) |
| N6 | an inventory error clears the evidence reasons | 2 |
| N7 | duplicates counted twice | 2 (pure tests only) |
| N8 / N9 | due date: boundary excluded / inverted | 18 / 21 |
| N10 / N11 | time-zone offset ignored / impossible day accepted | 1 / 2 (pure tests only) |
| N12 / N13 | reminder for any decision / malformed reminder silently dropped | 1 / 11 |
| N14 | `present_current` becomes a reason | 21 |
| N15 | `present_current` shown as "Doğrulandı" | 1 |
| N16 | non-accepted wishes compared with evidence | 33 |
| N17 / N18 | removed reason / material-change reason lost | 20 / 29 |
| N19 | uncertain items silently dropped | 20 |
| N20 / N21 | an unfinished or failed read counted as complete / headline gives an exact number while incomplete | 9 / 8 |
| N22 | unsupported target gets a button | 2 |
| N23 / N24 / N25 / N26 | navigation: wrong platform / wrong surface / other project / tool id dropped | 3 / 4 / 3 / 6 |
| N27 / N28 | opening an item saves "accepted" / starts a scan | 6 / 3 |
| N29 | opening an item removes it from the list (an implicit "mark seen") | 4 |
| N30 / N31 | project filter ignored / order depends on input order | 2 / 2 |
| N32 | Dashboard tile never reads the inventory | 5 |
| N33 | example project not marked | 2 |
Thin spots, stated honestly: N5, N7, N10, N11 and N12 are killed only by the pure tests (no UI fixture contains a duplicate key, an offset time, an impossible day or a non-postponed reminder). The "old" state of the *decisions* read is therefore asserted in the UI only as a visible warning, and the retained list only in the pure test.
Script: scratchpad `mutate5.py` (not committed). The first six mutants of the batch were run with an earlier version of the script that did not handle CRLF line endings in multi-line patterns; their patterns were single-line and valid, the others were run with the fixed script.

## Commands run locally (Windows, working tree before the commit)
| Command | Exit | Summary |
|---|---|---|
| `npm run typecheck` | 0 | `tsc --noEmit`, no output |
| `npx vitest run` | 0 | 15 files, **440 passed**, 0 failed, 0 skipped (PRODUCT-04: 359; +49 +32) |
| `npm run build` | 0 | vite build ok |
| `python -X utf8 -m unittest discover -s tests` | 0 | 286 ran, OK, 58 skipped (unchanged; reasons not itemised here) |
Diff scope check (`git diff --name-only`, untracked included): no path under `src-tauri/`, `.github/`, `docs/phase-3d*`, `package*.json` or the offline-preflight folder. `.skip` / `.only` / `.todo` search in `src/`: none.

## Not run / unsupported evidence
`cargo` (Rust untouched; CI runs it), `tauri build`, the real app, any GUI, WebView2, CDP port 9222, jq, the offline harness, Windows Sandbox/VM, macOS GUI. Mocked IPC and jsdom are **not** evidence of a real window: no layout, scrolling, focus ring, dark-theme contrast, screen-reader announcement or real database content is proven. Fake `Date` only fixes the instant used by the reminder tests.

## Known issues / risk notes
* Evidence reasons are only as strong as `classify_request`; `present_current` / `absent_current` may rest on a computer-wide or `local-cli` lookup (the contract cannot say which). Nothing here upgrades that.
* The clock is read once per answer; a reminder that becomes due while the page stays open shows up at the next read (no timer by design).
* The queue is rendered only when `ProjectsPage` receives `onOpenInMatrix` (App always passes it). Standalone `ProjectsPage` tests therefore do not render it; the queue is tested through `App`.
* Counts include decisions of example (synthetic) projects, marked "örnek veri" in the queue but not separated in the Dashboard number.
* The Turkish copy has had no editorial review. The Dashboard section is one more pair of stored reads at Dashboard load.
* The matrix intro, the queue intro and the observation block repeat some caveats; wording consolidation is a copy-review task.
