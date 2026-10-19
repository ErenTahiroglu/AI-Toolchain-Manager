# Independent review package — jq 1.8.2 / Windows x64 candidate recipe (Phase 3D-3B, Issue #23, blocker B1)

**What this is.** Everything an independent reviewer needs to review *one exact recipe* and to reproduce its staging checks on synthetic fixture bytes. **What this is not:** a review. No reviewer identity, signature, date or independent approval appears anywhere; the recipe's review block is exactly as authored (`draft`, reviewer `none-yet`, no staging evidence), and the machine-readable [`review-package.json`](review-package.json) (added in the commit *after* the candidate commit it describes) records evidence **and its class**. Nothing is activated: the production recipe, reference, evidence and enablement lists are empty and a release build can install nothing. jq was neither downloaded, copied, opened, installed nor executed.

## 1. The offline source contract (decision)

The candidate's first step is still `fetch-release-artifact`, because the install-recipe contract has exactly seven operation kinds and a new kind would change the schema, the planner, the validator and every test for tools that have nothing to do with this pilot. Instead:

| Question | Answer | Evidence |
| --- | --- | --- |
| Can that step invoke a network? | **No.** The application has no network client (no http crate among its direct dependencies; none of its production code uses a socket or an http library; the window may call only its own commands and reach only its own IPC), and **nothing interprets recipe `steps`**: only the validator and the plan builder read them; the adapter, the provider and the workflow never mention them. A plan's network use comes from the recipe's `network` list alone. | `attestation::tests::nothing_in_the_application_interprets_recipe_steps_as_a_command_and_it_has_no_network_client` |
| Is the offline source then merely implied? | **No — it is part of the reviewed recipe.** `artifact.origin: "local_retained_file"` is a new *optional* field: absent for every existing recipe (digest-neutral, tested), present in the candidate (so it **changes the candidate's digest**; no earlier review claim can apply). The validator enforces it: a real recipe must declare either download destinations or the local origin, never both, never neither; with the local origin there is no network list and no `network_download` permission; the local-use licence record covers only a recipe that declares it. | `recipe::tests::a_real_recipe_must_say_where_its_bytes_come_from_and_the_two_ways_never_mix`, `…the_origin_field_changes_the_digest_when_present…`, `attestation::tests::local_use_acceptance_needs_a_covering_record…` |
| What does the person read? | The **plan** (step 1: *"Dosya, bu bilgisayarda daha önce edinilip doğrulanmış YEREL bir dosyadan okunacaktı (indirme yok; ağ kullanılmaz)"*; plan page label *"Saklanan yerel dosyayı oku (indirme yok)"*) and the **installation preview** (row *"Dosyanın kaynağı"*: one local file, read-only, nothing downloaded, size/format/SHA-256 re-verified immediately before use). The statement is part of the preview digest the confirmation is bound to. | `attestation::tests::the_plan_of_the_candidate_says…`, `install_flow::tests::the_preview_states_where_the_bytes_come_from…`, UI tests, real-window check |
| Unrelated tools? | Unchanged: their plans keep the old wording byte for byte (no `artifactOrigin` key), synthetic recipes need no origin. | same plan test, last block |

## 2. What to review (and what is in the package)

| # | Item | File | Digest / hash |
| --- | --- | --- | --- |
| 1 | The exact candidate recipe | `docs/phase-3d3/jq-1.8.2-windows-x64.recipe.candidate.json` | recipe digest in `review-package.json` and `docs/phase-3d3b/evidence/candidate-recipe-digest.json` (computed by Python **and** by the application: they agree) |
| 2 | The exact authenticated-reference entry | `docs/phase-3d3b/evidence/reference-entry.json` | file + canonical SHA-256 |
| 3 | The retained source-attestation record (repository, workflow, ref, commit, retained evidence hashes) | `docs/phase-3d3/evidence/jq-1.8.2-windows-x64.source-attestation.json` | file + canonical SHA-256; every retained evidence file is a tracked file with the recorded hash |
| 4 | The local-use licence record and the written review | `…local-use-license.json`, `docs/phase-3d3/LICENSE-LOCAL-USE-REVIEW.md` | file + canonical SHA-256 |
| 5 | The complete notice manifest and bundle | `docs/phase-3d2/notices/` | manifest + bundle SHA-256 (the licence record binds the bundle's) |
| 6 | The staging manifest (requirement → exact test identifiers) and the fixture | `docs/phase-3d3b/staging-manifest.json`, `staging-fixture.json` | file hashes; fixture SHA-256 rebuilt independently |
| 7 | The schema | `schemas/reviewed-recipe.schema.json` | file + canonical SHA-256 |

## 3. Reproduce (commands and expected output)

```
python tools/verify_review_package.py                 # expects: "allPassed": true  (every hash, the digest, the empty lists, the untouched review block)
python tools/staging_fixture.py print                 # expects the two SHA-256 values of docs/phase-3d3b/staging-fixture.json
python tools/run_staging.py --report my-report.json   # expects: "allPassed": true, counts.passed = every identifier of the manifest, 0 failed / missing / ignored
```

Individual groups: `cd src-tauri && cargo test --lib -- --exact <identifier>`; `python -m unittest tests.test_phase3d3b_contract`; `npx vitest run src/Install.test.tsx`.

| Requirement of Issue #23 | Staging check(s) |
| --- | --- |
| Recipe validation | S1 |
| Attestation evidence binding | S2 |
| Local-use licence binding | S3 |
| Plan generation and the source-origin gate | S4 (the candidate's plan has exactly one blocker, the user's approval, once reviewed *on a test copy*; without the evidence the `source_origin_unverified` blocker is back) |
| Wrong digest and wrong source rejection | S5 |
| Offline artifact provider | S6 |
| Preview and consent | S7 |
| Installation and removal (on the pinned fixture) | S8 |
| Filesystem effect inventory | S9 |
| Rollback and recovery | S10 |
| No network, PATH, agent settings or process execution | S11 |
| Production stays closed | S12 |

**Fixture identity.** `JQ-PILOT-STAGING-FIXTURE-1: a synthetic stand-in; it is not jq and not a program.` behind a minimal native header: `windows-x64` 233 bytes, SHA-256 `d89485933ba5975899fd3f6c36d0fd98295ff1877043992718cf019e29b61762`; `macos-arm64` 113 bytes, `07274dbddf732a2670fcc0c24f1bcde44b898f9fc5a42571d494056ddffc0990`. Rebuilt by `tools/staging_fixture.py` from the format description, without the application's code, and pinned by a Rust test.

**What cannot be reproduced with the available infrastructure** (stated, not faked): the real-window run needs WebView2 and a desktop session (`tools/gui_install_flow.ps1`, results in `docs/phase-3d3/evidence/gui` and `docs/phase-3d3b/evidence/gui`); no operating-system network isolation exists without administrator rights (the staging runner can be wrapped in `tools/egress_guard.py` with an empty allow-list, which is proxy-variable based and **not** a firewall); Smart App Control's reaction to a real executable is not part of this package.

**Developer-run supporting evidence in the repository** (class 1, regenerate it yourself rather than trusting it): `docs/phase-3d3b/evidence/egress-guard-staging-run.json` — the staging runner (50 checks: 48 Rust identifiers and 2 Python modules) run under `tools/egress_guard.py` with an **empty** allow-list: child exit 0, 0 hosts contacted, 0 incidents (proxy-variable based, not a firewall); and `docs/phase-3d3b/evidence/gui/` — the real-window run of this commit's source (executable SHA-256 `77beaeee1cdff6d95e5875bc98d57e560237437dc21206ecfac969c9663251f2`, a debug build with `--features synthetic-pilot`, synthetic bytes only): 75/75 checks in the first phase and 6/6 after a hard restart, including *the preview states that the bytes come from a LOCAL retained file and that nothing is downloaded* and the swapped-file, missing-file and wrong-content refusals of the retained-file provider; before/after inventories and the diff show only entries under the manager folder were added.

## 4. Evidence classes (never merged)

1. **Developer-generated** — the recipe, records, tests, fixture, tools, local runs and real-window runs. Written by the author of the code, so they show the code matches the author's understanding; they are **not** independent.
2. **CI-generated** — the exact-commit Windows and macOS jobs (including `tools/run_staging.py`); supporting evidence of *this commit*, pass/fail only.
3. **Independently reviewed by ChatGPT** — only what the repository itself records: the conditional acceptance of the Request A cryptographic evidence (Issue #19 closure; its network contact is **not** accepted) and the instruction to replace the sidecar by application-presented notices for the local-use pilot. **Nothing about this recipe has been reviewed yet.**
4. **Awaiting independent review** — the recipe, independent staging evidence, the licence record and its legal assumption, the attestation record and reference entry, the notice bundle's completeness, a reproduction on a machine the author does not control, the activation diff (B2) and the user's authorisation (B3).

## 5. How a reviewer records the result (nothing here is pre-filled)

The review block of the candidate stays `draft` until the reviewer, **after** reproducing the above, sets `state: reviewed`, their own `reviewerId`, `reviewedRecipeDigest` equal to the recipe digest in this package, and their own **independent** staging evidence (`independent: true` only for checks they ran themselves, each with its evidence digest). Any edit of `recipe` changes the digest and voids the review.

## 6. Known limitations and unresolved risks

Listed in full in `review-package.json` (`knownLimitations`); in short: developer-authored tests; Request A's cryptography is recorded but not redone here (its first online run contacted an unlisted host, recorded and not repeated); the fixture is not the real file, which was not opened; no OS-level network isolation; the licence decision is not legal advice and rests on one assumption; no `jq` catalog entry, the installed file is named `artifact.bin`, and the Phase 3D-0 digest-literal guard must be replaced in the activation change; one unexplained intermittent test failure (Phase 3D-3); Smart App Control may block a test binary (BLOCKED / NOT RUN, never counted as a pass); the macOS runner is not a Mac; Issue #14 stays open.
