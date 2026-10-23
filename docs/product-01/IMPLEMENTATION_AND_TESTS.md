# PRODUCT-01 — Project Tool Workspace: implementation and tests

Baseline `37c58fb86b830ad676607556d12b8014d7657a2f` on `development` (clean tree, `origin/development` identical, `origin/main` `68eda3f0407490fabccb517dcf822ab4818fc0bf` untouched). Scope: **frontend and frontend tests only**. No Rust, migration, permission, capability, WebView2, dependency, network or installer change.

## What changed for the user

**Before.** The Projects page showed, per project, a table of decisions that had *already been recorded*, plus a form ("Karar ekle") where the user picked a tool, surface, OS and decision from four dropdowns. There was no view of the tools that had **no** decision yet, no compatibility for the chosen target, no search or filter, no catalog / license / limitation context next to the choice, and the form's wording "Bu proje için etkinleştirmek istiyorum" could be read as an action. A failing catalog load was silent (the dropdown was just empty).

**After.** A new panel **"Proje Araç Matrisi"** at the top of the Projects page. For one selected project, one operating system and one agent surface it lists every active catalog tool and shows, side by side but separately:

| Block | Content | Source of the data |
|---|---|---|
| Catalog | name, type, purpose, documented source (plain text, no link), license, review state, execution policy, "Katalogda etkin / kapalı / kaldırıldı", limitations | `list_catalog` (existing) |
| Compatibility for **this** OS + surface | vendor-declared / independently tested / unsupported / **unknown** (no record is *unknown*, never support) + the recorded limitation | `targetEvidence` of the catalog record, same lookup as Rust `catalog::compatibility` |
| Your wish | one of *Bu projede kullanmak istiyorum / Şimdi değil (ertele) / Reddet (tekrar sorma) / Bu projede kapalı tut / Henüz karar yok*, time, catalog version, reminder, "important change since the decision" | `list_projects` (existing `PreferenceView`) |
| Actual state (separate) | "Kurulum", "Doğrulama", "Kapsam" as the application *records* them, and "Ajanda etkin mi: Bilinmiyor — bu uygulama ajan ayarlarına bakmaz" | `PreferenceView.actual*` / `CatalogEntry.installState` — the app's own record |

