# Verification record (Phase 3D-6) — developer-generated, every run reported

Nothing here is independent evidence. The independent reviewer's B1 limitations stand: the Rust (48) and frontend (2) tests and the real window were **NOT independently run** by the reviewer. CI is not independent user-device testing. The exact-SHA CI result of the final commit cannot be written into that commit (it would change the SHA); it is reported in the hand-off and is readable on the CI run of that SHA.

## Commits (on `development`; no branch was created, no history rewritten, `main` untouched)

| Commit | What | Application behaviour |
| --- | --- | --- |
| `c31be87` | tests and doc comments accept the allowance *inactive or armed for the one authorized scope*; three new tests | none |
| `ddf0602` | **the arming**: `B3_ARMED_DATA_DIR_SHA256: None → Some("158c279e…e8a5")` (one line, one file) | this one constant |
| final commit | tests now *require* the armed state exactly; the Phase 3D-6 contract, documents and evidence | none |

The only non-comment source line that differs from the starting SHA `5d5feef3…` is the arming line (checked with `git diff 5d5feef3 -U0 -- src-tauri/src/jq_pilot.rs` after removing comment lines). Nothing under `docs/phase-3d2`, `docs/phase-3d3`, `docs/phase-3d3b`, the B1 binding record and its evidence, the adapter, `pilot_allowance*`, `install_flow.rs`, `managed_*`, `lifecycle.rs`, `recipe.rs`, `attestation.rs`, `pilot_supply.rs`, `Cargo.toml`, `lib.rs` or `.github` changed.

## Results on the armed tree (this computer, Windows)

| Check | Result |
| --- | --- |
| Directory digest, four independent computations | `158c279e…e8a5` ×4 (Python tool, Node, PowerShell, the application's Rust function through `ATM_B3_DIR_CHECK`) |
| `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | clean: default, `--features jq-pilot`, `--features synthetic-pilot` |
| `cargo test` default / `jq-pilot` / `synthetic-pilot` | **702 / 702 / 702 passed** (2 pre-existing ignored each); the default run had `ATM_B3_DIR_CHECK` set so the Rust digest check ran against the real directory name |
| Python `unittest discover` with the self-check JSON, `ATM_B3_DIR_CHECK` set | all pass, including `tests/test_phase3d6_contract.py` (exact constants, digest, **no plaintext path anywhere in the repository**, release builds closed) |
| `tools/verify_review_package.py` | no failures |
| Frontend | typecheck clean · 99/99 · build OK |
| Staging reproductions (`tools/run_staging.py`) | B3 manifest **21/21** (the three new arming tests in B3-8) · B2 **20/20** · B1 **50/50** |
| Headless self-check, default debug build (SHA-256 `ede88452…c505`, temporary data directory) | exit 0 on the first launch; `installSupplyProbe`: 0 recipes / references / enablement entries / evidence / notice bundles, `oneTimeAllowances: 0`, `oneTimeAllowanceArmed: false`, `pilotFeatures` both false, `entriesThatCanBeInstalledOrRemoved: 0` |
| Real window, armed `jq-pilot` debug build (SHA-256 `52513fe2…5dcb`), temporary data directory — **not** the authorized directory | **16/16**: jq listed as *Doğrulama tamamlanmadı* with the reason **`pilot_wrong_data_directory`**, no install or remove button, forged IPC refused, only the application's own SQLite files changed, no `atm-managed` and no `jq-pilot-input` folder, no install/approve audit row ([result](evidence/gui-jq-armed/gui/result.json)). The environment snapshot (PATH, Run/Environment/App-Paths keys, Smart App Control policy state, Defender preferences when readable, shortcuts, agent-settings hash) was **identical before and after** ([summary](evidence/gui-jq-armed/observation-summary.json)). |
| Real window, synthetic-pilot regression (SHA-256 `8137b714…b7ff`) | **BLOCKED / NOT RUN** — Smart App Control, below |

An earlier launch of the armed binary of this phase was run once with a driver check that expected only the *not armed* wording; it failed one check (15/16) because the accurate wording for another directory is *"…başka bir veri klasörü için etkinleştirilmiş…"*. The driver's expectation was widened (listed in [GUARD-TEST-CHANGES](GUARD-TEST-CHANGES.md)); the application was not changed. That capture, and a first network capture with a defective observer, were discarded and replaced (see [NETWORK-OBSERVATION](NETWORK-OBSERVATION.md)).

## What the synthetic tests exercise (unchanged mechanism, armed constants)

Preview, consent, installation, rollback and recovery on **synthetic bytes** in a temporary directory armed for *that* directory, by the existing B2/B3 tests: one installation and restart; repeated installation after success; removal then reinstallation (refused); eight concurrent confirmations (exactly one wins); a crash at each of 13 points; audit-write failure; corrupted, missing, forged and foreign allowance state; missing or altered receipts; wrong recipe, artifact, platform or data directory; consent replay and stale preview; unrelated files unchanged. New in this phase: the real supply, armed, refuses **every other** data directory before it reads a byte (3 directories, 0 provider requests, nothing created), and the armed digest is one scope identifier that no ordinary location matches.

## Smart App Control record

* Blocked, recorded as **BLOCKED / NOT RUN**, not counted: the `synthetic-pilot` debug application (`8137b714…b7ff`) at launch — *"Uygulama Denetimi ilkesi bu dosyayı engelledi"* — twice (CodeIntegrity event 3077 at 12:48:18 and 12:48:30). The identical binary was re-run once, as the rule allows; it was **not** rebuilt to evade the decision and no setting was changed.
* Not blocked in this phase: the default debug application (`ede88452…c505`, one launch), the armed `jq-pilot` debug application (`52513fe2…5dcb`, four launches: two driver runs that were superseded, one process-attribution launch, the final observed run), and every `cargo test` executable that ran.
* jq was never run, so Smart App Control's reaction to a real jq is still unobserved.

Evidence files were committed as captured, with one edit: the absolute path of this repository's work tree was replaced by `<repo>` in two logs (no earlier committed file contains a personal absolute path). The authorized directory's path appears in no file.

## Not shown

The real preview and installation (not started), jq itself, Smart App Control's reaction to a real executable, an armed pilot in the authorized directory in a real window (deliberately not launched), a second computer, macOS beyond CI. The WebView2 runtime's own outbound connections are reported in [NETWORK-OBSERVATION](NETWORK-OBSERVATION.md).
