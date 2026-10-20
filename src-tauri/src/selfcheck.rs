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
        let closure = closure_probe(&conn).map_err(|e| e.to_string())?;
        let discovery_probe = crate::discovery_probe::synthetic_probe(&conn, &dir).map_err(|e| e.to_string())?;
        let plan_probe = plan_probe(&conn).map_err(|e| e.to_string())?;
        let lifecycle_probe = lifecycle_probe(&conn, &dir).map_err(|e| e.to_string())?;
        // what this very build could install: read from the service the application starts with (a release build: nothing)
        let install_supply_probe = crate::install_flow::supply_probe(&conn, dir.clone()).map_err(|e| e.to_string())?;
        // real-device acceptance of the handle-relative file-system layer: artificial files in a new folder below the data directory
        let fs_acceptance = crate::managed_fs_probe::run(&dir);
        // read-only Windows Hello availability (no key, no prompt)
        let hello = crate::os_hello::availability_probe();
        let report = health::run_and_record(&conn, &ctx).map_err(|e| e.to_string())?;
        let chain = audit::verify(&conn).map_err(|e| e.to_string())?;
        Ok(json!({
            "appVersion": env!("CARGO_PKG_VERSION"),
            "platform": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "dbVersion": db::user_version(&conn).map_err(|e| e.to_string())?,
            "scenarioResults": scenario_results,
            "preferenceProbe": probe,
            "closureProbe": closure,
            "discoveryProbe": discovery_probe,
            "planProbe": plan_probe,
            "lifecycleProbe": lifecycle_probe,
            "installSupplyProbe": install_supply_probe,
            "fsAcceptanceProbe": fs_acceptance,
            "windowsHelloProbe": hello,
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

/// Builds read-only installation plans for a few catalog entries with the built binary's planner, so the contract tests can
/// check them against the install-recipe schema. Nothing is executed or written.
fn plan_probe(conn: &rusqlite::Connection) -> crate::error::AppResult<serde_json::Value> {
    let host = crate::plan::HostFacts::detect();
    let mut plans = Vec::new();
    for (entry, platform, surface) in [
        ("codebase-memory-mcp", host.platform.as_str(), "local-cli"),
        ("superpowers", host.platform.as_str(), "claude-code-terminal"),
        ("graphify", host.platform.as_str(), "local-cli"),
        ("strix", host.platform.as_str(), "local-cli"),
    ] {
        // an unsupported host (neither Windows x64 nor macOS ARM64) has no valid platform label to plan for
        if !crate::plan::PLATFORMS.contains(&platform) {
            continue;
        }
        let request = crate::plan::PlanRequest {
            entry_id: entry.to_string(),
            platform_target: platform.to_string(),
            agent_surface: surface.to_string(),
            scope: "user".to_string(),
            method_id: None,
            project_alias: None,
        };
        plans.push(serde_json::to_value(crate::plan::plan_for(conn, &request, &host)?)?);
    }
    Ok(serde_json::Value::Array(plans))
}

/// Runs the SYNTHETIC lifecycle (recipe, plan, approval, install, repeat, update, failed update with rollback, crash and
/// recovery, uninstall) inside a throw-away sandbox below the self-check's own data directory, with made-up artifacts and
/// made-up authenticated references. Nothing real is installed, started or downloaded; the contract tests check the report.
fn lifecycle_probe(conn: &rusqlite::Connection, dir: &std::path::Path) -> Result<serde_json::Value, String> {
    use crate::{
        approval::Action,
        lifecycle::{Authority, Engine, Fault, Outcome, Point, Recovered, SandboxRoot, TxnError},
        synthetic::{approve_and_verify, Scenario},
    };
    let host = crate::plan::HostFacts::detect();
    if !crate::plan::PLATFORMS.contains(&host.platform.as_str()) {
        return Ok(json!({ "skipped": "unsupported_platform" }));
    }
    let platform = host.platform.as_str();
    // only a column template for the synthetic catalog entry; it never becomes a recipe
    let template = catalog::get_entry(conn, "gitleaks").map_err(|e| e.to_string())?;
    let tool = "synthetic-probe";
    let v1 = Scenario::new(&template, tool, "1.0.0", platform, b"probe artifact one");
    let v2 = Scenario::new(&template, tool, "2.0.0", platform, b"probe artifact two");
    let v3 = Scenario::new(&template, tool, "3.0.0", platform, b"probe artifact three");
    let mut refs = v1.refs.clone();
    for sc in [&v2, &v3] {
        refs.authenticated.extend(sc.refs.authenticated.clone());
    }
    let base = dir.join("lifecycle-probe");
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let computer = "synthetic-probe-computer";
    let root = SandboxRoot::create(&base, computer).map_err(|e| e.to_string())?;
    let clock = chrono::Utc::now;
    static FAIL_AFTER_SWITCH: fn(Point) -> Option<Fault> = |p| (p == Point::AfterPointerSwitch).then_some(Fault::Fail);
    static CRASH_AFTER_SWITCH: fn(Point) -> Option<Fault> = |p| (p == Point::AfterPointerSwitch).then_some(Fault::Crash);
    let engine = |fault: Option<&'static (dyn Fn(Point) -> Option<Fault> + Sync)>| Engine {
        root: &root,
        refs: &refs,
        host_platform: platform,
        clock: &clock,
        fault,
    };
    let approval = |sc: &Scenario, action: Action, from: Option<&str>| {
        let (plan, valid) = (sc.plan(), sc.validated());
        approve_and_verify(conn, &valid, &plan, platform, action, from, computer, chrono::Utc::now())
            .map(|a| (a, valid))
            .map_err(|e| e.code().to_string())
    };
    let plans: Vec<serde_json::Value> =
        [&v1, &v2].iter().map(|sc| serde_json::to_value(sc.plan()).map_err(|e| e.to_string())).collect::<Result<_, _>>()?;
    let detail = |r: Result<Outcome, TxnError>| match r {
        Ok(o) => json!({ "ok": true, "alreadyApplied": o.already_applied, "receipt": o.receipt }),
        Err(TxnError::Failed { code, rollback }) => json!({ "ok": false, "code": code, "rollback": rollback }),
        Err(TxnError::Interrupted(p)) => json!({ "ok": false, "interrupted": format!("{p:?}") }),
        Err(TxnError::Refused(c)) => json!({ "ok": false, "refused": c }),
        Err(TxnError::Busy) => json!({ "ok": false, "refused": "busy" }),
        Err(TxnError::Pending(_)) => json!({ "ok": false, "refused": "pending" }),
    };
    let mut steps: Vec<serde_json::Value> = Vec::new();
    let active = |e: &Engine| e.active_version(tool).ok().flatten();
    let mut record =
        |name: &str, e: &Engine, d: serde_json::Value| steps.push(json!({ "step": name, "activeVersion": active(e), "detail": d }));

    let (a1, r1) = approval(&v1, Action::Install, None)?;
    let e = engine(None);
    record("install", &e, detail(e.install(&Authority { recipe: &r1, approval: &a1 }, &v1.bytes)));
    record("install-again", &e, detail(e.install(&Authority { recipe: &r1, approval: &a1 }, &v1.bytes)));
    let (a2, r2) = approval(&v2, Action::Update, Some("1.0.0"))?;
    record("update", &e, detail(e.update(&Authority { recipe: &r2, approval: &a2 }, &v2.bytes)));
    // a failing update: the pointer was switched, then the step fails; rolled back to 2.0.0
    let (a3, r3) = approval(&v3, Action::Update, Some("2.0.0"))?;
    let failing = engine(Some(&FAIL_AFTER_SWITCH));
    record("failed-update", &e, detail(failing.update(&Authority { recipe: &r3, approval: &a3 }, &v3.bytes)));
    // a crash in the middle of the same update, then recovery as at the next start
    let (a3b, r3b) = approval(&v3, Action::Update, Some("2.0.0"))?;
    let crashing = engine(Some(&CRASH_AFTER_SWITCH));
    record("crashed-update", &e, detail(crashing.update(&Authority { recipe: &r3b, approval: &a3b }, &v3.bytes)));
    let recovered: Vec<&str> = e
        .recover()
        .map_err(|err| format!("{err:?}"))?
        .into_iter()
        .map(|r| match r {
            Recovered::RolledBack(_) => "rolled_back",
            Recovered::RolledForward(_) => "rolled_forward",
            Recovered::NeedsAttention(..) => "needs_attention",
            Recovered::CorruptJournal(_) => "corrupt_journal",
            Recovered::OrphanLockRemoved(_) => "orphan_lock_removed",
            Recovered::LeftoversRetained(..) => "leftovers_retained",
        })
        .collect();
    record("recovery", &e, json!({ "recovered": recovered }));
    let (a4, r4) = approval(&v2, Action::Uninstall, None)?;
    record("uninstall", &e, detail(e.uninstall(&Authority { recipe: &r4, approval: &a4 })));
    let sandbox_name = root.path().file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
    let entries: Vec<String> = std::fs::read_dir(&base)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().replace(&sandbox_name, "<sandbox>"))
        .collect();
    Ok(json!({
        "synthetic": true,
        "platform": platform,
        "recipe": serde_json::to_value(&v1.reviewed).map_err(|e| e.to_string())?,
        "recipeDigest": r1.digest(),
        "plans": plans,
        "steps": steps,
        "baseEntries": entries,
    }))
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

/// RT-1B-01 / RT-1B-02 in the built binary: the same task id in two projects is two tasks, and a removed
/// entry cannot come back under another classification.
fn closure_probe(conn: &rusqlite::Connection) -> crate::error::AppResult<serde_json::Value> {
    let mut shown = Vec::new();
    for alias in ["self-check-scope-a", "self-check-scope-b"] {
        projects::add_reference(conn, alias, alias, "")?;
        let input = recommend::TaskInput {
            project_alias: alias.into(),
            task_id: "shared-task-id".into(),
            task_text: "Design a React component".into(),
            evidence_files: vec![],
            agent_surface: "claude-code-terminal".into(),
            platform_target: "windows-x64".into(),
            phase: None,
            availability: vec![],
        };
        shown.push(recommend::evaluate(conn, &input)?.recommendations.len());
    }
    let url = "https://github.com/self-check/restore-probe";
    let entry = catalog::add_entry(conn, url, "agent_plugin", "")?;
    let fingerprint_before = entry.capability_fingerprint.clone();
    catalog::remove_entry(conn, &entry.entry_id)?;
    let refused = catalog::add_entry(conn, url, "executable_tool", "").err().map(|e| e.code);
    let restored = catalog::add_entry(conn, url, "agent_plugin", "")?;
    Ok(json!({
        "sameTaskIdShownPerProject": shown,
        "restoreWithOtherKindRefusedWith": refused,
        "restoredKind": restored.kind,
        "fingerprintUnchanged": restored.capability_fingerprint == fingerprint_before,
    }))
}
