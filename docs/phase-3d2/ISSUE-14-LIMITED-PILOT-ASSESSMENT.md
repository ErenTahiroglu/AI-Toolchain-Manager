# Issue #14: a limited, explicitly authorised pilot on the existing Windows machine versus a general release (Phase 3D-2)

**Issue #14 stays open as the production and distribution gate.** This note evaluates only one narrower question: *what are the specific risks of ONE explicitly authorised, installation-only pilot on the existing Windows development PC, and can a second physical Windows PC be replaced for that pilot by a narrower, independently justifiable alternative?* It proposes; it decides nothing and grants nothing. The Phase 3D-1 gate ([H1–H8](../phase-3d1/ISSUE-14-WINDOWS-PILOT-HARDWARE-GATE.md)) is **not weakened for anything else**.

## 1. Evidence that exists for this machine

| Topic | Evidence | Class |
| --- | --- | --- |
| **Local NTFS acceptance** | `fsAcceptanceProbe` in `--self-check` of the Phase 3D-2 build on the development PC (isolated data directory): filesystem `ntfs`, **13 pass, 1 not exercised** (creating a link needs a privilege that is not available — covered instead by the junction test `a_link_in_place_of_a_managed_folder_is_never_followed`), **3 observed** (a held folder cannot be removed by another process: *prevented by the OS*; an open file cannot be removed: *locked by the holder*; folder flush: *flushed*), 0 failed. The same probe passed 14 on the GitHub Windows runner. | real device = the machine the code is written on; **not independent** |
| **Managed-root ownership** | The root is derived only from the application's own resolved data directory (no command, IPC argument, recipe or plan field names a root; source-scan tests). It is created owner-only and read back, its identity (volume + object id) is recorded in the manager's database *before* it is marked active, and it is re-opened only if identity, owner-only permissions and the exact fixed layout match. | tests + probe |
| **Workflow on the real window** | `tools/gui_install_flow.ps1` drove the real application (synthetic pilot build, isolated data directory) through preview, forged IPC, cancel, install, replay, details, removal with user data, reinstall, tamper detection and a hard kill + restart: see [GUI-EVIDENCE](GUI-EVIDENCE.md). | real device, synthetic bytes |
| **Fail-closed recovery** | A crash injected at 13 points of an installation is recovered after a "restart" (new service, no memory of the old consent): rolled back or forward, or — in the two irreducible create-to-identity windows — **kept and reported as needing attention, never deleted on a guess**. Failures at five points roll back with a `restored` report and nothing but the permanent layout left. A held-open file (Windows) stops a removal and everything is put back. Tampered, extra, forged-pointer and deleted installed files are reported as drift and never repaired. | tests (`install_flow_tests.rs`, `adapter_tests.rs`) |

## 2. Residual risks specific to a single pilot on this machine

| Risk | Statement |
| --- | --- |
| **Windows pointer-replacement race** | Replacing the one transaction-owned *pointer* file checks the destination's identity and link count immediately before the replacing rename, but **a window remains** (a destination cannot be pinned against deletion and replaced in one operation). A same-account process that wins that race can change what the pointer file contains. Consequences are bounded: the pointer is verified on every status read (digest, receipt, file identity from the journal), so a changed pointer shows as *failed / needs attention*, and nothing outside the manager folder is touched. The window is **accepted-and-disclosed** in this proposal (H3); a "detach-then-create" variant is the alternative. Red Team decision required. |
| **Same-account programs** | The managed folder is owner-only, but any program running as the same Windows account can read and write it, and can drive the application window. The in-app confirmation does not defend against that ([MVP-CONSENT-MODEL](MVP-CONSENT-MODEL.md)). The journal is unauthenticated; macOS/Windows identity checks make forgery *detectable*, not impossible. |
| **Smart App Control / Defender** | SAC is in **enforcement** mode on this PC. It intermittently blocks *freshly built unsigned* binaries (`os error 4551`, per-file reputation; it did so several times during this phase and was handled by rebuilding, never by changing a setting — see [WINDOWS-SMART-APP-CONTROL](../WINDOWS-SMART-APP-CONTROL.md)). Nothing in the workflow starts the installed file, so SAC's reaction to *executing* `jq-windows-amd64.exe` is **not tested and not claimed**; its reaction to the file merely being *written* into the manager folder has not been recorded for a real jq (H2 — record only, on the pilot's own authorisation). Defender: no detection or quarantine was observed in the runs of this phase, but Defender events were not systematically collected, so this is an absence of observation, not a result. An anti-virus holding a file briefly shows as `fs_locked_by_another_process`; the adapter reports it and does not retry in a loop. |
| **Other file systems and profiles** | OneDrive Files On-Demand, a redirected profile, a non-NTFS volume, a mount-point transition, a non-ASCII account path: **not exercised** (H5–H7). The managed root refuses a network/unknown filesystem and a volume without the atomic primitives, but real behaviour there is unknown. |
| **Hardware / OS variance** | One CPU, one Windows build, one disk. |

## 3. Is a second physical PC required *for the pilot*?

Proposal (for the Red Team to accept or refuse): **not automatically**, if **all** of the following hold, because the pilot is local, reversible and installation-only:

1. The user explicitly authorises **one** installation of the retained, re-verified file on **this** machine, installation only (no execution, no PATH, no agent configuration), with the preview and confirmation of this phase.
2. The recipe, reference, licence and attestation blockers of [JQ-PILOT-TRANSITION](JQ-PILOT-TRANSITION.md) are closed by reviewed changes first.
3. The H3 decision (pointer-replacement window) is made in writing.
4. SAC/Defender reactions to the *written* file are **recorded only** (H2); any block ends the pilot without any setting being touched.
5. A before/after listing proves nothing outside the manager folder changed; the removal is exercised right afterwards (the whole point of a reversible pilot).
6. The result is recorded as **"works on this one machine"** — it establishes nothing about other computers, other profiles, other security software or other file systems, and the report must say so.

A fresh Windows VM on this PC remains useful supplementary evidence (H1, H2) but is **not** independent hardware and does not close H4/H5.

## 4. What a multi-machine release (closing Issue #14) additionally needs

H1 the acceptance probe on an **independent physical** Windows 11 PC (different hardware, account, security software, ideally a OneDrive-redirected profile); H2 SAC/Defender behaviour recorded there; H5 anti-virus lock interplay with a different scanner; H6 mount-point and cloud-placeholder behaviour; H7 a non-ASCII account path; the H3 decision; a **signed** application (Phase 0 G5) so SAC reputation does not depend on local builds; and for macOS the separate H8 (real Apple Silicon Mac, signed and notarised application, Gatekeeper). The CI Windows and macOS jobs are clean runner images, **not** user machines and not a Mac desktop. **Nothing in this phase closes, narrows or re-labels Issue #14.**
