//! The ONLY file-system access of the discovery engine. Read-only by construction: no write, create,
//! rename, delete or execute call exists in this module (a test scans the source for them).
//!
//! # Confinement model
//!
//! Every access starts from an absolute `root` (a PATH folder, the user's home, or the project folder the
//! user typed) and names files below it with fixed components. The whole chain from the volume root down to
//! the target is validated, not just the last component:
//!
//! * **Unix (macOS):** the chain is walked component by component with `openat(..., O_NOFOLLOW | O_DIRECTORY)`
//!   starting at `/`. A symbolic link anywhere (including in an ancestor) makes the walk fail, and because each
//!   step is relative to the directory handle already opened, there is no gap between "check" and "use" that an
//!   attacker could use to swap a folder for a link. File content is read from a descriptor opened with
//!   `O_NOFOLLOW`. Network file systems (smbfs, nfs, afpfs, webdav…) are refused.
//! * **Windows:** every ancestor must be a plain directory (no junction, symlink, mount point or other reparse
//!   point), the drive must not be remote, and UNC/device paths are refused. Because the standard library has no
//!   handle-relative open, the guarantee against a swap *after* that check is established differently: the
//!   file or folder is opened, and `GetFinalPathNameByHandle` on that very handle must name exactly the path we
//!   intended. If any ancestor was redirected in between, the final path differs and the handle is discarded
//!   before a single byte is read. 8.3 short names, case differences resolved by the OS, `subst` drives and
//!   mapped network drives all fail this comparison and are therefore refused (fail closed).
//!
//! Residual limitation (documented, not hidden): on Windows the *presence-only* checks (`walk` for PATH names,
//! plugin cache folders) validate the root by handle but stat the final component by path, so a concurrent writer
//! could swap that last component between the check and the `stat`; this can only change what a metadata query
//! sees, never what is read, and such a swap needs write access to the user's own folders.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

/// `~/.claude.json` can be large (history, caches); everything else is small.
pub const MAX_CONFIG_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_SMALL_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// A symbolic link, junction or other reparse point was found where a real file or directory was expected,
    /// or the path ends up somewhere other than where it says.
    Link,
    /// A network share, remote drive, network file system or device path.
    Network,
    /// The kind of file system could not be determined, or is not one known to be local. Treated like a network
    /// file system: nothing below it is read (fail closed).
    UnknownFilesystem,
    NotRegularFile,
    TooLarge,
    AccessDenied,
    NotUtf8,
    Io,
}

impl Skip {
    pub fn describe(self) -> &'static str {
        match self {
            Skip::Link => "yolun bir parçası sembolik bağlantı/junction olduğu veya başka bir yere yönlendiği için izlenmedi",
            Skip::Network => "ağ paylaşımı, uzak sürücü veya aygıt yolu olduğu için okunmadı",
            Skip::UnknownFilesystem => "dosya sistemi türü belirlenemedi veya yerel olduğu bilinmiyor; güvenlik için okunmadı",
            Skip::NotRegularFile => "beklenen türde normal dosya/klasör değil",
            Skip::TooLarge => "dosya güvenli boyut sınırından büyük",
            Skip::AccessDenied => "erişim izni yok",
            Skip::NotUtf8 => "metin olarak okunamadı",
            Skip::Io => "okunamadı",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalKind {
    File,
    Dir,
}

#[derive(Debug)]
pub enum Walk {
    Found {
        #[cfg_attr(not(windows), allow(dead_code))] // read by the Windows implementation only
        path: PathBuf,
        is_link: bool,
    },
    NotFound,
    Skipped(Skip),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReadOutcome {
    Text(String),
    NotFound,
    Skipped(Skip),
}

#[cfg(windows)]
pub fn is_link_like(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

/// True for paths that would touch the network (Windows UNC shares) or a device namespace.
pub fn is_network_or_device_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if cfg!(windows) {
        if let Some(rest) = s.strip_prefix(r"\\?\").or_else(|| s.strip_prefix("//?/")) {
            // \\?\C:\ is a local verbatim path; \\?\UNC\server\share is a network share.
            let mut chars = rest.chars();
            let drive = chars.next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false);
            return !(drive && chars.next() == Some(':'));
        }
        return s.starts_with(r"\\") || s.starts_with("//");
    }
    false
}

/// File-system type names (as reported by macOS `statfs`) that mean the data lives on another machine.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn is_network_fstype(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "smbfs" | "nfs" | "afpfs" | "webdav" | "cifs" | "ftp" | "autofs" | "ncpfs" | "fuse.sshfs" | "9p"
    )
}

