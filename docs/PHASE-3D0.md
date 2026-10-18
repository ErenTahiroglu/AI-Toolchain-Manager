# Phase 3D-0 — First real pilot readiness gate (jq 1.8.2)

Baseline `a695dbe743ca6f15aaa69fe47325490e9583b2cf` (accepted Phase 3C). This phase is an **evidence and readiness gate, not an installation**. Only permission 1 of §7 (reading public documentation and metadata) was exercised. **No jq file or any other third-party artifact was downloaded, extracted or executed; nothing was installed; no key, approval, button, package-manager call, PATH change, agent-configuration edit, network call by the application, backup deletion or security-setting change exists or happened.** The production recipe list and the authenticated-reference set are empty; `Adapter::production` still refuses every operation; `identity_verified` is unchanged.

| Document | Content |
| --- | --- |
| [phase-3d0/JQ-PROVENANCE.md](phase-3d0/JQ-PROVENANCE.md) | what was verified about jq 1.8.2 (identity, attestation, GPG channel, licences, SAC/Gatekeeper), what is only declared, what is unknown |
| [phase-3d0/evidence/README.md](phase-3d0/evidence/README.md) | the stored public metadata and the offline verifier reports |
| [phase-3d0/ISSUE-14-RESIDUAL-RISKS.md](phase-3d0/ISSUE-14-RESIDUAL-RISKS.md) | the Windows replacement window and the macOS unlink window: model, blast radius, options, decision |
| [phase-3d0/APPROVAL-READINESS.md](phase-3d0/APPROVAL-READINESS.md) | the minimum real approval for one install; Windows Hello vs macOS Secure Enclave; what is denied |
| [phase-3d0/DEVICE-TEST-PLAN.md](phase-3d0/DEVICE-TEST-PLAN.md) | exact procedures for the dev PC, a second Windows PC and a real Apple Silicon Mac, with evidence classes |

## 1. What changed in the code (narrow, justified)

* `tools/verify_sigstore_bundle.py` (+ stored public evidence and `tests/test_phase3d0_contract.py`): an **offline** verifier of a Sigstore/GitHub attestation bundle (28 checks) with negative tests; no network, no file writes, nothing executed. It turns "the release has an attestation" into "this statement is authentically signed by this workflow identity for this exact digest". Reviewer tooling, not application code. `requirements-test.txt` gains `cryptography` for it (test dependency).
* `managed_fs_probe.rs` + `fsAcceptanceProbe` in `--self-check`: the **real-device acceptance probe** (artificial files in a new folder below the isolated data directory, removed afterwards, result codes only). It is what a second PC or a real Mac runs; it also runs in CI on the release binaries.
* `Dir::probe_atomic_rename` and its use in `ManagedRoot::establish/open`: a volume that cannot do the atomic no-clobber move or the known-file replacement (for example a macOS volume without `RENAME_EXCL`/`RENAME_SWAP`) is **refused** (`Unsupported`), with nothing left behind — "preserve unsupported status".
* `Dir::filesystem_name` (volume type recorded by the probe), a CI artifact `device-test-unsigned-<platform>-<sha>` (3 days) for device runs.
* No migration, no new production capability, no change to `Adapter::production`, recipe or reference lists.

## 2. Findings in one place

