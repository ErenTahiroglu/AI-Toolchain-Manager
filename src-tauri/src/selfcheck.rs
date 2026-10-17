//! `ai-toolchain-manager --self-check <output.json>`: opens an isolated database, seeds it, runs the
//! health checks without any window/tray, and writes a JSON report. Used by CI and by
//! tests/test_phase1a_audit_contract.py. It never touches the real user data directory: it uses
//! `ATM_DATA_DIR` when set, otherwise a fresh temporary directory.

use crate::{audit, catalog, db, health};
use serde_json::json;
use std::path::PathBuf;

pub fn maybe_run() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    let pos = args.iter().position(|a| a == "--self-check")?;
    let Some(out) = args.get(pos + 1) else {
        eprintln!("--self-check needs an output file path");
        return Some(2);
    };
    Some(run(PathBuf::from(out)))
}

fn run(out: PathBuf) -> i32 {
    let dir = match std::env::var_os("ATM_DATA_DIR") {
        Some(d) => PathBuf::from(d),
        None => std::env::temp_dir().join(format!("atm-self-check-{}", uuid::Uuid::new_v4())),
    };
    let result = (|| -> Result<serde_json::Value, String> {
        std::fs::create_dir_all(&dir).map_err(|e| format!("data dir: {e}"))?;
        let conn = db::open_file(&dir.join("atm.sqlite"), Some(&dir.join("backups"))).map_err(|e| e.to_string())?;
        catalog::seed_all(&conn).map_err(|e| e.to_string())?;
        let ctx = health::HealthContext {
            tray: health::TrayState::NotAttempted,
            autostart: health::AutostartState::NotQueried,
            previous_shutdown: health::PreviousShutdown::FirstRun,
        };
        let report = health::run_and_record(&conn, &ctx).map_err(|e| e.to_string())?;
        let chain = audit::verify(&conn).map_err(|e| e.to_string())?;
        Ok(json!({
            "appVersion": env!("CARGO_PKG_VERSION"),
            "platform": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "dbVersion": db::user_version(&conn).map_err(|e| e.to_string())?,
            "report": report,
            "auditChain": chain,
            "auditEvents": audit::list_ascending(&conn).map_err(|e| e.to_string())?,
        }))
    })();
    match result {
        Ok(value) => {
            if std::fs::write(&out, serde_json::to_vec_pretty(&value).unwrap_or_default()).is_err() {
                return 2;
            }
            // Exit code 1 only for a real failure; "unknown"/"skipped" are reported but not failures.
            if value["report"]["overall"] == "failed" || value["auditChain"]["ok"] != true {
                1
            } else {
                0
            }
        }
        Err(why) => {
            let _ = std::fs::write(&out, json!({ "error": crate::redact::sanitize(&why, 300) }).to_string());
            2
        }
    }
}
