-- Migration 10 (AGENT-01): the agent surface `codex-cli` becomes a selectable project target. Table rebuild (SQLite cannot alter a CHECK).
--
-- ONLY the CHECK list of `project_preferences.agent_surface` changes: it gains 'codex-cli'. Every column and every row is copied unchanged and in the
-- same order (nothing is re-mapped, re-decided, re-dated or re-fingerprinted); the primary key and all other constraints are identical. A decision stored
-- for an existing surface keeps its surface; no row is created for the new surface; compatibility for it stays "unknown" until publisher evidence exists.
-- `preference_history` (append-only, no CHECK on the surface), `discovery_observations` and the install-plan layer are deliberately NOT touched.
CREATE TABLE project_preferences_v10 (
    project_id                TEXT NOT NULL REFERENCES project_registry(project_id),
    entry_id                  TEXT NOT NULL REFERENCES catalog_entries(entry_id),
    agent_surface             TEXT NOT NULL CHECK (agent_surface IN ('claude-code-terminal', 'claude-desktop-code', 'antigravity-ide', 'codex-cli', 'local-cli')),
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
INSERT INTO project_preferences_v10
    (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint, remind_at,
     actual_install_state, actual_verification_state, observed_scope)
SELECT project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint, remind_at,
       actual_install_state, actual_verification_state, observed_scope
FROM project_preferences
ORDER BY rowid;
DROP TABLE project_preferences;
ALTER TABLE project_preferences_v10 RENAME TO project_preferences;
