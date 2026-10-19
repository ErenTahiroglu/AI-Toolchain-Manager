# Phase 3D-2 — Product MVP reset: the first useful local installation experience

Baseline `b90b06cc47be633219521361ff91d3d8a6697b83`. **No jq file was downloaded, copied, opened or executed. No Windows Hello key was created, used or deleted. No production installation exists: the release supply is empty, `Adapter::production` is closed, the production recipe list and authenticated-reference set are empty.** `main` is untouched.

Product direction (Issue #21): a normal user-scoped installation needs a visible in-app confirmation, not a hardware credential. Windows Hello / TPM work stays as optional, inactive research.

| Topic | Result | Document |
| --- | --- | --- |
| Workflow | Catalog → preview → confirm → result → installed-tool details, with eight distinct statuses, built into the real catalog page and driven by six typed Rust commands | [MVP-CONSENT-MODEL](phase-3d2/MVP-CONSENT-MODEL.md), `src-tauri/src/install_flow.rs`, `src/components/InstallPanel.tsx` |
| Consent | short-lived (120 s), plan-bound, single-use, checked in Rust; **explicitly not authentication** | [MVP-CONSENT-MODEL](phase-3d2/MVP-CONSENT-MODEL.md), `consent.rs` |
| Supply | release build: empty; synthetic tools only in tests and in a build made with `--features synthetic-pilot` | same |
| jq transition | five exact blockers, replayed on copies in tests; a fifth (`source_origin_unverified`) was found | [JQ-PILOT-TRANSITION](phase-3d2/JQ-PILOT-TRANSITION.md) |
| Notices | application-presented, digest-bound notice manifest; no sidecar; complete conservative package; Q1–Q3/C1–C5 still open | [NOTICES-DECISION](phase-3d2/NOTICES-DECISION.md), [notices/](phase-3d2/notices/) |
| Issue #14 | stays open; limited-pilot assessment, local NTFS evidence, residual risks | [ISSUE-14-LIMITED-PILOT-ASSESSMENT](phase-3d2/ISSUE-14-LIMITED-PILOT-ASSESSMENT.md) |
| Old guards | all preserved; new boundary guards added | [GUARD-TEST-CHANGES](phase-3d2/GUARD-TEST-CHANGES.md) |
| Real-window evidence | 60 checks on the real application window, two phases (including a hard kill and restart) | [GUI-EVIDENCE](phase-3d2/GUI-EVIDENCE.md) |

## Limitations stated up front

Synthetic bytes only; a real jq, a real `.exe`, the retained Request A file, a download, an execution, a second PC, a real Mac and Windows Hello remain untouched and untested; the macOS runner is not a Mac desktop; the in-app confirmation does not defend against a program of the same Windows account; a crash in the create-to-identity window leaves an ambiguous orphan that needs a person (no UI action resolves it yet); Smart App Control blocked fresh unsigned test binaries several times and was handled by rebuilding, never by a setting.