/// File-system types known to be local storage. Anything not listed here (including FUSE/macFUSE, which can proxy a
/// remote machine) is refused: unknown means "do not read".
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn is_local_fstype(name: &str) -> bool {
    matches!(name.to_ascii_lowercase().as_str(), "apfs" | "hfs" | "msdos" | "exfat" | "ntfs" | "udf" | "cd9660" | "tmpfs" | "ufs" | "zfs")
}

/// The single decision point for a directory's file system. `Err(())` is a failed lookup and is NEVER read as local.
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn judge_fstype(name: Result<&str, ()>) -> Result<(), Skip> {
    match name {
        Err(()) => Err(Skip::UnknownFilesystem),
        Ok(n) if is_network_fstype(n) => Err(Skip::Network),
        Ok(n) if is_local_fstype(n) => Ok(()),
        Ok(_) => Err(Skip::UnknownFilesystem),
    }
}

fn valid_component(c: &str) -> bool {
    !c.is_empty() && c != "." && c != ".." && !c.contains(['/', '\\', '\0', ':'])
}

/// Test-only seam to simulate a file system change at a precise moment (between validation and use).
#[cfg(test)]
pub(crate) mod hook {
    use std::cell::RefCell;

    type Hook = Box<dyn FnMut(&str)>;
    thread_local! {
        static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    }

    pub fn set(f: impl FnMut(&str) + 'static) {
        HOOK.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }

    pub fn clear() {
        HOOK.with(|h| *h.borrow_mut() = None);
        FSTYPE.with(|h| *h.borrow_mut() = None);
    }

    type FsTypeHook = Box<dyn Fn(u64) -> Option<Result<String, ()>>>;
    thread_local! {
        static FSTYPE: RefCell<Option<FsTypeHook>> = const { RefCell::new(None) };
    }

    /// Simulates `fstatfs` for the directory with this inode: `Some(Ok(name))` is a reported type, `Some(Err(()))` a
    /// failed lookup, `None` leaves the real lookup in charge.
    #[cfg_attr(not(unix), allow(dead_code))]
    pub fn set_fstype(f: impl Fn(u64) -> Option<Result<String, ()>> + 'static) {
        FSTYPE.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }

    #[cfg_attr(not(unix), allow(dead_code))]
    pub fn fstype_override(inode: u64) -> Option<Result<String, ()>> {
        FSTYPE.with(|h| h.borrow().as_ref().and_then(|f| f(inode)))
    }

    pub fn fire(stage: &str) {
        // Take the hook out while it runs so it can safely touch the file system without re-entering itself.
        if let Some(mut f) = HOOK.with(|h| h.borrow_mut().take()) {
            f(stage);
            HOOK.with(|h| *h.borrow_mut() = Some(f));
        }
    }
}

#[cfg(not(test))]
mod hook {
    #[inline(always)]
    pub fn fire(_stage: &str) {}

    #[cfg_attr(not(unix), allow(dead_code))]
    #[inline(always)]
    pub fn fstype_override(_inode: u64) -> Option<Result<String, ()>> {
        None
    }
}

/// Walks `rel` below `root`, refusing links and redirections anywhere on the chain.
pub fn walk(root: &Path, rel: &[&str], final_kind: FinalKind, allow_final_link: bool) -> Walk {
    imp::walk(root, rel, final_kind, allow_final_link)
}

/// Reads one regular UTF-8 text file below `root` (links refused, size capped). Never writes.
pub fn read_text(root: &Path, rel: &[&str], max_bytes: u64) -> ReadOutcome {
    match imp::open_file(root, rel) {
        Ok(file) => finish_read(file, max_bytes),
        Err(OpenFail::NotFound) => ReadOutcome::NotFound,
        Err(OpenFail::Skip(s)) => ReadOutcome::Skipped(s),
    }
}

