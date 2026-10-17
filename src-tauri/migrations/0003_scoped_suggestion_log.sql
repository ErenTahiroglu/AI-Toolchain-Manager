-- Migration 3 (Phase 1B closure, RT-1B-01 and RT-1B-02). Additive and transactional; no user data is deleted.
--
-- RT-1B-01: the suggestion log was keyed by (task_id, entry_id) although every row also carries a
-- project_id, so one project's "already shown" state could suppress another project that happened to
-- use the same task id. The key becomes (project_id, task_id, entry_id). Every existing row is copied
-- unchanged; the old key was stricter, so the copy cannot conflict.
CREATE TABLE suggestion_log_v3 (
    project_id TEXT NOT NULL,
    task_id    TEXT NOT NULL CHECK (length(task_id) BETWEEN 1 AND 64),
    entry_id   TEXT NOT NULL,
    phase      TEXT NOT NULL CHECK (phase IN ('start', 'late')),
    shown_at   TEXT NOT NULL,
    PRIMARY KEY (project_id, task_id, entry_id)
);
INSERT INTO suggestion_log_v3 (project_id, task_id, entry_id, phase, shown_at)
SELECT project_id, task_id, entry_id, phase, shown_at FROM suggestion_log;
DROP TABLE suggestion_log;
ALTER TABLE suggestion_log_v3 RENAME TO suggestion_log;

-- RT-1B-02: the classification of a user-added entry is part of its capability fingerprint and
-- therefore of every earlier decision. It can no longer be changed in place (for example by restoring
-- a removed entry with a different type). A different classification would need a separately reviewed
-- change, which does not exist in this phase.
CREATE TRIGGER catalog_user_kind_immutable BEFORE UPDATE OF kind ON catalog_entries
WHEN OLD.origin = 'user' AND NEW.kind <> OLD.kind
BEGIN SELECT RAISE(ABORT, 'the classification of a user-added entry cannot be changed in place'); END;
