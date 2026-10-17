//! Phase 2A: read-only local discovery. See docs/PHASE-2A.md.
//!
//! Nothing in this module executes a program, starts an agent or MCP server, opens a network
//! connection, downloads, installs, or writes anywhere except this application's own database.

pub mod detect;
pub mod fsx;
pub mod privacy;
pub mod store;
pub mod types;

#[cfg(test)]
mod tests;

use crate::{
    audit::now_utc,
    error::{AppError, AppResult},
};
use detect::{Authorization, ProjectScan, Roots};
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};
use store::ScanSummary;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRequest {
    pub alias: String,
    /// A folder the user chose by typing it. It is used for this scan only and is never stored or logged.
    pub directory: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    pub path_executables: bool,
    pub user_config: bool,
    pub project: Option<ProjectRequest>,
}

fn dir_error(code: &'static str, message: &str) -> AppError {
    AppError::new(code, message)
}

/// The user's chosen project folder must be an absolute, local, real directory: no relative or `..` paths, no
/// network shares, no links, not a drive/filesystem root.
pub fn validate_project_dir(raw: &str) -> AppResult<PathBuf> {
    let text = raw.trim();
    if text.is_empty() || text.chars().count() > 260 || text.chars().any(|c| c.is_control()) {
        return Err(dir_error("PROJECT_DIR_INVALID", "Klasör yolu boş, çok uzun veya geçersiz karakterler içeriyor."));
    }
    let path = Path::new(text);
    if !path.is_absolute() {
        return Err(dir_error(
            "PROJECT_DIR_INVALID",
            "Klasör yolu tam (mutlak) bir yol olmalı, örneğin C:\\Projeler\\site veya /Users/ad/site.",
        ));
    }
    if fsx::is_network_or_device_path(path) {
        return Err(dir_error("PROJECT_DIR_NETWORK", "Ağ paylaşımları ve aygıt yolları taranmaz; yerel bir klasör seçin."));
    }
    // `Path::components` does not recognise `..` inside Windows verbatim (`\?\`) paths, so check the text itself.
    let dotted = text.split(['/', '\\']).any(|segment| segment == ".." || segment == ".");
    if dotted || path.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
        return Err(dir_error("PROJECT_DIR_INVALID", "Klasör yolunda '.' veya '..' bölümleri olamaz."));
    }
    if path.parent().is_none() {
        return Err(dir_error(
            "PROJECT_DIR_TOO_BROAD",
            "Bir sürücünün veya dosya sisteminin kök klasörü seçilemez; belirli bir proje klasörü seçin.",
        ));
    }
    // The WHOLE chain from the volume root down to the folder is checked (not only the last component): a link,
    // junction, `subst`/mapped drive or network file system anywhere above it is refused.
    match fsx::walk(path, &[], fsx::FinalKind::Dir, false) {
        fsx::Walk::Found { .. } => Ok(path.to_path_buf()),
        fsx::Walk::NotFound => Err(dir_error("PROJECT_DIR_NOT_FOUND", "Bu klasör bulunamadı veya erişilemiyor.")),
        fsx::Walk::Skipped(fsx::Skip::Link) => Err(dir_error(
            "PROJECT_DIR_LINK",
            "Bu klasör veya üst klasörlerinden biri sembolik bağlantı/junction ya da başka bir yere yönlendirilmiş; güvenlik için izlenmez. Gerçek klasörün yolunu yazın.",
        )),
        fsx::Walk::Skipped(fsx::Skip::Network) => {
            Err(dir_error("PROJECT_DIR_NETWORK", "Ağ paylaşımları, uzak sürücüler ve aygıt yolları taranmaz; yerel bir klasör seçin."))
        }
        fsx::Walk::Skipped(fsx::Skip::AccessDenied) => {
            Err(dir_error("PROJECT_DIR_UNREADABLE", "Bu klasöre veya üst klasörlerinden birine erişim izni yok."))
        }
        fsx::Walk::Skipped(fsx::Skip::NotRegularFile) => Err(dir_error("PROJECT_DIR_NOT_A_DIRECTORY", "Bu yol bir klasör değil.")),
        fsx::Walk::Skipped(_) => Err(dir_error("PROJECT_DIR_INVALID", "Bu klasör güvenli biçimde doğrulanamadı; taranmadı.")),
    }
}

/// Turns the user's request into an authorization. At least one kind of look is required.
pub fn authorize(conn: &Connection, request: &ScanRequest) -> AppResult<Authorization> {
    let project = match &request.project {
        None => None,
        Some(p) => {
            let exists: Option<i64> = conn
                .query_row("SELECT 1 FROM project_registry WHERE alias = ?1 AND removed_at IS NULL", [&p.alias], |r| r.get(0))
                .optional()?;
            if exists.is_none() {
                return Err(AppError::new("PROJECT_NOT_FOUND", "Böyle bir proje kaydı yok."));
            }
            Some(ProjectScan { alias: p.alias.clone(), dir: validate_project_dir(&p.directory)? })
        }
    };
    if !request.path_executables && !request.user_config && project.is_none() {
        return Err(AppError::new("SCAN_NOT_AUTHORIZED", "Taramak için en az bir bölüme izin vermelisiniz; hiçbir şey okunmadı."));
    }
    Ok(Authorization { path_executables: request.path_executables, user_config: request.user_config, project })
}

pub fn run_scan(conn: &Connection, roots: &Roots, auth: &Authorization) -> AppResult<ScanSummary> {
    let started = now_utc();
    let observations = detect::detect_all(roots, auth);
    store::save_scan(conn, roots, auth, &observations, &started)
}
