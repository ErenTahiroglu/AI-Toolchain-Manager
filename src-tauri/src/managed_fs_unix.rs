//! Unix (macOS) implementation of `managed_fs`: every operation is `openat` / `mkdirat` / `fstatat` / `unlinkat` / `renameat`
//! relative to a held directory descriptor, with `O_NOFOLLOW` so a symbolic link is never traversed.
//!
//! macOS has no "delete exactly this inode" call: `remove_file_verified` confirms `(st_dev, st_ino)` of the name with `fstatat`
//! right before `unlinkat`; that window of microseconds is documented, not hidden.

use super::{hook, FsError, Identity, Kind, Stat};
use std::{
    ffi::CString,
    os::unix::{
        ffi::OsStrExt,
        io::{AsRawFd, FromRawFd},
    },
    path::{Component, Path},
};

fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

fn map_errno(code: i32) -> FsError {
    match code {
        libc::ENOENT => FsError::NotFound,
        libc::EEXIST => FsError::Exists,
        libc::ELOOP => FsError::Link,
        libc::ENOTDIR => FsError::NotDir,
        libc::EISDIR => FsError::NotFile,
        libc::EXDEV => FsError::CrossDevice,
        libc::EACCES | libc::EPERM | libc::EROFS => FsError::Denied,
        libc::EBUSY | libc::ETXTBSY => FsError::Locked,
        libc::ENOTEMPTY => FsError::NotEmpty,
        libc::ENAMETOOLONG | libc::EINVAL => FsError::InvalidName,
        _ => FsError::Io,
    }
}

pub fn map_io(e: &std::io::Error) -> FsError {
    e.raw_os_error().map(map_errno).unwrap_or(FsError::Io)
}

fn cname(name: &str) -> Result<CString, FsError> {
    CString::new(name.as_bytes()).map_err(|_| FsError::InvalidName)
}

fn kind_of(mode: libc::mode_t) -> Kind {
    match mode & libc::S_IFMT {
        libc::S_IFREG => Kind::File,
        libc::S_IFDIR => Kind::Dir,
        libc::S_IFLNK => Kind::Link,
        _ => Kind::Other,
    }
}

fn stat_of(st: &libc::stat) -> Stat {
    Stat {
        kind: kind_of(st.st_mode),
        identity: Identity { volume: st.st_dev as u64, object: st.st_ino as u64 },
        links: st.st_nlink as u64,
        size: st.st_size as u64,
    }
}

fn fstat_fd(fd: libc::c_int) -> Result<(libc::stat, Stat), FsError> {
    // SAFETY: zeroed stat is a valid out-parameter; `fd` is a valid descriptor owned by the caller.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        return Err(map_errno(errno()));
    }
    let s = stat_of(&st);
    Ok((st, s))
}

pub fn file_stat(f: &std::fs::File) -> Result<Stat, FsError> {
    fstat_fd(f.as_raw_fd()).map(|(_, s)| s)
}

pub fn sync_file(f: &std::fs::File) -> Result<(), FsError> {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: valid descriptor. F_FULLFSYNC asks the drive to flush its cache; fall back to fsync if unsupported.
        if unsafe { libc::fcntl(f.as_raw_fd(), libc::F_FULLFSYNC) } == 0 {
            return Ok(());
        }
    }
    f.sync_all().map_err(|e| map_io(&e))
}

const DIR_FLAGS: libc::c_int = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;

pub struct DirImp {
    fd: libc::c_int,
    pub identity: Identity,
}

impl Drop for DirImp {
    fn drop(&mut self) {
        // SAFETY: the descriptor is owned by this value and closed exactly once.
        unsafe {
            libc::close(self.fd);
        }
    }
}

#[cfg(target_os = "macos")]
fn check_local_filesystem(fd: libc::c_int) -> Result<(), FsError> {
    // SAFETY: zeroed statfs is a valid out-parameter; `fd` is valid.
    let mut sf: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstatfs(fd, &mut sf) } != 0 {
        return Err(FsError::Network);
    }
    let bytes: Vec<u8> = sf.f_fstypename.iter().take_while(|c| **c != 0).map(|c| *c as u8).collect();
    let name = String::from_utf8_lossy(&bytes).into_owned();
    if crate::discovery::fsx::is_local_fstype(&name) {
        Ok(())
    } else {
        Err(FsError::Network) // network or unrecognised: refused
    }
}

#[cfg(not(target_os = "macos"))]
fn check_local_filesystem(_fd: libc::c_int) -> Result<(), FsError> {
    Ok(())
}

fn dir_from_fd(fd: libc::c_int) -> Result<DirImp, FsError> {
    match fstat_fd(fd) {
        Ok((_, s)) if s.kind == Kind::Dir => Ok(DirImp { fd, identity: s.identity }),
        Ok(_) => {
            // SAFETY: owned descriptor, closed once on this path.
            unsafe { libc::close(fd) };
            Err(FsError::NotDir)
        }
        Err(e) => {
            // SAFETY: as above.
            unsafe { libc::close(fd) };
            Err(e)
        }
    }
}

