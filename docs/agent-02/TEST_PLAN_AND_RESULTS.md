# AGENT-02 — test plan and results (local candidate, nothing committed)

Authoring machine: Windows 11, Smart App Control active. Date: 2026-11-14. Baseline: `development` `642558e0127887f8655eb8a80fffb5c90d54ebfb`.
This is the **author's** evidence; it is not independent acceptance.

## Results

| Check | Result |
|---|---|
| `npx tsc --noEmit` (TypeScript typecheck) | PASS, no output |
| `npx vitest run` (frontend) | PASS — 38 files, 1022 tests (this includes the new `src/Agent02.test.ts`; three older assertions about the surface count were updated) |
| `python -X utf8 -m unittest discover -s tests` | PASS — 504 tests, 58 skipped (baseline 489 + 15 new in `tests/test_agent02_contract.py`; six older contract tests that pinned "ten migrations / last file 0010" were updated to the approved eleven) |
| `cargo fmt --check` (src-tauri) | PASS (exit 0) |
| `cargo clippy --all-targets -- -D warnings` | **BLOCKED / NOT RUN** |
| `cargo test` (including the new `agent02_tests.rs` and the extended `config_txn_tests.rs` / `managed_fs_mcp_tests.rs` lists) | **BLOCKED / NOT RUN** — the Rust tests are written and formatted but have **never been compiled or executed** |
| `git diff --check` | PASS (exit 0) |
| `python tools/check_action_pins.py` (Stage 1H guard) | `ACTION_PINS_RESULT PASS files=3 uses=13`; `tools/check_action_pins.py`, its allowlist and `.github/` are unchanged (checked by `git diff` in `tests/test_agent02_contract.py`) |

### Why the Rust build is BLOCKED
Compiling the crate's dependencies failed on this machine: Windows Code Integrity (Smart App Control) refused to load a freshly built procedural-macro library
(`serde_with_macros-….dll`, `LoadLibraryExW failed … os error 4551`, Turkish text "Uygulama Denetimi ilkesi bu dosyayı engelledi") and the dependent build scripts (`tauri`,
`tauri-plugin-autostart`) then failed. No setting was changed and no workaround was attempted, per the project's standing rule. An earlier `cargo clippy` run had already
failed with `can't find crate for tauri / tauri_plugin_autostart / ece_lib` before any cleanup. **Disclosure:** to diagnose that, the author ran
`cargo clean -p tauri -p tauri-plugin-autostart -p ece`, which deleted about 1.4 GiB of regenerable build output (1332 files, inside `src-tauri/target/` only), so the previously cached
dependency build is gone on this machine until it can be rebuilt. CI is separate evidence and has not been run (no push is authorized).

### What replaced the missing Rust run (partial substitute, not equivalent)
`tests/test_agent02_contract.py` executes **migration 0011 for real with Python's sqlite3** (foreign keys ON) on a schema built from the real migrations 0001–0010: all rows and
columns preserved in order (including both `codex-cli` rows), only the surface CHECK widened, the five old and six new ids accepted, 24 wrong spellings refused, primary key / foreign key /
decision / platform / actual-state constraints intact, `foreign_key_check` empty, `integrity_check` ok, no table/index/trigger/view added or removed, other tables' DDL unchanged,
`discovery_observations` and `preference_history` DDL byte-identical, and a deliberate failure after the rebuild rolled back (old table and object set restored, version not bumped)
followed by a clean real run. It also checks that the Rust and TypeScript lists, the migration, and the catalog schema enum are the same eleven ids in the same order, and that
`plan.rs`, `recipe.rs`, `config_txn.rs`, `managed_fs_mcp.rs`, `install_flow.rs`, discovery and the other schemas name none of the new ids.
**Not substituted:** the Rust-side behaviour (`projects::set_preference` per surface, the `DB_SCHEMA_TOO_NEW` refusal, `migrate_with` rollback on the real runner, the install-plan refusal, the discovery
CHECK, per-surface `WrongSurface`). These tests exist but have not run.

## Test plan (what each layer covers)
1. TypeScript registry (`src/Agent02.test.ts`): eleven ids in one order across TS/Rust/migration; exact schema enum; plan list untouched; no desktop/alias/provider surface; labels; per-surface rows (no preview,
   none-documented, null paths, Kiro path recorded but preview none); no inherited capabilities; capability vocabulary unchanged; every new evidence id exists in the register; `UNSUPPORTED_SURFACE` and no path/diff for every
   new surface; per-surface evidence isolation; per-surface preference isolation (cline-cli vs cline-vscode, codex-cli vs codex-vscode, kiro-cli vs kiro-ide); unknown legacy ids stay unreachable.
2. UI: the existing Projects matrix selector lists all eleven surfaces in order (`src/Agent01.test.tsx`, updated); the shared guide lists ten agent surfaces and still labels Cline/Kiro as managed hosts and Cline Desktop as MANUAL_VERIFY.
3. SQLite and Rust (`src-tauri/src/agent02_tests.rs`, **not run**): migration 10→11 with row preservation, wide CHECK, rollback, re-run, file database with snapshot, newer database refused, per-surface preferences, acceptance refused for every
   entry × platform × new surface, unlisted ids refused, install plan refuses every new surface, no declaration detector, discovery CHECK unchanged, source pins.
4. Python contract (`tests/test_agent02_contract.py`, runs): the sqlite3 migration run above and cross-file consistency.
5. Static: `git diff --check`, `cargo fmt --check`, pin guard unchanged.

## Negative-matrix coverage (Issue #53 section 11)
| Requirement | Evidence | Ran? |
|---|---|---|
| Existing Codex CLI rows survive migration | Python sqlite3 + Rust test | Python yes / Rust no |
| Cross-surface preference leakage impossible | vitest (`buildRows`, `targetCounts`) + Rust per-surface test | vitest yes / Rust no |
| Unknown compatibility stays unknown, acceptance refused | vitest (`compatStateFor`, `acceptBlockReason`) + Rust all-entries loop | vitest yes / Rust no |
| New surfaces not installer targets | vitest + Python source pins + Rust `plan_for` test | vitest and Python yes / Rust no |
| New surfaces cannot write MCP configuration | extended `WrongSurface` lists (Rust) + Python source pins; no preview in TS | Python/TS yes / Rust no |
| No discovery probe | Rust detector test + discovery CHECK unchanged (Python checks DDL) | Python yes / Rust no |
| No credentials read, no process, no network added | no new command, capability, dependency, spawn or fetch (Python diff checks; vitest asserts no fetch in the existing UI tests) | yes |
| No provider classified as an agent host | vitest guide test + Python | yes |
| Schema rejects unknown ids | exact enum test; sqlite CHECK test | yes |
| Existing surface identifiers stable | all three lists compared | yes |

## Limitations
* The Rust compile, clippy and unit tests are **incomplete**; the classification therefore cannot be "ready".
* No real application database, agent, editor or user configuration was used; no network except the documentation reads recorded in `SOURCE_EVIDENCE.md`.
* No GUI check was possible.

## Future work
Codex desktop experience and Cline Desktop: Issue #54 (own migration, own evidence). Modern Web Guidance catalog: #52. Developer-agent skill target: #55.
