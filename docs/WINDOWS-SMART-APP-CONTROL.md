# Windows Smart App Control (SAC) build diagnostics — Phase 1A closure

**Date:** 2026-10-17 · **Machine:** developer Windows 11 x64 · **Scope:** diagnosis only. No security setting, exclusion, policy or registry value was changed, and no signing credential was created.

## Findings

| Item | Result |
| --- | --- |
| Symptom | `cargo` / `tauri build` (release profile) fails with `os error 4551` ("Uygulama Denetimi ilkesi bu dosyayı engelledi"). |
| Blocking component | Smart App Control in **enforcement** mode (`VerifiedAndReputablePolicyState = 1`). CodeIntegrity policy ID `{0283ac0f-fff1-49ae-ada1-8a933130cad6}` is the SAC policy. |
| Relevant events | `Microsoft-Windows-CodeIntegrity/Operational`: **3077** (block, "violated code integrity policy") paired with **3033** ("did not meet the Enterprise signing level requirements"). 3089 carries correlated signature info. |
| Blocked executable (sanitized) | `<repo>\src-tauri\target\release\build\icu_properties_data-<hash>\build-script-build.exe`, 136 192 bytes, SHA-256 `4FA3D36DC8F6119AEE2AB3C1F49EF5D57A1DEA0757987875EA37BC4D71BD1F9A` (local build output; not expected to be bit-identical on other machines). The loading process was `cargo.exe` from the user's rustup toolchain. |
| Build stage | Running the **build script** of dependency `icu_properties_data 2.3.0` (transitive, via Tauri's URL stack) during `cargo build --release`. This happens before the application itself is compiled. |
| Authenticode | `NotSigned` (as is every locally compiled Rust build script, including those in the debug profile that run fine). |
| Legitimate Rust artifact? | Consistent with yes: it sits at the standard cargo path `target/<profile>/build/<crate>-<hash>/build-script-build.exe`, was produced by the local `rustc` from a crates.io crate pinned (with checksum) in `src-tauri/Cargo.lock`, and cargo itself launched it. This is a path/provenance judgement, not a binary reverse-engineering result. |
| Reproduces? | **Yes**, deterministically for that file: three separate release attempts (including one after the first diagnosis) failed identically with the same file. |
| Not only release | The same policy also blocked freshly built proc-macro DLLs `target\debug\deps\time_macros-<hash>.dll` (15 × 3077/3033 pairs). That produced `can't find crate for time_macros` in `cargo check`/`clippy`. An earlier note called this a "stale cache"; **that was wrong** — it was SAC. After `cargo clean -p time -p time-macros` the rebuilt DLL was allowed. SAC verdicts on unsigned local artifacts are therefore **intermittent per file**, which matches reputation-based enforcement. |

## Evidence classification

| Evidence | Status |
| --- | --- |
| Windows **debug** build, unit tests, headless `--self-check`, GUI runtime checks | **Verified working** (unsigned debug binary was allowed to run) |
| Windows **release** build and run, local | **Unverified** — blocked by SAC (OS policy), not by a code defect |
| Windows **release** build | **Verified in CI** on `windows-latest` (separate evidence; CI runners do not enforce SAC) |
| macOS ARM64 release build + headless self-check | Verified in CI only; no macOS GUI test |

## Options that were deliberately NOT taken

Turning SAC off (irreversible without an OS reset), adding exclusions, changing CI/registry policy, or purchasing/creating a code-signing certificate. Any of these would need the user's explicit decision. A signed release pipeline is a later release gate (Phase 0 G5).

## Practical guidance

* Develop and test locally with the **debug** profile; rely on CI for release builds.
* If a debug build is blocked, re-running after `cargo clean -p <crate>` sometimes yields an allowed artifact. This is a workaround observation, not a guarantee.
