# Real-device test plan for the managed filesystem layer (Phase 3D-0)

Goal: obtain evidence that **CI cannot give** — behaviour on a second Windows computer and on a real Apple Silicon Mac, including external volumes — **using artificial files only**. jq is not needed, not downloaded and not run. Nothing here changes PATH, agent configurations, Smart App Control, Defender, SmartScreen or Gatekeeper, requires administrator rights, or uses the network.

## 1. What is run

The headless self-check of the application binary contains an acceptance probe (`fsAcceptanceProbe`, code in `src-tauri/src/managed_fs_probe.rs`). It creates **one new uniquely named folder** below the self-check data directory, performs the checks of §4 on files it writes itself, removes the folder, and reports short result codes (no paths, no account name, no content). The data directory is always an **isolated folder chosen by the tester** (`ATM_DATA_DIR`); to test an external volume the tester points `ATM_DATA_DIR` at an empty folder on that volume.

## 2. Getting the binary (choose one; record which)

* **A. Build on the device** from the exact commit (needs Rust and Node on that device — developer tools; installing them is the tester's own decision, not part of this plan).
* **B. CI artifact.** The CI job of the exact SHA uploads `device-test-unsigned-<platform>-<sha>` (unsigned binary + `SHA256.txt`, kept 3 days). Verify the SHA-256 against `SHA256.txt` and against the commit in the CI log before running. On macOS the downloaded file is quarantined and has lost its execute bit: **do not remove the quarantine attribute or change Gatekeeper settings**; run it from Terminal after `chmod +x`; if macOS refuses to run it, **record the exact message and stop** (that is a result, not a failure to hide) and fall back to A.
* The existing development PC uses its own debug build.

## 3. Procedure (every device)

1. Record: OS name and build, architecture, file system of the test volume, whether the volume is internal or external, whether the account name or home folder contains non-ASCII characters, whether the home folder is cloud-synced (OneDrive/iCloud). Keep these notes local; the repository receives only "OS version, architecture, volume type, result".
2. Create an **empty** folder `atm-device-test` on the volume to test. Set `ATM_DATA_DIR` to it (PowerShell: `$env:ATM_DATA_DIR = "<folder>"`; Terminal: `export ATM_DATA_DIR="<folder>"`).
3. Run `ai-toolchain-manager --self-check result.json` (Windows: `.\ai-toolchain-manager.exe --self-check result.json`). Exit code 0 is expected.
4. Open `result.json` → `fsAcceptanceProbe`. Copy `platform`, `arch`, `filesystem`, `failed` and the `checks` list (result codes only) into the notes.
5. Windows only, optional (no administrator needed if Developer Mode is on): run once as a **standard user** and once from a folder that is redirected (OneDrive "Documents") to record the difference.
6. Delete the `atm-device-test` folder yourself afterwards. Do not run anything else from it.

Block rules: if Smart App Control, SmartScreen, Defender or Gatekeeper blocks the binary, **do not bypass it** — capture the sanitized event (Windows: Event Viewer → Microsoft-Windows-CodeIntegrity/Operational, event 3077/3033; macOS: the exact dialog or Terminal message), mark the test **blocked**, and stop.

## 4. What each check proves and how it is classified

Classes: **L** verified locally (development PC, this phase) · **CW** verified in Windows CI · **CM** verified in macOS CI (a runner, not a Mac desktop) · **R** requires real-device testing · **B** blocked or unsupported.

| Requirement (probe check id or test) | Dev PC (Win 11, NTFS) | Windows CI | macOS CI (APFS runner) | Second Windows PC | Real Apple Silicon Mac |
| --- | --- | --- | --- | --- | --- |
| Anchor opens without following links (`anchor_opens_without_following_links`) | L | CW | CM | R | R |
| Owner-only root created and read back | L | CW | CM | R (also on a OneDrive-redirected profile) | R |
| Exclusive create refuses an existing name; identity and link count from the handle | L | CW | CM | R | R |
| **No-clobber move supported and enforced** (`no_clobber_move_*`, the OS primitive) | L | CW | CM (APFS) | R | **R on the internal volume and on an external volume** (exFAT/FAT/APFS/HFS+/network → expected `Unsupported` where the volume lacks `RENAME_EXCL`) |
| Replacement of one known file; unknown destination refused | L | CW | CM | R | R (needs `RENAME_SWAP`) |
| Verified deletion refuses hard links and same-content replacements | L | CW | CM | R | R |
| Links refused and never followed (`a_link_is_reported_and_refused`) | **not exercised here** (creating a link needs Developer Mode/privilege; junction covered by unit tests) | CW (junction, unit test) | CM (symlink) | R (with Developer Mode, or by the unit test route) | R |
| Held folder cannot be swapped (Windows) / is not redirected (macOS) — *observed* | L: prevented by the OS | CW | CM | R | R |
| Open file: Windows `Locked`, macOS name removed and holder unaffected — *observed* | L: locked | CW | CM | R (anti-virus can add locks) | R |
| Folder flush support — *observed* | L: flushed | — | CM | R | R |
| Mount transition (real) | **B**: simulated only | simulated | simulated | R (volume mount point) | R (mounted disk image under a managed sub-folder) |
| Cloud-sync placeholders | — | — | — | R (OneDrive Files On-Demand) | R (iCloud Drive "Optimize storage") |
| Non-ASCII account/home path | L (Unicode folder tests) | CW | CM | R | R |
| Smart App Control reaction to the unsigned binary | L: intermittently blocks fresh local build output (diagnosed) | n/a | n/a | R (record only) | Gatekeeper: R (record only) |
| Real approval key storage (Windows Hello / Secure Enclave) | **B**: not implemented | — | — | R after implementation | **B**: needs a signed build |

CI is *not* clean-machine evidence: runners are headless, freshly provisioned, do not enforce Smart App Control and have no user session UI. A "CW/CM" entry means "the same code passed there", nothing more.

## 5. Recording the result

A device run is entered into the repository only as: date, OS and version, architecture, volume file system and kind (internal/external), probe `failed` count, and the list of `observed`/`not_exercised` codes. Any `fail` is reported as a failure of this requirement with the code; none is retried until it passes. Machine-specific details (paths, names, serials) stay in the tester's local notes.

## 6. Current status (this phase)

* Development PC (Windows 11 x64, NTFS): probe run through the built debug binary — 13 checks pass, 1 not exercised (link creation needs a privilege), 3 observed (held folder prevented, open file locked, folder flushed); `failed: 0`.
* Windows CI runner (`windows-latest`, NTFS), release binary, exact SHA `84d1dfb`: `failed: 0`, 14 pass (including link refusal: the runner may create symlinks), 3 observed (held folder prevented by the OS, open file locked, folder flushed).
* macOS CI runner (`macos-14`, Apple Silicon, **APFS** — a headless runner, not a Mac desktop), release binary, same SHA: `failed: 0`, 14 pass — `renameatx_np(RENAME_EXCL)` refused an existing destination atomically and left it unchanged, `RENAME_SWAP` replaced the known file, links were refused — and 3 observed (a held folder **can** be removed by another process while the held descriptor is not redirected; an open file's name **can** be removed while the holder is unaffected; folder flush supported). This is the first evidence that both Apple primitives exist on APFS; it says nothing about external volumes.
* Validated by `tests/test_phase3d0_contract.py` against the self-check output on both runners.
* Second Windows PC and real Mac: **not run** (no such device was available to this session).
