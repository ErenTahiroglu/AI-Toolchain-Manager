//! CONFIG-02C (Issue #40): the READ-ONLY file-system half of the single-file `.mcp.json` snapshot.
//!
//! A child of `managed_fs` on purpose: it re-uses that module's handle-relative, no-follow primitives (`NtCreateFile` / `openat`) and needs
//! the one thing `Dir` refuses -- the exact name `.mcp.json` (`valid_name` rejects a leading dot). That name is a COMPILE-TIME CONSTANT here.
//! No function in this module takes a file name, a relative path or a path inside the root; the only text that ever becomes a path is the
//! project root, and `mcp_read.rs` has already checked it.
//!
//! What this module does, in order (contract `docs/config-02c/READ_ONLY_CONTRACT.md` section 5):
//! 1. opens the root by walking its chain without following any link (ancestor METADATA is touched, ancestor contents are not read);
//! 2. lists the root's entry names in memory, only to refuse a case-variant spelling of the constant name; the names are dropped at once;
//! 3. looks at the constant name without following it: absent, a link, not a regular file, or more than one hard link are refusals;
//! 4. opens it relative to the root handle, re-reads kind, identity and link count from the opened handle;
//! 5. reads at most 1 MiB + 1 bytes, then re-reads identity, size and last-write time, reads a SECOND time and compares the bytes, and finally
//!    looks at the name once more and lists the root's names once more (the exact spelling must still be there and no other spelling of it);
//!    any difference is `Changed` (or a case alias) and NOTHING of the first reading is returned.
//!
//! The root is opened through `Dir::open_anchor_readonly`: on Windows the final folder is opened without the add-file / add-subdirectory
//! rights that the writer paths request, so a project folder the user can read but not write is read like any other.
//!
//! It contains no call that creates, writes, deletes, moves or re-names anything, holds no lock, opens no connection and starts no process.
//! It keeps nothing after it returns: the bytes go to the caller, the handles are closed, no cache or token survives.
//!
//! Residual (documented, not hidden): a writer that replaces the content inside the window and restores size and last-write time cannot be
//! detected. The digest of a result describes the bytes read at that moment.
#![allow(dead_code)]

use super::{hook, sha256_hex, Dir, FileH, FsError, Kind, MAX_LIST_ENTRIES};
use std::path::Path;

/// The only file this module can address.
pub const TARGET: &str = ".mcp.json";
/// The most bytes that are accepted (1 MiB). A larger file is `TooLarge`, never truncated.
pub const MAX_BYTES: u64 = 1_048_576;

/// Where in the sequence a failure happened (the mapping to a user-facing status depends on it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Root,
    Listing,
    Entry,
    Open,
    Read,
}

/// Why nothing was returned. `Fs` carries the existing handle-layer vocabulary unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    Fs(FsError),
    /// The root was opened and no entry with the exact constant name is there.
    Absent,
    HardLinked,
    CaseAlias,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failure {
    pub stage: Stage,
    pub fault: Fault,
}

fn fail(stage: Stage, e: FsError) -> Failure {
    Failure { stage, fault: Fault::Fs(e) }
}

/// What was read: the exact bytes, their length and SHA-256. Never a path, never an identity.
pub struct Observed {
    pub size: u64,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

/// One bounded, consistent reading of `<root>/.mcp.json`. `root` must already have passed the text checks of `mcp_read::root_from_text`.
pub fn read(root: &Path) -> Result<Observed, Failure> {
    hook::fire("mcpread:before-root");
    let dir = Dir::open_anchor_readonly(root).map_err(|e| fail(Stage::Root, e))?;
    hook::fire("mcpread:root-opened");

    // The names are compared and dropped here; they are never returned, shown, logged or stored.
    let ambiguous = {
        let names = dir.imp.list(MAX_LIST_ENTRIES).map_err(|e| fail(Stage::Listing, e))?;
        names.iter().any(|n| n != TARGET && n.eq_ignore_ascii_case(TARGET))
    };
    if ambiguous {
        return Err(Failure { stage: Stage::Listing, fault: Fault::CaseAlias });
    }
    hook::fire("mcpread:listed");

    let Some(named) = dir.imp.stat_name(TARGET).map_err(|e| fail(Stage::Entry, e))? else {
        return Err(Failure { stage: Stage::Entry, fault: Fault::Absent });
    };
    match named.kind {
        Kind::File => {}
        Kind::Link => return Err(fail(Stage::Entry, FsError::Link)),
        _ => return Err(fail(Stage::Entry, FsError::NotFile)),
    }
    if named.links != 1 {
        return Err(Failure { stage: Stage::Entry, fault: Fault::HardLinked });
    }
    hook::fire("mcpread:statted");

    let mut file = FileH { file: dir.imp.open_file(TARGET).map_err(|e| fail(Stage::Open, e))? };
    let opened = file.stat().map_err(|e| fail(Stage::Open, e))?;
    if opened.kind != Kind::File || opened.identity != named.identity || opened.links != 1 {
        return Err(fail(Stage::Open, FsError::Changed));
    }
    let stamp = file.modified().map_err(|e| fail(Stage::Open, e))?;
    hook::fire("mcpread:opened");

    let first = file.read_bounded(MAX_BYTES).map_err(|e| fail(Stage::Read, e))?;
    hook::fire("mcpread:first-read");
    let unchanged = |file: &FileH, len: usize| -> Result<(), Failure> {
        let now = file.stat().map_err(|e| fail(Stage::Read, e))?;
        let when = file.modified().map_err(|e| fail(Stage::Read, e))?;
        if now.kind != Kind::File || now.identity != opened.identity || now.links != 1 || now.size != len as u64 || when != stamp {
            return Err(fail(Stage::Read, FsError::Changed));
        }
        Ok(())
    };
    unchanged(&file, first.len())?;
    hook::fire("mcpread:before-second-read");

    let second = file.read_bounded(MAX_BYTES).map_err(|e| fail(Stage::Read, e))?;
    hook::fire("mcpread:second-read");
    if second != first {
        return Err(fail(Stage::Read, FsError::Changed));
    }
    unchanged(&file, second.len())?;

    hook::fire("mcpread:before-final-stat");
    match dir.imp.stat_name(TARGET).map_err(|e| fail(Stage::Read, e))? {
        Some(s) if s.kind == Kind::File && s.identity == opened.identity && s.links == 1 && s.size == first.len() as u64 => {}
        _ => return Err(fail(Stage::Read, FsError::Changed)),
    }
    // Every look above matches the name case-insensitively (Windows `OBJ_CASE_INSENSITIVE`, a case-insensitive APFS volume). A change of only
    // the spelling after the first listing would therefore pass the identity checks. The spelling on disk is read once more, names only.
    spelling_is_exact(&dir)?;

    Ok(Observed { size: first.len() as u64, sha256: sha256_hex(&first), bytes: first })
}

/// The names currently in the root, compared and dropped: the exact constant must be there, and no other spelling of it may be.
fn spelling_is_exact(dir: &Dir) -> Result<(), Failure> {
    let names = dir.imp.list(MAX_LIST_ENTRIES).map_err(|e| fail(Stage::Listing, e))?;
    if names.iter().any(|n| n != TARGET && n.eq_ignore_ascii_case(TARGET)) {
        return Err(Failure { stage: Stage::Listing, fault: Fault::CaseAlias });
    }
    if !names.iter().any(|n| n == TARGET) {
        return Err(fail(Stage::Read, FsError::Changed));
    }
    Ok(())
}

#[cfg(test)]
#[path = "managed_fs_read_tests.rs"]
mod tests;
