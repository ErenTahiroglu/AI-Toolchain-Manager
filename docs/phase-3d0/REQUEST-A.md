# Request A (corrected) — download two pinned files and verify them WITHOUT running them

**Status: a proposal only. No permission to download has been granted. Nothing in this document has been executed against a remote host.** It replaces the wording of Phase 3D-0 §8, which wrongly promised "`github.com` only" and named signature files on the mutable `master` branch (Issue #17, RT-3D0-01/02). Turkish version for the user: [REQUEST-A.tr.md](REQUEST-A.tr.md).

> **Executed once on 2026-10-18** (sanitized record: `evidence/request-a-session-20261018/`). The execution revealed one unauthorised network contact by the GitHub CLI and two procedure gaps; the procedure below is the CORRECTED one (§8, [REQUEST-A-NETWORK-INCIDENT.md](REQUEST-A-NETWORK-INCIDENT.md), Issue #19). A new run needs its own explicit user authorisation.

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
| `tmaproduction.blob.core.windows.net` | Azure storage for the attestation bundle (signed `bundle_url` from the attestations API); used by the **online** form of `gh attestation verify` | GitHub CLI | **NOT authorised.** Contacted without authorisation in the first execution (incident record); not added. Only a separate explicit user authorisation naming it could allow a future online check |
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

# 3. fresh Sigstore trusted root: the only network step of the verification; TUF hosts only, behind the egress guard
#    (a refused attempt = incident, exit 97; the snapshot stored in the repository is NOT used as a trust anchor)
python -I tools\egress_guard.py --allow-host tuf-repo-cdn.sigstore.dev --allow-host tuf-repo.github.com --report $S\trusted_root.guard.json --child-stdout $S\trusted_root.jsonl --child-stderr $S\trusted_root.err -- gh attestation trusted-root

# 4. the MAINTAINED verifier, strict policy, OFFLINE REPLAY with the retained bundle: this is the default and needs NO host at all.
#    gh --bundle wants one bare Sigstore bundle per line; the stored evidence is the GitHub API response wrapper. The extractor copies each
#    bundle BYTE FOR BYTE (no re-serialisation; a re-serialisation was used in the first execution and gave different bytes with the same
#    content), so no signature, certificate or trusted-root check is touched or weakened.
#    (<name>/<stored> = jq-windows-amd64.exe/jq-1.8.2-windows-amd64, then jq-macos-arm64/jq-1.8.2-macos-arm64)
python -I tools\extract_sigstore_bundles.py --in docs\phase-3d0\evidence\<stored>.attestations.json --out $S\bundles\<name>.jsonl
python -I tools\egress_guard.py --report $S\<name>.gh-offline.guard.json --child-stdout $S\<name>.gh-offline.json --child-stderr $S\<name>.gh-offline.err -- `
  gh attestation verify $S\artifacts\<name>\<name> --repo jqlang/jq --bundle $S\bundles\<name>.jsonl --custom-trusted-root $S\trusted_root.jsonl `
  --cert-identity "https://github.com/jqlang/jq/.github/workflows/ci.yml@refs/tags/jq-1.8.2" `
  --cert-oidc-issuer https://token.actions.githubusercontent.com `
  --source-ref refs/tags/jq-1.8.2 --source-digest 34f7186b86743a083a589741b6cea95293524108 --signer-digest 34f7186b86743a083a589741b6cea95293524108 `
  --deny-self-hosted-runners --predicate-type https://slsa.dev/provenance/v1 --hostname github.com --format json
"$LASTEXITCODE" | Set-Content -NoNewline $S\<name>.gh-offline.exit      # the guard has an EMPTY allowlist: any connection attempt is an incident (97)
#    The ONLINE form (the same command without --bundle/--custom-trusted-root) is NOT part of this procedure: it contacts api.github.com, the TUF
#    hosts AND the signed-storage host above. It may be run only after a separate explicit authorisation that names every host, and then only behind
#    the guard with exactly those hosts. If the guard refuses anything, STOP and report; never add a host to make a run pass.

# 5. the supporting (custom) verifier, with the fresh root, comparing the DOWNLOADED bytes
python -I tools\verify_sigstore_bundle.py --bundle docs\phase-3d0\evidence\<bundle> --trusted-root $S\trusted_root.jsonl --subject-name <name> --subject-sha256 <pinned> `
  --repo-uri https://github.com/jqlang/jq --workflow-path .github/workflows/ci.yml --ref refs/tags/jq-1.8.2 --commit 34f7186b86743a083a589741b6cea95293524108 `
  --repo-id 5101141 --owner-id 104800540 --artifact $S\artifacts\<name>\<name> > $S\<name>.custom.json

# 6. GPG with an ISOLATED keyring, no key servers, no network, no new software.
#    GNUPGHOME must be spelled the way the INSTALLED gpg expects. Git for Windows' gpg is an MSYS program: it reads "C:\..." as a relative path
#    (first execution: "keyblock resource ... No such file or directory", exit 2, ERRSIG + NO_PUBKEY = a TOOL-INVOCATION error, not a
#    cryptographic failure) and needs the POSIX spelling "/c/Users/...". Check the spelling first: `gpg --version` must print a "Home:" line
#    that is exactly the session folder (no doubled prefix).
$env:GNUPGHOME = "/" + $S.Substring(0,1).ToLower() + $S.Substring(2).Replace("\","/") + "/gnupg"      # Git-for-Windows gpg; a native Windows gpg takes "$S\gnupg"
gpg --version | Select-String "Home:"
gpg --batch --no-tty --disable-dirmngr --no-auto-key-retrieve --no-auto-key-locate --status-fd=1 --import $S\pinned\jq-release-new.key > $S\gpg-import.status 2> $S\gpg-import.err ; "$LASTEXITCODE" | Set-Content -NoNewline $S\gpg-import.exit
#    Observed in the first execution: the import printed "imported: 1" but exited 2 because gpg tried to start a local gpg-agent
#    ("error running '/usr/bin/gpg-agent': exit status 2"); an agent is not needed to import a PUBLIC key. Cause of the agent failure: INFERRED
#    (Git-for-Windows' agent cannot start in this isolated home), not investigated. Exit 2 from the import is accepted ONLY IF gpg-import.status
#    contains an IMPORT_OK line for the pinned fingerprint AND keyring.colons (next line) lists exactly that one primary key; otherwise STOP.
gpg --batch --no-tty --list-keys --with-colons --with-fingerprint > $S\keyring.colons
gpg --batch --no-tty --disable-dirmngr --no-auto-key-retrieve --no-auto-key-locate --status-fd=1 --verify $S\pinned\<name>.asc $S\artifacts\<name>\<name> > $S\<name>.gpg.status 2> $S\<name>.gpg.err ; "$LASTEXITCODE" | Set-Content -NoNewline $S\<name>.gpg.exit
#    The verify must exit 0 with GOODSIG and exactly one VALIDSIG by the pinned fingerprint (signing key AND primary key). Anything else: classify with
#    tools/review_jq_verification.classify_gpg_status(): only a clearly environmental cause (the keyring folder could not be opened) is an
#    "invocation_error" (fix the invocation and run again, keeping the failed files as attempt records); BADSIG, EXPKEYSIG, REVKEYSIG, a missing
#    key without such a cause, or anything unexpected is a "cryptographic_failure" and stays fail-closed.

# 7. static facts only (no execution): Authenticode status of file 1 (Get-AuthenticodeSignature); PE / Mach-O header fields read as bytes

# 8. consensus review — six separate facts; exits 0 only if EVERYTHING agrees AND the network policy was kept.
#    $S\network-report.json is the egress guard's report of the one network step (or an incident record of the same shape); without it, or with any
#    incident or unlisted host, network_policy_compliant is false
python -I tools\review_jq_verification.py $S --network-report $S\network-report.json     # exit 0 / 1 (a check failed) / 3 (consistent but network policy violated or unproven)
```

Exact flags were checked against the installed GitHub CLI **2.101.0**: `--cert-identity` and `--signer-workflow` are mutually exclusive (so the stricter exact-identity form is used, which already contains workflow path and ref); a decoy file with the stored bundle and root is rejected without any API request (offline path is network-free); the CLI's positive path could not be exercised before real bytes exist.

## 4. Fail-closed rules

1. Any hop to a host outside the list, any non-HTTPS hop, more than 3 redirects, a wrong size or SHA-256, a missing pinned file → abort, keep nothing from that download, report the host/reason; the user decides.
2. **If the GitHub CLI result and the custom verifier disagree in either direction, the session fails** (tested). The custom verifier is supporting evidence, never the authority; the stored trusted-root snapshot is not trusted merely because its digest is written in the same repository — the session uses a root freshly obtained through the CLI's own TUF client.
3. The review requires: pinned size and SHA-256 of both files; the digest present in the pinned publisher checksum list; the official verifier's structured result (subject, predicate type, log timestamp, every certificate field the CLI prints); the supporting verifier's 28 checks **including the downloaded bytes**; for each file exactly one valid GPG signature by the pinned fingerprint (signing key and primary key), no bad/expired/revoked/errored signature, a keyring holding exactly that one certificate; all hops https and on the host list; no execute permission on either file.
4. **Publication is atomic and never overwrites (Issue #18).** A verified download waits in a uniquely named, exclusively created staging file in the destination folder and is published with a hard link, which the operating system refuses if any entry (file, directory, symlink, dangling symlink) already has the final name, even one created a moment earlier. There is no check-then-replace, rename-over or copy. The folder is probed with throw-away files before any network use; if the filesystem cannot do this the download is refused (never a fallback). Only files this process created are ever removed; unknown contents are preserved, and a staging name that cannot be removed is reported as `leftovers`.
5. Nothing from this session creates or changes any recipe, reference, catalog entry or `identity_verified` value. Promotion is a separate reviewed code change after the Red Team has accepted the evidence.

## 5. Evidence retained for independent review

The whole session folder is **kept** (not deleted) until the Red Team has reviewed it, unless the user asks otherwise: the two files, the five pinned text files, `fetch.log` (hops and final hosts), `trusted_root.jsonl`, every `*.gh.json/.exit`, `*.gh-offline.json/.exit`, `*.custom.json`, `*.gpg.status`, `keyring.colons`, the static-fact notes and the reviewer's verdict. Its SHA-256 listing is recorded in the repository (not the binaries). Existing backups are never touched.

## 6. Effects

About 1.9 MB of executables plus about 7 KB of text downloaded; roughly 40 KB of verification output written; one isolated keyring folder; **no** change to any system setting, registry key, PATH, startup item, agent configuration, Smart App Control, Defender or Gatekeeper; no installation; no execution of either file (not even `--version`); nothing placed in the repository except sanitized result records and the SHA-256 listing. Windows Defender may scan the files when they are written — that is the operating system's own behaviour and is recorded if it quarantines anything.

## 7. Not requested

Executing either file; installing anything; adding a digest to any reference list or recipe; any other file, version or platform; a macOS `codesign`/Gatekeeper check (needs a real Mac and is a separate step); any network use by the application itself.

## 8. Amendments after the first execution (Issue #19)

1. **Network.** The allowlist is now enforced for the GitHub CLI as well, by `tools/egress_guard.py` (deny by default, decided before any connection, incident on any refusal, exit 97). The default procedure is OFFLINE (no host). The one network step that remains is the trusted-root refresh (two TUF hosts, behind the guard). `tmaproduction.blob.core.windows.net` stays **not authorised**; an online check needs a **separate explicit authorisation** that names every host, and any unlisted host stops the run. The guard is not an operating-system firewall (a child that ignores proxy variables is not stopped): see [REQUEST-A-NETWORK-INCIDENT.md](REQUEST-A-NETWORK-INCIDENT.md) §5.
2. **Reporting.** Six separate facts: `artifact_crypto_verified`, `exact_bytes_verified`, `official_verifier_passed`, `network_policy_compliant`, `installation_authorized` (always false), `execution_authorized` (always false). A cryptographic PASS is never a network-compliance PASS, and the executed session is permanently recorded as `network_policy_compliant: false`.
3. **Offline replay.** `tools/extract_sigstore_bundles.py` replaces the ad-hoc conversion: it copies each bundle byte for byte. The bundle files used in the first execution were produced by a re-serialisation (same content, different bytes); the byte-exact files give the identical `gh` output (closure replay record).
4. **GPG on Windows.** POSIX-spelled `GNUPGHOME` for Git-for-Windows' gpg; verify the `Home:` line; exit codes and status files are recorded; the failed `gpg-agent` start during public-key import is explained (observed vs inferred) and tolerated only when the key was demonstrably imported; invocation errors are told apart from cryptographic failures, and unexpected failures stay fail-closed.
