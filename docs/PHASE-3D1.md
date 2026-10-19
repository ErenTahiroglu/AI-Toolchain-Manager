# Phase 3D-1 — Windows-first jq pilot: licensing, real approval and reviewed recipe readiness

Baseline `acaaafa9cb9214181e363d4d997f6f07140312ca`. **Nothing in this phase installs, downloads, starts or enrolls anything real.** No jq executable was re-downloaded, copied or run; the retained Request A bytes were read as data only (static analysis). No Windows Hello key was created or used, no prompt was shown, no credential changed. `Adapter::production` still refuses everything; the production recipe list and the authenticated-reference set are empty; there is no install button, no command and no UI for any of it. Issue #14 stays open.

| Topic | Result | Document |
| --- | --- | --- |
| jq licence and distribution notices | **`LICENSE_CONDITIONAL`** — permissive components; static bundle whose notices upstream does not ship; 3 open questions (Unicode tables in Oniguruma, which mingw-w64 runtime parts are linked, libgcc + GCC exception) | [phase-3d1/LICENSE-REVIEW-JQ-1.8.2-WINDOWS.md](phase-3d1/LICENSE-REVIEW-JQ-1.8.2-WINDOWS.md) + [evidence](phase-3d1/evidence/license/) + [draft notice bundle](phase-3d1/THIRD-PARTY-NOTICES.jq-1.8.2.windows-x64.DRAFT.txt) |
| Windows Hello availability | **Available** to an unsigned unpackaged desktop process on the development PC (read-only probe: supported = true, verifier available) — create/sign never run | [phase-3d1/WINDOWS-HELLO-APPROVAL.md](phase-3d1/WINDOWS-HELLO-APPROVAL.md) |
| Approval design | OS-signed, presence proven by the signing operation, bound to computer/recipe/plan/artifact/effects/action, expiring, single-use, durable ledger + hash-chained audit, explicit enabling, reversible; hardware claim **never assumed**: with today's code no real key satisfies the production policy (decision A/B/C needed) | same |
| Draft jq recipe | **DRAFT, BLOCKED, non-executable**: blocked by missing authenticated reference, licence acceptance, review and staging evidence | [phase-3d1/JQ-RECIPE-DRAFT.md](phase-3d1/JQ-RECIPE-DRAFT.md) |
| Issue #14 / hardware | H1–H4 gate the first real install; a same-host VM is supplementary evidence only | [phase-3d1/ISSUE-14-WINDOWS-PILOT-HARDWARE-GATE.md](phase-3d1/ISSUE-14-WINDOWS-PILOT-HARDWARE-GATE.md) |
| End-to-end synthetic dry run | Preflight → … → recovery with the durable trust path, fault injection at every point, unrelated files byte-identical; explicit list of what the production path has not exercised | [phase-3d1/SYNTHETIC-DRY-RUN.md](phase-3d1/SYNTHETIC-DRY-RUN.md) |
| Proposed user authorisations (Turkish, TEKLİF) | Talep B (Hello enrollment test, throw-away key) and Talep C (first real install of the retained, re-verified file, no execution) — **not granted** | [phase-3d1/FIRST-INSTALL-AUTHORIZATION-REQUEST.tr.md](phase-3d1/FIRST-INSTALL-AUTHORIZATION-REQUEST.tr.md) |

## What changed in the code (narrow)

* `os_hello.rs` (+ `windowsHelloProbe` in `--self-check`): the **read-only** availability probe; and, Windows-only, the real Hello backend — compiled, **never run**, referenced from no command.
* `os_approval.rs`: OS credential backend/verifier interfaces, `provision`/`deprovision`, `OsKeyStore` (signs only with presence), fakes.
* `approval_store.rs`: durable ledger, append-only hash-chained audit, approval-security setting (own SQLite file; no migration of the main database).
* `cng_verify.rs` (Windows): CNG RSA/SHA-256 verification with a public key; tested with ephemeral software keys.
* `device_trust.rs`: `KeyProtection` gains `OsReportedHardware` and `OsSoftware`; `KeyStore::sign_present` and `issue_with(PresenceSource::SignedByOs)`; the approval is verified against its own key before it is recorded; `TrustPolicy::pilot_candidate_os_reported_hardware()` (proposal, unused). `TrustPolicy::production()` accepts only a **verified** hardware key, which nothing can produce yet.
* Tests: `approval_store_tests.rs`, `os_approval_tests.rs`, the rehearsal in `adapter_tests.rs`, the draft-recipe tests in `recipe_tests.rs`, `tests/test_phase3d1_contract.py`.
* `Cargo.toml`: the `windows` crate (already a transitive dependency; build is offline) for WinRT, and two `windows-sys` features. Network use of the research: read-only `api.github.com` requests behind `tools/egress_guard.py` (0 incidents).

## Limitations stated up front

Windows Hello creation/signing/attestation/deletion, prompt parenting and the signature format are **unexercised**; the licence review is an engineering reading, not legal advice, with three open questions; the rehearsal injects faults in-process rather than cutting power; a real second PC, a real Mac and any real jq installation are absent; the macOS pilot has its own blockers (signed app, Secure Enclave, Gatekeeper, real Mac).

## Red Team closure (Issue #20)

The independent review gave a CONDITIONAL PASS and two pre-real-approval blockers, both closed in [PHASE-3D1-CLOSURE.md](PHASE-3D1-CLOSURE.md): a stored assurance label is never an authority (RT-3D1-01) and issuance/consumption/revocation commit atomically with their audit rows (RT-3D1-02). New: [WINDOWS-HELLO-THREAT-MODEL.md](phase-3d1/WINDOWS-HELLO-THREAT-MODEL.md) (Option B is a **disabled candidate**), [LICENSE-Q1-Q3-RESOLUTION-PROPOSAL.md](phase-3d1/LICENSE-Q1-Q3-RESOLUTION-PROPOSAL.md) (`LICENSE_CONDITIONAL` unchanged), [FLAKY-LIFECYCLE-TEST.md](phase-3d1/FLAKY-LIFECYCLE-TEST.md).

