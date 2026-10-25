# CONFIG-02A — implementation, tests, limits

## Changed behaviour (all TypeScript; no Rust, IPC, capability, migration or dependency change)

| Path | Change |
|---|---|
| `src/configDeclarations.ts` | The registry holds exactly one reviewed declaration (see `SOURCES_AND_DECLARATION.md`); `ReviewedDeclaration` gains `review.repositoryUrl`, `review.note`, `warnings`. |
| `src/configPlan.ts` | `DRAFT_NOTICE`, `draftNotice`; unknown compatibility no longer blocks a JSON-host text draft; source binding (`source_mismatch`); case-insensitive name collision; equivalence by URL identity (protocol + host + path); the staleness key covers the declaration, its review, its warnings and the catalog pin. |
| `src/components/ConfigPreview.tsx` | Shows the draft notice and the review note. No new control: still no Apply, copy or export. |
| `src/configPlan.test.ts`, `src/ConfigPreview.test.tsx`, `src/configPlanCodex.test.ts` | Fixtures carry a pinned commit; the "registry is empty" and "unknown compatibility blocks" tests were rewritten to the new, narrower rules (explicitly unsupported still blocks). |
| `src/test/exaEntry.ts` (new) | Loads the **real** catalog record from `data/catalog-candidates-c01.json`. |
| `src/configPlanExa.test.ts` (new) | Planner tests against the real record and the shipped registry. |
| `src/ConfigPreviewExa.test.tsx` (new) | End-to-end through the real Projects page. |
| `tests/test_config02a_contract.py` (new), `tests/test_config01_contract.py`, `tests/test_agent01_contract.py` | Registry "empty" assertions became "exactly one entry / no Codex entry"; cross-pin of declaration and catalog JSON; purity and no-IPC checks. |
| `docs/RESEARCH-SOURCES.md`, `README.md`, `docs/config-02a/` | R55, R56; the README no longer says every preview is blocked. |

## Tests (proportionate; no mutation run for this milestone)

- Positive: the real catalog record, through the shipped registry, gives `PREVIEW_ONLY_NOT_APPLIED` with the exact stanza on Windows and macOS for a new file and for pasted JSON; unrelated servers and keys are preserved.
- Refusals: same name (also with a different case), an equivalent URL under another name, duplicate keys, comments, trailing commas, malformed or non-object input, a file whose `mcpServers` is not an object.
- No fallback: Codex gets the TOML result; Antigravity and any other entry get `BLOCKED_INSUFFICIENT_EVIDENCE`; nothing is borrowed from the Claude shape.
- Catalog gates: removed, disabled, quarantined, execution-blocked, unsupported, non-MCP entries cannot produce a draft; unknown compatibility produces the draft plus the constraint.
- Source binding: a different pinned commit or repository URL, a tampered registry entry, or a declaration with a key, header, query string or non-HTTPS URL is rejected.
- Staleness: a change of project, OS, surface, mode, pasted text or catalog pin withdraws a shown preview (UI) / marks it stale (planner).
- UI: the required Turkish sentence is on screen; the page never says installed, verified, connected, accepted, tested; no Apply/copy/export; secrets pasted by the user never appear outside the paste box; rendering is idempotent.
- Contract (Python): the registry entry equals the catalog JSON's pin and repository; the stanza is exactly the documented one; the sources contain no IPC/file/network/storage/process access; the capability file grants nothing new; migration count unchanged.

Regression results are recorded in Issue #38 (counts, skips and CI), not in this repository.

## Limits

- Pure text transformation of text the user pastes. The application reads no project file, writes none, connects to nothing, runs nothing and stores nothing.
- The preview shows what a file would look like. It cannot tell whether Claude Code would load it, whether the endpoint answers, or whether the user should want the service.
- JSON only; the diff is line based on a re-serialised document, so a pasted file's own formatting is not preserved in the "after" text.
- One entry, one host, one server name. There is no mechanism to add another without a new source review.
- The review is the developer's own comparison with the sources. It is **not** an independent verification and must not be described as one.

## Follow-up plan (not authorized; for an independent decision)

A later milestone ("CONFIG-02B") could add a **single-project, permissioned real read and write** of `.mcp.json`. It must not start before the gate in `docs/config-01/FUTURE_WRITE_GATE.md` is met and the owner explicitly authorizes it. Minimum shape, in order: (1) an independent re-check of the declaration and the sources, including a Claude Code version note; (2) explicit selection of exactly one project root by the user, canonicalised, no links or reparse points followed; (3) a read-only mode first (show the real file next to the draft, still no write); (4) a write that is atomic, no-clobber, backed up, rolled back on failure, limited to the `exa` stanza, preceded by a consent screen naming the file and the service; (5) no network or client launch from the application at any step; (6) only after that, a separate decision about any connection test. Issues #14 and #25 remain separate, open, and untouched.
