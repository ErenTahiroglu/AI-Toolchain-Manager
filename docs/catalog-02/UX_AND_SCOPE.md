# CATALOG-02 — the three options, the summaries, the choice behaviour

## The idea
There is no single Playwright mode that serves every use. The publisher presents three, in three repositories, and the catalog now holds them as three **separate candidate entries**: none is installed, none is run, none is connected, and none is chosen for the user. The Catalog page gets a plain-language comparison, **"Playwright seçenekleri"**, above the card list. It is shown whenever the catalog holds any of the three; it is not a popup, needs no click to read, and does nothing by itself.

| # | Option | Entry id | Fits when | Watch out for |
|---|---|---|---|---|
| 1 | **CLI + Skills** — short commands for a coding agent | `microsoft-playwright-cli` | a coding agent (e.g. Claude Code) does brief browser tasks across many repositories; the publisher's documents point coding agents here and call it lighter on context than MCP (**not measured here**) | sessions can keep cookies and storage (the persistent option writes the profile to disk); a shared install is not a shared or isolated profile; commands can change cookies, run page code, record traces; skills install and commands are not authorised |
| 2 | **Test / Library** — a project's own end-to-end tests | `microsoft-playwright` | you write and maintain browser tests *inside a project*; its test dependencies are pinned to that project, which a machine-wide CLI does not replace | running tests opens browsers, visits sites, writes screenshots/traces/state and may download browser runtimes; test scripts are code; this is not an MCP server |
| 3 | **MCP** — rich, stateful browser control through the agent | `microsoft-playwright-mcp` | the agent should see the page structurally and keep one browser state over long exploratory work (self-healing tests, long autonomous flows); costs more context and tokens than the CLI | needs the agent's MCP configuration (this application writes none); page snapshots go to the agent's model provider; the default profile is persistent on disk; extension mode can reuse a logged-in browser |

The "suggested starting point" mark sits on option 1 only and always with its qualifier: *the publisher's documents show it as the starting point for coding-agent browser tasks; that does not mean it works in every repository, is installed, or has verified compatibility, and nothing enables it*.

## Summaries (catalog `purpose`)
Each of the three entries has a two-to-three-sentence Turkish `purpose`, at most 300 characters: **what it does → when to prefer it over the others → one concrete limitation or privacy caveat**, and that this application installs/runs/connects to nothing. Longer detail lives in the record (`privacy.notes`, `networkClaims`, `dependencies`, per-cell `limitation`), not in the summary, and no README text is copied. No database field was added: `purpose` already exists and is rendered by the catalog card and the project matrix.
Because the CATALOG-01 data file must stay byte-identical, the framework entry's *displayed* purpose is refined by a `purposeRefinements` list in the new data file; the loader applies it, and the ordinary metadata refresh updates older databases. Only `purpose` changes — the record, fingerprint, revision, enabled flag and every decision are untouched, and an entry the user owns is never refined.

## How choosing works
1. **Read** the three options (cautions expand in place).
2. **Optionally pick a target**: project, operating system, agent surface — three labelled fields, **all empty at first**; nothing is defaulted or remembered. The projects come from the existing read-only project list.
3. **"Matriste aç"** (one button per option) navigates to the Project Tool Workspace with that exact project + OS + agent surface + variant, through the same typed handoff the Recommendations page uses; the matrix brings that variant's row into view. If the target is incomplete the button says what is missing, moves focus to the first missing field and opens nothing. If there is no project, it says to add one on the Projects page first. If a variant has no catalog record, it gets **no button and no substitute**.
4. **Decide only in the matrix.** The comparison writes nothing; the matrix remains the one surface where an ordinary preference is saved, by the user's explicit action. Opening is allowed even where "kullanmak istiyorum" is not available — the matrix then explains why, and "Şimdi değil" / "Reddet" remain possible.
5. **More than one option may be wanted.** There is no exclusivity and no remembered selection; each button is independent.

For the chosen target the comparison also prints the publisher's declaration for that exact combination ("Üretici belirtmiş (test edilmedi)" or "Bilinmiyor") and, where the existing rule blocks "accepted", the matrix's own reason. Unknown stays unknown even for the suggested option.

Honest handling of existing records: **removed** (says so; opening still works, the matrix shows an old decision or explains), **disabled** (says how to enable it), **user-owned** (the user added that repository from a link: labelled as theirs, no publisher evidence applied, compatibility unknown), **missing** (stated; no substitute).

## Future idea, not a feature
A shared Playwright CLI runtime could later be reused by several projects while each project keeps **its own preference** and its own isolated browser sessions/profiles. This is an architecture proposal in the UI (an expandable note) and here. It is not installed, does not allow cookie or session sharing between projects, and writes no file; it would need its own phase with explicit approval and the real-installation gates (Issues #14 and #25).

## Accessibility and wording
Labelled fields (`<label for>`), a `fieldset`/`legend`, headings per option, native `<details>` for cautions, real buttons reachable by Tab and activated by Enter, an accessible name that carries the variant **and** the target, `role="status"` messages, visible focus from the existing styles, no icons and no colour-only status. No sentence says installed, works, verified, supported or active about any option; a test pins that. Not covered: a real screen reader, high-zoom and dark-theme review, and editorial review of the Turkish — they need a person with the app open.