/// Reads at most `max_bytes` from the START of one regular file (no UTF-8 requirement). Used for file headers only.
pub fn read_prefix(root: &Path, rel: &[&str], max_bytes: u64) -> Result<Vec<u8>, ReadFail> {
    let file = match imp::open_file(root, rel) {
        Ok(f) => f,
        Err(OpenFail::NotFound) => return Err(ReadFail::NotFound),
        Err(OpenFail::Skip(s)) => return Err(ReadFail::Skipped(s)),
    };
    match file.metadata() {
        Ok(m) if m.is_file() => {}
        Ok(_) => return Err(ReadFail::Skipped(Skip::NotRegularFile)),
        Err(_) => return Err(ReadFail::Skipped(Skip::Io)),
    }
    let mut bytes = Vec::new();
    file.take(max_bytes).read_to_end(&mut bytes).map_err(|_| ReadFail::Skipped(Skip::Io))?;
    Ok(bytes)
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReadFail {
    NotFound,
    Skipped(Skip),
}

pub(crate) enum OpenFail {
    NotFound,
    Skip(Skip),
}

/// Is `rel` below `root` a real directory (not a link)?
pub fn dir_exists(root: &Path, rel: &[&str]) -> Walk {
    walk(root, rel, FinalKind::Dir, false)
}

fn finish_read(file: fs::File, max_bytes: u64) -> ReadOutcome {
    let meta = match file.metadata() {
        Ok(m) => m,
        Err(_) => return ReadOutcome::Skipped(Skip::Io),
    };
    if !meta.is_file() {
        return ReadOutcome::Skipped(Skip::NotRegularFile);
    }
    if meta.len() > max_bytes {
        return ReadOutcome::Skipped(Skip::TooLarge);
    }
    let mut bytes = Vec::new();
    // Read at most max+1 bytes so a file that grew after the size check is still caught.
    if file.take(max_bytes + 1).read_to_end(&mut bytes).is_err() {
        return ReadOutcome::Skipped(Skip::Io);
    }
    if bytes.len() as u64 > max_bytes {
        return ReadOutcome::Skipped(Skip::TooLarge);
    }
    match String::from_utf8(bytes) {
        Ok(text) => ReadOutcome::Text(text.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(text)),
        Err(_) => ReadOutcome::Skipped(Skip::NotUtf8),
    }
}

// ================================================================================================
// Windows
// ================================================================================================
#[cfg(windows)]
mod imp {
    use super::*;
    use std::{
        ffi::OsStr,
        os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    };
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetFinalPathNameByHandleW, GetLongPathNameW};

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_READ_ATTRIBUTES: u32 = 0x0080;
    const DRIVE_REMOTE: u32 = 4;

    fn skip_of(e: &std::io::Error) -> Skip {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            Skip::AccessDenied
        } else {
            Skip::Io
        }
    }

    /// `symlink_metadata` never follows the final component.
    fn stat(path: &Path) -> Result<Option<fs::Metadata>, Skip> {
        match fs::symlink_metadata(path) {
            Ok(m) => Ok(Some(m)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(skip_of(&e)),
        }
    }

    fn final_path(file: &fs::File) -> Option<String> {
        let handle = file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
        let mut buf = vec![0u16; 512];
        loop {
            // SAFETY: `handle` is a valid open handle owned by `file`; `buf` is writable for `buf.len()` u16s.
            let n = unsafe { GetFinalPathNameByHandleW(handle, buf.as_mut_ptr(), buf.len() as u32, 0) };
            if n == 0 {
                return None;
            }
            if (n as usize) < buf.len() {
                buf.truncate(n as usize);
                return Some(String::from_utf16_lossy(&buf));
            }
            buf.resize(n as usize + 1, 0);
        }
    }

    fn normalise(s: &str) -> String {
        s.strip_prefix(r"\\?\").unwrap_or(s).replace('/', "\\").trim_end_matches('\\').to_lowercase()
    }

    /// Expands 8.3 short names (`RUNNER~1`) without following any link: `GetLongPathNameW` only rewrites names.
    fn long_form(path: &Path) -> Option<String> {
        let wide: Vec<u16> = OsStr::new(path).encode_wide().chain(std::iter::once(0)).collect();
        let mut buf = vec![0u16; 512];
        loop {
            // SAFETY: `wide` is NUL-terminated and outlives the call; `buf` is writable for `buf.len()` u16s.
            let n = unsafe { GetLongPathNameW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
            if n == 0 {
                return None;
            }
            if (n as usize) < buf.len() {
                buf.truncate(n as usize);
                return Some(String::from_utf16_lossy(&buf));
            }
            buf.resize(n as usize + 1, 0);
        }
    }

    /// Does the handle really point at `expected`? (After resolving every link, junction, `subst` and mapped drive.)
    fn handle_is_at(file: &fs::File, expected: &Path) -> Result<(), Skip> {
        let actual = final_path(file).ok_or(Skip::Io)?;
        if actual.starts_with(r"\\?\UNC\") {
            return Err(Skip::Network);
        }
        let actual = normalise(&actual);
        if actual == normalise(&expected.to_string_lossy()) || long_form(expected).map(|l| normalise(&l) == actual).unwrap_or(false) {
            Ok(())
        } else {
            Err(Skip::Link)
        }
    }

    fn open_dir_handle(path: &Path) -> Result<fs::File, Skip> {
        // Attribute-only access; BACKUP_SEMANTICS is what allows opening a directory.
        fs::OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(7)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .map_err(|e| skip_of(&e))
    }

    fn verify_dir(path: &Path) -> Result<(), Skip> {
        let handle = open_dir_handle(path)?;
        handle_is_at(&handle, path)
    }

    fn drive_is_remote(path: &Path) -> bool {
        use std::path::{Component, Prefix};
        let Some(Component::Prefix(p)) = path.components().next() else { return false };
        let letter = match p.kind() {
            Prefix::Disk(l) | Prefix::VerbatimDisk(l) => l as char,
            _ => return true, // UNC and device prefixes are never local
        };
        let root: Vec<u16> = OsStr::new(&format!("{letter}:\\")).encode_wide().chain(std::iter::once(0)).collect();
        // SAFETY: `root` is a NUL-terminated UTF-16 string that outlives the call.
        unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_REMOTE }
    }

    /// Every existing component of `root`, from just below the volume root down to `root` itself, must be a plain directory.
    fn check_chain(root: &Path) -> Walk {
        let mut chain: Vec<&Path> = root.ancestors().filter(|a| a.parent().is_some()).collect();
        chain.reverse();
        for p in chain {
            match stat(p) {
                Err(s) => return Walk::Skipped(s),
                Ok(None) => return Walk::NotFound,
                Ok(Some(m)) => {
                    if is_link_like(&m) {
                        return Walk::Skipped(Skip::Link);
                    }
                    if !m.is_dir() {
                        return Walk::Skipped(Skip::NotRegularFile);
                    }
                }
            }
        }
        Walk::Found { path: root.to_path_buf(), is_link: false }
    }

    fn check_root(root: &Path) -> Walk {
        if is_network_or_device_path(root) || drive_is_remote(root) {
            return Walk::Skipped(Skip::Network);
        }
        if !root.is_absolute() {
            return Walk::Skipped(Skip::Io);
        }
        match check_chain(root) {
            Walk::Found { .. } => {}
            other => return other,
        }
        hook::fire("after-chain-check");
        match verify_dir(root) {
            Ok(()) => Walk::Found { path: root.to_path_buf(), is_link: false },
            Err(s) => Walk::Skipped(s),
        }
    }

    pub(super) fn walk(root: &Path, rel: &[&str], final_kind: FinalKind, allow_final_link: bool) -> Walk {
        match check_root(root) {
            Walk::Found { .. } => {}
            other => return other,
        }
        let mut current = root.to_path_buf();
        for (i, component) in rel.iter().enumerate() {
            if !valid_component(component) {
                return Walk::Skipped(Skip::Io);
            }
            current.push(component);
            let last = i + 1 == rel.len();
            match stat(&current) {
                Err(s) => return Walk::Skipped(s),
                Ok(None) => return Walk::NotFound,
                Ok(Some(m)) => {
                    let link = is_link_like(&m);
                    if link && !(last && allow_final_link) {
                        return Walk::Skipped(Skip::Link);
                    }
                    if last {
                        if link {
                            return Walk::Found { path: current, is_link: true };
                        }
                        let kind_ok = match final_kind {
                            FinalKind::File => m.is_file(),
                            FinalKind::Dir => m.is_dir(),
                        };
                        if !kind_ok {
                            return Walk::Skipped(Skip::NotRegularFile);
                        }
                        if final_kind == FinalKind::Dir {
                            if let Err(s) = verify_dir(&current) {
                                return Walk::Skipped(s);
                            }
                        }
                        return Walk::Found { path: current, is_link: false };
                    }
                    if !m.is_dir() {
                        return Walk::Skipped(Skip::NotRegularFile);
                    }
                }
            }
        }
        Walk::Found { path: current, is_link: false }
    }

    pub(super) fn open_file(root: &Path, rel: &[&str]) -> Result<fs::File, OpenFail> {
        let path = match walk(root, rel, FinalKind::File, false) {
            Walk::Found { path, .. } => path,
            Walk::NotFound => return Err(OpenFail::NotFound),
            Walk::Skipped(s) => return Err(OpenFail::Skip(s)),
        };
        hook::fire("before-open");
        let file = match fs::OpenOptions::new().read(true).open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(OpenFail::NotFound),
            Err(e) => return Err(OpenFail::Skip(skip_of(&e))),
        };
        // The decisive check: the object we hold is at the path we meant, so nothing was redirected in between.
        handle_is_at(&file, &path).map_err(OpenFail::Skip)?;
        Ok(file)
    }
}

