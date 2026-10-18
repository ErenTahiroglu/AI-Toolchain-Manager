# Trusted artifact provenance — comparison and decision

Status: **research + design proposal. Nothing was downloaded, extracted or run; the application ships no reference digest and the authenticated-reference list is still empty.** Facts about releases were read through public GitHub metadata on 2026-10-18 (see `PILOT-COMPARISON.md` for sources). `identity_verified` keeps its Phase 2C meaning: it is set only for an exact-file SHA-256 that equals an entry of the compiled-in authenticated reference set. Nothing in this document weakens it, and a GitHub-side checksum, a sidecar `.sha256` or a `checksums.txt` published next to the file **does not authenticate the publisher** — it only detects corruption in transit.

## 1. The question

What must be true before the manager may treat bytes it fetched as "the file the publisher built from the reviewed source"? Three separate claims are easy to confuse:

| Claim | Meaning |
| --- | --- |
| **Integrity** | the bytes are identical to some known file |
| **Origin / provenance** | a specific publisher / build pipeline produced that file from a specific source revision |
| **Benignity** | the file does what it should and nothing else |

A digest gives integrity. Only a signature or attestation checked against an independently pinned trust root gives origin. **No mechanism below gives benignity**; that remains a human review plus a bounded, disclosed residual risk.

## 2. Approaches compared

| # | Approach | Authenticates | Does **not** authenticate | Trust root / pinning | Rotation & revocation | Offline | Fit for Win x64 + macOS arm64 | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | **Publisher-signed artifact** (minisign/GPG/cosign key held by the maintainers) | origin, if the public key was obtained independently | the key's holder being honest; a compromised key | public key pinned in a reviewed reference | maintainers rotate; the manager only learns via app update | yes | yes | key distribution is manual and per project |
| 2 | **Signed manifest / checksum file** (signature over `checksums.txt`) | origin of the list of digests | the same as 1 | same | same | yes | yes | needs a signature format per project |
| 3 | **GitHub artifact attestation / Sigstore** (SLSA provenance, DSSE, Fulcio cert, Rekor log) | that *a specific workflow in a specific repository at a specific commit built these exact bytes* (digest is the in-toto subject) | that the workflow or source is benign; compromise of the repository or its CI; GitHub's OIDC issuer is trusted | Sigstore TUF root + the expected repo/workflow identity | short-lived certs (minutes); revocation = Rekor/transparency + yanking | yes **if** the bundle is stored and the trust root is bundled | yes (file-based, no OS dependence) | a verifier (`gh attestation verify` or `sigstore-rs`) and TUF root handling |
| 4 | **Authenticated release digest** (GitHub shows `sha256:` per asset) | that GitHub holds those bytes under that release | the publisher; a modified release is re-digested by GitHub | GitHub (TLS) | none | no | yes | zero, but weak |
| 5 | **Reproducible build** | that independent parties derive the same bytes from source | needs independent rebuilders; rarely available | multiple builders | n/a | partly | partly | very high |
| 6 | **Independently pinned exact-file digest** (reviewer verifies once, the app ships the digest in its signed release) | integrity against that reviewed file; origin only as strongly as the review that produced it | the reviewer's mistakes | the application release signature (the compiled-in authenticated reference set) | next app release | **yes, fully** | yes | one reviewed entry per tool/version/target |
| 7 | **OS-native signature verification** (Authenticode on Windows, Developer-ID/notarisation or ad-hoc signature on macOS) | that *some* code-signing identity signed the file, and the OS policy accepts it | that the identity is the intended publisher unless pinned; macOS ad-hoc signatures carry no identity | OS trust stores (+ pinning of the expected subject/Team ID) | CA revocation (online) | partly | **unknown**: many open-source CLI releases are unsigned on Windows and ad-hoc/unsigned on macOS | high; platform-specific; must not run the binary |
| 8 | **Bounded manual review of one pinned release** | whatever the reviewer checks (licence, notes, diff of source tag, attestation if any) | everything else | the Red Team and the application release | n/a | n/a | yes | human time |

## 3. Decision (design proposal for Red Team)

**Layer 6 as the trust anchor, 3 and 8 as the evidence that feeds it, 7 only where it already exists, 4 never as authentication.**

