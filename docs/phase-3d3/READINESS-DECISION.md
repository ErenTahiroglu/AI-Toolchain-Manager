# Windows jq pilot: installation-readiness decision (Phase 3D-3)

Target, exactly: **jq 1.8.2 · windows-x64 · `jq-windows-amd64.exe` · 1,035,264 bytes · SHA-256 `a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627`**. Nothing was downloaded, copied, opened or executed in this phase. No real tool is installable in any build.

## Recommendation

| | |
| --- | --- |
| **Now** | **NO-GO — not executable.** Two review items and one activation change are still missing (below), and the user has not authorised anything. |
| **After those three items** | **GO for exactly one local, installation-only pilot** on the existing Windows development computer, under [the authorization request](AUTHORIZATION-REQUEST.tr.md), if the user then grants it. The evidence gathered so far supports that decision; nothing found argues against it. |

## What is settled (with the evidence)

| Gate | State | Where |
| --- | --- | --- |
| Product decision: in-app confirmation, no Windows Hello | accepted; Windows Hello code untouched and unreachable | [MVP consent model](../phase-3d2/MVP-CONSENT-MODEL.md) |
| **Issue #22** audit honesty | **closed in code and tests** (pre-installation row mandatory; missing outcome row → `needs_attention`; reconciliation from receipt + journal; read-back after the operation) | [AUDIT-INTEGRITY](AUDIT-INTEGRITY.md) |
| **Source attestation** | the `source_origin_unverified` blocker disappears **only** with a substantiating record (`attestation.rs`: 9 bindings — artifact digest/size/name/format, repository slug+ids, tag+commit, workflow path/ref/issuer/runner/run/log index, verification outcome, GPG, retained evidence hashes); a bare `sourceAttested: true` is refused for any real recipe; the policy names no tool | `evidence/jq-1.8.2-windows-x64.source-attestation.json` |
| **Licence for the local use** | `LICENSE_PASS_LOCAL_USE` (state `accepted_local_use`), bound to the exact file digest and the exact notice text, no network, install/uninstall only, can never satisfy a redistribution check; Q1–Q3 evaluated (none blocks the local use); `LICENSE_CONDITIONAL` stays for any bundled/hosted/redistributed use | [LICENSE-LOCAL-USE-REVIEW](LICENSE-LOCAL-USE-REVIEW.md) |
| Candidate recipe (revision 2) | exact file, exact digest, attested source, local-use licence, **no network**, fixed effects, rollback `designed`, install + uninstall only | `jq-1.8.2-windows-x64.recipe.candidate.json` |
| Retained-file provider | one fixed folder, one file name taken from the recipe, size checked before reading, plain single-link file only, no link followed, read-only; the same transition exercised with **fixture bytes** end to end | `artifact_provider.rs`, `install_flow_pilot_tests.rs` |
| Explicit enablement list | a real recipe is applicable only if its exact tool/version/platform/recipe digest/artifact digest is listed; the list is **empty** in every release build | `lifecycle.rs`, tested |
| Windows local filesystem acceptance, before/after inventories, failure and recovery evidence | gathered on the development PC | [GUI-EVIDENCE](GUI-EVIDENCE.md), [ISSUE-14-PILOT-DECISION](ISSUE-14-PILOT-DECISION.md) |

## Exact remaining blockers

| # | Blocker | Who | Why it cannot be done here |
| --- | --- | --- | --- |
| **B1** | **Independent review of the candidate recipe** (review state `draft` → `reviewed`, by someone other than the author, over the recipe's final digest) and **independent staging evidence** (`not_reviewed`, `staging_evidence`) | Red Team / independent reviewer | The validator requires independence; the author's own review would be the very thing it refuses. The test suite proves that with both filled in (on a copy) the recipe validates and the plan's only blocker is the user's approval. |
| **B2** | **Activation change** — one reviewed commit that (a) puts the three records into `BUILT_IN_EVIDENCE_JSON`, the reviewed recipe into `BUILT_IN_RECIPES_JSON` and the file digest into the authenticated-reference list; (b) adds one enablement entry (recipe digest + artifact digest) to the pilot list; (c) compiles the notice bundle into the supply (its SHA-256 must equal the licence record's); (d) adds the catalog entry `jq` (the recipe's tool id must equal a catalog entry id); (e) constructs the retained-file provider for the fixed input folder; (f) provides an acting adapter in that one build. **Proposal for (f)**: a separate, off-by-default Cargo feature `jq-pilot` (like `synthetic-pilot`), never enabled by CI's release build, so the default build stays inert. | Red Team approval of the diff | The phase forbids activating the production recipe or reference before that approval. |
| **B3** | The user's **separate, explicit authorization** of the one installation | the user | Not given. |

Not blockers (stated so nobody wonders): Smart App Control / Defender reaction to *executing* the file (it is never executed); a second physical PC (the Red Team allowed the single-machine path; Issue #14 stays open for any wider use); the Phase 3D-1 Windows Hello work (optional, inactive); Q1–Q3 (open only for redistribution).

## Open assumptions the decision rests on (unchanged by this phase)

* The legal assumption of the local-use review §4 (copying one's own file between folders of one account is not redistribution).
* Request A's cryptographic evidence is "conditionally accepted" (Issue #19): the first online run contacted a host outside the approved list (`tmaproduction.blob.core.windows.net`); this is recorded in the attestation record (`networkPolicyCompliant: false`) and was not repeated. Nothing in this phase contacted any host for jq.
* The installed file is stored as `versions/jq/1.8.2/artifact.bin` (the adapter's fixed name): it cannot be launched by the name `jq`, is not on the search path, and is installed without ever being run.