// ================================================================================================
// Unix (macOS)
// ================================================================================================
#[cfg(unix)]
mod imp {
    use super::*;
    use std::{
        ffi::CString,
        os::unix::{ffi::OsStrExt, io::FromRawFd},
        path::Component,
    };

    struct Fd(libc::c_int);

    impl Drop for Fd {
        fn drop(&mut self) {
            // SAFETY: the descriptor is owned by this value and closed exactly once.
            unsafe {
                libc::close(self.0);
            }
        }
    }

    enum Fail {
        NotFound,
        Skip(Skip),
    }

    fn errno() -> i32 {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
    }

    fn classify(code: i32) -> Fail {
        match code {
            libc::ENOENT => Fail::NotFound,
            libc::EACCES | libc::EPERM => Fail::Skip(Skip::AccessDenied),
            libc::ELOOP => Fail::Skip(Skip::Link),
            _ => Fail::Skip(Skip::Io),
        }
    }

    fn c_name(name: &std::ffi::OsStr) -> Result<CString, Fail> {
        CString::new(name.as_bytes()).map_err(|_| Fail::Skip(Skip::Io))
    }

    const DIR_FLAGS: libc::c_int = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;

    fn open_root() -> Result<Fd, Fail> {
        let slash = CString::new("/").expect("static");
        // SAFETY: valid NUL-terminated path, read-only flags.
        let fd = unsafe { libc::open(slash.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(classify(errno()));
        }
        let root = Fd(fd);
        check_filesystem(&root).map_err(Fail::Skip)?;
        Ok(root)
    }

    /// Stat without following a final link, relative to an open directory.
    fn stat_at(dir: &Fd, name: &CString) -> Result<libc::stat, Fail> {
        // SAFETY: `st` is fully written by a successful call; `dir` is a valid descriptor.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::fstatat(dir.0, name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) };
        if rc != 0 {
            Err(classify(errno()))
        } else {
            Ok(st)
        }
    }

    fn open_child_dir(parent: &Fd, name: &std::ffi::OsStr) -> Result<Fd, Fail> {
        let c = c_name(name)?;
        // SAFETY: `parent` is a valid directory descriptor, `c` a valid NUL-terminated name.
        let fd = unsafe { libc::openat(parent.0, c.as_ptr(), DIR_FLAGS) };
        if fd >= 0 {
            let child = Fd(fd);
            check_filesystem(&child).map_err(Fail::Skip)?;
            return Ok(child);
        }
        let code = errno();
        // ENOTDIR/ELOOP: tell a symbolic link apart from a plain file sitting where a folder was expected.
        if code == libc::ENOTDIR || code == libc::ELOOP {
            return Err(match stat_at(parent, &c) {
                Ok(st) if (st.st_mode & libc::S_IFMT) == libc::S_IFLNK => Fail::Skip(Skip::Link),
                Ok(_) => Fail::Skip(Skip::NotRegularFile),
                Err(f) => f,
            });
        }
        Err(classify(code))
    }

    fn inode_of(fd: &Fd) -> u64 {
        // SAFETY: zeroed stat is a valid out-parameter; `fd` is a valid descriptor.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstat(fd.0, &mut st) } == 0 {
            st.st_ino as u64
        } else {
            0
        }
    }

