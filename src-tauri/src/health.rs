//! Deterministic, in-process operational health checks (Phase 0 §6 layer 1).
//!
//! These are NOT independent verification of any tool: nothing here runs, installs or inspects an
//! external tool, and no check may report `passed` unless it observed the property it names.
//! `unknown` and `skipped` are first-class results and never count as success.

use crate::{
    audit::{self, ChainReport, NewEvent},
    db,
    error::AppResult,
};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    NotAttempted,
    Created,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutostartState {
    NotQueried,
    Enabled,
    Disabled,
    QueryFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviousShutdown {
    FirstRun,
    Clean,
    Unclean,
}

#[derive(Debug, Clone, Copy)]
pub struct HealthContext {
    pub tray: TrayState,
    pub autostart: AutostartState,
    pub previous_shutdown: PreviousShutdown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub id: &'static str,
    pub title: &'static str,
    /// passed | failed | unknown | skipped
    pub status: &'static str,
    pub explanation: String,
    pub error_code: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub ran_at: String,
    /// failed | unknown | passed. Skipped checks are ignored; any failure wins.
    pub overall: &'static str,
    pub checks: Vec<CheckResult>,
}

fn check(
    id: &'static str,
    title: &'static str,
    status: &'static str,
    explanation: impl Into<String>,
    error_code: Option<&'static str>,
) -> CheckResult {
    CheckResult { id, title, status, explanation: explanation.into(), error_code }
}

fn scalar(conn: &Connection, sql: &str) -> Option<i64> {
    conn.query_row(sql, [], |r| r.get(0)).ok()
}

fn check_database(conn: &Connection) -> CheckResult {
    let integrity: Option<String> = conn.query_row("PRAGMA quick_check", [], |r| r.get(0)).ok();
    let version = db::user_version(conn).ok();
    match (integrity.as_deref(), version) {
        (Some("ok"), Some(v)) if v == db::LATEST_VERSION => check(
            "database",
            "Yerel veritabanı",
            "passed",
            format!("Veritabanı açıldı, bütünlük denetimi 'ok', şema sürümü {v} güncel."),
            None,
        ),
        (Some("ok"), Some(_)) => {
            check("database", "Yerel veritabanı", "failed", "Şema sürümü beklenen değil.", Some("DB_VERSION_MISMATCH"))
        }
        _ => check(
            "database",
            "Yerel veritabanı",
            "failed",
            "Veritabanı bütünlük denetimi başarısız veya okunamadı.",
            Some("DB_INTEGRITY_FAILED"),
        ),
    }
}

fn check_invariants(conn: &Connection) -> CheckResult {
    let tools = scalar(conn, "SELECT count(*) FROM tools");
    let unreviewed = scalar(conn, "SELECT count(*) FROM tools WHERE review_state = 'unreviewed'");
    let managed = scalar(conn, "SELECT count(*) FROM tools WHERE install_state <> 'not_managed' OR validation_state <> 'unknown'");
    let real_projects = scalar(conn, "SELECT count(*) FROM projects WHERE is_synthetic <> 1");
    let sources = scalar(conn, "SELECT count(*) FROM prompt_sources WHERE status = 'planned' AND read_only = 1");
    let expected = crate::catalog::EXPECTED_TOOL_COUNT as i64;
    let ok =
        tools == Some(expected) && unreviewed == Some(expected) && managed == Some(0) && real_projects == Some(0) && sources == Some(3);
    if ok {
        check(
            "phase-1a-invariants", "Phase 1A güvenlik kuralları", "passed",
            "9 araç kayıtlı ve hepsi 'incelenmemiş'; hiçbir araç kurulu veya doğrulanmış görünmüyor; yalnızca örnek projeler var; 3 istem kaynağı salt okunur.",
            None,
        )
    } else {
        check(
            "phase-1a-invariants",
            "Phase 1A güvenlik kuralları",
            "failed",
            "Katalog veya durum kuralları beklenen değerlerde değil.",
            Some("INVARIANT_VIOLATION"),
        )
    }
}

fn check_chain(report: &AppResult<ChainReport>) -> CheckResult {
    match report {
        Ok(r) if r.ok && r.count > 0 => check(
            "audit-chain",
            "Denetim kaydı bütünlüğü",
            "passed",
            format!("{} kayıtlık zincir tutarlı. (Aynı kullanıcının yeniden yazmasına karşı kanıt değildir.)", r.count),
            None,
        ),
        Ok(r) if r.ok => check("audit-chain", "Denetim kaydı bütünlüğü", "unknown", "Henüz denetim kaydı yok.", None),
        Ok(_) => check(
            "audit-chain",
            "Denetim kaydı bütünlüğü",
            "failed",
            "Denetim zinciri tutarsız; kayıt değiştirilmiş veya bozulmuş olabilir.",
            Some("AUDIT_CHAIN_BROKEN"),
        ),
        Err(_) => check("audit-chain", "Denetim kaydı bütünlüğü", "failed", "Denetim zinciri okunamadı.", Some("AUDIT_CHAIN_UNREADABLE")),
    }
}

const CAPABILITIES_JSON: &str = include_str!("../capabilities/default.json");
const FORBIDDEN_PERMISSION_PARTS: &[&str] = &["shell", "fs", "sql", "http", "opener", "process", "dialog", "updater", "os:", "core:"];

/// Evaluate a capability manifest against the least-privilege policy.
pub fn capability_violations(manifest: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    if manifest.get("windows") != Some(&serde_json::json!(["main"])) {
        problems.push("windows".to_string());
    }
    let Some(perms) = manifest.get("permissions").and_then(Value::as_array) else {
        return vec!["permissions".to_string()];
    };
    for p in perms {
        match p.as_str() {
            Some(s) if s.starts_with("allow-") && !FORBIDDEN_PERMISSION_PARTS.iter().any(|bad| s.contains(bad)) => {}
            other => problems.push(format!("{other:?}")),
        }
    }
    problems
}

fn check_capabilities() -> CheckResult {
    match serde_json::from_str::<Value>(CAPABILITIES_JSON) {
        Ok(manifest) => {
            let problems = capability_violations(&manifest);
            if problems.is_empty() {
                check(
                    "capabilities",
                    "WebView izinleri",
                    "passed",
                    "Pencere yalnızca uygulamanın kendi tanımlı komutlarını çağırabilir; kabuk, dosya sistemi, SQL veya ağ izni yok.",
                    None,
                )
            } else {
                check(
                    "capabilities",
                    "WebView izinleri",
                    "failed",
                    "İzin bildirimi en az ayrıcalık kuralını ihlal ediyor.",
                    Some("CAPABILITY_POLICY_VIOLATION"),
                )
            }
        }
        Err(_) => check("capabilities", "WebView izinleri", "failed", "İzin bildirimi okunamadı.", Some("CAPABILITY_UNREADABLE")),
    }
}

fn check_tray(ctx: &HealthContext) -> CheckResult {
    match ctx.tray {
        TrayState::Created => check("tray", "Sistem tepsisi", "passed", "Tepsi simgesi bu oturumda oluşturuldu.", None),
        TrayState::Failed => check(
            "tray",
            "Sistem tepsisi",
            "failed",
            "Tepsi simgesi oluşturulamadı; pencere kapatılınca uygulama tepsiye gizlenmez, kapanır.",
            Some("TRAY_CREATE_FAILED"),
        ),
        TrayState::NotAttempted => check("tray", "Sistem tepsisi", "skipped", "Bu çalıştırma pencere açmadığı için tepsi denenmedi.", None),
    }
}

fn check_startup(conn: &Connection, ctx: &HealthContext) -> CheckResult {
    let preference = crate::settings::get(conn, "startup_enabled").map(|v| v == "true").ok();
    match (ctx.autostart, preference) {
        (AutostartState::NotQueried, _) => {
            check("startup", "Başlangıçta çalıştırma", "skipped", "Sistem durumu bu çalıştırmada sorgulanmadı.", None)
        }
        (AutostartState::QueryFailed, _) => check(
            "startup",
            "Başlangıçta çalıştırma",
            "failed",
            "İşletim sistemi başlangıç durumu okunamadı.",
            Some("STARTUP_QUERY_FAILED"),
        ),
        (AutostartState::Enabled, Some(true)) | (AutostartState::Disabled, Some(false)) => {
            check("startup", "Başlangıçta çalıştırma", "passed", "Kayıtlı tercih ile işletim sistemi durumu aynı.", None)
        }
        _ => check(
            "startup",
            "Başlangıçta çalıştırma",
            "failed",
            "Kayıtlı tercih ile işletim sistemi durumu uyuşmuyor.",
            Some("STARTUP_MISMATCH"),
        ),
    }
}

fn check_previous_shutdown(ctx: &HealthContext) -> CheckResult {
    match ctx.previous_shutdown {
        PreviousShutdown::Clean => check("previous-shutdown", "Önceki kapanış", "passed", "Önceki oturum düzgün kapandı.", None),
        PreviousShutdown::FirstRun => {
            check("previous-shutdown", "Önceki kapanış", "skipped", "İlk çalıştırma; karşılaştırılacak önceki oturum yok.", None)
        }
        PreviousShutdown::Unclean => check(
            "previous-shutdown",
            "Önceki kapanış",
            "failed",
            "Önceki oturum beklenmedik şekilde sonlandı (çökme, kapatma veya elektrik kesintisi olabilir).",
            Some("UNCLEAN_SHUTDOWN"),
        ),
    }
}

pub fn run(conn: &Connection, ctx: &HealthContext) -> HealthReport {
    let chain = audit::verify(conn);
    let checks = vec![
        check_database(conn),
        check_invariants(conn),
        check_chain(&chain),
        check_capabilities(),
        check_tray(ctx),
        check_startup(conn, ctx),
        check_previous_shutdown(ctx),
        check(
            "single-instance",
            "Tek uygulama örneği",
            "unknown",
            "Eklenti kayıtlı, ancak uygulama kendi içinden ikinci başlatmayı gözleyemez; bu dışarıdan test edilmelidir.",
            None,
        ),
        check(
            "adapters",
            "Araç bağdaştırıcıları",
            "skipped",
            "Phase 1A'da bağdaştırıcı yok; hiçbir araç kurulmaz, güncellenmez veya değiştirilmez.",
            None,
        ),
        check("updater", "Uygulama güncelleyicisi", "skipped", "Phase 1A'da güncelleyici yok.", None),
    ];
    let overall = if checks.iter().any(|c| c.status == "failed") {
        "failed"
    } else if checks.iter().any(|c| c.status == "unknown") {
        "unknown"
    } else {
        "passed"
    };
    HealthReport { ran_at: audit::now_utc(), overall, checks }
}

/// Run the checks and append one audit event per check. A check that could not be recorded makes
/// the run fail visibly rather than silently losing evidence.
pub fn run_and_record(conn: &Connection, ctx: &HealthContext) -> AppResult<HealthReport> {
    let report = run(conn, ctx);
    for c in &report.checks {
        audit::append(
            conn,
            &NewEvent {
                event_type: "health_check",
                outcome: c.status,
                check_id: c.id,
                error_code: c.error_code,
                verdict_source: "operational-health",
                actor_type: "manager",
            },
        )?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx() -> HealthContext {
        HealthContext {
            tray: TrayState::NotAttempted,
            autostart: AutostartState::NotQueried,
            previous_shutdown: PreviousShutdown::FirstRun,
        }
    }

    fn seeded() -> Connection {
        let conn = db::open_memory().unwrap();
        crate::catalog::seed_all(&conn).unwrap();
        conn
    }

    fn status<'a>(r: &'a HealthReport, id: &str) -> &'a str {
        r.checks.iter().find(|c| c.id == id).unwrap().status
    }

    #[test]
    fn healthy_seeded_database_never_reports_failed_but_is_not_fully_verified() {
        let conn = seeded();
        audit::append(
            &conn,
            &NewEvent {
                event_type: "health_check",
                outcome: "passed",
                check_id: "x",
                error_code: None,
                verdict_source: "operational-health",
                actor_type: "manager",
            },
        )
        .unwrap();
        let report = run(&conn, &ctx());
        assert_eq!(status(&report, "database"), "passed");
        assert_eq!(status(&report, "phase-1a-invariants"), "passed");
        assert_eq!(status(&report, "audit-chain"), "passed");
        assert_eq!(status(&report, "capabilities"), "passed");
        // Single-instance cannot be observed in-process, so the run can never claim a full pass.
        assert_eq!(status(&report, "single-instance"), "unknown");
        assert_eq!(report.overall, "unknown");
    }

    #[test]
    fn missing_seed_is_a_failure_not_a_success() {
        let conn = db::open_memory().unwrap();
        let report = run(&conn, &ctx());
        assert_eq!(status(&report, "phase-1a-invariants"), "failed");
        assert_eq!(report.overall, "failed");
    }

    #[test]
    fn broken_audit_chain_fails_overall() {
        let conn = seeded();
        for _ in 0..3 {
            audit::append(
                &conn,
                &NewEvent {
                    event_type: "health_check",
                    outcome: "passed",
                    check_id: "x",
                    error_code: None,
                    verdict_source: "operational-health",
                    actor_type: "manager",
                },
            )
            .unwrap();
        }
        conn.execute_batch("DROP TRIGGER audit_events_no_delete; DELETE FROM audit_events WHERE sequence = 2;").unwrap();
        let report = run(&conn, &ctx());
        assert_eq!(status(&report, "audit-chain"), "failed");
        assert_eq!(report.overall, "failed");
    }

    #[test]
    fn unclean_shutdown_tray_failure_and_startup_mismatch_are_failures() {
        let conn = seeded();
        let bad =
            HealthContext { tray: TrayState::Failed, autostart: AutostartState::Enabled, previous_shutdown: PreviousShutdown::Unclean };
        let report = run(&conn, &bad);
        assert_eq!(status(&report, "tray"), "failed");
        assert_eq!(status(&report, "previous-shutdown"), "failed");
        assert_eq!(status(&report, "startup"), "failed", "OS says enabled but the stored preference is the default 'false'");
        assert_eq!(report.overall, "failed");
    }

    #[test]
    fn run_and_record_writes_one_chained_event_per_check() {
        let conn = seeded();
        let report = run_and_record(&conn, &ctx()).unwrap();
        let events = audit::list_ascending(&conn).unwrap();
        assert_eq!(events.len(), report.checks.len());
        assert!(audit::verify(&conn).unwrap().ok);
        assert!(events.iter().all(|e| e["verdictSource"] == "operational-health"));
        assert!(events.iter().all(|e| e["eventType"] == "health_check"));
        // Honesty: no recorded event may claim an independent verification.
        assert!(events.iter().all(|e| e["eventType"] != "verify"));
    }

    #[test]
    fn capability_policy_rejects_dangerous_permissions() {
        assert!(capability_violations(&json!({"windows":["main"],"permissions":["allow-list-tools"]})).is_empty());
        for bad in
            ["shell:allow-execute", "fs:default", "sql:allow-execute", "http:default", "opener:default", "core:default", "allow-shell-open"]
        {
            assert!(!capability_violations(&json!({"windows":["main"],"permissions":[bad]})).is_empty(), "{bad}");
        }
        assert!(!capability_violations(&json!({"windows":["*"],"permissions":[]})).is_empty());
    }

    #[test]
    fn shipped_capability_manifest_matches_build_command_list() {
        let build_rs = include_str!("../build.rs");
        let manifest: Value = serde_json::from_str(CAPABILITIES_JSON).unwrap();
        let mut granted: Vec<String> = manifest["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().trim_start_matches("allow-").replace('-', "_"))
            .collect();
        granted.sort();
        let mut declared: Vec<String> =
            build_rs.lines().filter_map(|l| l.trim().strip_prefix('"').and_then(|l| l.strip_suffix("\",")).map(str::to_string)).collect();
        declared.sort();
        assert_eq!(granted, declared);
    }
}
