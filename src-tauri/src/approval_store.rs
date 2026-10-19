//! Phase 3D-1: the DURABLE approval ledger, the append-only audit journal and the approval-security setting, in one small SQLite file
//! of their own (`approval-ledger.sqlite`), apart from the main database.
//!
//! * **Single use survives a restart.** Consuming an approval is one conditional `UPDATE ... WHERE status = 'issued'` inside an
//!   immediate transaction on a `synchronous = FULL` database: of any number of concurrent or later attempts, exactly one wins.
//!   `consumed` and `revoked` are terminal (a trigger refuses to leave them), so no restart, reopen or second connection can
//!   resurrect an approval.
//! * **Audit is a hash chain and append-only.** Triggers refuse `UPDATE` and `DELETE`; every row carries the hash of the previous row
//!   (`prev_hash` is `UNIQUE`, so two writers cannot fork the chain). [`ApprovalStore::verify_audit_chain`] recomputes it. If an audit
//!   row cannot be written the caller ([`crate::device_trust::authorize`]) denies the operation.
//! * **No silent key creation.** The setting `approval_security` starts disabled. [`ApprovalStore::enable_approval_security`] needs a
//!   typed phrase that contains this computer's id (a deliberate act, audited); only then [`ApprovalStore::enrollment_permit`] hands out
//!   an [`EnrollmentPermit`], the only thing that lets an OS credential backend create a key. Nothing in the application calls
//!   any of this: there is no command, no button and no production caller in this phase.
//!
//! The content is closed vocabulary and identifiers: no path, no credential, no key material other than a PUBLIC key.
#![allow(dead_code)] // no command reaches this module in Phase 3D-1

use crate::{
    audit::{canonical_json, sha256_hex},
    device_trust::{ApprovalLedger, AuditRecord, AuditSink, LedgerStatus, TrustError},
};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::{path::Path, sync::Mutex};

pub const SCHEMA_VERSION: i64 = 1;
pub const FILE_NAME: &str = "approval-ledger.sqlite";
const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

const SCHEMA: &str = "
CREATE TABLE counter (id INTEGER PRIMARY KEY CHECK (id = 1), value INTEGER NOT NULL);
INSERT INTO counter (id, value) VALUES (1, 0);
CREATE TABLE approvals (
    approval_id TEXT PRIMARY KEY,
    counter     INTEGER NOT NULL UNIQUE,
    status      TEXT NOT NULL CHECK (status IN ('issued', 'consumed', 'revoked')),
    txn_id      TEXT,
    issued_at   TEXT NOT NULL,
    finished_at TEXT
);
CREATE TRIGGER approvals_terminal BEFORE UPDATE ON approvals WHEN OLD.status <> 'issued'
BEGIN SELECT RAISE(ABORT, 'a consumed or revoked approval is terminal'); END;
CREATE TRIGGER approvals_no_delete BEFORE DELETE ON approvals
BEGIN SELECT RAISE(ABORT, 'approvals are never deleted'); END;
CREATE TABLE audit (
    seq        INTEGER PRIMARY KEY AUTOINCREMENT,
    ts         TEXT NOT NULL,
    event      TEXT NOT NULL,
    outcome    TEXT NOT NULL,
    approval_id TEXT,
    tool_id    TEXT NOT NULL,
    version    TEXT NOT NULL,
    action     TEXT NOT NULL,
    code       TEXT,
    prev_hash  TEXT NOT NULL UNIQUE,
    hash       TEXT NOT NULL UNIQUE
);
CREATE TRIGGER audit_no_update BEFORE UPDATE ON audit BEGIN SELECT RAISE(ABORT, 'the audit journal is append-only'); END;
CREATE TRIGGER audit_no_delete BEFORE DELETE ON audit BEGIN SELECT RAISE(ABORT, 'the audit journal is append-only'); END;
CREATE TABLE approval_security (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled INTEGER NOT NULL,
    enabled_at TEXT,
    computer_id TEXT,
    key_name TEXT,
    key_id TEXT,
    public_key_hex TEXT,
    assurance TEXT,
    attestation_digest TEXT,
    key_recorded_at TEXT
);
INSERT INTO approval_security (id, enabled) VALUES (1, 0);
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    /// The file could not be opened, created or migrated.
    Unavailable,
    /// The file has a schema this build does not know.
    UnsupportedSchema,
    /// The audit chain does not verify.
    AuditChainBroken,
    /// The typed phrase was wrong.
    PhraseMismatch,
    /// Approval security is not enabled (for this computer).
    NotEnabled,
    /// A key is already recorded for this setting.
    KeyAlreadyRecorded,
    /// The ledger and the audit journal disagree (an approval state that its journal does not explain).
    Inconsistent,
    /// The setting belongs to another computer.
    OtherComputer,
}