    #[cfg(target_os = "macos")]
    fn fstype_name(dir: &Fd) -> Result<String, ()> {
        // SAFETY: zeroed statfs is a valid out-parameter; `dir` is a valid descriptor.
        let mut sf: libc::statfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstatfs(dir.0, &mut sf) } != 0 {
            return Err(());
        }
        let bytes: Vec<u8> = sf.f_fstypename.iter().take_while(|c| **c != 0).map(|c| *c as u8).collect();
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Every directory handle — the root, each component of the selected root and each child below it — is judged
    /// here, so a network volume mounted at ANY depth is refused before a single entry of it is opened. A failed
    /// lookup or an unrecognised type is refused too. (Other Unix systems are not a supported target; they are not
    /// judged.)
    fn check_filesystem(dir: &Fd) -> Result<(), Skip> {
        if let Some(simulated) = hook::fstype_override(inode_of(dir)) {
            return judge_fstype(simulated.as_deref().map_err(|_| ()));
        }
        #[cfg(target_os = "macos")]
        {
            let name = fstype_name(dir);
            judge_fstype(name.as_deref().map_err(|_| ()))
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(())
        }
    }

    /// Opens `root` and then each directory in `dirs`, every step relative to the previous handle with O_NOFOLLOW.
    fn open_chain(root: &Path, dirs: &[&str]) -> Result<Fd, Fail> {
        if !root.is_absolute() {
            return Err(Fail::Skip(Skip::Io));
        }
        let mut dir = open_root()?;
        for component in root.components() {
            match component {
                Component::RootDir => {}
                Component::Normal(name) => {
                    hook::fire("between-components");
                    dir = open_child_dir(&dir, name)?;
                }
                _ => return Err(Fail::Skip(Skip::Io)),
            }
        }
        for name in dirs {
            if !valid_component(name) {
                return Err(Fail::Skip(Skip::Io));
            }
            hook::fire("between-components");
            dir = open_child_dir(&dir, std::ffi::OsStr::new(name))?;
        }
        Ok(dir)
    }

