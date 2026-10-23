# PRODUCT-02 — one decision entry point, race-free loads, context handoff: implementation and tests

Baseline `03743924c54d48899c045014e21e7f7cec0b82ed` on `development` (clean tree, `origin/development` identical, `origin/main` `68eda3f0407490fabccb517dcf822ab4818fc0bf` untouched, no stash, one worktree). PRODUCT-01 code baseline `0dea679…` (CI green, run 40031146182). Scope: **frontend, frontend tests, README row and this documentation only**. No file under `src-tauri/`, `.github/`, `package*.json` or `docs/phase-3d*` changed (verified with `git diff --stat 0374392 -- …`: empty). No new dependency, IPC command, network call, migration, permission or feature flag.

**CI of the PRODUCT-01 docs-only commit `0374392`** was read, not assumed: at 22:52Z its macOS job was `success` and its Windows job was still `in_progress` (step "Build Tauri app"), so that run is **unresolved** here. The CI result of the PRODUCT-02 commit is reported in the handoff message, not in this file (a commit cannot contain its own result).

## Baseline matrix of the Projects page (old → new)

| Action | Baseline `0374392` | PRODUCT-02 |
|---|---|---|
| Save a wish | legacy "Karar ekle" form **and** the matrix | matrix only |
| See recorded wishes | legacy all-target table **and** the matrix (one target) | matrix + a read-only per-target count navigator |
| Reset a wish | legacy table **and** matrix | matrix only |
| Add / remove a project reference, synthetic labelling | project cards | unchanged |
| Get from Recommendations to a project's wishes | re-select project / OS / surface by hand | one action that opens the same target |
| Quick decision on a suggestion | Recommendations card | unchanged (same API, same key), wording unified |

Preserved on purpose: every backend guard (`set_preference` refusals, append-only history, audit events), the key *project + tool + OS + agent surface*, removed-entry decisions (visible, labelled, reset-only), the append/remove project flows, the Install and Plan pages, the Recommendations rules and its quick-decision buttons. Branch governance (`docs/BRANCH-GOVERNANCE.md`) was read: only `development` is written, `main` is frozen, no force push.

## What changed

### 1. P01-RT-01 — `useLoad` is "latest request wins" (`src/components/common.tsx`)
Every load takes a generation number; only the load that still holds the current generation may touch state, so a slower earlier **success, failure or `loading` bookkeeping** is ignored. Unmount (and a change of `deps`) invalidates whatever is in flight; `reload()` after unmount does nothing; a synchronous throw from the loader becomes an `AppError`. The loader is read through a ref so `reload()` never uses a stale closure. API unchanged (`data, error, loading, reload, setData`); no cache, no retry. All 22 existing `useLoad(...)` call sites are untouched. `setData` is still the raw state setter (see gaps).

### 2. P01-RT-02 — one decision entry point (`Projects.tsx`, `ProjectToolWorkspace.tsx`, `workspace.ts`)
The legacy `PreferenceRow`/`PreferenceForm` (≈90 lines) are removed. The only controls that call `setProjectPreference` / `resetProjectPreference` are inside the matrix. Nothing is lost: each project card shows its decision count and an **"Araç matrisinde aç"** button; inside the matrix a **read-only navigator** ("Bu projedeki kayıtlı kararlar") lists every target that holds decisions, with counts (removed catalog entries counted and labelled), derived from the already-loaded `list_projects` data — no new call. Decisions on a target this build does not offer (corrupt / future) are counted and reported, never dropped or deleted. The matrix target is now controlled by `ProjectsPage` (`TargetRequest`), resolved by the pure `resolveTarget` (see 3).

Vocabulary: the stored value `accepted` is shown as **"Bu projede kullanmak istiyorum"** everywhere (matrix, `labels.ts`, Recommendations button and confirmation, Inventory explanation). Stored values and their backend meaning are unchanged; "Etkinleştir…" is gone from the UI.

### 3. Recommendations → matrix handoff (`App.tsx`, `Recommendations.tsx`, `workspace.ts`)
`App` holds `handoff: TargetRequest | null` (typed local state; no router, no URL, no storage). `RecommendationsPage` gets one button, **"Bu hedefi Proje Araç Matrisi’nde aç"**, which calls `onOpenInMatrix({alias, platform, surface})` and sets the page to Projects; it does not evaluate the task or call anything. Any normal sidebar navigation clears the handoff, so it cannot be applied twice or leak into a later visit. `resolveTarget` keeps a valid request exactly; a project that no longer exists falls back to the first project **with a visible note**; an unknown OS or surface falls back to the first known one **with a note**; while the project list is loading nothing is chosen and nothing is claimed. The save/reset confirmation is bound to the target it was made for and never shown under another.

