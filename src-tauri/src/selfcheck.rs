//! `ai-toolchain-manager --self-check <output.json>`: opens an isolated database, seeds it, runs the
//! health checks without any window/tray, and writes a JSON report. Used by CI and by
//! tests/test_phase1a_audit_contract.py. It never touches the real user data directory: it uses
//! `ATM_DATA_DIR` when set, otherwise a fresh temporary directory.

use crate::{audit, catalog, db, health, projects, recommend};
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
        let scenario_results = run_scenarios(&conn).map_err(|e| e.to_string())?;
        let probe = preference_probe(&conn).map_err(|e| e.to_string())?;
        let report = health::run_and_record(&conn, &ctx).map_err(|e| e.to_string())?;
        let chain = audit::verify(&conn).map_err(|e| e.to_string())?;
        Ok(json!({
            "appVersion": env!("CARGO_PKG_VERSION"),
            "platform": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "dbVersion": db::user_version(&conn).map_err(|e| e.to_string())?,
            "scenarioResults": scenario_results,
            "preferenceProbe": probe,
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

/// Runs every synthetic scenario through the real engine of the built binary (read-only apart from the
/// "already shown" log) so CI can compare the results with `data/recommendation-scenarios.json`.
fn run_scenarios(conn: &rusqlite::Connection) -> crate::error::AppResult<serde_json::Value> {
    projects::add_reference(conn, "self-check-scenarios", "Self-check scenarios", "")?;
    let mut out = Vec::new();
    for (i, s) in recommend::scenarios()?.iter().enumerate() {
        let input = recommend::TaskInput {
            project_alias: "self-check-scenarios".into(),
            task_id: format!("self-check-{i}"),
            task_text: s.task_text.clone(),
            evidence_files: s.evidence_files.clone(),
            agent_surface: "claude-code-terminal".into(),
            platform_target: "windows-x64".into(),
            phase: None,
            availability: vec![],
        };
        let eval = recommend::evaluate(conn, &input)?;
        out.push(json!({
            "id": s.id,
            "recommendations": eval.recommendations.iter().map(|r| (r.entry_id.clone(), json!(r.level))).collect::<serde_json::Map<_, _>>(),
            "suppressed": eval.suppressed.iter().map(|x| (x.entry_id.clone(), json!(x.reason_code))).collect::<serde_json::Map<_, _>>(),
        }));
    }
    Ok(json!(out))
}

/// Exercises reject / postpone / reset / catalog-remove so their audit events (which the Phase 0 schema
/// defines) are produced by the built binary and validated by the Python contract test.
fn preference_probe(conn: &rusqlite::Connection) -> crate::error::AppResult<serde_json::Value> {
    projects::add_reference(conn, "self-check-probe", "Self-check probe", "")?;
    let key = |entry: &str, surface: &str, decision: &str| {
        projects::set_preference(conn, "self-check-probe", entry, surface, "windows-x64", decision, None)
    };
    key("agent-browser", "local-cli", "rejected")?;
    key("agentic-awesome-skills", "claude-code-terminal", "postponed")?;
    projects::reset_preference(conn, "self-check-probe", "agent-browser", "local-cli", "windows-x64")?;
    catalog::remove_entry(conn, "betterleaks")?;
    catalog::restore_entry(conn, "betterleaks")?;
    // Things that must be refused:
    let refused = [
        key("strix", "local-cli", "accepted").err().map(|e| e.code),
        key("anthropic-frontend-design", "antigravity-ide", "accepted").err().map(|e| e.code),
        key("gitleaks", "local-cli", "installed").err().map(|e| e.code),
    ];
    Ok(json!({ "refused": refused }))
}
