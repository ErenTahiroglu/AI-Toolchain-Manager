//! Windows implementation of `managed_fs`: every open is `NtCreateFile` relative to a directory handle, with
//! `FILE_OPEN_REPARSE_POINT` so a symbolic link, junction or mount point is opened as itself and then refused, never traversed.
//!
//! The NT entry points are declared here (raw-dylib against `ntdll.dll`) so that the exact flags are visible and reviewed in
//! one place.

use super::{hook, FsError, Identity, Kind, Stat};
use std::{
    ffi::c_void,
    os::windows::io::{AsRawHandle, FromRawHandle},
    path::Path,
    ptr::{null, null_mut},
};
use windows_sys::Win32::Storage::FileSystem::{FlushFileBuffers, GetDriveTypeW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};

type Handle = *mut c_void;

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root: Handle,
    name: *const UnicodeString,
    attributes: u32,
    security_descriptor: *const c_void,
    qos: *const c_void,
}

#[repr(C)]
struct Iosb {
    status: isize,
    information: usize,
}

#[link(name = "ntdll", kind = "raw-dylib")]
extern "system" {
    fn NtCreateFile(
        file: *mut Handle,
        access: u32,
        attributes: *const ObjectAttributes,
        iosb: *mut Iosb,
        allocation: *const i64,
        file_attributes: u32,
        share: u32,
        disposition: u32,
        options: u32,
        ea: *const c_void,
        ea_len: u32,
    ) -> i32;
    fn NtSetInformationFile(file: Handle, iosb: *mut Iosb, info: *const c_void, len: u32, class: i32) -> i32;
    fn NtQueryDirectoryFile(
        file: Handle,
        event: Handle,
        apc: *const c_void,
        apc_ctx: *const c_void,
        iosb: *mut Iosb,
        info: *mut c_void,
        len: u32,
        class: i32,
        single: u8,
        name: *const UnicodeString,
        restart: u8,
    ) -> i32;
    fn NtClose(file: Handle) -> i32;
}

#[link(name = "advapi32", kind = "raw-dylib")]
extern "system" {
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl: *const u16, revision: u32, sd: *mut *mut c_void, size: *mut u32) -> i32;
    fn ConvertSecurityDescriptorToStringSecurityDescriptorW(
        sd: *mut c_void,
        revision: u32,
        info: u32,
        out: *mut *mut u16,
        len: *mut u32,
    ) -> i32;
    fn GetSecurityInfo(
        file: Handle,
        object_type: u32,
        info: u32,
        owner: *mut *mut c_void,
        group: *mut *mut c_void,
        dacl: *mut *mut c_void,
        sacl: *mut *mut c_void,
        sd: *mut *mut c_void,
    ) -> u32;
}

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    fn LocalFree(mem: *mut c_void) -> *mut c_void;
}

// access rights
const FILE_READ_DATA: u32 = 0x1; // = FILE_LIST_DIRECTORY
const FILE_WRITE_DATA: u32 = 0x2; // = FILE_ADD_FILE
const FILE_APPEND_DATA: u32 = 0x4; // = FILE_ADD_SUBDIRECTORY
const FILE_TRAVERSE: u32 = 0x20;
const FILE_READ_ATTRIBUTES: u32 = 0x80;
const DELETE: u32 = 0x10000;
const READ_CONTROL: u32 = 0x20000;
const SYNCHRONIZE: u32 = 0x100000;
// share
const SHARE_READ: u32 = 1;
const SHARE_WRITE: u32 = 2;
const SHARE_ALL: u32 = 7;
// dispositions and options
const FILE_OPEN: u32 = 1;
const FILE_CREATE: u32 = 2;
const FILE_DIRECTORY_FILE: u32 = 0x1;
const FILE_NON_DIRECTORY_FILE: u32 = 0x40;
const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x20;
const FILE_OPEN_REPARSE_POINT: u32 = 0x200000;
const OBJ_CASE_INSENSITIVE: u32 = 0x40;
// information classes
const FILE_NAMES_INFORMATION: i32 = 12;
const FILE_DISPOSITION_INFORMATION: i32 = 13;
const FILE_DISPOSITION_INFORMATION_EX: i32 = 64;
const FILE_RENAME_INFORMATION_EX: i32 = 65;

