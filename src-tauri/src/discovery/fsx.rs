//! The ONLY file-system access of the discovery engine. It is read-only by construction (no write,
//! create, rename, delete or execute call exists in this module) and conservative about links:
//!
//! * a symbolic link or Windows reparse point (junction, app-execution alias) is never followed, except
//!   that a link found *as the final file* may be reported as present-but-unverified;
//! * network paths (UNC) are never touched, so a scan cannot trigger network access;
//! * reads are size-capped and must be valid UTF-8 regular files.

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
    /// A symbolic link / junction / reparse point was found where a real file or directory was expected.
    Link,
    NotRegularFile,
    TooLarge,
    AccessDenied,
    NotUtf8,
    Io,
}

impl Skip {
    pub fn describe(self) -> &'static str {
        match self {
            Skip::Link => "sembolik bağlantı veya junction olduğu için izlenmedi",
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
    Found { path: PathBuf, is_link: bool },
    NotFound,
    Skipped(Skip),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReadOutcome {
    Text(String),
    NotFound,
    Skipped(Skip),
}

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

fn valid_component(c: &str) -> bool {
    !c.is_empty() && c != "." && c != ".." && !c.contains(['/', '\\', '\0', ':'])
}

/// Walks `rel` below `root` one component at a time, refusing links on the way. `root` itself must be a real
/// directory. Components are fixed program constants or validated plugin identifiers, never raw user input.
pub fn walk(root: &Path, rel: &[&str], final_kind: FinalKind, allow_final_link: bool) -> Walk {
    if is_network_or_device_path(root) {
        return Walk::Skipped(Skip::Link);
    }
    match stat(root) {
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
                    return if kind_ok { Walk::Found { path: current, is_link: false } } else { Walk::Skipped(Skip::NotRegularFile) };
                }
                if !m.is_dir() {
                    return Walk::Skipped(Skip::NotRegularFile);
                }
            }
        }
    }
    // An empty `rel` means the root itself.
    Walk::Found { path: current, is_link: false }
}

/// Reads one regular UTF-8 text file below `root` (links refused, size capped). Never writes.
pub fn read_text(root: &Path, rel: &[&str], max_bytes: u64) -> ReadOutcome {
    let path = match walk(root, rel, FinalKind::File, false) {
        Walk::Found { path, .. } => path,
        Walk::NotFound => return ReadOutcome::NotFound,
        Walk::Skipped(s) => return ReadOutcome::Skipped(s),
    };
    match stat(&path) {
        Ok(Some(m)) if m.len() > max_bytes => return ReadOutcome::Skipped(Skip::TooLarge),
        Err(s) => return ReadOutcome::Skipped(s),
        Ok(None) => return ReadOutcome::NotFound,
        Ok(Some(_)) => {}
    }
    let file = match fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ReadOutcome::NotFound,
        Err(e) => return ReadOutcome::Skipped(skip_of(&e)),
    };
    let mut bytes = Vec::new();
    // Read at most max+1 bytes so a file that grew after the size check is still caught.
    if let Err(e) = file.take(max_bytes + 1).read_to_end(&mut bytes) {
        return ReadOutcome::Skipped(skip_of(&e));
    }
    if bytes.len() as u64 > max_bytes {
        return ReadOutcome::Skipped(Skip::TooLarge);
    }
    match String::from_utf8(bytes) {
        Ok(text) => ReadOutcome::Text(text.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(text)),
        Err(_) => ReadOutcome::Skipped(Skip::NotUtf8),
    }
}

/// Is `rel` below `root` a real directory (not a link)?
pub fn dir_exists(root: &Path, rel: &[&str]) -> Walk {
    walk(root, rel, FinalKind::Dir, false)
}

/// Replaces the user's home directory prefix with `~` so stored locations never contain the account name.
pub fn collapse_home(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home {
        if let Ok(rest) = path.strip_prefix(home) {
            let tail = rest.to_string_lossy().replace('\\', "/");
            return if tail.is_empty() { "~".to_string() } else { format!("~/{tail}") };
        }
    }
    path.to_string_lossy().into_owned()
}
