# Phase 0 Technical Handoff

**Evidence snapshot:** 2026-10-10. This is a design contract, not an implementation or an authorization to start Phase 1. Tool-specific evidence and limitations are in [TOOL-COMPATIBILITY.md](TOOL-COMPATIBILITY.md) and [RESEARCH-SOURCES.md](RESEARCH-SOURCES.md).

## 1. Product Boundary

Build an AI-provider-independent, local-first desktop manager for reusable developer tools. It inventories, assesses, installs only reviewed/approved recipes, validates observed functionality, tracks per-project preferences, and produces local diagnostics. It does not replace Claude Code or Antigravity, act as an unrestricted package manager, promise token savings, or claim that an audit proves correctness.

Supported product targets for initial design: macOS 13+ ARM64 (the stricter Claude Code minimum; current Antigravity 2.0 lists macOS 12+) and Windows 10 version 1809+ x64 (Claude Code's documented native Windows minimum). Minimums are evidence-based candidates, not yet a user-approved support policy. Do not infer that every catalog tool supports both targets.

## 2. Architecture and Responsibilities

| Component | Responsibility | Hard boundary |
| --- | --- | --- |
| Tauri 2 host (Rust) | Single-instance lifecycle, tray, autostart, OS queries, process supervision, narrowly scoped file edits, verified updater | Native code is fully trusted by the OS. Keep the WebView capability list minimal; no generic shell command API. |
| React + TypeScript WebView | Inventory, review queue, project tool controls, approval/diff screens, reports | Calls typed Rust commands only; never receives secrets or raw unrestricted filesystem/shell access. |
| Rust domain services | Catalog parser, trust policy, state machine, adapter registry, validation orchestrator, redaction, audit event writer, sync engine | Inputs from remote repositories and agents are untrusted data. Separate validation from mutation. |
| SQLite local store | Catalog snapshots, approvals, project policies, local paths, event references, verification history, sync cursor | Store under OS per-user application-data directory. Encrypt or omit secret material; preferred design stores no credentials. Back up before schema migration. |
| Tool adapters | One typed adapter per supported agent/package manager; read status and perform declared operations | An adapter must name exact files/processes it may inspect/change. No arbitrary shell interpolation. |
| GitHub reader | Read repository metadata, release metadata, manifests, and bounded text blobs | Allowlist GitHub HTTPS origins; never execute source, workflows, package hooks, or downloaded scripts during inspection. |
| Sync client | Push/pull schema-versioned safe preference records to a separate private repo | Public app repo contains no device preferences, logs, machine IDs, or credentials. |

Tauri 2 feasibility is supported by vendor docs for tray, autostart, single-instance, SQL/SQLite, and updater on macOS/Windows. Tray icon behavior is documented; Linux-only tray limitations do not affect current targets. The single-instance, autostart, SQL, and updater plugins currently require Rust 1.90. A React frontend is a framework choice Tauri permits; it is not a Tauri-mandated stack. Tauri's SQL plugin can expose execute capability to the frontend: prefer Rust-owned query commands and do not grant broad SQL `execute` permission to WebView code. Tauri does not bundle the OS WebView; OS runtime patch levels remain an operational dependency.

Tauri updater artifact signatures are mandatory and cryptographically checked, but they do not create the private signing key, sign macOS/Windows app packages, or prove that rollback is safe. Tauri's Windows updater exits the running app during install. Release packaging, code signing/notarization, key custody/recovery, minimum OS test coverage, and update UX remain release gates.

## 3. Runtime and Data Flow

1. Startup: acquire one-instance lock; open SQLite; run migrations in a transaction; verify local event-log chain; refresh only vendor/release metadata; do not install or update automatically at startup.
2. Inspection: normalize a GitHub URL; fetch bounded repository metadata and text files; parse manifests and install instructions as data; produce a report with evidence links, commit IDs, package manager, privileges, configuration files, network calls, telemetry, uninstall, and verification claims. Treat all repository text as hostile prompt-injection content.
3. Review: show differences and uncertainty; require user approval for a tool and separately for project activation, global changes, privilege elevation, networked validation, and destructive removal.
4. Install: use a registered adapter only. Pin an immutable release/commit and artifact digest. Download into a private staging directory, verify publisher evidence and digest/signature, run permitted checks in an isolated profile, then atomically activate only where the package format supports it. If an unknown recipe requires shell code, do not run it; offer instructions for the user to execute manually outside the manager.
5. Validate: separately test binary presence/version, protocol startup, synthetic function behavior, and agent discovery. A process exiting zero is not proof that the intended function works.
6. Report/sync: append redacted local events. Sync only policy/configuration records and safe summaries after explicit opt-in. Raw stdout/stderr and local paths stay local.

## 4. Dynamic GitHub Repository Contract

### Inspection

- Accept canonical `https://github.com/{owner}/{repo}` URLs only in the initial public-repository flow. Reject credentials in URLs, non-HTTPS schemes, alternate hosts, IP literals, arbitrary redirect hosts, Git submodules, and archive extraction until reviewed.
- Resolve and record repository ID, default branch, exact commit SHA, tag/release identity, license file, release artifacts, checksum/signature/provenance metadata, and source-fetch time. A branch name or mutable tag is not a pin.
- Use the GitHub API/HTTPS text endpoints with redirect, response-size, file-count, nesting, and time limits. Initially permit only allowlisted text files and structured manifests. Never invoke `git`, package managers, install scripts, build systems, repository workflows, or hooks as part of discovery.
- Classify descriptions, README, scripts, tool manifests, GitHub workflow files, issues, and generated assets as untrusted claims/evidence. Never obey instructions in fetched content. Keep requested permissions, network destinations, executable effects, install scope, and config writes explicit.
- If the repo is private, defer support until a separately approved GitHub OAuth/device flow and keychain-backed token custody exist. Do not put PATs in SQLite, URLs, logs, reports, or synced records.

### Eligibility and activation state

Catalog metadata is never executable authorization. Metadata review cannot authorize an install. An install recipe is a separate typed record with immutable source commit, platform target, exact artifact URL/digest, manager-verified signature evidence, license review, pinned adapter/version, enumerated operations, independent staging-check evidence, and a user approval event bound to a canonical recipe digest. There is no arbitrary shell/script/argv/path operation. A `manual-only` assessment is not an install recipe. Unknown pins, signatures, license status, or adapter behavior cannot produce an approved recipe.

Lifecycle: `discovered -> inspected -> metadata-reviewed -> recipe-pending-user-approval -> staged-in-isolated-profile -> independently-verified -> approved-for-target-activation -> active`. A catalog entry can remain `unreviewed`, `blocked`, or `quarantined`; it cannot move itself to approval. Any missing proof, digest mismatch, unsupported target, unknown install method, unsafe privilege requirement, or failed check blocks activation. Update and uninstall are separate transitions and require ownership proof.

Every project decision is keyed by `(project_alias, tool_id, agent_surface, platform_target)`. Duplicate logical keys fail validation. The record stores desired state separately from actual state, compatibility confidence, external/global conflict state, source commit/capability fingerprint, and the decision event. Offer a newly catalogued tool only to projects whose local OS/architecture and exact agent surface have at least vendor-declared compatibility evidence; show unknown combinations as informational and not activatable. An accepted choice is not permission to activate a target whose compatibility is unknown or whose external conflict is unresolved. A global plugin/config cannot be silently treated as project-isolated. If the host cannot isolate the user’s chosen target or profile, report `conflict`/`unknown`, do not claim enforcement, and defer or ask the user to resolve it.

Suggestion suppression is based on stable identity `(project_alias, tool_id, agent_surface, platform_target)`, not each upstream commit. Persist `rejectionReminderAt` and an optional `resetEventId`; a reset is a user audit event `reset_rejection`. After `reject`, ordinary README changes and version bumps enter update review and do not recreate an installation suggestion. Re-suggest only for a genuinely new tool identity, material capability or compatibility fingerprint change, explicit user reset, or an expired user-configured reminder. `later` suppresses until its reminder; `accepted` is also an update state, not a fresh install prompt. These neutral preference fields may sync; device paths and logs do not.

### Separate user operations

- **Disable for project:** preserve catalog record, installation, and all unrelated settings; change only this project's manager-owned integration.
- **Remove from catalog:** remove the manager's catalog mapping; do not uninstall the software or erase its project files.
- **Uninstall managed component:** remove only files/components whose ownership and original state are proven. Preserve indexes, caches, credentials, shared runtimes, and user-authored configuration unless separately selected and confirmed. Show external package-manager commands without executing when ownership is ambiguous.
- **Rollback:** restore the exact prior verified artifact and manager-owned config snapshot. If migration or package manager cannot reverse a change, state that before approval and do not claim rollback support.

## 5. Response Profile Contract

One and only one project value: `caveman`, `asd-ste100-oriented-technical-english`, or `none`. The schema rejects arrays/multiple values. `none` means the manager adds no response-style instruction; it cannot remove rules installed independently by a person or another tool.

- Caveman is an opt-in natural-language output profile. Its repository describes instructions/plugins separately from CLI telemetry, proxy, compression, browsing and MCP middleware. The profile alone must not install, configure, launch, or route through those optional capabilities.
- ASD-STE100-oriented mode is an original, non-certified style profile for English technical prose only. Turkish prose is unchanged. It must not rephrase source code, identifiers, paths, commands, mathematical notation, exact error messages, literal values, or quoted user data. Do not transform a completed answer mechanically.
- The profile is advisory instructions placed in a manager-owned block. No compatibility contract guarantees every model/host obeys style instructions. The app must not announce certification, compliance, or guaranteed savings.
- Preserve all existing `CLAUDE.md`, `GEMINI.md`, `AGENTS.md`, and host-native instruction contents. Write only exact marker-delimited manager blocks; retain byte-for-byte before-images and hashes. If a block is edited, duplicated, malformed, or conflicts with another active response profile, stop and ask; never replace the whole file.
- ASD-STE100 is available at no cost from its owner but is copyrighted and a registered trademark. Do not bundle, quote at length, reconstruct, or redistribute its specification/dictionary. Seek written permission/legal review before shipping licensed text or a checker. Generic original guidance is not certification.

## 6. Self-Audit and Independent Verification

Keep three distinct layers:

1. **Operational health:** OS/process/version/path, package state, signature/digest, configuration parse, IPC/MCP handshake, freshness, last function-test time, and known agent discovery. Report `unknown` separately from failure.
2. **Independent deterministic verification:** a separately authored test harness and synthetic fixtures check expected behavior (for example, fixed input/output, secret scanner dummy token + redacted output, MCP handshake and tool-schema contract). Verify the result independently of the installer and the target tool's own self-report. Run upstream tests only as supporting evidence.
3. **Optional AI-assisted Red Team:** local-only by default. It proposes attacks/failure hypotheses; it cannot certify, set a green status, or override deterministic evidence. Source/context must not leave device without explicit disclosure and consent.

Use disposable test homes/projects, dummy credentials, denied external networking, bounded CPU/RAM/process/disk/time, and isolated fixture copies. Inject interrupted downloads, wrong hashes/signatures, malformed manifests, permission denial, full disk, locked/active executable, corrupt DB/migration, offline mode, host config edits, symlink swaps, path traversal, sync conflicts, partial report writes, and rollback interruption. Terminate and clean up only inside the test fixture. No failure injection against user environment or production services.

No single check proves complete correctness. A test report states exact tool version, artifact digest, platform, fixture, check implementation/version, observed result, exclusions, and timestamp. Upstream self-reported scores, README examples, “works on my machine,” and an installer success message never count as independent functional verification.

## 7. Security and Threat Model

### Assets and principals

Assets: user code and project instructions; agent settings; credentials; downloaded executables; tool state/indexes; local audit records; private sync repo. Principals: user, manager UI/frontend, Rust core/adapters, GitHub/content publisher, installed tool, AI host/agent, other local process/user, GitHub sync service.

### Primary threats and controls

| Threat | Required control |
| --- | --- |
| Malicious repo content / prompt injection | Static parsing only; treat fetched text as tainted; allowlisted declarative manifest; no auto-run; code review and immutable pin. |
| Compromised maintainer/release/tag | Exact release asset digest + signature/provenance policy; immutable commit pin; independently verify; quarantine changed/moved refs. Checksums served beside artifact alone are not authentic. |
| Command injection / path traversal | No shell string; argv array; canonical path and root confinement; reject `..`, symlinks, device/FIFO paths; race-safe open/replace; least privilege. |
| Tauri WebView privilege escalation | Minimal capabilities/CSP; typed commands; no generic shell or arbitrary file access; validate all IPC; no secrets in UI. |
| Agent instruction/config tampering | Managed markers and ownership ledger; user-visible diff; snapshot/restore; refuse unknown files, user edits, and conflicting managers. |
| Secret exposure in scan output/logs | Synthetic test inputs; redact before persistence; allowlisted structured event schema; raw stdout/stderr local ephemeral only; opt-in export preview. |
| Telemetry/network exfiltration | Per-tool egress inventory; default deny; identify opt-out limits; no Caveman proxy/telemetry by default; user consent for outbound validation. |
| Cross-device data disclosure | Separate private repository, strict schema allowlist, exclude credentials/paths/runtime/indexes/raw reports; client-side preview; least-privileged GitHub token in OS credential store. |
| Races with running agents | Detect relevant processes/files; lock manager-owned operations; stage first. If safe inactivity cannot be established, defer and do not mutate. |
| Log forgery/deletion | Per-user ACL, sequence + hash chain, periodic integrity check, optional signed exports. Same-user admin can rewrite local logs; never call them tamper-proof. |
| False-green self-audit | Independent test harness, exact evidence, `unknown` state, failure injection; AI reviewer cannot approve. |

## 8. Installation, Update, and Rollback Policy

- Identify installer ownership, scope, package manager, dependencies, privileges, declared network destinations, verification check, persistent data, and uninstall procedure before approval.
- Prefer signed immutable vendor packages/package-manager transactions over installer scripts. Do not use `curl | shell`, `irm | iex`, `npx` of moving branches, `uvx` of unpinned versions, `@latest`, or remote package postinstall as an automatic recipe.
- Automatic update policy is user-controlled (`automatic_verified`, `ask`, or `manual`); default is `automatic_verified` only for recipes that meet every gate below. Begin metadata checks after startup settles (default delay 10 minutes), then check at most once per 24 hours per source with jitter/backoff. A check is not an install. Under `automatic_verified`, a compatible update can download to staging and activate only after immutable reviewed source, trusted signature/provenance, digest verification, exact target match, independent staging function tests, reversible/compatible migration, adapter-owned backup, no active dependent session, and user policy all pass. Otherwise require approval or block. Do not mutate at app startup.
- Show a non-intrusive tray update indicator for newly staged/safe updates, a user-action notification for approval-required/risky updates, and a failure notification when rollback/recovery needs attention. Coalesce repeated notices by `(tool, release, reason)`; do not re-notify a rejected same-release install suggestion. Never put a project path, command output, or secret in a notification. User can disable automatic checks, change cadence, or select manual-only.
- Update-source freshness: after three missed daily checks (default 72 hours), label remote release metadata `stale`; stale metadata can be viewed but cannot authorize automatic installation. Offline is an immediate explicit state, not a failed or current check. Retry with bounded exponential backoff and jitter; no rapid polling.
- Auto-update eligibility requires an immutable reviewed source, trusted signed/provenance-backed artifact, digest verified before execution, exact target match, successful independent function tests, backward-compatible or reversible migration, and no active dependent agent session. Any unverified condition requires explicit approval or blocks update.
- Never replace an executable/configuration while an active session may use it. Process state `unknown` is a block, not “inactive.” Prepare staged version first; perform a same-volume atomic switch where supported; retain previous working version/config until post-update smoke test passes. Keep a documented manual recovery path.
- Rollback is per-component and evidence-based. Preserve user data; restore only manager-owned files. Do not claim rollback for irreversible external migrations or package managers that do not preserve old versions.
- App updates use Tauri's signed updater artifacts; private key stays in protected release CI secret storage, not source or sync. OS package signatures/notarization are separate release requirements. Windows self-update exits the app; schedule with an explicit restart notice and no active operation.

## 9. Private GitHub Synchronization Contract

Use a distinct private repository, which is not created in Phase 0. Public repo policy: never commit device configs, project preferences, logs, paths, credentials, generated indexes, local state, private hostnames, or identifiers.

Sync allowlist: schema/version; idempotent event UUID; tool IDs and approved immutable revisions; per-project enabled/disabled/rejected/later decisions scoped to agent/platform; rejection reminder/reset event; one response-profile enum; user-selected neutral project/device aliases; coarse OS/architecture/app-version summaries; preference timestamps, base revision, payload SHA-256, and conflict metadata. Compute the payload digest over canonical UTF-8 JSON (sorted object keys, compact separators) for the allowlisted `payload` object only; verify it before merge and recompute after an accepted merge. Exclude secrets, OAuth tokens, machine/user names, absolute paths, executable locations, command environments/arguments, active session state, raw reports/output, repository source, graph/index DBs, and telemetry.

Use one append-only change record per neutral, user-chosen device alias and a monotonically increasing per-device sequence. An alias must not encode a username, hostname, serial, MAC address, or machine-specific path. Envelope fields: schema version, idempotent event UUID, alias, per-device sequence, base server revision, creation time, coarse OS/architecture/app version, last-successful-sync timestamp, freshness state, and the strict allowlisted project preference payload. It does not include local device IDs, repo URLs, paths, executable state, active sessions, process names, logs, or raw diagnostics. The sync-envelope schema rejects undeclared fields; secret/path scans also run before serialization.

Updates include base revision and content hash. If base revision matches, accept as a fast-forward. If revisions diverge, merge only commutative, stable-ID tool preference operations when changed fields do not overlap. Same-field concurrent edits become visible conflicts; do not use silent last-write-wins. Response profile, agent-target choice, rejection decision, and security policy conflicts require user resolution. A pull never activates a tool or changes a local installation automatically. A synced target preference is applied only to the exact matching local platform and agent surface; a macOS/Antigravity IDE selection is retained but never applied to Windows/Claude Code. Queue offline edits with event UUID and base revision; retry is idempotent. After seven days without successful sync (default), mark the device snapshot stale; show last-seen time and allow the user to keep local-only state. Staleness never grants update/install permission. Expired auth, unknown schema, or failed validation stops sync writes and preserves local state.

Offline: use last verified local snapshot; label it with sync time and stale age; continue local health checks and approved offline tools; queue local changes without claiming remote sync. Stale state never authorizes automatic install/update. Sync retry is idempotent by event ID. Authentication loss or schema incompatibility leaves local state intact and stops writes. User must explicitly pair each local project to a safe shared project alias; do not upload a private Git remote URL as an identifier.

## 10. Agent Integration and Diagnostic Reports

Maintain a capability registry per product surface/version: read-only detection, skill/plugin/MCP install, instruction file path, project/global scope, lifecycle hooks, active-session query, config format, safe merge method, validation path, and uninstall support. Claude Code terminal, Claude Desktop Code tab, Antigravity IDE, Antigravity CLI, and Antigravity 2.0 are distinct surfaces. “Gemini CLI” support is not proof of Antigravity IDE support. The project policy stores decisions per exact surface/platform; it must detect existing global plugins and conflicting instructions before promising a project-scoped profile.

Where documented, use the host's native plugin/MCP manager. For project instructions, add only a managed block to `CLAUDE.md`, `GEMINI.md`, `AGENTS.md`, `.agents/rules/`, or the host's verified global equivalent. Preserve user text and format; detect JSONC/YAML structurally, not with regex. For MCP, redact env values in UI and reports, do not copy host credentials, and verify the server is actually connected and exposes the expected tool set.

### Diagnostic contracts

- Human report: app/platform/version; project alias; tool name/version/digest; state and confidence; last verified time; check results and evidence IDs; exact failure category; actionable next step; data-sharing note.
- Structured NDJSON event: event UUID, chain UUID, segment-start flag, per-segment sequence, UTC timestamp, previous record hash, optional prior segment head, record digest, trace UUID, event type, tool/revision/version, target, adapter/check identity and version, verdict-source enum, outcome, evidence digest, approval reference, stable error code, and a redaction-policy record. Unknown fields and free-form raw command/output/URL values are rejected.
- A `verify/passed` event requires an independent-harness verdict, tool/version, exact target, adapter/check IDs and versions, artifact digest, evidence digest, trace ID, and redaction-policy record. Installer exit code or target self-report cannot satisfy it. An `approve/passed` event includes the canonical recipe digest; the recipe references that event and repeats the digest. `redacted:true` is not proof: the persistence/export boundary must run the redactor, verify the output against the strict allowlist, hash only the redacted payload, and drop the payload on redaction failure. Schema assertions are still claims; tests inspect actual serialized bytes using dummy secrets.
- Local event storage: append-only API, restrictive OS ACL, canonical JSON serialization, SHA-256 record digest excluding only its own digest field, segment/rotation rules, and periodic chain verification. Rotation starts a new segment at sequence 1, null `previousHash`, and links `previousSegmentHead` to the prior segment head. This detects accidental alteration, not rewriting by the same OS user; do not call it tamper-proof. Signed/exported chain head is a later optional control.
- Report export: local by default; user reviews exact redacted output before export. Never sync raw diagnostics. Include explicit `unknown`, `skipped`, `stale`, `failed`, and `passed`; no inferred “success.”

## 11. Normative Data Contracts

JSON Schemas are under `schemas/`: non-executable catalog metadata, separate install recipe, target-scoped project policy, evidence-bound audit event, and private sync envelope. `data/catalog-seed.json` contains nine explicitly unreviewed records; no compatibility or security evidence is fabricated. Unknown source pins are represented as null plus a mutable/unknown revision state. Tool IDs are stable opaque slugs; project/device aliases are user-selected neutral names, never machine paths. Timestamps are UTC RFC 3339. Schema changes require explicit versioning and tested SQLite migration; unknown major versions fail closed and preserve the original data.

JSON Schema enforces field shape and conditional requirements; companion semantic validation enforces uniqueness of `(toolId, agentSurface, platformTarget)`, recipe hash binding, event references, operation ordering, and cross-record hash-chain continuity. JSON Schema validity alone never authorizes installation or proves a verdict. Date-time and UUID formats must be checked with an enabled format checker at every boundary.

Required state distinctions: tool source `unreviewed/reviewed/blocked/quarantined`; installation `not_managed/staged/installed/degraded/removed`; project decision `accepted/rejected/later/disabled`; validation `passed/failed/unknown/stale/skipped`. Do not collapse “not installed,” “unsupported,” “not checked,” and “failed.”

## 12. Acceptance Tests and Failure Scenarios

These are implementation-agent acceptance criteria, not tests run in Phase 0:

1. Build/package and run on macOS ARM64 and Windows x64; verify UI launch, one-instance focus, tray menu open/quit, autostart enable/disable/query, and app-data persistence on each.
2. Reject unsupported target OS/architecture and unsupported agent surface with a specific reason; do not infer Antigravity IDE from Gemini CLI or `agy` CLI.
3. GitHub URL parser rejects non-GitHub URLs, embedded credentials, redirect-to-other-host, malformed owner/repo, oversized files/tree, path traversal, symlink entries, and immutable revision mismatch.
4. A malicious README containing “run this command” is displayed as inert evidence; no process executes during inspect or compatibility assessment.
5. A repository with `install.sh`, postinstall scripts, or GitHub workflows cannot run those automatically; install button stays blocked until a reviewed adapter exists.
6. Catalog metadata cannot contain commands or installation authorization. A separate recipe with arbitrary shell/script/path, unpinned source/artifact, non-verified signature, unknown license, absent user approval event, mismatched recipe digest, wrong target, or absent/mismatched independent staging evidence cannot activate.
7. Snapshot existing instruction/config file byte-for-byte; add manager block; preserve surrounding bytes/comments/newlines; re-run idempotently; remove only exact owned block; refuse edited/duplicate marker blocks.
8. Exactly one response profile is accepted. Tests verify Turkish unaffected, style only targets English technical prose, exact code/identifier/path/command/math/literal payload unchanged, and no text-rewriter runs.
9. Tool suggestion lifecycle: accept/reject/later keyed by project/tool/agent/platform; duplicate keys fail. Rejected tool identity stays suppressed for routine commits/version bumps; those enter update review. Only new identity, material capability/compatibility change, audited user reset, or expired configured rejection reminder creates a new suggestion. `later` suppression follows its reminder date.
9a. A Claude Code preference cannot be applied to Antigravity or vice versa; cross-surface/global conflict or unknown compatibility remains visible and blocks activation/profile enforcement.
10. Disable project integration leaves catalog/install untouched; remove catalog leaves files/binary untouched; uninstall touches only owned data and preserves user config/indexes unless separately consented.
11. Updates fail closed during active or unknown agent session, locked executable, dirty config, unavailable backup, offline state, partial download, or signature failure. Simulated failure after activation restores old config/binary where supported.
12. Offline mode reports cached verification age and can run known local checks; no hidden network request or success claim.
13. Secret scanner tests use dummy secrets; logs, JSONL, UI errors, crash reports, and sync payloads contain no raw dummy value. A scan with actual report output remains local unless user explicitly previews/exports.
14. Telemetry/proxy capability is absent by default for Caveman; enabling a profile does not install a proxy. Outbound calls are observable and require a separate consent policy.
15. Local sync round-trip validates the allowlisted envelope; machine path, host name, repo URL, secret, extra field, and raw diagnostics mutations are rejected. Replay/duplicate UUID, same-field conflict, concurrent profile changes, stale device, offline queue, expired authorization, and unknown future schema preserve local state and expose accurate status. No remote private repository is needed for fixture tests.
16. Log hash-chain tampering, truncation, order changes, sequence gaps, bad rotation head, and clock skew are detected; report says integrity check is not proof against same-user administrative rewriting. A `verify/passed` event without evidence fields or with AI/self-report as verdict source is rejected.
17. Failure injection runs only in disposable homes/projects with networking denied and CPU/memory/process/disk/time limits. Missing sandbox means test is `not run/blocked`, never passed.
18. Health check, independent test, and AI red-team output are separate report sections; AI-only “pass” cannot set verified state.
19. App self-audit covers database open/migration transaction, local event-chain integrity, adapter allowlist, capability manifest, update signature verification, and tray/autostart/one-instance behavior. A failed self-check marks the app degraded and blocks catalog mutation/update.

## 13. Phase 1 Implementation Sequence (Proposal)

1. Freeze accepted product minimum OS versions, Antigravity target surface, data retention, approval UX, and sync pairing model.
2. Prototype only the Tauri lifecycle spike (tray, startup, one-instance, close-to-tray, packaged build) on both target OSes. No tool installers.
3. Implement typed domain states, SQLite schema/migrations, audit event allowlist/redaction, and fake adapters with fixture tests.
4. Implement GitHub metadata-only inspection, immutable evidence snapshots, and hostile-content tests; no arbitrary install executor.
5. Add read-only detection and independent function checks for one low-risk tool on each supported OS; publish verification evidence before enabling installation.
6. Add project instruction preview/managed blocks and preservation/restore test suite for Claude Code first; Antigravity only after vendor-surface tests.
7. Add approved native adapters one at a time. Add sync last, after data classification/schema and conflict tests are accepted.
8. Independent human/security review of threat model, permission surface, installer flows, signing, and logs before any public release.

## 14. Phase 1 Plan and Gates

Current adjudication state: Phase 0 R1 documentation/schema/test corrections are prepared locally and await a second independent Red Team decision. Do not commit, push, modify `main`, install tools, change environments, or start Phase 1 while that decision is pending. Passing these package tests does not constitute independent approval.

If R1 is independently approved, Phase 1 must still not start until the user separately authorizes it. The only proposed first slice is a non-mutating lifecycle/UI prototype with fake tools; no arbitrary GitHub install, real MCP edits, real agent configuration edits, private sync, updater activation, or real projects. Before later implementation beyond that lifecycle prototype, close or formally accept these prerequisites:

- **G1, product scope:** confirm minimum macOS/Windows versions and whether target is Antigravity IDE, Antigravity 2.0, or both; confirm that Antigravity CLI is not a substitute.
- **G2, legal:** decide whether original ASD-style guidance only is acceptable; obtain written permission/legal review before bundling copyrighted standard/dictionary text or claiming certification.
- **G3, safe execution:** choose a supported isolated staging/test design on both operating systems. Do not create a generic privileged shell runner. If OS-enforced sandbox is unavailable, installer remains manual-only.
- **G4, active session invariant:** prove safe process/session detection for each native adapter or adopt fail-closed deferral whenever state is unknown.
- **G5, release trust:** document immutable artifact verification, signing/notarization, update-key custody/recovery and rollback limits.
- **G6, privacy:** approve event retention/export, private repository schema/ownership, auth mechanism and project pairing. No private config repo or credentials in this phase.
- **G7, independent review:** have a reviewer not authoring the implementation review the first prototype and rerun acceptance/security tests.
- **G8, contract tests:** run all schema meta-validation, nine seed validation, adversarial negative fixtures, cross-record validators, and static Markdown/source checks in CI. Preserve actual command output and hashes in the next review manifest.

### Evidence needed to close open assumptions

Run builds/tests on actual macOS ARM64 and Windows x64; install/verify/uninstall one tool per type inside clean disposable user profiles; confirm the exact Antigravity IDE/2.0 config/plugin install surface against current Google docs and the running product; get written ASD-STE100 redistribution guidance if applicable; and verify signed updater/package rollback on each OS. No Phase 0 research statement substitutes for these checks.

## 15. Schedule, Notification, and Monitoring Contract

- Update metadata check: begin after startup settles (default 10 minutes), then at most once per 24 hours per source, with jitter and bounded exponential backoff. User controls automatic checks/cadence; network disabled/offline means no request.
- Update installation: default policy is `automatic_verified` only for a compatible, pinned recipe whose signer, digest, target, independent staging test, migration/rollback path, and inactive-session gate all pass. Otherwise `ask` or `manual`; risky and uncertain releases always require approval. Never update on app startup merely because a release exists.
- Stale update metadata: after 72 hours without a successful check, label remote release state stale. Stale state is view-only and cannot authorize automatic update.
- Tray/notification rules: coalesce by tool/release/reason; show safe availability as a tray indicator; notify for approval-required/risky updates and recovery-required failures. Suppress repeat notices for same rejected install identity/release. Never include paths, commands, scan findings, secrets, or raw errors in OS notifications.
- Device sync freshness: report `offline` while offline; mark a device snapshot stale after seven days without successful sync (default). Preserve local data and never treat stale/unknown freshness as update or activation approval.
- Self-monitoring defaults: emit an in-process heartbeat every 60 seconds; run local operational probes every 5 minutes with a 10-second total deadline and at most one probe per component at a time. If a probe is still running at deadline, cancel it, mark that component `unknown/degraded`, and skip overlapping work. Three consecutive failures or a missed heartbeat for 3 minutes marks the app degraded and raises one coalesced tray/next-launch notice; recovery requires two consecutive successful probes and records the recovery event. No health probe may mutate a project or install/update a tool. The in-process loop can test database, adapter registry, IPC, timer progress, and last successful check only while the process is scheduled. It cannot independently detect its own total process/OS freeze. Phase 0 does not require a second daemon. The next launch must detect a missing/stale heartbeat and show degraded recovery status; an external watchdog is a separate future decision, not implied.