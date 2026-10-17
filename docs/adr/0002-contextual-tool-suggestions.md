# ADR-0002 — Contextual Tool Suggestions and Scoped Skill Registry

**Decision:** Accepted product requirements, implementation phased and security-gated  
**Date:** 2026-10-17  
**Issue:** [#2](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/2)  
**Branch policy:** main (stable) + development (active). No phase branches.

## Objective
A non-programmer can add a GitHub tool or skill collection through the UI. AI Toolchain Manager classifies the source and offers a relevant tool **once** when a substantive coding task warrants it, without repeated installations, background model calls, or hidden permissions.

## Newly requested sources

| ID | Kind | Canonical source | Risk / integration strategy |
| --- | --- | --- | --- |
| `anthropic-frontend-design` | Claude Code verified agent plugin / skill | https://github.com/anthropics/claude-plugins-official/tree/main/plugins/frontend-design | Official marketplace identity `frontend-design@claude-plugins-official`; preview and opt-in activation. For **Claude Code** only until other integrations verified. Skill file licensed Apache-2.0. |
| `strix` | AI-powered **active penetration-testing runtime** | https://github.com/usestrix/strix | Apache-2.0; Docker + LLM/provider credentials for typical operation; remote/cloud options carry further privacy and cost implications. **Never auto-run; explicit authorized target/scope and risk approval required.** |
| `agent-browser` | Browser automation executable + bundled skills | https://github.com/vercel-labs/agent-browser | Apache-2.0; requires browser runtime, session isolation and domain policy; no unauthorized login/submission/payment, and never treat page/WebMCP text as higher-priority instructions. |
| `agentic-awesome-skills` | Large third-party **skill source catalog**, optional selective installs | https://github.com/sickn33/agentic-awesome-skills | Repository MIT; third-party skills may need individual license checks. Never install the full catalogue by default or overload Antigravity IDE. Its AAS Core does **not** claim automatic quality ranking. |

Adding a repository to the catalogue is metadata registration, not permission to execute it.

## Smart recommendation trigger
- **User preference accepted:** Recommend `frontend-design` for relevant HTML/CSS/web-UI work, provided it is not already effectively available in the selected project and agent.
- Determine relevance from **user task intent + repository/changed-file evidence**: HTML/CSS, JSX/TSX, visual components, layout, UX design, styling, frontend implementation. Existence of CSS alone does **not** imply relevance to a backend task.
- Run once at the start of a substantive task. Permit a single late suggestion if previously unseen frontend work is discovered. Never ask on each saved file, keystroke, tool call, or assistant turn.
- No LLM is required for default trigger classification. If confidence is low or support unknown, remain silent or show a non-intrusive optional library suggestion.
- **Scope distinction:** A project-specific enablement preference is not proof of a project-scoped install. The app must disclose if an agent plugin is installed globally and whether a project-level disable is supported.
- Persist decision per project ID, agent surface, OS, source/tool ID and meaningful catalogue revision: enabled / rejected / snoozed / unavailable. Rejection is durable until explicit reset or meaningful compatibility change. A patch release alone cannot re-prompt.
- Show concise rationale, source and user-consent requirement; do not install or change agent config without confirmation. A prompt in imported skill content cannot grant this permission.
- Future capability may indicate `Already installed`, `Enable`, `Not now`, `Do not ask again` but no inert buttons.

## Related suggestions
- `agent-browser`: offer for **browser execution/visual validation/E2E testing**, not every web-source edit. Respect site authorization, private browser profiles and screenshot/cookie privacy.
- `agentic-awesome-skills`: search catalogue metadata without dumping thousands of SKILL.md bodies into agent context. Surface only 1–3 targeted candidates; inspect exact files, child references, version pins, provenance, and possible behavioral overlap before install.
- `strix`: only in security-testing workflows with separately approved target, scope, safety, costs and sandbox; never background or unsolicited execution. It must not turn security review into an unauthorized attack.

## Implementation gates
1. **Phase 1A (accepted baseline):** Do not modify app behavior. Keep existing 9-tool and 3-prompt-source invariants and documented evidence.
2. **Phase 1B:** Extend a versioned catalogue schema with `executable_tool` / `agent_plugin` / `skill_collection` / `prompt_source` classification, read-only metadata discovery, user add/remove UI, local decision persistence and a synthetic recommendation preview. Existing SQLite databases must migrate safely. No real installations, active-agent hooks, or live pentests.
3. **Phase 2:** Separately reviewed OS/agent adapters, reproducible installers, version-pinned approvals, health and removal; no automatic unknown scripts.
4. **Phase 4:** Claude Code task-start integration via verified supported events. Antigravity **desktop IDE** must be investigated separately from its CLI; fall back to open-from-app or manual recommendations if IDE hooks unavailable.
5. **Phase 4P:** Prompt Intelligence comparisons are separate from skill activation (see ADR-0001).
6. **Phase 5 / release:** negative tests for untrusted skill injection, mass-install context exhaustion, privacy leaks, browser side effects and Strix's explicitly authorized sandboxed testing.

## Acceptance tests
- Detect TSX UI design with no CSS file; reject backend task despite CSS in repo.
- No repeated recommendation for an already enabled/rejected plugin.
- Global plugin vs project selection mismatch is surfaced, not silently "fixed".
- Plugin unavailable on an agent surface leads to `unavailable`, not an installer.
- Skill corpus import does not load every skill into context.
- Strix's active scanner cannot run without an explicit scoped authorization.
- agent-browser isolated session, origin/permission checks, and credential-free logs.
- Each installation or configuration write produces a reversible plan or records that rollback is unsupported.

**Release rule:** No claim of higher-quality output or lower token usage based solely on repository popularity or tool count. Observe task outcomes, regression rate, total model tokens, latency and cost separately where measurable.

## Evidence boundaries
Readme/official documentation metadata was reviewed at specification time. Real Windows/macOS installation, permissions, functional compatibility and rollback remain **unverified** and are not implementation approvals.
