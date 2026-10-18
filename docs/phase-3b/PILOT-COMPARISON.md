# First-pilot candidate comparison (research only)

Status: **assessment, not permission.** No binary was downloaded, extracted, run or probed; the application contains no recipe, digest or reference for any of these tools. Everything below was read from public GitHub metadata through the read-only REST API on **2026-10-18** (research by the developer's assistant, not application behaviour); it can change at any time, and every item must be re-read when a pilot is actually prepared.

## 1. Sources (primary)

| Tool | Release page | API endpoints read |
| --- | --- | --- |
| jq 1.8.2 | https://github.com/jqlang/jq/releases/tag/jq-1.8.2 | `GET /repos/jqlang/jq/releases/tags/jq-1.8.2`, `GET /repos/jqlang/jq`, `GET /repos/jqlang/jq/git/ref/tags/jq-1.8.2`, `GET /repos/jqlang/jq/commits/34f7186b86743a083a589741b6cea95293524108`, `GET /repos/jqlang/jq/attestations/sha256:<asset digest>` (metadata of the Sigstore bundle only) |
| ripgrep 15.2.0 | https://github.com/BurntSushi/ripgrep/releases/tag/15.2.0 | `GET /repos/BurntSushi/ripgrep/releases/tags/15.2.0`, `GET /repos/BurntSushi/ripgrep`, `GET /repos/BurntSushi/ripgrep/git/ref/tags/15.2.0` + the annotated-tag object, `GET /repos/BurntSushi/ripgrep/attestations/sha256:<asset digest>` (404) |
| Gitleaks 8.30.1 | https://github.com/gitleaks/gitleaks/releases/tag/v8.30.1 | as recorded in `../GITLEAKS-PILOT-ASSESSMENT.md` |

## 2. Matrix

| Criterion | **jq 1.8.2** | **ripgrep 15.2.0** | **Gitleaks 8.30.1** |
| --- | --- | --- | --- |
| Published | 2026-06-20 (patch release, security fixes) | 2026-07-15 | 2026-03-21 |
| Licence (API) | `NOASSERTION` — jq is MIT-licensed with additional bundled notices; **needs a manual licence review** | `Unlicense` (dual MIT/Unlicense upstream) | MIT |
| Artifact form | **raw single executables** (`jq-windows-amd64.exe` 1 035 264 B, `jq-macos-arm64` 841 504 B) — **no archive** | ZIP (`…x86_64-pc-windows-msvc.zip`) and TAR.GZ (`…aarch64-apple-darwin.tar.gz`) | ZIP (`…_windows_x64.zip`, 8 438 883 B) and TAR.GZ (`…_darwin_arm64.tar.gz`) |
| Exact-file digest published for the installed file | **yes** — each asset *is* the installed file; GitHub lists `sha256` per asset, and the attestation subjects name the same digests | **no** — only archive digests; the extracted `rg(.exe)` digest is not published | **no** — only archive digests |
| Digests (assets) | win `a6fc67fe…f0c627`, mac `2d75340b…eca07e` | win zip `71b2fef8…9aafc5`, mac tar.gz `3750b2e9…973e4` (+ same-release `.sha256` sidecars) | win zip `d29144de…afc4e`, mac tar.gz `b40ab0ae…eb6a5` (+ `checksums.txt`) |
| **Independent provenance** | **GitHub artifact attestations present** (Sigstore bundle v0.3, DSSE in-toto, predicate SLSA provenance v1; subjects include both targets with the digests above; workflow `jqlang/jq/.github/workflows/ci.yml@refs/tags/jq-1.8.2`, source commit `34f7186b…`, Fulcio certificate from `token.actions.githubusercontent.com`, Rekor log entry present) | **none** (attestation lookup 404; no signature assets). The tag `15.2.0` is an **annotated, verified-signed** tag (a maintainer's key) | **none** (lightweight tag, unsigned commit, no signature assets, attestation lookup 404) |
| Source pin quality | tag `jq-1.8.2` is lightweight, but the **commit signature is valid**; the attestation binds the artifact to that commit and workflow | signed annotated tag → commit `e89fff89…` | lightweight tag → commit `83d9cd68…` unsigned |
| Runtime needs | none (static native binary); no daemon, no network, no telemetry; installing does not require running it | same | same, but **prints the secrets it finds** (output hygiene problem if ever run) |
| Needs extraction pipeline | **no** | yes (ZIP **and** TAR.GZ) | yes (ZIP **and** TAR.GZ) |
| Catalogued in `data/catalog-seed.json` | **no** (a reviewed catalog entry would have to be added) | no | yes (unreviewed) |
| Repository state | active (pushed 2026-10-16), `SECURITY.md` present | active (pushed 2026-08-04), no `SECURITY.md` | active; new features frozen per catalog |
| Open unknowns (cannot be answered without downloading) | Authenticode status of the Windows exe; whether the macOS arm64 binary has at least an ad-hoc signature; Gatekeeper/quarantine behaviour; Smart App Control reaction; the complete licence text | all of the left column, plus the extracted-file digest | all of the left column, plus the extracted-file digest |
| What it exercises of the manager | download → verify → stage → place → pointer switch → rollback → uninstall for **one file** | additionally needs the whole extraction pipeline in the very first pilot | same as ripgrep, with the weakest provenance |

## 3. Selection

* **Recommended pilot: jq 1.8.2** (`windows-x64`, `macos-arm64`, user scope, `local-cli` surface). It is the only candidate that offers *both* an exact-file artifact (so no extraction code is needed in the first pilot) *and* publisher-side build provenance that covers exactly the bytes to be installed. The pilot is a **plumbing pilot**: it validates the manager's download-verify-place-rollback path with the smallest possible blast radius; it is not chosen because jq matters to AI agent toolchains. If the Red Team prefers a catalogued tool, Gitleaks becomes eligible only after the missing evidence below exists for it.
* **Fallback (only after jq is proven or ruled out): ripgrep 15.2.0.** Its provenance is weaker (signed tag, same-origin sidecars, no attestation), and it needs the extraction pipeline specified in `ARCHIVE-EXTRACTION-SPEC.md`.
* **Not recommended: Gitleaks 8.30.1.** Weakest provenance, archive-only artifacts, prints secrets.

### Explicit NO-GO for any real installation in Phase 3B (and until all of this exists)

1. Written Red Team approval of the provenance model (`PROVENANCE-DECISION.md`) and of the pilot choice.
2. A **reviewer-produced** reference entry: the exact-file digests of the two jq files verified with the official attestation verifier against `--repo jqlang/jq` and the expected signer workflow, on a throw-away environment, with a **separate explicit authorization to download** those two files for verification. (Phase 3B did not do this.)
3. A manual **licence review** of the jq release (API says `NOASSERTION`).
4. A **reviewed recipe** for exactly this version and target, with a real clean-machine staging check by someone other than the author.
5. A real Apple Silicon Mac check (ad-hoc signature, quarantine, Gatekeeper) and a real Windows check (Authenticode, Smart App Control reaction), recorded as such.
6. The Phase 3C isolated production adapter, merged and exact-SHA CI green, with its filesystem acceptance tests (`FILESYSTEM-OWNERSHIP.md` §9) passing on real Windows and macOS.
7. A separate user authorization for the first real action, and the real approval flow of `APPROVAL-AND-AUDIT.md`.

## 4. Minimum evidence summary per candidate

| | Have | Missing for a real pilot |
| --- | --- | --- |
| jq | attestations, exact-file artifacts, signed source commit | reviewer verification of the two files; licence review; real-OS checks; recipe; adapter |
| ripgrep | signed annotated tag, sidecar digests | everything above **plus** an authenticated extracted-file digest and the extraction pipeline |
| Gitleaks | licence (MIT) | provenance, extracted-file digest, extraction pipeline, output hygiene, everything else |
