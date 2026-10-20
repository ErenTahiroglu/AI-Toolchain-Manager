# Phase 3D-5 — B3 pre-authorization: a ONE-installation gate for the jq pilot (Issue #24)

Baseline `1e2fe043b28a94ad113f78913f4c83ed2b725e5a` (B2, accepted by Red Team for its narrow pilot preparation; B1 evidence accepted with its documented limitations). Frozen `main` `68eda3f0407490fabccb517dcf822ab4818fc0bf` is untouched. **jq was not downloaded, copied, opened, hashed, installed or executed, and no jq file exists where any build of this phase can reach it. No real installation has been authorized.**

## The finding and the correction

[Issue #24](phase-3d5/evidence/issue-24-body.md): once `jq_pilot::B3_UNLOCK` is `Some(recipe digest)`, the feature-enabled build installs the real provider and the enablement entry on *every start*; the single-use preview token only limits one *preview*. The user's authorization is for **ONE** installation, so "one" has to be a durable fact of the manager's own state, not a property of the build or of a nonce.

| Requirement of the task | What now enforces it | Where |
| --- | --- | --- |
| One successful installation **consumes** the allowance | a durable record in the application's own database moves `reserved → consumed` only after the transaction **completed and was read back** | `pilot_allowance.rs`, `install_flow.rs::apply` |
| **Restart does not reset it** | the record (and the receipts and the audit trail as witnesses) is read from disk at every start/preview/confirmation; nothing lives in process memory | `b3_one_installation_survives_a_restart…` |
| **Removal does not reopen it**; a separate affirmative removal is still possible | `consumed` is terminal; removal needs its own preview and consent and the state then reads *Removed — kurulum izni kullanıldı* (`can_install = false`) | `b3_a_removal_never_reopens…` |
| Another preview/confirmation cannot authorize a second installation | the preview is refused (`INSTALL_PILOT_ALLOWANCE_CONSUMED`) **before** any byte of the retained file is read; an old offer is refused at confirmation | same test, `b3_an_inactive_or_spent_pilot_never_asks_the_provider…` |
| **Concurrent** requests cannot spend it twice | reservation is one compare-and-swap on the record; eight services with eight independent consent books race: exactly one completes, no loser ever holds the allowance | `b3_eight_services_…` |
| Uncertain consumption/recovery → **fail closed, `needs_attention`** | invalid / contradicted / foreign / unreadable record, unreadable receipts, an unsettled reservation: the tool is *Başarısız — dikkat gerekiyor* and neither installation nor removal can start | `b3_a_corrupted_missing_forged…`, `b3_a_crash_at_any_point…` |
| Accurate audit records; **no silent deletion or reset** | the code has no delete/reset path (scanned); a release (only when the attempt provably changed nothing) is recorded in the audit trail **before** the allowance reopens; without that row it stays reserved | `b3_a_release_needs_its_own_audit_row…`, `b3_the_allowance_has_no_reset_path…` |
| A one-use *preview nonce* is not a one-use *pilot authorization* | the nonce is untouched and still per preview; the allowance is a separate record bound to recipe + artifact + computer + data directory | |
| No Windows Hello / TPM / biometric / credential framework | none is used (scanned) | |

## How it works (small, existing infrastructure)

* **Bound to** jq 1.8.2 · windows-x64 · recipe digest `9c7a020d…cca1` · artifact SHA-256 `a6fc67fe…c627` (all read from the committed pilot files, never literals in Rust), this computer's manager identity and **one isolated data directory** (`data_dir_digest`: SHA-256 of the normalised path; `tools/pilot_data_dir_digest.py` computes the same value; the path itself is never stored).
* **Inactive until armed.** `jq_pilot::B3_ARMED_DATA_DIR_SHA256` is `None`: the pilot is *not armed* for any directory, so the service answers `pilot_not_armed` and nothing real can be previewed or confirmed, **whatever `B3_UNLOCK` says**. It is set only by the commit that records the user's separate authorization (and names the one directory). A different directory — or a database copied elsewhere — cannot use it (`pilot_wrong_data_directory`, `…state_for_another_installation`).
* **One record** (an internal settings row, changed only by compare-and-swap, sealed with its own digest). `Open → reserved → consumed` (terminal) or `→ released` (provably nothing changed: transaction rolled back **and** read back as *not installed*) `→ reserved` again (counted).
* **Witnesses besides the record**: the manager's committed install receipts and the audit trail's passed `install` rows. A record that is missing, "released" or damaged while a witness says *installed* is a conflict, never an allowance.
* **Reservation point**: after the mandatory approval audit row and the opened manager folder, immediately before the transaction. A refused confirmation, an audit failure, a stale preview or an artifact mismatch never touches it.
* **Recovery**: a reservation whose installation *committed* (receipt of that very approval, vouched for by its committed journal) is settled as consumed from the receipt, once. Nothing else is: an interrupted or rolled-back attempt is **not** proof that nothing was installed, so it stays *needs attention*; resolving it is a reviewed human decision, never a code path (see the [checklist](phase-3d5/PILOT-CHECKLIST.tr.md), "Beklenmeyen sonuç").
* **Not changed**: filesystem ownership rules, conflict refusal, rollback, audit reconciliation, the preview/consent flow, the release build (no one-time rule exists in it: `Sources::production().one_time` is `None`; self-check reports `oneTimeAllowances: 0`).

## What was and was not run, and by whom

Developer-authored tests, on this computer and in CI, on **synthetic bytes** only. The independent reviewer's B1 limitations stand: Rust/frontend tests and the real window were **NOT independently run by ChatGPT**; this phase's tests are not independent evidence either. CI is not independent user-device testing. Smart App Control blocked some freshly built test executables on this computer; they are recorded as BLOCKED / NOT RUN and not counted ([record](phase-3d5/SMART-APP-CONTROL-RECORD.md)). Issue #14 stays open for any broader release.

Which earlier tests changed (only B2's own, and why): [GUARD-TEST-CHANGES](phase-3d5/GUARD-TEST-CHANGES.md).

## Final request

[B3-AUTHORIZATION-REQUEST.tr.md](phase-3d5/B3-AUTHORIZATION-REQUEST.tr.md) (draft; **yetki VERİLMEDİ**) and the preparation [PILOT-CHECKLIST.tr.md](phase-3d5/PILOT-CHECKLIST.tr.md) (**hazırlıktır, izin DEĞİLDİR**).