fn open_dir_at(parent: libc::c_int, name: &CString) -> Result<DirImp, FsError> {
    // SAFETY: `parent` is a valid directory descriptor, `name` a valid NUL-terminated string.
    let fd = unsafe { libc::openat(parent, name.as_ptr(), DIR_FLAGS) };
    if fd < 0 {
        let code = errno();
        if code == libc::ENOTDIR || code == libc::ELOOP {
            // tell a symbolic link apart from a plain file where a folder was expected
            // SAFETY: zeroed stat is a valid out-parameter.
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            if unsafe { libc::fstatat(parent, name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) } == 0 && kind_of(st.st_mode) == Kind::Link
            {
                return Err(FsError::Link);
            }
        }
        return Err(map_errno(code));
    }
    dir_from_fd(fd)
}

pub fn open_anchor(path: &Path) -> Result<DirImp, FsError> {
    if !path.is_absolute() {
        return Err(FsError::InvalidName);
    }
    let slash = CString::new("/").expect("static");
    // SAFETY: valid NUL-terminated path.
    let fd = unsafe { libc::open(slash.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(map_errno(errno()));
    }
    let mut dir = dir_from_fd(fd)?;
    for comp in path.components() {
        match comp {
            Component::RootDir => {}
            Component::Normal(name) => {
                hook::fire("between-components");
                let c = CString::new(name.as_bytes()).map_err(|_| FsError::InvalidName)?;
                // no device comparison along the anchor's own chain (macOS firmlinks); only symbolic links are refused
                dir = open_dir_at(dir.fd, &c)?;
            }
            _ => return Err(FsError::InvalidName),
        }
    }
    check_local_filesystem(dir.fd)?;
    Ok(dir)
}

impl DirImp {
    pub fn open_child_dir(&self, name: &str) -> Result<DirImp, FsError> {
        hook::fire("before-open");
        let d = open_dir_at(self.fd, &cname(name)?)?;
        if hook::volume_of(d.identity) != hook::volume_of(self.identity) {
            return Err(FsError::CrossDevice);
        }
        Ok(d)
    }

    pub fn create_child_dir(&self, name: &str, _owner_only: bool) -> Result<DirImp, FsError> {
        let c = cname(name)?;
        // SAFETY: valid directory descriptor and name. Mode 0700 for every managed folder; the umask can only remove bits.
        if unsafe { libc::mkdirat(self.fd, c.as_ptr(), 0o700) } != 0 {
            return Err(map_errno(errno()));
        }
        let d = open_dir_at(self.fd, &c)?;
        if hook::volume_of(d.identity) != hook::volume_of(self.identity) {
            return Err(FsError::CrossDevice);
        }
        Ok(d)
    }

    /// Owner-only: owned by the effective user and no permission bits for group or others.
    pub fn verify_owner_only(&self) -> Result<(), FsError> {
        let (st, _) = fstat_fd(self.fd)?;
        // SAFETY: geteuid has no preconditions.
        if st.st_uid != unsafe { libc::geteuid() } || (st.st_mode & 0o077) != 0 {
            return Err(FsError::Denied);
        }
        Ok(())
    }

    pub fn create_file(&self, name: &str) -> Result<std::fs::File, FsError> {
        let c = cname(name)?;
        // SAFETY: valid directory descriptor and name; O_EXCL|O_CREAT never adopts an existing file; O_NOFOLLOW refuses a link.
        let fd = unsafe {
            libc::openat(
                self.fd,
                c.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600 as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(map_errno(errno()));
        }
        // SAFETY: freshly opened descriptor owned by no one else.
        Ok(unsafe { std::fs::File::from_raw_fd(fd) })
    }

    pub fn open_file(&self, name: &str) -> Result<std::fs::File, FsError> {
        hook::fire("before-open");
        let c = cname(name)?;
        // O_NONBLOCK: a FIFO cannot hang the open. O_NOFOLLOW: a link in the last position fails with ELOOP.
        // SAFETY: valid descriptor and name.
        let fd = unsafe { libc::openat(self.fd, c.as_ptr(), libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK) };
        if fd < 0 {
            return Err(map_errno(errno()));
        }
        // SAFETY: as above.
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        match file_stat(&file)? {
            s if s.kind == Kind::File => Ok(file),
            s if s.kind == Kind::Link => Err(FsError::Link),
            _ => Err(FsError::NotFile),
        }
    }

    pub fn stat_name(&self, name: &str) -> Result<Option<Stat>, FsError> {
        let c = cname(name)?;
        // SAFETY: zeroed stat is a valid out-parameter; valid descriptor and name.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstatat(self.fd, c.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) } != 0 {
            return match errno() {
                libc::ENOENT => Ok(None),
                code => Err(map_errno(code)),
            };
        }
        Ok(Some(stat_of(&st)))
    }

    pub fn list(&self, max: usize) -> Result<Vec<String>, FsError> {
        // SAFETY: duplicate the descriptor so closedir does not close ours; fdopendir takes ownership of the duplicate.
        let dup = unsafe { libc::dup(self.fd) };
        if dup < 0 {
            return Err(map_errno(errno()));
        }
        let dirp = unsafe { libc::fdopendir(dup) };
        if dirp.is_null() {
            let code = errno();
            unsafe { libc::close(dup) };
            return Err(map_errno(code));
        }
        unsafe { libc::rewinddir(dirp) };
        let mut out = Vec::new();
        let mut result = Ok(());
        loop {
            // SAFETY: `dirp` is a valid DIR*; readdir returns null at the end.
            let ent = unsafe { libc::readdir(dirp) };
            if ent.is_null() {
                break;
            }
            // SAFETY: d_name is a NUL-terminated array inside the returned entry.
            let name = unsafe { std::ffi::CStr::from_ptr((*ent).d_name.as_ptr()) }.to_string_lossy().into_owned();
            if name == "." || name == ".." {
                continue;
            }
            out.push(name);
            if out.len() > max {
                result = Err(FsError::TooLarge);
                break;
            }
        }
        // SAFETY: closes the DIR* and the duplicate descriptor.
        unsafe { libc::closedir(dirp) };
        result.map(|()| out)
    }

    pub fn remove_file_verified(&self, name: &str, check: &dyn Fn(&std::fs::File, &Stat) -> Result<(), FsError>) -> Result<(), FsError> {
        let c = cname(name)?;
        // SAFETY: valid descriptor and name; no write access is needed to verify.
        let fd = unsafe { libc::openat(self.fd, c.as_ptr(), libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK) };
        if fd < 0 {
            return Err(map_errno(errno()));
        }
        // SAFETY: freshly opened descriptor.
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        let stat = file_stat(&file)?;
        if stat.kind == Kind::Link {
            return Err(FsError::Link);
        }
        check(&file, &stat)?;
        hook::fire("before-delete");
        // confirm the NAME still designates the object that was verified, immediately before the unlink
        // SAFETY: zeroed stat is a valid out-parameter.
        let mut now: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstatat(self.fd, c.as_ptr(), &mut now, libc::AT_SYMLINK_NOFOLLOW) } != 0 {
            return Err(map_errno(errno()));
        }
        if stat_of(&now).identity != stat.identity || kind_of(now.st_mode) != Kind::File {
            return Err(FsError::Changed);
        }
        // SAFETY: valid descriptor and name; flags 0 = unlink a non-directory.
        if unsafe { libc::unlinkat(self.fd, c.as_ptr(), 0) } != 0 {
            return Err(map_errno(errno()));
        }
        Ok(())
    }

    pub fn remove_empty_dir(&self, name: &str) -> Result<(), FsError> {
        let c = cname(name)?;
        // a link is refused, not followed: unlinkat(AT_REMOVEDIR) on a symlink fails with ENOTDIR, but check explicitly first
        match self.stat_name(name)? {
            None => return Err(FsError::NotFound),
            Some(s) if s.kind == Kind::Link => return Err(FsError::Link),
            Some(s) if s.kind != Kind::Dir => return Err(FsError::NotDir),
            Some(_) => {}
        }
        // SAFETY: valid descriptor and name; AT_REMOVEDIR removes only an empty directory.
        if unsafe { libc::unlinkat(self.fd, c.as_ptr(), libc::AT_REMOVEDIR) } != 0 {
            return Err(map_errno(errno()));
        }
        Ok(())
    }

    pub fn rename_file(&self, from: &str, to_dir: &DirImp, to: &str, replace: bool, expect: Option<Identity>) -> Result<(), FsError> {
        let (cf, ct) = (cname(from)?, cname(to)?);
        match self.stat_name(from)? {
            None => return Err(FsError::NotFound),
            Some(s) if s.kind == Kind::Link => return Err(FsError::Link),
            Some(s) if s.kind != Kind::File => return Err(FsError::NotFile),
            Some(s) if expect.is_some_and(|i| i != s.identity) => return Err(FsError::Changed),
            Some(_) => {}
        }
        if !replace && to_dir.stat_name(to)?.is_some() {
            return Err(FsError::Exists);
        }
        // SAFETY: valid descriptors and names; renameat replaces the destination atomically.
        if unsafe { libc::renameat(self.fd, cf.as_ptr(), to_dir.fd, ct.as_ptr()) } != 0 {
            return Err(map_errno(errno()));
        }
        Ok(())
    }

    pub fn sync(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            // SAFETY: valid descriptor.
            if unsafe { libc::fcntl(self.fd, libc::F_FULLFSYNC) } == 0 {
                return true;
            }
        }
        // SAFETY: valid descriptor.
        unsafe { libc::fsync(self.fd) == 0 }
    }
}
