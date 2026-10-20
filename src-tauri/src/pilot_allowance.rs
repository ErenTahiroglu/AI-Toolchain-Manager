//! Phase 3D-5: the ONE-INSTALLATION allowance of a real pilot recipe (Issue #24).
//!
//! A compile-time unlock (`jq_pilot::B3_UNLOCK`) lasts as long as the build does and a consent preview is single-use per *preview*, so neither is a
//! one-use *pilot authorization*. This module makes "one installation" a durable, fail-closed fact of the manager's own state:
//!
//! * **Bound** to one exact tool, version, platform, recipe digest and artifact SHA-256 ([`OneTimePilot`]), to this computer's manager identity and to
//!   ONE isolated data directory ([`data_dir_digest`]). A pilot that is not *armed* for the data directory it runs in cannot be installed at all.
//! * **One durable record** in the application's own database (an internal settings key, changed only by compare-and-swap): `reserved` (an
//!   installation is in flight) -> `consumed` (it completed and was read back) or `released` (it provably changed nothing and was rolled back).
//!   `consumed` never goes back: a removal does not reopen the installation. The record carries a digest of its own content, so a damaged or hand-edited
//!   record is detected.
//! * **Witnesses besides the record.** The manager's own committed install receipts and the audit trail's passed `install` rows say whether an
//!   installation happened. A record that is missing or says "open" while a witness says "installed" is a conflict, never an allowance.
//! * **Fail closed.** Anything uncertain (an unreadable or invalid record, an unreadable receipt directory, a record for another installation, a
//!   reservation nobody settled) is `needs_attention`. Nothing here deletes data or resets the allowance; the only way back from `reserved` is the
//!   transaction's own verified outcome, or recovery finding the committed receipt of that very reservation.
//!
//! Nothing here uses a key store, Windows Hello, a biometric or a generic credential framework: it is a small state machine over existing state.
#![allow(dead_code)]

use crate::{
    audit::{canonical_json, sha256_hex},
    lifecycle::Receipt,
    recipe::ValidatedRecipe,
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SCHEMA_VERSION: u32 = 1;
pub const RESERVED: &str = "reserved";
pub const CONSUMED: &str = "consumed";
pub const RELEASED: &str = "released";

/// The one real recipe a build may install ONCE. Absent (`None` in `Sources`) in every release build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OneTimePilot {
    pub tool_id: String,
    pub version: String,
    pub platform_target: String,
    pub recipe_digest: String,
    pub artifact_sha256: String,
    /// [`data_dir_digest`] of the ONE isolated data directory this pilot is armed for. `None` = the allowance is INACTIVE: nothing can be installed.
    pub armed_data_dir_sha256: Option<String>,
}

impl OneTimePilot {
    pub fn covers(&self, r: &ValidatedRecipe) -> bool {
        let x = r.recipe();
        !x.synthetic
            && self.tool_id == x.tool_id
            && self.version == x.version
            && self.platform_target == x.target.platform_target
            && self.recipe_digest == r.digest()
            && self.artifact_sha256 == x.artifact.sha256
    }

    /// One digest over everything the allowance is bound to (the record and its settings key derive from it).
    pub fn binding_digest(&self) -> String {
        sha256_hex(&format!(
            "atm-pilot-allowance-v1|{}|{}|{}|{}|{}",
            self.tool_id, self.version, self.platform_target, self.recipe_digest, self.artifact_sha256
        ))
    }

    pub(crate) fn key(&self) -> String {
        format!("_pilot_allowance_{}", &self.binding_digest()[..32])
    }
}

/// Identifies the isolated data directory a pilot is armed for: SHA-256 of its normalised path (backslashes, lower case, no trailing separator).
/// A path is never stored, only this digest. `tools/pilot_data_dir_digest.py` computes the same value for the person who has to name the directory.
pub fn data_dir_digest(dir: &Path) -> String {
    let normalised = dir.to_string_lossy().replace('/', "\\").to_lowercase();
    sha256_hex(&format!("atm-pilot-data-dir-v1|{}", normalised.trim_end_matches('\\')))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    pub schema_version: u32,
    pub binding_digest: String,
    pub tool_id: String,
    pub version: String,
    pub platform_target: String,
    pub recipe_digest: String,
    pub artifact_sha256: String,
    pub computer_id: String,
    pub data_dir_sha256: String,
    /// `reserved` | `consumed` | `released`
    pub state: String,
    pub attempts: u32,
    /// The approval of the reservation (the receipt of the same installation carries it).
    pub approval_id: String,
    pub txn_id: Option<String>,
    pub receipt_digest: Option<String>,
    pub updated_at: String,
    pub record_digest: String,
}