1. The only thing the application trusts is an entry of its **compiled-in authenticated reference set** (exact-file SHA-256, tool, version, platform, artifact kind, format). That set is part of the signed application release, so its trust root is the same one that already protects the application. It is empty today and stays empty until a Red-Team-approved entry exists.
2. An entry is **produced by a reviewer, offline, on a throw-away environment**, from:
   * the publisher's attestation, verified with the official verifier against an **explicit repository and workflow identity** (`--repo`, `--signer-workflow`), so the digest in the entry is the subject digest of an attestation by the intended pipeline;
   * the exact-file digest compared with what the release page lists (agreement, not authentication);
   * licence review, source-tag review, release notes, CVE history;
   * the reviewer and date recorded in the reference's provenance text.
3. The application never contacts a verifier or Sigstore at run time in this design. A later phase may add **in-application attestation verification** (bundle stored with the recipe, Sigstore trust root bundled and rotated through app updates) as defence in depth; it is not required for the first pilot and is not proposed for Phase 3C.
4. **Pinning and rotation:** a reference is exact (one file, one version). A new upstream version is a new entry, a new recipe revision and a new approval. Revocation = removing the entry in an application update (plus a "yanked" list the same release may carry); a device that never updates keeps trusting the old entry — disclosed.
5. **Downgrade / replay:** the reference names an exact version; the installer compares the exact version and digest, so serving an older (vulnerable) but validly attested file does not match. The update path requires the previous version in the approval (`fromVersion`).
6. **Scope:** an entry authenticates *one file for one platform target*. Never a family, never "latest".

### Essential for the first pilot (minimum)

* exact-file SHA-256 in the compiled-in reference set, reviewed with the attestation verified by the reviewer;
* digest check of the downloaded bytes against the reference before anything is written, and again on the staged file and on the extracted/placed file (already implemented in the synthetic engine);
* licence review recorded; a real clean-machine staging check by someone other than the author;
* residual-risk text shown to the user at approval time.

### Defence in depth (valuable, deferrable)

* in-application Sigstore bundle verification; certificate-transparency monitoring; a second independent reviewer; pinning the expected Authenticode subject / macOS Team ID where the vendor signs; periodic re-verification of installed bytes against the reference ("installed content drift"); vulnerability-advisory feed.

### Residual risks to disclose to the user (cannot be removed)

* upstream repository or CI compromise before the build — an attestation says *where* it was built, not that it is benign;
* reviewer error or a compromised reviewer machine;
* trust in GitHub's OIDC issuer and in Sigstore's public-good infrastructure for the reviewer's check;
* no revocation reaches a device until the application is updated;
* the manager cannot prove the installed binary is harmless, only that it is the reviewed one.

## 4. Why not the alternatives as the anchor

* **Publisher signature (1/2)**: not available for the leading candidates (no signature assets found); would need a key-distribution scheme per project.
* **GitHub release digest / sidecar (4)**: same origin as the file; changes whenever a maintainer or an attacker with release rights replaces the asset.
* **Reproducible builds (5)**: no candidate documents independent rebuilders.
* **OS-native verification (7)**: would require inspecting the downloaded file (allowed only in a later, authorized phase); several candidates may be unsigned; and a valid signature by an unknown party is still not "the publisher". It stays a possible additional check, never a replacement for the pinned digest.
* **Manual review alone (8)**: not repeatable, not machine-checkable on each user's computer; used as the *input* to option 6.

## 5. Per-platform feasibility notes (assumptions to confirm in Phase 3C)

* Verifying an attestation needs the artifact file; with `gh attestation verify <file> --repo … --signer-workflow …` (and `--bundle <file>` for offline verification) it can be done by a reviewer on a clean machine. Not done in Phase 3B.
* Whether the macOS arm64 candidate binary carries at least an ad-hoc code signature (needed to run on Apple Silicon) and how Gatekeeper treats a file downloaded by the manager (quarantine attribute) is **unknown** and must be checked on a real Mac in Phase 3D preparation.
* Whether the Windows candidate is Authenticode-signed is **unknown**; Smart App Control may block unsigned executables; the manager must never alter SAC or Defender and must report such blocks honestly.

## 6. Consequences for `identity_verified`

Unchanged. A `ReferenceDigest` from the compiled-in set is still the only way to obtain it; this document adds a *procedure* for creating such an entry, not a new way to obtain the flag. Plans for real recipes keep `source_origin_unverified` until a later phase defines how attestation evidence is represented in a recipe; Phase 3C must add it as separate, typed evidence (never by reusing the digest field).