impl StoreError {
    pub fn code(self) -> &'static str {
        match self {
            StoreError::Unavailable => "approval_store_unavailable",
            StoreError::UnsupportedSchema => "approval_store_unsupported_schema",
            StoreError::AuditChainBroken => "approval_audit_chain_broken",
            StoreError::PhraseMismatch => "approval_security_phrase_mismatch",
            StoreError::NotEnabled => "approval_security_not_enabled",
            StoreError::KeyAlreadyRecorded => "approval_key_already_recorded",
            StoreError::Inconsistent => "approval_ledger_inconsistent",
            StoreError::OtherComputer => "approval_security_other_computer",
        }
    }
}

/// The only thing that lets an OS credential backend create a key. Only [`ApprovalStore::enrollment_permit`] makes one, and only after the
/// person explicitly enabled approval security for this computer. It cannot be cloned or built elsewhere.
#[derive(Debug)]
pub struct EnrollmentPermit {
    computer_id: String,
}

impl EnrollmentPermit {
    pub fn computer_id(&self) -> &str {
        &self.computer_id
    }
}

/// What the store keeps about the approval key: identifiers and the PUBLIC key only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRecord {
    pub key_name: String,
    pub key_id: String,
    pub public_key_hex: String,
    pub assurance: String,
    pub attestation_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalSecurity {
    pub enabled: bool,
    pub enabled_at: Option<String>,
    pub computer_id: Option<String>,
    pub key: Option<KeyRecord>,
}

/// The phrase the person has to type to enable approval security for this computer.
pub fn enable_phrase(computer_id: &str) -> String {
    format!("ENABLE-OS-APPROVAL {computer_id}")
}

/// The phrase the person has to type to remove the approval key record and switch approval security off again.
pub fn disable_phrase(computer_id: &str) -> String {
    format!("DISABLE-OS-APPROVAL {computer_id}")
}

/// `(event, outcome, approval_id, code)` of one audit row.
pub type AuditEventRow = (String, String, Option<String>, Option<String>);

type Clock = Box<dyn Fn() -> DateTime<Utc> + Send + Sync>;

pub struct ApprovalStore {
    conn: Mutex<Connection>,
    clock: Clock,
    /// Test-only fault injection at the commit point: 1 = the commit fails (the transaction is rolled back), 2 = the process aborts.
    #[cfg(test)]
    pub(crate) fault: std::sync::atomic::AtomicU32,
}

fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

impl ApprovalStore {
    pub fn open(path: &Path) -> Result<ApprovalStore, StoreError> {
        ApprovalStore::open_with_clock(path, Box::new(Utc::now))
    }

