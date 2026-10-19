# User-facing evidence: the real application window, driven end to end (Phase 3D-2)

**What was run.** `tools/gui_install_flow.ps1` started the **real application window** (a debug build made with `--features synthetic-pilot`, **never the release build**; SHA-256 of the executable `3f5e60b8dfbc57481601e6673ff83008380e07ca51b7dca5fc56c4028a5f3069`, built from this commit's source plus one temporary comment line that was removed afterwards — Smart App Control had refused to start two earlier, otherwise identical builds, and the only remedy used was a rebuild, never a setting) with an **isolated data directory** and drove it through the WebView2 debugging port on `127.0.0.1` (this machine only) with `tools/gui_install_flow.mjs`. After each step the file system of the isolated directory was inspected. Only synthetic tools and bytes were involved; nothing installed was executed; no network, no Windows Hello, no real tool. Raw results: [`evidence/gui/first-result.json`](evidence/gui/first-result.json) and [`after-restart-result.json`](evidence/gui/after-restart-result.json); pictures next to them.

**Result: 54/54 checks true in phase `first`, 6/6 in phase `after-restart` (the first process was killed, not closed).**

| Area | What the real window and the disk showed |
| --- | --- |
| Statuses | 16 install panels = 13 real catalog entries + 3 synthetic. `synthetic-hello` *Kurulabilir*; `synthetic-unverified-tool` *Doğrulama tamamlanmadı*; `synthetic-license-pending` *Lisans incelemesi gerekli*; a **real** entry (gitleaks) *Doğrulama tamamlanmadı* with no install button, and a forged `prepare_install` for it through the IPC is refused. Looking creates nothing on disk. |
| Preview | version, platform, digest, file name, every filesystem effect (relative places, none absolute), "nothing is executed / no network / PATH untouched", notices with unresolved points, "this confirmation is not an identity proof", SENTETİK DENEME banner; the confirm button is disabled until the acknowledgement is ticked; opening a preview creates nothing. ([picture](evidence/gui/first-2-preview.png)) |
| Forged IPC (called from the page through the real IPC) | unknown extra field (`path`) rejected by the IPC layer; `acknowledged:false` refused and the offer **burned**; the correct digest afterwards refused; made-up and malformed offer ids refused; still nothing created. |
| Cancel | "Vazgeç" withdraws the offer; status returns to *Kurulabilir*; a replay of the cancelled id is refused; nothing created. |
| Install | result *Tamamlandı* with receipt ([picture](evidence/gui/first-3-result.png)); the file sits exactly where the preview said and **its SHA-256 equals the digest shown in the preview**; the version folder holds only `artifact.bin` (no sidecar/notice file); the manager folder holds exactly the fixed layout; no lock/staging/snapshot leftovers; nothing outside the manager folder and the database; a second install of the same tool is refused before any preview. |
| Details | version, relative location, digest, ≥ 5 passed live checks, receipts, notices, "what the application never does"; no absolute path or account name ([picture](evidence/gui/first-4-details.png)); the dashboard counts one installed tool; the audit chain verifies, and **all 36 audit rows of the run validate against the Phase 0 audit-event schema** (an earlier run of this same check found an unlisted `verdictSource` value, which was fixed and pinned by a Rust test). |
| Removal | preview → confirm → *Kaldırma tamamlandı* ([picture](evidence/gui/first-5-removed.png)); version folder and pointer gone; the tool's own user data **byte-identical**; status *Kaldırıldı*; reinstall works. |
| Tamper | one flipped bit in the installed file → *Başarısız / dikkat gerekiyor* (`installed_artifact_drift`), **nothing repaired or deleted** (the tampered bytes are still there), no install/removal can be opened; after the owner restores the file the tool verifies again ([picture](evidence/gui/first-6-tampered.png)). |
| Restart | after killing the process and starting it again: still *Kurulu ve doğrulandı*; an offer id of the earlier process is `consent_unknown`; recovery has nothing to do; no leftovers; the audit chain verifies. |

## Local NTFS acceptance probe (same build, development PC)

[`evidence/gui/fs-acceptance-probe-dev-pc.json`](evidence/gui/fs-acceptance-probe-dev-pc.json): filesystem `ntfs`, 13 pass, 1 not exercised (link creation needs a privilege), 3 observed, 0 failed. This is the machine the code is written on — **not independent evidence** (see [Issue #14 assessment](ISSUE-14-LIMITED-PILOT-ASSESSMENT.md)).

## What this does not show

A real tool, a real executable, Smart App Control's reaction to executing an installed file, a second computer, a macOS window, Windows Hello, a mid-transaction crash of the *window process* (crashes are injected in the Rust tests at 13 points; the GUI run only kills the process between operations), or any multi-user situation.
