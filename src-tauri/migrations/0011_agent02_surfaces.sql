-- Migration 11 (AGENT-02): six more agent surfaces become selectable project targets (read-only / manual preferences only).
-- Table rebuild (SQLite cannot alter a CHECK), same convention as migration 10.
--
-- ONLY the CHECK list of `project_preferences.agent_surface` changes: it gains 'codex-vscode', 'cline-cli', 'cline-vscode', 'cline-jetbrains', 'kiro-cli' and
-- 'kiro-ide'. The list stays an EXACT allowlist (no LIKE, no GLOB, no pattern). Every column and every row is copied unchanged and in the same order (nothing is
-- re-mapped, re-decided, re-dated or re-fingerprinted); the primary key and all other constraints are identical. Existing decisions (including every `codex-cli`
-- decision) keep their surface; no row is created for a new surface; compatibility for it stays "unknown" until publisher evidence exists.
-- Inspected before writing this file, objects attached to `project_preferences`: only the automatic SQLite index that backs its composite PRIMARY KEY (it is
-- re-created by the rebuilt table's own PRIMARY KEY clause); no explicit secondary index, no trigger, no dependent view and no inbound foreign key. None of the
-- triggers in the database is attached to `project_preferences`. SQL-only migration schema (scripts 0001-0010): five triggers, on OTHER tables (`audit_events`,
-- `preference_history`, `catalog_entries`). Full runtime-initialised schema: the application also runs `catalog::migrate_phase1a_data` (Rust), which adds nine more
-- freeze triggers (INSERT, UPDATE, DELETE on each of `tools`, `projects`, `project_tool_decisions`); these also belong to other tables. This rebuild touches none of them.
-- `preference_history` (append-only, no CHECK on the surface), `discovery_observations` (its own
-- four-surface CHECK) and the install-plan layer are deliberately NOT touched.
CREATE TABLE project_preferences_v11 (
    project_id                TEXT NOT NULL REFERENCES project_registry(project_id),
    entry_id                  TEXT NOT NULL REFERENCES catalog_entries(entry_id),
    agent_surface             TEXT NOT NULL CHECK (agent_surface IN ('claude-code-terminal', 'claude-desktop-code', 'antigravity-ide', 'codex-cli', 'codex-vscode', 'cline-cli', 'cline-vscode', 'cline-jetbrains', 'kiro-cli', 'kiro-ide', 'local-cli')),
    platform_target           TEXT NOT NULL CHECK (platform_target IN ('windows-x64', 'macos-arm64')),
    decision                  TEXT NOT NULL CHECK (decision IN ('accepted', 'rejected', 'postponed', 'disabled')),
    decided_at                TEXT NOT NULL,
    catalog_revision          TEXT NOT NULL,
    capability_fingerprint    TEXT NOT NULL,
    remind_at                 TEXT,
    actual_install_state      TEXT NOT NULL DEFAULT 'not_managed' CHECK (actual_install_state = 'not_managed'),
    actual_verification_state TEXT NOT NULL DEFAULT 'unknown' CHECK (actual_verification_state = 'unknown'),
    observed_scope            TEXT NOT NULL DEFAULT 'unknown' CHECK (observed_scope = 'unknown'),
    PRIMARY KEY (project_id, entry_id, agent_surface, platform_target)
);
INSERT INTO project_preferences_v11
    (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint, remind_at,
     actual_install_state, actual_verification_state, observed_scope)
SELECT project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint, remind_at,
       actual_install_state, actual_verification_state, observed_scope
FROM project_preferences
ORDER BY rowid;
DROP TABLE project_preferences;
ALTER TABLE project_preferences_v11 RENAME TO project_preferences;
