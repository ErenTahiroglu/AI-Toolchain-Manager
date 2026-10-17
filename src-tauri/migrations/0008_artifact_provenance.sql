-- Migration 8 (Phase 2C): artifact fingerprints and reference digests. Additive.
--
-- * `discovery_scans.authorized_fingerprint`: whether the user allowed reading file contents to compute SHA-256
--   fingerprints in that scan (older scans: 0 = no).
-- * `reference_digests`: reference values for artifacts. Everything stored here is UNAUTHENTICATED by construction: the
--   `authentication` column can only be 'none', and the application never treats a row as an independent trust anchor
--   (an authenticated reference exists only as reviewed code, see discovery/reference.rs). Nothing is inserted by this
--   migration and no scan inserts anything: no digest is ever derived from a local file.
-- The fingerprint and the match verdict of an observation live in its `provenance_json` (closed vocabulary + a hex digest).
ALTER TABLE discovery_scans ADD COLUMN authorized_fingerprint INTEGER NOT NULL DEFAULT 0 CHECK (authorized_fingerprint IN (0, 1));

CREATE TABLE reference_digests (
    ref_id          INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_id        TEXT NOT NULL CHECK (length(entry_id) BETWEEN 2 AND 80),
    artifact_kind   TEXT NOT NULL CHECK (artifact_kind IN ('archive', 'installer', 'executable', 'script', 'plugin_file', 'package')),
    format          TEXT NOT NULL CHECK (length(format) BETWEEN 1 AND 40),
    platform_target TEXT NOT NULL CHECK (platform_target IN ('windows-x64', 'macos-arm64')),
    version         TEXT NOT NULL CHECK (length(version) BETWEEN 1 AND 64),
    algorithm       TEXT NOT NULL CHECK (algorithm = 'sha256'),
    sha256          TEXT NOT NULL CHECK (length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
    source_kind     TEXT NOT NULL CHECK (source_kind IN ('user_provided', 'readme_declared', 'package_metadata_declared', 'catalog_unreviewed', 'synthetic_test')),
    source_label    TEXT NOT NULL CHECK (length(source_label) <= 80),
    authentication  TEXT NOT NULL DEFAULT 'none' CHECK (authentication = 'none'),
    added_at        TEXT NOT NULL,
    removed_at      TEXT
);
CREATE INDEX reference_digests_entry ON reference_digests (entry_id, platform_target);
