//! Phase 3C: the production-oriented, handle-relative file-system layer (Issue #14).
//!
//! NOTHING in the application calls this module yet: it is exercised by tests only, behind the isolated test-mode adapter
//! (`adapter.rs`). It does not wrap the synthetic engine (`lifecycle.rs`), which is path-based.
//!
//! # What it guarantees (and on which platform)
//!
//! * **Handle-relative, no-follow traversal.** The anchor (the application data directory) is opened by walking its chain from the
//!   volume root, every component opened relative to the previous handle: `NtCreateFile` with `RootDirectory` and
//!   `FILE_OPEN_REPARSE_POINT` on Windows, `openat(O_NOFOLLOW | O_DIRECTORY)` on macOS. Below the anchor every operation is
//!   relative to a held directory handle; no full path is ever resolved again.
//! * **Links are never followed.** A symlink, junction, mount point or any other reparse point is reported as [`Kind::Link`] /
//!   [`FsError::Link`] and refused.
//! * **Mount transitions.** Every directory below the anchor must be on the same volume (volume serial / `st_dev`) as the anchor;
//!   network file systems are refused at the anchor. (Ancestors of the anchor may legitimately cross volumes: macOS firmlinks.)
//! * **Exclusive creation, identity from the open handle.** Files and folders are created with `FILE_CREATE` / `O_EXCL`;
//!   the identity (volume + file index / `st_dev` + `st_ino`) and link count are read from the handle that was created.
//! * **Verified deletion.** [`Dir::remove_file_verified`] opens the file, checks kind, identity, link count (must be 1), size and
//!   SHA-256 THROUGH THE OPEN HANDLE and deletes. Windows deletes through that same handle (`FileDispositionInformationEx`), so the
//!   object deleted is exactly the one verified. macOS has no inode-based unlink: it confirms `(st_dev, st_ino)` with `fstatat`
//!   immediately before `unlinkat`; a window of microseconds remains (documented in `docs/phase-3c`). No recursive deletion exists.
//! * **Owner-only root.** The managed root is created with a protected DACL granting only the owner (`D:P(A;OICI;FA;;;OW)`) on
//!   Windows and mode `0700` on macOS, and the result is read back and verified.
//! * **Bounded.** Reads and hashes have explicit limits; directory listings are bounded; names are a strict ASCII subset.
//! * **Durability.** Files are fully flushed (`FlushFileBuffers` / `F_FULLFSYNC`); directory flushes are best effort and reported.
//!
//! What it cannot guarantee: a same-user process can still win a race between the last identity check and the unlink on macOS; it
//! can also rewrite the manager's own database. See `docs/phase-3c/FILESYSTEM-GUARANTEES.md`.
#![allow(dead_code)] // test-mode adapter only; no command reaches this module

use std::{io::Read, path::Path};

pub const MAX_NAME_LEN: usize = 100;
pub const MAX_LIST_ENTRIES: usize = 4096;
pub const MAX_HASH_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Identity {
    pub volume: u64,
    pub object: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    /// A symbolic link, junction, mount point or other reparse point.
    Link,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    pub kind: Kind,
    pub identity: Identity,
    pub links: u64,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    Exists,
    Link,
    NotDir,
    NotFile,
    CrossDevice,
    Network,
    Denied,
    Locked,
    TooLarge,
    InvalidName,
    NotEmpty,
    /// The object is not (or no longer) the one that was expected.
    Changed,
    Unsupported,
    Io,
}

