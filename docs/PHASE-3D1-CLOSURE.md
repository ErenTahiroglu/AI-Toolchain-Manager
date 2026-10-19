# Phase 3D-1 — Red Team closure (Issue #20: RT-3D1-01, RT-3D1-02)

Baseline `898943bf3155e239289156004f279e4e238d510e`. **No Windows Hello key was created, used or deleted; no prompt was shown; no jq file was downloaded, installed or executed; Option B is a disabled candidate; the production adapter, recipe list and authenticated-reference set are unchanged.** Network use in this closure: read-only `api.github.com` requests behind `tools/egress_guard.py` (0 incidents) and the reading of public Microsoft/Unicode pages.

| Topic | Result | Where |
| --- | --- | --- |
| RT-3D1-01 stored assurance label | never an authority: effective assurance re-established from the live credential on open and before every signature; a stronger recorded label, changed evidence, another computer or an edited key record is rejected and audited; `HardwareVerified` needs an `AttestationVerifier` that no real backend has | `os_approval.rs`, `os_hello.rs`; [WINDOWS-HELLO-APPROVAL.md §9](phase-3d1/WINDOWS-HELLO-APPROVAL.md) |
| RT-3D1-02 approval and audit | issuance, consumption and revocation are each ONE `ApprovalStore` transaction with their journal row; no `authorized` row without the consumption; audit failure prevents both; journal-vs-ledger cross-check | `approval_store.rs`, `device_trust.rs` |
| Tests that fail on the old code | 3 mutations of the new code (label check removed, old audit-before-consume order, old issue-then-audit order) each fail the new tests (run during this closure) | `approval_store_tests.rs`, `os_approval_tests.rs` |
| Hello threat model, Option B | what Microsoft documents and does not; P1–P7 kept apart; same-user isolation assumed absent; test-only enrollment checklist | [phase-3d1/WINDOWS-HELLO-THREAT-MODEL.md](phase-3d1/WINDOWS-HELLO-THREAT-MODEL.md) |
| Licence Q1–Q3 | `LICENSE_CONDITIONAL` unchanged; conservative complete bundle (adds Unicode License V3; libssp confirmed linked); synthetic sidecar behaviour tested | [phase-3d1/LICENSE-Q1-Q3-RESOLUTION-PROPOSAL.md](phase-3d1/LICENSE-Q1-Q3-RESOLUTION-PROPOSAL.md) |
| Flaky lifecycle test | mechanism established (Windows delete-pending entries misread as unknown content; a post-condition raced the same state); engine tolerance added, no test weakened; 0 failures in 27 post-fix fresh-binary runs vs about 12 % before; antivirus **not** proven | [phase-3d1/FLAKY-LIFECYCLE-TEST.md](phase-3d1/FLAKY-LIFECYCLE-TEST.md) |

## Prerequisites for a SEPARATELY user-authorised, test-only Windows Hello enrollment (nothing below is granted)

1. The user's explicit written authorisation of exactly the test described in the threat-model checklist (*Talep B*), naming the throw-away key (`atm-test-<random>`), the isolated data folder, the two Hello prompts and the deletion.
2. Red Team acceptance of this closure and of the checklist; Option B stays disabled.
3. The research mode of the run: network observation before/after; any unexpected destination ends the test.
4. A written decision on what happens to a signature scheme or evidence-stability result that differs from the assumptions (PSS vs PKCS#1; chain digest stability).
5. Confirmation that no real jq, recipe, reference or production approval is involved.

## Still missing before ANY real jq installation

Everything listed in [PHASE-3D1.md](PHASE-3D1.md) and [FIRST-INSTALL-AUTHORIZATION-REQUEST.tr.md](phase-3d1/FIRST-INSTALL-AUTHORIZATION-REQUEST.tr.md) remains: Option A/B/C decision and the real-credential test; an authenticated reference entry and a reviewed recipe (separate reviewed changes); `LICENSE_PASS` (conditions C1–C5 and Q1–Q3 accepted in writing); Issue #14 (independent second physical Windows PC, H1–H4); a gated, reviewed code path that does not exist today; the user's separate explicit authorisation. The macOS pilot has its own blockers (signed app, Secure Enclave, Gatekeeper, a real Mac).
