# ATM-GOV-01 — what belongs to the external engine, and what this app must never do

Related: [#32](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/32) (parent architecture), [#33](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/33) (this viewer), [#34](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/34) (a future adapter for verified engine output; **blocked** on the engine and on an independently accepted #33), [#35](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/35) (future human-approved GitHub publication; **design only**, no write permission exists).

## The external repository
The canonical separate verifier is **`ErenTahiroglu/Evidence-Checkpoint-Engine`**. At the independent GitHub check on **2026-10-25 it was empty**: no commit, executable, README, CI or accepted output schema. Its initial contract work is tracked in its **issue #1**. This change did not read, fetch, depend on or assume anything from that repository, and nothing in the AI Toolchain Manager treats the repository's existence as evidence of verification.

## Boundary
| Concern | Owner | In ATM-GOV-01 |
|---|---|---|
| Fetching commits, runs, jobs, artifacts, signatures; checking branch heads; computing digests; applying a pass/fail policy; producing a signed or otherwise authenticated result | **Evidence-Checkpoint-Engine** (future, independent) | **absent, by design** — no algorithm, no stub, no mock verdict in React or Rust |
| Showing a person what a record *claims* | AI Toolchain Manager | `Checkpoint Merkezi` viewer: pasted JSON, in memory, every record labelled **`KAYNAK BEYANI — DOĞRULANMADI`** |
| Consuming a *verified* engine output and presenting it as such | a later adapter (#34) | **not built**; needs the engine's accepted output schema first |
| Publishing anything to GitHub | a later, human-approved path (#35) | **not built, not designed here, no permission** |
| Independent human acceptance | the Red Team | outside the software; a pasted "accepted" is only a claim |

## Rules for anyone continuing this work
1. **No verifier logic in React.** Do not add fetching, hash comparison, branch-head checks, CI interpretation, status aggregation or "verified" fields to the viewer, however convenient. A green-looking summary computed from pasted text would be exactly the false trust this milestone exists to prevent.
2. **The viewer envelope is not the engine's manifest.** `atm.checkpoint-view.v1` is a private, minimal, untrusted-input format. The engine's canonical format, once it exists, will be defined there; #34 must map *that* output through its own, separately reviewed adapter and must not reuse the viewer parser as a trust boundary.
3. **Three kinds of evidence stay separate** — source assertion, machine evidence, independent human approval — in data model, wording and UI. An author report, a CI success, an Issue comment and a pasted JSON field are *none* of them independent verification.
4. **Do not relax the fail-closed parser** (unknown keys, bounded sizes, strict syntax, no free-text that cannot be policed) to make a real-world record "fit"; change the schema version instead, with review.
5. **No new privilege.** No IPC, capability, network, token, clipboard, file or storage for this feature without a new, separately reviewed decision. #14 and #25 stay open.

## What #34 will need from the engine before it can start
An accepted, versioned output schema; a statement of what the engine actually checked (commit existence, run/attempt/job conclusions, artifact identity and retention, signatures) and what it did not; a way to tell *unavailable / not checked* from *negative*; and an independent review of the engine itself. None of this exists today.