Actions: **"Yalnızca tercihimi kaydet"** and **"Kararı sıfırla"** — through the existing `set_project_preference` / `reset_project_preference` only. Search, type / decision / compatibility filters, a summary line (wishes vs the application's record), "Listeyi yenile". The old per-project cards, tables, add form and the project add / remove flows are unchanged and still tested.

Design rules that are enforced by tests (next section): wishes are keyed by project + tool + OS + surface; "accepted" never changes any real-state text; an unavailable choice says why; removed catalog entries keep a visible, labelled old wish that can only be reset; every save / reset is single-flight; a confirmation is shown only under the target it was saved for; failed loads are errors with retry, never "empty".

**Deliberate decision — no `list_install_states` call.** The install-state command probes the managed install folder on disk. The brief forbids filesystem reads from the new view, so "Gerçek durum" is the application's *recorded* state, the panel says so ("bu ekranın bilgisayarı denetlediği anlamına gelmez"), and it points to the Araç Kütüphanesi page for installation detail.

## Files changed (working tree vs baseline)

| File | Change |
|---|---|
| `src/workspace.ts` (new, 179 lines) | Pure logic: target lists, labels, `compatClass` (fail-safe), `evidenceFor`, advisory `acceptBlockReason`, `buildRows` (project / target scoped), `filterRows`, `summarize`, `normalize`, `sameKey`. No I/O. |
| `src/components/ProjectToolWorkspace.tsx` (new, 344 lines) | The panel and its rows. Calls only `api.setProjectPreference` and `api.resetProjectPreference`. |
| `src/pages/Projects.tsx` (+2) | Imports and renders the panel with the page's existing `projects` and `catalog` loaders. |
| `src/labels.ts` (+1) | Label for the existing backend state `independently-tested` (it was displayed as the raw string). |
| `src/styles.css` (+10) | Row / grid / summary styles; one column under 560 px. |
| `src/workspace.test.ts` (new, 24 tests) | Unit tests of the logic, including a contract test that pins the surface / platform / decision lists and the acceptance rules to `src-tauri/src/projects.rs`. |
| `src/Workspace.test.tsx` (new, 38 tests) | Component + safety tests inside the real Projects page against the in-memory backend. |
| `docs/product-01/*` (new) | This evidence set. |

Not touched: everything under `src-tauri/`, `docs/phase-3d*`, `.github/`, `package*.json`, the git-ignored `src-tauri/target/offline-preflight/` (including `review-package-rev5.zip`), the retained jq bytes, the authorized pilot directory, the WebView2 profile.

## Required tests → where they are

| # | Requirement | Test(s) (`src/Workspace.test.tsx` unless noted) |
|---|---|---|
| 1 | A's choice never shown as B's | "one project's wish never appears for another project…"; `workspace.test.ts` "takes only this project's wish…" |
| 2 | Surface / platform are separate keys | "the same project and tool on another surface or operating system is a separate wish…" |
| 3 | accepted ≠ installed / verified / active; disabled ≠ deletion | "'accepted' reads as a wish…", "saving 'accepted' stores a wish…", "'disabled' is only a recorded wish…" |
| 4 | Catalog-disabled ≠ absent executable; unknown ≠ supported | "a catalog entry that is switched off…", "unknown compatibility stays unknown…", "shows 'unsupported'…", "execution-policy entry…"; `workspace.test.ts` "only an explicit declaration… counts" |
| 5 | Removed and materially changed shown truthfully; no new action on removed | "a removed catalog entry keeps its old wish visible…", "flags a decision made before a material capability change…" |
| 6 | Loading / empty / failure / retry / rapid target changes, no stale positive | "shows a loading state first…", "says so when the catalog is empty…", "no wishes…", "no project at all…", "a failing catalog…", "a failing project list…", "a refused save…", "a refused reset…", "a double click…", "changing the target while a save is in flight…", "the refresh button…", "an answer for another key…" |
| 7 | Stable filtering / search; accessibility | "search and filters" (3 tests), `workspace.test.ts` "filtering is stable…"; "accessibility" (4 tests) |
| 8 | No install / confirm / remove-executable, no network, no filesystem, no agent-config calls | "a full session issues only the four existing read / wish commands…" (command allow-list, 21 forbidden commands, `fetch` and `window.open` spies, no links), "the source of the new view names no network, file, shell, window or plugin API and calls only setProjectPreference / resetProjectPreference" |
| 9 | Existing flows green | The full pre-existing suite (99 tests) passes unchanged; "the existing Projects page still works around the workspace" |

## Commands run and exact results (this machine, Windows 11, Node v24.20.0)

| Command | Result |
|---|---|
| `npm run typecheck` (baseline, before any change) | exit 0 |
| `npx vitest run` (baseline) | exit 0 — 4 files, **99 passed** |
| `npm run typecheck` (after) | exit 0 |
| `npx vitest run` (after) | exit 0 — 6 files, **161 passed** (99 existing + 24 + 38), 0 failed, 0 skipped |
| `npm run build` (`tsc --noEmit && vite build`) | exit 0 — 34 modules; `dist/` is git-ignored |
| `python -X utf8 -m unittest discover -s tests` | exit 0 — **286 run, OK, 58 skipped** (skips are reported by unittest; their reasons are not itemised here) |

**Mutation checks** (the tests were run against deliberately broken logic, then the files were restored and re-hashed): unknown compatibility counted as support → 7 tests failed; wish lookup ignoring surface / platform → 3 failed; confirmation shown under any target → 1 failed ("changing the target while a save is in flight"); an answer for another key accepted → 2 failed. All four were caught.

Two defects were found and fixed *by* these tests before commit: the hidden accessible-name suffix of the save / reset buttons lost its separating space ("…kaydet(Alfa Araç…") — the space now sits outside the visually hidden span; and a post-save confirmation would have vanished with its row under an active "Karar yok" filter — the confirmation now lives in a status line bound to the target.

### Tests NOT run (explicit)

* `cargo fmt --check`, `cargo clippy`, `cargo test` — default, `--features synthetic-pilot` and `--features jq-pilot` — **not run locally** (no Rust file changed). They run in CI for the pushed SHA (including the four `issue_4_*` tests of `closure_tests.rs`).
* `tauri build`, headless `--self-check`, any application start, any GUI / WebView2 / port-9222 session, any macOS GUI check — **not run**; forbidden in this phase.
* Visual / responsive rendering: jsdom has no layout engine, so the 560 px collapse, dark theme contrast and real screen-reader output are **not verified**; only roles, names, states and keyboard activation are asserted.
* The Rev 5 offline-preflight harness — **not run, not edited** (deferred).

## CI evidence for the exact SHAs

Code commit **`0dea679bc19711ce289f7faa2c00cd384c196cc2`** (pushed to `origin/development`; `origin/main` still `68eda3f0407490fabccb517dcf822ab4818fc0bf`).

* CI run: https://github.com/ErenTahiroglu/AI-Toolchain-Manager/actions/runs/40031146182 — `completed / success` for that exact SHA.
  * windows-x64 job: https://github.com/ErenTahiroglu/AI-Toolchain-Manager/actions/runs/40031146182/job/120679414032 — success.
  * macos-arm64 job: https://github.com/ErenTahiroglu/AI-Toolchain-Manager/actions/runs/40031146182/job/120679414162 — success.
  * Steps not run by design: "jq-pilot feature is refused on macOS" (macOS-only step) is skipped on Windows; "jq-pilot feature configuration" (Windows-only step) is skipped on macOS. All other steps (Python contract tests, `npm ci` + typecheck + test + build, `cargo fmt/clippy/test`, `synthetic-pilot` and Windows `jq-pilot` configurations on **synthetic bytes**, staging reproductions, Tauri build, headless self-check) concluded `success`.
* What this proves: those steps passed on the CI runners for that SHA. It is not a local run (the Rust steps were not run on the developer machine), not a macOS GUI test, and not evidence about any real tool, jq or WebView2.
* This documentation-only follow-up commit changes no code; its own CI result is reported separately in the phase report, because a commit cannot contain its own SHA.

## Known gaps

1. `useLoad` has no guard against an older response overwriting a newer one after two quick reloads (shared hook; unchanged). The Tauri IPC answers in order in practice; not proven.
2. The surface / platform lists exist in three places (`Projects.tsx`, `Recommendations.tsx`, `workspace.ts`). Only `workspace.ts` is pinned to the Rust constants by a test.
3. `acceptBlockReason` mirrors the backend checks as *advice*. A contract test pins the key strings; if the Rust rules change, the backend still refuses and the panel shows its error, but the advice could be stale.
4. The old "Karar ekle" form keeps its older wording ("Bu proje için etkinleştirmek istiyorum") to avoid a regression in its tests; two entry points with different wording coexist until they are merged (see backlog).
5. Ordering uses a code-point comparison of accent-folded names (deterministic everywhere), not ICU collation.
6. "Gerçek durum" is the application's record, not an observation of the computer (by design, see above). It cannot show a synthetic pilot installation; the Araç Kütüphanesi page does.
7. Turkish copy was written by the implementer without a separate editorial review.
