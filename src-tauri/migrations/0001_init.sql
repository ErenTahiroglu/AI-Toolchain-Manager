-- Migration 1 (Phase 1A). Applied inside a transaction by db::migrate.
-- Phase 1A invariants are enforced by CHECK constraints so that no code path can
-- record a real installation, a real project, or a verified tool by accident.
-- A later, explicitly reviewed migration must relax them.

CREATE TABLE settings (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE tools (
    tool_id          TEXT PRIMARY KEY NOT NULL,
    display_name     TEXT NOT NULL,
    category         TEXT NOT NULL,
    repo_url         TEXT NOT NULL,
    review_state     TEXT NOT NULL CHECK (review_state IN ('unreviewed', 'metadata-reviewed', 'blocked', 'quarantined')),
    -- Phase 1A: the manager never installs anything.
    install_state    TEXT NOT NULL DEFAULT 'not_managed' CHECK (install_state = 'not_managed'),
    -- Phase 1A: nothing has been independently validated.
    validation_state TEXT NOT NULL DEFAULT 'unknown' CHECK (validation_state = 'unknown'),
    record_json      TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

CREATE TABLE projects (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    alias        TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    description  TEXT NOT NULL,
    -- Phase 1A: only clearly synthetic sample projects may exist.
    is_synthetic INTEGER NOT NULL CHECK (is_synthetic = 1),
    created_at   TEXT NOT NULL
);

CREATE TABLE project_tool_decisions (
    project_id      INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    tool_id         TEXT NOT NULL REFERENCES tools(tool_id),
    agent_surface   TEXT NOT NULL,
    platform_target TEXT NOT NULL,
    decision        TEXT NOT NULL CHECK (decision IN ('accepted', 'rejected', 'later', 'disabled')),
    actual_state    TEXT NOT NULL DEFAULT 'unknown' CHECK (actual_state = 'unknown'),
    UNIQUE (project_id, tool_id, agent_surface, platform_target)
);

CREATE TABLE prompt_sources (
    source_id      TEXT PRIMARY KEY NOT NULL,
    display_name   TEXT NOT NULL,
    repo_url       TEXT NOT NULL,
    license_note   TEXT NOT NULL,
    planned_format TEXT NOT NULL,
    notes          TEXT NOT NULL,
    trust_status   TEXT NOT NULL CHECK (trust_status = 'untrusted-reference'),
    status         TEXT NOT NULL CHECK (status = 'planned'),
    read_only      INTEGER NOT NULL CHECK (read_only = 1)
);

-- Hash-chained, schema-v2-conformant audit events (see schemas/audit-event.schema.json).
CREATE TABLE audit_events (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    sequence      INTEGER NOT NULL UNIQUE,
    record_digest TEXT NOT NULL,
    record_json   TEXT NOT NULL
);

CREATE TRIGGER audit_events_no_update BEFORE UPDATE ON audit_events
BEGIN SELECT RAISE(ABORT, 'audit_events is append-only'); END;
CREATE TRIGGER audit_events_no_delete BEFORE DELETE ON audit_events
BEGIN SELECT RAISE(ABORT, 'audit_events is append-only'); END;

-- Local, redacted, bounded diagnostic records (never synced, never exported automatically).
CREATE TABLE diagnostics (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    ts        TEXT NOT NULL,
    level     TEXT NOT NULL CHECK (level IN ('info', 'warn', 'error')),
    component TEXT NOT NULL,
    code      TEXT NOT NULL,
    detail    TEXT NOT NULL
);
