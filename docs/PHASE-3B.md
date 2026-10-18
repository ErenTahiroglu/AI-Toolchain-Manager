# Phase 3B — Production installation readiness, trusted provenance and pilot selection

Baseline `3cb4a30ff9c3b12be1cb880919de0eb9e7d7e6b3` (accepted Phase 3A closure; security gate Issue #14, RT-3A-R1). This phase authorizes **research, design, specifications and non-executing validation only.** No real tool was downloaded, extracted, installed, started or probed; the production recipe list and the authenticated-reference list are still empty; the synthetic engine is still disconnected from every command and screen; there is still no approval button.

The detailed documents live in `docs/phase-3b/`:

| Document | Content |
| --- | --- |
| [FILESYSTEM-OWNERSHIP](phase-3b/FILESYSTEM-OWNERSHIP.md) | Issue #14: what was hardened now (manifest + identity + digest), and the production design (owned root, handle-relative no-follow access, TOCTOU boundary, what is impossible) |
| [PROVENANCE-DECISION](phase-3b/PROVENANCE-DECISION.md) | eight provenance approaches compared; the decision; essential vs defence-in-depth vs residual risk |
| [PILOT-COMPARISON](phase-3b/PILOT-COMPARISON.md) | jq vs ripgrep vs Gitleaks from primary sources; the selection and the explicit NO-GO conditions |
| [ARCHIVE-EXTRACTION-SPEC](phase-3b/ARCHIVE-EXTRACTION-SPEC.md) | archive vs extracted-file evidence, the checks, the extraction procedure (design), the synthetic negative suite |
| [APPROVAL-AND-AUDIT](phase-3b/APPROVAL-AND-AUDIT.md) | what a real approval needs; key-custody options; why a typed confirmation + unkeyed SHA-256 is not enough |
| [ADAPTER-PROCESS-SAFETY](phase-3b/ADAPTER-PROCESS-SAFETY.md) | boundaries, in-use files, reversibility and acceptance tests of the first pilot adapter |
| [PORTABILITY-AND-MANUAL-TESTS](phase-3b/PORTABILITY-AND-MANUAL-TESTS.md) | the five evidence levels, what is and is not known, manual checklists for another Windows PC and a real Apple Silicon Mac |

## 1. Issue #14 — implemented now (synthetic engine only)

Cleanup used to delete a file because its **name** was expected. It now deletes a file only if it is **provably the one the transaction created**: the journal holds a transaction-owned manifest (written *before* each file is needed) with the file's identity (volume serial + file index on Windows, device + inode on Unix, read from the open handle), size, SHA-256, and every place it may legitimately be; deletion requires a plain file, a single hard link, the same identity and the same digest, all-or-nothing, non-recursive. The staged file is re-verified before activation, the saved pointer before it is restored, and an uninstall is refused before anything moves unless every installed version artifact is vouched for by a committed receipt. A new rule: if the pointer could not be restored, the version folder it may still name is **kept** (no dangling pointer; one Phase 3A test that had asserted the opposite was corrected to the stronger invariant).

Remaining filesystem risks (exact, disclosed in `FILESYSTEM-OWNERSHIP.md`): the engine is still **path-based**, so a same-user process can win a race between check and unlink; ancestors of the caller-chosen base are not verified; ownership of the sandbox root is not yet recorded in the manager's own database; there is no mount-transition check; there is no handle-relative creation/deletion. These are Phase 3C acceptance items; they cannot all be removed (no inode-based unlink on macOS).

## 2. Decisions proposed to the Red Team

1. **Trust anchor:** the compiled-in authenticated reference set (exact-file SHA-256, part of the signed application release), fed by a **reviewer-run, offline attestation verification** against an explicit repository and workflow identity. A checksum next to a release file is never authentication. `identity_verified` is unchanged.
2. **Pilot:** **jq 1.8.2** (raw single executables, GitHub build attestations covering exactly the installed bytes, signed source commit). Fallback: ripgrep (needs the extraction pipeline, weaker provenance). Gitleaks: not recommended. **A real installation remains NO-GO** until the conditions in `PILOT-COMPARISON.md` §3 exist.
3. **Archives:** not needed for the jq pilot; the specification and a metadata-only checker exist so the fallback is not blocked by design later.
4. **Approval:** design only (OS-protected key + user-presence gate; fail closed if neither exists).

## 3. Milestones (planning only; none is started)

| Milestone | Goal | Includes | Excludes |
| --- | --- | --- | --- |
| **3B** (this phase) | trust + pilot decision, Issue #14 hardening | the documents above; manifest/identity hardening; archive checker | any real installation, download, execution, approval UI |
| **3C** | isolated production adapter with synthetic fixtures, **no activation** | handle-relative no-follow filesystem layer for Windows + macOS with the acceptance tests of `FILESYSTEM-OWNERSHIP.md` §9; manager-owned root derived from the app-data dir with ownership recorded in the manager's database; adapter for a **single raw executable** artifact (no extraction code); approval and key-store interfaces behind traits with fake stores only; local test-only artifact source; exact-SHA CI on both OSes; source-scan tests that no process is spawned and no PATH/environment is touched | real download, real recipe, real reference, UI approval, archives, in-application attestation verification, process detection |
| **3D** | the first real pilot, **only after** Red Team approval and a separate explicit user authorization | reviewer verification of the two jq files (authorized download), licence review, reviewed recipe + reference entry, real-OS checks (Authenticode/SAC on Windows; ad-hoc signature/Gatekeeper on a real Mac), one install → update-or-rollback → uninstall cycle on the developer's machines | any other tool, PATH changes, agent configuration, running the tool |

### Minimum safety controls for the first pilot

owner-only managed root; write-ahead journal + manifest identity; digest check at download, staging and activation; reference-set authentication; separate approval with expiry and single use; no process spawn; no PATH/registry/agent-config writes; honest rollback reports; user data untouched; residual-risk text at approval.

### Deferred (explicitly not first-pilot requirements)

extraction code and the gzip layer; in-application Sigstore verification; process-in-use detection; update notifications; multiple tools or versions kept beyond one previous; Windows ARM / Intel macOS; shims or PATH helpers; automated vulnerability feeds; a second independent reviewer.

### Requirements that cannot be met as stated

* "Prove the binary is harmless" — only "it is the reviewed file" is provable.
* "Delete exactly this inode on macOS" — no such system call; verified `unlinkat` plus a disclosed window.
* "Defend against malware running as the same user" — it has the manager's own privileges.
* "Test macOS behaviour in CI as if on a Mac desktop" — CI runners are headless; Gatekeeper, TCC, Keychain prompts and Secure Enclave need a real Mac.

## 4. Tests added in this phase

See the final counts in the README. New: 11 synthetic lifecycle tests for Issue #14 (substitution under expected names, same-content replacement, hard-link alias, link-replaced staging, crash + resume, swap between check and use, forged saved pointer, uninstall preflight, digest-only edit) and 14 archive-checker tests. Mutation checks: identity, link-count, digest and pointer-verification rules (lifecycle); traversal, local-header-name, overlap and digest-reuse rules (archive checker) — each disabled mutation fails at least one test.
