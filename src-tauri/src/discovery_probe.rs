//! Headless entry points for the discovery engine, used by CI and by the Windows/macOS evidence runs:
//!
//! * `synthetic_probe` builds a throw-away fake home, PATH folder and project folder (with dummy secrets)
//!   inside the isolated self-check directory, scans it with the real engine of the built binary, and proves
//!   that nothing in the tree changed and no dummy secret reached the stored observations.
//! * `--discover-local <out.json>` scans THIS computer read-only (PATH names + Claude Code user files, no
//!   project folder) and writes the sanitized observations. Only the user running the command asked for it.

use crate::{
    discovery::{
        self,
        detect::{Authorization, ProjectScan, Roots, PLATFORM_MACOS, PLATFORM_WINDOWS},
        store,
    },
    error::{AppError, AppResult},
    projects,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

const DUMMY_SECRET: &str = "ghp_SELFCHECKDUMMYSECRETDUMMYSECRET0123";
const DUMMY_SECRET_2: &str = "sk-selfcheck-dummy-secret-value-0123456789";

fn exe(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

fn io(e: std::io::Error) -> AppError {
    AppError::new("PROBE_IO", format!("Sentetik ortam hazırlanamadı: {e}"))
}

fn write(path: &Path, content: &str) -> AppResult<()> {
    fs::create_dir_all(path.parent().unwrap()).map_err(io)?;
    fs::write(path, content).map_err(io)
}

/// A stable fingerprint of every file below `root`: relative name, size, modification time and content.
fn tree_fingerprint(root: &Path) -> AppResult<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) -> AppResult<()> {
        let mut entries: Vec<PathBuf> =
            fs::read_dir(dir).map_err(io)?.map(|e| e.map(|e| e.path())).collect::<Result<_, _>>().map_err(io)?;
        entries.sort();
        for path in entries {
            let meta = fs::symlink_metadata(&path).map_err(io)?;
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned();
            let content = if meta.is_file() { fs::read(&path).map_err(io)? } else { Vec::new() };
            let modified =
                meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
            out.push(format!(
                "{rel}|{}|{modified}|{}|{}",
                meta.len(),
                meta.permissions().readonly(),
                crate::audit::sha256_hex(&String::from_utf8_lossy(&content))
            ));
            if meta.is_dir() {
                walk(&path, root, out)?;
            }
        }
        Ok(())
    }
    let mut lines = Vec::new();
    walk(root, root, &mut lines)?;
    Ok(crate::audit::sha256_hex(&lines.join("\n")))
}

pub fn synthetic_probe(conn: &Connection, dir: &Path) -> AppResult<Value> {
    let base = dir.join("discovery-probe");
    let _ = fs::remove_dir_all(&base);
    let home = base.join("home");
    let bin = base.join("bin");
    let project = base.join("project");
    fs::create_dir_all(&bin).map_err(io)?;
    fs::create_dir_all(&project).map_err(io)?;
    let key = "frontend-design@claude-plugins-official";
    write(
        &home.join(".claude/settings.json"),
        &json!({ "enabledPlugins": { key: true }, "env": { "TOKEN": DUMMY_SECRET }, "apiKeyHelper": DUMMY_SECRET_2 }).to_string(),
    )?;
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &json!({ "version": 2, "plugins": { key: [{ "scope": "user", "installPath": "ignored", "version": "1.0.0" }] } }).to_string(),
    )?;
    fs::create_dir_all(home.join(".claude/plugins/cache/claude-plugins-official/frontend-design/1.0.0")).map_err(io)?;
    write(
        &home.join(".claude.json"),
        &json!({ "oauthAccount": { "accessToken": DUMMY_SECRET }, "mcpServers": { "codebase-memory": { "command": "codebase-memory-mcp", "args": [DUMMY_SECRET_2], "env": { "K": DUMMY_SECRET } } } }).to_string(),
    )?;
    write(&home.join(format!(".local/bin/{}", exe("claude"))), "stub")?;
    write(&bin.join(exe("gitleaks")), "stub")?;
    write(&project.join(".mcp.json"), &json!({ "mcpServers": { "codebase-memory": { "command": "codebase-memory-mcp", "headers": { "Authorization": format!("Bearer {DUMMY_SECRET}") } } } }).to_string())?;
    write(&project.join(".claude/settings.json"), &json!({ "enabledPlugins": { key: false } }).to_string())?;
    write(&project.join("unrelated-source.txt"), DUMMY_SECRET_2)?;

    projects::add_reference(conn, "probe-project", "Probe project", "")?;
    let roots = Roots {
        platform: if cfg!(windows) { PLATFORM_WINDOWS } else { PLATFORM_MACOS }.to_string(),
        home: Some(home.clone()),
        path_entries: vec![bin.clone()],
    };
    let auth = Authorization {
        path_executables: true,
        user_config: true,
        project: Some(ProjectScan { alias: "probe-project".into(), dir: project.clone() }),
    };
    let before = tree_fingerprint(&base)?;
    let summary = discovery::run_scan(conn, &roots, &auth)?;
    let again = discovery::run_scan(conn, &roots, &auth)?; // repeated scanning
    let after = tree_fingerprint(&base)?;
    let observations = store::latest_observations(conn)?;
    let dump = serde_json::to_string(&observations)?;
    Ok(json!({
        "scan": summary,
        "secondScanId": again.scan_id,
        "treeUnchanged": before == after,
        "dummySecretInObservations": dump.contains("SELFCHECK") || dump.contains("selfcheck-dummy") || dump.contains("Bearer"),
        "observations": observations,
    }))
}

/// `--discover-local <out.json>`: a read-only scan of this computer. Not part of the normal UI flow.
pub fn maybe_discover_local() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    let pos = args.iter().position(|a| a == "--discover-local")?;
    let Some(out) = args.get(pos + 1) else {
        eprintln!("--discover-local needs an output file path");
        return Some(2);
    };
    let dir = match std::env::var_os("ATM_DATA_DIR") {
        Some(d) => PathBuf::from(d),
        None => std::env::temp_dir().join(format!("atm-discover-{}", uuid::Uuid::new_v4())),
    };
    let result = (|| -> AppResult<Value> {
        fs::create_dir_all(&dir).map_err(io)?;
        let conn = crate::db::open_file(&dir.join("atm.sqlite"), Some(&dir.join("backups")))?;
        crate::catalog::seed_all(&conn)?;
        let roots = Roots::from_environment();
        let auth = Authorization { path_executables: true, user_config: true, project: None };
        let summary = discovery::run_scan(&conn, &roots, &auth)?;
        Ok(json!({ "scan": summary, "inventory": store::inventory(&conn)? }))
    })();
    Some(match result {
        Ok(v) => {
            if fs::write(out, serde_json::to_vec_pretty(&v).unwrap_or_default()).is_ok() {
                0
            } else {
                2
            }
        }
        Err(e) => {
            let _ = fs::write(out, json!({ "error": e.code }).to_string());
            2
        }
    })
}
