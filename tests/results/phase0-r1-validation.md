# Phase 0 R1 Validation Results

**Run date:** 2026-10-10  
**Branch:** `phase0/architecture` (orphan; no commits)  
**Scope:** Documentation/data-contract fixtures only. No application code, installer, host agent, or target OS test was run.

## Executed Checks

| Command/check | Outcome |
| --- | --- |
| `python3 -m unittest discover -s tests -v` | **PASS**: 16 tests, 0 failures. Latest run: `Ran 16 tests in 0.083s` / `OK`. |
| Draft 2020-12 schema validation and `FormatChecker` for every seed record | **PASS**: 5 schemas meta-valid; 9 seed records; 0 validation errors. |
| `cmp` supplied adjudication against preserved copy | **PASS**: byte-identical. SHA-256 `e00033ca897287710446865625e83d3c267c0057930a9a427438717c58c06b61`. |
| Static local Markdown link/provenance checks in unittest | **PASS**: local package links resolve; all 8 R1 disposition rows present. |
| VS Code diagnostics on five schemas and contract test module | Five schema files: **no errors**. Pylance's selected Python 3.14 environment reports `jsonschema` unresolved; its installed-module inventory contained only `pip`. Terminal tests used the system Python bundled with Xcode Command Line Tools, where `jsonschema 4.23.0` was already available. `requirements-test.txt` declares this test-only dependency. No installation or interpreter switch occurred. A byte-identical original containing local paths is preserved outside the repository and excluded from this public snapshot. |
| `git diff --check` | Not used as evidence for untracked documents; the unittest performed local Markdown link/provenance checks. |

## Adversarial Behaviors Covered

- Arbitrary shell command in catalog install-method metadata is rejected.
- Catalog metadata cannot carry authorization or unbacked independent-compatibility/signature claims.
- Shell operation, arbitrary artifact host, enabled lifecycle script, and Claude/Antigravity target mismatch are rejected from install recipes.
- Recipe tampering, mismatched staging evidence, catalog pin/license/target mismatch, missing approval event, mismatched approval-event digest, and recipe digest mismatch block activation.
- Duplicate project/tool/agent/platform decision keys are rejected; active state with unknown compatibility or external conflict is rejected.
- `verify/passed` without independent evidence is rejected; self-report/AI cannot source a pass; raw output fields, invalid chain links, and forged rotation heads are rejected.
- Sync envelope rejects machine IDs/paths and secret fields, verifies the canonical payload digest, and handles concurrent profile/same-field changes as user conflicts. Offline/stale sync cannot authorize installation.
- Rejected prompts remain suppressed for ordinary upstream revisions; material capability changes, expired reminders, explicit reset events, and new tool identities follow distinct paths.

## Not Run / Not Proven

- No macOS ARM64 or Windows x64 app build or runtime test.
- No tool/plugin/MCP install, execution, update, rollback, or uninstall.
- No code signing, notarization, updater-key, or network behavior test.
- No private sync repository or credential; no live sync server.
- No OS-enforced sandbox, active-session detection, production config merge, or production redaction boundary test because no application exists.

All catalog records remain `unreviewed`; fixtures use synthetic names/digests and are not executable or compatibility evidence. R1 package awaits independent Red Team re-adjudication.