    pub fn open_with_clock(path: &Path, clock: Clock) -> Result<ApprovalStore, StoreError> {
        let conn = Connection::open(path).map_err(|_| StoreError::Unavailable)?;
        conn.busy_timeout(std::time::Duration::from_secs(10)).map_err(|_| StoreError::Unavailable)?;
        // durability first: the consumption of an approval must be on disk before the transaction proceeds
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;").map_err(|_| StoreError::Unavailable)?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(|_| StoreError::Unavailable)?;
        match version {
            0 => {
                let tx = conn.unchecked_transaction().map_err(|_| StoreError::Unavailable)?;
                // two processes may race to initialise: re-check inside the transaction
                let again: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(|_| StoreError::Unavailable)?;
                if again == 0 {
                    tx.execute_batch(SCHEMA).map_err(|_| StoreError::Unavailable)?;
                    tx.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};")).map_err(|_| StoreError::Unavailable)?;
                }
                tx.commit().map_err(|_| StoreError::Unavailable)?;
            }
            SCHEMA_VERSION => {}
            _ => return Err(StoreError::UnsupportedSchema),
        }
        Ok(ApprovalStore {
            conn: Mutex::new(conn),
            clock,
            #[cfg(test)]
            fault: std::sync::atomic::AtomicU32::new(0),
        })
    }

    fn now(&self) -> String {
        stamp((self.clock)())
    }

    // ---- audit -----------------------------------------------------------------------------------------------------------------

    /// The single commit point of every transaction of this store (the test seam sits here and nowhere else).
    fn finish(&self, tx: rusqlite::Transaction) -> rusqlite::Result<()> {
        #[cfg(test)]
        match self.fault.swap(0, std::sync::atomic::Ordering::SeqCst) {
            1 => return Err(rusqlite::Error::ExecuteReturnedResults), // injected commit failure: `tx` is dropped, i.e. rolled back
            2 => std::process::abort(),                               // injected process death in the middle of the transaction
            _ => {}
        }
        tx.commit()
    }

    fn append_audit(&self, conn: &mut Connection, rec: &AuditRecord) -> rusqlite::Result<()> {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.append_audit_tx(&tx, rec)?;
        self.finish(tx)
    }

    /// Appends one chained row inside the caller's transaction.
    fn append_audit_tx(&self, tx: &rusqlite::Transaction, rec: &AuditRecord) -> rusqlite::Result<()> {
        let ts = self.now();
        let prev: String = tx
            .query_row("SELECT hash FROM audit ORDER BY seq DESC LIMIT 1", [], |r| r.get(0))
            .optional()?
            .unwrap_or_else(|| GENESIS.to_string());
        let hash = chain_hash(&prev, &ts, rec);
        tx.execute(
            "INSERT INTO audit (ts, event, outcome, approval_id, tool_id, version, action, code, prev_hash, hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![ts, rec.event, rec.outcome, rec.approval_id, rec.tool_id, rec.version, rec.action, rec.code, prev, hash],
        )?;
        Ok(())
    }

    /// Recomputes the whole chain. `Ok(n)` is the number of rows.
    pub fn verify_audit_chain(&self) -> Result<usize, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Unavailable)?;
        verify_chain_on(&conn)
    }

    /// Cross-checks every approval against the journal: each one was issued exactly once in the journal; a consumed one has exactly one
    /// `authorized` row, any other state has none; a revoked one has exactly one revocation row.
    pub fn verify_consistency(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Unavailable)?;
        let mut stmt = conn.prepare("SELECT approval_id, status FROM approvals").map_err(|_| StoreError::Unavailable)?;
        let rows: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|_| StoreError::Unavailable)?
            .collect::<Result<_, _>>()
            .map_err(|_| StoreError::Unavailable)?;
        for (id, status) in rows {
            let (issued, authorized, revoked) = journal_counts(&conn, &id).map_err(|_| StoreError::Unavailable)?;
            let ok = issued == 1
                && match status.as_str() {
                    "issued" => authorized == 0 && revoked == 0,
                    "consumed" => authorized == 1 && revoked == 0,
                    "revoked" => authorized == 0 && revoked == 1,
                    _ => false,
                };
            if !ok {
                return Err(StoreError::Inconsistent);
            }
        }
        Ok(())
    }

    /// Is `audit` this very store? The atomic operations write the journal in the ledger's own transaction, so a different sink is refused.
    fn is_this_store(&self, audit: &dyn AuditSink) -> bool {
        std::ptr::eq(audit as *const dyn AuditSink as *const (), self as *const ApprovalStore as *const ())
    }

    /// The journal in order (for tests and for the person's review): `(event, outcome, approval_id, code)`.
    pub fn audit_events(&self) -> Result<Vec<AuditEventRow>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Unavailable)?;
        let mut stmt =
            conn.prepare("SELECT event, outcome, approval_id, code FROM audit ORDER BY seq").map_err(|_| StoreError::Unavailable)?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map_err(|_| StoreError::Unavailable)?;
        rows.collect::<Result<_, _>>().map_err(|_| StoreError::Unavailable)
    }

    fn audit_internal(&self, event: &'static str, outcome: &'static str, code: Option<&'static str>) -> Result<(), StoreError> {
        let rec = AuditRecord {
            event,
            outcome,
            approval_id: None,
            tool_id: "-".to_string(),
            version: "-".to_string(),
            action: "approval-security",
            code,
        };
        let mut conn = self.conn.lock().map_err(|_| StoreError::Unavailable)?;
        self.append_audit(&mut conn, &rec).map_err(|_| StoreError::Unavailable)
    }

    // ---- approval security -----------------------------------------------------------------------------------------------------

    pub fn security(&self) -> Result<ApprovalSecurity, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Unavailable)?;
        conn.query_row(
            "SELECT enabled, enabled_at, computer_id, key_name, key_id, public_key_hex, assurance, attestation_digest FROM approval_security WHERE id = 1",
            [],
            |r| {
                let key_id: Option<String> = r.get(4)?;
                Ok(ApprovalSecurity {
                    enabled: r.get::<_, i64>(0)? == 1,
                    enabled_at: r.get(1)?,
                    computer_id: r.get(2)?,
                    key: match key_id {
                        Some(key_id) => Some(KeyRecord {
                            key_name: r.get(3)?,
                            key_id,
                            public_key_hex: r.get(5)?,
                            assurance: r.get(6)?,
                            attestation_digest: r.get(7)?,
                        }),
                        None => None,
                    },
                })
            },
        )
        .map_err(|_| StoreError::Unavailable)
    }

    /// The person's explicit act. Requires the exact phrase for this computer; audited; idempotent for the same computer.
    pub fn enable_approval_security(&self, computer_id: &str, typed_phrase: &str) -> Result<(), StoreError> {
        if typed_phrase != enable_phrase(computer_id) {
            let _ = self.audit_internal("approval_security_enable", "refused", Some(StoreError::PhraseMismatch.code()));
            return Err(StoreError::PhraseMismatch);
        }
        let current = self.security()?;
        if current.enabled && current.computer_id.as_deref() != Some(computer_id) {
            return Err(StoreError::OtherComputer);
        }
        if current.enabled {
            return Ok(());
        }
        self.audit_internal("approval_security_enable", "enabled", None)?; // the audit comes first: no audit, no enabling
        let conn = self.conn.lock().map_err(|_| StoreError::Unavailable)?;
        conn.execute(
            "UPDATE approval_security SET enabled = 1, enabled_at = ?1, computer_id = ?2 WHERE id = 1 AND enabled = 0",
            params![self.now(), computer_id],
        )
        .map_err(|_| StoreError::Unavailable)?;
        Ok(())
    }

    /// The person's explicit act to switch approval security off: needs the exact phrase, is audited first, and clears the recorded key
    /// and the enabled flag. It does NOT touch the operating system's key: [`crate::os_approval::deprovision`] deletes that first. The
    /// ledger and the audit journal are kept (history is never erased).
    pub fn disable_approval_security(&self, computer_id: &str, typed_phrase: &str) -> Result<(), StoreError> {
        if typed_phrase != disable_phrase(computer_id) {
            let _ = self.audit_internal("approval_security_disable", "refused", Some(StoreError::PhraseMismatch.code()));
            return Err(StoreError::PhraseMismatch);
        }
        let s = self.security()?;
        if !s.enabled {
            return Err(StoreError::NotEnabled);
        }
        if s.computer_id.as_deref() != Some(computer_id) {
            return Err(StoreError::OtherComputer);
        }
        self.audit_internal("approval_security_disable", "disabled", None)?;
        let conn = self.conn.lock().map_err(|_| StoreError::Unavailable)?;
        conn.execute(
            "UPDATE approval_security SET enabled = 0, enabled_at = NULL, computer_id = NULL, key_name = NULL, key_id = NULL, public_key_hex = NULL, assurance = NULL, attestation_digest = NULL, key_recorded_at = NULL WHERE id = 1 AND computer_id = ?1",
            params![computer_id],
        )
        .map_err(|_| StoreError::Unavailable)?;
        Ok(())
    }

    /// Hands out the one permit that allows an OS backend to create the approval key: enabled, for this computer, and no key recorded yet.
    pub fn enrollment_permit(&self, computer_id: &str) -> Result<EnrollmentPermit, StoreError> {
        let s = self.security()?;
        if !s.enabled {
            return Err(StoreError::NotEnabled);
        }
        if s.computer_id.as_deref() != Some(computer_id) {
            return Err(StoreError::OtherComputer);
        }
        if s.key.is_some() {
            return Err(StoreError::KeyAlreadyRecorded);
        }
        Ok(EnrollmentPermit { computer_id: computer_id.to_string() })
    }

    /// Records the key a backend created under a permit (identifiers and the PUBLIC key only). Once.
    pub fn record_key(&self, permit: &EnrollmentPermit, key: &KeyRecord) -> Result<(), StoreError> {
        self.audit_internal("approval_key_record", "recorded", None)?;
        let conn = self.conn.lock().map_err(|_| StoreError::Unavailable)?;
        let n = conn
            .execute(
                "UPDATE approval_security SET key_name=?1, key_id=?2, public_key_hex=?3, assurance=?4, attestation_digest=?5, key_recorded_at=?6
                 WHERE id = 1 AND enabled = 1 AND computer_id = ?7 AND key_id IS NULL",
                params![key.key_name, key.key_id, key.public_key_hex, key.assurance, key.attestation_digest, self.now(), permit.computer_id],
            )
            .map_err(|_| StoreError::Unavailable)?;
        if n == 1 {
            Ok(())
        } else {
            Err(StoreError::KeyAlreadyRecorded)
        }
    }
}

