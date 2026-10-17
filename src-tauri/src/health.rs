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
    const ID: &str = "catalog-invariants";
    const TITLE: &str = "Katalog ve tercih güvenlik kuralları";
    let seeded = scalar(conn, "SELECT count(*) FROM catalog_entries WHERE origin = 'seed'");
    let claims =
        scalar(conn, "SELECT count(*) FROM catalog_entries WHERE install_state <> 'not_managed' OR verification_state <> 'unknown'");
    let bad_prefs = scalar(
        conn,
        "SELECT count(*) FROM project_preferences WHERE actual_install_state <> 'not_managed' OR actual_verification_state <> 'unknown' OR observed_scope <> 'unknown'",
    );
    let bad_kind = scalar(
        conn,
        "SELECT count(*) FROM catalog_entries WHERE kind NOT IN ('executable_tool','agent_plugin','skill_collection','prompt_source')",
    );
    let unreviewed_claims =
        scalar(conn, "SELECT count(*) FROM catalog_entries WHERE origin = 'seed' AND review_state IN ('metadata-reviewed') ");
    // Strix must stay quarantined and execution-blocked; no entry may be auto-runnable.
    let strix_ok = scalar(
        conn,
        "SELECT count(*) FROM catalog_entries WHERE entry_id = 'strix' AND review_state = 'quarantined' AND execution_policy = 'blocked-until-scoped-authorization'",
    );
    let free_run = scalar(
        conn,
        "SELECT count(*) FROM catalog_entries WHERE execution_policy NOT IN ('manual-only','blocked-until-scoped-authorization')",
    );
    let samples = scalar(conn, "SELECT count(*) FROM project_registry WHERE kind = 'synthetic'");
    let sources = scalar(conn, "SELECT count(*) FROM prompt_sources WHERE status = 'planned' AND read_only = 1");
    let frozen = scalar(conn, "SELECT count(*) FROM sqlite_master WHERE type = 'trigger' AND name LIKE '%_frozen_%'");
    // The stored capability fingerprint must still match what the entry actually is. A changed classification
    // (or any other in-place edit) that left the old fingerprint behind would let an old decision be reused.
    let fingerprints_ok = crate::catalog::list_catalog(conn)
        .map(|all| {
            all.iter().all(|e| crate::catalog::capability_fingerprint(&e.record, &e.kind, &e.execution_policy) == e.capability_fingerprint)
        })
        .unwrap_or(false);
    let ok = fingerprints_ok
        && seeded == Some(crate::catalog::SEED_ENTRY_COUNT as i64)
        && claims == Some(0)
        && bad_prefs == Some(0)
        && bad_kind == Some(0)
        && unreviewed_claims == Some(0)
        && strix_ok == Some(1)
        && free_run == Some(0)
        && samples == Some(3)
        && sources == Some(3)
        && frozen == Some(9);
    if ok {
        check(
            ID, TITLE, "passed",
            "13 kaynak kayıtlı; hiçbir araç kurulu veya doğrulanmış görünmüyor; kayıtlı kararlar yalnızca istek durumudur; Strix karantinada ve çalıştırma kapalı; eski Faz 1A tabloları donduruldu.",
            None,
        )
    } else {
        check(ID, TITLE, "failed", "Katalog, tercih veya durum kuralları beklenen değerlerde değil.", Some("INVARIANT_VIOLATION"))
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

fn check_discovery(conn: &Connection) -> CheckResult {
    const ID: &str = "discovery";
    const TITLE: &str = "Bilgisayar taraması (gözlemler)";
    let scans = scalar(conn, "SELECT count(*) FROM discovery_scans");
    let Some(scans) = scans else {
        return check(ID, TITLE, "failed", "Tarama kayıtları okunamadı.", Some("DISCOVERY_UNREADABLE"));
    };
    if scans == 0 {
        return check(ID, TITLE, "unknown", "Henüz bu bilgisayar taranmadı; hiçbir araç için gözlem yok.", None);
    }
    // Stored fields must already satisfy the privacy gate (closed grammars for location, version, version source and
    // project alias; free text without absolute paths, server names or anything the redactor would change), and no
    // observation may claim a test (the SQL constraint says so too; this catches a bypassed database).
    let unsafe_rows = (|| -> rusqlite::Result<i64> {
        use crate::discovery::privacy;
        let mut stmt = conn.prepare(
            "SELECT evidence, limitations_json, version, version_source, location, project_alias, functionality_tested, provenance_json, identity_state, identity_verified FROM discovery_observations",
        )?;
        let mut bad = 0;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, i64>(9)?,
            ))
        })? {
            let (evidence, limitations, version, source, location, alias, tested, ladder, identity_state, identity_verified) = row?;
            // The evidence ladder must parse as closed vocabulary and agree with the columns and the verified flag.
            let ladder_bad = (identity_verified == 1) != (identity_state == "validated")
                || (ladder != "{}"
                    && serde_json::from_str::<privacy::LadderCheck>(&ladder).map(|l| l.identity != identity_state).unwrap_or(true));
            let limitation_list: Vec<String> = serde_json::from_str(&limitations).unwrap_or_else(|_| vec!["\u{1}".to_string()]); // an unreadable list counts as unsafe
            if tested != 0
                || ladder_bad
                || !privacy::text_is_clean(&evidence)
                || limitation_list.iter().any(|l| !privacy::text_is_clean(l))
                || version.as_deref().map(|v| !privacy::is_documented_version(v)).unwrap_or(false)
                || source.as_deref().map(|s| !privacy::VERSION_SOURCES.contains(&s)).unwrap_or(false)
                || location.as_deref().map(|l| !privacy::is_safe_location(l)).unwrap_or(false)
                || alias.as_deref().map(|a| !privacy::is_safe_project_alias(a)).unwrap_or(false)
            {
                bad += 1;
            }
        }
        Ok(bad)
    })();
    match unsafe_rows {
        Ok(0) => {
            let newest: Option<String> = conn.query_row("SELECT max(finished_at) FROM discovery_scans", [], |r| r.get(0)).ok().flatten();
            let stale = newest
                .as_deref()
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .map(|t| (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_hours() >= crate::discovery::store::STALE_AFTER_HOURS)
                .unwrap_or(true);
            if stale {
                check(ID, TITLE, "unknown", "Son tarama eski; gözlemler güncel olmayabilir. Yeniden tarayın.", None)
            } else {
                check(
                    ID,
                    TITLE,
                    "passed",
                    "Gözlemler tutarlı ve gizli bilgi içermiyor. Bunlar bulunma/yapılandırma kanıtıdır; işlev doğrulaması değildir.",
                    None,
                )
            }
        }
        Ok(_) => check(
            ID,
            TITLE,
            "failed",
            "Kayıtlı gözlemlerden en az biri güvensiz içerik taşıyor veya test edilmiş gibi işaretli.",
            Some("DISCOVERY_EVIDENCE_UNSAFE"),
        ),
        Err(_) => check(ID, TITLE, "failed", "Gözlem kayıtları okunamadı.", Some("DISCOVERY_UNREADABLE")),
    }
}

