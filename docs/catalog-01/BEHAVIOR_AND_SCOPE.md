# CATALOG-01 — behaviour, scope, before / after

Tracking issue: [#26](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/26) (stays open until independent Red Team acceptance). Issues #14 and #25 stay open. REAL_INSTALLATION_NO_GO.

Baseline (verified before any edit): `development` = `origin/development` = `a6545e81e9a43925a58845f2db547c465426e599`, `origin/main` = `68eda3f0407490fabccb517dcf822ab4818fc0bf` (untouched), clean tree, one worktree. The commit's own SHA and the exact-SHA CI result are not written here; both are in the hand-off reply.

## What this change is
Three user-selected GitHub repositories become **catalog candidates**: metadata the publisher itself describes, read from a pinned commit. Nothing is installed, downloaded, fetched, launched or connected; no key, token, sign-in or payment is involved; no agent configuration is touched. A catalog entry is a row the user can read; "enabled" keeps its existing meaning (the row takes part in the catalog UI and informational suggestions). It does not mean active in any agent.

| Catalog ID | Kind | Category | Repository (pinned commit) | Licence |
|---|---|---|---|---|
| `microsoft-playwright` | `executable_tool` | `library` | `microsoft/playwright` `91a765d1681cb3eeb0d0fb89fefec3cf0d21bd9b` (`main`) | Apache-2.0 |
| `apify-mcp-server` | `agent_plugin` (MCP service) | `mcp-server` | `apify/apify-mcp-server` `cdf5b23230b26cd42e5d0db81528365f6b9cdc9c` (`master`) | MIT |
| `exa-mcp-server` | `agent_plugin` (MCP service) | `mcp-server` | `exa-labs/exa-mcp-server` `0b499f095c4ca145eee07b09137641c037d58449` (`main`) | MIT |

All three: `reviewState: unreviewed`, execution policy `manual-only`, install state `not_managed`, verification state `unknown`, no plugin identity, no source sub-path, no artifact hash or signature, no release version claimed (the pinned commit is the only revision statement), `observedAt` 2026-10-24.

## Where it lives (additive)
* New data file `data/catalog-candidates-c01.json` in the same wrapper shape as `catalog-candidates-1b.json`; every `record` validates against `schemas/catalog-entry.schema.json` v2.
* `src-tauri/src/catalog.rs`: a second loader call for that file (`CATALOG01_COUNT = 3`, `SEED_ENTRY_COUNT` 13 → 16, the shared wrapper parser factored out of the existing candidate loader — same checks, same error for the Phase 1B file).
* `data/catalog-seed.json` and `data/catalog-candidates-1b.json` are **byte-for-byte unchanged** (pinned by SHA-256 in a Rust test and a Python test). The 13 earlier entries' stored records are identical to those files' records.
* No migration, no IPC command, no Tauri capability, no dependency, no frontend change, no discovery (scan) change.

## Behaviour that had to be handled, not just added
1. **A link the user added earlier.** Entries added from a GitHub link get the id `owner-repo`, so a user who had already added `microsoft/playwright` owns the id `microsoft-playwright` — the very id suggested for the seed. Before this change the seed loader silently skipped such an id; it now **reports `SEED_SOURCE_TAKEN`** (same diagnostic the existing source-collision path writes, at each start) and still never overwrites, merges or duplicates. A user-added Apify or Exa link (different id, same source) was already skipped and reported. The user's row, note, enabled flag and removal always win.
2. **The "all seeds present" health check.** `catalog-invariants` required exactly `SEED_ENTRY_COUNT` seed rows. With a user-owned source that seed is legitimately absent, which would have turned the check into a `failed` / `INVARIANT_VIOLATION`. It now accepts `seed rows + seeds held by the user = SEED_ENTRY_COUNT`; "held" means a non-seed row owns the seed's id, or owns its source key while live. An empty or truncated catalog still fails. The check's message now states the number (and that some entries are the user's) instead of the fixed text "13 kaynak kayıtlı".
3. **The read date shown under each entry.** The text "Bilgiler 2026-10-17 tarihli herkese açık belgelerden alındı …" was a constant. It is now the date in the record's own `sourceSnapshot.observedAt`: 2026-10-24 for the new three (as it must be), 2026-10-17 for the Phase 1B four (unchanged), and **2026-10-10 for the Phase 0 nine** — their records have always been stamped 2026-10-10, so the old line was inaccurate for them. This is a display-text correction for the nine, not a change to any stored record. User-added entries keep their offline text.
4. **Counts in existing tests.** Four assertions that hard-coded the catalog size (13 → 16, 14 → 17) were updated to the new size; nothing else in them changed. Two test *names* (`seed_loads_thirteen_entries…`, `migrated_database_gets_the_four_new_entries…`) keep their historical wording.

## Before / after
| | Before | After |
|---|---|---|
| Seeded catalog entries (fresh database) | 13 | 16 (the same 13 plus the three) |
| Candidate count vs tested tools | 13 candidates, none tested | 16 candidates, none tested; three of them carry publisher-declared labels read from pinned commits |
| Upgrade of an existing database | n/a | the three rows appear; every decision, disabled / removed state, preference, history row and the audit chain are as they were |
| A user-added row owning a seed's id or live source | seed skipped, id collision silent | seed skipped, `SEED_SOURCE_TAKEN` recorded, health check still `passed` |
| `README.md` | "13 hazır kayıtlı araç" | "16 hazır kayıtlı aday" with the explicit "none is a tested tool" sentence; the Phase 3D-0 status line is marked as stale, without claiming a new phase |

## Compatibility labels (exactly what the sources support)
Six combinations are stated for every entry (Windows x64 / macOS arm64 × `local-cli` / `claude-code-terminal` / `antigravity-ide`); each carries its limitation. "vendor-declared" means only that the publisher's own text names that surface and operating system; it was not tested, and no CPU architecture is stated by any of the sources.

| Entry | `local-cli` | `claude-code-terminal` | `antigravity-ide` |
|---|---|---|---|
| Playwright | vendor-declared (both OSes: Cross-Browser table lists macOS and Windows) | unknown (agent offerings live in other repositories, not read) | unknown |
| Apify | vendor-declared (both: `manifest.json` lists darwin and win32 for the local stdio package) | vendor-declared (both: README names Claude Code; OS list from the manifest) | unknown (Antigravity not named) |
| Exa | unknown (no OS list) | unknown (a Claude plugin route is documented, no OS is named) | unknown (README lists "Google Antigravity" without naming IDE / CLI / Antigravity 2 or an OS) |

A judgement call for the reviewer: the Apify rows combine a client statement (README) with an OS list that belongs to the package manifest; the Exa rows are kept `unknown` because no OS is named at all. A combination that is `unknown` cannot be set to "accepted" in the project matrix (existing rule, tested here); "postponed" / "rejected" remain possible.

## What is deliberately not done
No separate entries for `microsoft/playwright-cli` or `microsoft/playwright-mcp`; no recipe, no install plan, no artifact digest; no endpoint (`https://mcp.apify.com`, `https://mcp.exa.ai/mcp`) is stored as anything but informational text; no key, token or URL parameter is stored; no detector for these tools was added to the discovery (scan) code, so a stored scan says nothing about them; no recommendation scenario was added; no frontend change.
