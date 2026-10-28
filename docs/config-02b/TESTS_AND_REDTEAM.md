# CONFIG-02B — tests, OS-specific outcomes, red-team questions, suggested follow-up

Issue #39. The numbers and CI results of a particular run live in the Issue's evidence comment, not here. This document says what the tests assert and which of them can be skipped; it is a developer's description, not an independent review.

## Where the tests are

| File | Scope |
|---|---|
| `src-tauri/src/config_txn_tests.rs` | pure: strict JSON, merge, plan binding, approval fixture, refusal codes; reads the shared golden file |
| `src-tauri/src/managed_fs_mcp_tests.rs` | file-system half, on directories created by each test under the test runner's temporary area |
| `src/configPlanGolden.test.ts` | the TypeScript preview produces the same "after" text as the shared golden cases |
| `tests/fixtures/config-02b-golden.json` | the golden cases, read by both languages |
| `tests/test_config02b_contract.py` | isolation: test-only compilation, no IPC/capability/migration/dependency change, constants equal the reviewed declaration and the catalog pin, the documents keep the boundaries |

## What the negative tests cover (Rust, synthetic roots)

Stale hash or changed content; same bytes in a different file object (identity); unexpectedly existing file (before the plan is committed, and appearing *inside* the last window); vanished file; duplicate and case-variant `exa`; same endpoint under another name (six spellings); malformed JSON, comments, trailing commas, duplicate keys (also via escapes), BOM, lone surrogates, oversize; the preview's mask text in the file; wrong project, OS or surface; a plan committed into another root; approval missing, for another plan, expired, reused; two competing plans (sequential and one committing inside the other's window); failure injected at every step; failure after the swap; crash leftover; an unsupported volume (`WRITE_ADAPTER_BLOCKED`, no fallback); non-private or project-root backup folder; existing backup of the same name; directory, hard-linked, case-aliased target; junction/symlink at the target, swapped in inside the last window, as the root itself and among the root's ancestors; rollback only of the written file and never over a later edit; a snapshot of the whole test area proving that the only changes are `.mcp.json` and the backup file, and that a canary outside the root and every other agent's file are untouched.

## Platform-dependent outcomes

| Case | Windows | macOS |
|---|---|---|
| Directory junction / symlink at the target, as the root, among ancestors | exercised with `mklink /J` | exercised with a symlink |
| **File symlink** at the target | **SKIPPED when the account lacks the symlink privilege** (the test prints `CONFIG02B_SKIPPED: …`); otherwise exercised | exercised |
| Link swapped in during the last window | refused as `target_not_regular_file` (the destination is opened as a plain file) | refused as `target_is_link` |
| In-place edit inside the replace window | `windows_does_not_detect_…`: documents that the edit is **lost and not detected** | `macos_detects_…`: detected, kept, `NeedsAttention` |
| Hard link, case alias | exercised; a case-variant name is refused on every file system because the match ignores case | same |
| Atomic probe on the volume | asserted for the test volume | asserted for the test volume |

A test that cannot build its situation prints `CONFIG02B_SKIPPED` and returns; the Rust harness has no skip state, so **the evidence comment counts those lines separately and never as passes**. Cases not exercised at all: a locked or read-only target, long paths, non-ASCII roots, network shares, a crash *between* every pair of steps (only injected failures are tested), concurrent OS threads racing the same file for real (the competing-plan test is deterministic by construction), preservation of permissions.

## Questions for the independent red team

1. Is `managed_fs::imp::rename_file` with `Mode::Replace` an acceptable basis for replacing a **user-edited** file, given that the Phase 3C layer was designed for manager-owned files? In particular the Windows window (§8 of the contract) — close, accept with a stated risk, or require a different primitive?
2. Should the replace be refused outright where the displaced original cannot be retained (Windows today)?
3. Is a hard-link count of exactly 1 and a no-follow chain the right acceptance rule for a project that sits under a symlinked parent (refused conservatively today)? What should the selection UX do then?
4. Is the backup folder's requirement (separate, owner-only, never inside the project) enough, and where must the recovery record live?
5. Is identity-only authority for deleting the staging file (no content compare) acceptable given Issue #14's concern about substituted expected-name files?
6. Does a one-shot approval bound to a digest, a root identity and a short lifetime cover the TOCTOU concerns that matter for consent, or must the consent also bind the displayed text's own digest?
7. Is refusing a file that contains `«gizlendi»` the right way to keep preview text out of the writer, or should the writer be made unable to receive text at all (it already receives none: only a root handle)?
8. Does the TypeScript/Rust golden parity belong in the contract, given that the preview normalises integer-like keys and re-prints numbers while the writer keeps order and number text?
9. Is the reviewed Exa declaration itself still acceptable now that it would be written, not shown? (It remains a syntax-only claim.)

## Suggested single follow-up (not authorised)

**CONFIG-02C — read-only acquisition of the real `.mcp.json` of one user-selected project**, no write. A native directory chooser returns a handle; the application stores that root's identity (not just its text) in the project registry, reads `.mcp.json` through the existing no-follow layer, and feeds the **real** bytes to the preview so that the before/after the user sees is of the real file. It exercises the root-identity and read primitives of this prototype against a real project at no risk of changing it, delivers visible product value (an honest diff of the user's actual file), and leaves every write question — including the Windows replace window — to a later, separately approved step. It needs its own threat model, its own Tauri capability decision and independent review; none of that is started here.
