//! Allow-listed user settings. Unknown keys and values are rejected, never stored.

use crate::{
    audit::now_utc,
    error::{AppError, AppResult},
};
use rusqlite::{Connection, OptionalExtension};
use std::collections::BTreeMap;

/// (key, allowed values, default)
const PUBLIC: &[(&str, &[&str], &str)] = &[
    ("theme", &["system", "light", "dark"], "system"),
    ("close_to_tray", &["true", "false"], "true"),
    // Preference only. It is false until the user switches it on; the OS state is queried separately.
    ("startup_enabled", &["true", "false"], "false"),
];

pub fn get_all(conn: &Connection) -> AppResult<BTreeMap<String, String>> {
    let mut out: BTreeMap<String, String> = PUBLIC.iter().map(|(k, _, default)| (k.to_string(), default.to_string())).collect();
    let mut stmt = conn.prepare("SELECT key, value FROM settings WHERE key NOT LIKE '\\_%' ESCAPE '\\'")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (key, value) = row?;
        // A stored value that is no longer valid falls back to the default instead of propagating.
        if let Some((_, allowed, _)) = PUBLIC.iter().find(|(k, _, _)| *k == key) {
            if allowed.contains(&value.as_str()) {
                out.insert(key, value);
            }
        }
    }
    Ok(out)
}

pub fn get(conn: &Connection, key: &str) -> AppResult<String> {
    Ok(get_all(conn)?.remove(key).unwrap_or_default())
}

pub fn set(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    let Some((_, allowed, _)) = PUBLIC.iter().find(|(k, _, _)| *k == key) else {
        return Err(AppError::new("SETTING_UNKNOWN", "Bilinmeyen ayar anahtarı reddedildi."));
    };
    if !allowed.contains(&value) {
        return Err(AppError::new("SETTING_INVALID", "Bu ayar için geçersiz değer reddedildi."));
    }
    write(conn, key, value)
}

fn write(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        rusqlite::params![key, value, now_utc()],
    )?;
    Ok(())
}

/// Internal bookkeeping keys always start with `_` and are never exposed through `get_all`/`set`.
pub fn get_internal(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    debug_assert!(key.starts_with('_'));
    Ok(conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

pub fn set_internal(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    debug_assert!(key.starts_with('_'));
    write(conn, key, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let conn = crate::db::open_memory().unwrap();
        let all = get_all(&conn).unwrap();
        assert_eq!(all["theme"], "system");
        assert_eq!(all["startup_enabled"], "false", "startup must be opt-in");
    }

    #[test]
    fn rejects_unknown_keys_and_invalid_values() {
        let conn = crate::db::open_memory().unwrap();
        assert_eq!(set(&conn, "theme", "neon").unwrap_err().code, "SETTING_INVALID");
        assert_eq!(set(&conn, "_session_state", "clean").unwrap_err().code, "SETTING_UNKNOWN");
        assert_eq!(set(&conn, "evil", "1").unwrap_err().code, "SETTING_UNKNOWN");
        set(&conn, "theme", "dark").unwrap();
        assert_eq!(get(&conn, "theme").unwrap(), "dark");
    }

    #[test]
    fn internal_keys_are_hidden() {
        let conn = crate::db::open_memory().unwrap();
        set_internal(&conn, "_session_state", "running").unwrap();
        assert!(!get_all(&conn).unwrap().contains_key("_session_state"));
        assert_eq!(get_internal(&conn, "_session_state").unwrap().as_deref(), Some("running"));
    }
}
