# CATALOG-02 — description-quality requirement, audit and next backlog

Nothing here proposes installing anything. REAL_INSTALLATION_NO_GO; Issues #14 and #25 stay open; no unsupported claim is invented for any entry below.

## 1. Catalog-wide description-quality requirement (applies to every future repository)
An entry is not ready to be added to the shipped catalog until its data answers all of these. Items Q1–Q6 are machine-checked (`catalog_c02_tests.rs::the_description_quality_gaps_of_the_catalog_are_exactly_the_documented_backlog`); item M is a human review item.

| Code | Requirement | Checked how |
|---|---|---|
| Q1 | **Purpose summary:** 100–300 characters of Turkish prose, not a placeholder; what it does → when one would choose it over a neighbouring option → one concrete limitation or privacy caveat → what this application does *not* do. No copied README, URL, command or markdown. | length and placeholder text by test; wording by review |
| Q2 | **Licence:** a known SPDX identifier with an evidence id (otherwise `review-required`/`unknown`, stated as such). | test |
| Q3 | **Source pin and evidence date:** an immutable commit (or, failing that, a clearly labelled release tag) and an `observedAt` date; evidence ids registered in `docs/RESEARCH-SOURCES.md`. | test (immutable commit + date); register by the Python contract test |
| Q4 | **Runtime requirements:** the runtimes and tools the publisher's own manifest or README names (or an explicit "none stated" in the dependency list). Disagreements between sources are recorded, never resolved by guessing. | test (non-empty) |
| Q5 | **Cost / credentials / network / privacy implications:** `networkClaims` and `privacy.notes` filled in; `mayTransmitProjectContent` and `telemetry` set conservatively (`unknown` / `null` when not evidenced). | test (non-empty) |
| Q6 | **Compatibility:** all platform × agent-surface cells the app offers, each with a stated limitation; `unknown` unless the publisher's own text names the surface **and** the operating system. | test |
| M | **Relevant project situations:** when this entry fits and when a neighbouring one does (for families of alternatives, an explicit comparison like "Playwright seçenekleri"). | human review of each new entry; not machine-checkable |

A new entry must pass Q1–Q6 or be added to the documented-gap list on purpose, in the same change, with the reason.

## 2. Audit of the 16 earlier entries (nothing fabricated; gaps listed, not filled)
Method: the criteria above, applied by a Python script over the four data files **and** independently by the Rust test over the seeded database — both produce this list. Entries not listed meet Q1–Q6.

| Entry | Fails | What is missing | What would close it (needs a source read, not a guess) |
|---|---|---|---|
| `ponytail` | Q1, Q3 | purpose is the placeholder "Amacı bu sürümde ayrıntılı incelenmedi" (66 characters); pinned to release tag `v5.1.0` only (`release-tag-observed`, no commit) | read the publisher's README at a commit; write a real summary; pin the commit |
| `codebase-memory-mcp` | Q1, Q4 | one-line purpose (55 characters); no runtime requirements listed | read the README/manifest; list its runtime needs |
| `graphify` | Q1 | one-line purpose (53 characters) | summary with a "when" and a caveat |
| `gitleaks` | Q1, Q4 | one-line purpose (57); no runtime requirements | as above (a single native binary is plausible, but "plausible" is not evidence) |
| `markitdown` | Q1 | one-line purpose (48) | summary with a "when" and a caveat |
| `caveman` | Q1 | one-line purpose (55) | summary with a "when" and a caveat |
| `cloudflare-security-audit-skill` | Q1 | one-line purpose (60) | summary with a "when" and a caveat |
| `superpowers` | Q1, Q4 | one-line purpose (45); no runtime requirements | as above |
| `betterleaks` | Q1, Q4 | one-line purpose (57); no runtime requirements | as above |
| `anthropic-frontend-design` | Q4 | no runtime requirements listed (purpose, pin, licence are fine) | confirm from the marketplace entry whether anything beyond Claude Code is needed |
| `agentic-awesome-skills` | Q2, Q4 | licence is `review-required` (third-party skills carry their own, some GPL) — correct, but not a "known SPDX with evidence"; no runtime requirements | keep `review-required`; a per-skill licence review is the real fix |
| `strix`, `agent-browser` | none | — | (Strix stays quarantined; this is a description audit, not a trust decision) |
| `apify-mcp-server`, `exa-mcp-server`, `microsoft-playwright` (CATALOG-01) | none | — | — |

Also for **all** entries: item M (relevant project situations) has never been reviewed outside the Playwright family — that is a manual pass, not a code change. The nine Phase 0 entries' purposes live in `catalog.rs` (`NINE_META`), not in a data file; improving them means touching that table and, because those summaries are shown to users, an editorial Turkish review.

## 3. Remaining fit and limitations of this phase
* **The suggested default has no declared combination.** CLI + Skills cannot be saved as "kullanmak istiyorum" on any target until a publisher source names an operating system for it (or an independent test exists). That is the correct consequence of the rules, but it will surprise a user who follows the suggestion. Either wait for publisher evidence, or design an *independent check* phase (a harness on synthetic data, no real browser use) — not a relaxation of the rule.
* **The MCP / Claude Code `vendor-declared`** rests on a documented command plus OS-named profile paths; a stricter reviewer may prefer `unknown`. It is one data edit.
* **Node version** disagreement (18 vs the framework's private 20) is disclosed, not resolved. Resolve it from the published npm metadata in a future read-only evidence pass.
* **No discovery (scan) support** for these three: a stored scan says nothing about them. Detection of a `playwright-cli` binary or a Playwright MCP entry in an agent configuration would be a separate, read-only discovery phase.
* **The comparison is Catalog-page only.** The matrix shows the descriptions through the ordinary `purpose`; a family-aware matrix view, and showing the comparison from the Recommendations page, are possible later steps.
* **A shared CLI runtime with per-project isolated sessions** is a proposal only (see `UX_AND_SCOPE.md`); it belongs after the real-installation gates, with explicit approval, never before.
* **jsdom and mocked IPC** are not a real window; the Turkish copy has had no editorial review; keyboard behaviour was tested with `user-event`, not a physical keyboard or screen reader.
* `tools/gui_jq_pilot_locked.mjs` (a Windows GUI harness, not run here) still asserts 14 install panels; with 18 entries it will expect 19 the next time it is run. Left untouched on purpose (Phase 3D tooling).
