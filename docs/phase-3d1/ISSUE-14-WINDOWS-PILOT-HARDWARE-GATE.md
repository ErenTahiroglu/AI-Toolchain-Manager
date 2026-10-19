# Issue #14 and the hardware gate of the first Windows pilot (Phase 3D-1)

**Issue #14 stays open.** Green CI, the Windows runner and the local development PC do not close it. This note takes the requirements of [DEVICE-TEST-PLAN.md](../phase-3d0/DEVICE-TEST-PLAN.md) and [ISSUE-14-RESIDUAL-RISKS.md](../phase-3d0/ISSUE-14-RESIDUAL-RISKS.md), prioritises the Windows x64 pilot, and decides what each possible test machine can and cannot stand for. No acceptance criterion is weakened; no Smart App Control or Defender setting is touched.

## 1. What exists today

| Evidence | Where | Class |
| --- | --- | --- |
| Acceptance probe (`fsAcceptanceProbe` in `--self-check`): 13 pass / 1 not exercised / 3 observed | development PC, NTFS, locally built unsigned binary | **real device — but the same machine the code is written on** |
| Same probe, 14 pass | GitHub Windows runner (NTFS) | CI runner: a clean image, **not** a user's PC |
| Same probe on macOS, incl. `RENAME_EXCL`/`RENAME_SWAP` | GitHub macOS runner (APFS) | CI runner: **not** a Mac desktop |
| Behavioural tests of the whole adapter, fault injection, rehearsal | all of the above | tests, not device evidence |
| `windowsHelloProbe` (read-only) | development PC | availability only |

## 2. What the first Windows pilot still needs from real devices (priority order)

| # | Requirement | Gate for | Satisfied by |
| --- | --- | --- | --- |
| H1 | The acceptance probe on an **independent** physical Windows 11 PC (different hardware, different account, preferably with a OneDrive-redirected profile and a different anti-virus) with the unsigned CI artifact `device-test-unsigned-windows-x64-<sha>` | first real install | a second physical PC |
| H2 | Smart App Control / Defender reaction to the unsigned binary and to a file of the type `jq-windows-amd64.exe` **written (not run)** into the managed folder — recorded only | first real install | the second PC and the development PC |
| H3 | The Windows file-replacement window (pointer file): Red Team decision (accept-and-disclose proposed, or the "detach-then-create" option) | first real install | decision, not hardware |
| H4 | Windows Hello enrollment, one real signature, attestation status, prompt behaviour for the real application window, deletion of the key | first real install | the development PC (Hello is configured there) **after** its own explicit authorisation; ideally repeated on the second PC |
| H5 | Anti-virus lock interplay (`Locked` observed; retry behaviour) on a PC with a *different* scanner than the development PC | recommended for the pilot, required for general release | second PC |
| H6 | Mounted volume / mount-point transition, cloud-placeholder behaviour (OneDrive Files On-Demand) | general release | second PC |
| H7 | Non-ASCII account/home path | general release | second PC |
| H8 | macOS: all rows of the plan on a real Apple Silicon Mac, signed application, Secure Enclave | **macOS pilot only** | a real Mac — **separate blocker, not part of the Windows pilot** |

H1, H2, H3 and H4 gate the **first real installation**; H5–H7 gate wider use; H8 gates only the macOS pilot.

## 3. Can the "second Windows PC" be satisfied by something other than a second physical PC?

Possibly **for some rows, never silently for all**. The criterion is *independence* of hardware, installation and security software from the machine the code was written on; a virtual machine hosted **on that same PC** shares the host's hardware, its disk, its Defender updates and (for Hyper-V enhanced sessions, shared folders and clipboard) parts of its user session.

| Row | Fresh Windows VM on the development PC | Independent physical PC |
| --- | --- | --- |
| H1 file-system behaviour (NTFS semantics, handle-relative operations, `FileRenameInformationEx`) | **Useful**: a different Windows installation and profile; same virtual-disk-on-NTFS stack and same hardware | required for acceptance |
| H2 Smart App Control reaction | **Useful and cheap**: SAC state is per installation (a fresh VM may start in evaluation mode); record only | required: the real machine's policy is what matters |
| H3 decision | n/a | n/a |
| H4 Windows Hello / TPM | **Not equivalent**: a virtual TPM may exist, but Hello biometrics and the physical TPM attestation path are not exercised; any "hardware-backed" claim from a VM is not evidence about real hardware | required (the development PC may serve for the first run; a second physical PC for independence) |
| H5 anti-virus interplay | not equivalent: same scanner family as the host unless a third-party product is installed in the guest | required for acceptance |
| H6/H7 OneDrive, mounts, profile paths | partly (mounts, Unicode paths); OneDrive KFM needs an account and is a different test | required for general release |
| Flush/power-loss semantics | **not equivalent**: the virtual disk may cache writes differently | required if claimed |

**Position:** a VM run is welcome as *additional* evidence, labelled `VM (same host)`, and may be proposed to the Red Team as satisfying H1/H2 for a **first pilot on the development PC only** if the Red Team accepts that explicitly in writing; it **cannot** replace H4/H5 and cannot close Issue #14. A genuinely independent machine (another person's or a spare physical PC, not hosted on this one) satisfies the criterion as written. A cloud Windows VM (e.g. a hosted desktop) is independent hardware but is a *virtual* machine and is classified like the local VM, plus a network-access question this project has not authorised.

## 4. What is preserved, unchanged

* The documented Windows file-replacement window and the macOS unlink window ([ISSUE-14-RESIDUAL-RISKS.md](../phase-3d0/ISSUE-14-RESIDUAL-RISKS.md)) are untouched; no mitigation was implemented in this phase.
* Smart App Control, Defender and Gatekeeper are not altered; the intermittent SAC block of fresh unsigned test binaries (`os error 4551`) still occurs on the development PC and was handled again in this phase by changing a source comment and rebuilding, never by changing a setting.
* The runner results are labelled as runner results everywhere; no claim of a real Mac desktop or of a clean-machine/release validation is made.

## 5. Procedure for H1 (unchanged; for the second PC)

1. Download the CI artifact `device-test-unsigned-windows-x64-<sha>` from the exact-SHA run (its SHA-256 is printed in the run) on the second PC.
2. Run `ai-toolchain-manager.exe --self-check <output.json>` with `ATM_DATA_DIR` pointing at an empty, isolated folder (and again at a folder on any external or redirected volume).
3. Send back the JSON (result codes only: no path, no account name, no content).
4. If Smart App Control blocks the unsigned binary, **record** the event and stop; do not change any setting.
