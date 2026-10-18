# Gitleaks as a candidate for a first real-world pilot — eligibility assessment (research only)

Status: **assessment, not permission.** Gitleaks is *not* pre-approved. Nothing was downloaded, extracted, executed or installed to write this, and the application contains no Gitleaks recipe, digest or reference (the compiled-in recipe list and the authenticated reference set are both empty, with tests). The facts below were read from public GitHub metadata through a read-only API on **2026-10-18** — that was research by the developer's assistant, not application runtime behaviour — and they can change.

## Why Gitleaks is a reasonable *candidate* (and why that is not a reason to pick it)

* A single native command-line scanner, MIT-licensed (`LICENSE` = MIT per the GitHub license API), no agent plugin, no hooks, no background service, no required runtime: the smallest possible footprint among the catalogued tools.
* Release assets exist for exactly the two supported targets: `gitleaks_8.30.1_windows_x64.zip` and `gitleaks_8.30.1_darwin_arm64.tar.gz` (plus other platforms).
* It is **not** chosen because it is convenient: the catalog entry is still `unreviewed`, and the selection of a pilot tool needs the Red Team's separate decision.

## What the evidence actually is (v8.30.1, published 2026-03-21)

| Question | Finding | Consequence |
| --- | --- | --- |
| Official distribution | GitHub release of `gitleaks/gitleaks` (11 uploaded assets: 10 platform archives + `gitleaks_8.30.1_checksums.txt`; the release page counts 13 with its two automatic source archives) | Same origin as the code; no independent channel |
| Pinned release version | tag `v8.30.1` → commit `83d9cd684c87d95d656c1458ef04895a7f1cbd8e` | The tag is a **lightweight** tag (ref type `commit`, not a signed annotated tag) and the commit is **unsigned** (`verification.reason = unsigned`). A commit id is only a *declared pin*, never proof of origin (Issue #11) |
| Independent signature | **None found**: no `.sig`, `.pem`, `.sigstore`, `.intoto.jsonl` or SBOM asset; GitHub artifact-attestation lookup for the Windows asset returned *Not Found* | No independent authentication exists today |
| Digest | A `checksums.txt` is published **in the same release**, and GitHub itself reports a `sha256` per asset | Both are same-origin with the files: they detect corruption, not a malicious publisher or a compromised release. They cannot be the "independently authenticated" evidence the trust model requires |
| What is digested | The release assets are **archives** (`.zip`, `.tar.gz`). No digest of the extracted `gitleaks.exe` / `gitleaks` is published | Policy since Phase 2C: an archive digest never authenticates the extracted file. The recipe model only accepts an exact-file digest. Either an independent party must publish/attest the extracted binary's digest, or extraction must be designed and reviewed (formats, path traversal, size, symlinks) before an archive can be used |
| Compatible artifacts | Windows x64 zip and macOS arm64 tar.gz exist. Whether the macOS binary is signed/notarised was not determined (no download was allowed) | Both would be checked as exact-file Windows PE x64 / Mach-O arm64 formats by the format sniffer; the macOS signature question is open |
| License and redistribution | MIT. The manager would not redistribute the binary; it would fetch the publisher's release at install time | License text must still be captured as reviewed evidence (`R` row in RESEARCH-SOURCES) and the notice kept next to the installed files |
| Repository state | not archived; default branch `master`; last push 2026-09-30; `SECURITY.md` exists | A security reporting channel exists; no support guarantee. The catalog notes that new features are frozen |

## What a pilot would require (all missing today)

1. **An independently authenticated digest of the exact file that is installed**, from a source the manager can verify without trusting GitHub-the-publisher (for example a reviewed, pinned-key signature over a manifest produced by a party independent of the release pipeline — whose design, key pinning, rotation and revocation are themselves a review item).
2. **A reviewed extraction design** if archives stay the artifact form (or a recipe form whose `digestScope` is `archive` with a separate, authenticated inner-file digest).
3. **A reviewed recipe** (`schemas/reviewed-recipe.schema.json`) for exactly one version, one target (`windows-x64` / `macos-arm64`), surface `local-cli`, user scope, with an accepted license review, a staging check on the target by someone independent of the author, and a real (not simulated) rollback check.
4. **Manager-owned install location only** (`versions/gitleaks/<version>/`, a pointer file as the activation switch). No PATH change, no shell profile edit, no agent configuration. How a person then *runs* it (a shim in a manager-owned folder the user adds to PATH themselves, or an explicit "run" feature) is an open design question that must not default to changing PATH.
5. **Receipts and journals** as implemented for the synthetic engine, but exercised against a real target; **removal** = unregister the pointer and delete the manager-owned version folders, keeping any reports the user created; **update** = new exact version, new recipe revision, new approval, pointer switch, previous version retained for rollback.
6. **Rollback** verified on a real Windows x64 and a real macOS arm64 machine (today it is only simulated in temporary directories).
7. **Session safety.** Gitleaks is not an agent, so no agent session is involved; the remaining risk is a scan that is *running* when the pointer is switched. The engine does not yet detect running processes (it must not start or probe any), so an update/uninstall must be refused or deferred when the file is in use (Windows locks it; macOS does not) — a design item, not solved.
8. **Output hygiene.** Gitleaks can print the secrets it finds. The manager must not capture, display, store or sync its output; the pilot should not run it at all in the first iteration (installation, removal and rollback only).
9. Independent review of the installer path itself (process-tree/job-object control, environment isolation, an adversarial suite), cross-platform exact-SHA CI, a manual macOS pass on real hardware, and a separate explicit user authorization.

## Verdict

**Not eligible today.** The blocking items are (1) no independent authentication of any digest, (2) archive-only artifacts with no authenticated inner-file digest, (3) no verified rollback on real targets, and (4) no running-process safety. Gitleaks remains the *shape* of tool the first pilot should have (single static binary, no integration), so Phase 3B should build the missing authentication path generically and then re-evaluate the candidate with fresh evidence.

## Update (Phase 3B, 2026-10-18)

The evidence above was re-read and compared with two other candidates in [phase-3b/PILOT-COMPARISON.md](phase-3b/PILOT-COMPARISON.md). Result: Gitleaks remains **not eligible**, and is **not recommended** as the first pilot (weakest provenance, archive-only artifacts, prints secrets); the proposed pilot is jq 1.8.2 (raw single executables with publisher build attestations), pending the Red Team's decision and the NO-GO conditions listed there. The trust model that would make any pilot possible is in [phase-3b/PROVENANCE-DECISION.md](phase-3b/PROVENANCE-DECISION.md).
