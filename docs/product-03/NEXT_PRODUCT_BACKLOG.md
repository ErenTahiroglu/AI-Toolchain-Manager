# PRODUCT-03 — next product milestones (proposal; replaces the PRODUCT-02 list)

Done by PRODUCT-03: Recommendations evaluation is context-safe (P02-RT-01); the Recommendations page has no write path and hands one suggested tool to the matrix; `useLoad.setData` no longer loses to an older read. Still **not** proposed: real installation, the jq pilot, any change to pilot gates or Smart App Control handling, the offline-preflight harness (deferred), signing / auto-update, cloud sync. Real installation stays behind its own acceptance gate (Issues #14 and #25 open, independent Red Team, separate explicit instruction).

| # | Milestone | Measurable user value | Prerequisites (state today) |
|---|---|---|---|
| 1 | **Suggestions inside the matrix.** Show the deterministic rule result as a "Bu görev için önerilir" mark on matrix rows for a task typed on the Recommendations page (kept in the typed handoff, no new storage). | From a task description to the relevant undecided tools without a second screen. | Handoff carries target + entry (done). The rule set still needs its own audit — **not audited**. |
| 2 | **Observed vs wished, per row.** Latest *stored* discovery observation next to the wish, with age and the "name match is not identity" caveat. | A user sees which wishes the last scan did not find without leaving the page. | `list_inventory` is a stored-data read; needs the decision that the Projects page may call it. |
| 3 | **Review queue.** One list of decisions with `materialChange`, postponed wishes past their reminder date, and wishes on removed catalog entries, with a Dashboard count tile. | The count reaches 0 after review. | Fields exist; frontend + one tile; no background process. |
| 4 | **Per-project plan export (read-only).** Markdown/JSON "desired toolchain" file that never says "installed". | A plan can be handed to a teammate. | First user-driven write: typed command, user-chosen location, security review. |
| 5 | **Evaluation log hygiene (needs a Rust change, so its own phase).** Let a UI-discarded evaluation not consume the "already shown for this task" slot, or expose a read-only dry-run evaluation. | No suggestion is ever "used up" by an answer nobody saw; today the UI works around it with a fresh task id. | Rust change in `recommend.rs`/`commands.rs` — outside this phase's allowed scope. |
| 6 | **Hygiene bundle.** README status banner; Turkish copy review; fold the Plan page's target selection into the shared target helper; a cancellation/`AbortController`-style convention for new async flows. | No stale phase claim on the first README screen. | None. Small and safe. |

## Order and why
1 → 2 → 3 are frontend-only and build on what exists; 5 should precede any feature that evaluates more often than the user clicks; 4 introduces the first write path and should wait for its own review.
