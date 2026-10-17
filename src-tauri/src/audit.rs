//! Append-only, hash-chained audit events that conform to `schemas/audit-event.schema.json` (v2).
//!
//! Integrity model (Phase 0 §10): this detects accidental edits, truncation and reordering.
//! It is NOT tamper-proof against the same OS user, who can rewrite both records and hashes.

use crate::error::{AppError, AppResult};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

pub const POLICY_ID: &str = "atm-local-redaction";
pub const POLICY_VERSION: &str = "1";

pub struct NewEvent {
    pub event_type: &'static str,
    pub outcome: &'static str,
    pub check_id: &'static str,
    pub error_code: Option<&'static str>,
    pub verdict_source: &'static str,
    pub actor_type: &'static str,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChainReport {
    pub ok: bool,
    pub count: i64,
    pub head: Option<String>,
    pub problem: Option<String>,
}

/// Deterministic canonical JSON (sorted keys, compact). Written by hand so it never depends on
/// whether some crate in the dependency graph enabled serde_json's `preserve_order`.
pub fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> =
                keys.into_iter().map(|k| format!("{}:{}", serde_json::to_string(k).unwrap(), canonical_json(&map[k]))).collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(items) => format!("[{}]", items.iter().map(canonical_json).collect::<Vec<_>>().join(",")),
        other => serde_json::to_string(other).unwrap(),
    }
}

