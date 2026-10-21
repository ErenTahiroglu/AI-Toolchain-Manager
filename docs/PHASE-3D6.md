# Phase 3D-6 — B3: the authorized Windows jq pilot is bound to ONE data directory

Baseline `5d5feef35c5309124807e4b2ac5710757865c729` (Phase 3D-5: the one-installation allowance, accepted by the independent Red Team with its documented limitations; Issue #24 closed). Frozen `main` `68eda3f0407490fabccb517dcf822ab4818fc0bf` is untouched. Issue #14 stays open for any broader release.

**jq was NOT installed and NOT executed — not even `--version`. The retained jq file has not been opened, copied, hashed or placed anywhere an application can reach it (`jq-pilot-input` does not exist). No real preview was opened and no real confirmation was given. The user's authorization is bound here and NOT used; it is used only after the final independent Red Team review and a separate, explicit start.**

## The user's decision, and what this phase did with it

The user authorized ONE installation-only local jq pilot in ONE named Windows data directory (a directory that is not the repository's work tree). The repository does not contain that path: only its **scope identifier**, the digest below (a scope identifier, **not a secret** and not an authentication credential; it names *which* directory, it proves nothing about who is asking).

| Step | Result |
| --- | --- |
| Independent check of the path digest | `158c279e2151088c91783a5d788cc5877cc417cb23e0bff69dbeade99a7ee8a5` in **four** computations that share no code: `tools/pilot_data_dir_digest.py`, a Node script, PowerShell, and the application's own Rust `pilot_allowance::data_dir_digest` (the last through the local `ATM_B3_DIR_CHECK` test; the path is supplied by the person running it and never committed). All four equal the expected value; spelling variants (`/`, case, trailing separator) are equal; neighbours (parent, child, `…2`) differ. |
| Read-only inspection of the directory | exists; **empty** (0 entries, hidden and system entries included); a plain directory, not a junction/symbolic link/reparse point, and no ancestor is one; on local NTFS; outside the repository's work tree and not an ancestor of it; not a git work tree; its parent (the user's `repos` folder) is an ordinary directory. Nothing was created, deleted, moved or written. |
| Arming commit | one line: `B3_ARMED_DATA_DIR_SHA256: None → Some("158c279e…e8a5")`; `B3_UNLOCK` unchanged. |

A small preparatory commit came first (tests and doc comments only) so that the arming commit could be exactly one line: the B2/B3 tests and the Phase 3D-5 contract accepted *either* state; after the arming a final commit makes them require the armed state exactly, so a silent return to `None` or a different digest would fail.

## What "armed" means now

* In the ONE authorized directory the allowance can be previewed, confirmed and spent **once** (Phase 3D-5 mechanism, unchanged: durable record, receipts and audit trail as witnesses, compare-and-swap only, no reset path). Nothing about it changed.
* In **any other** data directory the real supply answers `pilot_wrong_data_directory` **before** any preview and before the provider is asked for a byte (the tests count provider requests: 0). That includes a spelling that is not the same path (an 8.3 short name, a path through a junction, a sub-folder, the parent, a copied database).
* `ATM_DATA_DIR` must therefore be set to that directory in its ordinary full spelling. If it is not, the pilot **fails closed** and tells so; it does not guess.
* Release builds are unaffected: the `jq-pilot` feature is non-default and Windows-only, no CI or release build enables it, and `Sources::production()` carries no recipe, reference, enablement entry or one-time rule (the headless self-check reports all zeros).

## What was changed, and what was not

Changed: the one constant in `src-tauri/src/jq_pilot.rs`; the tests that previously pinned the allowance to *inactive* (listed in [GUARD-TEST-CHANGES](phase-3d6/GUARD-TEST-CHANGES.md)); doc comments; new tests; documents. **Not changed:** the reviewed recipe and its candidate file, the authenticated reference, the source-attestation evidence, the local-use licence record, the notices manifest, the filesystem effects, the one-time allowance mechanism, the production adapter, `main`.

## Evidence (developer-generated; every run reported)

See [VERIFICATION](phase-3d6/VERIFICATION.md) for the exact commands, results, the Smart App Control record and the CI run of the final SHA. In short: nothing here is independent evidence; the independent reviewer's B1 limitations stand (Rust/frontend tests and the real window were **NOT independently run** by the reviewer); CI is not independent user-device testing.

## Remaining steps before the first authorized real installation

[REMAINING-STEPS.tr.md](phase-3d6/REMAINING-STEPS.tr.md) (preparation, not permission). Unexpected results end the pilot: **no automatic cleanup and no automatic retry**; an uncertain consumption stays `needs_attention` and is resolved by a reviewed human decision, never by code.
