# Windows Hello approval for the first Windows pilot (Phase 3D-1)

**Status: the API is reachable on the development PC; nothing real was enrolled, created, signed or prompted.** Every key-creating or signing path is implemented against an interface, tested with **fake** credential backends, and unreachable from the application. A real enrollment needs its own explicit user authorisation (proposed as *Talep B* in [FIRST-INSTALL-AUTHORIZATION-REQUEST.tr.md](FIRST-INSTALL-AUTHORIZATION-REQUEST.tr.md), not granted).

## 1. Is the API available to an unsigned desktop program here? — evidence

`--self-check` now contains `windowsHelloProbe` ([`src-tauri/src/os_hello.rs`](../../src-tauri/src/os_hello.rs)). It asks the OS two questions and nothing else: `KeyCredentialManager.IsSupportedAsync()` and `UserConsentVerifier.CheckAvailabilityAsync()`. It creates, opens, deletes and signs nothing and shows no PIN/biometric prompt (`keyCreated:false`, `promptShown:false` are asserted in its output).

Result on the development PC (Windows 11, locally built **unsigned** debug binary — Authenticode `NotSigned` — run without package identity), 2026-10-19:

```
state: api_reachable_and_hello_available
keyCredentialManagerIsSupported: ok, supported = true
userConsentVerifierAvailability: ok, available
processHasNoPackageIdentity: true
```

So the supported WinRT API **can be called from an ordinary unsigned Tauri/Rust desktop process on this machine**, and Windows Hello is set up and available. The CI Windows runner is expected *not* to have Hello configured; its probe result is recorded in the delivery report as runner evidence only. What this does **not** show: that a key can be created, that a prompt appears in front of the right window, or what a signature looks like (§6).

## 2. What a genuine approval is (and is not)

