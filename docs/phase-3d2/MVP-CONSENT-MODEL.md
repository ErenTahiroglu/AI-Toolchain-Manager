# The MVP consent model (Phase 3D-2): a visible in-app confirmation, checked in Rust

**Product decision (Issue #21):** a normal, user-scoped installation does not need Windows Hello, TPM attestation or a biometric prompt. Those designs (Phase 3D-1) are retained as research and stay **optional and inactive**: nothing in the installation workflow references them (a source-scan test enforces it: `install_flow_tests.rs`, `the_workflow_modules_never_touch_windows_hello_the_approval_ledger_or_a_key_store`).

## 1. What the user does

1. Opens a tool in the catalog. Its **status** is shown in words and a symbol (never colour alone): *Kurulabilir*, *Doğrulama tamamlanmadı*, *Lisans incelemesi gerekli*, *Onayınız bekleniyor*, *Kuruluyor…*, *Kurulu ve doğrulandı*, *Başarısız / dikkat gerekiyor*, *Kaldırıldı*.
2. Presses **"Kurulum önizlemesini aç"**. Rust builds the preview (exact source, version, platform/architecture, file name, size, SHA-256, licence state, notices, every filesystem effect with its place relative to the manager's own folder, what will *not* happen, rollback expectations) and opens a short-lived **offer** bound to that exact preview. Nothing is changed.
3. Ticks *"Yukarıdaki tüm etkileri okudum ve anladım"* and presses **"Onaylıyorum ve kur"** (or *kaldır*). That press is the explicit affirmative action.
4. Sees the **result** (receipt, or the refusal / failure with an honest rollback report) and may open the **installed-tool details** (version, relative location, digest, five live verification checks, receipt history, full notices text, a list of what the application never does).

## 2. What Rust does with the press

| Step | Where | Refusal code(s) |
| --- | --- | --- |
| The offer id is 64 lower-case hex characters (256 random bits); anything else is refused before the book is consulted | `consent.rs` | `consent_bad_identifier` |
| The offer must be live (≤ 120 s old), not cancelled, not superseded by a newer preview of the same tool, and **not used before** — taking it out of the book is the single-use mechanism (a mutex; 16 racing confirmations of one offer yield exactly one grant) | `consent.rs` | `consent_expired` / `_cancelled` / `_superseded` / `_already_used` / `_unknown` |
| The interface must echo the **digest of the preview it displayed**; a different digest, or a missing acknowledgement, **burns** the offer | `consent.rs` | `consent_preview_changed` / `consent_not_acknowledged` |
| **Freshness**: the preview is rebuilt from scratch (catalog, recipe, authenticated references, manager state, notices, plan) and must hash to the confirmed digest; the artifact is re-read and re-checked against the recipe and the authenticated reference | `install_flow.rs` | `preview_stale`, `needs_attention`, `artifact_*` |
| A Phase 3A approval is derived **from the grant** (actor kind `in_app_confirmation`, which only a grant can produce) and verified against the plan | `approval.rs`, `install_flow.rs` | `ApprovalError` codes |
| The adapter takes the tool's lock, then **consumes the grant by value** (no clone, no serialisation) before the first change and re-derives every bound field (tool, version, platform, computer, recipe digest, artifact digest, effects digest, plan id, replaced version) | `adapter.rs` | `consent_binding_mismatch`, `consent_expired`, `consent_actor_mismatch` |
| Journal-before-effect transaction, rollback, receipt, audit rows (`approve`, `install`/`uninstall`, `rollback`) in the hash-chained audit trail | `adapter.rs`, `audit.rs` | — |

The WebView sends **identifiers and one acknowledgement only**. The request types refuse unknown fields (`deny_unknown_fields`), no command has a path, URL, command-line, bytes or digest-to-trust parameter (source-scan test), and no module of the workflow can start a process, use the network or edit the environment (source-scan + behavioural tests).

## 3. What this is NOT (stated in the preview itself)

* It is **not authentication** and **not cryptographic proof that a person was present**. The offer id is an unguessable handle, not a secret the user holds.
* The interface that shows the preview is the interface that sends the confirmation. **Any program running as the same Windows account that can drive this window or call the application's IPC could press the same button.** The model does not protect against that, and no part of it claims to.
* A confirmation nonce is therefore not a credential and is never described as one anywhere in the interface, the code or the documents.

What the model *does* give: nothing is applied without an explicit confirmation of exactly the previewed content; a stale, cancelled, replayed, expired, forged or altered confirmation is refused; every outcome is audited; and everything an installation does stays inside the manager-owned folder with a rollback.

## 4. Higher-risk operations keep separate, future gates

Refused today and unchanged: arbitrary GitHub install scripts, system-wide installation, administrator elevation, PATH changes, agent-configuration changes, unsigned arbitrary installer execution, **running** an installed program, package-manager commands, unreviewed downloads. Phase 3D-1's OS-credential approval (Option A/B/C, Windows Hello) remains the candidate gate for any of them or for a later, wider release; it is not enabled and no real Hello key was created.

## 5. The release build can install nothing

* Release supply is **empty**: `RecipeRegistry::production()` (`BUILT_IN_RECIPES_JSON = "[]"`), `BUILT_IN_AUTHENTICATED = []`, no artifact provider, no notices. Every real catalog entry is *Doğrulama tamamlanmadı*; `prepare_install` answers `INSTALL_NOT_AVAILABLE`; no manager folder is created.
* The only constructor of an acting adapter (`Adapter::mvp_synthetic`) is compiled in tests and in a build made with the non-default Cargo feature `synthetic-pilot`. Without it, the service's `adapter()` returns `production_adapter_not_enabled` and no acting code exists. `Adapter::production` is unchanged and still refuses everything.
* Even with the feature, every recipe must be `synthetic` with a `synthetic-` tool id (`check_authority`), so a real tool such as jq cannot pass.