const DIR_ACCESS: u32 =
    SYNCHRONIZE | READ_CONTROL | FILE_READ_DATA | FILE_WRITE_DATA | FILE_APPEND_DATA | FILE_TRAVERSE | FILE_READ_ATTRIBUTES;
/// Folders on the way DOWN to the anchor are only traversed: they need no write access (and a user has none on the volume root).
const CHAIN_ACCESS: u32 = SYNCHRONIZE | FILE_READ_DATA | FILE_TRAVERSE | FILE_READ_ATTRIBUTES;
const OWNER_ONLY_SDDL: &str = "D:P(A;OICI;FA;;;OW)";

pub fn map_status(status: i32) -> FsError {
    match status as u32 {
        0xC000_0034 | 0xC000_003A | 0xC000_0039 => FsError::NotFound,
        0xC000_0035 => FsError::Exists,
        0xC000_0022 | 0xC000_0121 | 0xC000_00A2 => FsError::Denied,
        0xC000_0043 | 0xC000_0056 => FsError::Locked,
        0xC000_0103 => FsError::NotDir,
        0xC000_00BA => FsError::NotFile,
        0xC000_0101 => FsError::NotEmpty,
        0xC000_0033 | 0xC000_003B => FsError::InvalidName,
        0xC000_00D4 => FsError::CrossDevice,
        0xC000_050B | 0xC000_0279 => FsError::Link,
        _ => FsError::Io,
    }
}

pub fn map_io(e: &std::io::Error) -> FsError {
    match e.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound,
        std::io::ErrorKind::AlreadyExists => FsError::Exists,
        std::io::ErrorKind::PermissionDenied => FsError::Denied,
        _ => match e.raw_os_error() {
            Some(32) | Some(33) => FsError::Locked,
            _ => FsError::Io,
        },
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

#[allow(clippy::too_many_arguments)] // the shape of the NT call, kept visible on purpose
/// One `NtCreateFile` call. `root` is the directory handle the name is relative to (null for an absolute NT path).
fn nt_open(
    root: Handle,
    name: &[u16],
    access: u32,
    share: u32,
    disposition: u32,
    options: u32,
    file_attributes: u32,
    sd: *const c_void,
) -> Result<Handle, FsError> {
    let bytes = (name.len() * 2) as u16;
    let us = UnicodeString { length: bytes, maximum_length: bytes, buffer: name.as_ptr() };
    let oa = ObjectAttributes {
        length: std::mem::size_of::<ObjectAttributes>() as u32,
        root,
        name: &us,
        attributes: OBJ_CASE_INSENSITIVE,
        security_descriptor: sd,
        qos: null(),
    };
    let mut h: Handle = null_mut();
    let mut iosb = Iosb { status: 0, information: 0 };
    // SAFETY: all pointers refer to live locals; `root` is either null or a handle owned by the caller for the whole call.
    let st = unsafe { NtCreateFile(&mut h, access, &oa, &mut iosb, null(), file_attributes, share, disposition, options, null(), 0) };
    if st >= 0 {
        Ok(h)
    } else {
        Err(map_status(st))
    }
}

fn close(h: Handle) {
    // SAFETY: the handle is owned by the caller and closed exactly once.
    unsafe {
        NtClose(h);
    }
}

fn handle_stat(h: Handle) -> Result<Stat, FsError> {
    // SAFETY: `info` is plain data that the call fills in; `h` is a valid open handle.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(h, &mut info) } == 0 {
        return Err(FsError::Io);
    }
    let attr = info.dwFileAttributes;
    let kind = if attr & 0x400 != 0 {
        Kind::Link
    } else if attr & 0x10 != 0 {
        Kind::Dir
    } else {
        Kind::File
    };
    Ok(Stat {
        kind,
        identity: Identity {
            volume: u64::from(info.dwVolumeSerialNumber),
            object: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        },
        links: u64::from(info.nNumberOfLinks),
        size: (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow),
    })
}

pub fn file_stat(f: &std::fs::File) -> Result<Stat, FsError> {
    handle_stat(f.as_raw_handle())
}

pub fn sync_file(f: &std::fs::File) -> Result<(), FsError> {
    f.sync_all().map_err(|e| map_io(&e))
}