* **Device-bound key.** A Windows Hello *key credential* created by the OS credential service (`KeyCredentialManager.RequestCreateAsync`), non-exportable by design; we keep only its **public** key, its SHA-256 id and the assurance label.
* **User presence from the OS operation itself.** The approval payload is signed with `KeyCredential.RequestSignAsync`. The OS verifies the person (PIN/biometric) **inside that call**; no signature exists unless it succeeded. `OsKeyStore::sign_present` reports `Confirmed` only for a successful call, `Declined` for a cancel, `Unavailable` otherwise. `OsKeyStore::sign` — the silent path — **always refuses**. There is no button, no flag and no confirmation string that stands in for presence; `presence_confirmed` is inside the signed payload and is only honoured because a signature over it exists.
* **Verified, not trusted.** After signing, the approval is verified against the recorded public key (RSA PKCS#1 v1.5 / SHA-256 through CNG, `cng_verify.rs`) **before anything is recorded**; `authorize` verifies again at use. A plain SHA-256, an unsigned string, a bit-flipped or re-flagged approval, or another key's signature is refused (tests).
* **Tied to one operation.** The signed payload binds the Phase 3A approval (recipe digest, plan, tool, exact version, platform, scope, project, **computer id**, action, expiry), the **exact artifact SHA-256** and the **effects digest**; `authorize` re-checks every one against the operation actually being performed.
* **Single-use, expiring, durable.** §4.

## 3. Hardware claims are verified rather than assumed

`KeyProtection` (in `device_trust.rs`) has four values; only the first can satisfy `TrustPolicy::production()`:

| Value | Meaning | Who can produce it today |
| --- | --- | --- |
| `OsProtected` | OS key whose **hardware protection this program verified** (attestation chain validated) | **nobody**: no verifier exists |
| `OsReportedHardware` | the OS *reports* TPM attestation for the credential (`GetAttestationAsync` = Success); only its SHA-256 is kept as evidence; **not validated** | the real Windows Hello backend, when attestation succeeds |
| `OsSoftware` | no attestation reported (software-protected credential) | the real backend otherwise |
| `SoftwareFake` | test double | tests only |

Consequence, stated plainly: **with the code as it is, a real Windows Hello key can never satisfy the production policy.** Production real approval therefore stays closed even if someone enrolled a key. A decision is needed before a pilot (for the user and the Red Team):

* **Option A (strictest):** implement and review attestation verification (TPM2 attestation statement and certificate chain, vendor roots). Large; not started.
* **Option B (proposed for the *pilot only*):** accept `OsReportedHardware` through `TrustPolicy::pilot_candidate_os_reported_hardware()` — a constant that exists, is unused, refuses fakes and software keys, and is tested. This accepts the OS's own statement that the key is TPM-backed. It is weaker than A and must be accepted explicitly.
* **Option C:** refuse Hello keys altogether and use a different mechanism (§7).

## 4. Single use, expiry, restart safety, audit

`approval_store.rs` — a separate SQLite file `approval-ledger.sqlite` (not the main database; no migration of existing data):

* `consume` is one conditional `UPDATE … WHERE status='issued'` in an immediate transaction, `synchronous=FULL`; of 8 simultaneous connections exactly one wins (test); `consumed`/`revoked` are terminal (trigger); nothing is deleted; reopening the file cannot resurrect an approval (test: restart then replay → `approval_replayed`).
* Expiry is enforced by `authorize` (`now >= expiresAt`); a computer id mismatch, a different recipe/artifact/effects digest or action is `approval_binding_mismatch`.
* The audit journal is append-only (triggers) and a **hash chain** (`prev_hash` unique, so writers cannot fork it); `verify_audit_chain` recomputes it and detects an edited or removed row (test). The audit record is written **before** the consumption; if it cannot be written, `authorize` denies and the approval stays unconsumed (test: audit table made unusable → `audit_unavailable`, status still `issued`, usable once the audit works again). A ledger that cannot be written also denies.
* Content is closed vocabulary and ids; the only key material is a public key.

## 5. No silent key creation; enrollment is reversible

1. Approval security starts **disabled** (`approval_security.enabled = 0`).
2. The person must type `ENABLE-OS-APPROVAL <computer id>` (audited; wrong phrases are audited refusals). Only then can `enrollment_permit` hand out an `EnrollmentPermit` — the only value that allows a backend `create_key` call. Nothing in the application calls any of it: there is no command, no button; tests assert that `commands.rs`, `state.rs`, `adapter.rs`, `lifecycle.rs` and `main.rs` never reference these modules.
3. A key that already exists under the name but is not recorded is **never adopted** (`UnrecordedKeyExists`).
4. The recorded public key must equal the backend's key at every use (key swap → `approval_key_mismatch`).
5. **Reversal:** `deprovision` needs `DISABLE-OS-APPROVAL <computer id>`, **deletes the OS key first** (`KeyCredentialManager.DeleteAsync`), and only then clears the record; if the deletion fails the record stays and describes the key that exists. Ledger and audit history are kept.

## 6. What is NOT exercised (honest list)

* `RequestCreateAsync`, `OpenAsync`, `RequestSignAsync`, `GetAttestationAsync`, `RetrievePublicKeyWithBlobType`, `DeleteAsync` have **never run** against a real credential. The code compiles on Windows and is covered only through the trait with fakes.
* **Prompt parenting.** An unpackaged desktop process may show the Hello prompt without a proper parent window (behind other windows). A window-handle interop variant may be needed; unknown until the enrollment test.
* **Signature shape.** That `RequestSignAsync` yields RSASSA-PKCS1-v1_5 over SHA-256 of the data, and that `BCryptPublicKey` is a `BCRYPT_RSAPUBLIC_BLOB`, is assumed from the documented behaviour; the CNG verifier is proven only with ephemeral software RSA keys. If reality differs, the approval's own post-signing verification **fails closed** (`approval_signature_invalid`) before anything is recorded — the first real run is the check.
* Attestation retrieval returns data we hash but do not validate.
* Behaviour with a roaming profile, a work/school account policy, a device without TPM, a locked-out PIN, or a changed Hello PIN/biometric (the key may be invalidated; the code treats every error as "unavailable → deny").
* macOS: not designed here; needs a signed application and the Secure Enclave ([../phase-3d0/APPROVAL-READINESS.md](../phase-3d0/APPROVAL-READINESS.md)).

## 7. Supported alternative if Hello proves unusable in the enrollment test

A TPM-backed CNG key (Microsoft Platform Crypto Provider, `NCryptCreatePersistedKey`) with a CNG UI policy requiring strong protection, signed through `NCryptSignHash`: hardware attestation is available through `NCRYPT_PCP_*` properties and the key never leaves the TPM, but user presence would rest on the CNG PIN prompt rather than Windows Hello biometrics, and the same verification-before-trust discipline applies. Not implemented; offered for independent review only if option B/Hello is rejected or blocked.

## 8. Tests

`approval_store_tests.rs` (11), `os_approval_tests.rs` (15), `cng_verify.rs` (2, Windows), `device_trust_tests.rs` (unchanged, still passing), and the rehearsal in `adapter_tests.rs` ([SYNTHETIC-DRY-RUN.md](SYNTHETIC-DRY-RUN.md)). `Adapter::production` still refuses everything; `TrustPolicy::production()` still refuses every key this repository can create.
