# Windows Hello approval — threat model, Option B limits and the test-only enrollment checklist (Phase 3D-1 closure)

Status: **analysis only**. No key was created, no prompt was shown, no credential was touched. Option B ("accept the OS-reported hardware status") is a **provisional candidate for a future, strictly scoped Windows pilot**, selected by the Red Team; it is **not** activated: `TrustPolicy::pilot_candidate_os_reported_hardware()` exists, is unused by any production path, and a test pins that.

## 1. What the official documentation says (and does not say)

Read on 2026-10-19 (Microsoft Learn pages; page dates as served):

| Source | What it states | What it does **not** state |
| --- | --- | --- |
| [`KeyCredential.RequestSignAsync`](https://learn.microsoft.com/en-us/uwp/api/windows.security.credentials.keycredential.requestsignasync) (updated 2026-03-30) | "Prompts the user to cryptographically sign data using their key credential." One parameter: the data (`IBuffer`). | Anything about what the prompt shows. There is **no parameter** with which a caller can put its own description of the operation into the prompt. |
| [`KeyCredentialManager`](https://learn.microsoft.com/en-us/uwp/api/windows.security.credentials.keycredentialmanager) (updated 2026-09-30) | `RequestCreateAsync` / `OpenAsync` / `DeleteAsync` act on "a key credential for the current user **and application**"; keys are RSA 2048-bit; remarks say the signature format is **"PKCS #1 RSA PSS with SHA256"**; attestation data = key metadata + a certificate chain, to be validated by the relying party. A newer overload takes a `WindowId`. | What "application" means for a process **without package identity**; whether another process of the same user can open the same credential by name. The remarks' signature format conflicts with the PKCS#1 v1.5 assumption of the first implementation (see §6). |
| Microsoft Q&A, *Using C++/WinRT without MSIX packaging* (2023-07, an external-staff reply) | The API docs list `RequestCreateAsync` among APIs that need package identity, yet a console and an MFC app **without** identity called it successfully; the staff answer: "It seems to be a documentation issue." | Any statement on credential scope for unpackaged callers; the thread does not conclude. |
| This repository's probe (2026-10-19) | From an unsigned, unpackaged desktop process on the development PC: `IsSupportedAsync` = true, user-consent verifier available. | That a key can be created or used (never run). |

Consequence: **unpackaged use is observed to be possible on this PC, not documented as supported, and its credential scope is not documented.** The design therefore assumes the weakest case: *any process running as the same Windows user might be able to open and use a credential created by this application.*

## 2. Properties that must never be merged into one

| # | Property | What establishes it | Status in this repository |
| --- | --- | --- | --- |
| P1 | **Verified TPM attestation** | the program validates the attestation chain and statement for exactly this public key (`AttestationVerifier`) | **not implemented**; no real backend has a verifier; nothing real can be `HardwareVerified` (compile-time absence asserted by tests) |
| P2 | **OS-reported hardware status** | `GetAttestationAsync` = Success with a non-empty certificate chain, evidence bound to the key by digest | implemented as *evidence only*; yields at most `OsReportedHardware`; **not verified** |
| P3 | **Software-protected credential** | no or unbound attestation | `OsSoftware`; always refused |
| P4 | **An unauthenticated database label** | nothing: a local file anyone with the user's rights can edit | **never an authority** (RT-3D1-01): the effective assurance is re-established from the live credential; a stronger label is rejected and audited |
| P5 | **User presence during signing** | the OS prompt succeeded inside `RequestSignAsync` (a signature exists only then) | by design; **not exercised** against a real credential |
| P6 | **The user understood the exact target operation** | **nothing the OS provides**: the prompt is the OS's, with no caller text | **not established by any option**; mitigated only by the application showing the exact plan *immediately before* the prompt and by the signed payload binding the digests |
| P7 | **Isolation from other programs of the same user** | the OS credential scope, which is undocumented for unpackaged callers | **not established; assumed absent** |

A successful `RequestSignAsync` proves P5 (a person passed the PIN/biometric prompt) and nothing about P6 or P7. **A signature over the approval payload is not evidence that the person agreed to *that* payload**: if another same-user program asks for a signature over different bytes, the person sees the same generic prompt.

## 3. Threats and what actually mitigates them

| Threat | Mitigation | Residual risk |
| --- | --- | --- |
| T1 Another same-user program (including an AI agent the person runs) asks for a signature over attacker-chosen bytes ("blind signing") and the person approves a generic prompt | The approval still needs a matching `issued` row in the ledger (written by this application), the exact artifact/recipe/plan/computer/effects digests, expiry and single use; the application shows the exact plan right before the prompt; **a Hello prompt that does not follow our own confirmation screen must be declined** | A same-user attacker who can also write the ledger file can pre-issue rows; P6/P7 are **not** closed by Option A or B. The pilot's threat model is *accident, drift and unattended automation*, **not** a same-user adversary |
| T2 Database tampering (label, evidence, key record, computer id, approval state, audit rows) | RT-3D1-01/02: authority re-established from the live credential; hash-chained append-only journal verified inside every transaction; ledger cross-checked against the journal | A file owner can still delete the whole store (detected as "no approval", never as an approval) |
| T3 Replay, double use, concurrent use | one conditional update + its success row in one transaction; terminal states; restart-safe | none known |
| T4 Key replaced by another credential under the same name | recorded public key must equal the live one on every use | an attacker who can create a credential **and** rewrite the record (same user) can substitute a key of their own |
| T5 Downgrade / forged evidence | stronger recorded assurance than established is rejected; evidence must be bound to the exact key and unchanged | evidence is OS-reported, not validated (P2) |
| T6 Hello reset, PIN change or lockout invalidates the key | every error is "unavailable, deny" | availability, not security |
| T7 Remote desktop / virtual TPM / roaming profile | unknown behaviour; fail closed | to be observed in the enrollment test only on the development PC |
| T8 Prompt hidden behind other windows or lacking a parent | the application times out and denies; the newer `WindowId` overload may be needed | to be observed |

## 4. Option B — what it does and does not give

Option B accepts a credential whose OS-reported attestation succeeded. It adds, compared with a software key: the private key is non-exportable and bound to this device's TPM (a stolen copy of the files cannot sign elsewhere). It does **not** add: verification of the hardware claim (P1), app isolation (P7), or user awareness of the target (P6). It must therefore be presented to the user as *"protects against copying the approval key to another computer and against silent signing without you present; does not protect against another program running as you."* Option A (verifying the attestation) would turn P2 into P1 but **would not solve P6/P7 either**, and must not be oversold as doing so.

**Conditions before Option B may be considered for activation** (all required, none met): a real-credential end-to-end test (enrollment checklist below) whose results are reviewed; documented same-user isolation behaviour from that test; the user's explicit permission; independent review; the signature scheme and the evidence-stability assumptions confirmed.

## 5. Test-only enrollment checklist (for a FUTURE, separately authorised run — nothing here was executed)

**A. Before anything**
1. Written user authorisation for exactly this test (Talep B) and Red Team acceptance of this checklist.
2. A throw-away key name `atm-test-<random>` (never `atm-approval-*`); an isolated `ATM_DATA_DIR` in a new temporary folder; the real application data and backups untouched; jq not involved.
3. Record the starting facts: Windows build, TPM present/ready (read-only), Hello configured, `windowsHelloProbe` result.
4. Network observation prepared: snapshot of established connections and resolver cache before and after (read-only), because `GetAttestationAsync` may involve Microsoft services; **any unexpected destination ends the test**.

**B. Steps and what to record (closed fields only; never a PIN, biometric datum or screenshot of the credential UI unless the user agrees)**
1. Type `ENABLE-OS-APPROVAL <computer id>`; then create the key. Record: did a prompt appear, in front, with what generic title; time taken; status code.
2. Record: public key blob length, magic `RSA1`, key id, key size.
3. `GetAttestationAsync`: status, whether the certificate chain is non-empty, its digest; call it **three times** and record whether the chain digest is stable (the key store assumes it is and fails closed if not).
4. One signature over a harmless fixed payload: record whether the prompt appears, cancel behaviour (run once cancelling), wrong-PIN behaviour (once), and **which padding verifies** (PSS vs PKCS#1 v1.5, both verifiers are implemented; exactly one is expected to pass).
5. Verify the signature offline with the recorded public key; confirm that the approval's own post-signing verification passes.
6. Restart the application: `OsKeyStore::open` must succeed with the real credential (assurance re-established, evidence unchanged) or fail closed; record which.

**C. Same-user isolation experiment (test key only)**
1. From a *second process of ours* (a separate run of the same binary), `OpenAsync(<test key>)` and `RequestSignAsync`: does it work? does a prompt appear?
2. From Windows PowerShell 5.1 (a Microsoft program already on the PC): the same through the WinRT projection.
3. Record whether the credential is reachable by another process and what the person sees. This decides how P7 is documented; it is **not** a security test of Windows.

**D. Cleanup and abort**
1. `DISABLE-OS-APPROVAL <computer id>`: the OS key is deleted first; then verify `OpenAsync` reports not found and the record is cleared; the audit chain and consistency check pass.
2. Delete the temporary data folder. Existing backups and the retained jq session are never touched.
3. **Abort and clean up on:** an unexpected network destination; an unexpected prompt; any error code not listed in the record; a signature that does not verify; evidence instability.

**E. Evidence to keep for the Red Team:** the closed-field record above, the exit codes, the audit-chain verification, the network observation, and the deletion confirmation.

## 6. Open technical assumptions made explicit

* **Signature padding.** The first implementation assumed PKCS#1 v1.5; Microsoft Learn now says PSS. The verifier implements both, configured with the documented one (PSS); the first real signature decides; a mismatch fails closed.
* **Evidence stability.** The key store compares the certificate-chain digest on open; if the chain changes between calls the key store refuses (fails closed), which would make real use impossible until the checklist's step B3 is understood.
* **Attestation may need the network.** Therefore it is requested only at enrollment, at key-store opening and just before a signature, never on every check.
* **Unpackaged support** is observed, not documented (§1).
