# Phase 1B — Dynamic Tool Registry, Project Preferences and Contextual Recommendations

**Status:** implemented on `development`; awaiting independent Red Team review. Not merged to `main`. Phase 2 has not started.
**Architecture:** [ADR-0002](adr/0002-contextual-tool-suggestions.md), [Issue #2](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/2), [BRANCH-GOVERNANCE](BRANCH-GOVERNANCE.md), [ADR-0001](adr/0001-prompt-intelligence.md).
**Baseline:** Phase 1A accepted at `4c8437b…` (CI green); `development` had advanced legitimately to `d902c33` (ADR-0002, CI green) and was fast-forwarded, never reset.

Nothing in Phase 1B installs, downloads, executes, verifies or configures anything. Registering a repository is metadata only.

## What a user can do now

| Screen | Capability |
| --- | --- |
| Araç Kütüphanesi | Paste a GitHub link → offline check (strict URL rules, duplicate detection) → choose a type → add as an unreviewed *candidate*. Enable/disable an entry, remove it (two-step, "nothing is uninstalled"), restore it. Each card shows source, type, purpose, plugin identity, license, revision, execution policy, review/install/verification status, limitations and a platform/agent compatibility table. |
| Projeler | Three labelled sample projects plus real project **references** (alias + display name + private note; the folder is never opened). Per project, tool, agent surface and platform the user records *accepted / rejected / postponed / disabled*, with decision time and catalog revision, and can reset a decision. |
| Öneriler | A preview of the deterministic recommendation engine on labelled synthetic tasks (or text the user types). Recommendations explain themselves, say what would happen, and offer *enable for this project?* / *not now* / *reject*. Suppressed suggestions are listed with their reasons. |

## Catalog and classification

Four kinds: `executable_tool`, `agent_plugin`, `skill_collection`, `prompt_source`. The nine Phase 0 tools are preserved unchanged. Four candidates were added from public documentation read on 2026-10-17 (evidence R23–R26 in [RESEARCH-SOURCES](RESEARCH-SOURCES.md); records in `data/catalog-candidates-1b.json`, validated against the Phase 0 catalog schema):

| Entry | Kind | Notes |
| --- | --- | --- |
| `anthropic-frontend-design` | agent_plugin | Marketplace identity `frontend-design@claude-plugins-official`, source path `plugins/frontend-design` of `anthropics/claude-plugins-official`. Claude Code only: Claude Code on Windows/macOS is *vendor-declared*; Antigravity IDE is *unknown*. |
| `strix` | executable_tool | **Quarantined**, execution policy `blocked-until-scoped-authorization`. Active pentesting; needs Docker and an LLM key. Publisher's installer is a piped remote shell script and is recorded only as a warning, never as an install method. Can be shown for information, never "accepted", never run. |
| `agent-browser` | executable_tool | Native CLI is *vendor-declared* for Windows x64 and macOS ARM64 as a local CLI; Claude Code surface is *unknown*. |
| `agentic-awesome-skills` | skill_collection | Source of 2,700+ skills. Never installed as a whole; license is `review-required` (third-party skills carry their own, some GPL). |

No candidate is marked reviewed or tested: all are `unreviewed` (Strix `quarantined`), install state `not_managed`, verification `unknown`. GitHub metadata review is not functional verification.

## User-added repositories and remote metadata

Remote GitHub metadata retrieval is **not implemented** in Phase 1B. A user-added entry has `metadata_state = not-fetched`: only owner and name derived from the link are known; license, revision and compatibility are *unknown* and shown as such. This is the offline fallback and removes a whole class of network risk (the app still makes no network request, and the WebView has no network permission). If it is added later it must use minimal read-only access, a domain allowlist, timeouts, rate-limit handling and untrusted-content treatment.

URL rules (`src-tauri/src/github_url.rs`): only `https://github.com/{owner}/{repo}` (optional trailing `/` or single `.git`). Rejected: other schemes or hosts (look-alikes, `www.`, IP literals, `github.com@evil`, `raw.githubusercontent.com`), credentials, ports, queries, fragments, percent-encoding, backslashes, whitespace/control/non-ASCII characters, extra path segments (`/tree/…`, `/blob/install.sh`, `/..`), reserved GitHub site paths (`/settings/…`, `/orgs/…`), malformed owner/repo names and over-long input. Duplicates are detected case-insensitively against seeds and user entries.

## Project preferences

Key: `(project, tool, agent surface, platform)`, unique. Stored: decision, time, catalog revision, capability fingerprint, optional reminder. **Desired state and actual state are separate columns**: `actual_install_state` (`not_managed`), `actual_verification_state` (`unknown`), `observed_scope` (`unknown`) are fixed by SQL `CHECK` constraints in this phase.

* *Accepted* requires an enabled, non-quarantined entry with `vendor-declared` or `independently-tested` compatibility for that exact platform + surface. Otherwise it is refused with a reason (`PREFERENCE_NOT_ACTIVATABLE`, `ACTIVATION_BLOCKED`, `ENTRY_DISABLED`). Rejecting and postponing are always possible.
* A rejection survives version/commit bumps. It is lifted only by an explicit reset or by a *material* capability change (the fingerprint covers targets, install-method kinds, privilege/network claims and data flows, not versions).
* Every decision and reset is appended to `preference_history` (append-only). `reject`, `defer`, `reset_rejection` and `catalog_remove` are also schema-valid audit events (Phase 0 `audit-event.schema.json` v2); accepting/disabling are history-only because the schema defines no such event type and `approve` would imply a recipe approval.
* A real project is `reference` only. The note is local text, never read, logged, audited or synced (verified across all 13 tables).

## Recommendation rules (deterministic, zero model calls)

Gates in order: catalog state → compatibility → earlier decision → hypothetical availability → once-per-task de-duplication → per-task cap (2). Triggers use whole-token keyword rules (Turkish-folded) over the task text; file *names* only reinforce a weak intent and never create one.

| Task shape | Result |
| --- | --- |
| Design a React/TSX component (even with no CSS file); improve HTML/CSS layout | Frontend Design: `suggest` on Claude Code |
| SQL migration / API endpoint / database work in a repo that has CSS or TSX files | nothing (`no-ui-intent` explained) |
| Mixed "API endpoint + fix the button styling" | Frontend Design |
| Test the web app in a browser / e2e with a web context | agent-browser: `informational` on Claude Code (compatibility unknown) |
| "Find a reusable skill for …" | agentic-awesome-skills with at most 3 provenance-stamped candidates |
| Plan an authorized penetration test | Strix: `informational` only; "security", "XSS", "review headers" alone never trigger it |
| Already enabled / rejected / postponed / disabled; installed at project scope | silent |
| Installed globally (hypothetical) | silent but **disclosed**: it may be active in every project and project-level disabling is unverified |

Skill collections store identity only (id, name, ≤300-char description, tags, source commit, license). Search returns ≤3 candidates (known license first, license warning otherwise, overlap flagged with other catalog tools); a 10,000-skill cap and field caps keep context bounded. There is no import UI or network fetch in this phase; the index is empty in the running app and exercised by tests.

`src-tauri/src/gate.rs` states the only possible answer to "run/install": not available. For Strix it additionally defines the complete scoped-authorization an eventual phase must demand (exact target, non-production environment, duration, rate, budget, written permission reference, in-person confirmation); even a valid one is refused in Phase 1B.

## Database migration (schema v1 → v2)

Additive and transactional (`src-tauri/migrations/0002_catalog_registry.sql` + `catalog::migrate_phase1a_data`): new tables `catalog_entries`, `project_registry`, `project_preferences`, `preference_history`, `suggestion_log`, `skill_index`; Phase 1A data copied (nine tools with identical records, three synthetic projects, six decisions, `later` → `postponed`); legacy tables **kept but frozen** by triggers so two sources of truth cannot diverge; a consistent `VACUUM INTO` backup is written first; the audit chain and settings are untouched. A newer-than-known database is refused untouched; any failure rolls back and leaves the version unchanged.

## Evidence (exact commit is stated in the final report)

| Evidence | Result |
| --- | --- |
| Rust unit/integration tests (`cargo test`) | 92 passed, including 12 database tests |
| Migration from a **real Phase 1A database** written by commit `9918e9e` (`src-tauri/tests/fixtures/phase1a-9918e9e.sqlite.fixture`: v1, 9 tools, 3 projects, 6 decisions, 22-event chain) | Passed: all records preserved, chain verifies, backup usable, repeat starts idempotent, user reset not resurrected |
| Injected migration failure / later-step failure / corrupted legacy JSON / truncated or overwritten files | Passed: rolled back, version unchanged, no panic |
| Frontend tests, type check, build | 36 passed |
| Python contract tests | 41 passed with the built binary's self-check output (11 skip honestly without it): new candidates validate against the Phase 0 schema; Rust-produced `reject/defer/reset_rejection/catalog_remove` events validate against the Phase 0 audit schema; the built binary reproduces every synthetic scenario expectation |
| Windows debug GUI run, applying the real Phase 1A database | Passed: app migrated it itself; add/duplicate/misleading-link/remove/restore, project reference, decision save/refusals, once-per-task, no-suggestion and Strix-information flows were driven through the real WebView |
| Local Windows release runtime | **Unverified** (Smart App Control; see [WINDOWS-SMART-APP-CONTROL](WINDOWS-SMART-APP-CONTROL.md)). It also blocked a freshly rebuilt build script once during this phase; `cargo clean -p ai-toolchain-manager` and rebuilding was allowed. Release builds are verified in CI only. |

## Known limitations

* Offline only: no remote GitHub metadata, no live detection of what is installed in Claude Code or Antigravity. "Already installed / global" is a hypothetical input in the preview.
* Recommendation rules are keyword heuristics for English and Turkish; they will miss paraphrases and can misjudge unusual wording. They are a prototype, not a quality claim; no output-quality or token-saving claim is made.
* Antigravity IDE compatibility stays *unknown* for every new entry; Strix and agent-browser-on-Claude-Code are information only.
* No skill import UI; the skill index is empty in the running app.
* No Claude Code task-start integration (Phase 4); the preview is manual.
* macOS: only CI compilation, tests and the headless self-check exist; no macOS GUI run.