impl FsError {
    pub fn code(self) -> &'static str {
        match self {
            FsError::NotFound => "not_found",
            FsError::Exists => "exists",
            FsError::Link => "link_refused",
            FsError::NotDir => "not_a_directory",
            FsError::NotFile => "not_a_regular_file",
            FsError::CrossDevice => "other_volume",
            FsError::Network => "network_or_unknown_filesystem",
            FsError::Denied => "access_denied",
            FsError::Locked => "locked_by_another_process",
            FsError::TooLarge => "too_large",
            FsError::InvalidName => "invalid_name",
            FsError::NotEmpty => "not_empty",
            FsError::Changed => "changed",
            FsError::Unsupported => "unsupported",
            FsError::Io => "io",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceOutcome {
    Done,
    /// The replacement happened, but the displaced old file could not be removed and is still present under the source name.
    DisplacedKept,
}

/// What a file must be to be deleted by [`Dir::remove_file_verified`] (the link count must always be exactly 1).
#[derive(Debug, Clone, Default)]
pub struct Expect {
    pub identity: Option<Identity>,
    pub size: Option<u64>,
    pub sha256: Option<String>,
}

/// A single name: ASCII letters, digits and `. _ - +`, starting with a letter or digit, no `..`, no trailing dot, not a Windows
/// device name, at most [`MAX_NAME_LEN`] bytes.
pub fn valid_name(c: &str) -> bool {
    let b = c.as_bytes();
    if b.is_empty() || b.len() > MAX_NAME_LEN || !b[0].is_ascii_alphanumeric() || c.contains("..") || c.ends_with('.') {
        return false;
    }
    if !b.iter().all(|x| x.is_ascii_alphanumeric() || matches!(x, b'.' | b'_' | b'-' | b'+')) {
        return false;
    }
    let stem = c.split('.').next().unwrap_or("").to_ascii_uppercase();
    !(matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Test-only seams: simulate a change at a precise moment, or a different volume.
#[cfg(test)]
pub(crate) mod hook {
    use super::Identity;
    use std::cell::RefCell;

    type Hook = Box<dyn FnMut(&str)>;
    type VolumeHook = Box<dyn Fn(u64) -> Option<u64>>;
    thread_local! {
        static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
        static VOLUME: RefCell<Option<VolumeHook>> = const { RefCell::new(None) };
    }

    pub fn set(f: impl FnMut(&str) + 'static) {
        HOOK.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }

    pub fn clear() {
        HOOK.with(|h| *h.borrow_mut() = None);
        VOLUME.with(|h| *h.borrow_mut() = None);
    }

    /// Pretend the directory with this object id lives on another volume.
    pub fn set_volume(f: impl Fn(u64) -> Option<u64> + 'static) {
        VOLUME.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }

    pub fn volume_of(id: Identity) -> u64 {
        VOLUME.with(|h| h.borrow().as_ref().and_then(|f| f(id.object))).unwrap_or(id.volume)
    }

    thread_local! {
        static UNSUPPORTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// Pretend the volume offers no atomic no-clobber primitive: every `move_file` / `replace_owned_file` then fails `Unsupported`.
    pub fn set_unsupported(on: bool) {
        UNSUPPORTED.with(|u| u.set(on));
    }

    pub fn unsupported() -> bool {
        UNSUPPORTED.with(|u| u.get())
    }

    pub fn fire(stage: &str) {
        if let Some(mut f) = HOOK.with(|h| h.borrow_mut().take()) {
            f(stage);
            HOOK.with(|h| *h.borrow_mut() = Some(f));
        }
    }
}

#[cfg(not(test))]
pub(crate) mod hook {
    use super::Identity;

    #[inline(always)]
    pub fn fire(_stage: &str) {}

    #[inline(always)]
    pub fn volume_of(id: Identity) -> u64 {
        id.volume
    }

    #[inline(always)]
    pub fn unsupported() -> bool {
        false
    }
}

/// The judgement behind every verified deletion: a plain file with exactly one link, and (when given) the expected identity, size and
/// SHA-256 -- all read from the open handle.
fn verify_expect(file: &mut std::fs::File, stat: &Stat, expect: &Expect) -> Result<(), FsError> {
    if stat.kind != Kind::File {
        return Err(FsError::NotFile);
    }
    if stat.links != 1 {
        return Err(FsError::Changed);
    }
    if expect.identity.is_some_and(|i| i != stat.identity) {
        return Err(FsError::Changed);
    }
    if expect.size.is_some_and(|s| s != stat.size) {
        return Err(FsError::Changed);
    }
    if let Some(want) = &expect.sha256 {
        let mut h = FileH { file: file.try_clone().map_err(|e| imp::map_io(&e))? };
        let (got, _) = h.sha256_bounded(MAX_HASH_BYTES)?;
        if got != *want {
            return Err(FsError::Changed);
        }
    }
    Ok(())
}

/// An open file (regular, created or opened without following links), with bounded helpers.
pub struct FileH {
    file: std::fs::File,
}

impl FileH {
    pub fn stat(&self) -> Result<Stat, FsError> {
        imp::file_stat(&self.file)
    }

    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), FsError> {
        use std::io::Write;
        self.file.write_all(bytes).map_err(|e| imp::map_io(&e))
    }

    /// The last-write time the operating system reports for this open handle. Only ever compared with a later reading of the same handle.
    pub fn modified(&self) -> Result<std::time::SystemTime, FsError> {
        self.file.metadata().and_then(|m| m.modified()).map_err(|e| imp::map_io(&e))
    }

    /// Full flush to stable storage (`FlushFileBuffers` / `F_FULLFSYNC`).
    pub fn sync_all(&self) -> Result<(), FsError> {
        imp::sync_file(&self.file)
    }

    /// At most `limit` bytes; a larger file is [`FsError::TooLarge`], never truncated silently.
    pub fn read_bounded(&mut self, limit: u64) -> Result<Vec<u8>, FsError> {
        use std::io::{Seek, SeekFrom};
        self.file.seek(SeekFrom::Start(0)).map_err(|e| imp::map_io(&e))?;
        let mut out = Vec::new();
        (&mut self.file).take(limit + 1).read_to_end(&mut out).map_err(|e| imp::map_io(&e))?;
        if out.len() as u64 > limit {
            return Err(FsError::TooLarge);
        }
        Ok(out)
    }

    /// SHA-256 and size of the content, read in chunks from the start through this handle.
    pub fn sha256_bounded(&mut self, limit: u64) -> Result<(String, u64), FsError> {
        use sha2::{Digest, Sha256};
        use std::io::{Seek, SeekFrom};
        self.file.seek(SeekFrom::Start(0)).map_err(|e| imp::map_io(&e))?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let n = self.file.read(&mut buf).map_err(|e| imp::map_io(&e))?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > limit {
                return Err(FsError::TooLarge);
            }
            hasher.update(&buf[..n]);
        }
        Ok((hasher.finalize().iter().map(|b| format!("{b:02x}")).collect(), total))
    }
}

/// A directory held open. All operations are relative to it; it is never resolved by path again.
pub struct Dir {
    imp: imp::DirImp,
}

impl Dir {
    /// Opens the anchor (an absolute path of a local volume) by walking its chain component by component without following any
    /// link. Network volumes and unknown file systems are refused.
    pub fn open_anchor(path: &Path) -> Result<Dir, FsError> {
        Ok(Dir { imp: imp::open_anchor(path)? })
    }

    pub fn identity(&self) -> Identity {
        self.imp.identity
    }

    pub fn child(&self, name: &str) -> Result<Dir, FsError> {
        if !valid_name(name) {
            return Err(FsError::InvalidName);
        }
        let d = self.imp.open_child_dir(name)?;
        Ok(Dir { imp: d })
    }

    pub fn create_child(&self, name: &str) -> Result<Dir, FsError> {
        if !valid_name(name) {
            return Err(FsError::InvalidName);
        }
        Ok(Dir { imp: self.imp.create_child_dir(name, false)? })
    }

    /// Creates a folder that only the current user can use (protected owner-only DACL / mode 0700) and verifies the result.
    pub fn create_owner_only_child(&self, name: &str) -> Result<Dir, FsError> {
        if !valid_name(name) {
            return Err(FsError::InvalidName);
        }
        let d = self.imp.create_child_dir(name, true)?;
        d.verify_owner_only()?;
        Ok(Dir { imp: d })
    }

    /// Re-reads the permissions of this folder and confirms they are owner-only.
    pub fn is_owner_only(&self) -> bool {
        self.imp.verify_owner_only().is_ok()
    }

    pub fn create_file(&self, name: &str) -> Result<FileH, FsError> {
        if !valid_name(name) {
            return Err(FsError::InvalidName);
        }
        Ok(FileH { file: self.imp.create_file(name)? })
    }

    /// Opens an existing regular file for reading (never a link; never a folder).
    pub fn open_file(&self, name: &str) -> Result<FileH, FsError> {
        if !valid_name(name) {
            return Err(FsError::InvalidName);
        }
        Ok(FileH { file: self.imp.open_file(name)? })
    }

    /// Describes the entry itself without following it. `Ok(None)`: nothing is there.
    pub fn stat(&self, name: &str) -> Result<Option<Stat>, FsError> {
        if !valid_name(name) {
            return Err(FsError::InvalidName);
        }
        self.imp.stat_name(name)
    }

    /// The entries of this folder (bounded). Names that are not valid names are reported as `("<invalid>", Other)`.
    pub fn list(&self) -> Result<Vec<(String, Kind)>, FsError> {
        let names = self.imp.list(MAX_LIST_ENTRIES)?;
        let mut out = Vec::with_capacity(names.len());
        for n in names {
            if !valid_name(&n) {
                out.push(("<invalid>".to_string(), Kind::Other));
                continue;
            }
            // (an entry that vanished between listing and stat is simply not reported)
            if let Some(s) = self.imp.stat_name(&n)? {
                out.push((n, s.kind));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    /// Looks at a plain file WITHOUT changing anything: the same judgement as [`Dir::remove_file_verified`], made through an open
    /// handle. Used to validate a whole set of files before any of them is deleted.
    pub fn check_file(&self, name: &str, expect: &Expect) -> Result<Stat, FsError> {
        let mut f = self.open_file(name)?;
        let stat = f.stat()?;
        verify_expect(&mut f.file, &stat, expect)?;
        Ok(stat)
    }

    /// Opens, reads (bounded) and closes a plain file.
    pub fn read_file(&self, name: &str, limit: u64) -> Result<Vec<u8>, FsError> {
        self.open_file(name)?.read_bounded(limit)
    }

    /// Deletes `name` only if it is a plain file that matches `expect`, checked through the very handle used for the deletion.
    /// Windows deletes through that handle; macOS re-confirms `(st_dev, st_ino)` right before `unlinkat`.
    pub fn remove_file_verified(&self, name: &str, expect: &Expect) -> Result<(), FsError> {
        if !valid_name(name) {
            return Err(FsError::InvalidName);
        }
        self.imp.remove_file_verified(name, &|file, stat| {
            let mut dup = file.try_clone().map_err(|e| imp::map_io(&e))?;
            verify_expect(&mut dup, stat, expect)
        })
    }

    /// Removes an EMPTY folder (never recursive; a link is refused, not followed). The caller must not hold an open `Dir` for the
    /// folder being removed (on Windows that handle would block the removal: reported as `Locked`).
    pub fn remove_dir(&self, name: &str) -> Result<(), FsError> {
        if !valid_name(name) {
            return Err(FsError::InvalidName);
        }
        self.imp.remove_empty_dir(name)
    }

    /// Moves a plain file into `to_dir` under `to` and REFUSES (with [`FsError::Exists`]) if anything at all already exists there --
    /// a file, a folder, a link, a special entry. The refusal is made by the operating system atomically with the move
    /// (`FILE_RENAME_INFORMATION_EX` without the replace flag on Windows, `renameatx_np(RENAME_EXCL)` on macOS), never by a
    /// preliminary check alone. Where the platform cannot do that the result is [`FsError::Unsupported`]: there is no fallback to an
    /// ordinary rename. If `expect` is given, the file being moved must have that identity.
    pub fn move_file(&self, from: &str, to_dir: &Dir, to: &str, expect: Option<Identity>) -> Result<(), FsError> {
        if !valid_name(from) || !valid_name(to) {
            return Err(FsError::InvalidName);
        }
        if hook::unsupported() {
            return Err(FsError::Unsupported);
        }
        if hook::volume_of(self.imp.identity) != hook::volume_of(to_dir.imp.identity) {
            return Err(FsError::CrossDevice);
        }
        self.imp.rename_file(from, &to_dir.imp, to, imp::Mode::NoReplace, expect).map(|_| ())
    }

    /// Replaces ONE specific, already-known file: the destination must be a plain single-link file with exactly the identity `dest`
    /// (the caller's own previous transaction-owned file, for example the active pointer it is switching), and the file moved in must
    /// have the identity `src`. Anything else at the destination -- another file, a link, a folder -- is refused and left untouched.
    /// This is the only way to overwrite anything; there is no general `replace` flag.
    ///
    /// Windows verifies the destination through an open handle immediately before the replacing rename. macOS exchanges the two
    /// names atomically (`RENAME_SWAP`), verifies what was displaced and, if it is not `dest`, swaps back. The displaced old file is
    /// then removed with a verified deletion; if that fails it stays where it is and [`ReplaceOutcome::DisplacedKept`] says so.
    pub fn replace_owned_file(&self, from: &str, to_dir: &Dir, to: &str, src: Identity, dest: Identity) -> Result<ReplaceOutcome, FsError> {
        if !valid_name(from) || !valid_name(to) {
            return Err(FsError::InvalidName);
        }
        if hook::unsupported() {
            return Err(FsError::Unsupported);
        }
        if hook::volume_of(self.imp.identity) != hook::volume_of(to_dir.imp.identity) {
            return Err(FsError::CrossDevice);
        }
        let remains = self.imp.rename_file(from, &to_dir.imp, to, imp::Mode::Replace { dest }, Some(src))?;
        if !remains {
            return Ok(ReplaceOutcome::Done);
        }
        match self.remove_file_verified(from, &Expect { identity: Some(dest), size: None, sha256: None }) {
            Ok(()) => Ok(ReplaceOutcome::Done),
            Err(_) => Ok(ReplaceOutcome::DisplacedKept),
        }
    }

    /// Proves, with scratch files in a new uniquely named folder below this one, that THIS volume supports everything the adapter
    /// depends on: exclusive creation, an atomic no-clobber move (`Exists` when the destination is taken, destination untouched) and
    /// the replacement of one known file. Anything else -- in particular [`FsError::Unsupported`] -- is a refusal: a volume that
    /// cannot do it is never used for managed state. It removes what it created and nothing else; a leftover after a crash is a
    /// folder named `capprobe-<random>` that is reported, never adopted.
    pub fn probe_atomic_rename(&self) -> Result<(), FsError> {
        let name = format!("capprobe-{}", &uuid::Uuid::new_v4().simple().to_string()[..16]);
        let d = self.create_child(&name)?;
        let write = |n: &str| -> Result<Identity, FsError> {
            let mut f = d.create_file(n)?;
            f.write_all(b"probe")?;
            f.sync_all()?;
            Ok(f.stat()?.identity)
        };
        let outcome = (|| {
            let (a, b) = (write("a.bin")?, write("b.bin")?);
            d.move_file("a.bin", &d, "c.bin", Some(a))?;
            if d.move_file("b.bin", &d, "c.bin", Some(b)) != Err(FsError::Exists) {
                return Err(FsError::Changed); // the destination was not protected
            }
            if d.stat("c.bin")?.map(|s| s.identity) != Some(a) {
                return Err(FsError::Changed);
            }
            d.replace_owned_file("b.bin", &d, "c.bin", b, a)?;
            if d.stat("c.bin")?.map(|s| s.identity) != Some(b) {
                return Err(FsError::Changed);
            }
            Ok(())
        })();
        for n in ["a.bin", "b.bin", "c.bin"] {
            let _ = d.remove_file_verified(n, &Expect::default());
        }
        drop(d);
        let _ = self.remove_dir(&name);
        outcome
    }

    /// The name of the file system this folder lives on (`ntfs`, `apfs`, ...), when the platform reports it. Used only to record on
    /// which kind of volume a real-device acceptance run happened.
    pub fn filesystem_name(&self) -> Option<String> {
        self.imp.filesystem_name()
    }

    /// Flushes the folder's entries (best effort: returns whether the platform accepted the request).
    pub fn sync(&self) -> bool {
        self.imp.sync()
    }
}

#[cfg(windows)]
#[path = "managed_fs_windows.rs"]
mod imp;

#[cfg(unix)]
#[path = "managed_fs_unix.rs"]
mod imp;

#[cfg(not(any(windows, unix)))]
compile_error!("managed_fs supports Windows and Unix (macOS) only");

#[cfg(test)]
#[path = "managed_fs_tests.rs"]
mod tests;

// CONFIG-02C (Issue #40): the READ-ONLY half, compiled in every build. A child of this module for the same reason as the 02B prototype below (it needs
// the private handle-relative layer and the exact name `.mcp.json`, which `valid_name` refuses), but it contains no write, create, remove or move call.
#[path = "managed_fs_read.rs"]
pub(crate) mod read_mcp;

// CONFIG-02B (Issue #39): a TEST-ONLY prototype of a one-file transaction on synthetic directories. It is a child of this module so that it can use the
// private handle-relative layer instead of copying it; it is not compiled in any shipping build and nothing can call it.
#[cfg(test)]
#[path = "managed_fs_mcp.rs"]
mod mcp_file;