fn record_digest(r: &Record) -> String {
    let mut blank = r.clone();
    blank.record_digest = String::new();
    match serde_json::to_value(&blank) {
        Ok(v) => sha256_hex(&canonical_json(&v)),
        Err(_) => String::new(),
    }
}

fn sealed(mut r: Record) -> Record {
    r.record_digest = record_digest(&r);
    r
}

/// What the allowance says right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Nothing was ever installed under this allowance (or the last attempt provably changed nothing): ONE installation may be started.
    Open { attempts: u32 },
    /// An installation was started and nobody settled it (in flight in this process, or interrupted).
    Reserved { approval_id: String },
    /// The one installation happened. Never reopens.
    Consumed,
    /// The pilot is not armed (inactive).
    NotArmed,
    /// The pilot is armed for another data directory.
    WrongDirectory,
    /// The state cannot be trusted: a closed code.
    Invalid(&'static str),
}

impl Status {
    pub fn is_open(&self) -> bool {
        matches!(self, Status::Open { .. })
    }
}

fn read_raw(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()
}

/// Parses and validates a stored record against the allowance, the computer and the data directory.
fn parse(raw: &str, pilot: &OneTimePilot, computer: &str, dir_sha: &str) -> Result<Record, &'static str> {
    let r: Record = serde_json::from_str(raw).map_err(|_| "pilot_allowance_state_invalid")?;
    if r.schema_version != SCHEMA_VERSION
        || r.record_digest != record_digest(&r)
        || !matches!(r.state.as_str(), RESERVED | CONSUMED | RELEASED)
    {
        return Err("pilot_allowance_state_invalid");
    }
    if r.binding_digest != pilot.binding_digest()
        || r.tool_id != pilot.tool_id
        || r.version != pilot.version
        || r.platform_target != pilot.platform_target
        || r.recipe_digest != pilot.recipe_digest
        || r.artifact_sha256 != pilot.artifact_sha256
        || r.computer_id != computer
        || r.data_dir_sha256 != dir_sha
    {
        return Err("pilot_allowance_state_for_another_installation");
    }
    Ok(r)
}

/// Does the audit trail hold a passed `install` row of this exact artifact? (A witness besides the record; unreadable = the caller fails closed.)
fn audit_witness(conn: &Connection, pilot: &OneTimePilot) -> Result<bool, ()> {
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM audit_events WHERE json_extract(record_json, '$.eventType') = 'install'
               AND json_extract(record_json, '$.outcome') = 'passed' AND json_extract(record_json, '$.toolId') = ?1
               AND json_extract(record_json, '$.artifactDigest') = ?2",
            rusqlite::params![pilot.tool_id, pilot.artifact_sha256],
            |r| r.get(0),
        )
        .map_err(|_| ())?;
    Ok(n > 0)
}

fn receipt_witness(pilot: &OneTimePilot, receipts: &[Receipt]) -> bool {
    receipts.iter().any(|r| {
        r.action == crate::approval::Action::Install && !r.synthetic && r.tool_id == pilot.tool_id && r.recipe_digest == pilot.recipe_digest
    })
}

/// The allowance's status. `receipts` are the manager's committed receipts of the tool (`None` = they could not be read, which is uncertain).
pub fn evaluate(conn: &Connection, pilot: &OneTimePilot, computer: &str, dir_sha: &str, receipts: Option<&[Receipt]>) -> Status {
    match pilot.armed_data_dir_sha256.as_deref() {
        None => return Status::NotArmed,
        Some(d) if d != dir_sha => return Status::WrongDirectory,
        Some(_) => {}
    }
    let Some(receipts) = receipts else { return Status::Invalid("pilot_allowance_witness_unreadable") };
    let Ok(raw) = read_raw(conn, &pilot.key()) else { return Status::Invalid("pilot_allowance_unreadable") };
    let record = match raw.as_deref().map(|r| parse(r, pilot, computer, dir_sha)).transpose() {
        Ok(r) => r,
        Err(code) => return Status::Invalid(code),
    };
    let Ok(audited) = audit_witness(conn, pilot) else { return Status::Invalid("pilot_allowance_unreadable") };
    let witnessed = audited || receipt_witness(pilot, receipts);
    match record {
        None if witnessed => Status::Invalid("pilot_allowance_record_missing"),
        None => Status::Open { attempts: 0 },
        Some(r) if r.state == RELEASED && witnessed => Status::Invalid("pilot_allowance_state_contradicts_receipts"),
        Some(r) if r.state == RELEASED => Status::Open { attempts: r.attempts },
        Some(r) if r.state == RESERVED => Status::Reserved { approval_id: r.approval_id },
        Some(_) => Status::Consumed,
    }
}

