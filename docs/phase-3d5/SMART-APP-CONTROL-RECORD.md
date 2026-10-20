# Smart App Control during Phase 3D-5 (record only)

**Rule of this phase.** No Smart App Control or Defender setting is changed. Test executables are not rebuilt in order to get past a block. A blocked executable is recorded here (sanitized) and its run is classified **BLOCKED / NOT RUN**, never counted as a pass; an identical binary is re-run at most once. CI is separate evidence. jq (a real executable) was never provided, opened or executed, so its treatment by Smart App Control is **not tested and not claimed**.

| When (UTC, 2026-10-20/21) | What was blocked (sanitized) | Event | Classification |
| --- | --- | --- | --- |
| 22:05 | the library **test executable** of the **default** configuration, SHA-256 `dab2d369ee0887b012c1c8e6312564bea3cf8be7244228fa7d1060d524684661` (`target\debug\deps\ai_toolchain_manager_lib-feb5a3fd10e7d745.exe`), loaded by `cargo.exe` | `Microsoft-Windows-CodeIntegrity/Operational` **3077**, Smart App Control policy `{0283ac0f-fff1-49ae-ada1-8a933130cad6}`; cargo `os error 4551` — blocked twice (the identical binary was re-run once) | **BLOCKED / NOT RUN** for that exact binary; not rebuilt to evade |
| shortly after | the library test executable of the **`jq-pilot`** configuration, SHA-256 `74adedf2d469c0ca4b3dba2fef9af2138a04699e534abc9eb885eeed6a5c2529` (`…lib-395f48013d3c74f3.exe`) — blocked twice | same event / `os error 4551` | **BLOCKED / NOT RUN** for that exact binary |

| 22:32 | the **default debug application** (`ai-toolchain-manager.exe`, SHA-256 `6f7a8d97c300f80cd2040e0977da0ac4969d503db8ec96fc5775ba093ce98972`) on its **first launch** for the headless `--self-check` (the Bash tool reported "Permission denied") | CodeIntegrity **3077** at 22:32:45 | blocked once; the **identical** binary, launched again once from PowerShell without rebuilding, ran (exit 0) — that second run is the self-check evidence; the first launch is not counted |

Binaries produced by later, substantive source changes were built as part of the normal work and ran; the configurations' final results are in [PHASE-3D5](../PHASE-3D5.md) and the report, and are counted only from runs that actually executed.
