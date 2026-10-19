# jq 1.8.2 / Windows x64: the minimum reviewable transition from the blocked draft to a future approved pilot (Phase 3D-2)

**Nothing is activated.** The production recipe list is `[]`, the authenticated-reference set is empty, `Adapter::production` is closed, the draft stays in `docs/phase-3d1/`, jq was neither downloaded, installed nor executed (the retained Request A bytes were not even opened in this phase). The two tests in `recipe_tests.rs` named `the_jq_pilot_transition_…` and `even_a_fully_valid_jq_recipe_…` replay every step below on **copies** and prove the list is exact.

Target: jq 1.8.2, Windows x64, file `jq-windows-amd64.exe`, 1,035,264 bytes, SHA-256 `a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627` (retained, verified by Request A; the digest appears as a literal only in this document, the draft JSON and the Python contract tests — never in a Rust source).

## 1. Revision 2 of the draft (the shape of the pilot)

The first pilot **copies the retained, re-verified file**; it does not download. Revision 2 therefore differs from the draft in exactly three fields: `revision: 2`, `recipeId: jq-1.8.2-windows-x64`, and **no network** (`permissions` without `network_download`, `network: []`). Everything else is the draft: exact version, exact file digest (`digestScope: exact_file`), fixed filesystem effects (manager staging / versions / state only), lifecycle `install` + `uninstall` (no update), rollback contract `designed` (never claimed verified), `doesNotUndo: tool_user_data, external_effects, agent_configuration`.

## 2. The five remaining blockers, in the order they can be closed

| # | Blocker (code) | What closes it | Who / evidence | Proven by |
| --- | --- | --- | --- | --- |
| 1 | `artifact_not_authenticated` | One entry in the compiled-in authenticated-reference list: `entry_id jq`, kind `executable`, format `pe-x64`, platform `windows-x64`, version `1.8.2`, the digest above, source label = the Request A evidence set. A separate, reviewed code change. | Red Team approval of the entry against `docs/phase-3d0/evidence/request-a-session-20261018/` (54 files, SHA listing) | test: with a hypothetical reference the blocker disappears |
| 2 | `license_not_accepted` | `license.state: accepted`, an SPDX expression covering the whole bundle (≤ 40 characters, e.g. `MIT AND BSD-2-Clause AND Unicode-3.0`) and an `evidenceId` that names a RESEARCH-SOURCES entry (`R…`) holding a written `LICENSE_PASS` that accepts C1–C5 and Q1–Q3 (see [NOTICES-DECISION](NOTICES-DECISION.md)). This changes the recipe digest. | Red Team / counsel | test: step 2 leaves exactly `not_reviewed`, `staging_evidence` |
| 3 | `not_reviewed` | A review record `reviewed` by an independent reviewer **after** step 2, over the recipe's final digest. A review recorded earlier is `changed_after_review`. | independent reviewer | test (including the "review never carries over" case) |
| 4 | `staging_evidence` | At least one independent, passed staging check with an evidence digest, and no failed one. Staging means: the Phase 3D-1 style rehearsal of this very recipe against the **synthetic** engine plus a recorded dry run of the preview; no jq is run. | independent reviewer | test: step 4 leaves no problem and `validate` succeeds |
| 5 | **plan blocker `source_origin_unverified`** (found in this phase) | After 1–4 the plan is still `blocked`: the recipe's commit is only *declared*, and no evidence kind exists through which a **real** recipe can have its source attested (`sourceAttested: true` on a real recipe is refused as `source_attestation_unverifiable`). Either (a) a reviewed real attestation kind built from the Request A evidence (signed tag → commit `34f7186b86743a083a589741b6cea95293524108`), or (b) a **written Red Team decision** that for an installation-only pilot the authenticated *file* digest is the identity and the source pin is informational, turning this blocker into a warning. | Red Team decision | test: `even_a_fully_valid_jq_recipe…` asserts the plan's blockers are exactly `user_approval_missing` + `source_origin_unverified` |

Fixed and **not** open: effects (see the preview of any recipe — `install_flow.rs::effects`, proven against the real file system at every injection point by `everything_the_preview_promises…`), rollback expectations (below), network (none).

## 3. Even after 1–5, nothing installs

* `RecipeRegistry::synthetic(…)` refuses a real recipe; only the compiled-in list may hold one (a separate reviewed change that keeps `BUILT_IN_RECIPES_JSON` honest).
* The adapter's authority check requires `recipe.synthetic` and a `synthetic-` tool id. A gated, reviewed code path that relaxes this for exactly one named recipe **does not exist** and is a separate phase.
* The artifact provider is empty in a release build. For the pilot a provider must read the retained file **without any path coming from the interface**. Proposal (not implemented): a fixed, manager-owned input folder into which the person copies the verified file by hand; Rust reads only that name, then re-verifies size, format and digest against the recipe and the authenticated reference before anything is previewed (`check_artifact_for`, already used). No file-dialog and no path in IPC.
* The user's separate, explicit authorisation of the first real installation (installation only; no execution).

## 4. Accurate rollback expectations (what the preview will say)

Rollback is **designed**, exercised only with synthetic files. It undoes: the active-version pointer (restored from the snapshot), the staging folder, the version folder this transaction created, a removed version (restored on a failed removal). It does **not** undo: the tool's own user data, effects outside the manager folder (there are none by design), agent configuration (never touched). Anything that no longer matches its journal record (foreign file, drift, an unprovable orphan) is **kept and reported**, never deleted or guessed; a crash in the create-to-identity window leaves an *ambiguous orphan* that needs a person (`failed_needs_attention`, tested for the stage file and the pointer temp file).
