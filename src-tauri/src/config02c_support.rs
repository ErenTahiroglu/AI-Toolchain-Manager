//! CONFIG-02C test support (compiled for tests only): fixtures the tests build themselves, and the machine-visible result line.
//!
//! Every case of `docs/config-02c/THREAT_AND_PRIVACY_TEST_PLAN.md` that has Rust code ends in exactly ONE line
//! `CONFIG02C_RESULT|<id>|<os>|<PASS|FAIL|SKIPPED|BLOCKED>|<reason>`, printed to standard output and, when the environment variable
//! `CONFIG02C_RESULTS` names a file, appended to it. The reason is a fixed short code and never a path. A case that cannot be built here
//! (no privilege for a symlink, no mount, no way to deny access) says SKIPPED with the reason; a case that panics says FAIL; an id with no
//! line at all is NOT RUN. `tools/config02c_results_check.py` enforces that rule in CI. Nothing in this file touches a real user file: all
//! fixtures live below a directory this process creates under the runner's temporary area, with made-up names and content.
use crate::managed_fs::hook;
use std::{
    fs,
    io::Write,
    panic::{catch_unwind, resume_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
};

pub enum Verdict {
    Pass,
    Skipped(&'static str),
    /// The environment prevented the case from running (kept for completeness of the vocabulary; no case builds it today).
    #[allow(dead_code)]
    Blocked(&'static str),
}

pub fn os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else {
        "macos"
    }
}

fn emit(id: &str, status: &str, reason: &str) {
    let line = format!("CONFIG02C_RESULT|{id}|{}|{status}|{reason}", os());
    println!("{line}");
    if let Ok(path) = std::env::var("CONFIG02C_RESULTS") {
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// Runs one case and writes its single result line. A panic is recorded as FAIL and then re-raised, so the test still fails.
pub fn case(id: &str, body: impl FnOnce() -> Verdict) {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Verdict::Pass) => emit(id, "PASS", ""),
        Ok(Verdict::Skipped(why)) => emit(id, "SKIPPED", why),
        Ok(Verdict::Blocked(why)) => emit(id, "BLOCKED", why),
        Err(panic) => {
            emit(id, "FAIL", "assertion");
            resume_unwind(panic)
        }
    }
}

/// A unique, searchable stand-in for a secret: `ATM_SENTINEL_<8 hex>_<position>`.
pub fn sentinel(position: &str) -> String {
    format!("ATM_SENTINEL_{}_{position}", &uuid::Uuid::new_v4().simple().to_string()[..8])
}

/// Windows turns a canonical path into a verbatim `\\?\C:\...` one; the reader asks for the plain spelling.
pub fn plain(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text).to_string())
}

pub const CANARY: &[u8] = b"canary: must never change";

pub struct Bed {
    pub base: PathBuf,
    pub root: PathBuf,
    pub outside: PathBuf,
    _tmp: tempfile::TempDir, // last: handles must be closed before the folder goes away
}

impl Drop for Bed {
    fn drop(&mut self) {
        hook::clear();
    }
}

/// `<tmp>/project` (the root under test) and `<tmp>/outside` (holds a canary file that must never be read or changed).
pub fn bed() -> Bed {
    hook::clear();
    let tmp = tempfile::tempdir().unwrap();
    let base = plain(&tmp.path().canonicalize().unwrap());
    let root = base.join("project");
    let outside = base.join("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("canary.txt"), CANARY).unwrap();
    Bed { base, root, outside, _tmp: tmp }
}

impl Bed {
    pub fn target(&self) -> PathBuf {
        self.root.join(".mcp.json")
    }

    pub fn put(&self, bytes: &[u8]) {
        fs::write(self.target(), bytes).unwrap();
    }

    pub fn canary_untouched(&self) {
        assert_eq!(fs::read(self.outside.join("canary.txt")).unwrap(), CANARY);
        assert_eq!(fs::read_dir(&self.outside).unwrap().count(), 1, "nothing was created next to the canary");
    }
}

pub fn make_dir_link(link: &Path, target: &Path) -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
}

pub fn remove_dir_link(link: &Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd").args(["/C", "rmdir"]).arg(link).output();
    }
    #[cfg(unix)]
    {
        let _ = fs::remove_file(link);
    }
}

pub fn make_file_link(link: &Path, target: &Path) -> bool {
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link).is_ok()
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
}
