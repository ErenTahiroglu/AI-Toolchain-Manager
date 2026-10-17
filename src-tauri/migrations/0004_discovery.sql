-- Migration 4 (Phase 2A): local, read-only discovery observations. Purely additive.
--
-- These rows record what a scan SAW on this computer. They are machine-specific and local only (never
-- synced, never committed). They are deliberately separate from user preferences: a decision to enable a
-- tool is not evidence that it exists, and an observation never changes a decision.
-- Nothing here can record that functionality was tested or verified: no executable is ever run in Phase 2A.

CREATE TABLE discovery_scans (
    scan_id                TEXT PRIMARY KEY NOT NULL,
    started_at             TEXT NOT NULL,
    finished_at            TEXT NOT NULL,
    platform_target        TEXT NOT NULL CHECK (platform_target IN ('windows-x64', 'macos-arm64', 'unsupported')),
    -- Which kinds of look the user authorised. Paths are never stored.
    authorized_path        INTEGER NOT NULL CHECK (authorized_path IN (0, 1)),
    authorized_user_config INTEGER NOT NULL CHECK (authorized_user_config IN (0, 1)),
    authorized_project     INTEGER NOT NULL CHECK (authorized_project IN (0, 1)),
    status                 TEXT NOT NULL CHECK (status IN ('completed', 'failed')),
    observation_count      INTEGER NOT NULL CHECK (observation_count >= 0)
);

CREATE TABLE discovery_observations (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_id             TEXT NOT NULL REFERENCES discovery_scans(scan_id) ON DELETE CASCADE,
    subject             TEXT NOT NULL CHECK (length(subject) BETWEEN 2 AND 80),
    platform_target     TEXT NOT NULL CHECK (platform_target IN ('windows-x64', 'macos-arm64', 'unsupported')),
    agent_surface       TEXT NOT NULL CHECK (agent_surface IN ('claude-code-terminal', 'claude-desktop-code', 'antigravity-ide', 'local-cli')),
    method              TEXT NOT NULL CHECK (method IN ('path-executable', 'native-install-location', 'legacy-local-install', 'claude-settings-file',
                                                       'claude-plugin-declaration', 'claude-mcp-config', 'no-detector', 'not-documented', 'unsupported-platform')),
    presence            TEXT NOT NULL CHECK (presence IN ('present', 'absent', 'unknown', 'unsupported')),
    configuration       TEXT NOT NULL CHECK (configuration IN ('configured', 'disabled', 'not_configured', 'referenced_not_present', 'malformed', 'not_applicable', 'unknown')),
    version             TEXT CHECK (version IS NULL OR length(version) BETWEEN 1 AND 64),
    version_source      TEXT CHECK (version_source IS NULL OR length(version_source) <= 80),
    scope               TEXT NOT NULL CHECK (scope IN ('global', 'project', 'local', 'unknown')),
    project_alias       TEXT,
    confidence          TEXT NOT NULL CHECK (confidence IN ('high', 'medium', 'low', 'none')),
    evidence            TEXT NOT NULL CHECK (length(evidence) <= 420),
    location            TEXT CHECK (location IS NULL OR length(location) <= 300),
    limitations_json    TEXT NOT NULL,
    identity_verified   INTEGER NOT NULL CHECK (identity_verified IN (0, 1)),
    -- Phase 2A invariant: observing is not testing.
    functionality_tested INTEGER NOT NULL DEFAULT 0 CHECK (functionality_tested = 0),
    observed_at         TEXT NOT NULL
);
CREATE INDEX discovery_observations_key ON discovery_observations (subject, platform_target, agent_surface, method, scope, project_alias, id);