/// How a move treats an existing destination.
#[derive(Clone, Copy)]
pub enum Mode {
    /// An existing destination of any kind is refused.
    NoReplace,
    /// Only a plain single-link file with exactly this identity may be replaced.
    Replace { dest: Identity },
}

pub struct DirImp {
    h: Handle,
    pub identity: Identity,
}

// SAFETY: a kernel handle may be used from any thread.
unsafe impl Send for DirImp {}

impl Drop for DirImp {
    fn drop(&mut self) {
        close(self.h);
    }
}

struct SecurityDescriptor(*mut c_void);

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        // SAFETY: allocated by the security API with LocalAlloc; freed once.
        unsafe {
            LocalFree(self.0);
        }
    }
}

fn owner_only_descriptor() -> Result<SecurityDescriptor, FsError> {
    let sddl: Vec<u16> = OWNER_ONLY_SDDL.encode_utf16().chain(std::iter::once(0)).collect();
    let mut sd: *mut c_void = null_mut();
    // SAFETY: `sddl` is NUL-terminated; `sd` receives a LocalAlloc'd descriptor.
    if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), 1, &mut sd, null_mut()) } == 0 {
        return Err(FsError::Io);
    }
    Ok(SecurityDescriptor(sd))
}

fn open_dir_handle(root: Handle, name: &str, access: u32, share: u32) -> Result<DirImp, FsError> {
    let h = nt_open(
        root,
        &wide(name),
        access,
        share,
        FILE_OPEN,
        FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
        0,
        null(),
    )?;
    finish_dir(h)
}

fn finish_dir(h: Handle) -> Result<DirImp, FsError> {
    match handle_stat(h) {
        Ok(s) if s.kind == Kind::Dir => Ok(DirImp { h, identity: s.identity }),
        Ok(s) => {
            close(h);
            Err(if s.kind == Kind::Link { FsError::Link } else { FsError::NotDir })
        }
        Err(e) => {
            close(h);
            Err(e)
        }
    }
}

