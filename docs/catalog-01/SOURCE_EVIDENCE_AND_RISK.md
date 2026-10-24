# CATALOG-01 — source evidence, licensing and risk

All facts below were read **read-only** on 2026-10-24 from the three repositories at the pinned commits (the GitHub API, `repos/{owner}/{repo}/contents/{path}?ref={sha}`; each commit existed and was still the head of its default branch when read). Evidence ids R35–R37 are registered in `docs/RESEARCH-SOURCES.md`. Nothing was installed, executed, cloned or downloaded beyond the text of those files, and neither hosted endpoint was contacted. Everything is the **publisher's description**: not a verified installation, not a vendor-identity attestation, not permission to activate anything.

## Source references
| Id | Repository @ commit | Files read | What was taken from them |
|---|---|---|---|
| R35 | `microsoft/playwright` @ `91a765d1681cb3eeb0d0fb89fefec3cf0d21bd9b` (2026-10-24T17:04:31Z) | `LICENSE`, `README.md`, `package.json` | Apache License 2.0 (GitHub: `Apache-2.0`). README: framework for web automation and testing; separate offerings (Playwright Test, CLI, MCP, library, VS Code extension); Cross-Browser table (Linux / macOS / Windows). `package.json` is the private monorepo manifest (`playwright-internal`, `1.65.0-next`, Node `>=20`) — not treated as an end-user requirement. No telemetry statement found. |
| R36 | `apify/apify-mcp-server` @ `cdf5b23230b26cd42e5d0db81528365f6b9cdc9c` (2026-10-24T05:37:10Z) | `LICENSE.md`, `README.md`, `package.json`, `manifest.json`, `server.json` | MIT. Package `@apify/actors-mcp-server` 0.19.4, Node `>=22.0.0`, `bin` `actors-mcp-server`. `manifest.json`: platforms darwin / win32 / linux, token required and sensitive. `server.json`: remote Streamable HTTP `https://mcp.apify.com/`. README: Claude Code / Claude.ai / Cursor / VS Code compatibility; discovery calls free, Actor runs variable-cost, several payment routes; telemetry default on with opt-out; Sentry on stdio; hosted features absent locally. |
| R37 | `exa-labs/exa-mcp-server` @ `0b499f095c4ca145eee07b09137641c037d58449` (2026-10-19T19:23:36Z) | `LICENSE`, `README.md`, `package.json` | MIT. Package `exa-mcp-server` 3.4.2, Node `>=20.0.0`, `bin` `exa-mcp-server`. README: hosted Streamable HTTP `https://mcp.exa.ai/mcp`, anonymous with rate limits, sign-in or API key for more (key may be passed in the URL — never done here), Claude plugin route, skills `search` and `exa-agent`, client table incl. "Google Antigravity" (manual `mcp_config.json`). No OS list, no telemetry statement. |

Not read (and not claimed): the `microsoft/playwright-cli` and `microsoft/playwright-mcp` repositories, the Apify and Exa web sites, terms of service and privacy policies, the Claude plugin marketplace, npm registry metadata, any release or tag, any workflow or source code of the three repositories beyond the files listed. Licence evidence is the repository licence file only; third-party components inside each tool were not examined.

## Risk and licensing table
| | Microsoft Playwright | Apify MCP Server | Exa MCP Server |
|---|---|---|---|
| Licence (repository file) | Apache-2.0 | MIT | MIT |
| What it is | Browser automation / E2E testing framework (library + test runner); **not** an MCP server | MCP service exposing Apify Actors (hosted + local stdio package) | MCP service for web search / fetch / research (hosted + local npm package); also an agent plugin with skills |
| Network / data flow | Loads any site a script points at (including local or private ones); downloads browser runtimes in the publisher's setup step | Actors fetch third-party sites for you and send Actor inputs to Apify; hosted endpoint informational only | Queries and URLs go to Exa; anonymous use is rate-limited; hosted endpoint informational only |
| `mayTransmitProjectContent` | unknown (`null`) | **true** | **true** |
| Telemetry | unknown | opt-out (default on, publisher) | unknown |
| Cost | none stated | **variable, paid compute**; payment routes exist; none authorised | paid features / credits may apply; Exa Agent needs sign-in or key; none authorised |
| Credentials | none for the framework itself; saved sign-in state / cookies / traces can be sensitive | token or sign-in needed; **never in a URL, config, log or test file** | optional sign-in or key; **never in a URL** (the README shows a URL parameter — do not use it) |
| Local side effects | launches real browsers; writes results, screenshots, traces, storage state | stdio package runs as a local process with the user's token | local npm package runs as a local process |
| Privilege | current user; controls real browsers | acts with the permissions of the connected account; the full tool set can create / change / delete Actors and stored data | sends your prompts / queries to a third party |
| Execution policy here | `manual-only` (nothing in the app can run it) | `manual-only` | `manual-only` |

## What this entry is NOT
Not a recommendation of any of the three; not a statement that they are safe, current, maintained, free, private or compatible with a given agent; not proof of the publisher's identity (an `apify` or `microsoft` account name is not an attestation); not an install recipe (publisher methods are recorded as labels only — `unknown`, plus the Claude plugin identifier for Exa, which was not checked against the marketplace); not a connection or an account. Hosted-service behaviour may differ from the open-source local package (the Apify README says so explicitly).