/// Compare-and-swap: writes `value` only if the key still holds `expected` (`None` = the key must not exist). Returns whether this call won.
fn swap(conn: &Connection, key: &str, expected: Option<&str>, value: &str, now: &str) -> bool {
    let changed = match expected {
        None => conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO NOTHING",
            rusqlite::params![key, value, now],
        ),
        Some(old) => conn.execute(
            "UPDATE settings SET value = ?2, updated_at = ?3 WHERE key = ?1 AND value = ?4",
            rusqlite::params![key, value, now, old],
        ),
    };
    matches!(changed, Ok(1))
}

/// A record sealed with a correct digest (tests forge records with it; the application itself only writes through the transitions).
pub(crate) fn write_record(r: Record) -> String {
    serde_json::to_string(&sealed(r)).unwrap_or_default()
}

/// Everything a transition needs besides the connection: which allowance, for whom, where and when.
pub struct Scope<'a> {
    pub pilot: &'a OneTimePilot,
    pub computer: &'a str,
    pub dir_sha: &'a str,
    pub now: &'a str,
}

/// Reserves the ONE installation. Only from "never used" or "released"; exactly one concurrent caller can win. The caller has already evaluated
/// the status as `Open` (witnesses included).
pub fn reserve(conn: &Connection, sc: &Scope, approval_id: &str) -> Result<(), &'static str> {
    let (pilot, key) = (sc.pilot, sc.pilot.key());
    let current = read_raw(conn, &key).map_err(|_| "pilot_allowance_unreadable")?;
    let attempts = match current.as_deref() {
        None => 1,
        Some(raw) => {
            let r = parse(raw, pilot, sc.computer, sc.dir_sha)?;
            if r.state != RELEASED {
                return Err("pilot_allowance_unavailable");
            }
            r.attempts + 1
        }
    };
    let next = write_record(Record {
        schema_version: SCHEMA_VERSION,
        binding_digest: pilot.binding_digest(),
        tool_id: pilot.tool_id.clone(),
        version: pilot.version.clone(),
        platform_target: pilot.platform_target.clone(),
        recipe_digest: pilot.recipe_digest.clone(),
        artifact_sha256: pilot.artifact_sha256.clone(),
        computer_id: sc.computer.to_string(),
        data_dir_sha256: sc.dir_sha.to_string(),
        state: RESERVED.to_string(),
        attempts,
        approval_id: approval_id.to_string(),
        txn_id: None,
        receipt_digest: None,
        updated_at: sc.now.to_string(),
        record_digest: String::new(),
    });
    if swap(conn, &key, current.as_deref(), &next, sc.now) {
        Ok(())
    } else {
        Err("pilot_allowance_unavailable")
    }
}

/// Settles THIS reservation (matched by its approval id): `reserved` -> `consumed` / `released`.
fn settle(
    conn: &Connection,
    sc: &Scope,
    approval_id: &str,
    to: &str,
    txn: Option<&str>,
    receipt_digest: Option<&str>,
) -> Result<(), &'static str> {
    let key = sc.pilot.key();
    let raw = read_raw(conn, &key).map_err(|_| "pilot_allowance_unreadable")?.ok_or("pilot_allowance_record_missing")?;
    let mut r = parse(&raw, sc.pilot, sc.computer, sc.dir_sha)?;
    if r.state != RESERVED || r.approval_id != approval_id {
        return Err("pilot_allowance_unavailable");
    }
    r.state = to.to_string();
    r.txn_id = txn.map(str::to_string);
    r.receipt_digest = receipt_digest.map(str::to_string);
    r.updated_at = sc.now.to_string();
    if swap(conn, &key, Some(&raw), &write_record(r), sc.now) {
        Ok(())
    } else {
        Err("pilot_allowance_unavailable")
    }
}

/// The installation completed and was read back: the allowance is spent for good.
pub fn consume(conn: &Connection, sc: &Scope, approval_id: &str, txn: &str, receipt_digest: &str) -> Result<(), &'static str> {
    settle(conn, sc, approval_id, CONSUMED, Some(txn), Some(receipt_digest))
}

/// The attempt provably changed nothing (rolled back, read back as "not installed"): the allowance is open again. The caller records its audit row FIRST.
pub fn release(conn: &Connection, sc: &Scope, approval_id: &str) -> Result<(), &'static str> {
    settle(conn, sc, approval_id, RELEASED, None, None)
}

#[cfg(test)]
#[path = "pilot_allowance_tests.rs"]
mod tests;
