//! Phase 2A: read-only local discovery. See docs/PHASE-2A.md.
//!
//! Nothing in this module executes a program, starts an agent or MCP server, opens a network
//! connection, downloads, installs, or writes anywhere except this application's own database.

pub mod detect;
pub mod fsx;
pub mod privacy;
pub mod reference;
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
    /// Allow reading the CONTENT of program files found by name, only to compute a SHA-256 fingerprint. Nothing is run.
    #[serde(default)]
    pub fingerprint_artifacts: bool,
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
    Ok(Authorization {
        path_executables: request.path_executables,
        user_config: request.user_config,
        project,
        // Fingerprinting reads file content of programs found on PATH, so it is meaningless (and refused) without the PATH look.
        fingerprint_artifacts: request.fingerprint_artifacts && request.path_executables,
    })
}

/// The origin each (non-removed) catalog entry pins: `owner/repo` parsed from its repository link.
pub fn load_pins(conn: &Connection) -> AppResult<Vec<detect::CatalogPin>> {
    let mut stmt = conn.prepare("SELECT entry_id, repo_url FROM catalog_entries WHERE removed_at IS NULL")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut pins = Vec::new();
    for row in rows {
        let (id, url) = row?;
        if let Some(pin) = detect::CatalogPin::from_repo_url(&id, &url) {
            pins.push(pin);
        }
    }
    Ok(pins)
}

pub fn run_scan(conn: &Connection, roots: &Roots, auth: &Authorization) -> AppResult<ScanSummary> {
    let started = now_utc();
    // Roots built by the caller may already carry pins (tests); otherwise the catalog's own pins are used.
    let mut roots = roots.clone();
    if roots.pins.is_empty() {
        roots.pins = load_pins(conn)?;
    }
    // Reference digests: the compiled-in authenticated list plus the database's UNAUTHENTICATED rows. A caller (tests, the
    // synthetic probe) may supply its own set; production never does.
    if roots.references == reference::ReferenceSet::default() {
        roots.references = reference::ReferenceSet::production(conn)?;
    }
    let roots = &roots;
    let observations = detect::detect_all(roots, auth);
    store::save_scan(conn, roots, auth, &observations, &started)
}