fn sha256_hex(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn without(map: &Map<String, Value>, omit: &[&str]) -> Value {
    Value::Object(map.iter().filter(|(k, _)| !omit.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect())
}

pub fn record_digest(record: &Map<String, Value>) -> String {
    sha256_hex(&canonical_json(&without(record, &["recordDigest"])))
}

fn payload_digest(record: &Map<String, Value>) -> String {
    sha256_hex(&canonical_json(&without(record, &["recordDigest", "redaction"])))
}

pub fn now_utc() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn append(conn: &Connection, event: &NewEvent) -> AppResult<Value> {
    let tx = conn.unchecked_transaction()?;
    let last: Option<(i64, String, String)> = tx
        .query_row("SELECT sequence, record_digest, record_json FROM audit_events ORDER BY id DESC LIMIT 1", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .optional()?;

    let (sequence, previous_hash, chain_id, segment_start) = match &last {
        None => (1, Value::Null, uuid::Uuid::new_v4().to_string(), true),
        Some((seq, digest, json_text)) => {
            let chain = serde_json::from_str::<Value>(json_text)?
                .get("chainId")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| AppError::new("AUDIT_CORRUPT", "Denetim kaydı okunamadı."))?;
            (seq + 1, Value::String(digest.clone()), chain, false)
        }
    };

    let mut record = Map::new();
    record.insert("schemaVersion".into(), json!(2));
    record.insert("eventId".into(), json!(uuid::Uuid::new_v4().to_string()));
    record.insert("chainId".into(), json!(chain_id));
    record.insert("segmentStart".into(), json!(segment_start));
    record.insert("sequence".into(), json!(sequence));
    record.insert("timestamp".into(), json!(now_utc()));
    record.insert("previousHash".into(), previous_hash);
    record.insert("traceId".into(), json!(uuid::Uuid::new_v4().to_string()));
    record.insert("eventType".into(), json!(event.event_type));
    record.insert("outcome".into(), json!(event.outcome));
    record.insert("targetId".into(), json!("unknown"));
    record.insert("checkId".into(), json!(event.check_id));
    record.insert("checkVersion".into(), json!("1"));
    record.insert("verdictSource".into(), json!(event.verdict_source));
    record.insert("actorType".into(), json!(event.actor_type));
    if let Some(code) = event.error_code {
        record.insert("errorCode".into(), json!(code));
    }
    let payload = payload_digest(&record);
    record.insert(
        "redaction".into(),
        json!({
            "status": "applied-before-persistence",
            "policyId": POLICY_ID,
            "policyVersion": POLICY_VERSION,
            "payloadDigest": payload,
        }),
    );
    let digest = record_digest(&record);
    record.insert("recordDigest".into(), json!(digest));

    let value = Value::Object(record);
    tx.execute(
        "INSERT INTO audit_events (sequence, record_digest, record_json) VALUES (?1, ?2, ?3)",
        rusqlite::params![sequence, digest, canonical_json(&value)],
    )?;
    tx.commit()?;
    Ok(value)
}

/// Newest first.
pub fn list(conn: &Connection, limit: i64) -> AppResult<Vec<Value>> {
    let mut stmt = conn.prepare("SELECT record_json FROM audit_events ORDER BY id DESC LIMIT ?1")?;
    let rows = stmt.query_map([limit.clamp(1, 500)], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(serde_json::from_str(&row?)?);
    }
    Ok(out)
}

/// Oldest first (used by the self-check export and the cross-language contract test).
pub fn list_ascending(conn: &Connection) -> AppResult<Vec<Value>> {
    let mut stmt = conn.prepare("SELECT record_json FROM audit_events ORDER BY id ASC")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(serde_json::from_str(&row?)?);
    }
    Ok(out)
}

pub fn verify(conn: &Connection) -> AppResult<ChainReport> {
    let mut stmt = conn.prepare("SELECT sequence, record_digest, record_json FROM audit_events ORDER BY id ASC")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?;

    let mut count = 0i64;
    let mut prev_digest: Option<String> = None;
    let mut chain_id: Option<String> = None;
    let fail = |count: i64, head: Option<String>, why: &str| ChainReport { ok: false, count, head, problem: Some(why.to_string()) };

    for row in rows {
        let (column_seq, column_digest, text) = row?;
        count += 1;
        let Ok(Value::Object(record)) = serde_json::from_str::<Value>(&text) else {
            return Ok(fail(count, prev_digest, &format!("Kayıt {count}: okunamıyor")));
        };
        let seq = record.get("sequence").and_then(Value::as_i64);
        if seq != Some(count) || column_seq != count {
            return Ok(fail(count, prev_digest, &format!("Kayıt {count}: sıra numarası boşluklu veya değiştirilmiş")));
        }
        let expected_prev = prev_digest.clone().map(Value::String).unwrap_or(Value::Null);
        if record.get("previousHash") != Some(&expected_prev) {
            return Ok(fail(count, prev_digest, &format!("Kayıt {count}: önceki kayıt bağlantısı bozuk")));
        }
        if record.get("segmentStart").and_then(Value::as_bool) != Some(count == 1) {
            return Ok(fail(count, prev_digest, &format!("Kayıt {count}: bölüm başlangıcı geçersiz")));
        }
        let this_chain = record.get("chainId").and_then(Value::as_str).map(str::to_string);
        if this_chain.is_none() || (chain_id.is_some() && chain_id != this_chain) {
            return Ok(fail(count, prev_digest, &format!("Kayıt {count}: zincir kimliği tutarsız")));
        }
        chain_id = this_chain;
        let stored = record.get("recordDigest").and_then(Value::as_str).unwrap_or_default();
        if stored != column_digest || stored != record_digest(&record) {
            return Ok(fail(count, prev_digest, &format!("Kayıt {count}: özet (hash) uyuşmuyor, kayıt değiştirilmiş olabilir")));
        }
        let stored_payload = record.get("redaction").and_then(|r| r.get("payloadDigest")).and_then(Value::as_str);
        if stored_payload != Some(payload_digest(&record).as_str()) {
            return Ok(fail(count, prev_digest, &format!("Kayıt {count}: içerik özeti uyuşmuyor")));
        }
        prev_digest = Some(stored.to_string());
    }
    Ok(ChainReport { ok: true, count, head: prev_digest, problem: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(check: &'static str) -> NewEvent {
        NewEvent {
            event_type: "health_check",
            outcome: "passed",
            check_id: check,
            error_code: None,
            verdict_source: "operational-health",
            actor_type: "manager",
        }
    }

    fn db_with_events(n: usize) -> Connection {
        let conn = crate::db::open_memory().unwrap();
        for _ in 0..n {
            append(&conn, &event("t")).unwrap();
        }
        conn
    }

    #[test]
    fn empty_chain_is_valid() {
        let conn = db_with_events(0);
        let report = verify(&conn).unwrap();
        assert!(report.ok && report.count == 0 && report.head.is_none());
    }

    #[test]
    fn chain_links_and_verifies() {
        let conn = db_with_events(4);
        let report = verify(&conn).unwrap();
        assert!(report.ok, "{report:?}");
        assert_eq!(report.count, 4);
        let events = list_ascending(&conn).unwrap();
        assert_eq!(events[0]["segmentStart"], json!(true));
        assert_eq!(events[0]["previousHash"], Value::Null);
        assert_eq!(events[1]["previousHash"], events[0]["recordDigest"]);
        assert_eq!(events[3]["sequence"], json!(4));
    }

    #[test]
    fn canonical_json_matches_python_reference_vector() {
        // Reference computed with: json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
        let value = json!({"b": [1, true, null, "ç\"x"], "a": {"z": 1, "y": "q"}});
        assert_eq!(canonical_json(&value), "{\"a\":{\"y\":\"q\",\"z\":1},\"b\":[1,true,null,\"ç\\\"x\"]}");
        assert_eq!(sha256_hex(&canonical_json(&value)), "a6e64129f4e9b7747b7d33ee03f352994c3e24b9e17f0abae65392220e6aed4d");
    }

    fn tamper(conn: &Connection, sql: &str) {
        // Simulates a same-user attacker/accident that bypasses the append-only triggers.
        conn.execute_batch("DROP TRIGGER audit_events_no_update; DROP TRIGGER audit_events_no_delete;").unwrap();
        conn.execute_batch(sql).unwrap();
    }

    #[test]
    fn append_only_triggers_block_normal_edits() {
        let conn = db_with_events(2);
        assert!(conn.execute("UPDATE audit_events SET record_json='{}' WHERE sequence=1", []).is_err());
        assert!(conn.execute("DELETE FROM audit_events WHERE sequence=1", []).is_err());
    }

    #[test]
    fn detects_modified_record() {
        let conn = db_with_events(3);
        tamper(&conn, "UPDATE audit_events SET record_json = replace(record_json, 'passed', 'failed') WHERE sequence = 2");
        let report = verify(&conn).unwrap();
        assert!(!report.ok);
        assert!(report.problem.unwrap().contains("Kayıt 2"));
    }

    #[test]
    fn detects_deleted_middle_record_and_truncated_head() {
        let conn = db_with_events(4);
        tamper(&conn, "DELETE FROM audit_events WHERE sequence = 2");
        assert!(!verify(&conn).unwrap().ok);

        let conn = db_with_events(3);
        tamper(&conn, "DELETE FROM audit_events WHERE sequence = 1");
        assert!(!verify(&conn).unwrap().ok);
    }

    #[test]
    fn detects_reordered_records() {
        let conn = db_with_events(3);
        tamper(
            &conn,
            "UPDATE audit_events SET sequence = 99 WHERE sequence = 2;
             UPDATE audit_events SET sequence = 2 WHERE sequence = 3;
             UPDATE audit_events SET sequence = 3 WHERE sequence = 99;",
        );
        assert!(!verify(&conn).unwrap().ok);
    }
}
