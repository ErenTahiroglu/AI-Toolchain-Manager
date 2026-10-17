-- Migration 2 (Phase 1B): dynamic catalog registry, project registry, per-project preferences.
-- ADDITIVE: no Phase 1A table is altered or dropped. The data copy and the legacy freeze are done
-- by db::migrate_2_data (Rust) in the same transaction. Phase 1B invariants stay enforced in SQL:
-- nothing can be recorded as installed or independently verified, and every user decision is a
-- *desired* state that is stored separately from the (always unknown) actual state.

CREATE TABLE catalog_entries (
    entry_id               TEXT PRIMARY KEY NOT NULL
        CHECK (length(entry_id) BETWEEN 2 AND 64 AND entry_id NOT GLOB '*[^a-z0-9._-]*' AND entry_id GLOB '[a-z0-9]*'),
    kind                   TEXT NOT NULL CHECK (kind IN ('executable_tool', 'agent_plugin', 'skill_collection', 'prompt_source')),
    origin                 TEXT NOT NULL CHECK (origin IN ('seed', 'user')),
    display_name           TEXT NOT NULL CHECK (length(display_name) BETWEEN 1 AND 120),
    purpose                TEXT NOT NULL CHECK (length(purpose) <= 500),
    source_key             TEXT NOT NULL CHECK (length(source_key) BETWEEN 3 AND 300),
    repo_url               TEXT NOT NULL CHECK (repo_url LIKE 'https://github.com/%'),
    source_path            TEXT,
    plugin_identity        TEXT,
    review_state           TEXT NOT NULL CHECK (review_state IN ('unreviewed', 'metadata-reviewed', 'blocked', 'quarantined')),
    -- documented-snapshot: recorded from public documentation, offline. not-fetched: nothing but the link is known.
    metadata_state         TEXT NOT NULL CHECK (metadata_state IN ('documented-snapshot', 'not-fetched')),
    execution_policy       TEXT NOT NULL CHECK (execution_policy IN ('manual-only', 'blocked-until-scoped-authorization')),
    license_state          TEXT NOT NULL CHECK (license_state IN ('known', 'unknown', 'review-required')),
    license_spdx           TEXT,
    -- "enabled" only means: the entry takes part in the catalog UI, project choices and suggestions.
    enabled                INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    -- Phase 1B: the manager installs nothing and verifies nothing.
    install_state          TEXT NOT NULL DEFAULT 'not_managed' CHECK (install_state = 'not_managed'),
    verification_state     TEXT NOT NULL DEFAULT 'unknown' CHECK (verification_state = 'unknown'),
    revision               TEXT NOT NULL,
    capability_fingerprint TEXT NOT NULL CHECK (length(capability_fingerprint) = 64),
    record_json            TEXT NOT NULL,
    added_at               TEXT NOT NULL,
    updated_at             TEXT NOT NULL,
    removed_at             TEXT
);
-- One live entry per source; removing an entry frees the key (the row stays as history).
CREATE UNIQUE INDEX catalog_entries_live_source ON catalog_entries (source_key) WHERE removed_at IS NULL;

CREATE TABLE project_registry (
    project_id   TEXT PRIMARY KEY NOT NULL,
    alias        TEXT NOT NULL UNIQUE CHECK (length(alias) BETWEEN 2 AND 42 AND alias NOT GLOB '*[^a-z0-9-]*'
                                             AND alias GLOB '[a-z0-9]*' AND alias NOT LIKE '%-'),
    display_name TEXT NOT NULL CHECK (length(display_name) BETWEEN 1 AND 80),
    kind         TEXT NOT NULL CHECK (kind IN ('synthetic', 'reference')),
    -- Local-only free text chosen by the user. Never read, scanned or probed; never logged or synced.
    local_note   TEXT NOT NULL DEFAULT '' CHECK (length(local_note) <= 200),
    created_at   TEXT NOT NULL,
    removed_at   TEXT
);

CREATE TABLE project_preferences (
    project_id                TEXT NOT NULL REFERENCES project_registry(project_id),
    entry_id                  TEXT NOT NULL REFERENCES catalog_entries(entry_id),
    agent_surface             TEXT NOT NULL CHECK (agent_surface IN ('claude-code-terminal', 'claude-desktop-code', 'antigravity-ide', 'local-cli')),
    platform_target           TEXT NOT NULL CHECK (platform_target IN ('windows-x64', 'macos-arm64')),
    decision                  TEXT NOT NULL CHECK (decision IN ('accepted', 'rejected', 'postponed', 'disabled')),
    decided_at                TEXT NOT NULL,
    catalog_revision          TEXT NOT NULL,
    capability_fingerprint    TEXT NOT NULL,
    remind_at                 TEXT,
    -- Actual state, deliberately separate from the desired `decision` above. Always unknown in Phase 1B.
    actual_install_state      TEXT NOT NULL DEFAULT 'not_managed' CHECK (actual_install_state = 'not_managed'),
    actual_verification_state TEXT NOT NULL DEFAULT 'unknown' CHECK (actual_verification_state = 'unknown'),
    observed_scope            TEXT NOT NULL DEFAULT 'unknown' CHECK (observed_scope = 'unknown'),
    PRIMARY KEY (project_id, entry_id, agent_surface, platform_target)
);

-- Append-only history: one row for every decision and every explicit reset.
CREATE TABLE preference_history (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id       TEXT NOT NULL,
    entry_id         TEXT NOT NULL,
    agent_surface    TEXT NOT NULL,
    platform_target  TEXT NOT NULL,
    action           TEXT NOT NULL CHECK (action IN ('accepted', 'rejected', 'postponed', 'disabled', 'reset')),
    at               TEXT NOT NULL,
    catalog_revision TEXT NOT NULL
);
CREATE TRIGGER preference_history_no_update BEFORE UPDATE ON preference_history
BEGIN SELECT RAISE(ABORT, 'preference_history is append-only'); END;
CREATE TRIGGER preference_history_no_delete BEFORE DELETE ON preference_history
BEGIN SELECT RAISE(ABORT, 'preference_history is append-only'); END;

-- Which suggestion was already shown for which task (at most once per task and entry).
CREATE TABLE suggestion_log (
    task_id    TEXT NOT NULL CHECK (length(task_id) BETWEEN 1 AND 64),
    entry_id   TEXT NOT NULL,
    project_id TEXT NOT NULL,
    phase      TEXT NOT NULL CHECK (phase IN ('start', 'late')),
    shown_at   TEXT NOT NULL,
    PRIMARY KEY (task_id, entry_id)
);

-- Per-skill identity for skill collections. Bodies of SKILL.md files are never stored here.
CREATE TABLE skill_index (
    entry_id     TEXT NOT NULL REFERENCES catalog_entries(entry_id),
    skill_id     TEXT NOT NULL CHECK (length(skill_id) BETWEEN 2 AND 80 AND skill_id NOT GLOB '*[^a-z0-9._-]*'),
    name         TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 80),
    description  TEXT NOT NULL CHECK (length(description) <= 300),
    tags         TEXT NOT NULL CHECK (length(tags) <= 200),
    source_sha   TEXT NOT NULL CHECK (length(source_sha) = 40),
    license_spdx TEXT,
    PRIMARY KEY (entry_id, skill_id)
);
