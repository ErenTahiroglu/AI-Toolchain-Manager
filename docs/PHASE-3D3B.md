# Phase 3D-3B — Independent jq recipe review package and the offline-input contract

Baseline `753c62e123267baf5be2fc88ce84528041ad6eba`. Issue #23. **jq was not downloaded, copied, opened, installed, executed, updated or removed; no Windows Hello key; `main` untouched; the production recipe, reference, evidence and enablement lists are empty; a release build can install nothing.** This is a narrow B1 evidence-qualification task: B2 (activation) and B3 (the user's authorisation) were not begun, and **nothing is marked reviewed**.

| Topic | Result | Where |
| --- | --- | --- |
| **Offline source contract** | decision: keep the operation kind (a logical acquisition step that cannot invoke a network — proven by code evidence) and make the offline source an explicit, reviewed, digest-bound recipe field `artifact.origin = local_retained_file`; the validator, the plan and the installation preview all say so; unrelated tools are byte-for-byte unchanged | [REVIEW-PACKAGE §1](phase-3d3b/REVIEW-PACKAGE.md) |
| **Recipe digest** | the candidate changed (revision 2 now carries the origin), so its digest changed and every earlier review claim is void (there was none); the digest is computed independently by Python and by the application and they agree | `phase-3d3b/evidence/candidate-recipe-digest.json` |
| **Review package** | recipe + digest, exact reference entry, attestation record, licence record and review, notice manifest and bundle, staging manifest, fixture; hashes verified against tracked files and the candidate commit; four evidence classes never merged; limitations listed | [REVIEW-PACKAGE](phase-3d3b/REVIEW-PACKAGE.md), `review-package.json` (added after the candidate commit) |
| **Reproducible staging** | 12 requirements → 48 exact Rust test identifiers (plus the Python and frontend checks), one runner that fails on a missing/renamed/ignored/failing identifier and reports a blocked run as BLOCKED / NOT RUN; fixture identity pinned and rebuilt without the application | `staging-manifest.json`, `tools/run_staging.py`, `tools/staging_fixture.py`, `tools/verify_review_package.py` |
| **What stays unreproducible** | the real window (needs WebView2), OS-level network isolation, Smart App Control on a real executable — stated, not faked | manifest `notReproducibleInCi` |

## Code changes

`recipe.rs` (optional `artifact.origin`; `artifact_source_undeclared` / `artifact_origin_conflict` / `artifact_origin_unknown`), `plan.rs` (`artifact_origin`, step texts for the local origin, consistency checks; digest-neutral when absent), `attestation.rs` (the local-use licence covers only a recipe that declares the origin), `install_flow.rs` and the UI (the preview row *Dosyanın kaynağı*, the plan label), `schemas/reviewed-recipe.schema.json`, `pilot_supply.rs` (the retained synthetic tool declares the origin), tests and tools. No earlier test was weakened; the only changes to existing tests are test helpers that now declare the origin where they model the offline candidate.

## Limitations stated up front

Developer-authored tests are not independent evidence; the Request A cryptography is recorded, not redone; the fixture is not the real file; no OS-level network isolation; the licence decision rests on one legal assumption; B1, B2 and B3 remain open.
