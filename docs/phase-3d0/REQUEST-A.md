# Request A (corrected) — download two pinned files and verify them WITHOUT running them

**Status: a proposal only. No permission to download has been granted. Nothing in this document has been executed against a remote host.** It replaces the wording of Phase 3D-0 §8, which wrongly promised "`github.com` only" and named signature files on the mutable `master` branch (Issue #17, RT-3D0-01/02). Turkish version for the user: [REQUEST-A.tr.md](REQUEST-A.tr.md).

Download-and-verify is permission 2+3 of the five separate permissions. **Installing (4) and executing (5) are not requested and not covered.**

## 1. Pinned inputs (all of them)

| Input | Exact value |
| --- | --- |
| Release | <https://github.com/jqlang/jq/releases/tag/jq-1.8.2> (tag → commit `34f7186b86743a083a589741b6cea95293524108`) |
| File 1 | `jq-windows-amd64.exe` — **1 035 264 bytes** — SHA-256 `a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627` — `https://github.com/jqlang/jq/releases/download/jq-1.8.2/jq-windows-amd64.exe` |
| File 2 | `jq-macos-arm64` — **841 504 bytes** — SHA-256 `2d75340ba57a4b4b4c8708a21c2dc8e958a48aaa8bba13b27f77f6e4c0eca07e` — `https://github.com/jqlang/jq/releases/download/jq-1.8.2/jq-macos-arm64` |
| Signature commit (immutable) | `f0dabad36a5fdb37393829e6bd4c465d70c40a3a` — "Update signatures of 1.8.2 (#3566)", parent = the release commit `34f7186b…`, authored by `github-actions[bot]`, committed by GitHub (web-flow, GitHub reports the signature `valid`), pull request #3566 **approved and merged by the maintainer `itchyny` two minutes after the bot opened it**. Retrieved only by this commit id, never from `master`. |
| Signature 1 | `…/f0dabad…/sig/v1.8.2/jq-windows-amd64.exe.asc` — 833 bytes — SHA-256 `055c433ac06536be0acf06edc727c1b0db2de97e7d3c85a8f1cbef32bffbf2f0` (git blob `75165071fc832a0450a24086543ab7108ed96bf4`) |
| Signature 2 | `…/f0dabad…/sig/v1.8.2/jq-macos-arm64.asc` — 833 bytes — SHA-256 `8ce9edc9647d78a4320de42ce6aa4ae82e7eb3c98a661011cd37ce9ebd32a6f1` (blob `a62e320419372860261b459656d5584e671b472c`) |
| Public key | `…/f0dabad…/sig/jq-release-new.key` — 3 167 bytes — SHA-256 `710cc5f046706bb728479e57aebe0ade744317cbd27ecc0bf5d4c92cb582283b` (blob `baf8464209a818037ba7b62ceb7636d7958da7e5`); required **primary-key fingerprint `93079A9511EFC0B7D6CDBFB5B0DA60FB454BAF18`** (RSA 4096, created 2025-07-31, no expiry, UID `jq Release Signing Key <no-send-email@jqlang.github.io>`, encryption subkey `0343D5D7CCA45BCC2596B849104F445439A0A518`), reviewed by hand in Phase 3D-0 closure |
| Checksum list | `…/f0dabad…/sig/v1.8.2/sha256sum.txt` — 2 302 bytes — SHA-256 `dc86824a41c165ece971ff691aff6e08bbfe6e1d1f531688b47ee78c283a85cd` (equals the release asset digest of `sha256sum.txt`) |
| Identity policy | repository `jqlang/jq` (id `5101141`, owner id `104800540`); workflow certificate identity exactly `https://github.com/jqlang/jq/.github/workflows/ci.yml@refs/tags/jq-1.8.2`; source ref `refs/tags/jq-1.8.2`; source digest and signer digest `34f7186b86743a083a589741b6cea95293524108`; OIDC issuer `https://token.actions.githubusercontent.com`; GitHub-hosted runner; predicate type `https://slsa.dev/provenance/v1` |

The pinned text files and their hashes are stored in `docs/phase-3d0/evidence/pinned-f0dabad…/` and in `tools/review_jq_verification.py`; a mismatch of any byte aborts the session.

**The GPG key and signatures give NO independent publisher authentication.** They come from the same repository as the artifacts, the key was created on 2025-07-31 and all older releases were re-signed with it the same day, the website lists no fingerprint, and the signing step runs in the same release job as the attestation. They are a consistency and defence-in-depth check, nothing more.

## 2. Network policy (corrects Phase 3D-0)

| Destination | Why | Who contacts it | Rule |
| --- | --- | --- | --- |
| `github.com` | the two release-asset URLs (answer with a redirect) | `tools/fetch_pinned.py` | allowed |
| `objects.githubusercontent.com`, `release-assets.githubusercontent.com` | GitHub-controlled asset storage the release URL redirects to — **whichever one GitHub actually uses; both are named in advance** | `tools/fetch_pinned.py` (redirect target) | allowed; **any other host aborts the download** and is reported to the user, who decides |
| `raw.githubusercontent.com` | the pinned text files, addressed by commit id | `tools/fetch_pinned.py` | allowed, commit-pinned URLs only |
| `api.github.com` | attestation lookup for the online verification | GitHub CLI (`gh attestation verify`) | allowed; this is the only place the stored GitHub login of the CLI is used |
| `tuf-repo-cdn.sigstore.dev`, `tuf-repo.github.com` | refresh of the Sigstore trusted root (TUF) — these two hostnames are embedded in the installed CLI (found by a static scan of the binary) | GitHub CLI (`gh attestation trusted-root`, `verify`) | allowed **only because they are named here**; the CLI does not log TUF requests, so they are reported as *expected*, not *observed* |
| anything else | — | — | **forbidden**; no third-party destination is contacted |

Rules: the fetch tool sends **no credential of any kind** (no token, no cookie, no Authorization header — tested against a loopback server with a token in the environment); it follows at most 3 redirects, each of which must be `https` and on the list; it logs every hop (scheme, host, path **without the query string**, status) so the **final actual hosts** are reported; it never executes or opens what it downloads. The GitHub CLI runs with `GH_TELEMETRY=false`, `GH_NO_UPDATE_NOTIFIER=1`, `GH_PROMPT_DISABLED=1`. Disclosure: during Phase 3D-0 the command `gh attestation trusted-root` was run once to obtain the public trusted root; it contacted the two TUF hosts above without their having been listed in advance. That was public metadata only, and it is now listed.

## 3. Procedure (exact commands; Windows PowerShell on the development PC)

All of it happens in a new session folder `$S` below the assistant's scratch directory — outside the repository, outside any managed root, never on PATH. Each downloaded file goes into **its own new, empty folder**; nothing is given an execute permission; nothing is opened by anything except hashing and signature tools.

```powershell
$env:GH_TELEMETRY = "false"; $env:GH_NO_UPDATE_NOTIFIER = "1"; $env:GH_PROMPT_DISABLED = "1"
$S = "<scratch>\jq-review-<UTC date>"        # new folder; sub-folders: artifacts\<file>\, pinned\, gnupg\
$C = "f0dabad36a5fdb37393829e6bd4c465d70c40a3a"
$H = @("--allow-host","github.com","--allow-host","objects.githubusercontent.com","--allow-host","release-assets.githubusercontent.com")

# 1. pinned text files (raw.githubusercontent.com, by commit id), each checked against its pinned size and SHA-256
python -I tools\fetch_pinned.py --url https://raw.githubusercontent.com/jqlang/jq/$C/sig/v1.8.2/jq-windows-amd64.exe.asc --dest $S\pinned\jq-windows-amd64.exe.asc --size 833  --sha256 055c433ac06536be0acf06edc727c1b0db2de97e7d3c85a8f1cbef32bffbf2f0 --allow-host raw.githubusercontent.com --log $S\fetch.log
python -I tools\fetch_pinned.py --url https://raw.githubusercontent.com/jqlang/jq/$C/sig/v1.8.2/jq-macos-arm64.asc          --dest $S\pinned\jq-macos-arm64.asc          --size 833  --sha256 8ce9edc9647d78a4320de42ce6aa4ae82e7eb3c98a661011cd37ce9ebd32a6f1 --allow-host raw.githubusercontent.com --log $S\fetch.log
python -I tools\fetch_pinned.py --url https://raw.githubusercontent.com/jqlang/jq/$C/sig/jq-release-new.key                  --dest $S\pinned\jq-release-new.key          --size 3167 --sha256 710cc5f046706bb728479e57aebe0ade744317cbd27ecc0bf5d4c92cb582283b --allow-host raw.githubusercontent.com --log $S\fetch.log
python -I tools\fetch_pinned.py --url https://raw.githubusercontent.com/jqlang/jq/$C/sig/v1.8.2/sha256sum.txt                --dest $S\pinned\sha256sum.txt               --size 2302 --sha256 dc86824a41c165ece971ff691aff6e08bbfe6e1d1f531688b47ee78c283a85cd --allow-host raw.githubusercontent.com --log $S\fetch.log

# 2. the two executables, each alone in its own folder (redirects limited to the hosts above; any other host aborts)
python -I tools\fetch_pinned.py --url https://github.com/jqlang/jq/releases/download/jq-1.8.2/jq-windows-amd64.exe --dest $S\artifacts\jq-windows-amd64.exe\jq-windows-amd64.exe --size 1035264 --sha256 a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627 @H --log $S\fetch.log
python -I tools\fetch_pinned.py --url https://github.com/jqlang/jq/releases/download/jq-1.8.2/jq-macos-arm64        --dest $S\artifacts\jq-macos-arm64\jq-macos-arm64               --size 841504  --sha256 2d75340ba57a4b4b4c8708a21c2dc8e958a48aaa8bba13b27f77f6e4c0eca07e @H --log $S\fetch.log

# 3. fresh Sigstore trusted root (TUF hosts above) — the snapshot stored in the repository is NOT used as a trust anchor
gh attestation trusted-root > $S\trusted_root.jsonl

# 4. the MAINTAINED verifier, strict policy, for each file (<name> = jq-windows-amd64.exe, then jq-macos-arm64)
gh attestation verify $S\artifacts\<name>\<name> --repo jqlang/jq `
  --cert-identity "https://github.com/jqlang/jq/.github/workflows/ci.yml@refs/tags/jq-1.8.2" `
  --cert-oidc-issuer https://token.actions.githubusercontent.com `
  --source-ref refs/tags/jq-1.8.2 --source-digest 34f7186b86743a083a589741b6cea95293524108 --signer-digest 34f7186b86743a083a589741b6cea95293524108 `
  --deny-self-hosted-runners --predicate-type https://slsa.dev/provenance/v1 --hostname github.com --format json > $S\<name>.gh.json ; "$LASTEXITCODE" | Set-Content -NoNewline $S\<name>.gh.exit
#    the same, offline replay with the stored bundle and the FRESH root from step 3 (adds --bundle docs\phase-3d0\evidence\<bundle> --custom-trusted-root $S\trusted_root.jsonl) -> <name>.gh-offline.json / .exit

# 5. the supporting (custom) verifier, with the fresh root, comparing the DOWNLOADED bytes
python -I tools\verify_sigstore_bundle.py --bundle docs\phase-3d0\evidence\<bundle> --trusted-root $S\trusted_root.jsonl --subject-name <name> --subject-sha256 <pinned> `
  --repo-uri https://github.com/jqlang/jq --workflow-path .github/workflows/ci.yml --ref refs/tags/jq-1.8.2 --commit 34f7186b86743a083a589741b6cea95293524108 `
  --repo-id 5101141 --owner-id 104800540 --artifact $S\artifacts\<name>\<name> > $S\<name>.custom.json

# 6. GPG with an ISOLATED keyring, no key servers, no network
$env:GNUPGHOME = "$S\gnupg"
gpg --batch --no-tty --disable-dirmngr --no-auto-key-retrieve --no-auto-key-locate --import $S\pinned\jq-release-new.key
gpg --batch --no-tty --list-keys --with-colons --with-fingerprint > $S\keyring.colons
gpg --batch --no-tty --disable-dirmngr --no-auto-key-retrieve --no-auto-key-locate --status-fd=1 --verify $S\pinned\<name>.asc $S\artifacts\<name>\<name> > $S\<name>.gpg.status

# 7. static facts only (no execution): Authenticode status of file 1 (Get-AuthenticodeSignature); PE / Mach-O header fields read as bytes

# 8. consensus review — exits 0 only if EVERYTHING agrees
python -I tools\review_jq_verification.py $S
```

Exact flags were checked against the installed GitHub CLI **2.101.0**: `--cert-identity` and `--signer-workflow` are mutually exclusive (so the stricter exact-identity form is used, which already contains workflow path and ref); a decoy file with the stored bundle and root is rejected without any API request (offline path is network-free); the CLI's positive path could not be exercised before real bytes exist.

## 4. Fail-closed rules

1. Any hop to a host outside the list, any non-HTTPS hop, more than 3 redirects, a wrong size or SHA-256, a missing pinned file → abort, keep nothing from that download, report the host/reason; the user decides.
2. **If the GitHub CLI result and the custom verifier disagree in either direction, the session fails** (tested). The custom verifier is supporting evidence, never the authority; the stored trusted-root snapshot is not trusted merely because its digest is written in the same repository — the session uses a root freshly obtained through the CLI's own TUF client.
3. The review requires: pinned size and SHA-256 of both files; the digest present in the pinned publisher checksum list; the official verifier's structured result (subject, predicate type, log timestamp, every certificate field the CLI prints); the supporting verifier's 28 checks **including the downloaded bytes**; for each file exactly one valid GPG signature by the pinned fingerprint (signing key and primary key), no bad/expired/revoked/errored signature, a keyring holding exactly that one certificate; all hops https and on the host list; no execute permission on either file.
4. Nothing from this session creates or changes any recipe, reference, catalog entry or `identity_verified` value. Promotion is a separate reviewed code change after the Red Team has accepted the evidence.

## 5. Evidence retained for independent review

The whole session folder is **kept** (not deleted) until the Red Team has reviewed it, unless the user asks otherwise: the two files, the five pinned text files, `fetch.log` (hops and final hosts), `trusted_root.jsonl`, every `*.gh.json/.exit`, `*.gh-offline.json/.exit`, `*.custom.json`, `*.gpg.status`, `keyring.colons`, the static-fact notes and the reviewer's verdict. Its SHA-256 listing is recorded in the repository (not the binaries). Existing backups are never touched.

## 6. Effects

About 1.9 MB of executables plus about 7 KB of text downloaded; roughly 40 KB of verification output written; one isolated keyring folder; **no** change to any system setting, registry key, PATH, startup item, agent configuration, Smart App Control, Defender or Gatekeeper; no installation; no execution of either file (not even `--version`); nothing placed in the repository except sanitized result records and the SHA-256 listing. Windows Defender may scan the files when they are written — that is the operating system's own behaviour and is recorded if it quarantines anything.

## 7. Not requested

Executing either file; installing anything; adding a digest to any reference list or recipe; any other file, version or platform; a macOS `codesign`/Gatekeeper check (needs a real Mac and is a separate step); any network use by the application itself.
