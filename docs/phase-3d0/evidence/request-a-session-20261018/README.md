# Request A execution — sanitized record (session `jq-review-20261018T2240Z`)

Starting repository state: `742495af2af2649cde49917f2ca1b9d7808a19de` (clean tree; branches `main` = `68eda3f0…`, `development`). User authorisation: permissions 1–3 only (read, download the two named files, verify bytes and provenance). **Nothing was installed, executed (not even `--version`), added to PATH, copied into a managed root, or entered into any reference list or recipe.** The binaries and the scratch session folder stay outside Git and are kept for Red Team review; this folder holds only sanitized text records (local paths replaced by `<scratch>`, signed URL queries redacted). `session-sha256.txt` lists every file of the kept session (SHA-256, size, relative path), including the two binaries.

## Outcome

| Step | Result |
| --- | --- |
| Pinned text from commit `f0dabad3…` (2 signatures, checksum list, public key) | all four fetched from `raw.githubusercontent.com` directly (no redirect); size and SHA-256 equal the pins |
| `jq-windows-amd64.exe` | 1 035 264 bytes, SHA-256 `a6fc67fe…c627` = pin = publisher checksum list |
| `jq-macos-arm64` | 841 504 bytes, SHA-256 `2d75340b…a07e` = pin = publisher checksum list |
| Download hops (both) | `github.com` (302) → `release-assets.githubusercontent.com` (200): inside the approved list |
| Fresh Sigstore trusted root (`gh attestation trusted-root`) | obtained; byte-identical to the stored snapshot (SHA-256 `65ca537f…2b9c`) — recorded, but the stored file was still not used as the anchor |
| `gh attestation verify`, strict policy, online | both files exit 0; Windows 2 attestations, macOS 1 |
| `gh attestation verify`, offline replay (bundle + fresh root) | both files exit 0; JSON byte-identical to the online results; the replay made **no network connection at all** (observed with the logging proxy) |
| Supporting custom verifier (29 checks incl. downloaded bytes) | both pass |
| GPG, isolated keyring, no key server | one primary key `93079A95…AF18` in the keyring; each file: `GOODSIG` + one `VALIDSIG` by exactly that fingerprint (signatures made 2026-06-20); `TRUST_UNDEFINED` is expected (no web of trust) |
| Static facts | Windows: PE32+ x64 console executable, **no embedded certificate table, Authenticode `NotSigned`**. macOS: thin Mach-O arm64 `MH_EXECUTE`, `LC_CODE_SIGNATURE` present = ad-hoc + linker-signed CodeDirectory (identifier `jq`), **no CMS slot → no Developer ID signature, not notarized**; `codesign`/Gatekeeper assessment still needs a real Mac |
| `tools/review_jq_verification.py` over the session | **PASS — 151 checks, 0 failed** (official and supporting verifier agree for both files) |

Provenance (Sigstore, official verifier), exact-file digest match, and local code-signature facts are three separate findings; none implies the others, and none creates `identity_verified`.

## Deviations and findings the Red Team must see

1. **Unlisted host contacted by the GitHub CLI.** The online `gh attestation verify` fetches the attestation bundle from the signed storage URL that the attestations API returns, on **`tmaproduction.blob.core.windows.net`** (GitHub's attestation storage on Azure). That host is **not** in the approved list of REQUEST-A.md §2, I did not know this in advance, and the first (unproxied) online run therefore contacted it. It was found afterwards by re-running the same command through a local deny-by-default logging proxy (`proxy-observation-hosts.log`): the proxy tunnelled only `tuf-repo-cdn.sigstore.dev`, `tuf-repo.github.com`, `api.github.com` (now **observed**, not merely expected) and **refused** the Azure host, whereupon the CLI failed (`proxy-observation-jq-windows-amd64.exe.online.err`, signed query redacted). The data fetched was a public attestation bundle over HTTPS with a read-only expiring URL; no credential of this project was sent (the CLI's stored login is used only against `api.github.com`). The host is **not added** to the policy by this session: whether the online check may keep using it is the user's decision. The offline replay does not need it.
2. **Procedure gap, offline replay.** `gh --bundle` reads one Sigstore bundle per line, not the stored GitHub API response wrapper; the first offline attempt failed (`jq-*.gh-offline.attempt1-wrapper-format.err`: “unknown field attestations”). The `bundle` objects were extracted locally from the stored evidence into `bundles/<file>.jsonl` (no network, contents unchanged) and the replay then passed. The REQUEST-A.md procedure should say so.
3. **Procedure gap, GPG on Windows.** Git's GnuPG treats a Windows-style `GNUPGHOME` as a relative path (first attempt `NO_PUBKEY`, kept as `attempt1-winpath-*` in the session). The POSIX spelling of the same folder worked. `gpg` also tried to start a local `gpg-agent` during the import (exit 2, irrelevant for public-key import; the key was imported). No key server or network was used.
4. Not done (by design): macOS `codesign`/Gatekeeper, any execution, any real-device filesystem evidence (Issue #14).

## Files

`review.json` (151 checks), `fetch.log` (download hops), `keyring.colons`, `gpg-import.log`, `*.gpg.status`, `*.custom.json`, `*.gh.json` (online result; the offline result is byte-identical, see `session-sha256.txt`), `*.gh*.exit`, `static-header-facts.json`, `macho-codesign-structure.json`, `authenticode-windows.json`, `proxy-observation-*`, `session-sha256.txt`.
