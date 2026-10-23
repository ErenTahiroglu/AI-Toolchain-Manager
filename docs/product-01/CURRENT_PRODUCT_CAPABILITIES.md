# PRODUCT-01 — current product capabilities (source-linked inventory)

Baseline `37c58fb86b830ad676607556d12b8014d7657a2f` (`development`); frozen `main` `68eda3f0407490fabccb517dcf822ab4818fc0bf`. Inventory taken by reading the source named in each row; nothing here was measured on a running application. "Implemented" means the code and a test for it exist, not that it was exercised on a user's machine. Statuses: **Implemented**, **Partial**, **Gated** (code exists but is switched off or locked), **Missing**.

## 1. What a user can do today (default build)

| Area | Status | What exists | Source | Honest limit |
|---|---|---|---|---|
| Tool catalog | Implemented | 13 documented seed entries + user-added GitHub candidates (offline: the link is parsed, nothing is fetched); enable / disable / remove / restore of a *catalog record* | `src-tauri/src/catalog.rs` (`list_catalog` 563, `compatibility` 578), `src/pages/Catalog.tsx` | A catalog switch is a record, not an installation. Metadata is a 2026-10-17 documentation snapshot, not independently reviewed. |
| Projects | Implemented | Synthetic sample projects and *reference* projects (alias + private note); the app never opens or scans the folder | `src-tauri/src/projects.rs` (`add_reference` 188), `src/pages/Projects.tsx` | A project is a label, not a directory the app knows. |
| Per-project desired choices | Implemented | One decision per **(project, tool, agent surface, platform)**: `accepted / postponed / rejected / disabled`, with time, catalog revision, reminder date, material-change flag, append-only history, audit events for reject / defer / reset-rejection | `projects.rs` (`set_preference` 272–354, `reset_preference` 357–384; surfaces / platforms / decisions 17–19) | A decision is intent. `accepted` is refused where compatibility is unknown / unsupported, the entry is catalog-disabled, or its execution policy is blocked (`projects.rs` 289–308). |
| Project Tool Workspace ("Proje Araç Matrisi") | **New in PRODUCT-01** | See `IMPLEMENTATION_AND_TESTS.md` | `src/workspace.ts`, `src/components/ProjectToolWorkspace.tsx` | Planner only. |
| Deterministic suggestions | Partial | Fixed rules, no model call, per-project dedupe, suggestion log capped per project | `src-tauri/src/recommend.rs` (`prune_log` 457–463), `src/pages/Recommendations.tsx` | A preview driven by task text typed into the page; it is not connected to a live agent session (Issue #2 stays open). This inventory did not audit the rule set. |
| Read-only discovery / inventory | Implemented | With per-area permission: PATH file names, a few Claude Code user files, one project folder; optional SHA-256 fingerprint; "request vs observation" view | `src-tauri/src/discovery/`, `src/pages/Inventory.tsx` | Finding a file name is not identity, configuration or function. No program is run. |
| Installation plan preview | Implemented | "What would happen" draft per tool / platform / surface | `src-tauri/src/plan.rs`, `src/pages/Plan.tsx` | A draft; `executable: false`. |
| Audit, health, diagnostics, settings | Implemented | Hash-chained audit log, self-checks, local diagnostics, theme / tray / startup | `audit.rs`, `health.rs`, `diagnostics.rs`, `settings.rs` | — |
| Prompt library page | Partial | Three sources listed as *planned, read-only, untrusted* | `src/pages/Prompts.tsx` | "İndir / ara" is a disabled button. No retrieval exists. |

## 2. Classification requested for this phase

| Topic | Classification | Evidence / reason |
|---|---|---|
| **Product usability (day-to-day)** | **Partial → improved by PRODUCT-01.** Before: decisions could only be added through a flat "Karar ekle" form and a table of already-recorded rows; undecided tools, per-target compatibility and filtering were not visible in the Projects page. After: one matrix per project / OS / surface. | `src/pages/Projects.tsx` (before: lines 60–148 only) |
| **Real installation** | **Gated.** Default build: documented as "no reviewed recipe, nothing installable" (`install_flow.rs` reports `verification_incomplete`; see the Catalog page copy and README) — this inventory did not run the app to observe it. With the non-default `synthetic-pilot` feature: made-up tools can be installed into the manager's own folder after preview + explicit consent. `jq-pilot` (Windows only, non-default feature): one authorized installation allowance, bound by digest to one data directory; **never used**; no real third-party tool has been installed by this app. | `src-tauri/Cargo.toml` features; `install_flow.rs`; `jq_pilot.rs`, `pilot_allowance.rs`; `docs/phase-3d6/REMAINING-STEPS.tr.md` |
| **Agent integration** | **Missing.** The app reads no agent session and writes no agent configuration (`settings.json`, `CLAUDE.md`, plugin lists are never modified). Discovery can *observe* some files read-only. | README "Henüz yapamadıkları"; `docs/BRANCH-GOVERNANCE.md` ("no agent config mutations") |
| **2-PC sync** | **Missing.** No sync code, no cloud, no account. | README "Cihazlar arası eşitleme … yok" |
| **Prompt Intelligence — Issue #1** | **Missing** (OPEN). Only a placeholder list page. | `src/pages/Prompts.tsx`; Issue #1 |
| **Contextual agent triggers — Issue #2** | **Partial** (OPEN). A rule engine and a preview page exist; no hook into a real agent session, no live trigger. | `recommend.rs`; `src/pages/Recommendations.tsx`; Issue #2 |
| **Issue #4 (suggestion retention)** | **Code uses a project-scoped query; test coverage exists; Issue stays OPEN.** `prune_log` ranks *this project's* rows newest-first (`WHERE project_id = ?1 … ORDER BY rowid DESC LIMIT ?2`) instead of subtracting from the global `rowid`. Four dedicated Rust tests: `closure_tests.rs` 186, 209, 232, 247 (`issue_4_*`). This inventory did not run them; they run in CI's `cargo test` for the pushed SHA. Reading a comment and the SQL is not closure evidence. | `recommend.rs` 451–463 |
| **Issue #14 (cleanup substituted-file gate)** | **OPEN, untouched.** | Issue #14; no Rust file changed in PRODUCT-01 |
| **Issue #25 (WebView2 outbound HTTPS / profile outside isolated root)** | **OPEN, untouched.** The offline-preflight harness that was meant to address it is **deferred, not passed** (see below). | Issue #25 |
| **Later hardening** | Not started: release signing / notarization, auto-update, installer, independent reviewed recipes for real tools, an OS-enforced isolation boundary for any real-tool GUI test, a real-Mac GUI check. | README "Desteklenen ve doğrulanmamış platformlar" |

## 3. Offline-preflight harness status (deferred)

The Windows PowerShell harness under the git-ignored `src-tauri/target/offline-preflight/` (Revisions 1–5) is **DEFERRED, NOT PASSED**. Revision 4 parsed in PowerShell 7 and 5.1 but its `-SelfTestPure` run FAILED (205 checks, 2 failed); Revision 5 is a static patch that was never parsed or run. The Rev 5 review package was left in place, unexecuted and unedited. No claim of isolation, offline GUI proof or sandboxing is made anywhere in this product phase.

## 4. Safety boundaries preserved by this phase

No file under `src-tauri/` and no `docs/phase-3d*` file changed; `jq_pilot.rs`, `pilot_allowance.rs`, the armed digest, permissions, capabilities and feature flags are byte-identical to the baseline. The application was not started; no WebView2 window, no port 9222, no `--features jq-pilot` local run, no installation, update, or agent configuration write took place.
