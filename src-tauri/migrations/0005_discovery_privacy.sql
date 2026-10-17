-- Migration 5 (Phase 2A closure): privacy gate for discovery observations. Additive.
--
-- `record_version` tells apart rows written before the privacy gate existed (1: may have held a full path,
-- another account's folder name or an MCP server name) from rows written under the gate (2). The Rust step that
-- runs after this statement scrubs every version-1 row in place and leaves the rows marked as legacy.
-- No row, scan, preference or audit event is deleted.
ALTER TABLE discovery_observations ADD COLUMN record_version INTEGER NOT NULL DEFAULT 1 CHECK (record_version IN (1, 2));