    fn lift(f: Fail) -> Walk {
        match f {
            Fail::NotFound => Walk::NotFound,
            Fail::Skip(s) => Walk::Skipped(s),
        }
    }

    pub(super) fn walk(root: &Path, rel: &[&str], final_kind: FinalKind, allow_final_link: bool) -> Walk {
        let mut path = root.to_path_buf();
        for c in rel {
            path.push(c);
        }
        let Some((last, parents)) = rel.split_last() else {
            return match open_chain(root, &[]) {
                Ok(_) => Walk::Found { path, is_link: false },
                Err(f) => lift(f),
            };
        };
        let dir = match open_chain(root, parents) {
            Ok(d) => d,
            Err(f) => return lift(f),
        };
        if !valid_component(last) {
            return Walk::Skipped(Skip::Io);
        }
        let name = match c_name(std::ffi::OsStr::new(last)) {
            Ok(n) => n,
            Err(f) => return lift(f),
        };
        let st = match stat_at(&dir, &name) {
            Ok(st) => st,
            Err(f) => return lift(f),
        };
        let kind = st.st_mode & libc::S_IFMT;
        if kind == libc::S_IFLNK {
            return if allow_final_link { Walk::Found { path, is_link: true } } else { Walk::Skipped(Skip::Link) };
        }
        let ok = match final_kind {
            FinalKind::File => kind == libc::S_IFREG,
            FinalKind::Dir => kind == libc::S_IFDIR,
        };
        if !ok {
            return Walk::Skipped(Skip::NotRegularFile);
        }
        if final_kind == FinalKind::Dir {
            // Re-open without following to prove it is still a real directory and to apply the network check.
            if let Err(f) = open_child_dir(&dir, std::ffi::OsStr::new(last)) {
                return lift(f);
            }
        }
        Walk::Found { path, is_link: false }
    }

