//! Phase 3D-3: the narrowly constrained local artifact provider for the first real pilot.
//!
//! It answers one question -- "what are the bytes of the file this exact recipe names?" -- by reading ONE file, by the file name written in the
//! recipe, from ONE fixed folder that the application chose (never the interface), through the handle-relative, no-follow file layer:
//!
//! * the folder is opened without following a link; the file must be a plain regular file with exactly one link;
//! * its size must equal the size in the recipe (a different size is refused before a single byte is read);
//! * it is read once, bounded by that size; nothing is written, renamed, deleted or executed -- the provider only reads;
//! * the SHA-256, the format and the authenticated reference are judged by the callers (`check_artifact_for`) on the very bytes returned here,
//!   immediately before the preview is offered and again immediately before the transaction.
//!
//! The provider has no parameter for a path, a URL or a command, and it is not part of the release supply: nothing in a release build
//! constructs one.
#![allow(dead_code)] // wired in only when a reviewed change enables a real recipe; used by tests and the pilot feature today

use crate::{
    install_flow::ArtifactProvider,
    managed_fs::{Dir, FsError, Kind},
};
use std::path::PathBuf;

/// What the provider may be asked for: exactly the recipes it was built for.
#[derive(Debug, Clone)]
pub struct RetainedEntry {
    pub tool_id: String,
    pub version: String,
    pub platform_target: String,
    pub file_name: String,
    pub size_bytes: u64,
}

pub struct RetainedFileProvider {
    folder: PathBuf,
    entries: Vec<RetainedEntry>,
}

impl RetainedFileProvider {
    pub fn new(folder: PathBuf, entries: Vec<RetainedEntry>) -> RetainedFileProvider {
        RetainedFileProvider { folder, entries }
    }
}

impl ArtifactProvider for RetainedFileProvider {
    fn bytes(&self, tool: &str, version: &str, platform: &str) -> Result<Vec<u8>, &'static str> {
        let e = self
            .entries
            .iter()
            .find(|e| e.tool_id == tool && e.version == version && e.platform_target == platform)
            .ok_or("artifact_not_available")?;
        if e.size_bytes == 0 || e.size_bytes > 256 * 1024 * 1024 {
            return Err("artifact_size_mismatch");
        }
        let dir = Dir::open_anchor(&self.folder).map_err(|e| match e {
            FsError::NotFound => "artifact_not_available",
            _ => "artifact_not_a_plain_file",
        })?;
        let stat = match dir.stat(&e.file_name) {
            Ok(Some(s)) => s,
            Ok(None) | Err(FsError::NotFound) => return Err("artifact_not_available"),
            Err(_) => return Err("artifact_not_a_plain_file"),
        };
        if stat.kind != Kind::File || stat.links != 1 {
            return Err("artifact_not_a_plain_file");
        }
        let mut file = dir.open_file(&e.file_name).map_err(|_| "artifact_not_a_plain_file")?;
        let opened = file.stat().map_err(|_| "artifact_not_a_plain_file")?;
        if opened.kind != Kind::File || opened.links != 1 || opened.identity != stat.identity {
            return Err("artifact_not_a_plain_file");
        }
        if opened.size != e.size_bytes {
            return Err("artifact_size_mismatch");
        }
        file.read_bounded(e.size_bytes).map_err(|_| "artifact_not_a_plain_file")
    }
}

#[cfg(test)]
#[path = "artifact_provider_tests.rs"]
mod tests;
