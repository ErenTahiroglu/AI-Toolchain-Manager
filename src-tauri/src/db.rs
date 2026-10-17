//! SQLite ownership. The WebView never sees SQL; only typed commands reach this module.

use crate::error::{AppError, AppResult};
use rusqlite::{Connection, Transaction};
use std::path::Path;

/// One schema step. Each step runs in its own transaction together with the `user_version` bump,
/// so a failure rolls back completely and leaves the previous version intact.
pub enum Migration {
    Sql(&'static str),
    Rust(fn(&Transaction) -> AppResult<()>),
}

fn migrate_2(tx: &Transaction) -> AppResult<()> {
    tx.execute_batch(include_str!("../migrations/0002_catalog_registry.sql"))?;
    crate::catalog::migrate_phase1a_data(tx)
}

fn migrate_5(tx: &Transaction) -> AppResult<()> {
    tx.execute_batch(include_str!("../migrations/0005_discovery_privacy.sql"))?;
    crate::discovery::store::scrub_legacy_observations(tx)
}

fn migrate_7(tx: &Transaction) -> AppResult<()> {
    // AUTOINCREMENT's high-water mark must survive the rebuild so that an id is never reused.
    let seq: Option<i64> = tx.query_row("SELECT seq FROM sqlite_sequence WHERE name = 'discovery_observations'", [], |r| r.get(0)).ok();
    tx.execute_batch(include_str!("../migrations/0007_identity_taxonomy.sql"))?;
    if let Some(seq) = seq {
        tx.execute("DELETE FROM sqlite_sequence WHERE name = 'discovery_observations'", [])?;
        tx.execute("INSERT INTO sqlite_sequence (name, seq) VALUES ('discovery_observations', ?1)", [seq])?;
    }
    crate::discovery::store::reclassify_legacy_trust(tx)
}

/// Ordered, append-only list of migrations. Index + 1 == `PRAGMA user_version` after applying it.
/// 1 = Phase 1A foundation. 2 = Phase 1B registry (additive; copies Phase 1A data, then freezes the legacy tables).
/// 3 = Phase 1B closure: project-scoped suggestion log, immutable classification of user-added entries.
/// 4 = Phase 2A: local read-only discovery scans and observations (additive).
/// 5 = Phase 2A closure: `record_version` column and in-place privacy scrub of legacy observation rows.
/// 6 = Phase 2B: passive provenance ladder columns on discovery observations (additive).
/// 7 = Phase 2B closure: identity taxonomy (`record_corroborated`; `validated` reserved) and in-place lowering of
///     high-trust values stored by earlier builds. Table rebuild; every row and id is preserved.
const MIGRATIONS: &[Migration] = &[
    Migration::Sql(include_str!("../migrations/0001_init.sql")),
    Migration::Rust(migrate_2),
    Migration::Sql(include_str!("../migrations/0003_scoped_suggestion_log.sql")),
    Migration::Sql(include_str!("../migrations/0004_discovery.sql")),
    Migration::Rust(migrate_5),
    Migration::Sql(include_str!("../migrations/0006_passive_provenance.sql")),
    Migration::Rust(migrate_7),
];

pub const LATEST_VERSION: i64 = MIGRATIONS.len() as i64;

fn configure(conn: &Connection) -> AppResult<()> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    // Journal mode returns a row, so read it instead of using pragma_update.
    conn.query_row("PRAGMA journal_mode=WAL", [], |row| row.get::<_, String>(0))?;
    Ok(())
}