fn chain_hash(prev: &str, ts: &str, rec: &AuditRecord) -> String {
    let v = serde_json::json!({
        "ts": ts, "event": rec.event, "outcome": rec.outcome, "approvalId": rec.approval_id,
        "toolId": rec.tool_id, "version": rec.version, "action": rec.action, "code": rec.code,
    });
    sha256_hex(&format!("{prev}{}", canonical_json(&v)))
}

/// Recomputes the chain on any connection or transaction.
fn verify_chain_on(conn: &Connection) -> Result<usize, StoreError> {
    let mut stmt = conn
        .prepare("SELECT ts, event, outcome, approval_id, tool_id, version, action, code, prev_hash, hash FROM audit ORDER BY seq")
        .map_err(|_| StoreError::Unavailable)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, String>(9)?,
            ))
        })
        .map_err(|_| StoreError::Unavailable)?;
    let mut prev = GENESIS.to_string();
    let mut n = 0;
    for row in rows {
        let (ts, event, outcome, approval_id, tool_id, version, action, code, prev_hash, hash) =
            row.map_err(|_| StoreError::Unavailable)?;
        let v = serde_json::json!({ "ts": ts, "event": event, "outcome": outcome, "approvalId": approval_id, "toolId": tool_id, "version": version, "action": action, "code": code });
        let expect = sha256_hex(&format!("{prev}{}", canonical_json(&v)));
        if prev_hash != prev || hash != expect {
            return Err(StoreError::AuditChainBroken);
        }
        prev = hash;
        n += 1;
    }
    Ok(n)
}

