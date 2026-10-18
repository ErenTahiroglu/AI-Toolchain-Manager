# Real approval and audit — production design

Status: **design proposal. No approval button, no approval command and no real actor exists, and none is added.** `ApprovalPolicy::production()` still accepts no actor at all. Phase 3A's synthetic approval proves the *binding* logic; it deliberately proves nothing about who pressed a button.

## 1. What is wrong with the Phase 3A approval if used for real

The Phase 3A approval is bound to computer, recipe id + revision + digest, plan id, tool + exact version, platform/surface/scope/project alias, expiry and a typed confirmation, and the binding is a **plain SHA-256** over those fields. That is *tamper evidence for accidents*, not authorization:

* an unkeyed SHA-256 can be recomputed by anyone who can write the database or call the code path;
* a typed confirmation string is computable from public facts (`expected_confirmation`), so it proves nothing about a human;
* nothing ties the approval to the local user, the session, or a user-presence gesture;
* nothing makes it single-use or revocable beyond expiry.

Therefore a real approval needs an **actor the operating system authenticates** and a **secret the attacker model says an unprivileged process cannot read or use without the user**.

## 2. Requirements for a real approval

1. **Explicit user action** in the application's own window (a deliberate gesture on a screen that shows the exact plan, the residual-risk text and the effects). Never reachable from a deep link, a command-line argument, a file, another process or a remote call; never auto-confirmed; the confirmation text is a UX safeguard, not the security control.
2. **Authenticated to the local user and the current session.** The approval record carries a signature or MAC made with a key that exists only for this user on this computer (§3) and whose use needs the user's logon session (and, where available, a user-presence/verification prompt).
3. **Bound to** the computer id, the recipe id + revision + digest, the exact plan id, tool + exact version, platform, surface, scope, project alias, the **file-system effects list**, and the **reference digest** of the artifact — everything `approval.rs` already binds — plus an approval id and a monotonic counter.
4. **Durable** (survives restart), **single-use** (consumed atomically inside the first transaction's journal write; a retry of the *same* transaction id after a crash may resume, a new transaction needs a new approval), **expiring** (short; default 10 minutes for the first pilot), **revocable** (the user can withdraw it; revocation is checked at every step).
5. **Revalidated before apply**, immediately before the first change to managed state: expiry, revocation, the recipe's still being in the compiled-in list at the same digest, the reference still being authenticated, the plan facts still current (the plan is recomputed, not trusted), the project still registered, the manager's lock acquired.
6. **Audited** without credentials or secrets: who (actor type `local_user`), what (action, tool, exact version, platform), when, outcome, approval id, plan id, recipe digest — no paths, no names of private projects beyond the registered alias, no tokens.

## 3. Key custody options compared

| Option | What it gives | What it does not | Windows | macOS | Cost / risk |
| --- | --- | --- | --- | --- | --- |
| **A. OS-protected credential storage** (store a random per-install secret; MAC the approval with it) | a secret not readable by other users; on Windows DPAPI binds to the user's logon; on macOS the Keychain item can be restricted to the app's code identity | a **same-user process** running as an allowed app/with the user's session can often read DPAPI blobs; Keychain access control depends on the app's code signature (an ad-hoc/unsigned development build has no stable identity); no proof of user presence | DPAPI / Credential Manager (`CryptProtectData`) | Keychain (generic password item, `kSecAttrAccessible…ThisDeviceOnly`) | low; portable code via a vetted crate |
| **B. Per-device asymmetric key, non-exportable** (create key in the platform's key store; sign the approval; verify with the stored public key) | the private key is not extractable; signing can require user presence | still usable by same-user code if the key is not gated by a prompt; Windows needs a TPM or software KSP; a macOS Secure-Enclave key requires a properly signed app with entitlements | CNG key storage provider (Platform Crypto Provider/TPM where available, Software KSP otherwise) | Keychain/Secure Enclave key (`kSecAttrTokenIDSecureEnclave`) | highest; platform-specific; signing identity matters on macOS |
| **C. OS user-verification gate** (Windows Hello / `UserConsentVerifier`, macOS LocalAuthentication) in front of A or B | proof that a person was present at approval time | not a key by itself; availability varies; headless/CI cannot test it | WinRT `UserConsentVerifier` | `LAContext.evaluatePolicy` | medium; add as the *gate*, not the trust root |
| **D. Unkeyed SHA-256 + typed confirmation** (Phase 3A) | accident detection | **insufficient — explicit non-goal** | — | — | none |

**Proposal for the first pilot:** B-with-C where the platform offers it, otherwise A-with-C, and **refuse to approve at all if neither is available** (fail closed: no "degraded" real approval). The approval record is a signature/MAC over the canonical binding bytes; the verifier is the application itself. All of this is a *proposal*: whether the macOS Keychain / Secure Enclave paths work for the application's signing identity can only be established with the signed application on a real Mac (see `PORTABILITY-AND-MANUAL-TESTS.md`), and whether a CI runner can exercise it at all is doubtful (the unit tests would use a fake key store behind a trait; the real stores need manual tests).

## 4. Threat-model statement

The design protects against: accidental or scripted approval by other processes that cannot use the user's key or session; replay of an old approval (single use, expiry, counter); an approval for plan A being used for plan B (binding); a modified plan after approval (revalidation by recomputation). It does **not** protect against malware that runs as the same user in the same session and can drive the application's UI or use the key while unlocked — the same limit the audit chain already documents (`audit.rs`: detects accidental edits, not same-user tampering).

## 5. Lifecycle of one real approval (sequence, no code)

1. The user opens the plan view; the manager recomputes the plan (not cached) and shows exact version, target, effects, residual risks, expiry.
2. The user presses *Approve*; the OS user-presence gate runs (C) if available; the key (A or B) signs the canonical binding; the record is stored with `status = pending_use`, `expires_at`, `counter`.
3. The user presses *Install* (a separate action, so a later, deliberate step): the manager revalidates (§2.5), takes the tool lock, and **atomically** writes `status = consumed, txn_id` and the journal's first entry in one durable step.
4. Failure after step 3 never re-opens the approval; the user must approve again.
5. Revocation sets `status = revoked`; a running transaction notices at its next step boundary and rolls back as for any failure.

## 6. Audit events (additions proposed for a later phase)

`approval_created`, `approval_consumed`, `approval_expired`, `approval_revoked`, `approval_rejected_<code>` with the existing hash-chained, redacted record format (no path, no secret, no free text). Hash chaining remains accident/edit detection only; the document does not claim tamper-proof logs.

## 7. What Phase 3B implemented

Nothing of this: no key store, no approval UI, no new IPC command. The synthetic approval path is unchanged and still unreachable from the application; a test asserts that the production policy accepts no actor.
