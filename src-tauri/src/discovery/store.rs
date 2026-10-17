//! Persistence and presentation of observations in the application's own local database.
//! Nothing here touches the scanned targets.

use super::{
    detect::{Authorization, Roots},
    types::Observation,
};
use crate::{
    audit::{self, now_utc, NewEvent},
    diagnostics,
    error::AppResult,
};
use rusqlite::Connection;
use serde::Serialize;

pub const KEEP_SCANS: i64 = 20;
pub const STALE_AFTER_HOURS: i64 = 24;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub scan_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub platform_target: String,
    pub authorized_path: bool,
    pub authorized_user_config: bool,
    pub authorized_project: bool,
    pub status: String,
    pub observation_count: i64,
}

pub fn save_scan(
    conn: &Connection,
    roots: &Roots,
    auth: &Authorization,
    observations: &[Observation],
    started_at: &str,
) -> AppResult<ScanSummary> {
    let scan_id = uuid::Uuid::new_v4().to_string();
    let finished = now_utc();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO discovery_scans (scan_id, started_at, finished_at, platform_target, authorized_path, authorized_user_config, authorized_project, status, observation_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'completed', ?8)",
        rusqlite::params![
            scan_id,
            started_at,
            finished,
            roots.platform,
            auth.path_executables as i64,
            auth.user_config as i64,
            auth.project.is_some() as i64,
            observations.len() as i64
        ],
    )?;
    for o in observations {
        tx.execute(
            "INSERT INTO discovery_observations
             (scan_id, subject, platform_target, agent_surface, method, presence, configuration, version, version_source, scope, project_alias,
              confidence, evidence, location, limitations_json, identity_verified, functionality_tested, observed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 0, ?17)",
            rusqlite::params![
                scan_id,
                o.subject,
                o.platform_target,
                o.agent_surface,
                o.method.as_str(),
                o.presence.as_str(),
                o.configuration.as_str(),
                o.version,
                o.version_source,
                o.scope.as_str(),
                o.project_alias,
                o.confidence.as_str(),
                o.evidence,
                o.location,
                serde_json::to_string(&o.limitations)?,
                o.identity_verified as i64,
                finished
            ],
        )?;
    }
    // Keep only the most recent scans; observations go with them (ON DELETE CASCADE).
    tx.execute(
        "DELETE FROM discovery_scans WHERE scan_id NOT IN (SELECT scan_id FROM discovery_scans ORDER BY finished_at DESC, rowid DESC LIMIT ?1)",
        [KEEP_SCANS],
    )?;
    // Counts only: no path, no tool list, no content.
    diagnostics::log(&tx, "info", "discovery", "DISCOVERY_SCAN_DONE", &format!("{} gözlem kaydedildi.", observations.len()))?;
    audit::append(&tx, &NewEvent::plain("inspect", "passed", "discovery-scan", None, "not-applicable", "user"))?;
    tx.commit()?;
    Ok(ScanSummary {
        scan_id,
        started_at: started_at.to_string(),
        finished_at: finished,
        platform_target: roots.platform.clone(),
        authorized_path: auth.path_executables,
        authorized_user_config: auth.user_config,
        authorized_project: auth.project.is_some(),
        status: "completed".to_string(),
        observation_count: observations.len() as i64,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredObservation {
    pub id: i64,
    pub scan_id: String,
    pub subject: String,
    pub platform_target: String,
    pub agent_surface: String,
    pub method: String,
    pub presence: String,
    pub configuration: String,
    pub version: Option<String>,
    pub version_source: Option<String>,
    pub scope: String,
    pub project_alias: Option<String>,
    pub confidence: String,
    pub evidence: String,
    pub location: Option<String>,
    pub limitations: Vec<String>,
    pub identity_verified: bool,
    pub functionality_tested: bool,
    pub observed_at: String,
    pub stale: bool,
    pub age_hours: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestVsObservation {
    pub entry_id: String,
    pub entry_name: String,
    pub accepted_projects: i64,
    /// present | absent | unknown | not_scanned
    pub observed: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub last_scan: Option<ScanSummary>,
    pub observations: Vec<StoredObservation>,
    pub requests: Vec<RequestVsObservation>,
    pub not_checked: Vec<String>,
    pub stale_after_hours: i64,
}

fn age_hours(observed_at: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(observed_at)
        .map(|t| (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_hours())
        .unwrap_or(i64::MAX)
}

fn last_scan(conn: &Connection) -> AppResult<Option<ScanSummary>> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .query_row(
            "SELECT scan_id, started_at, finished_at, platform_target, authorized_path, authorized_user_config, authorized_project, status, observation_count
             FROM discovery_scans ORDER BY finished_at DESC, rowid DESC LIMIT 1",
            [],
            |r| {
                Ok(ScanSummary {
                    scan_id: r.get(0)?,
                    started_at: r.get(1)?,
                    finished_at: r.get(2)?,
                    platform_target: r.get(3)?,
                    authorized_path: r.get::<_, i64>(4)? == 1,
                    authorized_user_config: r.get::<_, i64>(5)? == 1,
                    authorized_project: r.get::<_, i64>(6)? == 1,
                    status: r.get(7)?,
                    observation_count: r.get(8)?,
                })
            },
        )
        .optional()?)
}

/// The newest observation for each (subject, platform, surface, method, scope, project) key.
pub fn latest_observations(conn: &Connection) -> AppResult<Vec<StoredObservation>> {
    let mut stmt = conn.prepare(
        "SELECT o.id, o.scan_id, o.subject, o.platform_target, o.agent_surface, o.method, o.presence, o.configuration, o.version, o.version_source,
                o.scope, o.project_alias, o.confidence, o.evidence, o.location, o.limitations_json, o.identity_verified, o.functionality_tested, o.observed_at
         FROM discovery_observations o
         WHERE o.id IN (SELECT max(id) FROM discovery_observations GROUP BY subject, platform_target, agent_surface, method, scope, COALESCE(project_alias, ''))
         ORDER BY o.subject, o.scope, o.method",
    )?;
    let rows = stmt.query_map([], |r| {
        let limitations: String = r.get(15)?;
        let observed_at: String = r.get(18)?;
        let age = age_hours(&observed_at);
        Ok(StoredObservation {
            id: r.get(0)?,
            scan_id: r.get(1)?,
            subject: r.get(2)?,
            platform_target: r.get(3)?,
            agent_surface: r.get(4)?,
            method: r.get(5)?,
            presence: r.get(6)?,
            configuration: r.get(7)?,
            version: r.get(8)?,
            version_source: r.get(9)?,
            scope: r.get(10)?,
            project_alias: r.get(11)?,
            confidence: r.get(12)?,
            evidence: r.get(13)?,
            location: r.get(14)?,
            limitations: serde_json::from_str(&limitations).unwrap_or_default(),
            identity_verified: r.get::<_, i64>(16)? == 1,
            functionality_tested: r.get::<_, i64>(17)? == 1,
            observed_at,
            stale: age >= STALE_AFTER_HOURS,
            age_hours: age.min(i64::from(i32::MAX)),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn requests(conn: &Connection, observations: &[StoredObservation]) -> AppResult<Vec<RequestVsObservation>> {
    let mut stmt = conn.prepare(
        "SELECT e.entry_id, e.display_name, count(DISTINCT p.project_id)
         FROM project_preferences p
         JOIN catalog_entries e ON e.entry_id = p.entry_id AND e.removed_at IS NULL
         JOIN project_registry r ON r.project_id = p.project_id AND r.removed_at IS NULL
         WHERE p.decision = 'accepted' GROUP BY e.entry_id, e.display_name ORDER BY e.display_name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (entry_id, entry_name, accepted_projects) = row?;
        let mine: Vec<&StoredObservation> = observations.iter().filter(|o| o.subject == entry_id).collect();
        let observed = if mine.is_empty() {
            "not_scanned"
        } else if mine.iter().any(|o| o.presence == "present") {
            "present"
        } else if mine.iter().any(|o| o.presence == "absent") {
            "absent"
        } else {
            "unknown"
        };
        let note = match observed {
            "present" => "Bu bilgisayarda bulundu (bulunması, çalıştığı veya projede etkin olduğu anlamına gelmez).",
            "absent" => "Etkinleştirmek istediniz, ancak bu bilgisayarda bulunamadı.",
            "unknown" => "Etkinleştirmek istediniz; bu bilgisayarda olup olmadığı belirlenemedi.",
            _ => "Henüz bu araç için bir tarama gözlemi yok.",
        };
        out.push(RequestVsObservation { entry_id, entry_name, accepted_projects, observed: observed.to_string(), note: note.to_string() });
    }
    Ok(out)
}

pub fn inventory(conn: &Connection) -> AppResult<Inventory> {
    let observations = latest_observations(conn)?;
    let last = last_scan(conn)?;
    let mut not_checked: Vec<String> = Vec::new();
    match &last {
        None => not_checked.push("Henüz hiç tarama yapılmadı; aşağıdakilerin hiçbirine bakılmadı.".to_string()),
        Some(s) => {
            if !s.authorized_path {
                not_checked.push(
                    "PATH klasörlerinde ve Claude Code'un bilinen kurulum konumlarında program adları aranmadı (yetki verilmedi)."
                        .to_string(),
                );
            }
            if !s.authorized_user_config {
                not_checked
                    .push("Claude Code kullanıcı ayarları, eklenti kayıtları ve MCP tanımları okunmadı (yetki verilmedi).".to_string());
            }
            if !s.authorized_project {
                not_checked.push(
                    "Hiçbir proje klasörüne bakılmadı: projeye özel ayarlar ve .mcp.json için bir klasör seçmeniz gerekir.".to_string(),
                );
            }
        }
    }
    not_checked.extend(
        [
            "Çalışan Claude Code oturumlarında hangi eklentilerin yüklü olduğu.",
            "Programların sürümleri: kimliği doğrulanmamış hiçbir program çalıştırılmadı.",
            "Araçların gerçekten çalışıp çalışmadığı (işlev doğrulaması bu fazda yok).",
            "Antigravity masaüstü IDE'si: belgelenmiş bir algılama yöntemi doğrulanmadı.",
            "Claude Code'un VS Code uzantısındaki özel kopyası.",
            "Kaynak kodunuz ve proje dosyalarının içeriği: hiç okunmaz.",
        ]
        .map(str::to_string),
    );
    let requests = requests(conn, &observations)?;
    Ok(Inventory { last_scan: last, observations, requests, not_checked, stale_after_hours: STALE_AFTER_HOURS })
}
