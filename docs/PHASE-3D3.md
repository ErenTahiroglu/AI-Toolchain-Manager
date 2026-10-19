# Phase 3D-3 — Final Windows jq pilot readiness (no installation)

Baseline `4b283d45f2697de4acd7d233edf4ff3598e46f45`. **jq was not downloaded, copied, opened, installed, executed, updated or removed. No real third-party tool was touched. No Windows Hello key was created. The production recipe list, the authenticated-reference list and the new evidence list are empty; the enablement list is empty; the release build can install nothing.** `main` is untouched. Issue #14 stays open.

| Topic | Result | Document |
| --- | --- | --- |
| **Issue #22** — ignored audit errors | closed: mandatory pre-installation row; missing outcome row → `needs_attention` (never a plain success); read-back after the operation; reconciliation from receipt + journal; the compiler now refuses an ignored audit result (`#[must_use]`) | [AUDIT-INTEGRITY](phase-3d3/AUDIT-INTEGRITY.md) |
| **Source evidence** | `source_origin_unverified` disappears only with a substantiating, compiled-in record; a bare `sourceAttested: true` is refused for any real recipe; generic, names no tool | `src-tauri/src/attestation.rs`, `phase-3d3/evidence/…source-attestation.json` |
| **Licence for the local use** | `LICENSE_PASS_LOCAL_USE` for one local, non-redistributing use of one exact file; new state `accepted_local_use` that can never satisfy a redistribution check; Q1–Q3/C1–C5 evaluated; `LICENSE_CONDITIONAL` for everything else | [LICENSE-LOCAL-USE-REVIEW](phase-3d3/LICENSE-LOCAL-USE-REVIEW.md) |
| **Candidate recipe + provider** | revision 2, no network, review still `draft`; retained-file provider (one fixed folder, read-only, no link followed); real-recipe structure exercised end to end with fixture bytes; explicit enablement list | `phase-3d3/jq-1.8.2-windows-x64.recipe.candidate.json`, `artifact_provider.rs` |
| **Issue #14 limited pilot** | single-machine path, race handling, read-back, inventories | [ISSUE-14-PILOT-DECISION](phase-3d3/ISSUE-14-PILOT-DECISION.md) |
| **Smart App Control** | one test binary blocked → recorded as BLOCKED / NOT RUN; nothing changed, no comment edits to get past it | [SMART-APP-CONTROL-RECORD](phase-3d3/SMART-APP-CONTROL-RECORD.md) |
| **Real-window evidence** | 80 checks (two phases), inventories, probe | [GUI-EVIDENCE](phase-3d3/GUI-EVIDENCE.md) |
| **Decision** | **NO-GO today; GO-candidate for exactly one local installation-only pilot after B1–B3** | [READINESS-DECISION](phase-3d3/READINESS-DECISION.md), [PILOT-PROCEDURE](phase-3d3/PILOT-PROCEDURE.md), [AUTHORIZATION-REQUEST.tr](phase-3d3/AUTHORIZATION-REQUEST.tr.md) |
| Intermittent test failure | seen once on a fresh binary, not reproduced in 11 executions, not explained | [INTERMITTENT-TEST-NOTE](phase-3d3/INTERMITTENT-TEST-NOTE.md) |

## Code changes in one place

`attestation.rs` (new: evidence records and policy), `artifact_provider.rs` (new), `recipe.rs` (`problems_with` / `validate_with`; the licence state `accepted_local_use`), `plan.rs` (`build_plan_with_evidence`), `lifecycle.rs` (`PilotRecipe`, the enablement check; the path-based engine still passes an empty list), `adapter.rs` (`with_pilot_gate`, `committed_receipts`; a receipt of a real recipe says `synthetic: false`), `install_flow.rs` (audit honesty, read-back, reconciliation, enablement and notice checks), UI (`needs_attention` outcome, audit note, recovery text, local-use licence wording), `schemas/reviewed-recipe.schema.json` (two new enum values). One earlier test assertion had to follow the code and was **rewritten, not weakened** (`recipe_tests.rs` and `tests/test_phase3d2_contract.py` pinned the old text of the synthetic-only check; they now pin the new text "synthetic OR listed in the empty enablement list"). Every other earlier test is unchanged (the UI test double gained the new response fields, which the new types require).

## Limitations stated up front

Fixture bytes only; the real jq, the retained file, a real `.exe` write, a real SAC/Defender reaction to it, a second computer and macOS are untested; the in-app confirmation is not authentication; the legal reading is an engineering evaluation resting on one stated assumption; the first online verification of Request A contacted an unlisted host (recorded, conditionally accepted, not repeated); an intermittent test failure was seen once and is unexplained.
