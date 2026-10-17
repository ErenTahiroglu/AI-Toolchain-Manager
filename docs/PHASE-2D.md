# Phase 2D — Installation planning, portability and passive-security foundations

Baseline `de6ddd10ebb20d74510717180cc685c5aa2c4a1c` (accepted Phase 2C closure; Red Team follow-up Issue #10). **Nothing is installed, downloaded, executed, updated or sent anywhere**, no agent configuration or package manager is touched, no Windows security setting is changed, and the production authenticated-reference list is still **empty**. The deliverable is a read-only installation-plan preview.

## 1. Issue #10

### RT-2C-R1 — universal Mach-O architecture (validation policy v3)

* **Root cause.** `sniff` called any `CA FE BA BE` file with 1–8 entries a `MachOUniversal` and `consistent_with` accepted it for every `macos-*` target, so an x86_64-only universal file (or a header with only a plausible count) could be labelled compatible with Apple Silicon.
* **Fix.** The container magic no longer names a format. `sniff_reader` takes the leading bytes, the real file length and **the same opened handle** and:
  * parses the table in all four variants (32/64-bit entries, big/little-endian fields) from the leading 4 KiB — at most 8 entries, table fully inside the bytes seen;
  * rejects a count of 0 or above 8, a truncated table, a slice that starts inside the table, extends past the end of the file, is shorter than a 64-bit Mach-O header, overlaps another slice, or repeats a CPU;
  * reads 8 bytes at each slice offset from the same handle (bounded: ≤ 8 reads) and requires a little-endian 64-bit Mach-O magic whose CPU equals the table's claim and is arm64 or x86_64 — a 32-bit, other-CPU or disagreeing slice, or a failed read, gives `Unrecognised`;
  * names the result by what was verified: `macho-universal-arm64`, `macho-universal-x64` (Rosetta-only; **not** native arm64), `macho-universal-arm64-x64`.
  `consistent_with` now uses per-format architecture lists, so an x86_64-only universal file fits `macos-x64` only. The old label `macho-universal` is kept as a read-only legacy vocabulary word so older stored rows still parse; it is never produced, has no role and fits no target. `POLICY_VERSION` is 3, so stored v2 verdicts are invalidated when read. The size/time change check of the hasher covers the slice reads too.
* **Tests (synthetic bytes only).** arm64-only, x86_64-only and both-architecture universals in all four container variants; a table of 18 malformed variants (zero/absurd/oversized count, truncated table, offsets/sizes outside the file, overlap with the table or another slice, tiny slice, duplicate CPU, missing slice magic, disagreeing CPU, i386 slice, 32-bit slice magic, file shorter than declared); a failing reader; a Java class file (same magic). An x86_64-only universal reference never binds on `macos-arm64`; the legacy label binds nothing. No real tool validates: the production list is empty (test asserted).

### RT-2C-R2 — README

The root README describes the current application (supported/unverified platforms, what works, what does not, local development, security limits, evidence and links). The old Phase 0 text is preserved unchanged as [PHASE-0-OVERVIEW.md](PHASE-0-OVERVIEW.md). Nothing in it claims release readiness.

## 2. Installation-plan engine (`src-tauri/src/plan.rs`)

A plan is **reviewable data**: a pure function of (catalog entry, request, host facts, stored observations). The module has no process, network, file-write or SQL-write code (a test searches its source for such APIs), and `plan_install` is the only command that exposes it. The result always has `state = "preview_only"`, `executable = executed = modifiesAnything = false`, `eligibility.status = "blocked"`, `canExecute = false`, `rollback.available = false`. `InstallPlan::check_invariants` / `parse_plan` reject any plan (for example one read back from JSON) that claims otherwise, carries another `schemaVersion`, an altered statement or a step text that is not from the fixed vocabulary. The plan id is a SHA-256 over the canonical content (no timestamps), so equal inputs give equal plans.

### Closed vocabulary of operation kinds

Exactly the seven operations of `schemas/install-recipe.schema.json` (tested equal to the schema): `fetch-release-artifact`, `verify-artifact-digest`, `stage-private-copy`, `activate-staged-artifact`, `install-pinned-package`, `register-native-plugin`, `write-managed-instruction-block`. A step carries a **typed** schema-shaped operation only when every parameter is known and valid; otherwise `typed` is `null` and `missingParameters` names what a recipe would need (`expectedSha256`, `packageName`, `version`, `pluginId`). The self-check emits plans and a Python contract test validates every typed operation against the recipe schema.

| Catalog install method | Planned steps | Notes |
| --- | --- | --- |
| `native-plugin-manager` | `register-native-plugin` | needs a schema-valid plugin id (catalog ids with `@` are not); local-cli surface refused |
| `verified-release-archive` | fetch → verify digest → stage → activate (version switch) | verify step untyped while the catalog has no digest |
| `homebrew` (macOS), `winget` (Windows), `uv-tool`, `pipx`, `go-install` | `install-pinned-package` | package name and version are never taken from a README; always untyped today |
| `skills-cli`, `pip`, `docker`, `manual-instructions`, `unknown`, anything else | none | named as unsupported; no command is guessed or translated |

`write-managed-instruction-block` is part of the vocabulary but no catalog method produces it yet.

### What a plan explains

Tool identity/source and review state; platform, agent surface, scope, the schema's recipe target (if one exists) and catalog compatibility (publisher claim); install method; prerequisites with their source (`observed_scan` / `publisher_declared` / `not_checked`); ordered steps with network use, write areas (closed identifiers, no paths), administrator need (`no` / `maybe`) and reversibility; permissions; network destinations; telemetry and data/credential disclosure (from the record's closed words); disk usage (unknown unless known); configuration areas that would change; conflicts (existing installation, copies on PATH, conflicting version or identity records, unknown agent session); uninstall implications; rollback (always **not available** — design intent is stated separately); unknowns.

