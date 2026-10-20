# Evidence in the real application window (Phase 3D-4 / B2)

Two runs of the **real window** (WebView2, debug builds — never a release build), each with an **isolated data directory** and driven over the WebView2 debugging port on `127.0.0.1` (the same method as [Phase 3D-3](../phase-3d3/GUI-EVIDENCE.md)). Both builds were made from the source of the commit that carries these files (the documents and evidence files were added after the builds; no application source file changed afterwards — one Rust *test* file, `install_flow_b2_tests.rs`, was corrected after the builds, which cannot change an application executable). **No jq file was provided, opened or run in either run; nothing real was installed.** No Smart App Control block occurred for these two binaries (the one blocked test executable of this phase is recorded in the [SAC record](SMART-APP-CONTROL-RECORD.md)). These are developer-machine runs: not independent evidence.

## 1. The `jq-pilot` build: the real jq installation is locked (before B3)

`tools/gui_jq_pilot_locked.ps1` · debug build `--features jq-pilot`, executable SHA-256 `471a8ba2f88cdf18c3b075f74b1ae06526b93b5ef2409a96c87ab09fe737e2e8` · **15/15 checks true** ([result.json](evidence/gui-jq-locked/result.json), [picture](evidence/gui-jq-locked/1-catalog-jq-locked.png), [panel text](evidence/gui-jq-locked/jq-panel-text.txt)).

| What the real window and the disk showed |
| --- |
| The catalog lists **jq 1.8.2 (Windows x64) — yerel kurulum pilotu** (the pilot build seeds the row; the 13 ordinary entries are unchanged; no synthetic tool exists in this build). Its install panel reads **"? Doğrulama tamamlanmadı"** with the reason **"Bu derlemede bu gerçek araç için kurulum etkinleştirilmedi … `recipe_not_enabled`"**, and offers **no** install or remove button. |
| Forged IPC: `prepare_install(jq, install)` and `(jq, uninstall)` are refused (`INSTALL_NOT_AVAILABLE`); a request carrying a **path**, a **digest**, an **`allowRealInstall`** or an **`unlock`** field is rejected by the IPC layer; `confirm_install` of an unknown offer is refused. The installed-details of jq show nothing installed. |
| **File system** (before/after inventories, [diff](evidence/gui-jq-locked/inventory-diff.json)): nothing added, nothing removed; the only changed entries are the application's own `atm.sqlite-shm` / `-wal`. **No `atm-managed` folder and no `jq-pilot-input` folder was created** — while locked the application reads no retained file at all. |
| **Database**: exactly one `jq` row — `unreviewed`, `manual-only`, `not_managed`, `unknown`; the audit trail holds no `install`, `uninstall` or passed `approve` row. |

(The row's origin is `user`, so the catalog badge reads *"Sizin eklediğiniz aday"*: deliberately, because the health check counts `seed` rows. This is cosmetic and is noted, not hidden.)

## 2. Regression: the earlier synthetic workflow through the changed preview (final source)

`tools/gui_install_flow.ps1` · debug build `--features synthetic-pilot`, executable SHA-256 `dc7e1b6bb40a94a0dcec4128e10ae2b671c866c6233e47c718fe7113eedeee07` · **75/75 checks true** in phase `first` and **6/6** after a hard restart ([first-result.json](evidence/gui-synthetic-regression/first-result.json), [after-restart-result.json](evidence/gui-synthetic-regression/after-restart-result.json)). The preview now shows two more statements, **"Lisansın kapsamı"** and **"Kurulan dosya nasıl saklanır?"** (*stored as `artifact.bin`; not a command; not on PATH; never run*) — [picture](evidence/gui-synthetic-regression/first-2-preview.png). The inventory diff of the whole run shows only entries under `atm-managed/` added, nothing removed ([diff](evidence/gui-synthetic-regression/inventory-diff-start-to-final.json)).

## What this does not show

The real jq preview and installation (locked until B3, by design), jq itself, a real executable written to disk, how Smart App Control reacts to one, a second computer, macOS, a window-process crash mid-transaction (injected in the Rust tests at the same points as before), a real disk-full or access-denied condition.
