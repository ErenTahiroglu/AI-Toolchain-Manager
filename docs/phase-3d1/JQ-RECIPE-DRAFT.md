# jq 1.8.2 / Windows x64 — DRAFT installation recipe (Phase 3D-1)

**State: DRAFT, BLOCKED, NON-EXECUTABLE.** The recipe is a data file ([`jq-1.8.2-windows-x64.recipe.draft.json`](jq-1.8.2-windows-x64.recipe.draft.json)) in the documentation folder. No production source reads or includes it, it is not in the compiled-in recipe list (`BUILT_IN_RECIPES_JSON` is still `[]`), it cannot be registered as a real recipe (only `synthetic-*` recipes can), the production authenticated-reference set is still empty, and `Adapter::production` still refuses everything. A test reads the file, parses it with the strict reviewed-recipe shape and proves each of those statements ([`recipe_tests.rs`](../../src-tauri/src/recipe_tests.rs), `the_jq_draft_*`).

## 1. What the draft declares

| Item | Declared value | Evidence / note |
| --- | --- | --- |
| Publisher / provenance | repository `jqlang/jq`, immutable commit `34f7186b86743a083a589741b6cea95293524108` (tag `jq-1.8.2`); `sourceAttested: false` | Request A session: Sigstore attestation verified by the maintained GitHub CLI (offline replay) and the supporting verifier, GPG signature by the pinned fingerprint — [evidence](../phase-3d0/evidence/request-a-session-20261018/). The recipe format has no verifier for real source attestations, so it **does not claim one**. |
| Version / platform | exactly `1.8.2`, `windows-x64`, surface `local-cli`, scope `user` | never "latest" |
| File | `jq-windows-amd64.exe`, **1 035 264 bytes**, SHA-256 `a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627`, digest scope `exact_file`, format `pe-x64` | equals the pinned value, the publisher checksum list and the downloaded bytes |
| Installation directory | **manager-owned only**: `<managed root>/versions/jq/1.8.2/artifact.bin` (the adapter's fixed name; **not** `.exe`), pointer `<managed root>/state/jq.json`, receipt `<managed root>/receipts/<txn>.json`, journal `<managed root>/journal/<txn>.json`, staging `<managed root>/staging/…`; the managed root is `<app data>/atm-managed/root-<id>`, owner-only | nothing outside it is created or changed |
| Permitted file changes | `manager_staging` create/remove, `manager_versions` create/remove, `manager_state` replace/remove — the validator's closed vocabulary | no other area exists |
| PATH | **no change** | no permission or step in the vocabulary can change it |
| Agent configuration | **no change** | `does_not_undo` names `agent_configuration`; no such step exists |
| Package managers / scripts | **none** | steps are exactly fetch → verify digest → stage private copy → activate staged artifact |
| Launching the program | **never**: the installed file is not given an executable name or bit by the adapter, and no step starts a process | executing jq is a separate, later authorisation |
| Network | one destination class, `github_release_download` | the download itself is a separate user authorisation |
| Update | **not offered** in the first pilot (`lifecycleActions`: install, uninstall) | |
| Effects | **install:** stage → verify digest → activate by atomic version switch → receipt. **uninstall:** move the version folder and pointer aside, verify, remove; receipt. **rollback:** on any failure the engine restores the previous pointer, removes staging and the new version folder, restores what uninstall had moved. | each step is journal-before-effect with file identity (Phase 3C); crash recovery rolls back or forward |
| Failure and recovery | any failure → rollback report; a crash leaves a lock that blocks new work until `recover()`; an ambiguous orphan (a file whose creation was never recorded) is kept and needs the owner's explicit resolution | Phase 3C tests; rehearsed with the durable trust path in [SYNTHETIC-DRY-RUN.md](SYNTHETIC-DRY-RUN.md) |
| User data | `data/jq/` is **never opened** by any transaction; uninstall keeps it (`kept: user_data`) | rehearsal checks byte identity of two tools' user data |
| Required approval | Phase 3A approval **and** a device approval: OS-signed with user presence, bound to this recipe digest, plan, the exact artifact digest and the effects digest, single use, expiring | [WINDOWS-HELLO-APPROVAL.md](WINDOWS-HELLO-APPROVAL.md) |
| Licence / notices | `license.state: "unknown"`, SPDX `MIT` (jq itself), evidence `R32` | [LICENSE-REVIEW-JQ-1.8.2-WINDOWS.md](LICENSE-REVIEW-JQ-1.8.2-WINDOWS.md): `LICENSE_CONDITIONAL`; the validator blocks on `license_not_accepted` |

## 2. Why it is blocked — exactly

`recipe::problems` on the draft returns, and the test pins, exactly:

`artifact_not_authenticated` — there is no authenticated reference entry for this digest (the production set is empty, and **no entry is created by this phase**: "release metadata is consistent" is not authentication);
`license_not_accepted` — `LICENSE_CONDITIONAL` is not acceptance;
`not_reviewed` — `review.state` is `draft`;
`staging_evidence` — no independent passing evidence.

With a hypothetical authenticated reference added *in a test only*, the remaining blockers are the last three; steps, target, permissions, filesystem effects, network, rollback contract, lifecycle actions, digest scope, format and source immutability already satisfy the validator. A claimed review does not help: the review is bound to the recipe digest, and licence and authentication still block.

## 3. What the Red Team would have to accept before a production entry exists

1. The Request A evidence (cryptographic part accepted; the historical network deviation stays recorded).
2. A separate code change adding **one** authenticated reference entry whose authenticity rests on stated, reviewable evidence — not on the publisher's own metadata alone (the GPG key and the attestation come from the same repository; see [JQ-PROVENANCE.md](../phase-3d0/JQ-PROVENANCE.md) §3 for the residual trust risks: mutable release metadata, same-origin signatures, key created 2025-07-31, attestation by the publisher's own workflow).
3. `LICENSE_PASS` (conditions C1–C5 of the licence review).
4. A reviewed recipe with independent staging evidence from a **real** Windows device (Issue #14 gate) — the rehearsal here is synthetic.
5. A real approval path accepted under one of options A/B/C in the Windows Hello document.
6. The user's separate explicit authorisation for the first real action.

## 4. Open design points the draft makes visible

* The installed name is `artifact.bin`: the program cannot be launched by name from the managed root without a separate (later) decision on how a user or agent would invoke it — which this phase does not provide on purpose (no PATH change, no execution).
* A notices file next to the installed version needs a second (non-executable) artifact or a manager-written file: the adapter installs exactly one artifact per tool today (licence review C2).
* Windows has a documented, accepted file-replacement window (Issue #14) that applies to the pointer file.