* **jq provenance (V = verified offline here):** official repository identity, release, tag→commit, artifact names and digests agree across three publisher listings; the attestation bundle is authentic under the Sigstore trusted root and binds *exactly* `jq-windows-amd64.exe`/`jq-macos-arm64` to repository `jqlang/jq`, workflow `ci.yml`, ref `refs/tags/jq-1.8.2`, commit `34f7186b…`; a second channel (publisher GPG signatures, key fingerprint `9307 9A95 11EF C0B7 D6CD BFB5 B0DA 60FB 454B AF18`) exists but is same-origin and was created on 2025-07-31 with all older releases re-signed the same day. **No digest was entered into any reference list.**
* **Not established (needs the bytes or a device):** that the files on the CDN match the attested digests; GPG signature validity; Authenticode/codesign state; SAC and Gatekeeper behaviour; a complete third-party notice list (Oniguruma and the MinGW runtime are not in jq's `COPYING`).
* **Risk context:** 27 published advisories, 20 of them fixed in 1.8.2 and none listed against 1.8.2; releases are mutable; the workflow uses tag-pinned actions and a rolling MSYS2 toolchain; the binaries are not code-signed or notarised.
* **Issue #14:** both windows are bounded to manager-internal names inside the owner-only root and cannot destroy user data; accept-and-disclose is proposed; real-device probe evidence is still missing (second PC, real Mac).
* **Approval:** a real approval is realistic on Windows (Hello key) first; macOS is denied until a signed build can use the Secure Enclave.

## 3. jq remains the right pilot — with conditions

Still the best candidate: a single raw executable per platform (no extraction), attested exact-file digests, patched current release, tiny blast radius if only *installed*. Weak points that the rest of the process must carry: unsigned binaries (SAC may refuse to run it; Gatekeeper/ad-hoc-signature unknown), high advisory rate (→ **install without execute**), incomplete notices, no revocation. A **Windows-first** pilot is realistic; a macOS pilot additionally needs a signed application (approval key) and a real Mac run.

## 4. Smart App Control (developer-reported history preserved; fresh read-only capture added)

* The diagnosis of Phase 3C stays recorded as **developer-reported historical evidence**: the first CodeIntegrity excerpt the user supplied concerned a browser extension DLL and a security product, **not** the Cargo-launched test executable, and the original events have since rolled out of the 1 MiB circular log.
* **New, captured while available (read-only):** when the first test run of this phase was blocked (`os error 4551`, "blocked by the Application Control policy"), the log held — Windows event **3077** (record 16805679, 2026-10-18 23:53:20.518) and **3033** (record 16805677, 23:53:20.426): *process* `…\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe` *attempted to load* `<repo>\src-tauri\target\debug\deps\ai_toolchain_manager_lib-<hash>.exe` *that did not meet the Enterprise signing level requirements*, policy id `{0283ac0f-fff1-49ae-ada1-8a933130cad6}` (the Smart App Control policy). The denied image was written at 23:53:17, three seconds earlier, and is `NotSigned`. That is independent confirmation for this occurrence that `cargo.exe` is the launcher and the **freshly built unsigned test executable** is the blocked image; it does not make any claim about the earlier event excerpt.
* Effect on testing: that one `cargo test` invocation was blocked; after the test binary was rebuilt (a source file was touched and 20 s waited) the same tests ran and passed (37/37 in the affected module). No test was skipped because of SAC. No policy, exclusion, rename or download was used.

## 5. Tests completed and not run

See the delivery report for counts. **Not run:** anything on a second Windows computer or a real Mac (no device); a release (non-debug) local build (SAC; CI covers it); GUI checks on macOS; byte-level checks of any jq file (forbidden in this phase).

## 6. Real-device status

Real-device probe results exist only for the existing development PC; the CI runners (Windows NTFS, macOS APFS) also produced clean probe results for the release binaries (DEVICE-TEST-PLAN §6) — runner evidence, not clean-machine or Mac-desktop evidence. The probe code, its CI run on both runners and the procedure exist; the second PC and the Mac runs are the open items of Issue #14.

## 7. Pilot decision matrix — five separate permissions

| # | Permission | State | Needs |
| --- | --- | --- | --- |
| 1 | Reading public documentation and metadata | **Authorised and used in this phase** | — |
| 2 | Downloading exact selected third-party artifacts | **NOT authorised** | the user's explicit, specific approval (request in §8) |
| 3 | Verifying downloaded bytes and attestations **without execution** | **NOT authorised** (needs 2) | same approval; tooling ready (`tools/verify_sigstore_bundle.py --artifact`, `gh attestation verify`, `gpg --verify` with an isolated keyring) |
| 4 | Installing verified bytes into a manager-owned root | **NOT authorised, NO-GO** | everything in §9 |
| 5 | Executing the installed tool | **NOT authorised, NO-GO** | its own later request, after 4 and a separate security review |

## 8. The request for a download-and-verification-only next step (corrected in the Phase 3D-0 closure; still NOT granted)

The first wording of this section ("GitHub CLI over HTTPS to `github.com` only", signatures from `master`) was wrong (Issue #17). It is **superseded** by [phase-3d0/REQUEST-A.md](phase-3d0/REQUEST-A.md) (exact commands) and its Turkish counterpart [phase-3d0/REQUEST-A.tr.md](phase-3d0/REQUEST-A.tr.md): both artifacts pinned by name, size and SHA-256 (`jq-windows-amd64.exe`, 1 035 264 bytes, `a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627`; `jq-macos-arm64`, 841 504 bytes, `2d75340ba57a4b4b4c8708a21c2dc8e958a48aaa8bba13b27f77f6e4c0eca07e`); signatures and key pinned to the immutable commit `f0dabad36a5fdb37393829e6bd4c465d70c40a3a` with the exact fingerprint `93079A9511EFC0B7D6CDBFB5B0DA60FB454BAF18`; an explicit host list (`github.com`, the two GitHub asset-storage hosts, `raw.githubusercontent.com`, `api.github.com`, and the two TUF hosts embedded in the GitHub CLI) with fail-closed redirects and no credentials; the maintained GitHub CLI verifier as authority and the custom verifier as supporting evidence only; the whole session folder preserved for the Red Team. See [PHASE-3D0-CLOSURE.md](PHASE-3D0-CLOSURE.md).

## 9. GO / NO-GO

* **Download-and-verification-only next step (permissions 2–3): GO**, conditional only on the user's explicit approval of Request A. Nothing blocks it technically; the verifier, the identity policy and the isolated procedure exist and are tested; the evidence it would add (byte-level digests, GPG validity, Authenticode status) cannot be obtained any other way.
* **Actual installation (permission 4): NO-GO.** Open gates: (a) byte-level verification of both files (Request A) and its Red Team acceptance; (b) a reviewed reference entry with two digests and a reviewed recipe; (c) the notice/licence decision (§ JQ-PROVENANCE 5); (d) real-device probe results on a second Windows PC and a real Mac, and the Red Team decision on the two disclosed filesystem windows (Issue #14); (e) a real approval path: key store, presence, durable ledger and audit (APPROVAL-READINESS) — none implemented; (f) for macOS, a signed application and a real Mac check of the binary's signature/Gatekeeper behaviour; (g) a separate explicit user authorisation for the first real action; (h) the adapter has so far only been exercised in test mode with synthetic artifacts.
