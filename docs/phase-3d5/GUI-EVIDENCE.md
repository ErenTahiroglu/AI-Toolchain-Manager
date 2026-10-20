# Evidence in the real application window (Phase 3D-5)

Two runs of the **real window** (WebView2, debug builds — never a release build), each with an **isolated data directory**, driven over the WebView2 debugging port on `127.0.0.1` (same method as [Phase 3D-4](../phase-3d4/GUI-EVIDENCE.md)). Both were built from the final application source of this phase (documents, evidence and test files were added afterwards; no application source changed). **No jq file was provided, opened or run, and none existed anywhere in either run.** No Smart App Control block occurred for these two binaries. Developer-machine runs: not independent evidence.

## 1. The B3 activation candidate (`--features jq-pilot`): unlocked, but inactive

`tools/gui_jq_pilot_locked.ps1` · executable SHA-256 `f3af4e887230da6f354d7594991f2ed3a305fd389ae025980b9410f6ce56d3bf` · **16/16 checks true** ([result](evidence/gui-jq-candidate/result.json), [panel text](evidence/gui-jq-candidate/jq-panel-text.txt), [inventory diff](evidence/gui-jq-candidate/inventory-diff.json)).

* jq is listed as **"? Doğrulama tamamlanmadı"** with the reason **"Bu gerçek aracın TEK SEFERLİK kurulum izni bu derlemede henüz etkinleştirilmedi; ayrı bir kullanıcı yetkisi ve ayrı bir değişiklik gerekir. `pilot_not_armed`"** and **no** install/remove button, although `B3_UNLOCK` now names the reviewed recipe digest. (The generic sentence above the reason — *"…incelenmiş bir tarif … yok"* — is the status's standard text and is imprecise for jq; the reason line is the accurate one. Cosmetic, noted, not hidden.)
* Forged IPC (`prepare_install` install/uninstall, a request carrying a path / digest / `allowRealInstall` / `unlock`, `confirm_install` of an unknown offer) is refused; the installed-details show nothing installed.
* **File system**: nothing added, nothing removed; the only changed entries are `atm.sqlite-shm` / `-wal`. **No `atm-managed` and no `jq-pilot-input` folder exists** — the candidate reads no retained file while inactive. One `jq` catalog row, `unreviewed`/`manual-only`/`not_managed`; no `install`/`uninstall`/passed `approve` audit row; no allowance record.

## 2. Regression: the synthetic workflow through the changed service (`--features synthetic-pilot`)

`tools/gui_install_flow.ps1` · executable SHA-256 `fae35d516d79d6314df6c3803fece359171603e94b4d30dbde08520f7e753875` · **75/75 checks true** in phase `first`, **6/6** after a hard restart ([first](evidence/gui-synthetic-regression/first-result.json), [after restart](evidence/gui-synthetic-regression/after-restart-result.json), [preview](evidence/gui-synthetic-regression/first-2-preview.png), [inventory diff](evidence/gui-synthetic-regression/inventory-diff-start-to-final.json)). The synthetic tools carry no one-time rule (they install, remove and reinstall as before); the run shows the allowance code did not disturb them.

## What this does not show

The real jq preview and installation (inactive until a separately authorized, armed commit), an armed pilot in a real window (the armed path needs the real file; it is exercised by the synthetic-bytes tests only), jq itself, Smart App Control's reaction to a real executable, a second computer, macOS.
