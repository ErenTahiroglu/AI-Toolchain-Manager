# Evidence files for the jq 1.8.2 provenance review (public metadata only)

Collected read-only on 2026-10-18 (Phase 3D-0). **No binary artifact was downloaded and nothing was executed.** These files are *evidence for a reviewer*; the application does not read them and trusts none of them.

| File | What it is | How it was obtained |
| --- | --- | --- |
| `jq-1.8.2-windows-amd64.attestations.json` | GitHub attestations API response for the digest of `jq-windows-amd64.exe` (the same bundle twice: `jq-win64.exe` has the identical digest) | `gh api repos/jqlang/jq/attestations/sha256:a6fc67fe…c627` |
| `jq-1.8.2-macos-arm64.attestations.json` | the same for `jq-macos-arm64` | `gh api repos/jqlang/jq/attestations/sha256:2d75340b…ca07e` |
| `sigstore-trusted-root.jsonl` | the Sigstore trusted root (public-good and GitHub instances) as embedded and TUF-verified by the GitHub CLI | `gh attestation trusted-root` — SHA-256 of the stored file: `65ca537f6ed8a47fd0e560c421baa1f6c1efb8b25fc200d8c5c02c0e92eb2b9c` |
| `jq-1.8.2-sha256sum.txt` | the publisher's checksum list for 1.8.2 as committed to the repository | `gh api repos/jqlang/jq/contents/sig/v1.8.2/sha256sum.txt` |
| `report-windows-amd64.json`, `report-macos-arm64.json` | output of `tools/verify_sigstore_bundle.py` (28 checks each, all passed) | see below |

Reproduce (no network, nothing executed):

```
python -I tools/verify_sigstore_bundle.py --bundle docs/phase-3d0/evidence/jq-1.8.2-windows-amd64.attestations.json \
  --trusted-root docs/phase-3d0/evidence/sigstore-trusted-root.jsonl --subject-name jq-windows-amd64.exe \
  --subject-sha256 a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627 --repo-uri https://github.com/jqlang/jq \
  --workflow-path .github/workflows/ci.yml --ref refs/tags/jq-1.8.2 --commit 34f7186b86743a083a589741b6cea95293524108 \
  --repo-id 5101141 --owner-id 104800540
```

The trusted-root snapshot was current on 2026-10-18; the Sigstore roots rotate, so a later reviewer must refresh it (and say so) rather than reuse this file. Verifying the attestation bundle proves what the publisher's pipeline *claims* it built; it says nothing about the bytes of a file until that file is hashed and compared (`--artifact`), which was not done in this phase.