pub fn user_version(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

/// Apply pending migrations, each in its own transaction. Repeatable: re-running is a no-op.
/// A database from a NEWER app version is refused and left untouched (fail closed).
pub fn migrate(conn: &mut Connection) -> AppResult<()> {
    migrate_with(conn, MIGRATIONS)
}

pub fn migrate_with(conn: &mut Connection, migrations: &[Migration]) -> AppResult<()> {
    let current = user_version(conn)?;
    if current > migrations.len() as i64 {
        return Err(AppError::new(
            "DB_SCHEMA_TOO_NEW",
            "Veritabanı bu uygulamadan daha yeni bir sürümle oluşturulmuş; hiçbir şey değiştirilmedi.",
        ));
    }
    for (index, migration) in migrations.iter().enumerate() {
        let version = index as i64 + 1;
        if version <= current {
            continue;
        }
        let tx = conn.transaction()?;
        match migration {
            Migration::Sql(sql) => tx.execute_batch(sql)?,
            Migration::Rust(step) => step(&tx)?,
        }
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

/// Open (or create) the database file. When an existing database needs migrating,
/// a consistent snapshot is written to `backup_dir` first.
pub fn open_file(path: &Path, backup_dir: Option<&Path>) -> AppResult<Connection> {
    let open_failed = || AppError::new("DB_OPEN_FAILED", "Veritabanı dosyası açılamadı veya bozuk.");
    let mut conn = Connection::open(path).map_err(|_| open_failed())?;
    // A garbage file only fails on first use, so touch the schema here.
    let current = user_version(&conn).map_err(|_| open_failed())?;
    if current > LATEST_VERSION {
        // Refuse before configure(): switching journal mode would already modify the file.
        return Err(AppError::new(
            "DB_SCHEMA_TOO_NEW",
            "Veritabanı bu uygulamadan daha yeni bir sürümle oluşturulmuş; hiçbir şey değiştirilmedi.",
        ));
    }
    configure(&conn).map_err(|_| open_failed())?;

    if current > 0 && current < LATEST_VERSION {
        if let Some(dir) = backup_dir {
            std::fs::create_dir_all(dir).map_err(|_| AppError::new("DB_BACKUP_FAILED", "Geçiş öncesi yedek klasörü oluşturulamadı."))?;
            let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
            let target = dir.join(format!("atm-v{current}-{stamp}.sqlite"));
            conn.execute("VACUUM INTO ?1", [target.to_string_lossy().as_ref()])
                .map_err(|_| AppError::new("DB_BACKUP_FAILED", "Geçiş öncesi yedek alınamadı; geçiş yapılmadı."))?;
        }
    }
    // Migration 5 rewrites rows that may hold old personal text. With `secure_delete` the replaced bytes are overwritten
    // inside the database pages instead of lingering in free space, and a checkpoint + VACUUM afterwards rewrites the
    // file. This is LOGICAL compaction of this application's own database file: it does not reach pre-migration
    // backups, file-system snapshots, or the physical flash behind an SSD.
    let privacy_migration = current > 0 && current < 5;
    if privacy_migration {
        let _ = conn.pragma_update(None, "secure_delete", true);
    }
    migrate(&mut conn)?;
    if privacy_migration {
        compact(&conn);
    }
    Ok(conn)
}

/// Best effort: a failure here never fails the start-up (the data is already correct; only free space is untidy).
pub(crate) fn compact(conn: &Connection) {
    let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    let _ = conn.execute_batch("VACUUM");
    let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
}

#[cfg(test)]
pub fn open_memory() -> AppResult<Connection> {
    let mut conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", true)?;
    migrate(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_reaches_latest_version() {
        let conn = open_memory().unwrap();
        assert_eq!(user_version(&conn).unwrap(), LATEST_VERSION);
    }

    #[test]
    fn migration_is_repeatable_on_a_file_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.sqlite");
        for _ in 0..3 {
            let conn = open_file(&path, None).unwrap();
            assert_eq!(user_version(&conn).unwrap(), LATEST_VERSION);
            let tables: i64 =
                conn.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='tools'", [], |r| r.get(0)).unwrap();
            assert_eq!(tables, 1);
        }
    }

    #[test]
    fn newer_database_is_refused_and_left_byte_identical() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("future.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE marker(x); PRAGMA user_version = 999;").unwrap();
            conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())).ok();
        }
        let before = std::fs::read(&path).unwrap();
        let err = open_file(&path, None).unwrap_err();
        assert_eq!(err.code, "DB_SCHEMA_TOO_NEW");
        // Opening in WAL mode must not have been attempted before the version check.
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn corrupt_file_reports_an_error_instead_of_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("garbage.sqlite");
        std::fs::write(&path, vec![0x41u8; 4096]).unwrap();
        let err = open_file(&path, None).unwrap_err();
        assert_eq!(err.code, "DB_OPEN_FAILED");
    }

    #[test]
    fn failed_migration_rolls_back_completely() {
        let mut conn = Connection::open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let result = tx.execute_batch("CREATE TABLE ok_table(x); CREATE TABLE ok_table(x);");
        assert!(result.is_err());
        drop(tx); // rollback
        let n: i64 = conn.query_row("SELECT count(*) FROM sqlite_master WHERE name='ok_table'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }
}

#[cfg(test)]
mod migration_tests;
