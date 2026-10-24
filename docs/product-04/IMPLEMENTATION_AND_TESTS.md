# PRODUCT-04 — implementation and tests

Baseline: `development` = `origin/development` = `840bfd7f261127acf96e976fde51176370f40880` (PRODUCT-03), clean tree, one worktree. `main` = `68eda3f0407490fabccb517dcf822ab4818fc0bf`, not touched. Frontend only: nothing under `src-tauri/`, `.github/`, `package*.json`, `docs/phase-3d*` or the offline-preflight folder changed; no IPC command, dependency, migration, capability or permission was added; no jq, pilot directory, WebView2 profile, GUI, scan or install was touched or run.
The code commit's own SHA and the exact-SHA CI result are **not** written here (a commit cannot contain its own hash, and a second docs-only commit just to record CI was ruled out); both are in the hand-off report.

## Changed files
| File | Change |
|---|---|
| `src/observation.ts` (new, pure) | `requestMatches` (exact four-field key), `observationFor(row, target, inventoryRead)` → `ObservationView` (`loading`, `unreadable`, `removed`, `no_wish`, `no_row`, `ambiguous`, `unrecognised`, `observed`), `REQUEST_STATUSES`, `isKnownRequestStatus`. Imports types only. |
| `src/components/ObservationSection.tsx` (new) | Renders one `ObservationView` as the "Son kayıtlı envanter gözlemi" section. No `api` use, no control, no link. |
| `src/components/ProjectToolWorkspace.tsx` | New required prop `inventory: LoadState<Inventory>`; passes `observationFor(...)` to each row; inventory error notice with **Yeniden dene** and an "old" warning; *Listeyi yenile* also re-reads the inventory; a save/reset re-reads projects **and** inventory. The inventory is **not** part of `waiting`, so a slow or failed inventory never delays the Recommendations handoff focus. The component still uses only `setProjectPreference` / `resetProjectPreference`. |
| `src/pages/Projects.tsx` | `useLoad(api.listInventory)` (the one new typed read) passed to the workspace. |
| `src/labels.ts`, `src/pages/Inventory.tsx` | `permissionText` moved to `labels.ts` and imported by the Inventory page (same text; shared by the new section). |
| `src/styles.css` | `.ws-observe` rules. |
| `src/test/backend.ts` | `scanRef`, `REQUEST_NOTES` (the Rust classifier's notes), `makeRequest` fixtures. |
| `src/observation.test.ts` (new, 31 tests), `src/Observation.test.tsx` (new, 48 tests) | See below. |
| `src/Workspace.test.tsx`, `Handoff.test.tsx`, `EntryHandoff.test.tsx`, `Recommendations.test.tsx` | Command allow-lists re-expressed (see "Migrated tests"). |
| `docs/product-04/*` (3 files), `README.md` | Documentation; README product-feature row mentions the stored-observation section. |

## Why this is a stored read, not a scan
`list_inventory` → `state.with_db(discovery::store::inventory)` → `latest_observations`, `last_scan`, `requests`: SQL on the manager's database. `store.rs` contains no `std::fs`, `fs::`, `Command::new` or `reqwest` (checked with a pattern search; **not** by running Rust locally). The matrix has no scan control and the tests prove no `run_discovery_scan` call happens on open, target change, filter, refresh, save or reset.

## Tests (frontend, mocked typed IPC, jsdom)
`src/observation.test.ts` — the join in isolation: all four key fields (each varied alone → no match; display/project name never matches; right row picked among near-misses; exact string comparison); all eight statuses passed through unchanged (same object, note and basis); unknown status strings (`installed`, `verified`, `present`, `""`, `PRESENT_CURRENT`) never a success; non-accepted decisions (postponed, rejected, disabled, an unknown decision, none, a wish for another target) → `no_wish` even with a matching cached row; removed catalog entry → `removed`; loading / unreadable / old / reading states; raw `observations` are never a source.
`src/Observation.test.tsx` — through the real `ProjectsPage`:
* separation of wish and observation, "not a live check" sentence, the "does not show" list, no success wording or badge, one named region per row;
* **exact key**: project A vs B, Windows vs macOS, `claude-code-terminal` vs `local-cli`, same tool with four different outcomes, no carry-over after each switch, one `list_inventory` call for all switches; near-miss rows (other OS / project / surface) are not shown;
* non-accepted rows (rejected, postponed, disabled, undecided) with a matching cached row; removed catalog entry with a hostile `present_current` row;
* each of the eight statuses with its label, the backend note and basis present/absent; stale wording; permission scope reported as a permission (fingerprint ≠ verification); unrecognised status; contradictory rows;
* no scans / no requests / observation-only inventory → "no matching row", never "not scanned"; unknown compatibility stays unknown and "kullanmak istiyorum" stays unavailable;
* pending (matrix usable meanwhile), failed first read + retry, failed refresh after a good read (marked old, mark cleared on success), failed refresh after "no matching row", out-of-order reads in both orders (newer wins; an older early answer is not shown while the newer read runs), late failure of an older read;
* save → inventory re-read after the save (call order asserted) and the backend's row appears; a save the read does not know yet → conservative text; reset → comparison disappears despite the cached row; accepted → rejected drops the comparison;
* filters persist across refresh; Recommendations handoff keeps focus, mark, note and filter clearing, shows wish and observation side by side, makes no decision, does not wait for the inventory, survives an inventory failure; removed handoff target gets the existing note and no observation;
* privacy: path, evidence text and version from raw observations never reach the matrix; hostile note text is plain text;
* safety: open/navigate/filter/refresh/save/reset use only `list_projects`, `list_catalog`, `list_inventory`, `set_project_preference`, `reset_project_preference`; a list of 15 forbidden commands is never used; no `fetch`, no `window.open`, no `<a href>`, no scan control; source scans of `observation.ts` and `ObservationSection.tsx` (no network/file/shell/window/storage/plugin API, no `api.`, no control); `Projects.tsx` has exactly the one new read; the workspace component still names only the two wish commands.

### Migrated tests (none deleted, none skipped)
A title comparison against `HEAD` shows exactly two renamed tests, each now asserting more:
* `Handoff.test.tsx` "only moves the user: no task is evaluated, nothing is written, read or installed" → "…nothing is written or installed, and the only extra read is the stored inventory (never a scan)" (allow-list gains `list_inventory`; all forbidden commands unchanged).
* `Workspace.test.tsx` "a full session issues only the four existing read / wish commands…" → "…only the three existing reads (projects, catalog, STORED inventory) and the two wish commands…".
Also edited without renaming: `EntryHandoff.test.tsx` (allow-list gains `list_inventory`), `Recommendations.test.tsx` (`list_inventory` may appear only **after** the handoff reached the matrix; the Recommendations page itself still issues none), `Workspace.test.tsx` (`list_inventory` moved out of the forbidden list into the allow-list; `Projects.tsx` command list gains `listInventory`). `.skip` / `.only` / `.todo` search in `src/`: none.

## Mutation checks (one change at a time, SHA-256-verified restore)
All 23 mutants made the suite fail; **no survivors**. Counts are failing tests.
| # | Mutation | Failing |
|---|---|---|
| M1/M2/M3/M4 | join ignores platform / project / surface / uses tool id only | 16 / 16 / 15 / 19 |
| M5 | failed read treated as empty ("no matching row") | 6 |
| M6 | `previously_present` shown with the wording of `present_current` (stale as current) | 3 |
| M7 | label of `present_current` changed to "Kurulu" (detection called installation) | 16 |
| M8 | `run_discovery_scan` added to *Listeyi yenile* (forbidden call) | 4 |
| M9 | `run_discovery_scan` added to the page's load (forbidden call on open) | 9 |
| M10 | non-accepted wishes compared | 12 |
| M11 | removed entries compared | 2 |
| M12 | "old" mark never set | 4 |
| M13 / M14 | save / refresh no longer re-reads the inventory | 5 / 4 |
| M15 | missing row fabricated as `not_scanned` | 14 |
| M16 | `useLoad` accepts an older answer | 12 |
| M17 | contradictory rows: first one shown | 2 |
| M18 | unknown status treated as known | 2 |
| M19 | handoff focus waits for the inventory | 1 |
| M20 | retry button removed | 2 |
| M21 | "not a live check" sentence removed | 1 |
| M22 | extra data appended to the observation block | 8 (caught by the exact-text status tests; the *privacy* guarantee itself is structural: the view type carries only the backend `RequestVsObservation`, so raw observation fields are unreachable from the component — the privacy test pins that outcome rather than killing this mutant) |
| M23 | "old" warning text removed | 1 |
Equivalent mutants: none identified. Script: scratchpad `mutate4.py` (not committed).

## Commands run locally (Windows, `development` working tree before the commit)
| Command | Exit | Summary |
|---|---|---|
| `npm run typecheck` | 0 | `tsc --noEmit`, no output |
| `npx vitest run` | 0 | 13 files, **359 passed**, 0 failed, 0 skipped (PRODUCT-03: 280; +31 +48 new) |
| `npm run build` | 0 | vite build ok (`dist/` is untracked/ignored) |
| `python -X utf8 -m unittest discover -s tests` | 0 | 286 ran, OK, 58 skipped (unchanged from PRODUCT-03; skip reasons not itemised here) |
Diff scope check (`git diff --name-only`): no path under `src-tauri/`, `.github/`, `docs/phase-3d*`, `package*.json` or the offline-preflight folder.

## Not run / unsupported evidence
`cargo` (Rust untouched; CI runs it), `tauri build`, the real app, any GUI, WebView2, CDP port 9222, jq, the offline harness, Windows Sandbox/VM, macOS GUI. Mocked IPC and jsdom are **not** evidence of a real window: no layout, scrolling, focus ring, dark-theme contrast, screen reader, or real database content is proven. The Rust classifier is trusted as the source of truth and is covered by the existing Rust tests, not by these.

## Risk notes
* The status is only as strong as `classify_request`; the UI adds no trust and re-derives nothing. A computer-wide or `local-cli` name lookup can support a status shown under any project/surface (disclosed in the section; the contract cannot say which case applies — backlog #3).
* A wish saved while an inventory read is in flight can briefly show "no matching row"; the text is conservative and the save triggers a fresh read whose answer wins (tested in both arrival orders).
* The Turkish copy has had no editorial review.
* The Inventory page's own "İsteğiniz ile gözlem" list is unchanged and remains the place for raw evidence and explicit scans.
