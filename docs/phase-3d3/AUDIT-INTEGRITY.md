# Issue #22 — audit failures are never ignored (Phase 3D-3)

**The finding.** `install_flow.rs` called `audit::append` and discarded its `Result`. An affirmative confirmation could therefore lead to filesystem changes with no record that it was ever given, and a failed outcome row looked exactly like a fully audited success. This is a narrow local MVP audit-integrity fix; it does not bring back OS authentication, and the single-use consent, the journal, the receipt and the rollback are unchanged.

## What the code does now

| Moment | Rule | Result shown to the person |
| --- | --- | --- |
| Before the first filesystem change | The row *"affirmative confirmation approved"* (`approve` / `passed`, with recipe id, approved recipe digest, artifact digest and the approval reference) is **mandatory**. If it cannot be written, nothing is applied — not even the manager's own folder is created — and the spent consent is not reusable. | `refused`, code `audit_unavailable` |
| After a **completed** operation | The outcome row (`install` / `uninstall`, `passed`) is written. If it cannot be: the operation is **not** reported as a success. | `needs_attention`, code `audit_outcome_not_persisted`, `auditRecorded: false`, text "the operation WAS done and verified; only its audit row is missing" — receipt and rollback report as true |
| After a completed operation, the **state is read back** | The installed (or removed) state is inspected again, independently of the transaction's own bookkeeping (digest, size, identity, folder contents). Any difference ends as needs-attention and is audited as `unknown` / `POST_VERIFICATION_FAILED`. Nothing is repaired or deleted. | `needs_attention`, code `post_verification_failed` |
| A refusal / failure / cancellation row cannot be written | The answer stands (nothing happened or was undone), says in plain words that **its own row is missing**, and carries `auditRecorded: false`. A withdrawn offer changes nothing, so its informational row never turns a harmless withdrawal into an error. | unchanged outcome, `auditRecorded: false` |
| Any later status read | A committed receipt with **no** matching `passed` row (matched by the approval reference the receipt carries) keeps the tool in *failed / needs attention* (`audit_outcome_missing`); an unreadable audit table does the same (`audit_unavailable`). **No new install or removal can be previewed or confirmed** until it is reconciled. | status + reason |
| Recovery (explicit button, and once at start-up) | After the ordinary transaction recovery, every receipt without an audit row is examined. A row is written **only if** the receipt's own digest holds **and** the journal independently says `committed` for the same transaction, tool and approval **and** the recipe the receipt was made under is still the current reviewed recipe. The row is labelled `install-audit-reconciled`: it is never presented as the original. Anything else is counted as unreconciled with a reason. | summary: reconciled / unreconciled |

Reconciliation touches **only the audit table**: no manager file, no foreign file, no user data, no pointer. It cannot fabricate a success: with the journal broken (or no recipe), nothing is written and the tool stays in needs-attention. It is idempotent (a second run writes nothing).

## What was tested (`install_flow_pilot_tests.rs`, plus the real window)

* SQLite write failure injected with a `BEFORE INSERT` trigger on the audit table, per event type: pre-installation row → no filesystem change, consent spent, a later new preview works; post-install and post-removal rows → `needs_attention`, truthful receipt/state, blocked new operations, refused repeats, recovery that fails while the trigger stays and succeeds once it is gone, exactly one reconciled row, idempotent, same after a restart (new service).
* **Unavailable audit table** (renamed): a new installation is refused without any change; for an installed tool the state says `audit_unavailable`, nothing moves.
* **Journal broken** (digest no longer valid): no record is fabricated; the foreign files, the tool's user data and the receipts folder are byte-identical; with the journal restored the same recovery succeeds.
* **Read-back** after a change made inside the window the platform leaves open (the installed file altered right before the journal commits): `needs_attention`, nothing repaired, audited as `unknown`.
* **Every audit row** of a complete run uses only values the Phase 0 schema allows (enums pinned in Rust; the 46 rows of the real-window run were validated against `schemas/audit-event.schema.json`: 0 violations).
* The same sequence in the **real application window** with the failure injected through a second connection (`tools/gui_install_flow.mjs`): the answer reads "Yapıldı — ama dikkat gerekiyor", recovery first reports "1 kayıt kurulamadı", then "1 eksik denetim kaydı makbuzdan yeniden kuruldu"; no file of the manager folder changed during recovery.

## Limits

A *failed* or *refused* operation leaves no receipt, so a lost row for it cannot be reconstructed; the person was told at the time (`auditRecorded: false`), and the file system is in the state the answer describes. Reconciliation needs the recipe to still be the reviewed one; a recipe change after the operation leaves the gap visible instead of papering over it.