pub fn open_anchor(path: &Path) -> Result<DirImp, FsError> {
    let text = path.to_str().ok_or(FsError::InvalidName)?;
    let text = text.strip_prefix(r"\\?\").unwrap_or(text);
    if text.starts_with(r"\\") || text.starts_with("//") {
        return Err(FsError::Network);
    }
    let b = text.as_bytes();
    if b.len() < 3 || !b[0].is_ascii_alphabetic() || b[1] != b':' || b[2] != b'\\' {
        return Err(FsError::InvalidName);
    }
    let drive_root: Vec<u16> = format!("{}:\\", b[0] as char).encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: NUL-terminated wide string.
    match unsafe { GetDriveTypeW(drive_root.as_ptr()) } {
        2 | 3 | 6 => {} // removable, fixed, RAM disk
        _ => return Err(FsError::Network),
    }
    let nt_root = format!("\\??\\{}:\\", b[0] as char);
    let h = nt_open(
        null_mut(),
        &wide(&nt_root),
        CHAIN_ACCESS,
        SHARE_ALL,
        FILE_OPEN,
        FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
        0,
        null(),
    )?;
    let mut dir = finish_dir(h)?;
    let comps: Vec<&str> = text[3..].split(char::from(92u8)).filter(|c| !c.is_empty()).collect();
    for (i, comp) in comps.iter().enumerate() {
        if *comp == "." || *comp == ".." {
            return Err(FsError::InvalidName);
        }
        hook::fire("between-components");
        // no volume comparison along the anchor's own chain (only below the anchor); only the anchor itself is opened for use
        dir = if i + 1 == comps.len() {
            open_dir_handle(dir.h, comp, DIR_ACCESS, SHARE_READ | SHARE_WRITE)?
        } else {
            open_dir_handle(dir.h, comp, CHAIN_ACCESS, SHARE_ALL)?
        };
    }
    Ok(dir)
}

impl DirImp {
    pub fn open_child_dir(&self, name: &str) -> Result<DirImp, FsError> {
        hook::fire("before-open");
        let d = open_dir_handle(self.h, name, DIR_ACCESS, SHARE_READ | SHARE_WRITE)?;
        if hook::volume_of(d.identity) != hook::volume_of(self.identity) {
            return Err(FsError::CrossDevice);
        }
        Ok(d)
    }

    pub fn create_child_dir(&self, name: &str, owner_only: bool) -> Result<DirImp, FsError> {
        let sd = if owner_only { Some(owner_only_descriptor()?) } else { None };
        let sdp = sd.as_ref().map(|s| s.0 as *const c_void).unwrap_or(null());
        let h = nt_open(
            self.h,
            &wide(name),
            DIR_ACCESS,
            SHARE_READ | SHARE_WRITE,
            FILE_CREATE,
            FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT,
            0x10,
            sdp,
        )?;
        let d = finish_dir(h)?;
        if hook::volume_of(d.identity) != hook::volume_of(self.identity) {
            return Err(FsError::CrossDevice);
        }
        Ok(d)
    }

    /// Reads the DACL back and requires exactly the owner-only protected form that was requested.
    pub fn verify_owner_only(&self) -> Result<(), FsError> {
        let mut sd: *mut c_void = null_mut();
        let mut dacl: *mut c_void = null_mut();
        // SAFETY: out-parameters are valid locals; `self.h` is a valid handle opened with READ_CONTROL.
        let rc = unsafe { GetSecurityInfo(self.h, 1, 4, null_mut(), null_mut(), &mut dacl, null_mut(), &mut sd) };
        if rc != 0 {
            return Err(FsError::Denied);
        }
        let _free_sd = SecurityDescriptor(sd);
        let mut out: *mut u16 = null_mut();
        let mut len = 0u32;
        // SAFETY: `sd` came from GetSecurityInfo; `out` receives a LocalAlloc'd wide string.
        if unsafe { ConvertSecurityDescriptorToStringSecurityDescriptorW(sd, 1, 4, &mut out, &mut len) } == 0 {
            return Err(FsError::Io);
        }
        let _free_str = SecurityDescriptor(out as *mut c_void);
        // SAFETY: `out` points to `len` UTF-16 units (including the NUL) written by the call above.
        let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(out, len.saturating_sub(1) as usize) });
        if text == OWNER_ONLY_SDDL {
            Ok(())
        } else {
            Err(FsError::Denied)
        }
    }

    pub fn create_file(&self, name: &str) -> Result<std::fs::File, FsError> {
        let h = nt_open(
            self.h,
            &wide(name),
            SYNCHRONIZE | FILE_READ_DATA | FILE_WRITE_DATA | FILE_APPEND_DATA | FILE_READ_ATTRIBUTES,
            SHARE_READ,
            FILE_CREATE,
            FILE_NON_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT,
            0x80,
            null(),
        )?;
        // SAFETY: the handle was just created and is owned by no one else; `File` takes ownership.
        Ok(unsafe { std::fs::File::from_raw_handle(h) })
    }

    pub fn open_file(&self, name: &str) -> Result<std::fs::File, FsError> {
        hook::fire("before-open");
        let h = nt_open(
            self.h,
            &wide(name),
            SYNCHRONIZE | FILE_READ_DATA | FILE_READ_ATTRIBUTES,
            SHARE_ALL,
            FILE_OPEN,
            FILE_NON_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
            0,
            null(),
        )?;
        // SAFETY: as above.
        let file = unsafe { std::fs::File::from_raw_handle(h) };
        match handle_stat(h)? {
            s if s.kind == Kind::File => Ok(file),
            s if s.kind == Kind::Link => Err(FsError::Link),
            _ => Err(FsError::NotFile),
        }
    }

    pub fn stat_name(&self, name: &str) -> Result<Option<Stat>, FsError> {
        match nt_open(
            self.h,
            &wide(name),
            SYNCHRONIZE | FILE_READ_ATTRIBUTES,
            SHARE_ALL,
            FILE_OPEN,
            FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
            0,
            null(),
        ) {
            Ok(h) => {
                let s = handle_stat(h);
                close(h);
                s.map(Some)
            }
            Err(FsError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn list(&self, max: usize) -> Result<Vec<String>, FsError> {
        let mut out: Vec<String> = Vec::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut restart = 1u8;
        loop {
            let mut iosb = Iosb { status: 0, information: 0 };
            // SAFETY: `buf` is a live buffer of the stated length; the handle was opened with FILE_LIST_DIRECTORY.
            let st = unsafe {
                NtQueryDirectoryFile(
                    self.h,
                    null_mut(),
                    null(),
                    null(),
                    &mut iosb,
                    buf.as_mut_ptr() as *mut c_void,
                    buf.len() as u32,
                    FILE_NAMES_INFORMATION,
                    0,
                    null(),
                    restart,
                )
            };
            restart = 0;
            if st as u32 == 0x8000_0006 {
                break; // STATUS_NO_MORE_FILES
            }
            if st < 0 {
                return Err(map_status(st));
            }
            let mut off = 0usize;
            loop {
                let rd = |at: usize| u32::from_le_bytes(buf[at..at + 4].try_into().unwrap());
                let next = rd(off) as usize;
                let len = rd(off + 8) as usize;
                let units: Vec<u16> = (0..len / 2).map(|n| u16::from_le_bytes([buf[off + 12 + 2 * n], buf[off + 13 + 2 * n]])).collect();
                let name = String::from_utf16_lossy(&units);
                if name != "." && name != ".." {
                    out.push(name);
                    if out.len() > max {
                        return Err(FsError::TooLarge);
                    }
                }
                if next == 0 {
                    break;
                }
                off += next;
            }
        }
        Ok(out)
    }

    /// Opens `name` with DELETE access, lets `check` judge the open handle, then deletes THROUGH THAT HANDLE.
    pub fn remove_file_verified(&self, name: &str, check: &dyn Fn(&std::fs::File, &Stat) -> Result<(), FsError>) -> Result<(), FsError> {
        let h = nt_open(
            self.h,
            &wide(name),
            DELETE | SYNCHRONIZE | FILE_READ_DATA | FILE_READ_ATTRIBUTES,
            SHARE_ALL,
            FILE_OPEN,
            FILE_NON_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
            0,
            null(),
        )?;
        // SAFETY: fresh handle owned by the `File` from here on.
        let file = unsafe { std::fs::File::from_raw_handle(h) };
        let stat = handle_stat(h)?;
        if stat.kind == Kind::Link {
            return Err(FsError::Link);
        }
        check(&file, &stat)?;
        hook::fire("before-delete");
        delete_by_handle(h)
    }

    pub fn remove_empty_dir(&self, name: &str) -> Result<(), FsError> {
        let h = nt_open(
            self.h,
            &wide(name),
            DELETE | SYNCHRONIZE | FILE_READ_DATA | FILE_READ_ATTRIBUTES,
            SHARE_ALL,
            FILE_OPEN,
            FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
            0,
            null(),
        )?;
        let r = match handle_stat(h) {
            Ok(s) if s.kind == Kind::Dir => delete_by_handle(h),
            Ok(s) if s.kind == Kind::Link => Err(FsError::Link),
            Ok(_) => Err(FsError::NotDir),
            Err(e) => Err(e),
        };
        close(h);
        r
    }

    /// Returns whether the displaced destination still exists under the source name (never on Windows: a replaced file is gone).
    pub fn rename_file(&self, from: &str, to_dir: &DirImp, to: &str, mode: Mode, expect: Option<Identity>) -> Result<bool, FsError> {
        let h = nt_open(
            self.h,
            &wide(from),
            DELETE | SYNCHRONIZE | FILE_READ_ATTRIBUTES,
            SHARE_ALL,
            FILE_OPEN,
            FILE_NON_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
            0,
            null(),
        )?;
        let mut dest_handle: Option<Handle> = None;
        let r = (|| {
            let s = handle_stat(h)?;
            if s.kind == Kind::Link {
                return Err(FsError::Link);
            }
            if s.kind != Kind::File {
                return Err(FsError::NotFile);
            }
            if expect.is_some_and(|i| i != s.identity) {
                return Err(FsError::Changed);
            }
            // a replacement is allowed over ONE specific file only: the destination is opened (without following links) and
            // must be a plain single-link file with exactly the expected identity. (It cannot be pinned against deletion: Windows
            // refuses to replace a file that is open without delete sharing, which was tried and fails with a sharing violation. A
            // window between this check and the rename therefore remains, documented in docs/phase-3c.)
            let replace = match mode {
                Mode::NoReplace => false,
                Mode::Replace { dest } => {
                    let hd = nt_open(
                        to_dir.h,
                        &wide(to),
                        DELETE | SYNCHRONIZE | FILE_READ_ATTRIBUTES,
                        SHARE_ALL,
                        FILE_OPEN,
                        FILE_NON_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
                        0,
                        null(),
                    )?;
                    dest_handle = Some(hd);
                    let ds = handle_stat(hd)?;
                    if ds.kind == Kind::Link {
                        return Err(FsError::Link);
                    }
                    if ds.kind != Kind::File || ds.links != 1 || ds.identity != dest {
                        return Err(FsError::Changed);
                    }
                    true
                }
            };
            hook::fire("before-rename");
            let name = wide(to);
            // FILE_RENAME_INFORMATION_EX: Flags (u32, padded), RootDirectory (HANDLE), FileNameLength (u32), FileName[]
            let header = 20usize;
            let total = header + name.len() * 2;
            let mut buf = vec![0u64; total.div_ceil(8)];
            let base = buf.as_mut_ptr() as *mut u8;
            // POSIX_SEMANTICS, plus REPLACE_IF_EXISTS only for a verified replacement. Without the replace flag the operating
            // system refuses an existing destination (STATUS_OBJECT_NAME_COLLISION) atomically with the rename.
            let flags: u32 = 0x2 | if replace { 0x1 } else { 0 };
            // SAFETY: `buf` is large enough and 8-byte aligned; the offsets follow the documented structure layout (x64).
            unsafe {
                (base as *mut u32).write(flags);
                (base.add(8) as *mut Handle).write(to_dir.h);
                (base.add(16) as *mut u32).write((name.len() * 2) as u32);
                std::ptr::copy_nonoverlapping(name.as_ptr() as *const u8, base.add(20), name.len() * 2);
            }
            let mut iosb = Iosb { status: 0, information: 0 };
            // SAFETY: handle valid; the buffer holds a complete FILE_RENAME_INFORMATION_EX.
            let st = unsafe { NtSetInformationFile(h, &mut iosb, base as *const c_void, total as u32, FILE_RENAME_INFORMATION_EX) };
            if st >= 0 {
                Ok(false)
            } else {
                Err(map_status(st))
            }
        })();
        if let Some(hd) = dest_handle {
            close(hd);
        }
        close(h);
        r
    }

    pub fn filesystem_name(&self) -> Option<String> {
        let mut name = [0u16; 64];
        // SAFETY: the buffer is live and its length is passed; the other out-parameters are optional (null).
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetVolumeInformationByHandleW(
                self.h,
                null_mut(),
                0,
                null_mut(),
                null_mut(),
                null_mut(),
                name.as_mut_ptr(),
                name.len() as u32,
            )
        };
        if ok == 0 {
            return None;
        }
        let len = name.iter().position(|c| *c == 0).unwrap_or(name.len());
        Some(String::from_utf16_lossy(&name[..len]))
    }

    pub fn sync(&self) -> bool {
        // SAFETY: valid handle opened with write access to the directory.
        unsafe { FlushFileBuffers(self.h) != 0 }
    }
}

fn delete_by_handle(h: Handle) -> Result<(), FsError> {
    let mut iosb = Iosb { status: 0, information: 0 };
    let flags: u32 = 0x1 | 0x2; // FILE_DISPOSITION_DELETE | FILE_DISPOSITION_POSIX_SEMANTICS
                                // SAFETY: valid handle opened with DELETE; `flags` is a live u32.
    let mut st = unsafe { NtSetInformationFile(h, &mut iosb, &flags as *const u32 as *const c_void, 4, FILE_DISPOSITION_INFORMATION_EX) };
    if st < 0 && matches!(st as u32, 0xC000_0003 | 0xC000_000D | 0xC000_00BB) {
        // an older file system / OS without the extended class: plain delete-on-close
        let on: u8 = 1;
        // SAFETY: as above.
        st = unsafe { NtSetInformationFile(h, &mut iosb, &on as *const u8 as *const c_void, 1, FILE_DISPOSITION_INFORMATION) };
    }
    if st >= 0 {
        Ok(())
    } else {
        Err(map_status(st))
    }
}