    pub(super) fn open_file(root: &Path, rel: &[&str]) -> Result<fs::File, OpenFail> {
        let Some((last, parents)) = rel.split_last() else { return Err(OpenFail::Skip(Skip::NotRegularFile)) };
        let dir = match open_chain(root, parents) {
            Ok(d) => d,
            Err(Fail::NotFound) => return Err(OpenFail::NotFound),
            Err(Fail::Skip(s)) => return Err(OpenFail::Skip(s)),
        };
        if !valid_component(last) {
            return Err(OpenFail::Skip(Skip::Io));
        }
        let name = CString::new(last.as_bytes()).map_err(|_| OpenFail::Skip(Skip::Io))?;
        hook::fire("before-open");
        // O_NOFOLLOW: a link in the last position makes the open fail. O_NONBLOCK: a FIFO cannot hang the scan.
        // SAFETY: `dir` is a valid directory descriptor and `name` a valid NUL-terminated name.
        let fd = unsafe { libc::openat(dir.0, name.as_ptr(), libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK) };
        if fd < 0 {
            return Err(match classify(errno()) {
                Fail::NotFound => OpenFail::NotFound,
                Fail::Skip(s) => OpenFail::Skip(s),
            });
        }
        // SAFETY: `fd` was just opened and is owned by no one else; `File` takes ownership and closes it.
        Ok(unsafe { fs::File::from_raw_fd(fd) })
    }
}

// ================================================================================================
// Anything else is unsupported: refuse everything rather than guess.
// ================================================================================================
#[cfg(not(any(windows, unix)))]
mod imp {
    use super::*;

    pub(super) fn walk(_: &Path, _: &[&str], _: FinalKind, _: bool) -> Walk {
        Walk::Skipped(Skip::Io)
    }

    pub(super) fn open_file(_: &Path, _: &[&str]) -> Result<fs::File, OpenFail> {
        Err(OpenFail::Skip(Skip::Io))
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn network_file_system_names_are_recognised() {
        for n in ["smbfs", "NFS", "afpfs", "webdav", "cifs", "autofs"] {
            assert!(is_network_fstype(n), "{n}");
        }
        for n in ["apfs", "hfs", "ntfs", "ext4", "msdos", "exfat", "tmpfs", ""] {
            assert!(!is_network_fstype(n), "{n}");
        }
    }

    #[test]
    fn the_file_system_judgement_fails_closed() {
        assert_eq!(judge_fstype(Err(())), Err(Skip::UnknownFilesystem), "a failed lookup is never read as local");
        for n in ["smbfs", "nfs", "afpfs", "webdav", "autofs", "SMBFS"] {
            assert_eq!(judge_fstype(Ok(n)), Err(Skip::Network), "{n}");
        }
        for n in ["apfs", "hfs", "msdos", "exfat", "tmpfs", "APFS"] {
            assert_eq!(judge_fstype(Ok(n)), Ok(()), "{n}");
        }
        for n in ["", "fusefs", "macfuse", "osxfuse", "somethingnew", "lifs"] {
            assert_eq!(judge_fstype(Ok(n)), Err(Skip::UnknownFilesystem), "unknown type {n:?} is refused");
        }
    }

    #[test]
    fn component_validation_rejects_traversal_and_separators() {
        for bad in ["", ".", "..", "a/b", "a\\b", "a:b", "a\0b"] {
            assert!(!valid_component(bad), "{bad:?}");
        }
        assert!(valid_component(".claude") && valid_component("settings.json"));
    }
}
