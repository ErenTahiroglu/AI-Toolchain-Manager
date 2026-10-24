# CATALOG-03 — risk, privacy and overlap

Not a recommendation of anything. No entry here carries a badge, a ranking or a "use this" signal; every sentence below is a caution or a comparison of what the publishers *say*, so that a person can see what each candidate would touch before any future phase considers it.

## Risk at a glance (publisher claims; nothing was run)
| | Understand Anything | Claude-Mem | Karpathy-inspired guidelines | Ruflo |
|---|---|---|---|---|
| Catalog posture | `unreviewed`, manual-only | **quarantined**, blocked-until-scoped-authorization | `unreviewed`, manual-only | **quarantined**, blocked-until-scoped-authorization |
| What it reads | the whole project (first analysis) | everything the agent does: prompts, commands, tool results, file contents | nothing (text the agent reads) | the workspace and whatever its agents are given |
| What leaves the machine | project content goes to the host agent's model provider; cost grows with project size | observations go to the chosen provider: a hosted observer after sign-in, the user's OpenRouter/Gemini key, or the Anthropic plan; optional cloud sync; a Cowork module streams to cmem.ai | nothing in the tree could send data | agents call model providers; optional federation to other machines; some bundled MCP servers are cloud services |
| What it writes locally | a `.ua` (or legacy `.understand-anything`) data folder in the project; optional update hooks | a local worker, SQLite database and vector index; settings under the user profile; a launcher added to the user Path; Bun/uv installed when missing | nothing by itself (its install routes would write project instructions) | `.claude/`, `.claude-flow/`, `CLAUDE.md`, settings, helpers; hooks; a daemon; workers |
| Hooks / background | PostToolUse (Bash) and SessionStart hooks; an auto-update mode | six lifecycle hooks and a persistent worker | none | hooks, a daemon, 12 background workers, autonomous loops |
| Money | model tokens (heavy first run) | a free trial for the hosted observer, then a plan or your own key | none | model-provider usage; some cloud services need accounts |
| Identity / licence caveat | licence copyright line names an individual while the owner is an organisation (not investigated) | Node 20.12 vs 20.0 documented difference | **canonical owner differs from the owner named in README/manifests; no licence file** | **root package is named `claude-flow`; the README's `ruflo` package unverified**; stale plugin manifest |
| Pipe-to-shell installers described | yes (other platforms) | yes (OpenClaw) | download commands from the former owner | yes (POSIX) |

All four: telemetry unknown (not stated in the snapshots), install state `not_managed`, verification `unknown`, "enabled" means only that the row is visible in the catalog.

## Overlap with what is already in the catalog
Same purpose does **not** mean interchangeable: the entries differ in inputs, outputs, storage, hosts and risk. Descriptions of the existing entries are the catalog's own (their sources were not re-read here).

| Area | New candidate | Already in the catalog | How they relate (what a reader should not assume) |
|---|---|---|---|
| Understanding a code base | **Understand Anything** — a graph and dashboard built by an agent from the project, saved as files | `graphify` (a command tool that builds a knowledge graph from code and documents) and `codebase-memory-mcp` (a local knowledge-graph MCP server for a code base) | Three ways to get "a graph of the code". They differ in who builds it (an LLM pipeline with a static-analysis layer vs a command tool vs a server), where it lives (files in the project vs elsewhere), and what it costs (model tokens). Using more than one would mean several graphs and several write locations. Do not treat any as a substitute for another, and none as proof the code base is understood correctly. |
| Remembering across sessions | **Claude-Mem** — records what the agent does and replays a summary later | `codebase-memory-mcp` (memory *about the code*, not about sessions); Ruflo (below) has its own memory modules | Different memories: code structure vs session history vs swarm memory. Two session-memory systems each hooking the same events would write and inject overlapping context. |
| How the agent should behave | **Karpathy-inspired guidelines** — four principles as a `CLAUDE.md`/skill | `superpowers` (workflow skills that add working methods), `caveman` (a terse reply style) | All three change the *instructions* the agent follows and can pull in different directions (for example "ask before assuming" and "surgical changes" versus a method that plans and edits broadly, or a reply style that suppresses explanations). Which instruction wins depends on the host's and the project's priority order, which this application does not read or change. |
| Coordinating many agents | **Ruflo** — swarms, hooks, workers, federation | `superpowers` (single-agent working methods), `claude-mem` (memory), Understand Anything (analysis) | A harness that wants to own the agent's loop, hooks and memory overlaps with every other hook-installing plugin: the same events (session start, tool use) would run several handlers in an order nobody here has examined. It also overlaps with what this application may itself one day do (an agent-configuration and orchestration model), which is a design question for that future phase, not something a catalog row decides. |

## Combinations that deserve a stop sign before anyone installs anything
* **Claude-Mem + Ruflo** — two systems that capture sessions, inject context and run background processes on the same hook events.
* **Understand Anything + Claude-Mem** — the first reads the whole project through the model; the second then records what that run did (including file contents) and may send it to a provider.
* **Any hook-installing plugin + a second one** — hook ordering, duplicate prompts and doubled cost are unexamined; the application cannot yet show which hooks an agent has.
* **The Karpathy-inspired text + superpowers/caveman** — instruction conflicts, with the winner decided outside this application.

## What the card text does and does not say
The four summaries (100–300 characters, 2–3 sentences, Turkish) say what the thing does, when it fits, one concrete risk, and that this application installs and runs nothing; the two quarantined ones say "karantinada". They contain no URL, command or README text, no badge and no recommendation. The Karpathy-inspired summary names it as "Karpathy’den esinlenmiş, üçüncü taraf" and states the licence cannot be verified from a separate file. Longer cautions live in the record (`privacy.notes`, `networkClaims`, `dependencies`, per-cell limitations), which the card shows under "Sınırlamalar ve bilinmeyenler" and "Platform ve ajan uyumluluğu".

## Explicit exception to the description-quality gate (CATALOG-02, Q1–Q6)
`multica-ai-andrej-karpathy-skills` fails Q2 (a known SPDX licence with evidence) **on purpose**: the pinned tree contains no licence file, so any SPDX identifier would be a fabrication. It is pinned in the gate's documented-gap list with that reason; the other three pass every machine-checked criterion. Relevant project situations (item M) are covered in the summaries but, as for every entry, need a human reading.
