-- Migration 7 (Phase 2B Red Team closure): identity taxonomy. Table rebuild (SQLite cannot alter a CHECK).
--
-- Adds the state `record_corroborated` ("a locally readable installer record names the catalog-pinned source and the
-- files are present"; user-writable metadata, NOT authenticated identity) and keeps `validated` as a reserved state for
-- evidence bound to the installed artifact. A Rust step that runs after this script lowers every high-trust value that
-- earlier builds stored. All columns and all rows are copied unchanged; ids are preserved.
CREATE TABLE discovery_observations_v7 (
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
    functionality_tested INTEGER NOT NULL DEFAULT 0 CHECK (functionality_tested = 0),
    observed_at         TEXT NOT NULL,
    record_version      INTEGER NOT NULL DEFAULT 1 CHECK (record_version IN (1, 2)),
    identity_state      TEXT NOT NULL DEFAULT 'not_assessed'
        CHECK (identity_state IN ('not_assessed', 'unknown', 'evidence_available', 'record_corroborated', 'validated', 'conflicted')),
    version_state       TEXT NOT NULL DEFAULT 'unknown' CHECK (version_state IN ('unknown', 'known', 'conflicted')),
    provenance_json     TEXT NOT NULL DEFAULT '{}'
);
INSERT INTO discovery_observations_v7
    (id, scan_id, subject, platform_target, agent_surface, method, presence, configuration, version, version_source, scope, project_alias,
     confidence, evidence, location, limitations_json, identity_verified, functionality_tested, observed_at, record_version,
     identity_state, version_state, provenance_json)
SELECT id, scan_id, subject, platform_target, agent_surface, method, presence, configuration, version, version_source, scope, project_alias,
       confidence, evidence, location, limitations_json, identity_verified, functionality_tested, observed_at, record_version,
       identity_state, version_state, provenance_json
FROM discovery_observations;
DROP TABLE discovery_observations;
ALTER TABLE discovery_observations_v7 RENAME TO discovery_observations;
CREATE INDEX discovery_observations_key ON discovery_observations (subject, platform_target, agent_surface, method, scope, project_alias, id);
