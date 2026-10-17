# Mac → Windows Handoff — Phase 1A

**Repository:** https://github.com/ErenTahiroglu/AI-Toolchain-Manager
**Branch:** `development` (only active branch; `main` untouched)
**Base SHA before this handoff commit:** `68735a7` (docs(git): adopt two-branch main/development phase governance)
**Final SHA:** the commit that adds this file (see `git log -1 -- docs/handoffs/MAC_TO_WINDOWS_PHASE1A.md`; the exact value is reported in the session summary, since a file cannot contain its own commit hash).
**Date:** 2026-10-17

## 1. Repository state at handoff
- Local `development` == `origin/development` (0 ahead / 0 behind) before this commit; working tree clean; no stashes; single worktree.
- Only untracked/ignored files: `.DS_Store`, `tests/.DS_Store` (macOS metadata, ignored, disposable).
- No background cargo/tauri/vite/rustc processes.
- Legacy branches `phase0/architecture` (`68eda3f`, equals `main`) and `phase1a/desktop-shell` (`6e427d1`, ancestor of `development`) are fully contained in `main`/`development`. They were not deleted.
- GitHub default branch is already `main`.

## 2. Implemented feature inventory
Phase 0 only (architecture contracts, schemas, seed catalog, fixtures, tests, Prompt Intelligence ADR, branch governance).
**No Phase 1A source code was written.** Phase 1A started with the repository audit only; the work stopped at dependency planning because Rust is not installed on the Mac and the user decided not to install it.

## 3. Incomplete Phase 1A work
Everything: Tauri 2 shell, React/TypeScript UI, SQLite layer + migrations, tray, single-instance, autostart preference, Dashboard, Projects (mock), Tool Library (9 seed tools), Prompt Library (3 reference sources), Activity & Self-Audit, Settings, tests, CI. See the Phase 1A specification given to the agent.

## 4. Build and test status
- Executed on Mac: Phase 0 contract tests — `python3 -m unittest discover -s tests` → 16 tests OK (needs `jsonschema==4.23.0`, see `requirements-test.txt`).
- Rust compilation: **never run** (no Rust toolchain on Mac). No Rust, Windows or CI result exists.

## 5. Windows development dependencies (to be installed by the user on Windows)
- Windows 10 1809+ x64 (or Windows 11); Git; GitHub CLI (optional)
- Microsoft C++ Build Tools (MSVC, "Desktop development with C++") and WebView2 runtime (preinstalled on Win 11 / current Win 10)
- Rust stable ≥ 1.90 via rustup (MSVC toolchain `x86_64-pc-windows-msvc`)
- Node.js LTS + npm
- Python 3 + `pip install -r requirements-test.txt` for Phase 0 tests

Candidate versions seen on 2026-10-17 (verify and lock at install): tauri 2.12.4, tauri-build 2.7.3, tauri-plugin-single-instance 2.5.4, tauri-plugin-autostart 2.7.0, rusqlite 0.40.2 (bundled), @tauri-apps/cli 2.12.3, @tauri-apps/api 2.12.4, react 19.3.1, vite 8.3.4, @vitejs/plugin-react 6.1.2, typescript 7.0.2, vitest 5.0.5.

## 6. Safe setup
```powershell
git clone https://github.com/ErenTahiroglu/AI-Toolchain-Manager.git
cd AI-Toolchain-Manager
git checkout development
python -m pip install -r requirements-test.txt
python -m unittest discover -s tests   # expect 16 OK
```
Install toolchains only from official sources; commit lockfiles (`package-lock.json`, `Cargo.lock`).

## 7. Mac-specific assumptions to adapt
- Paths in notes use `/Users/eren/...`; none are in the committed code.
- Shell was zsh; use PowerShell/Git Bash on Windows. Beware CRLF: set `git config core.autocrlf input` or add `.gitattributes` (`* text=auto eol=lf`) before adding source files.
- App data dir will be `%APPDATA%` on Windows vs `~/Library/Application Support` on macOS; use Tauri path APIs, never hard-coded paths.
- Tray, autostart (Login Items vs registry Run key) and single-instance behave differently per OS; test on each.
- macOS targets ARM64 only; Windows x64 only.

## 8. Issues / blockers
- None blocking the transfer. Windows build unverified. Legacy branch cleanup is still a user action (see `docs/BRANCH-GOVERNANCE.md`).

## 9. Excluded from public Git
`.DS_Store` files only (already gitignored). No secrets, backups, or private files existed; no private archive was created.

## 10. Next development steps
1. Set up Windows toolchain (section 5), run Phase 0 tests.
2. Start Phase 1A on `development`: scaffold Tauri 2 + React/TS; Rust-owned SQLite (no frontend SQL permissions); migrations; seed from `data/catalog-seed.json`.
3. Implement screens and the self-audit per the Phase 1A spec (UNKNOWN/FAILED states, redaction, no real-environment access).
4. Add GitHub Actions matrix (`windows-latest`, `macos-latest` ARM64) running Rust tests, frontend tests and Phase 0 tests.

## 11. Independent macOS requirements
macOS 13+ ARM64 must be validated independently: CI on a `macos-latest` (Apple Silicon) runner and/or a manual run on the Mac with Rust installed by the user later. Windows results must not be used as macOS evidence, and vice versa.

## 12. Evidence nothing was lost
- Pre-commit tree was clean and identical to `origin/development` at `68735a7`; no local-only commits, stashes or worktrees.
- Tracked files at handoff: see `git ls-tree -r origin/development --name-only` (Phase 0 docs, schemas, data, tests, reviews, ADR).
- Phase 0 tests re-run: 16 OK.
