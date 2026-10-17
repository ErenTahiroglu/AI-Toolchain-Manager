-- Migration 6 (Phase 2B): passive provenance for discovery observations. Additive.
--
-- Each observation now records the evidence ladder (file name seen / installation record / publisher metadata /
-- identity state / version state / checks that were not performed) as small closed enumerations. Nothing here can hold
-- a path, a name or a free-text claim. Rows written before this migration keep their facts; their ladder is marked as
-- not assessed (`provenance_json` = '{}'), except that an identity the Phase 2A rules had already accepted
-- (identity_verified = 1) and a version already shown stay represented.
ALTER TABLE discovery_observations ADD COLUMN identity_state TEXT NOT NULL DEFAULT 'not_assessed'
    CHECK (identity_state IN ('not_assessed', 'unknown', 'evidence_available', 'validated', 'conflicted'));
ALTER TABLE discovery_observations ADD COLUMN version_state TEXT NOT NULL DEFAULT 'unknown'
    CHECK (version_state IN ('unknown', 'known', 'conflicted'));
ALTER TABLE discovery_observations ADD COLUMN provenance_json TEXT NOT NULL DEFAULT '{}';
UPDATE discovery_observations SET identity_state = 'validated' WHERE identity_verified = 1;
UPDATE discovery_observations SET version_state = 'known' WHERE version IS NOT NULL;
