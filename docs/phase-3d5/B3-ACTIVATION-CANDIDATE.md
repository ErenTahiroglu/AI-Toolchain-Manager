# B3 activation CANDIDATE — the one-line diff, and why it authorizes nothing

**This is a candidate for independent review. It is not an installation, not a permission and not the user's authorization.** It exists so the reviewer can read the exact change that B3 would make to the unlock, separately from the allowance mechanism that precedes it.

## The diff (commit after the allowance, `src-tauri/src/jq_pilot.rs`)

```diff
-pub const B3_UNLOCK: Option<&str> = None;
+pub const B3_UNLOCK: Option<&str> = Some("9c7a020d65d233e86577464a06c32b8f08fd4fd8403110e68fd551b674f7cca1");
 pub const B3_ARMED_DATA_DIR_SHA256: Option<&str> = None;   // unchanged: the allowance stays INACTIVE
```

plus the tests, documents and the GUI driver label that must follow (see [GUARD-TEST-CHANGES](GUARD-TEST-CHANGES.md)). Nothing else changes: not the recipe (digest `9c7a020d…cca1`), not the source, permissions, filesystem effects, version, architecture or method.

## What the candidate build does and does not do

| | Candidate (`--features jq-pilot`, Windows only) | Standard builds |
| --- | --- | --- |
| compiled | only with the non-default Windows `jq-pilot` feature (a compile error elsewhere and with `synthetic-pilot`) | the feature does not exist there; `Sources::production()` has no recipe, no reference, no enablement entry and **no one-time rule** (self-check: `oneTimeAllowances: 0`); `Adapter::production` refuses |
| supply | the reviewed recipe, reference, attestation, licence record, notices, catalog row `jq`, the real retained-file provider and the ONE enablement entry now exist | none |
| a preview of jq | **refused** (`pilot_not_armed`) — decided before the provider is asked; not one byte of any retained file is requested | n/a (no recipe) |
| a confirmation | refused | refused |
| the retained file | **not placed anywhere reachable in this phase**; no folder `jq-pilot-input` is created | n/a |
| installing | **impossible**: `B3_ARMED_DATA_DIR_SHA256 = None` | impossible |

Arming is a **separate** commit that exists only after the user's authorization: it sets `B3_ARMED_DATA_DIR_SHA256` to the digest of the ONE isolated data directory (`python tools/pilot_data_dir_digest.py "<dir>"`). After it, exactly one installation can ever complete there ([PHASE-3D5](../PHASE-3D5.md)).

## Evidence for the candidate (synthetic bytes or no jq input at all)

* `b3_the_activation_candidate_is_unlocked_but_inactive_and_never_asks_for_a_byte_of_a_retained_file` — the real `sources_for_app` supply with a counting provider: state `pilot_not_armed`, previews and confirmation refused, **0** provider requests, no manager folder, no input folder, no allowance record.
* `b2_the_unlock_is_one_constant…` — the constant names exactly the reviewed recipe digest and nothing else; `b2_the_real_committed_supply_is_windows_only_unlocked_for_the_candidate_but_inactive…` — gate 1, one-time rule present and **unarmed**, nothing created.
* Real window ([GUI-EVIDENCE](GUI-EVIDENCE.md)): the candidate `jq-pilot` build lists jq as *Doğrulama tamamlanmadı — `pilot_not_armed`*, no install button, forged IPC refused, nothing but the application's own database files changed on disk; no jq file existed anywhere in that run.
