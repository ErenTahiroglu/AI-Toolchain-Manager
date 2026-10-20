# B2 — what the jq pilot configuration consists of, and why it can do nothing yet

Target, exactly: **jq 1.8.2 · windows-x64 · `jq-windows-amd64.exe` · 1,035,264 bytes · SHA-256 `a6fc67fe…c627` · source `jqlang/jq` @ `34f7186b86743a083a589741b6cea95293524108`** · recipe digest `9c7a020d…cca1`. Nothing below downloads, copies, opens or runs it.

## 1. The eight components (all in `src-tauri/src/jq_pilot.rs`, compiled only for tests and the `jq-pilot` feature)

| # | Component | Source (committed, read with `include_str!`) | Checked by |
| --- | --- | --- | --- |
| 1 | the exact **authenticated reference** (`jq`, executable, `pe-x64`, `windows-x64`, `1.8.2`, digest) | `docs/phase-3d3b/evidence/reference-entry.json` | `assemble`: every field must equal the recipe's; kind must be `executable` |
| 2 | the **bound source-attestation record** | `docs/phase-3d3/evidence/…source-attestation.json` | the existing 9-binding validator (`SourceAttestation::problems_for`) via `recipe::validate_with` |
| 3 | the **`LICENSE_PASS_LOCAL_USE`** record | `docs/phase-3d3/evidence/…local-use-license.json` | `LocalUseLicense::problems_for` (scope, decision, file, recipe covers only install/uninstall, no network, C1–C5, no blocking question) |
| 4 | the exact **reviewed recipe** | the candidate file, **unchanged**, plus the review block derived from the binding | `verify_binding` + `validate_with` |
| 5 | the **notices manifest and bundle** | `docs/phase-3d2/notices/` | bundle SHA-256 == licence record == manifest; byte count; `writtenIntoToolFolder: false` |
| 6 | the **catalog row `jq`** (the recipe's tool id) | seeded by `jq_pilot::seed_catalog` in the pilot build only (origin `user`, so the health check's count of `seed` rows is unchanged; the row says: metadata only, unreviewed, manual-only, not installed) | `get_entry`; a row with any other id never receives the recipe |
| 7 | the fixed, local, **read-only provider** | `artifact_provider::RetainedFileProvider` (unchanged), one entry, one folder `jq-pilot-input` inside the application's data folder | refuses links, other names, wrong size, wrong digest, changed content (re-checked before the preview and before the transaction) |
| 8 | the one **activation entry** (`PilotRecipe`: tool, version, platform, recipe digest, artifact digest) | built from the validated recipe | `PilotRecipe::allows` (exact match only) |

Any failed check returns a closed reason code and the supply becomes the **empty release supply**.

## 2. The B1 binding (what "reviewed" means here)

`verify_binding` ties the review block to the reviewer's own words. It refuses, each with its own code, a changed capture (`binding_capture_changed`), a capture that does not say what the binding claims (`binding_not_in_decision`: every cited hash and tree, "B1 LIMITED-PILOT ACCEPTED", "NOT independently run by ChatGPT", "B2 GO … synthetic testing only", "B3 real installation NO-GO"), a changed recipe (`binding_recipe_differs`), a review block that is not `reviewed` over this digest by the named reviewer (`binding_review_block`), staging evidence that is not exactly the reviewer's two published digests (`binding_evidence_not_the_reviewers` / `…_incomplete`), CI marked independent, or the not-independently-executed list weakened (`binding_overclaims`). Tests change each piece in turn.

## 3. The lock

```
pub const B3_UNLOCK: Option<&str> = None;
```

| Layer | While `None` |
| --- | --- |
| provider | `LockedProvider` answers `real_install_locked` and touches no file system entry |
| enablement list (`pilot_gate`) | empty → the service says `recipe_not_enabled`, nothing can be previewed or confirmed |
| adapter | `check_authority_gated` refuses a non-synthetic recipe that is not listed (`not_a_synthetic_recipe`) |
| application folder | building the supply creates no folder; the pilot build creates **no** input folder and places **no** file |

B3 changes the one constant to the exact recipe digest it authorizes, in its own reviewed commit; a wrong digest unlocks nothing (tested). There is no `allow_real_install`, no path, no environment variable, no command-line argument and no IPC field that can unlock it (tests scan the module and the IPC types).

## 4. What the person is shown (Section 6 of the task, mapped)

| Required | Preview field |
| --- | --- |
| exact source, version, platform, SHA-256 | `source`, `version`, `platformTarget`, `artifact` |
| local retained file, no download | `artifactOrigin = local_retained_file`, `originText`, `networkUsed = false` |
| local-use licence scope | `licenseScopeText` (*this computer only, not redistribution*) |
| notices and unresolved redistribution questions | `notices.components` (7), `notices.unresolved` (Q1–Q3 + the scope statement) |
| every allowed filesystem effect | `effects` (relative to the manager folder, none absolute) |
| explicitly excluded operations | `notDone` (no program run, no network, no PATH, no agent files, no elevation, no files outside the manager folder, no tool data, no SAC/Defender change) |
| rollback and recovery | `rollback` (*designed*, what it does and does not undo) |
| in-app consent is not Windows identity authentication | `consentStatement` |
| the installed name | `installedAsText`: **`artifact.bin`**, not a command named jq, not on PATH, never run |

All of it is part of the preview digest the confirmation is bound to.

## 5. The artifact name (`artifact.bin`)

The adapter's fixed installed name is accepted for this copy-only pilot **because the preview says so**; jq does not become a command, a shortcut or a PATH entry. Renaming or launching would be a different, separately reviewed pilot.

## 6. Windows filesystem plan for the future one-machine pilot (Issue #14 stays open)

Fail closed on a conflicting file or an unexpected identity change; read the final state back; unresolved ownership → `needs_attention`; never repair, adopt or delete what the manager did not create; preserve every unrelated file. Before/after inventories with `tools/fs_inventory.mjs` (read-only) of the dedicated data folder, `diff`-ed; any difference outside `atm-managed/` and the SQLite files stops the pilot. Smart App Control / Defender are not modified; if Windows blocks a test binary the result is recorded BLOCKED / NOT RUN and the binary is not rebuilt to get around it ([record](SMART-APP-CONTROL-RECORD.md)).

## 7. Guarantees that did not move

The path-based synthetic engine is unreachable; the WebView sends identifiers and one acknowledgement only (unknown fields are refused); the approval ledger / Windows Hello are untouched; no process is started and no socket is opened by the pilot module; `main` is untouched.
