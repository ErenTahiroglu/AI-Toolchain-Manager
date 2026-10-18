# jq 1.8.2 — the review package for a FUTURE first pilot (preparation only)

**Nothing here is a permission and nothing here was executed.** No jq file was downloaded, extracted, probed or run in Phase 3C. The values below were read from public GitHub release/attestation *metadata* on 2026-10-18 (Phase 3B) and are **declared, not verified**: they are the starting point of the review, not an authenticated reference. The application contains no jq recipe, digest or reference; the production recipe list and the authenticated-reference set stay empty.

## 1. What the reviewer must establish (each is a gate; none is done)

| # | Item | Declared (metadata only) | What must be checked, and by whom | Status |
| --- | --- | --- | --- | --- |
| 1 | Windows x64 artifact | release asset `jq-windows-amd64.exe`, 1 035 264 bytes, published digest `sha256:a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627` | a reviewer downloads **only this file**, in a throw-away environment and with the user's separate written permission, hashes it, and compares | **not done** |
| 2 | macOS ARM64 artifact | release asset `jq-macos-arm64`, 841 504 bytes, published digest `sha256:2d75340ba57a4b4b4c8708a21c2dc8e958a48aaa8bba13b27f77f6e4c0eca07e` | same, on a real Apple Silicon Mac | **not done** |
| 3 | Attestation | GitHub artifact attestations exist for both files (Sigstore bundle v0.3, DSSE in-toto, SLSA provenance v1) | verify with the official verifier against an **explicit repository and signer workflow** (below); store the verifier output, not just "OK" | **not done** |
| 4 | Expected signer identity | repository `jqlang/jq`; workflow `.github/workflows/ci.yml` at ref `refs/tags/jq-1.8.2`; certificate issuer `https://token.actions.githubusercontent.com`; trigger `push`; environment `release`; GitHub-hosted runners | the verifier must be given these as constraints; any other identity is a failure | **not done** |
| 5 | Exact upstream source | commit `34f7186b86743a083a589741b6cea95293524108` (signature on the commit reported valid; the tag `jq-1.8.2` is lightweight) | the attestation's source digest must equal this commit; the tag must still point to it; review the diff since the previous release for anything beyond the stated fixes | **not done** |
| 6 | Licence and bundled notices | GitHub licence API says `NOASSERTION` (jq is MIT-licensed with additional third-party notices) | read `COPYING` and every bundled notice; decide what must be shown to the user and kept next to the installed file | **not done** |
| 7 | Reviewed reference digest | — | the reviewer's verified digest, **two** entries (one per platform), added by code change to the compiled-in authenticated reference set **only after** Red Team approval of this package; provenance text names the reviewer and the date | **not done** (nothing was added) |
| 8 | Windows Smart App Control compatibility | unknown whether the Windows exe is Authenticode-signed | on a real Windows PC, record the signature status of the file (read-only) and what SAC does when the file is *first run*; the manager itself never runs it. SAC and Defender are never altered | **not done** |
| 9 | macOS Gatekeeper and code signature | unknown whether the macOS binary carries at least an ad-hoc signature (required to run on Apple Silicon) | on a real Apple Silicon Mac record `codesign` status, quarantine attribute behaviour for a file the manager wrote, and the exact Gatekeeper message; never strip quarantine or disable Gatekeeper | **not done** |
| 10 | Independent clean-machine staging check | — | a person other than the author stages the two files with the Phase 3C adapter *against the real recipe* on a clean machine and records the receipts, the rollback and the removal | **not done** |

## 2. Verification commands to be used by the reviewer (templates; NOT executed)

The official GitHub CLI verifier needs the artifact file. With the file in a clean, empty directory (and the bundle stored next to it for an offline re-check):

```
gh attestation verify <file> --repo jqlang/jq --signer-workflow jqlang/jq/.github/workflows/ci.yml --format json
gh attestation verify <file> --bundle <bundle.jsonl> --repo jqlang/jq --signer-workflow jqlang/jq/.github/workflows/ci.yml
sha256sum <file>            # or the platform equivalent; compare with the verifier's subject digest AND with the release page
```

The reviewer records: tool version, command line, full JSON result, the certificate identity and issuer fields, the Rekor log index, the source commit and the digest, and compares the digest with the value in rows 1–2. An attestation proves **where and from which source the bytes were built**; it does not prove the bytes are benign (see `../phase-3b/PROVENANCE-DECISION.md` §3, residual risks).

## 3. What the manager will and will not do with it (design, Phase 3D)

* Fetch **only** the exact release asset named in a reviewed recipe, over HTTPS, after a separate explicit user authorization; verify the digest before writing anything; stage it, re-verify, place it in a manager-owned versioned folder, switch the pointer atomically, write a receipt.
* Never run it (no `--version`, no self-test), never change the program search path, never write agent configuration, never request elevation.
* Show the user the declared limits before approval: provenance proves origin, not harmlessness; no revocation reaches a device that is not updated; Windows may refuse to run an unsigned file; macOS may require the user to approve it.

## 4. Readiness assessment

jq 1.8.2 remains a *reasonable candidate* (single raw executable per platform, no extraction needed, publisher build attestations that cover exactly the installed bytes). It is **not ready**: rows 1–10 are all open, the reference set is empty on purpose, the real approval flow and real OS key custody do not exist, and the isolated adapter has not been proven on a real Mac or on a second Windows computer. A real installation stays **NO-GO** until every row is closed, the Red Team has approved, and the user has given separate written permission.
