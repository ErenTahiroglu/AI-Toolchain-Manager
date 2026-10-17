//! Structured, redacted, bounded local diagnostic records. Never sent anywhere.

use crate::{audit::now_utc, error::AppResult, redact};
use rusqlite::Connection;
use serde::Serialize;

const MAX_ROWS: i64 = 1000;
const MAX_DETAIL_CHARS: usize = 300;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticRow {
    pub id: i64,
    pub ts: String,
    pub level: String,
    pub component: String,
    pub code: String,
    pub detail: String,
}

/// `component` and `code` must be fixed identifiers chosen by the caller; only `detail` is free text.
pub fn log(conn: &Connection, level: &str, component: &str, code: &str, detail: &str) -> AppResult<()> {
    debug_assert!(matches!(level, "info" | "warn" | "error"));
    conn.execute(
        "INSERT INTO diagnostics (ts, level, component, code, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![now_utc(), level, component, code, redact::sanitize(detail, MAX_DETAIL_CHARS)],
    )?;
    conn.execute("DELETE FROM diagnostics WHERE id <= (SELECT max(id) FROM diagnostics) - ?1", [MAX_ROWS])?;
    Ok(())
}

pub fn list(conn: &Connection, limit: i64) -> AppResult<Vec<DiagnosticRow>> {
    let mut stmt = conn.prepare("SELECT id, ts, level, component, code, detail FROM diagnostics ORDER BY id DESC LIMIT ?1")?;
    let rows = stmt.query_map([limit.clamp(1, 500)], |r| {
        Ok(DiagnosticRow { id: r.get(0)?, ts: r.get(1)?, level: r.get(2)?, component: r.get(3)?, code: r.get(4)?, detail: r.get(5)? })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_are_redacted_before_storage() {
        let conn = crate::db::open_memory().unwrap();
        log(&conn, "error", "test", "T", "boom C:\\Users\\dummy\\x.txt token=DUMMYSECRET123").unwrap();
        let rows = list(&conn, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].detail.contains("dummy"));
        assert!(!rows[0].detail.contains("DUMMYSECRET123"));
        let raw: String = conn.query_row("SELECT group_concat(detail) FROM diagnostics", [], |r| r.get(0)).unwrap();
        assert!(!raw.contains("DUMMYSECRET123"));
    }

    #[test]
    fn table_is_bounded() {
        let conn = crate::db::open_memory().unwrap();
        for i in 0..(MAX_ROWS + 50) {
            log(&conn, "info", "t", "T", &format!("n{i}")).unwrap();
        }
        let n: i64 = conn.query_row("SELECT count(*) FROM diagnostics", [], |r| r.get(0)).unwrap();
        assert!(n <= MAX_ROWS);
    }
}