### Why every plan is blocked

`no_approved_recipe` is always present: no recipe store, approval flow or executor exists. Independent blockers are listed as well: `metadata_unreviewed`, `license_not_accepted`, `source_not_pinned`, `target_not_in_recipe_schema` (the schema cannot express local-cli targets), `compatibility_not_verified` / `unsupported_combination`, `different_computer_platform` / `unsupported_host`, `no_supported_method`, `method_not_declared`, `method_not_for_platform`, `method_not_for_surface`, `no_artifact_digest`, `package_name_unknown`, `version_not_pinned`, `agent_not_confirmed`, `active_session_unknown`, `prerequisites_unverified`, `execution_policy_blocked`, `entry_quarantined`, `entry_removed`.

### Untrusted text

Install-method names are matched against the compiled table only; malformed names are not even carried. Dependency lines from a record are sanitised, length-capped and shown **as the publisher's claim** (`declaredDependencies`, prerequisites with state `unknown`); they never become a step, argument, path or package name. Tests inject `curl … | sh`, `rm -rf`, encoded PowerShell, hostile pins and plugin ids (`../../x`) and assert that nothing reaches a typed operation, a step text or a write label.

## 3. User interface ("Kurulum Planı")

Choose a catalog entry, platform (Windows x64 / macOS Apple Silicon; defaults to this computer), agent surface, scope and (after the first preview) a method. The page shows a permanent "ÖNİZLEME" banner, then eligibility ("Bu plan uygulanamaz" + every reason with its code), warnings, target, prerequisites, steps headed **"UYGULANMAYACAK"** phrased in the conditional ("indirilecekti"), configuration, permissions/network/telemetry/data, conflicts, uninstall and rollback ("Geri alma: yok."), publisher claims, unknowns. Changing any selection removes the previous plan. There is no install/apply/approve/run button, no green badge, no path and no wording that reads as completed. An error (for example an invalid request) shows an error and no plan.

## 4. Portability

The planner and the discovery engine take their environment as input (home folder, PATH entries, platform label, host facts), so tests describe other machines instead of using the development machine. Display text carries `~`-labels, closed identifiers and positions, never account names, disk letters or install roots.

| Scenario | Evidence |
| --- | --- |
| Different account names (`alice`, `Ayşe Çelik`, `bob-the-builder 2`), install roots and nesting | same situation → byte-identical plan (same plan id); neither account name nor root appears in a plan |
| Missing tools / agent / scan | agent prerequisite `unknown` without a scan, `unmet` after a scan that did not find it, `met` only when seen |
| No administrator rights | `elevated = false/true/unknown` → `admin_may_be_needed` / `elevated_session` / `admin_unknown` warnings; nothing ever requests elevation |
| Missing or malformed agent configuration | `unknown` / `unmet`; the file is never repaired or rewritten (byte-compared) |
| Unsupported host or platform request | `unsupported_host`; `linux-x64`, `windows-arm64`, unknown surface/scope/method refused with a fixed message that does not echo input |
| Different project folders | project scope writes the project's own settings area; no folder name appears |
| Offline | building a plan has no network input; `usedToBuildPreview = false`; future network use is stated per step |
| Read-only / inaccessible | a `query_only` database connection still builds plans; a non-directory data folder is a clean error; missing home and PATH folders do not stop planning |
| Existing installations, copies, version/architecture mismatch | conflicts (`existing_installation_found`, `existing_installation_other_platform`, `multiple_copies_found`, `conflicting_version_records`, `conflicting_identity_records`); nothing is overwritten and the synthetic machine is byte-identical afterwards |

Application data is stored in the operating system's per-user app-data folder chosen by Tauri (or `ATM_DATA_DIR`); nothing in this phase hard-codes a user name or drive letter.

### Minimum requirements versus what was actually tested

| | Stated requirement (from Tauri/WebView2 documentation, not tested here) | Actually exercised |
| --- | --- | --- |
| Windows | Windows 10 or 11, x64, WebView2 runtime | Windows 11 Pro x64 (developer machine): all tests, debug build, WebView2 GUI smoke. GitHub Actions `windows-latest`: build + tests + headless self-check |
| macOS | a macOS release supported by Tauri 2 / WKWebView, Apple Silicon | **GitHub Actions `macos-14` (arm64) only**: build, tests, headless self-check. No real Mac GUI test. CI is not a substitute |

No claim of universal compatibility is made. Intel macOS and Linux are not targets.

## 5. Security and data

All earlier protections are unchanged. New command `plan_install` is listed in `build.rs`, `capabilities/default.json` and the health allow-list together (a unit test keeps them equal). No migration was needed (database version stays 8): preferences, observations, history, backups and the audit chain are untouched, and planning writes nothing (every table is dumped before and after, the audit chain is verified, and a `query_only` connection can still plan).

## 6. Remaining requirements before real installation or distribution

1. An approved-recipe store with the schema's gates (pinned commit, artifact URL/SHA-256, signature state, license review, staging evidence, user approval event bound to the recipe digest) and its own review.
2. A reviewed, authenticated reference-digest or signature source (key pinning, rotation, revocation); until then no tool is verified.
3. A typed executor with process-tree/job-object control, environment isolation, atomic version switch, backup/rollback of every touched agent file and an adversarial test suite; active-session detection for Claude Code and Antigravity.
4. Per-package-manager design (package names/versions, receipts, record-to-file linkage).
5. Signed/notarised packaging, an update channel with a pinned key, and a Windows release build validated outside CI.
6. A manual macOS pass on a real Apple Silicon Mac and an independent review, then a separate explicit authorization.
