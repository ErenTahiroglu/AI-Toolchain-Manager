-- Migration 9 (Phase 3C): the manager's OWN record of the folder it owns for managed installations. Additive.
--
-- A managed root is never recognised by a marker inside the folder (a marker is only a hint that anyone can copy). The identity of
-- the folder (volume + file index on Windows, device + inode on macOS) is recorded here, in the manager's database, when the
-- manager creates it; opening a root later requires that identity to match. Nothing in the application creates a row yet: the
-- table is used by the isolated test-mode adapter only, and the production recipe list stays empty.
--   * `creating` is written BEFORE the folder exists, `active` only after its identity was read from the created folder: a crash in
--     between leaves a `creating` row, which blocks adoption (an unfinished root is never taken over by guesswork).
--   * at most one live (`creating` or `active`) root per computer.
--   * ids are stored as decimal text because SQLite integers are signed 64-bit.
CREATE TABLE managed_roots (
    root_id      TEXT PRIMARY KEY CHECK (length(root_id) = 32 AND root_id NOT GLOB '*[^0-9a-f]*'),
    computer_id  TEXT NOT NULL CHECK (length(computer_id) BETWEEN 1 AND 80),
    platform     TEXT NOT NULL CHECK (platform IN ('windows', 'macos')),
    volume_id    TEXT CHECK (volume_id IS NULL OR (length(volume_id) BETWEEN 1 AND 20 AND volume_id NOT GLOB '*[^0-9]*')),
    object_id    TEXT CHECK (object_id IS NULL OR (length(object_id) BETWEEN 1 AND 20 AND object_id NOT GLOB '*[^0-9]*')),
    state        TEXT NOT NULL CHECK (state IN ('creating', 'active', 'retired')),
    created_at   TEXT NOT NULL,
    activated_at TEXT,
    CHECK (state <> 'active' OR (volume_id IS NOT NULL AND object_id IS NOT NULL AND activated_at IS NOT NULL))
);
CREATE UNIQUE INDEX managed_roots_one_live ON managed_roots (computer_id) WHERE state IN ('creating', 'active');