const CAPABILITIES_JSON: &str = include_str!("../capabilities/default.json");
/// The only permissions the WebView may ever hold: one `allow-*` per typed app command (see build.rs).
/// Anything else, including broad plugin/core permissions, fails the check. Adding a command means
/// editing build.rs, this list and capabilities/default.json together; a unit test keeps them equal.
pub const ALLOWED_PERMISSIONS: &[&str] = &[
    "allow-get-app-info",
    "allow-get-dashboard",
    "allow-list-catalog",
    "allow-preview-github-url",
    "allow-add-catalog-entry",
    "allow-set-catalog-entry-enabled",
    "allow-remove-catalog-entry",
    "allow-restore-catalog-entry",
    "allow-explain-execution-block",
    "allow-list-projects",
    "allow-add-project-reference",
    "allow-remove-project-reference",
    "allow-set-project-preference",
    "allow-reset-project-preference",
    "allow-list-scenarios",
    "allow-evaluate-task",
    "allow-run-discovery-scan",
    "allow-list-inventory",
    "allow-preview-backup-cleanup",
    "allow-list-prompt-sources",
    "allow-run-health-checks",
    "allow-list-audit-events",
    "allow-verify-audit-chain",
    "allow-list-diagnostics",
    "allow-get-settings",
    "allow-set-setting",
    "allow-get-startup-state",
    "allow-set-startup-enabled",
    "allow-report-ui-error",
];

/// Top-level manifest keys that carry no extra authority. Keys such as `webviews`, `remote`,
/// `platforms` or `local` change who gets the permissions, so they are rejected.
const ALLOWED_MANIFEST_KEYS: &[&str] = &["$schema", "identifier", "description", "windows", "permissions"];