/// `(issued rows, authorized rows, revocation rows)` the journal holds for one approval.
fn journal_counts(conn: &Connection, id: &str) -> rusqlite::Result<(i64, i64, i64)> {
    let count = |event: &str, outcome: &str| -> rusqlite::Result<i64> {
        conn.query_row(
            "SELECT COUNT(*) FROM audit WHERE approval_id = ?1 AND event = ?2 AND outcome = ?3",
            params![id, event, outcome],
            |r| r.get(0),
        )
    };
    Ok((count("approval_issue", "issued")?, count("approval_use", "authorized")?, count("approval_revoke", "revoked")?))
}

// ---- the device_trust interfaces, backed by the file -------------------------------------------------------------------------------

impl ApprovalLedger for ApprovalStore {
    fn next_counter(&self) -> Result<u64, TrustError> {
        let mut conn = self.conn.lock().map_err(|_| TrustError::LedgerUnavailable)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|_| TrustError::LedgerUnavailable)?;
        tx.execute("UPDATE counter SET value = value + 1 WHERE id = 1", []).map_err(|_| TrustError::LedgerUnavailable)?;
        let v: i64 = tx.query_row("SELECT value FROM counter WHERE id = 1", [], |r| r.get(0)).map_err(|_| TrustError::LedgerUnavailable)?;
        tx.commit().map_err(|_| TrustError::LedgerUnavailable)?;
        Ok(v as u64)
    }

    fn record_issue(&self, approval_id: &str, counter: u64) -> Result<(), TrustError> {
        let conn = self.conn.lock().map_err(|_| TrustError::LedgerUnavailable)?;
        match conn.execute(
            "INSERT INTO approvals (approval_id, counter, status, issued_at) VALUES (?1, ?2, 'issued', ?3)",
            params![approval_id, counter as i64, self.now()],
        ) {
            Ok(_) => Ok(()),
            Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => Err(TrustError::Replayed),
            Err(_) => Err(TrustError::LedgerUnavailable),
        }
    }

    fn status(&self, approval_id: &str) -> Result<Option<LedgerStatus>, TrustError> {
        let conn = self.conn.lock().map_err(|_| TrustError::LedgerUnavailable)?;
        let row: Option<(String, Option<String>)> = conn
            .query_row("SELECT status, txn_id FROM approvals WHERE approval_id = ?1", params![approval_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .map_err(|_| TrustError::LedgerUnavailable)?;
        Ok(row.map(|(status, txn)| match status.as_str() {
            "issued" => LedgerStatus::Issued,
            "revoked" => LedgerStatus::Revoked,
            _ => LedgerStatus::Consumed { txn_id: txn.unwrap_or_default() },
        }))
    }

    fn consume(&self, approval_id: &str, txn_id: &str) -> Result<(), TrustError> {
        let mut conn = self.conn.lock().map_err(|_| TrustError::LedgerUnavailable)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|_| TrustError::LedgerUnavailable)?;
        let n = tx
            .execute(
                "UPDATE approvals SET status = 'consumed', txn_id = ?2, finished_at = ?3 WHERE approval_id = ?1 AND status = 'issued'",
                params![approval_id, txn_id, self.now()],
            )
            .map_err(|_| TrustError::LedgerUnavailable)?;
        if n == 1 {
            return tx.commit().map_err(|_| TrustError::LedgerUnavailable);
        }
        let status: Option<String> = tx
            .query_row("SELECT status FROM approvals WHERE approval_id = ?1", params![approval_id], |r| r.get(0))
            .optional()
            .map_err(|_| TrustError::LedgerUnavailable)?;
        Err(match status.as_deref() {
            Some("consumed") => TrustError::Replayed,
            Some("revoked") => TrustError::Revoked,
            _ => TrustError::UnknownApproval,
        })
    }

    fn revoke(&self, approval_id: &str) -> Result<(), TrustError> {
        let mut conn = self.conn.lock().map_err(|_| TrustError::LedgerUnavailable)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|_| TrustError::LedgerUnavailable)?;
        let n = tx
            .execute(
                "UPDATE approvals SET status = 'revoked', finished_at = ?2 WHERE approval_id = ?1 AND status = 'issued'",
                params![approval_id, self.now()],
            )
            .map_err(|_| TrustError::LedgerUnavailable)?;
        if n == 1 {
            // the revocation and its journal row are one transaction
            let rec = AuditRecord {
                event: "approval_revoke",
                outcome: "revoked",
                approval_id: Some(approval_id.to_string()),
                tool_id: "-".into(),
                version: "-".into(),
                action: "revoke",
                code: None,
            };
            self.append_audit_tx(&tx, &rec).map_err(|_| TrustError::AuditUnavailable)?;
            return self.finish(tx).map_err(|_| TrustError::LedgerUnavailable);
        }
        let status: Option<String> = tx
            .query_row("SELECT status FROM approvals WHERE approval_id = ?1", params![approval_id], |r| r.get(0))
            .optional()
            .map_err(|_| TrustError::LedgerUnavailable)?;
        match status.as_deref() {
            Some("consumed") | Some("revoked") => Ok(()), // too late or already done: the transaction/revocation already stands
            _ => Err(TrustError::UnknownApproval),
        }
    }

    fn atomic_with_audit(&self) -> bool {
        true
    }

    /// Issuance and its `issued` journal row: ONE transaction. If the journal cannot be written, no approval row survives.
    fn record_issue_audited(&self, approval_id: &str, counter: u64, audit: &dyn AuditSink, record: AuditRecord) -> Result<(), TrustError> {
        if !self.is_this_store(audit) {
            return Err(TrustError::AuditUnavailable);
        }
        let mut conn = self.conn.lock().map_err(|_| TrustError::LedgerUnavailable)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|_| TrustError::LedgerUnavailable)?;
        verify_chain_on(&tx).map_err(|_| TrustError::AuditUnavailable)?;
        match tx.execute(
            "INSERT INTO approvals (approval_id, counter, status, issued_at) VALUES (?1, ?2, 'issued', ?3)",
            params![approval_id, counter as i64, self.now()],
        ) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => {
                return Err(TrustError::Replayed)
            }
            Err(_) => return Err(TrustError::LedgerUnavailable),
        }
        self.append_audit_tx(&tx, &record).map_err(|_| TrustError::AuditUnavailable)?;
        self.finish(tx).map_err(|_| TrustError::LedgerUnavailable)
    }

    /// Single-use consumption and its `authorized` journal row: ONE transaction. `authorized` is written only if THIS transaction changed
    /// the approval from `issued` to `consumed`; a replayed, revoked, unknown or inconsistent approval writes no such row. The chain and the
    /// approval's own journal history are verified first, so a ledger edited behind the journal's back is refused.
    fn consume_audited(&self, approval_id: &str, txn_id: &str, audit: &dyn AuditSink, record: AuditRecord) -> Result<(), TrustError> {
        if !self.is_this_store(audit) {
            return Err(TrustError::AuditUnavailable);
        }
        let mut conn = self.conn.lock().map_err(|_| TrustError::LedgerUnavailable)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|_| TrustError::LedgerUnavailable)?;
        verify_chain_on(&tx).map_err(|_| TrustError::AuditUnavailable)?;
        let status: Option<String> = tx
            .query_row("SELECT status FROM approvals WHERE approval_id = ?1", params![approval_id], |r| r.get(0))
            .optional()
            .map_err(|_| TrustError::LedgerUnavailable)?;
        match status.as_deref() {
            None => return Err(TrustError::UnknownApproval),
            Some("revoked") => return Err(TrustError::Revoked),
            Some("consumed") => return Err(TrustError::Replayed),
            Some("issued") => {}
            Some(_) => return Err(TrustError::LedgerInconsistent),
        }
        let (issued, authorized, revoked) = journal_counts(&tx, approval_id).map_err(|_| TrustError::AuditUnavailable)?;
        if issued != 1 || authorized != 0 || revoked != 0 {
            return Err(TrustError::LedgerInconsistent);
        }
        let n = tx
            .execute(
                "UPDATE approvals SET status = 'consumed', txn_id = ?2, finished_at = ?3 WHERE approval_id = ?1 AND status = 'issued'",
                params![approval_id, txn_id, self.now()],
            )
            .map_err(|_| TrustError::LedgerUnavailable)?;
        if n != 1 {
            return Err(TrustError::Replayed);
        }
        self.append_audit_tx(&tx, &record).map_err(|_| TrustError::AuditUnavailable)?;
        self.finish(tx).map_err(|_| TrustError::LedgerUnavailable)
    }
}

impl AuditSink for ApprovalStore {
    fn record(&self, record: AuditRecord) -> Result<(), TrustError> {
        let mut conn = self.conn.lock().map_err(|_| TrustError::AuditUnavailable)?;
        self.append_audit(&mut conn, &record).map_err(|_| TrustError::AuditUnavailable)
    }
}

#[cfg(test)]
#[path = "approval_store_tests.rs"]
mod tests;
