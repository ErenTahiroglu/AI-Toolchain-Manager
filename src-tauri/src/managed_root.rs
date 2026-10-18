//! Phase 3C: the manager-owned data root, with its identity recorded independently in the manager's own SQLite database.
//!
//! * The root is derived ONLY from [`AppDataAnchor`], which is built from the application's own resolved data directory. No
//!   command, IPC argument, recipe field or plan field can name a writable root, and a source-scan test asserts that no command
//!   module mentions this module.
//! * `establish` writes a `creating` row BEFORE the folder exists, creates the folder owner-only (read back and verified), reads its
//!   identity from the created folder and only then marks the row `active`. A crash in between leaves a `creating` row, which
//!   blocks adoption: an unfinished root is never taken over by guesswork.
//! * `open` requires the recorded identity to match the folder found, both folders on the way to be owner-only, and the root's top
//!   level to be exactly the fixed layout. A folder that merely carries a copied marker and the right names is never adopted: this
//!   module writes no marker at all.
//!
//! Limits (documented in `docs/phase-3c/FILESYSTEM-GUARANTEES.md`): the database is writable by the same user, so a same-user attacker
//! can rewrite both the folder and its record; macOS device numbers are not stable across mounts, so only the inode is compared
//! across restarts there (the device must equal the anchor's within one session).
#![allow(dead_code)] // test-mode adapter only; no command reaches this module

use crate::managed_fs::{Dir, FsError, Kind};
use rusqlite::{Connection, OptionalExtension};
use std::path::PathBuf;

pub const CONTAINER: &str = "atm-managed";
pub const LAYOUT: [&str; 8] = ["data", "journal", "locks", "receipts", "snapshots", "staging", "state", "versions"];

/// The application's own data directory, wrapped so that a root cannot be anchored anywhere else by accident. Only the start-up code
/// that resolved the data directory should construct it.
#[derive(Debug, Clone)]
pub struct AppDataAnchor(PathBuf);

impl AppDataAnchor {
    pub(crate) fn from_resolved_data_dir(dir: PathBuf) -> AppDataAnchor {
        AppDataAnchor(dir)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RootError {
    Fs(FsError),
    Db,
    /// A row says a root is being created, but it never became active: resolve by hand, nothing is adopted.
    Unfinished,
    /// An active root already exists for this computer: use `open`.
    AlreadyEstablished,
    NotRegistered,
    /// The folder found is not the folder the database recorded.
    IdentityMismatch,
    /// A folder on the way is not owner-only.
    Permissions,
    /// The root's top level is not exactly the fixed layout.
    Layout,
}

impl RootError {
    pub fn code(&self) -> &'static str {
        match self {
            RootError::Fs(e) => e.code(),
            RootError::Db => "database_error",
            RootError::Unfinished => "root_creation_unfinished",
            RootError::AlreadyEstablished => "root_already_established",
            RootError::NotRegistered => "root_not_registered",
            RootError::IdentityMismatch => "root_identity_mismatch",
            RootError::Permissions => "root_not_owner_only",
            RootError::Layout => "root_layout_unexpected",
        }
    }
}

impl From<FsError> for RootError {
    fn from(e: FsError) -> Self {
        RootError::Fs(e)
    }
}

impl From<rusqlite::Error> for RootError {
    fn from(_: rusqlite::Error) -> Self {
        RootError::Db
    }
}

pub struct ManagedRoot {
    dir: Dir,
    root_id: String,
    computer_id: String,
}

fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else {
        "macos"
    }
}

impl ManagedRoot {
    pub fn dir(&self) -> &Dir {
        &self.dir
    }

    pub fn root_id(&self) -> &str {
        &self.root_id
    }

    pub fn computer_id(&self) -> &str {
        &self.computer_id
    }

    pub fn establish(anchor: &AppDataAnchor, conn: &Connection, computer_id: &str, now: &str) -> Result<ManagedRoot, RootError> {
        let live: Option<String> = conn
            .query_row("SELECT state FROM managed_roots WHERE computer_id = ?1 AND state IN ('creating', 'active')", [computer_id], |r| {
                r.get(0)
            })
            .optional()?;
        match live.as_deref() {
            Some("creating") => return Err(RootError::Unfinished),
            Some(_) => return Err(RootError::AlreadyEstablished),
            None => {}
        }
        let root_id = uuid::Uuid::new_v4().simple().to_string();
        // intent first: the row exists before the folder does
        conn.execute(
            "INSERT INTO managed_roots (root_id, computer_id, platform, state, created_at) VALUES (?1, ?2, ?3, 'creating', ?4)",
            rusqlite::params![root_id, computer_id, platform(), now],
        )?;
        let top = Dir::open_anchor(&anchor.0)?;
        let container = match top.child(CONTAINER) {
            Ok(d) => d,
            Err(FsError::NotFound) => top.create_owner_only_child(CONTAINER)?,
            Err(e) => return Err(e.into()),
        };
        if !container.is_owner_only() {
            return Err(RootError::Permissions);
        }
        let root = container.create_owner_only_child(&format!("root-{root_id}"))?;
        for name in LAYOUT {
            root.create_child(name)?;
        }
        let id = root.identity();
        let changed = conn.execute(
            "UPDATE managed_roots SET volume_id = ?2, object_id = ?3, state = 'active', activated_at = ?4 WHERE root_id = ?1 AND state = 'creating'",
            rusqlite::params![root_id, id.volume.to_string(), id.object.to_string(), now],
        )?;
        if changed != 1 {
            return Err(RootError::Db);
        }
        Ok(ManagedRoot { dir: root, root_id, computer_id: computer_id.to_string() })
    }

    pub fn open(anchor: &AppDataAnchor, conn: &Connection, computer_id: &str) -> Result<ManagedRoot, RootError> {
        type Row = (String, String, Option<String>, Option<String>);
        let row: Option<Row> = conn
            .query_row(
                "SELECT root_id, state, volume_id, object_id FROM managed_roots WHERE computer_id = ?1 AND state IN ('creating', 'active')",
                [computer_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let Some((root_id, state, volume, object)) = row else { return Err(RootError::NotRegistered) };
        if state != "active" {
            return Err(RootError::Unfinished);
        }
        let (volume, object): (u64, u64) = match (volume.and_then(|v| v.parse().ok()), object.and_then(|o| o.parse().ok())) {
            (Some(v), Some(o)) => (v, o),
            _ => return Err(RootError::Db),
        };
        let top = Dir::open_anchor(&anchor.0)?;
        let container = top.child(CONTAINER)?;
        if !container.is_owner_only() {
            return Err(RootError::Permissions);
        }
        let root = container.child(&format!("root-{root_id}"))?;
        if !root.is_owner_only() {
            return Err(RootError::Permissions);
        }
        let id = root.identity();
        // Windows volume serials are stable; macOS device numbers are not (only the inode is compared across restarts)
        if id.object != object || (cfg!(windows) && id.volume != volume) {
            return Err(RootError::IdentityMismatch);
        }
        let listing = root.list()?;
        let names: Vec<&str> = listing.iter().map(|(n, _)| n.as_str()).collect();
        if names != LAYOUT || listing.iter().any(|(_, k)| *k != Kind::Dir) {
            return Err(RootError::Layout);
        }
        Ok(ManagedRoot { dir: root, root_id, computer_id: computer_id.to_string() })
    }
}

#[cfg(test)]
#[path = "managed_root_tests.rs"]
mod tests;
