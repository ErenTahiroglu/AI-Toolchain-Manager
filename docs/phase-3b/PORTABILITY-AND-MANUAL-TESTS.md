# Portability — what is actually known, and manual test checklists

Status: **statement of evidence levels + checklists. No universal compatibility is claimed.** "It compiles on a macOS runner" is not "it works on a Mac", and "it worked on the developer's PC" is not "it works on another Windows computer". Nothing here requires changing Smart App Control, Defender or any security policy.

## 1. The five levels

| Level | Meaning | Evidence that counts |
| --- | --- | --- |
| **L1** | theoretically supported by the framework (Tauri 2, Rust, WebView2 / WKWebView) | documentation only |
| **L2** | compiled in CI | the exact-SHA CI job built the binary for that target |
| **L3** | unit / integration tested | tests executed on that OS in CI or locally |
| **L4** | real GUI tested | a person (or the scripted smoke driver) used the built application on a real machine of that OS |
| **L5** | clean-machine, signed installation tested | a signed installer/bundle installed and run on a machine that never had the development tools |

## 2. Current state (as of this phase)

| Target | L1 | L2 | L3 | L4 | L5 |
| --- | --- | --- | --- | --- | --- |
| Windows 11 x64, the developer's PC | yes | yes (CI `windows-latest`, no bundle) | yes (CI + local, 418+ Rust tests, Python contracts, frontend) | **yes, this PC only**: scripted WebView2 smoke (inventory, project scan, fingerprint, WAL backup preview, plan page) with the **debug** build | **no** — no signed release build or installer has been tested; release builds are verified in CI only |
| Another Windows computer (x64) | yes | same binary class | CI only | **no** | **no** |
| Windows on ARM | yes | **no** (not a CI target) | no | no | no |
| macOS on Apple Silicon (arm64) | yes | yes (CI `macos-14`, build without bundle or signing) | yes — **tests run on a macOS CI runner, which is not a Mac desktop session** | **no** — no real GUI test on a Mac has been done | **no** — unsigned, not notarised |
| macOS on Intel | yes | no | no | no | no |
| Linux | not a supported target | | | | |

Consequences stated plainly: the claims that may be made are "builds and passes the automated tests on Windows x64 and macOS arm64 CI runners" and "was exercised through its GUI on one Windows 11 computer". Claims that may **not** be made: "works on macOS", "works on any Windows PC", "installable by a normal user", "signed".

## 3. What differs between machines and can break things (not yet observed)

* WebView2 runtime version and presence; Smart App Control / Defender / Group Policy blocking an **unsigned** executable (`os error 4551` was seen locally with fresh unsigned *test* executables; the fix is waiting and retrying, never changing SAC); corporate proxies; case-insensitive vs case-sensitive volumes; long paths; non-ASCII user names; OneDrive-redirected folders; cloud placeholders.
* macOS: Gatekeeper and quarantine on a downloaded, unsigned/un-notarised app; App Sandbox and privacy prompts (TCC) if folders outside the app container are read; Keychain access depends on the signing identity; case-insensitive APFS and Unicode normalisation; Rosetta is irrelevant on arm64 builds.
* Both: the app-data directory location, the SQLite WAL on network or synced folders, clock changes (approval expiry), antivirus handle delays during file deletion.

## 4. Manual checklist — another Windows computer (L4, then L5)

Use a computer that has no development tools. Do not change Smart App Control, Defender, SmartScreen or any policy; if something is blocked, **record the exact message and stop**.

1. Record: Windows build number, x64/ARM, WebView2 runtime version, user-name character set (ASCII or not), whether the home folder is OneDrive-redirected. (Do not record user names or paths in anything that is committed.)
2. Copy the debug/release executable produced by the exact-SHA CI artifact (compare its SHA-256 with the CI log first). Start it. Record whether Windows blocked it and the exact message.
3. Start with an **isolated data directory** (`ATM_DATA_DIR` set to an empty temporary folder) — never the user's real data.
4. Walk the screens: dashboard, inventory, project scan (use a throw-away synthetic project folder only), backups preview, plan page. Confirm: no network traffic expected by the user's firewall prompt, no console window flicker, no files created outside the chosen data directory.
5. Close and restart: the previous-shutdown state is reported correctly; kill the process from Task Manager once and restart: the crash is reported, nothing is corrupted.
6. Run `--self-check` headless and keep the JSON; compare with CI's.
7. L5 only: install the signed bundle from the release job, uninstall it, confirm no residue outside the install and data folders.

## 5. Manual checklist — a real Apple Silicon Mac (L4, then L5)

Needs a physical or properly provisioned Mac with Apple Silicon (not a CI runner). Do not disable Gatekeeper or System Integrity Protection.

1. Record: macOS version, architecture (`arm64`), whether the app was copied or downloaded (quarantine attribute present or not), whether the build is ad-hoc signed, signed or notarised.
2. Open the app the way a normal user would (double-click). Record the exact Gatekeeper message if blocked. Do not remove quarantine unless the maintainers decided it is the documented procedure.
3. Use an isolated data directory; walk the same screens as on Windows; confirm window, tray (menu bar) icon, autostart toggle behaviour, and that nothing is written outside the app-data folder.
4. Folder-permission prompts (TCC) for Desktop/Documents/Downloads: record whether the project scan needs them and that the app explains a denial honestly.
5. Keychain/Secure Enclave behaviour for the future approval design: only after Phase 3C defines it, and only with the signed build.
6. Sleep/wake and quit-from-menu-bar once; restart; crash test with `kill -9`; confirm recovery message.
7. Run `--self-check` headless; keep the JSON.
8. L5: install from the signed, notarised DMG/app on a Mac that never had development tools.

## 6. Rules for recording results

Results go to a local note first; anything machine-specific (paths, user names, serials) stays local. A result is entered in the repository only as "OS version, architecture, pass/fail, exact blocking message" after the Red Team agrees. A failed check is reported as failed.
