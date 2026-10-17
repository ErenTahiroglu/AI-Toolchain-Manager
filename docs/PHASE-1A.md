# Phase 1A — Cross-Platform Desktop Foundation

**Status:** implemented on `development`; awaiting independent Red Team review. Not merged to `main`. Phase 1B has not started.
**Stack:** Tauri 2 (Rust) · React 19 + TypeScript · SQLite (bundled `rusqlite`, owned by Rust).

This document records what Phase 1A does, what it deliberately does not do, and exactly which evidence exists. It does not authorize Phase 1B, installation, sync or Prompt Intelligence.

## Scope delivered

| Area | Behaviour |
| --- | --- |
| Shell | Tauri window, six-section navigation (Ana Sayfa, Projeler, Araç Kütüphanesi, İstem Kütüphanesi, Etkinlik ve Denetim, Ayarlar), Turkish UI, light/dark/system theme, keyboard navigation, skip link, focus management, status never shown by colour alone. |
| Database | `src-tauri/migrations/0001_init.sql` applied transactionally via `PRAGMA user_version`. Backup (`VACUUM INTO`) before any later migration; a database from a newer app version is refused untouched; a corrupt file is reported, not a panic. |
| Phase 1A invariants in SQL | `CHECK` constraints: `install_state = 'not_managed'`, `validation_state = 'unknown'`, only synthetic projects (`is_synthetic = 1`), prompt sources `planned` + read-only + `untrusted-reference`. A later reviewed migration must relax them. |
| Tool Library | The nine records of `data/catalog-seed.json` (single source of truth, embedded at compile time), all `unreviewed`, none installed or verified. Every "Kur" button is disabled with a reason. |
| Prompt Library | prompts.chat, the-prompt-library, Fabric as read-only planned references. No download, search or rewriting (Phase 4P, ADR-0001). |
| Projects | Three clearly labelled synthetic sample projects ("ÖRNEK VERİ" banner, "örnek" tags). Real state is shown as unknown. |
| Self-audit | Deterministic in-process checks with four honest results: `passed`, `failed`, `unknown`, `skipped`. Overall = failed > unknown > passed; skipped never counts as success. These are *operational health*, not independent verification. |
| Audit chain | Append-only hash-chained events that conform to `schemas/audit-event.schema.json` v2 (verified independently by Python). Detects accidental edits, deletion, truncation, reordering; **not** proof against rewriting by the same OS user. |
| Diagnostics | Redacted (paths, tokens, e-mail, URL credentials), length-capped, row-bounded local records. |
| Lifecycle | Single instance (second launch focuses the first), tray (open/quit), close-to-tray only when the tray exists, user-controlled startup (default **off**, OS state read back and verified after every change, mismatch surfaced), unclean-shutdown detection, panic marker ingested on next start, React error boundary + global handlers reported to the local log. |
| Least privilege | No plugin permission is granted to the WebView; only 14 typed app commands, each an explicit `allow-*` permission generated from `build.rs`. A health check and unit tests reject shell/fs/sql/http/opener/core permissions. Strict CSP. |

## Explicitly not implemented (by design)

Real tool installation, agent-configuration edits, GitHub fetching or sync, private repository, updater, telemetry, any network request, Prompt Intelligence retrieval/rewriting, packaging, signing, notarization.

## Platform notes

* Data directory: Tauri `app_data_dir()` (`%APPDATA%\com.erentahiroglu.aitoolchainmanager` on Windows, `~/Library/Application Support/...` on macOS). `ATM_DATA_DIR` overrides it for tests/CI only.
* Autostart: Tauri plugin (HKCU `Run` value on Windows, LaunchAgent on macOS). It passes `--minimized`; the app then stays in the tray only if a tray icon was actually created.
* `--self-check <out.json>`: headless mode that opens an isolated database, runs the checks and writes JSON. Used by CI and `tests/test_phase1a_audit_contract.py`.
* No Windows-only code paths were introduced; platform differences are handled by Tauri/OS APIs. Windows evidence must not be read as macOS evidence.

## Windows developer notes

* Windows **Smart App Control** (enforcement mode) blocked one unsigned release-profile build-script executable (`os error 4551`) during a local `tauri build`. The debug-profile build and tests are unaffected. SAC was **not** disabled. CI runners do not use SAC and build the release profile.
* A stale `cargo check` cache once produced `can't find crate for time_macros`; `cargo clean -p time -p time-macros` fixed it. Not a code problem.

## Run locally (Windows)

```powershell
npm ci
npm run tauri -- dev          # development window
# or, compile-only build (no installer, unsigned):
npm run tauri -- build --debug --no-bundle
.\src-tauri\target\debug\ai-toolchain-manager.exe
```

Tests: `npm test`, `cd src-tauri && cargo test`, `python -m unittest discover -s tests`.
