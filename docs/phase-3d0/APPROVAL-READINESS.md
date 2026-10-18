# Real approval — minimum production implementation for ONE locally approved install (Phase 3D-0)

Design and readiness review only. **No real approval action, key, prompt, button or key-store code was created.** `device_trust.rs` still has exactly one `KeyStore` implementation (a fake that declares itself `SoftwareFake`); `TrustPolicy::production()` refuses it; no Install button exists; no key is created in any user profile.

## 1. What already exists and what is missing

| Requirement | State |
| --- | --- |
| Approval bound to computer, recipe digest/revision, exact plan id, tool, exact version, platform, scope, project, artifact digest, effects digest, expiry | **Implemented** (Phase 3A `Approval` + `device_trust::authorize`), tested with fakes. The plan binding is enforced where the phase-3A `VerifiedApproval` is produced (`verify_approval` recomputes against the *current* plan); the caller must produce it immediately before `apply`. |
| Signature by a key only this user/device can use; user presence inside the signed payload | **Interface only** (`KeyStore`, `PresenceGate`); no OS implementation |
| Single-use, atomic consumption, expiry, revocation | **Interface + in-memory fake** (`ApprovalLedger`); no durable implementation |
| Audit before effect, deny if audit fails | **Implemented** against an interface; the durable sink must be the existing hash-chained `audit.rs` (accident evidence, not tamper-proof against the same user) |
| Denial when key storage or presence is unavailable | **Implemented** (`KeyUnavailable`, `PresenceUnavailable`, production policy refuses fakes) |
| Frozen plan displayed to the person | **Missing** (no UI by design) |

## 2. Windows — options

| Option | What it gives | Limits | Verdict |
| --- | --- | --- | --- |
| **Windows Hello key credential** (`Windows.Security.Credentials.KeyCredentialManager`, <https://learn.microsoft.com/en-us/uwp/api/windows.security.credentials.keycredentialmanager>) | an asymmetric key (documented as RSA 2048, signature PKCS#1 PSS/SHA-256) created *for the current user and application* and used only after Windows Hello user verification; supports `IsSupportedAsync` (device and user capability), `RequestCreateAsync`, `OpenAsync`, `DeleteAsync`, and attestation data | needs Windows Hello enrolled; behaviour of an **unpackaged, unsigned** desktop app is not established here (**U** — must be tried on a real device); WinRT from Rust needs the `windows` crate (a new dependency, authorization needed) | **Recommended** for the first pilot; deny if unsupported |
| CNG persisted key (Platform Crypto Provider / TPM, or software KSP) with a UI-protection policy | user-bound key container, non-exportable if requested | no genuine biometric/PIN presence guarantee in the software provider; more code | fallback candidate only if Hello is unavailable and the Red Team accepts a weaker presence |
| DPAPI (`CryptProtectData`) | protects a secret for the logon session | any process of the same user can unprotect it; it is encryption, not a signing key or a presence check | **Rejected** as an approval root |

## 3. macOS — options

| Option | What it gives | Limits | Verdict |
| --- | --- | --- | --- |
| **Secure Enclave P-256 key** (`kSecAttrTokenIDSecureEnclave`, access control `privateKeyUsage` + `userPresence`/biometry) | non-exportable key, presence enforced by hardware | per Apple's developer forums (DTS statements: <https://developer.apple.com/forums/thread/786171>, <https://developer.apple.com/forums/thread/728150>) key creation needs a **properly signed main app bundle with an application identifier / keychain-access-groups entitlement backed by a provisioning profile**, otherwise `-34018`; Apple's documentation page itself could not be read in this phase (script-rendered), so this is **forum-sourced, to be confirmed against current Apple documentation**. The project has **no signed macOS build yet** | **Not available today**; target for a signed release |
| Keychain item with `SecAccessControl` `userPresence` (login keychain) | per-user secret released only after Touch ID/password | access is tied to the app's code identity; an ad-hoc-signed development build changes identity on every build (repeated prompts, or none of the protection); no hardware non-exportability | weaker; acceptable only with explicit Red Team agreement |
| `LAContext.evaluatePolicy` alone | proof that a person authenticated | not bound to a key; replayable by the same process | rejected as a root |

**Consequence:** the first real approval is realistic on **Windows with Windows Hello first**; macOS approvals stay *denied* until a signed build can create a Secure Enclave key. That is the fail-closed outcome, not a gap to paper over.

## 4. Application identity and signing

* A key must be tied to *this application*; on Windows the credential is per user and application name (a name such as `atm-approval-v1` — never derived from user input). Unsigned builds can run on a developer machine, but Smart App Control may block a fresh unsigned executable at first launch (see `PHASE-3D0.md` §6), so a distributable pilot needs a signed binary. Neither a certificate nor notarisation exists; none was purchased or created.
* Keys are created **only** by an explicit, separate "enable approvals" action, show what will be created and how to delete it (`DeleteAsync` / Keychain deletion), and are never created at install, start-up, update or by a script. No such action exists yet.

## 5. Minimum implementation for the first approval (when authorised)

1. `KeyStore` for Windows Hello (create on explicit enable, sign, verify with the stored public key, delete); `PresenceGate` = the Hello prompt that signing itself triggers (presence is part of the signed payload, `presence_confirmed`).
2. `ApprovalLedger` in SQLite (one additive migration, **not created in this phase**): `issue` inserts `issued`; `consume` is `UPDATE … SET status='consumed', txn_id=? WHERE approval_id=? AND status='issued'` inside one transaction and must affect exactly one row; `revoke` sets `revoked` unless already consumed; counters monotonic. A crash after consume and before the journal entry **spends** the approval (fail closed; the person approves again).
3. `AuditSink` = the existing hash-chained audit, new closed-vocabulary event types (`approval_issue`, `approval_use`, `approval_refused_<code>`); if it cannot be written, nothing is authorised (already enforced by `authorize`).
4. A confirmation screen that shows the frozen plan (exact version, file name and URL, effects, residual-risk text, expiry) — **not** an "Install" button in this phase; the later action is a separate, named step.
5. Revalidation immediately before apply: recompute the plan, require the same `plan_id`, recipe digest, reference digest and expiry; refuse otherwise.

## 6. Failure and recovery

Key deleted, Hello reset, TPM cleared, profile restored on another PC, or clock moved: the signature no longer verifies or the approval is expired → denial with a stable code; the person re-approves. A revoked approval is honoured at the next step boundary of a running transaction (rolled back like any failure). Nothing is retried silently.

## 7. Must be validated on real devices (none done)

* Windows: `KeyCredentialManager.IsSupportedAsync` on the development PC and on a second PC; creation and signing from an unpackaged, unsigned build; behaviour when Hello is not enrolled (must deny); behaviour under Smart App Control.
* macOS: Secure Enclave key creation from a signed app (and the exact error from an unsigned one); presence prompt; behaviour when the key is deleted.
* Both: that fakes can never be accepted by the production policy (already unit-tested) and that a missing provider denies.