### 4. P01-RT-03/05 consistency
* One list of operating systems / agent surfaces (`TARGET_PLATFORMS`, `TARGET_SURFACES` in `workspace.ts`) now serves the matrix, Recommendations and the Plan page (their duplicate arrays are gone; Plan's explicit default values are unchanged, only its option order now follows the Rust order). The contract test that compares them to `src-tauri/src/projects.rs` stays.
* `summarize` no longer treats "anything other than not_managed/unknown" as success. The application's record has **no positive install/verification value at all** (migrations 0001/0002: `CHECK (install_state = 'not_managed')`, `CHECK (verification_state = 'unknown')`). `AFFIRMATIVE_INSTALL_STATES` / `AFFIRMATIVE_VERIFICATION_STATES` are the only places a positive value could be declared (both empty today); every other string is **"unrecognised"**, shown as "Tanınmayan durum (“…”) — başarı sayılmaz", counted separately, and never rendered as "kurulu" or "doğrulanmış". The source stays the app's own record, not a disk probe.
* Stale copy corrected: the Projects page intro referred to a "Gerçek durum sütunu" that no longer exists; the README feature row for projects now describes the matrix and its limits. The README's "Phase 3D-0" status banner is **not** changed (see backlog) — no phase claim was added.

## Files changed (vs `0374392`)

`git diff --numstat` (new test files counted with `git add -N`): **16 files, +943 / −203**.

| File | +/− | What |
|---|---|---|
| `src/components/common.tsx` | +46 −7 | `useLoad` latest-request-wins |
| `src/workspace.ts` | +117 −4 | `resolveTarget`, `targetCounts`, explicit record statuses, `TargetRequest` |
| `src/components/ProjectToolWorkspace.tsx` | +63 −14 | controlled target, navigator, fallback notes, record wording |
| `src/pages/Projects.tsx` | +29 −122 | legacy table/form removed; card count + open action; `handoff` prop |
| `src/App.tsx` | +15 −4 | handoff state, cleared on normal navigation |
| `src/pages/Recommendations.tsx` | +9 −6 | handoff button, shared lists, unified wording |
| `src/pages/Plan.tsx`, `src/labels.ts`, `src/pages/Inventory.tsx`, `src/styles.css`, `README.md` | +7 −5 | shared lists, one wording, nav styles, README row |
| `src/useLoad.test.tsx` (new) | +203 | 16 tests |
| `src/Handoff.test.tsx` (new) | +161 | 8 tests |
| `src/workspace.test.ts` | +82 −7 | 37 tests (was 24) |
| `src/Workspace.test.tsx` | +177 −8 | 49 tests (was 38) |
| `src/App.test.tsx` | +34 −26 | 4 legacy tests re-expressed, wording |
| `docs/product-02/*` (new) | — | this evidence |

## Existing tests that had to change, and why (nothing deleted, nothing weakened)

| Test | Reason | Now |
|---|---|---|
| App: "labels sample projects, separates desired choice from actual state…" | asserted the legacy table | same claims, on the matrix row reached through the navigator; "no installed / verified wording" check kept |
| App: "saves a decision with the full key and shows backend refusals" | used the legacy form | same full-key and backend-refusal assertions on the matrix row (surface local-cli, because `accepted` is correctly unavailable where compatibility is unknown) |
| App: "offers postponing with a bounded number of days and an explicit reset" | legacy form/table | same min/max and exact reset-call assertion on the matrix |
| App: "flags a material change but treats a plain version bump as informational only" | asserted legacy wording | same two-way assertion with the matrix wording; fixture uses real catalog ids |
| App: 3 uses of the button "Bu proje için etkinleştir?" | unified vocabulary | same button, "Bu projede kullanmak istiyorum" |
| Workspace: "'accepted' reads as a wish…" (summary line) | counters are now explicit | asserts "5 kurulu değil / yönetilmiyor · 5 doğrulanmadı" and the absence of any affirmative or unrecognised wording |
| Workspace: 2 tests about "the old form/table" | those controls were removed on purpose | replaced by 3 tests proving there is exactly one write/reset entry point |
| workspace.test.ts: "reports what the record says… if it ever says more" | old rule treated any other string as success | replaced by 5 tests for the explicit rule |

## Required-test coverage map

| Requirement | Tests |
|---|---|
| 1 out-of-order, stale errors, unmount, repeated reloads | `useLoad.test.tsx` (16): A→B, B→A, stale rejection after newer success, stale success after newer failure, overlapping loads, scrambled order, unmount (success and rejection, no React warning), reload after unmount, deps change, newest loader; plus the UI-level "a slow, stale project list that arrives after a newer one is ignored" (a stale read would resurrect an old saved preference) |
| 2 one entry point; add/remove preserved; all targets reachable | `Workspace.test.tsx`: "the Projects page has exactly one place…" (3), "every recorded decision stays reachable…" (6, including removed-entry decisions, unreachable targets, card shortcut, add/remove), App "registers a real project…", "removing a reference…" |
| 3 handoff, back-and-forth, invalid fallback | `Handoff.test.tsx` (8), `workspace.test.ts` `resolveTarget` (6) |
| 4 catalog-disabled vs installed, missing evidence, policy-blocked, wish ≠ active | PRODUCT-01 tests retained (`what the workspace shows…`, `a wish is not a state`) + record-status tests below |
| 5 hostile text inert; no scripts, URLs, network, fs, forbidden IPC | hostile catalog and **project** text tests; command allow-list test (4 commands, 21 forbidden, `fetch`/`window.open` spies, no links); source scans of the workspace, logic, **Projects.tsx and App.tsx** (banned APIs; exact `api.*` call sets) |
| 6 accessibility | roles, names, labels, `aria-current`, live status, error + retry, Enter-key activation, focus move on the card shortcut — **in jsdom only** |
| 7 full regression | all pre-existing Plan (13), Inventory (21), Install (26) and App (39) tests pass unchanged except the rows above |
| Unfamiliar / corrupt / future statuses | `workspace.test.ts` (5, 15 odd strings incl. `installed`, `installed_verified`, `passed`, `verified`, case and whitespace variants) and the UI test "an unfamiliar status is never shown or counted as success" |

## Commands run and exact results (Windows 11, Node v24.20.0, this machine)

| Command | Result |
|---|---|
| `npm run typecheck` | exit 0 |
| `npx vitest run` | exit 0 — **8 files, 209 passed, 0 failed, 0 skipped, 0 todo** (useLoad 16, workspace 37, Plan 13, Inventory 21, Handoff 8, Install 26, Workspace 49, App 39). Baseline before this phase: 161. |
| `npm run build` (`tsc --noEmit && vite build`) | exit 0 |
| `python -X utf8 -m unittest discover -s tests` | exit 0 — **286 run, OK, 58 skipped** (skips are unittest's; reasons not itemised here) |

**Mutation checks** (tests run against deliberately broken code, files restored and hash-verified): the original `useLoad` restored → 12 tests fail (11 in `useLoad.test.tsx` plus the UI-level stale-read test); App ignoring the handoff → 5 fail; normal navigation keeping a stale handoff → 1; `installed` declared affirmative → 4; silent project fallback → 2; navigator ignoring removed-entry decisions → 2; handoff dropping the surface → 3. All caught.

### Not run (explicit)
`cargo fmt/clippy/test` (default, `synthetic-pilot`, `jq-pilot`) — **not run locally** (no Rust change); they run in CI on the pushed SHA. `tauri build`, `--self-check`, any application start, GUI, WebView2, port 9222, any real tool/agent interaction, the offline-preflight harness (Rev 1–5, never opened) — **not run, by constraint**. Real visual layout, dark-theme contrast, narrow-window collapse and screen-reader output — **not verified** (jsdom has no layout or assistive technology; only roles, names, states and keyboard activation are asserted).

## Safety boundaries held
No installation, download, update, removal, jq access, agent-configuration write, network request, telemetry, model call, file read/write or new Tauri command was added or exercised. The only commands the Projects page can issue are `list_projects`, `list_catalog`, `set_project_preference`, `reset_project_preference`, `add_project_reference`, `remove_project_reference` (asserted). A desired preference is never shown as installed, verified or active; an unfamiliar state is "unrecognised", never success. Issues #1, #2, #4, #14, #25 were not edited; `main` was not touched.

## Known gaps
1. `useLoad.setData` is still the raw setter: a manual `setData` (used by Settings) does not cancel an in-flight load, which could later overwrite it. Pre-existing; rare.
2. Recommendations keeps its own quick-decision buttons (same API and key). That is a second *writable* path in the application, deliberately left because it is a task-contextual flow with its own tests; the Projects page has exactly one.
3. The advisory "why can't I choose this" logic in `acceptBlockReason` mirrors the backend; the backend remains authoritative and its refusals are shown.
4. Plan's target option order now follows the Rust order (was `local-cli` first); its defaults are unchanged.
5. The README status banner still says "Phase 3D-0"; it is out of date but correcting it needs an evidenced phase statement.
6. jsdom limits as above; the Turkish copy has had no separate editorial review.
7. CI for `0374392` (PRODUCT-01 docs) was unresolved when this was written (see top).
