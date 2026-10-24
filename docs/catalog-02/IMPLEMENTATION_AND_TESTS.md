# CATALOG-02 — implementation, tests, before / after

Tracking issue: [#27](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/27) (stays open for the independent reviewer). Baseline (verified before any edit): `development` = `origin/development` = `9586d5d354b98b1e05a56f95f02c63b0db6e0f18`, `origin/main` = `68eda3f0407490fabccb517dcf822ab4818fc0bf` (untouched), clean tree, one worktree.

**Exact-SHA CI is not recorded in this file.** A commit cannot contain its own hash or a CI result that does not exist yet, and a second, documentation-only commit was not made. The commit SHA, the Actions run URL and both job conclusions are in the hand-off reply and in the comment on Issue #27.

## What changed
| Area | Files |
|---|---|
| Data (additive) | new `data/catalog-candidates-c02.json` (two schema-v2 records + `purposeRefinements`); `docs/RESEARCH-SOURCES.md` (R38–R40). `catalog-seed.json`, `catalog-candidates-1b.json` and `catalog-candidates-c01.json` are **byte-identical** (SHA-256 pins in a Rust and a Python test). |
| Rust | `catalog.rs` (second embedded file, `CATALOG02_COUNT = 2`, `SEED_ENTRY_COUNT` 16 → 18, `refine_purposes`); everything else about seeding, collisions and the health invariant is CATALOG-01's mechanism, reused unchanged |
| Rust tests | new `catalog_c02_tests.rs` (17) + 1 test in `catalog.rs`; count expectations in `catalog.rs` tests (16 → 18, 17 → 19), `db/migration_tests.rs` (16 → 18) and `catalog_c01_tests.rs` (full-catalog expectations now 18; the upgrade test deletes all five later rows to simulate the pre-CATALOG-01 state); `lib.rs` registration |
| Frontend | new `src/playwrightChoice.ts` (wording and logic), new `src/components/PlaywrightChoice.tsx`; `pages/Catalog.tsx` (+1 import, +1 prop, +1 line), `App.tsx` (+1 prop) |
| Frontend tests | new `src/playwrightChoice.test.ts` (8), new `src/PlaywrightChoice.test.tsx` (22) |
| Python | new `tests/test_catalog02_contract.py` (13) |
| Docs | `docs/catalog-02/*` (4 files) |

No migration, no IPC command, no Tauri capability, no dependency, no feature flag, no change under `.github/`, `package*.json`, `docs/phase-3d*`, `jq_pilot.rs`, discovery (scan) code or any existing frontend component other than the two lines in the Catalog page and App. Nothing downloaded, installed, launched, connected to or configured; the Playwright packages were not run (source review of pinned text files and pure / mocked tests only).

## Before / after
| | Before (CATALOG-01) | After |
|---|---|---|
| Seeded entries, fresh database | 16 | 18 (the same 16 + `microsoft-playwright-cli` + `microsoft-playwright-mcp`); real count is lower by exactly the entries a user already owns by id or live source (reported as `SEED_SOURCE_TAKEN`, health stays `passed`) |
| Upgrade of a 16-entry database | n/a | two rows appear; every decision, disabled / removed state, preference, history row, the audit sequence and all 16 stored records are unchanged (the framework entry's *purpose text* is refreshed, nothing else) |
| Playwright in the catalog | one entry (Test / Library) whose summary did not say when to choose it | three separate entries with 2–3 sentence Turkish summaries (what / when over the others / one caveat), plus a comparison on the Catalog page |
| Choosing | cards only | explicit target (project, OS, agent surface; all empty at first) → "Matriste aç" per option → the matrix; no write |
| Catalog-wide description quality | undocumented | requirement Q1–Q6 + M in `NEXT_BACKLOG.md`, enforced by a test that pins the 11 known gaps of older entries |

## Checks run locally (Windows, this machine)
| Command | Exit | Result |
|---|---|---|
| `cargo fmt --check` | 0 | |
| `cargo clippy --all-targets -j 2 -- -D warnings` | 0 | |
| `cargo test -j 2 --lib` | 0 | 752 passed, 0 failed, 2 ignored (734 before + 18 new) |
| `npm run typecheck` | 0 | |
| `npx vitest run` | 0 | 19 files, 518 passed, 0 failed, 0 skipped (488 + 30 new) |
| `npm run build` | 0 | |
| `python -X utf8 -m unittest discover -s tests` | 0 | 309 ran (296 + 13 new), OK, 58 skipped (unchanged) |
| `cargo test` for the `main.rs` bin target (0 tests) | **NOT RUN** | not attempted: Windows Smart App Control blocked a fresh binary of this target in the PRODUCT-06 run; nothing was renamed, copied or bypassed. Only the exact-SHA CI can speak for it. |

No Smart App Control block occurred in any run of this phase (lib tests, 8 frontend and 8 Rust mutant runs).

## Mutation checks (one change at a time, SHA-256-verified restore) — 16 of 16 caught
Frontend (8): handoff always opens the CLI entry; ignores the chosen surface; the incomplete-target guard removed; a project preselected; a missing record still gets a button; the unknown-compatibility explanation dropped; MCP also marked as the starting point; option order changed. Rust / data (8): the CATALOG-02 file not loaded; the purpose refinement not applied; MCP no longer declared for Claude Code; CLI promoted to declared; the MCP pin moved; the quality gate weakened; a review state claimed; the user-owned-id skip removed. Thin spots: "a missing record still gets a button" and "a review state claimed" are each killed by few tests (one frontend test, and the existing review-claim tests respectively).

## Tests that were changed, and why
Only count expectations (above) and one scope assertion in `catalog_c01_tests.rs::the_tool_kinds_and_scope_are_what_the_issue_asked_for`: it asserted that the CLI and MCP repositories were *not* catalogued — true for CATALOG-01's scope, false by design now. Its intent (the framework entry is never folded together with its siblings) is kept and stated more strongly: each sibling source key exists exactly once, under its own id. Nothing was removed, skipped or weakened; two older test names keep their historical wording ("thirteen", "four").

## Not tested / limits
* A real window, screen reader, high zoom, dark theme and a physical keyboard (jsdom and `user-event` only); editorial review of the Turkish.
* The comparison is covered through the Catalog page and through the real App up to the Projects page and the matrix row being brought into view; saving a preference from there is the existing matrix behaviour (existing tests), not repeated here.
* The Windows GUI harness `tools/gui_jq_pilot_locked.mjs` (not run, Phase 3D tooling, untouched) asserts 14 install panels and will expect 19 with 18 entries.
* Source claims are a reading of pinned text files and two summarised pages (see `SOURCE_EVIDENCE.md`); nothing about the packages' behaviour was observed.