/// Evaluate a capability manifest against the explicit allowlist (deny by default).
pub fn capability_violations(manifest: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    let Some(object) = manifest.as_object() else {
        return vec!["manifest".to_string()];
    };
    for key in object.keys().filter(|k| !ALLOWED_MANIFEST_KEYS.contains(&k.as_str())) {
        problems.push(format!("unexpected key {key:?}"));
    }
    if object.get("windows") != Some(&serde_json::json!(["main"])) {
        problems.push("windows".to_string());
    }
    let Some(perms) = object.get("permissions").and_then(Value::as_array) else {
        problems.push("permissions".to_string());
        return problems;
    };
    let mut seen = std::collections::BTreeSet::new();
    for p in perms {
        match p.as_str() {
            Some(s) if ALLOWED_PERMISSIONS.contains(&s) => {
                if !seen.insert(s) {
                    problems.push(format!("duplicate {s:?}"));
                }
            }
            // Non-string entries (scoped permission objects) and unknown names are both rejected.
            other => problems.push(format!("unexpected permission {other:?}")),
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
        check_discovery(conn),
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
        audit::append(conn, &NewEvent::plain("health_check", c.status, c.id, c.error_code, "operational-health", "manager"))?;
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
        audit::append(&conn, &NewEvent::plain("health_check", "passed", "x", None, "operational-health", "manager")).unwrap();
        let report = run(&conn, &ctx());
        assert_eq!(status(&report, "database"), "passed");
        assert_eq!(status(&report, "catalog-invariants"), "passed");
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
        assert_eq!(status(&report, "catalog-invariants"), "failed");
        assert_eq!(report.overall, "failed");
    }

    #[test]
    fn broken_audit_chain_fails_overall() {
        let conn = seeded();
        for _ in 0..3 {
            audit::append(&conn, &NewEvent::plain("health_check", "passed", "x", None, "operational-health", "manager")).unwrap();
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

    fn manifest_with(perms: serde_json::Value) -> Value {
        json!({"windows": ["main"], "permissions": perms})
    }

    #[test]
    fn shipped_manifest_is_exactly_the_allowlist() {
        let manifest: Value = serde_json::from_str(CAPABILITIES_JSON).unwrap();
        assert!(capability_violations(&manifest).is_empty());
        let mut granted: Vec<&str> = manifest["permissions"].as_array().unwrap().iter().map(|p| p.as_str().unwrap()).collect();
        granted.sort();
        let mut allowed = ALLOWED_PERMISSIONS.to_vec();
        allowed.sort();
        assert_eq!(granted, allowed, "grant list and allowlist must be identical");
    }

    #[test]
    fn unexpected_permissions_are_rejected() {
        for bad in [
            // broad / plugin / core permissions
            "shell:allow-execute",
            "fs:default",
            "sql:allow-execute",
            "http:default",
            "opener:default",
            "core:default",
            "core:window:allow-close",
            "autostart:default",
            "autostart:allow-enable",
            "single-instance:default",
            "dialog:default",
            "updater:default",
            "process:allow-exit",
            "os:default",
            // look-alikes that the old prefix/substring rule could have admitted
            "allow-unknown-command",
            "allow-get-app-info-extra",
            "allow-list-catalo",
            "ALLOW-LIST-CATALOG",
            " allow-list-catalog",
            "allow-list-catalog ",
            "allow-",
            "allow-*",
            "*",
            "",
            "allow-shell-open",
        ] {
            assert!(!capability_violations(&manifest_with(json!([bad]))).is_empty(), "accepted {bad:?}");
        }
    }

    #[test]
    fn non_string_scoped_and_duplicate_permissions_are_rejected() {
        let scoped = json!([{"identifier": "allow-list-catalog", "allow": [{"path": "**"}]}]);
        assert!(!capability_violations(&manifest_with(scoped)).is_empty());
        assert!(!capability_violations(&manifest_with(json!([null, 7, ["allow-list-catalog"]]))).is_empty());
        assert!(!capability_violations(&manifest_with(json!(["allow-list-catalog", "allow-list-catalog"]))).is_empty());
        assert!(capability_violations(&manifest_with(json!(["allow-list-catalog"]))).is_empty());
    }

    #[test]
    fn manifest_scope_and_extra_keys_are_rejected() {
        let ok = json!({"identifier": "default", "windows": ["main"], "permissions": ["allow-list-catalog"]});
        assert!(capability_violations(&ok).is_empty());
        for (key, value) in [
            ("webviews", json!(["*"])),
            ("remote", json!({"urls": ["https://*"]})),
            ("platforms", json!(["windows"])),
            ("local", json!(true)),
        ] {
            let mut m = ok.clone();
            m[key] = value;
            assert!(!capability_violations(&m).is_empty(), "accepted extra key {key}");
        }
        for windows in [json!(["*"]), json!(["main", "other"]), json!([]), json!("main")] {
            let mut m = ok.clone();
            m["windows"] = windows;
            assert!(!capability_violations(&m).is_empty());
        }
        assert!(!capability_violations(&json!({"windows": ["main"]})).is_empty(), "missing permissions");
        assert!(!capability_violations(&json!(["allow-list-catalog"])).is_empty(), "non-object manifest");
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
