# PRODUCT-07 H — frozen wire and display contract (written before any source edit)

Tracking: [#29](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/29) (stays open for independent review). Deferred, **not** part of this change: F4 non-current ordering → [#36](https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/36).
Authority: the independent reviewer's conditional approval of policy **H** on #29 (decisions D1–D10). Baseline verified before this file was written: `development` = `origin/development` = `b7eff9e483ff6f3e866e93d571c844b54242464f`, `origin/main` = `68eda3f0407490fabccb517dcf822ab4818fc0bf`, no tracked change, no stash, one worktree, one **untracked** file `docs/product-07/PRODUCT_07_POLICY_PROPOSAL.md` (254 lines, SHA-256 `5ed8d6aafacb89f0dd863460662f71afeb76aa457e761c22ab848c0c8c9a38d5`) that is deliberately **not** staged by this work. The commit SHA and the exact-SHA CI result are not written in this file (they are in the hand-off reply and on #29).

REAL_INSTALLATION_NO_GO. REAL_CONFIG_WRITE_NO_GO. No migration, no IPC command, no Tauri capability, no dependency, no scan, no network, no write.

## 1. What changes and what must not

| Item | Contract |
|---|---|
| `RequestVsObservation.status`, `note`, `basis` | **Exactly unchanged** for every input (value, selection, text). `classify_request` selection (`max_by_key((rank, id))`, current-first) is untouched. A frozen verbatim copy of the pre-change function is the test oracle (§7). |
| `RequestVsObservation.origin` | The four existing members `projectRelation`, `surfaceRelation`, `observedAt`, `ageHours` keep their values and selection. **One member is added: `method`.** Therefore the serialized `origin` object is **not** byte-identical to before; the compatibility claim is *"additive field only; every pre-existing key unchanged"* — nothing stronger. |
| New members of `RequestVsObservation` | `declarationDetector` and `declarations` (both always present from the new backend; both optional in TypeScript). |
| Everything else | No new status value (no `mixed` verdict), no queue reason, no count change, no acknowledgement, no refresh, no stored data. Inventory page not changed (D8 deferred). |

## 2. Wire contract (JSON)

```jsonc
// existing members of RequestVsObservation unchanged; two are new:
"origin": { "projectRelation": "...", "surfaceRelation": "...", "observedAt": "...|null", "ageHours": 0|null,
            "method": "path-executable" | "native-install-location" | "legacy-local-install" |
                      "claude-settings-file" | "claude-plugin-declaration" | "claude-mcp-config" | null },
"declarationDetector": {
  "kind": "claude_mcp" | "not_implemented",
  "coverage": null |                                   // null exactly when kind = not_implemented
    { "project": <ScopeCoverage>, "local": <ScopeCoverage>, "global": <ScopeCoverage> }
},
"declarations": [                                       // 0..=3, at most one per scope, stable order project, local, global
  { "scope": "project" | "local" | "global",
    "configuration": "configured" | "not_configured" | "malformed" | "unreadable" | "not_assessed",
    "freshness": "current" | "previous_area_not_checked" | "previous_stale" | "previous_other",
    "observedAt": "<RFC 3339 as stored>" | null,
    "ageHours": <integer >= 0> | null }
]
// ScopeCoverage = "read" | "no_scan" | "platform_not_scanned" | "not_authorized" | "project_not_in_latest_scan" | "no_record"
```

Nothing else crosses the wire from this feature: **no** server name, raw MCP entry, command, argument, URL, token, environment, account name, path, location, evidence/limitation text, fingerprint, version, scan id, row id or confidence. Every string is a member of a closed list above or a validated timestamp.

### 2.1 `origin.method` (D3/D7)
* It is the `method` of **the selected presence winner** (the row `origin_of` already receives), mapped through a closed whitelist of the six detection techniques. The pseudo-methods `no-detector`, `not-documented`, `unsupported-platform` and any unknown/legacy string map to **`null`** (they are not techniques; nothing is guessed).
* It describes *how the winning row was produced* (a file name on PATH; a Claude plugin registry/cache record; …). It is **not** the declaration detector's method, and it proves neither identity nor execution.

### 2.2 `declarationDetector.kind`
* `claude_mcp` iff the request's catalog entry **and** surface are exactly what the existing Claude MCP detector emits for: entry `codebase-memory-mcp` on surface `claude-code-terminal` (`detect.rs` constants `MCP_ENTRY`, `SURFACE_CLAUDE`; exposed through one pure function `detect::has_declaration_detector`).
* `not_implemented` for **every other request** — `codex-cli`, `antigravity-ide`, `claude-desktop-code`, `local-cli`, every CLI-only tool, every plugin entry, every entry without a detector. This is a *statement about the application*, not about the tool: it never means "not configured" and never means "supported".

### 2.3 Eligible rows (declaration readings)
A stored observation is eligible iff **all** hold: `subject == entry`; `agent_surface == requested surface` (**exact**; a `local-cli` row is never eligible); `method == "claude-mcp-config"` (the existing Claude MCP detector — plugin-declaration and settings-file rows are **not** eligible in this phase, per the reviewer); `platform_target == request platform`; scope/alias are consistent: `project` and `local` rows must carry **exactly the request's alias**, `global` rows must carry **no** alias (a row that violates this — only possible by tampering or alias scrubbing — is ignored, never repaired; `unknown` scope is ignored). Other projects' rows can therefore never appear.

### 2.4 One reading per scope; deterministic duplicate rule
Within a scope the reading is the eligible row that is **current if any is current**, then the highest `id`. (The store already keeps one newest row per `(subject, platform, surface, method, scope, alias)`, so duplicates only arise from inconsistent data; the rule is still total and documented.) Output order is the fixed display order **project → local → global**. It is a display order, not a precedence claim; nothing is merged and no cross-scope conclusion is drawn.

### 2.5 `configuration` mapping (closed; unexpected → `not_assessed`)
| Stored `configuration` | Wire | Meaning (and nothing more) |
|---|---|---|
| `configured` | `configured` | the file read at that scope contains an MCP definition whose **name or command file name matches** the catalog tool. *Not* loaded, approved, authenticated, running, active, working, installed or safe. |
| `not_configured` | `not_configured` | the file was read (or does not exist) and **no matching definition** was found **in that scope**. Not "the tool is not configured anywhere". |
| `malformed` | `malformed` | the file is not valid JSON / not the expected shape; nothing can be said for that scope. |
| `unknown` | `unreadable` | the file was not read (size/link/permission skip). |
| anything else (`disabled`, `referenced_not_present`, `not_applicable`, an unknown string) | `not_assessed` | the MCP detector never emits these; a row that carries one is **not trusted to mean anything**. |

### 2.6 `freshness` (derived only from existing fields; no text parsing)
`current` iff the row is `current`. Otherwise: provenance `earlier_scan` and the latest scan did **not** check the area → `previous_area_not_checked`; provenance `latest_scan` and stale (≥ 24 h) → `previous_stale`; everything else non-current → `previous_other`. An old scan is **old even if its configuration is `configured`**: the UI must never present a `previous_*` reading as today's state.

### 2.7 `coverage` — separating the cases the brief lists
Computed per scope, **only when `kind = claude_mcp`**:
| Value | When | Not to be read as |
|---|---|---|
| `read` | a reading for this scope exists in `declarations` (its `freshness` says current or previous) | – |
| `no_scan` | no scan has ever been recorded | "not configured" |
| `platform_not_scanned` | the latest scan was for another platform than the request's | "not configured" |
| `not_authorized` | the latest scan was not authorised to read this area (`project` ← project folder; `global` ← user files; `local` ← both) | "not configured" |
| `project_not_in_latest_scan` | the area was authorised but this project's folder was not the one scanned (one scan covers ≤ 1 project) | "not configured" |
| `no_record` | nothing else explains the missing reading | "not configured" |
**An empty `declarations` array never means "not configured".** Only a *current* reading with `configuration = not_configured` is a confirmed negative for its own scope. The case "detector not implemented" is `kind = not_implemented` (coverage `null`). "File or JSON unreadable" is `malformed`/`unreadable`. "Older observation" is `freshness = previous_*`.

## 3. Display contract (TypeScript/React)
* **Two labelled blocks, never merged.** (a) the existing stored presence result (status label + backend note, **unchanged**) now introduced as "Saklanan varlık sonucu"; (b) a new, separate "Kapsama göre MCP tanımı (yapılandırma dosyası okuması)" block.
* Each reading line states scope (project / local / global in plain words), what was read, current vs previous (with age and the reason class), and the **fixed disclaimer** that a recorded match is not "kurulu/etkin/onaylı/çalışan/doğrulanmış". Missing scopes are described by their `coverage` value in words that never say "tanım yok".
* Coexistence ("program found by name" + "no definition in the project", or the reverse) is explained in one muted sentence: *the two answer different questions and can both be true*. **No red banner, no "çelişki/çatışma/hata", no green badge**, no new status. A `configured` reading is never green.
* `kind = not_implemented` → one sentence naming what is not read (Codex CLI, Antigravity IDE, Claude Desktop, bare `local-cli`, CLI-only tools) and that nothing can be said about MCP definitions there; a `local-cli` machine lookup is described as a machine-level program lookup and is **never** described as Codex-specific.
* D7: where `origin.method = claude-plugin-declaration` the origin block adds: the project/local label only says which settings scope was checked; the presence itself comes from the user-level plugin record/cache and is **not** a project-specific installation. Stored origin values and `note` are untouched.
* Older backend payload (no new members): the new block is simply not rendered and **no coverage claim** is made; the matrix looks exactly as today. A new member with an unknown enum value, wrong type, > 3 entries, a repeated or mis-ordered scope, or a `claude_mcp` detector on a surface other than `claude-code-terminal` → the whole block is "unavailable" (fail closed), never partially trusted.
* Shared read-only component `ScopedConfigNote` is used by the matrix (`ObservationSection`) and — display only, no new reason — by the review queue item. Accessibility: a heading, list semantics, no focus theft, long Turkish text wraps.

## 4. Compatibility matrix
| Backend | Frontend | Result |
|---|---|---|
| new | new | full block |
| new | old | old UI ignores the two new members and `origin.method` (structural typing) — today's behaviour, F1 caveat included |
| old | new | members missing → block not rendered, no claim |
| any | any | `status` vocabulary unchanged ⇒ no `unrecognised` path is triggered |

## 5. Differences from the author's earlier suggestion (D3), per the reviewer's "narrower safe contract" rule
1. Plugin-declaration rows are **no longer** eligible (only the Claude MCP detector).
2. `configuration` enum narrowed to what the MCP detector can emit (`disabled`, `referenced_not_present` dropped; `unknown` renamed `unreadable`; a defensive `not_assessed` added).
3. A `method` field on each reading was dropped (one eligible method).
4. Added, because the reviewer required these distinctions: per-reading `freshness` with a reason class, and the per-scope `coverage` descriptor.
5. `origin.method` limited to the six techniques; pseudo-methods → `null`.

## 6. Data limits stated honestly
* `local` scope reads the project's entry in `~/.claude.json` keyed by a normalised directory string; a differently spelled project key yields `not_configured` for `local` although the user may have an entry. Residual risk, stated in the UI copy as "eşleşen tanım bulunamadı".
* `.mcp.json` approval and disable lists are **not** read (existing detector limitation); `configured` therefore never implies the agent loads it.
* Absence from a scan is not absence from the computer (PATH search is name-only and not exhaustive).

## 7. Test obligations (see `H_IMPLEMENTATION_AND_TESTS.md` for the executed list)
Frozen oracle of the old `classify_request` compared over an exhaustive pair space plus a seeded random space; 0/1/2/3 readings and ordering; duplicate scopes; current vs earlier scans; global-only; other alias; other OS/surface; no detector (Codex, Antigravity, Desktop, `local-cli`, CLI-only); missing authorisation; skipped/malformed files; no rows; future timestamps; MCP presence stays `unknown`; PATH present + project `not_configured` and PATH absent + project `configured` shown together without a verdict; plugin presence from cache not promoted to project; sanitised wire (sentinels never appear); no new command/permission/migration; frontend: validators, old payloads, copy bans, coexistence wording, queue unchanged; mutation checks for the false-wording / stale-as-current / unsupported-as-scanned / guessed method / wrong alias / fallback-as-exact classes.
