# ADR-0001 — Prompt Intelligence: Reference Libraries, Comparison & Safe Augmentation

**Status:** APPROVED PRODUCT DIRECTION — implementation gated to Phase 4P  
**Date:** 2026-10-17  
**Branch policy:** `main` stable / `development` active; no phase-specific branches  
**Related:** [Issue #1](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/1)

## Product objective
At the start of each substantial agent assignment, optionally help a coding agent improve a user-authored task prompt with relevant parts of curated prompt references. **Quality first, efficiency second**: aim for at least the same observable task quality with lower token usage where possible, or clearly better quality at acceptable cost. No library guarantees either improvement. Preserve the user's original objectives, constraints, safety boundaries and approval requirements. Record both quality and token/time outcomes when comparable evidence is available.

## Architectural boundary
- `ExecutableTool`: installable CLI, agent plugin, skill, MCP runtime, etc. Governed by the existing installation contracts.
- `PromptSource`: read-only source of data, such as CSV prompts, YAML prompt records, or Markdown task patterns. **Never** infer executable installation permission from source registration.
- `PromptCandidate`: a bounded, provenance-stamped, untrusted excerpt from a source.
- `PromptDecision`: keep the original, minimally augment, or ask for user review; includes transparent reasons and limitations.
- `PromptEvaluation`: optional post-task assessment using task-specific acceptance tests. Only this stage can provide evidence of actual output improvement.

## Initial sources and treatment
| Source | Initial ingestion | Notes |
| --- | --- | --- |
| [f/prompts.chat](https://github.com/f/prompts.chat) | Read-only public prompt CSV/Markdown snapshots | Source README differentiates prompt/data CC0 from website code MIT. Content quality is variable; no full web service deployment required. |
| [JuliusBrussee/the-prompt-library](https://github.com/JuliusBrussee/the-prompt-library) | Read-only validated YAML prompt records | Repository MIT. Exclude potentially third-party/proprietary system-prompt copies unless their rights and suitability are independently verified. |
| [danielmiessler/Fabric](https://github.com/danielmiessler/Fabric) | Read-only `data/patterns` text snapshots | Repository MIT; Fabric runtime, providers and API keys are not required for reference retrieval. |

Each imported source must be version-pinned and recorded with repository, path, commit SHA, source license, content digest, timestamp and explicit trust status.

## Safe workflow
1. **USER DECISION APPROVED:** At the start of a **new substantial agent assignment** (implementation phase, design, security audit or complex research task), run at most one local, token-free preflight when this project feature is enabled. Never trigger on each chat message, tool call or agent substep. Also offer a manual trigger. Default to no automatically applied change; only show a draft/recommendation until bounded augmentation policy is independently validated.
2. Retrieve up to a small configurable number of relevant *read-only* candidates using local deterministic search, without calling a hosted LLM by default.
3. Apply non-negotiable gates: user objective and constraints preserved, no untrusted executable instructions, no new privilege, no secret exfiltration, legally reusable content, compatible output shape.
4. Compare original vs candidate **as a design heuristic**, not as a factual quality measurement. Suggested weights: constraint preservation 25, task relevance 25, verifiability 20, security/reliability 15, token/time economy 10, clarity 5. Require minimum quality and hard safety gates first; optimize token use **only without an unacceptable quality regression**. Track prompt-size savings separately from actual agent-session input/output tokens and wall-clock/cost. The weights require empirical calibration; irrelevant candidates are rejected regardless of total.
5. Default to the user's original prompt unless an identifiable, minimal, non-conflicting addition helps. Offer a before/after explanation and source attribution. Never replace the whole original prompt autonomously.
6. Optional opt-in A/B evaluation runs comparable tasks with original and augmented instructions under the same model/settings and tests actual results against task-specific expectations. These tests consume tokens and require explicit budget approval.

## Security, privacy and behavior
- Treat all imported prompt text as **untrusted data**, never as `system`, developer, administrator, policy or tool-authorization instructions.
- No automatic execution of scripts, workflows, GitHub Actions, prompt-provided commands or external endpoints.
- No remote prompt search or transmission of proprietary prompts/source code without explicit approval.
- Never upload private prompts, agent transcripts or local project files to the public application repository.
- Enforce context-size caps and deduplicate near-identical candidates; cache offline locally.
- Library ingestion is **separate** from project tool-install prompts, and can be enabled/disabled/deleted from the UI.
- Caveman / ASD-STE100-oriented English / None response style remains a separate exclusive setting; it is not a reference-source ranking factor.
- Provide source provenance and auditable decision records, redacted where necessary.

## Phase gates
- **Phase 1A:** Tauri desktop shell, local foundation and synthetic UI only. This feature may appear as a nonfunctional placeholder; no automatic lookup, indexing or agent prompt rewriting.
- **Future catalog/contracts phase:** Read-only source type, manifest schema, provenance, cached snapshots and local deterministic search under test.
- **Phase 4P (after agent integration):** Controlled Prompt Intelligence UI, retrieval, candidate comparisons, user review, optional minimal augmentation and evaluation. Do not silently hook every agent conversation.
- **Final Red Team gate:** Injection and data-exfiltration simulations, unrelated/low-quality prompt regression, source update and license changes, reproducibility, offline behavior, output-quality tests, latency/token accounting.

## Resolved and future-gated decisions
1. **Resolved:** Trigger once per new substantial agent assignment (not every chat/tool event) and support manual invocation.
2. **Safe Phase 4P default:** show the candidate and minimally augmented prompt for user review; no silent prompt replacement. Automated augmentation can be considered only after adversarial evaluation and explicit later authorization.
3. **Safe default:** optional model-based A/B trials remain disabled unless user separately approves the budget and actual task test plan.
4. **Goal:** improved observable task quality and/or lower total agent token consumption with non-regression in correctness, safety and scope. A shorter prompt alone is not a pass.
5. Do not start this module in Phase 1A, which remains only the desktop shell and mocked local workflows.