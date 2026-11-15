# AGENT-02 — SQLite migration 0011 and the read-only security scope

## Migration `src-tauri/migrations/0011_agent02_surfaces.sql`
* Append-only: migrations 0001–0010 are byte-identical to the baseline (a contract test checks `git diff`).
* Same table-rebuild convention as migration 0010: create `project_preferences_v11`, copy **all 12 columns of every row in the same order** (`ORDER BY rowid`), drop the old table, rename.
* The only change is the CHECK on `agent_surface`: the existing exact allowlist plus `codex-vscode`, `cline-cli`, `cline-vscode`, `cline-jetbrains`, `kiro-cli`, `kiro-ide`. It is an exact `IN (...)` list (no `LIKE`, `GLOB` or pattern); case, spaces and wildcards are rejected.
* Inspected before writing it, objects **attached to `project_preferences`**: only the automatic SQLite index backing its composite PRIMARY KEY (re-created by the rebuilt table's own PRIMARY KEY clause); **no explicit secondary index, no trigger, no dependent view and no inbound foreign key**. So nothing needs re-creating explicitly.
* **Database-wide**, no trigger is attached to `project_preferences`. Two different inventories exist and must not be mixed up:
  * **SQL-only migration schema** (scripts 0001–0010): five triggers, on other tables (`audit_events`, `preference_history`, `catalog_entries`).
  * **Full runtime-initialised schema**: the application also runs `catalog::migrate_phase1a_data` (Rust), which adds nine more freeze triggers — INSERT, UPDATE and DELETE on each of `tools`, `projects` and `project_tool_decisions`. These also belong to other tables.

  Migration 0011 touches none of these triggers. The application does **not** contain only five triggers overall.
* Not touched: `preference_history` (no surface CHECK), `discovery_observations` (its own four-surface CHECK stays), installation records, every other table, `plan.rs::SURFACES`.
* No row is created for a new surface; existing `codex-cli` decisions keep their surface and every value.
* Registered as the 11th step in `src-tauri/src/db.rs`; each step runs in its own transaction together with the `user_version` bump, so a failing step rolls back atomically. `DB_SCHEMA_TOO_NEW` still makes an older build refuse a newer database, and `open_file` still writes a pre-migration snapshot.
* Downgrade is **not** supported: a database at schema 11 may hold new-surface rows that the schema-10 CHECK would reject; an older binary refuses it instead (by design).

## Hard no-execution boundary
New surfaces are **project-preference targets only**. They are in `projects::SURFACES` (and therefore accepted by `recommend.rs`, which only produces suggestions), and in the catalog schema enum so evidence can be recorded later. They are **not** in:
`plan.rs::SURFACES` (installer plans), `recipe.rs`, `config_txn.rs` (every surface other than `claude-code-terminal` → `WrongSurface`), `managed_fs_mcp.rs`, `install_flow.rs`, `discovery/` (no declaration detector; the discovery CHECK is unchanged), the other schemas (`install-recipe`, `reviewed-recipe`, `project-policy`, `sync-envelope`, `audit-event`).
No Tauri command, capability, dependency, network access, process spawn, credential read, global-config read or runtime claim was added. The Stage 1H Action-pin guard and all workflows are unchanged.

## Residual risks
1. `recommend.rs` accepts the new surfaces as targets (suggestions only; execution gates unchanged).
2. Kiro's documented `autoApprove: ["*"]`, hot reload and in-file credentials are why no preview, read or write exists; any future Kiro adapter needs its own reviewed contract.
3. Codex/Cline/Kiro publisher pages change; rows are dated 2026-11-14 and not re-verified automatically.
4. Capability "documented" is a publisher statement, not a local test.
5. Desktop experiences (Codex desktop, Cline Desktop) are not covered here and will need their own migration — see Issue #54.
