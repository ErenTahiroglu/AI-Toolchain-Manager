# AGENT-01 — implementation, tests, evidence and the honest result of C5

Tracking issue: [#31](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/31) (stays open). Baseline: `development` = `51c4661cdd4dbf773cc01449195db2cf437240a5`; `main` = `68eda3f0407490fabccb517dcf822ab4818fc0bf` (untouched). **REAL_INSTALLATION_NO_GO. REAL_CONFIG_WRITE_NO_GO.** The commit SHA and the exact-SHA CI result are not written here (no self-referential hash, no documentation-only follow-up commit); they are in the hand-off reply and on Issue #31. "Local" below means this machine; CI is separate evidence.

## What was built

| Area | Change |
|---|---|
| Target | `codex-cli` is the fifth selectable agent surface, label "Codex CLI". It is a member of the Rust allowlist (`projects::SURFACES`), the TypeScript list (`TARGET_SURFACES`) and the database `CHECK`; the three are pinned to each other by tests. It is **not** added to the install-plan layer (`plan::SURFACES`), the install-recipe / audit schemas, the catalog schema, any catalog data file or the Tauri capability. |
| Registry | `src/agentHosts.ts`: a small declarative table, one row per surface (`clientHostId`, `agentSurface`, `configScope`, `configFormat`, `relativeTargetPath`, `scopeCondition`, `previewKind`, per-capability `documented` / `not-assessed`, `planSupported`, `evidenceIds`, `untouched`). The platform is *not* a field (it stays a separate dimension of the preference key); a provider brand never sets a field. `planSurfaces()` is what the Plan page uses, so the installer page does not offer Codex. |
| Planner | `src/configPlan.ts`: new status `TOML_PREVIEW_NOT_IMPLEMENTED` and a new `guidance` / `configFormat` on the plan. For a TOML target the planner stops after the catalog gates; it never calls the JSON parser, hashes or echoes text, composes a declaration or a diff. JSON hosts are byte-for-byte as before. |
| UI | `ConfigPreview.tsx`: for a TOML target no mode choice and no text field; the result shows status, path, format, scope, gates, never-touched files and the numbered static guide. `AgentGuide.tsx` (+ `agentGuide.ts`): the shared Turkish manual guide on the Projects page, with an exact-table name lookup. |
| Backend | `projects.rs`: `SURFACES` gains `codex-cli` (position 4, before `local-cli`). `audit_target` is unchanged, so a Codex event has `targetId: "unknown"` — no target id is invented. |
| Migration | `0010_codex_cli_surface.sql` (see below). The only schema change. |

### Migration 10 — the one unavoidable schema change
`project_preferences.agent_surface` has a `CHECK` list (migration 2). A saved Codex preference needs it widened; SQLite cannot alter a `CHECK`, so the table is rebuilt: create `project_preferences_v10` with identical columns/constraints except that one list, `INSERT … SELECT` all 12 columns in `rowid` order, drop the old table, rename. Nothing else is created, dropped, updated or deleted (a Python test lists the statements). Row preservation is checked three independent ways: Rust (in-memory v9 database with decisions on every historic surface → byte-for-byte compare of `project_preferences`, `preference_history`, `audit_events`, `catalog_entries`, `project_registry`; audit chain still verifies; no Codex row is created; the widened table accepts exactly `codex-cli`; a failing step rolls back to the untouched v9 table; a real file database is snapshot-then-migrated), Python `sqlite3` with foreign keys on against a schema built from the real migration files 1–9, and a column-and-DDL equality test against migration 2. The pre-migration `VACUUM INTO` snapshot (existing behaviour) is exercised too. `preference_history` (append-only, no `CHECK` on the surface), `discovery_observations` and everything else are untouched. It was *not* run against any real user database.

## Changed paths (exact)
Modified: `README.md`, `docs/RESEARCH-SOURCES.md` (R48–R54), `docs/config-01/FUTURE_WRITE_GATE.md` (§13), `src-tauri/src/db.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/projects.rs`, `src/components/ConfigPreview.tsx`, `src/configPlan.ts`, `src/labels.ts`, `src/pages/Plan.tsx`, `src/pages/Projects.tsx`, `src/styles.css`, `src/workspace.ts`.
New: `src-tauri/migrations/0010_codex_cli_surface.sql`, `src-tauri/src/agent01_tests.rs`, `src/agentHosts.ts`, `src/agentGuide.ts`, `src/components/AgentGuide.tsx`, `src/agentHosts.test.ts`, `src/configPlanCodex.test.ts`, `src/Agent01.test.tsx`, `tests/test_agent01_contract.py`, `docs/agent-01/` (these five files).
Not changed: `src-tauri/capabilities/`, `src-tauri/Cargo.toml`, `package*.json`, `.github/`, `schemas/`, `data/`, `src/configDeclarations.ts`, `src-tauri/src/plan.rs`, `recipe.rs`, `commands.rs`, any phase-3d tooling.

## Tests (local runs; CI is reported separately)

| Suite | Result |
|---|---|
| `npm run typecheck` | exit 0 |
| `npx vitest run` | 24 files, **625 passed** (579 before; 46 new: `agentHosts.test.ts` 13, `configPlanCodex.test.ts` 12, `Agent01.test.tsx` 21) |
| `npm run build` | exit 0 |
| `python -X utf8 -m unittest discover -s tests` | **346 tests OK**, 58 skipped (329 before; 17 new in `test_agent01_contract.py`; the skips are unchanged) |
| `cargo fmt --check` | exit 0 |
| `cargo clippy --all-targets -j 2 -- -D warnings` | exit 0 |
| `cargo test -j 2 --lib` | **778 passed, 0 failed, 2 ignored** (10 new in `agent01_tests.rs`) |
| Rust `main.rs` bin test target | **NOT RUN** (unchanged policy: a blocked or irrelevant target is reported, not worked around) |

Smart App Control record: an intermediate rebuild of the Rust test binary was blocked by Windows Smart App Control (`os error 4551`); the identical binary was re-run once and blocked again. It was recorded and **not** evaded (no setting touched, no renaming or copying). After the final source edit (a clippy fix in the test file) the binary built by the normal command ran. An earlier run of the *first* version of the Rust tests did execute and exposed five wrong assumptions in the tests themselves (they assumed an empty preference table, but the seeded database already holds the application's own sample decisions) — fixed to count relative to a baseline.

An observed flake: one full-suite vitest run once failed `PlaywrightChoice.test.tsx › more than one option can be opened independently` (a pre-existing, timing-sensitive component test); it passed alone twice and in the next two full runs. Not changed.

### What the new tests pin
Surface lists equal across Rust / TS / registry / migration; install-plan list unchanged; every historic entry × both OSes on Codex is `unknown` and "accepted" is refused by the backend (and unselectable in the matrix) while postpone/reject/reset work; unlisted spellings (`codex`, `Codex-CLI`, `codex-desktop`, `gemini-cli`, `deepseek`, `grok`, `groq` …) are refused by the backend and by target resolution; a Codex decision never appears under another surface and vice versa; the 22 historic identities and the data files have no Codex row; a Codex event has no invented audit target; health invariants unaffected; planner answers `TOML_PREVIEW_NOT_IMPLEMENTED` for every OS, mode, registry and text and never hashes/echoes text, secrets (`ghp_…`, `sk-…` sentinels) or `mcpServers`; TOML text into a JSON host fails closed as `INVALID_CURRENT_CONFIG`; hostile/oversized text cannot change a Codex answer; staleness covers surface/OS/entry/revision; Codex control has no field and no apply/save/copy/export/download button, makes no IPC call, clipboard write or network request; JSON controls unchanged; matrix: a postponed Codex wish is stored with exactly (project, entry, `codex-cli`, OS) and the Claude Code wish is untouched; hand-off to `codex-cli` opens Codex; review-queue treats Codex as its own four-field key; Plan page lacks Codex; guide: exact lookup (Grok ≠ Groq, Gemini CLI ≠ Gemini API, near-misses uncertain, unknown → `MANUAL_VERIFY`, no path/format/"compatible"/"installed" in an unknown answer), one text box, one button, no IPC beyond reads; the new modules contain no I/O, the capability file grants no filesystem/shell/network/clipboard/config permission, no TOML dependency exists.

### Mutation checks (one change at a time, SHA-256-verified restore) — 23 of 23 caught
Surface removed from the TS list; surface removed from the Rust list; Codex mapped onto the Claude JSON host; TOML target not recognised; pasted text hashed for TOML; an `npx -y` command in the guidance; Plan page offering every surface; registry marking Codex as an installer target; bare `gemini` classified as the Gemini API; Groq answered as Grok; paste field given to the TOML target; missing label; weakened no-universal-format sentence; fuzzy name matching; migration omitting `remind_at`; migration `CHECK` without `codex-cli`; migration silently re-deciding accepted rows; migration unregistered; scope condition losing the trust statement; a second (copy) button in the guide; guide unmounted; wrong Codex path; JSON wording leaking into the Codex plan. Thin spots: the three Rust-side mutants are caught through source-text pins (the vitest registry test reads `projects.rs`; the Python tests read the SQL and `db.rs`), not by running mutated Rust binaries.

## Observable UX
See `NEXT_PRODUCT_HANDOFF.md` ("What a user can now do"). Everything is Turkish, keyboard-reachable, announced through `role="status"` / focused result regions, and has no apply, save, copy, export or download control.

## No real write, no install — evidence
No Rust command, IPC route, Tauri permission, dependency or file API was added (a Python test and a vitest test pin the capability file, the command list and the module sources). Tests use in-memory SQLite, jsdom and synthetic strings with obviously fake sentinels. No Codex file (project or user), no real workspace, no GUI, WebView2, CDP 9222, jq pilot, offline harness or installer was touched. Network use was limited to reading public documentation pages and public registry metadata as page text (R48–R54); no package manager, `npx`, `codex`, `claude` or `gemini` process was run, and no key, account or credential was used.

## C5 — one bounded assessment: is there a qualified declaration for Playwright MCP on the Claude Code JSON host?
**Result: NO QUALIFIED DECLARATION. The production registry stays empty.**

Evidence gathered (R54 = npm registry metadata, plus the already registered R39 for the repository at commit `0c4107ac…`): `@playwright/mcp@0.0.83` exists; `dist-tags.latest` is `0.0.83` (so an exact `@0.0.83` pin is distinguishable from the publisher's unpinned `@latest`); integrity `sha512-oNcl+Ae2/IAjhfPeP46BfIkSakfmprY+aOtkv5MjrQ4lPav4/yNtPhL0iq8SlIM90oApWgBDUxaNKvktazUKOg==`; `gitHead` `f183dad4a52965583e3cc1d59b88cdc279e2e57d`; npm provenance (SLSA v1) and a GitHub-Actions trusted publisher are shown. Compatibility for Playwright MCP × Claude Code on both OSes is already `vendor-declared` in the catalog. Nothing was run, downloaded or connected.

Exact blockers:
1. **No independent reviewer exists.** The registry requires a review record (reviewer id, evidence ids, 40-hex `reviewedAgainst`) from an *independent* review. An AI-authored review is not that, and this phase may not approve installation. (Blocking by itself.)
2. **The exact pin does not pin what runs.** Version `0.0.83` depends on the exact **pre-release** pair `playwright@1.64.0-alpha-1790635538000` / `playwright-core@1.64.0-alpha-1790635538000` and ships no shrinkwrap (`_hasShrinkwrap: false`); `npx` resolves and fetches the dependency tree from the registry at every launch, and no integrity can be put in the config stanza. A stanza would look "version-pinned" while the executed tree is not.
3. **Flags and defaults at that release are unverified.** The repository README read in CATALOG-02 is from `main` commit `0c4107ac…`, not from the npm release's `gitHead` `f183dad4…`. The privilege-reducing options (isolated profile, origin allow-lists — the README itself says the latter are not a security boundary) therefore cannot be written into a stanza as verified.
4. **Unresolved privilege and data-disclosure questions** (from R39): a persistent browser profile by default (logged-in sessions), page-JavaScript evaluation, file upload, a tool titled "unsafe", an extension mode that attaches to an existing logged-in browser. A project `.mcp.json` entry is also loaded without a prompt in Claude Code's headless (`-p`) runs (R45). No decision about accepting that exists.

Honouring "avoid another broad research loop": the assessment stopped at these four findings; no further sources were pursued. What would unblock a first declaration is listed in `NEXT_PRODUCT_HANDOFF.md`.
