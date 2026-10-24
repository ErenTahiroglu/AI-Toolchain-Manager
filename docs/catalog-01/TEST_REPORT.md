# CATALOG-01 — test report

The commit's own SHA and the exact-SHA CI result are not written here (a commit cannot contain its own hash; CI is unknown before the push). They are in the hand-off reply. Nothing here is an independent acceptance claim.

## Commands run locally (Windows, this machine)
| Command | Exit | Result |
|---|---|---|
| `cargo fmt --check` | 0 | |
| `cargo clippy --all-targets -j 2 -- -D warnings` | 0 | |
| `cargo test -j 2 --lib` | 0 | 734 passed, 0 failed, 2 ignored (719 before + 15 new) |
| `npm run typecheck` | 0 | no output |
| `npx vitest run` | 0 | 17 files, 488 passed, 0 failed, 0 skipped (frontend untouched) |
| `npm run build` | 0 | vite build ok |
| `python -X utf8 -m unittest discover -s tests` | 0 | 296 ran (286 + 10 new), OK, 58 skipped (unchanged) |
| `cargo test` for the `main.rs` bin target (0 tests) | **NOT RUN** | not attempted: the fresh binary of this target was blocked by Windows Smart App Control in the PRODUCT-06 run; nothing was renamed, copied or bypassed. Only the exact-SHA CI can speak for it. |

## New tests
**Rust (`catalog_c01_tests.rs`, 15):** the same 13 entries plus exactly the three (ids, unique source keys); byte pins (SHA-256, line endings normalised) of the two earlier data files and equality of the stored records of the 13 with those files; kind / category / licence / repository / immutable commit / revision label / origin / review / install / verification / execution policy / enabled / no plugin identity / no artifact claim / limitation text of each candidate; Playwright is not an MCP server and its sibling repositories are not catalogued; the exact six-combination compatibility matrix per entry (no tested, no unsupported, each with a limitation); nothing is accepted, recommended, installed or active by arriving (no preference or history row, an unknown combination cannot be "accepted" while "postponed" works); no executable text, key, token pattern or command in the records; repeat seeding (three more times) changes nothing; an upgraded database (the three rows removed to simulate the older state) gets exactly the three while disabled / removed entries, preferences, history, the audit sequence and every older record stay as they were; a candidate the user disabled or removed is not resurrected; a user-added Apify link (different id, same source), a user-added Playwright link (the *same id* as the seed) and a removed user entry each keep the user's row, create no duplicate, are reported as `SEED_SOURCE_TAKEN` and leave the health invariant `passed`; an empty or truncated catalog still fails it; the displayed read date is the record's own; the loader's production code has no process, network or file access and embeds the new data file at build time.
**Python (`tests/test_catalog01_contract.py`, 10):** schema validation of the three records; exact ids / kinds / owners / licences / pins; nothing reviewed, tested, hashed or commanded; the six-combination matrix; cost / privacy / scope warnings; no forbidden text, secret pattern or URL outside the five allowed; every evidence id registered in `docs/RESEARCH-SOURCES.md` with its commit; the earlier files unchanged; the Rust loader's constants and file name.
**Changed existing assertions (counts only):** `catalog.rs` tests 13 → 16 and 14 → 17 (four places); `db/migration_tests.rs` 13 → 16 (two places). No assertion was removed, weakened or skipped; two test names keep their historical wording.

## Mutation checks (9, one change at a time, SHA-256-verified restore) — all caught
| | Mutant | Caught by |
|---|---|---|
| C1 | the CATALOG-01 file is not loaded | 19 Rust tests |
| C2 | a user-owned id is no longer reported | Rust (collision report) |
| C3 | health ignores entries the user owns | Rust (2) |
| C4 | every missing seed counts as explained | Rust (truncated catalog must fail) |
| C5 | the limitation date is hard-coded again | Rust (2) |
| C6 | Exa on Claude Code promoted to vendor-declared | Rust + Python |
| C7 | Playwright re-labelled an MCP server | Rust (2) + Python |
| C8 | a review is claimed (`metadata-reviewed`) | Rust (9) + Python |
| C9 | a historical seed file edited | Rust + Python |
Thin spots: C2 and C4 are each killed by a single test.

## Not tested / limits
* jsdom / the real window: the Library page's rendering of the new entries was not exercised (no frontend change; the page reads the same fields as for the other 13). A human look at the three cards is still advisable.
* No scan, detector or recommendation behaviour exists for these tools.
* The compatibility labels are a reading of three README / manifest snapshots, not a test; the Apify rows (client statement from the README plus an OS list from the package manifest) are a judgement call.
* `tools/gui_jq_pilot_locked.mjs` (a Windows GUI harness, not run here) asserts 14 install panels (13 entries + jq); with 16 entries it will expect 17 the next time someone runs it. It was left untouched on purpose (Phase 3D tooling) and is a known stale expectation.
* The hosted endpoints, licence texts beyond the repository files, third-party components and the Claude plugin marketplace were not examined.
