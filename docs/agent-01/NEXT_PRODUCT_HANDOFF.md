# AGENT-01 — hand-off: what is done, what is deliberately not, what comes next

Tracking issue: [#31](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/31) — **stays open** for the independent Red Team. Issues #14, #25 and #29 stay open. **REAL_INSTALLATION_NO_GO. REAL_CONFIG_WRITE_NO_GO.** The commit SHA and the exact-SHA CI result are in the hand-off reply and on Issue #31, not here.

## What a user can now do (observable, all read-only)

1. In **Projeler → Proje Araç Matrisi** the "Ajan yüzeyi" list has a fifth entry, **Codex CLI**. Choosing it shows that project's decisions for exactly (project, tool, OS, **Codex CLI**) — an existing Claude Code or command-line decision never appears there and is never changed.
2. For every one of the 22 historic catalog entries the compatibility on Codex is **unknown** (no evidence row exists), so "Bu projede kullanmak istiyorum" is shown as *(şu an seçilemez)* and the backend refuses it; "Şimdi değil" and "Reddet" work and are stored under `codex-cli`.
3. An MCP-server row's **Yapılandırma önizlemesi** for Codex has no text field and no mode choice; **Değişiklik önizle** shows `TOML_PREVIEW_NOT_IMPLEMENTED`, the file (`.codex/config.toml`, TOML, trusted projects only), the catalog gates, the files that are never touched, and a numbered static manual guide.
4. **Projeler** also has **Diğer ajanlar ve MCP/Skills kullanım kılavuzu**: the six-step workflow, the "things are different" list, the surfaces the application manages, documented examples (Gemini CLI, Grok/xAI, Groq, DeepSeek, Gemini API) and a name classifier that answers only from that table and otherwise `MANUAL_VERIFY`.
5. The **Kurulum Planı** page still offers the four surfaces the installer-side plan layer knows; Codex CLI is not one of them.

## What was deliberately NOT done

* No Codex TOML reading, parsing, diffing or writing; no `.codex/config.toml` or `~/.codex/config.toml` touched; no trust decision made; no `codex` process; no network.
* No catalog evidence row for Codex; no change to any data file, schema or seed; the 22 historic identities and their cells are unchanged.
* No fifth dimension (no "provider" in the preference key). The key remains (project, catalog entry, OS, surface).
* No brand-named adapter for any provider or client.
* No reviewed declaration (C5: NO QUALIFIED DECLARATION — see `IMPLEMENTATION_AND_TESTS.md`).
* No new IPC command, Tauri capability or dependency. **One migration was unavoidable** — see the review request below.

## Review requests for the Red Team (in order of importance)

1. **Migration 10 (`src-tauri/migrations/0010_codex_cli_surface.sql`).** Saving a Codex preference needs `project_preferences.agent_surface`'s `CHECK` widened, and SQLite cannot alter a `CHECK`, so the table is rebuilt. It copies every column of every row in `rowid` order, changes nothing else, drops and renames the table, and touches no other table. The existing `VACUUM INTO` pre-migration snapshot still runs; a failing step rolls back (tested). Please review that "all rows byte-for-byte unchanged" is enough evidence, and whether a rebuild on a real user database (rather than the in-memory and Python/sqlite3 checks) should be exercised by a separate, authorised step. An intermediate local Rust test build was **blocked by Smart App Control** (recorded in `IMPLEMENTATION_AND_TESTS.md`, not evaded); the final build ran (`cargo test --lib`: 778 passed, 0 failed, 2 ignored). The same claims are also checked independently in `tests/test_agent01_contract.py` with Python's `sqlite3` on a schema built from migrations 1–9 with foreign keys on.
2. **Is "not-assessed" honest enough** in the capability registry? (It is meant as "not read for this phase", never as absence.)
3. **Origin fallback.** An observation recorded for the command-line lookup (`local-cli`) may already be shown as a *fallback* ("yedek eşleşme") for a Codex request, exactly as it is for the other agent surfaces. It is labelled as not being evidence about the requested surface; the label is existing PRODUCT-06 behaviour and was not changed.
4. **Free-text lookup in the guide** is an exact table lookup; check that no phrasing reads as endorsement of a provider or client.

## What comes next (not started; each needs a new explicit instruction)

* **After independent acceptance of AGENT-01:** resume **PRODUCT-07 (#29)** for a bounded evidence-ranking policy proposal.
* Then **project-scoped CONFIG-02 write-path planning**, still subject to new explicit authorisation and the security gates in `docs/config-01/FUTURE_WRITE_GATE.md` (which now also covers a TOML adapter: a *separate* adapter with its own parser, golden tests and review — never a branch of the JSON one).
* Optional, only with evidence: a first vendor-declared Codex row for a specific tool/OS (a source-pinned data change plus the schema enum), a first **reviewed declaration** for the registry (what is missing is listed in `IMPLEMENTATION_AND_TESTS.md` §C5), and — if ever wanted — a reviewed restricted-TOML preview (criteria in `CODEX_SOURCE_AND_SCOPE.md`).

## What a first reviewed declaration would need (so nobody guesses)

An *independent human* reviewer who signs the review record (the registry requires `reviewId`, evidence ids and a 40-hex `reviewedAgainst`); an exact version with its transitive resolution understood (a package runner fetches code at every launch and an exact top-level pin does not pin the tree); the README **at that exact release tag** read for flags and defaults; an explicit privilege and data-disclosure decision (browser profile, page JavaScript, file upload, "unsafe" tool); and the project-file side effect that Claude Code's headless runs load project servers without asking. For Playwright MCP those items are open, which is why the registry stays empty.
