# CATALOG-03 — implementation, tests, before / after

Tracking issue: [#28](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/28) (stays open for the independent reviewer). Baseline (verified before any edit): `development` = `origin/development` = `b16a0cc8c670e71aa8c862625ff5fe9ff86a4739`, `origin/main` = `68eda3f0407490fabccb517dcf822ab4818fc0bf` (untouched), clean tree, one worktree, no stash.

**Exact-SHA CI is not recorded in this file.** A commit cannot contain its own hash or a CI result that does not exist yet, and no documentation-only follow-up commit was made. The commit SHA, the Actions run URL and both job conclusions are in the hand-off reply and in the comment on Issue #28.

## What changed
| Area | Files |
|---|---|
| Data (additive) | new `data/catalog-candidates-c03.json` (four schema-v2 records in the established wrapper); `docs/RESEARCH-SOURCES.md` (R41–R44). `catalog-seed.json`, `catalog-candidates-1b.json`, `-c01.json` and `-c02.json` are **byte-identical** (SHA-256 pins in a Rust and a Python test). |
| Rust | `catalog.rs`: a third embedded file, `CATALOG03_COUNT = 4`, `SEED_ENTRY_COUNT` 18 → 22, one more `all.extend`. The seeding, collision reporting, metadata refresh, user-owned-id skip and the health "explained shortfall" rule are CATALOG-01/02's mechanism, **reused unchanged**. |
| Rust tests | new `catalog_c03_tests.rs` (16); `catalog_c02_tests.rs` (`quality_gaps` made `pub(crate)`, one documented exception added, id sets and counts); `catalog_c01_tests.rs` (id sets and counts; the upgrade test deletes every later row); `catalog.rs` and `db/migration_tests.rs` (counts 18 → 22, 19 → 23); `lib.rs` registration |
| Python | new `tests/test_catalog03_contract.py` (13) |
| Docs | `docs/catalog-03/*` (4 files); `README.md` (22 candidates, none a tested tool) |

No migration, no IPC command, no Tauri capability, no dependency, no feature flag, no frontend change, and nothing under `.github/`, `package*.json`, `docs/phase-3d*`, `jq_pilot.rs` or the discovery (scan) code. Nothing was downloaded beyond text files, installed, launched, connected to or configured; none of the four tools was run, and none of their pipe-to-shell, `npx` or download-and-append commands was executed or reproduced.

## Before / after
| | Before (CATALOG-02) | After |
|---|---|---|
| Seeded entries, fresh database | 18 | 22 (the same 18 + 4); lower by exactly the entries a user already owns by id or live source (reported as `SEED_SOURCE_TAKEN`, health stays `passed`) |
| Upgrade of an 18-entry database | n/a | four rows appear; every decision, disabled / removed state, preference, history row, the audit sequence and all 18 stored records are unchanged |
| Quarantined, execution-blocked entries | `strix` | `strix`, `thedotmack-claude-mem`, `ruvnet-ruflo` (they can never be "accepted", whatever their compatibility cells say) |
| Entries with an unverifiable licence | one (`agentic-awesome-skills`, per-skill) | two; `multica-ai-andrej-karpathy-skills` is `review-required` with no SPDX id, a documented exception to the Q2 gate |
| Declared compatibility cells among the new four | n/a | one: Claude-Mem × Claude Code × Windows (on a quarantined entry) |

## Checks run locally (Windows, this machine)
| Command | Exit | Result |
|---|---|---|
| `cargo fmt --check` | 0 | |
| `cargo clippy --all-targets -j 2 -- -D warnings` | 0 | |
| `cargo test -j 2 --lib` | 0 | 768 passed, 0 failed, 2 ignored (752 before + 16 new) |
| `npm run typecheck` | 0 | |
| `npx vitest run` | 0 | 19 files, 518 passed, 0 failed, 0 skipped (unchanged: no frontend change) |
| `npm run build` | 0 | |
| `python -X utf8 -m unittest discover -s tests` | 0 | 322 ran (309 + 13 new), OK, 58 skipped (unchanged) |
| `cargo test` for the `main.rs` bin target (0 tests) | **NOT RUN** | not attempted: Windows Smart App Control blocked a fresh binary of this target in the PRODUCT-06 run; nothing was renamed, copied or bypassed. Only the exact-SHA CI can speak for it. |

No Smart App Control block occurred in any run of this phase (lib tests and the 12 mutant runs).

## What the new tests pin
**Rust (`catalog_c03_tests.rs`):** 18 → 22 on a fresh database, the exact id set, 22 unique source keys and exactly the four new lower-cased source identities; the four earlier data files byte-identical and every earlier record equal to its own file's; per record: kind, category, licence state / SPDX, repository, immutable commit, `observedAt`, no release, review state, execution policy, origin, install / verification state, enabled, no plugin identity / source path / artifact evidence, read date shown, unknown targets stated, quarantine statement; the canonical identities and that the former owner never appears in an identity field, only as labelled stale history, and that the summary never implies Karpathy authorship or endorsement; Ruflo's packaging ambiguity recorded and no approved install method; the Karpathy licence `review-required` with no invented proof; summaries (100–300 characters, 2–3 sentences, what / when / risk, "this application does not…", no URL / command / README / recommendation, "karantinada" on both quarantined ones); the quality gate (three pass everything, the fourth fails only Q2); the compatibility matrix (six cells each, only Claude-Mem × Claude Code × Windows declared, Ruflo's "all platforms" sentence and Antigravity-without-a-surface not taken as declarations, a surface the record does not mention is unknown); nothing accepted, recommended or installed by arriving (no preference or history rows; quarantined entries refuse "accepted" with `ACTIVATION_BLOCKED` even on the declared cell; unknown cells refuse it with `PREFERENCE_NOT_ACTIVATABLE`; "postponed" / "rejected" still work); no executable authority, secret, command or unexpected URL in the records; the risk statements; 18 → 22 upgrade preserving choices and the audit chain; idempotence and no resurrection of removed / disabled candidates; links the user added earlier win, are reported and are never duplicated (all four ids); a removed user entry stays removed; a link to the **former owner** is a different source, never merged; a truncated catalog still fails the health invariant; seeding writes no audit event, scan or observation; the loader is pure data.
**Python:** the same contract over the data file, including schema validation, pins, posture, the single declared cell, the licence exception, evidence registration with the commit in each register row, and the four unchanged files.
**Existing tests:** only counts and id sets changed (18 → 22, 19 → 23), plus one documented exception in the quality-gap list. Nothing was removed, skipped or weakened; two older test names keep their historical wording.

## Mutation checks (one change at a time, SHA-256-verified restore) — 12 of 12 caught
The CATALOG-03 file not loaded; Claude-Mem no longer quarantined; Ruflo no longer execution-blocked; the Karpathy licence promoted to a known MIT; the Karpathy repository pointing at the former owner; Ruflo declared for Claude Code on Windows from the unverified route; the Understand Anything pin moved; the summary implying Karpathy authorship; an earlier data file edited; Claude-Mem declared for macOS Claude Code without evidence; the user-owned-id skip removed; the health rule ignoring user-owned entries. Thin spots: several are killed by a single test (for example the macOS declaration, the pin change and the earlier-file edit by one Rust test plus the Python contract test); the user-owned-id and health mutants are Rust-only by nature.

## Not tested / limits
* Nothing about the four tools' behaviour was observed: claims are a reading of pinned text files. No real window, no screen reader, no Turkish editorial review (the catalog cards render the same fields as for every other entry; no frontend change was made, so no new UI test).
* The hosted services, accounts, the contents of hook scripts, the Claude-Mem Cowork module and the individual Ruflo modules were not examined.
* No discovery (scan) support exists for any of the four.
* `tools/gui_jq_pilot_locked.mjs` (a Windows GUI harness, not run, Phase 3D tooling, untouched) still asserts 14 install panels and will expect 23 with 22 entries.
