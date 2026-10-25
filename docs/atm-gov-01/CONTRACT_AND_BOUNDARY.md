# ATM-GOV-01 — "Checkpoint Merkezi": contract and trust boundary

Tracking: [#33](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/33) (stays open for independent review), parent [#32](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/32). Not started and not authorised here: #34 (adapter for a verified engine output), #35 (GitHub publication), CONFIG-02, #36, the external engine itself.
Baseline verified before any edit: `development` = `origin/development` = `ba2fb81af4cd8bd802dfb0b34fff1af4f24bf71b` (parent `b7eff9e…`), `main` = `68eda3f0407490fabccb517dcf822ab4818fc0bf` (frozen), exact-SHA CI run 40175302645 attempt 1 success on Windows x64 and macOS arm64, one untracked file (`docs/product-07/PRODUCT_07_POLICY_PROPOSAL.md`, developer-reported local file; not independently verified; never staged by this work), no stash, one worktree. The commit SHA and the exact-SHA CI result of this change are not written here (hand-off reply and #33 only).
REAL_INSTALLATION_NO_GO. REAL_CONFIG_WRITE_NO_GO. #14 and #25 stay open.

## 1. What this is — and is not
**Is:** a read-only, in-memory **viewer** of metadata the user pastes by hand. It parses a small, strictly bounded, explicitly versioned JSON envelope and shows it as inert text so a person can *navigate* a claimed checkpoint: repository, phase, full source commit, CI run + attempt, claimed job conclusions, artifact metadata, related issue/PR numbers, a reviewer's claimed assessment, limitations and missing evidence.

**Is not:** a verifier, a certificate, a cryptographic result, a test runner, a GitHub client, a source of truth, a generator of checkpoints, a policy engine or an acceptance workflow. **No pasted input can ever produce a trusted, verified, accepted or passing property in the UI.** The separate future verifier is the *Evidence-Checkpoint-Engine* repository (empty at the independent check on 2026-10-25; contract tracked in its issue #1); nothing of it is implemented, imitated or assumed here (see `EXTERNAL_ENGINE_HANDOFF.md`).

## 2. The three things that stay separate
| Kind | Examples | In this viewer |
|---|---|---|
| **Source assertion** | what a pasted record says (`success`, `accepted`, a SHA, a digest, an expiry) | shown, always labelled **`KAYNAK BEYANI — DOĞRULANMADI`**, never promoted |
| **Machine evidence** | a real CI run, a real artifact, a real commit object | **not consulted**: no network, no GitHub API, no file, no process. Syntax ≠ existence. |
| **Independent human approval** | a Red Team decision | not representable: a reviewer field in pasted text is only a *claim* about a review, shown as such |
Existing catalog/install `verificationState` fields are a different concept and are neither read nor changed.

## 3. Input envelope — `schemaVersion = "atm.checkpoint-view.v1"`
This is a **viewer** envelope. It is *not* the external engine's canonical or signed manifest and must not be mistaken for it. Unknown versions are rejected. Every object has **exactly** the listed keys (unknown keys, `null`, wrong types and `__proto__`-style keys are rejected, never ignored).

```jsonc
{
  "schemaVersion": "atm.checkpoint-view.v1",
  "checkpoints": [                                    // 1..10
    {
      "repository": "owner/repo",                     // required
      "phase": "short inert text",                    // required, <= 80
      "sourceSha": "<40 lowercase hex>",              // required
      "workflows": [                                  // required, 1..5
        { "name": "inert text",                       // required, <= 80
          "runId": 40175302645,                       // required, 1..2^53-1
          "runAttempt": 1,                            // required, 1..1000
          "ref": "development",                       // optional, an ASSERTION about the triggering ref, <= 100
          "jobs": [ { "name": "inert text", "claimedConclusion": "success" } ],      // required, 0..30
          "expectedJobs": [ "inert text" ] }                                          // optional, 0..30
      ],
      "artifacts": [                                  // optional, 0..10
        { "id": 123, "name": "inert text", "sha256": "<64 lowercase hex>",
          "expiresAt": "2026-11-01T00:00:00Z", "availabilityClaim": "retained" }     // only id required
      ],
      "issues": [33], "pullRequests": [],             // optional, 0..20 each, positive integers
      "reviewerClaim": { "role": "independent_reviewer", "assessment": "accepted", "onSha": "<40 lowercase hex>" },  // optional; onSha optional
      "limitations": ["inert text"],                  // optional, 0..10, each <= 160
      "missingEvidence": ["ci_not_checked"]           // optional, 0..8, closed list
    }
  ]
}
```

**Closed enums.** `claimedConclusion`: `success failure cancelled skipped neutral timed_out action_required stale unknown`. `availabilityClaim`: `retained expired unavailable missing not_checked`. `reviewerClaim.role`: `author independent_reviewer ci_system unknown`. `reviewerClaim.assessment`: `accepted rejected changes_requested pending unknown`. `missingEvidence`: `ci_not_checked artifact_not_checked branch_head_not_checked reviewer_not_confirmed logs_not_read commit_not_checked_remotely other_unspecified`.

**Field syntax (syntax only — never proof of fact).**
* `repository`: `owner/repo`; owner `[A-Za-z0-9](-?[A-Za-z0-9]){0,38}` (no leading/trailing/double hyphen), repo `[A-Za-z0-9._-]{1,100}` and not `.` or `..`. Comparison between records is **case-insensitive on the lower-cased form only**; display keeps what was pasted.
* `sourceSha`, `onSha`: exactly 40 characters `[0-9a-f]`. **Uppercase, short, long and abbreviated forms are rejected** (not normalised).
* `sha256`: exactly 64 `[0-9a-f]`.
* Numbers: plain non-negative integers only — no sign, fraction, exponent, leading zeros, or more than 16 digits; then range-checked (`runId`, artifact `id` ≤ 2^53−1; attempt ≤ 1000; issue/PR ≤ 999 999 999; zero is rejected).
* `expiresAt`: strict RFC 3339 with a valid calendar date and offset.
* `ref`: `[A-Za-z0-9][A-Za-z0-9._/-]{0,99}`, no `//` and no `..`.
* **Free text** (`phase`, names, `limitations`): the allow-list is letters (any script), digits, space, and `. , : ; ( ) # + _ - – — ' ’ ·`; **no** `< > [ ] { } \ / @ $ % & = ? ! * | ~ ^ " \`` and no control characters. Additionally rejected: URL-like text (`://`, `www.`, `javascript`, `data:`), and anything secret-shaped (GitHub/AWS/Slack token prefixes, `BEGIN` key blocks, runs of 32+ hex characters, 40+ character opaque strings, and the words password/passwd/secret/token/api key/bearer/authorization). **A field that cannot be policed is rejected, never "redacted".** Consequence (accepted): a legitimate sentence that merely contains the word "token" is refused.

**Cross-record rules (rejected, not repaired).** Duplicate checkpoints (same lower-cased repository + `sourceSha` + `phase`); duplicate `(runId, runAttempt)`; a `runId` that appears under two different repository/commit pairs (inconsistent run identity); duplicate job names inside a workflow (also inside `expectedJobs`); duplicate artifact ids, issue numbers, PR numbers or list entries.

## 4. Limits (all enforced, all tested)
Input ≤ **40 000** characters; nesting ≤ **8**; ≤ 10 checkpoints; ≤ 5 workflows each; ≤ 30 jobs / 30 expected jobs per workflow; ≤ 10 artifacts; ≤ 20 issues; ≤ 20 pull requests; ≤ 10 limitations (≤ 160 chars each); ≤ 8 missing-evidence entries; total JSON values ≤ 3 000; strings ≤ 80/100/160 as listed. Duplicate object keys are detected (also when written with different escapes) and rejected.

## 5. Output: a view-model with no verdict
`parseCheckpointInput(text, now)` is pure. It returns `{ ok: false, code, path }` — a code from a closed list and a path built **only from schema key names and indices, never from input text** — or `{ ok: true, checkpoints }`. Every checkpoint carries the constant `trust = "KAYNAK BEYANI — DOĞRULANMADI"`; there is **no** `verified`, `accepted`, `passed`, `trusted` or aggregate-status member anywhere, and no progress total. Time-based fields use the `now` argument; the UI reads the device clock **once, when the user presses "Göster"**, and says so.

Closed **notes** (fixed Turkish sentences, never built from input) may be attached when the record is internally odd: reviewer claim concerns another commit; expected job missing from the reported jobs; reported job not in the expected list; artifact claimed `retained` but its declared expiry has passed on this device's clock; artifact claimed `expired` while the declared expiry lies in the future; artifact without a digest; workflow without jobs. Notes are neutral: they never turn a claim into a negative or positive result.

## 6. What the screen says and never says
* Always: the label **`KAYNAK BEYANI — DOĞRULANMADI`** in a note at the top of the page and in every checkpoint, whatever the data claims; the full 40-character commit; run id **and** attempt together; "bildirilen" (claimed) before every conclusion, availability, expiry and reviewer assessment; the sentence *"Bu kayıt yalnızca bildirilen commit için bir beyandır; bu uygulama o commit'in güncel dal ucu (HEAD) olup olmadığını denetlemez."*; issue/PR numbers as inert text with **no link**.
* Never: green/"accepted" badges, icons, progress totals, "doğrulandı", "onaylandı", "başarıyla", a link or opener, rendered Markdown/HTML, a statement that an old commit applies to current `development`, `main` or "latest", or that unchecked evidence is absent or present.
* Unconfirmed is shown as unconfirmed ("denetlenmedi"), not as negative or positive proof.
* Errors are generic and **do not echo the input**: a Turkish sentence for the reason class, the closed code, the schema path.

## 7. Lifecycle and side effects
The pasted text and the parsed view live only in the page component's React state. The app re-mounts a page on every navigation (`ErrorBoundary key={page}`), so leaving the page destroys both; **"Temizle"** clears both on demand; **editing the text hides the previous result**; a failed attempt discards the previous result. Nothing is stored, exported, copied, logged or sent: no `invoke`/IPC, database, migration, Tauri capability, network, GitHub token, polling, timer, clipboard, file or process access, `localStorage`/`sessionStorage`/IndexedDB, telemetry or new dependency; no `dangerouslySetInnerHTML`; no `<a>`/button that opens anything.

## 8. Deliberate non-goals
Fetching or checking anything remotely; reading files; computing or comparing hashes; deciding pass/fail; aggregating records into a status; signing; exporting; persisting; supporting a second schema version; interpreting prompts or commands inside text (they are rejected or shown as inert text); a "diff" against the real repository.
