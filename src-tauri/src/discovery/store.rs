//! Persistence and presentation of observations in the application's own local database.
//! Nothing here touches the scanned targets.
//!
//! History is preserved (every scan keeps its own rows, up to `KEEP_SCANS`), but *what is shown as true now* is
//! derived at read time: an observation is **current** only when it comes from the latest scan, that scan is not
//! older than `STALE_AFTER_HOURS`, and it was made for this computer's platform. Everything else is shown as an
//! earlier result with the reason it is not current, and is never used to justify an action.

use super::declarations::{self, DeclarationDetector, DeclarationReading};
use super::{
    detect::{Authorization, Roots},
    privacy,
    types::{IdentityState, Method, Observation, Provenance, Scope, VersionState},
};
use crate::{
    audit::{self, now_utc, NewEvent},
    diagnostics,
    error::AppResult,
};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::collections::BTreeSet;

pub const KEEP_SCANS: i64 = 20;
pub const STALE_AFTER_HOURS: i64 = 24;
/// Rows written by the Phase 2A baseline (before the privacy gate) are version 1; rows written since are version 2.
pub const RECORD_VERSION: i64 = 2;

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
    pub authorized_fingerprint: bool,
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
        "INSERT INTO discovery_scans (scan_id, started_at, finished_at, platform_target, authorized_path, authorized_user_config, authorized_project, status, observation_count, authorized_fingerprint)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'completed', ?8, ?9)",
        rusqlite::params![
            scan_id,
            started_at,
            finished,
            roots.platform,
            auth.path_executables as i64,
            auth.user_config as i64,
            auth.project.is_some() as i64,
            observations.len() as i64,
            auth.fingerprint_artifacts as i64
        ],
    )?;
    // The privacy gate runs again right before persistence, so a caller that skipped `Observation::finalize`
    // still cannot write a path, an account name or an undocumented version string.
    let mut scrubbed_fields = 0usize;
    for original in observations {
        let mut o = original.clone();
        scrubbed_fields += privacy::scrub(&mut o);
        tx.execute(
            "INSERT INTO discovery_observations
             (scan_id, subject, platform_target, agent_surface, method, presence, configuration, version, version_source, scope, project_alias,
              confidence, evidence, location, limitations_json, identity_verified, functionality_tested, observed_at, record_version,
              identity_state, version_state, provenance_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 0, ?17, ?18, ?19, ?20, ?21)",
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
                finished,
                RECORD_VERSION,
                o.provenance.identity.as_str(),
                o.provenance.version.as_str(),
                serde_json::to_string(&o.provenance)?
            ],
        )?;
    }
    // Keep only the most recent scans; observations go with them (ON DELETE CASCADE).
    tx.execute(
        "DELETE FROM discovery_scans WHERE scan_id NOT IN (SELECT scan_id FROM discovery_scans ORDER BY finished_at DESC, rowid DESC LIMIT ?1)",
        [KEEP_SCANS],
    )?;
    // Counts only: no path, no tool list, no content.
    let note = if scrubbed_fields > 0 {
        format!("{} gözlem kaydedildi; {} alan gizlilik kuralı gereği gizlendi.", observations.len(), scrubbed_fields)
    } else {
        format!("{} gözlem kaydedildi.", observations.len())
    };
    diagnostics::log(&tx, "info", "discovery", "DISCOVERY_SCAN_DONE", &note)?;
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
        authorized_fingerprint: auth.fingerprint_artifacts,
        status: "completed".to_string(),
        observation_count: observations.len() as i64,
    })
}

/// Migration 5: rows written before the privacy gate (record_version 1) are scrubbed in place. Everything else about
/// them (presence, configuration, scan membership, timestamps) is preserved; only fields that fail the closed
/// grammars are replaced, and the rows stay marked as legacy so the UI can say so.
pub fn scrub_legacy_observations(tx: &Connection) -> AppResult<()> {
    type Row = (i64, Option<String>, String, Option<String>, Option<String>, Option<String>, String);
    let rows: Vec<Row> = {
        let mut stmt = tx.prepare(
            "SELECT id, location, evidence, version, version_source, project_alias, limitations_json FROM discovery_observations",
        )?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?;
        mapped.collect::<Result<Vec<_>, _>>()?
    };
    let mut changed = 0usize;
    for (id, location, evidence, version, version_source, alias, limitations_json) in rows {
        let new_location =
            location.as_deref().map(|l| if privacy::is_safe_location(l) { l.to_string() } else { privacy::LOCATION_REMOVED.to_string() });
        let mut new_evidence = privacy::strip_server_names(&evidence);
        if !privacy::text_is_clean(&new_evidence) {
            new_evidence = privacy::scrub_text(&new_evidence);
        }
        let version_ok = version.as_deref().map(privacy::is_documented_version).unwrap_or(true)
            && version_source.as_deref().map(|s| privacy::VERSION_SOURCES.contains(&s)).unwrap_or(true)
            && (version.is_some() || version_source.is_none());
        let (new_version, new_source) = if version_ok { (version.clone(), version_source.clone()) } else { (None, None) };
        let new_alias = alias.as_deref().filter(|a| privacy::is_safe_project_alias(a)).map(str::to_string);
        let limitations: Vec<String> = serde_json::from_str(&limitations_json).unwrap_or_default();
        let new_limitations: Vec<String> = limitations
            .iter()
            .map(|l| {
                let stripped = privacy::strip_server_names(l);
                if privacy::text_is_clean(&stripped) {
                    stripped
                } else {
                    privacy::scrub_text(&stripped)
                }
            })
            .collect();
        let new_limitations_json = serde_json::to_string(&new_limitations)?;
        if new_location != location
            || new_evidence != evidence
            || new_version != version
            || new_source != version_source
            || new_alias != alias
            || new_limitations_json != limitations_json
        {
            tx.execute(
                "UPDATE discovery_observations SET location = ?2, evidence = ?3, version = ?4, version_source = ?5, project_alias = ?6, limitations_json = ?7 WHERE id = ?1",
                rusqlite::params![id, new_location, new_evidence, new_version, new_source, new_alias, new_limitations_json],
            )?;
            changed += 1;
        }
    }
    // Counts only, and only when something was actually scrubbed (a database without discovery rows is untouched).
    if changed > 0 {
        diagnostics::log(
            tx,
            "info",
            "discovery",
            "DISCOVERY_LEGACY_SCRUBBED",
            &format!("{changed} eski gözlem kaydındaki konum/ad bilgisi gizlilik kuralı gereği temizlendi."),
        )?;
    }
    Ok(())
}

/// Migration 7: lowers every high-trust value an earlier build stored. Observation facts, scans and ids are untouched.
/// * A Phase 2A row that was `identity_verified = 1` (no provenance ladder) becomes `not_assessed` + `reclassified`.
/// * A Phase 2B row recorded as `validated` becomes `evidence_available` + `reclassified` (whether the installed
///   files were present cannot be known from the stored data, so the corroborated state is not guessed).
/// * `versionTrust: "trusted"` is renamed `installer_recorded` (a local record's declaration, not a verified fact).
pub fn reclassify_legacy_trust(tx: &Connection) -> AppResult<()> {
    let rows: Vec<(i64, i64, String, String)> = {
        let mut stmt = tx.prepare(
            "SELECT id, identity_verified, identity_state, provenance_json FROM discovery_observations
             WHERE identity_verified = 1 OR identity_state = 'validated' OR provenance_json LIKE '%\"trusted\"%'",
        )?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        mapped.collect::<Result<Vec<_>, _>>()?
    };
    let mut lowered = 0usize;
    for (id, verified, state, json) in rows {
        let mut doc: serde_json::Value = serde_json::from_str(&json).unwrap_or(serde_json::json!({}));
        let assessed = doc.get("assessed").and_then(serde_json::Value::as_bool).unwrap_or(false);
        let mut new_state = state.clone();
        if state == "validated" || verified == 1 {
            lowered += 1;
            if assessed {
                new_state = "evidence_available".to_string();
                doc["identity"] = serde_json::json!("evidence_available");
            } else {
                new_state = "not_assessed".to_string();
            }
            doc["reclassified"] = serde_json::json!(true);
        }
        if doc.get("versionTrust").and_then(serde_json::Value::as_str) == Some("trusted") {
            doc["versionTrust"] = serde_json::json!("installer_recorded");
        }
        let new_json = if doc.as_object().map(|o| o.is_empty()).unwrap_or(true) { "{}".to_string() } else { serde_json::to_string(&doc)? };
        tx.execute(
            "UPDATE discovery_observations SET identity_state = ?2, identity_verified = 0, provenance_json = ?3 WHERE id = ?1",
            rusqlite::params![id, new_state, new_json],
        )?;
    }
    if lowered > 0 {
        diagnostics::log(
            tx,
            "info",
            "discovery",
            "DISCOVERY_TRUST_LOWERED",
            &format!("{lowered} gözlemin kimlik güven düzeyi, doğrulanmış kanıta dayanmadığı için düşürüldü."),
        )?;
    }
    Ok(())
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
    /// Written before the privacy gate existed (scrubbed by migration 5).
    pub legacy: bool,
    // ---- provenance: which scan produced this and what that scan was allowed to look at -------------------
    pub scan_finished_at: String,
    pub scan_authorized_path: bool,
    pub scan_authorized_user_config: bool,
    pub scan_authorized_project: bool,
    pub scan_authorized_fingerprint: bool,
    /// `latest_scan` | `earlier_scan`
    pub provenance: String,
    /// Did the LATEST scan look at the area this observation belongs to (path / user files / this project)?
    pub latest_scan_checked: bool,
    /// From the latest scan, fresh, and for this computer's platform. Only a current observation describes today.
    pub current: bool,
    pub not_current_reason: Option<String>,
    /// The Phase 2B evidence ladder. `assessed = false` for rows written before it existed.
    pub ladder: Provenance,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRef {
    pub scan_id: String,
    pub finished_at: String,
    pub authorized_path: bool,
    pub authorized_user_config: bool,
    pub authorized_project: bool,
    pub authorized_fingerprint: bool,
}

/// How the selected observation relates to the project the preference belongs to. Taken from the observation's own `project_alias` only
/// (not from its `scope` column, and not from the other candidates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRelation {
    /// The observation carries this preference's project alias.
    ProjectSpecific,
    /// The observation carries no project alias: a computer-wide lookup that applies to every project.
    ComputerWide,
}

/// How the selected observation relates to the agent surface the preference asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceRelation {
    /// The observation's agent surface is the requested one.
    ExactSurface,
    /// The observation is a computer-level program lookup (`local-cli`) standing in for a different requested surface.
    LocalCliFallback,
}

/// Where the evidence behind a status came from, described from the ONE observation `classify_request` selected (never from the candidate set).
/// A descriptor of the evidence, not a verdict about the tool: it never says installed, working, verified, active or supported.
/// Deliberately small: no path, location, evidence text, fingerprint or version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceOrigin {
    pub project_relation: ProjectRelation,
    pub surface_relation: SurfaceRelation,
    /// The selected observation's `observed_at`, only when it is a valid RFC 3339 timestamp; otherwise None (never repaired or guessed).
    pub observed_at: Option<String>,
    /// Whole hours between `observed_at` and the moment the inventory was read, only when that timestamp is valid and not in the future.
    pub age_hours: Option<i64>,
    /// PRODUCT-07 H (additive): HOW the selected observation was produced - a closed whitelist of detection techniques, or None for a pseudo-method
    /// (`no-detector`, `not-documented`, `unsupported-platform`) and for any unknown/legacy string. It describes the PRESENCE winner's technique; it is not the
    /// declaration detector's method and it proves neither identity nor execution.
    pub method: Option<OriginMethod>,
}

/// The detection techniques an origin may name. Serialized with the same kebab-case strings as `types::Method`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OriginMethod {
    PathExecutable,
    NativeInstallLocation,
    LegacyLocalInstall,
    ClaudeSettingsFile,
    ClaudePluginDeclaration,
    ClaudeMcpConfig,
}

impl OriginMethod {
    /// Closed whitelist: anything else (including the pseudo-methods) is None - nothing is guessed.
    pub fn from_row(method: &str) -> Option<OriginMethod> {
        Some(match method {
            "path-executable" => OriginMethod::PathExecutable,
            "native-install-location" => OriginMethod::NativeInstallLocation,
            "legacy-local-install" => OriginMethod::LegacyLocalInstall,
            "claude-settings-file" => OriginMethod::ClaudeSettingsFile,
            "claude-plugin-declaration" => OriginMethod::ClaudePluginDeclaration,
            "claude-mcp-config" => OriginMethod::ClaudeMcpConfig,
            _ => return None,
        })
    }
}

/// What the user asked for (one accepted preference) next to what the scans saw, never merged.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestVsObservation {
    pub entry_id: String,
    pub entry_name: String,
    pub project_alias: String,
    pub project_name: String,
    pub agent_surface: String,
    pub platform_target: String,
    /// present_current | absent_current | unknown_current | previously_present | previously_absent | previously_unknown
    /// | platform_mismatch | not_scanned
    pub status: String,
    pub note: String,
    /// The scan the status is based on, with the permissions it had. None for not_scanned.
    pub basis: Option<ScanRef>,
    /// Where the selected observation came from (project-specific or computer-wide; exact surface or `local-cli` fallback). None exactly when
    /// there is no selected observation (`not_scanned`, `platform_mismatch`): then no scope is guessed.
    pub origin: Option<EvidenceOrigin>,
    /// PRODUCT-07 H (additive): whether anything reads MCP DEFINITIONS for this entry on this surface. Never "supported", never "not configured".
    pub declaration_detector: DeclarationDetector,
    /// PRODUCT-07 H (additive): 0..=3 scoped MCP-configuration readings (project, local, global), shown BESIDE the presence status above and never merged with it.
    /// An empty list does NOT mean "not configured"; `declaration_detector.coverage` says why a scope has no reading.
    pub declarations: Vec<DeclarationReading>,
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
    age_hours_at(observed_at, chrono::Utc::now())
}

/// Whole hours between `observed_at` and `now`; `i64::MAX` when the stamp is not RFC 3339. The ordering is decided BEFORE the hours are truncated:
/// `num_hours()` rounds toward zero, so a stamp 30 minutes in the future would otherwise read as "0 hours" and be indistinguishable from a fresh one.
/// An instant after `now` is therefore always negative (at most -1), so `origin_of` and every `age >= …` test treat it as "no age".
pub(crate) fn age_hours_at(observed_at: &str, now: chrono::DateTime<chrono::Utc>) -> i64 {
    chrono::DateTime::parse_from_rfc3339(observed_at)
        .map(|t| {
            let elapsed = now - t.with_timezone(&chrono::Utc);
            if elapsed < chrono::Duration::zero() {
                elapsed.num_hours().min(-1)
            } else {
                elapsed.num_hours()
            }
        })
        .unwrap_or(i64::MAX)
}

fn last_scan(conn: &Connection) -> AppResult<Option<ScanSummary>> {
    Ok(conn
        .query_row(
            "SELECT scan_id, started_at, finished_at, platform_target, authorized_path, authorized_user_config, authorized_project, status, observation_count, authorized_fingerprint
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
                    authorized_fingerprint: r.get::<_, i64>(9)? == 1,
                })
            },
        )
        .optional()?)
}

/// Parses the stored ladder and applies today's trust rules to it. A higher state written by an earlier build
/// (`validated` without authenticated evidence bound to the installed artifact) is lowered and flagged
/// `reclassified`, never silently kept. Unreadable or pre-2B (`{}`) content is "not assessed".
fn stored_provenance(json: &str, identity: &str, version: &str) -> Provenance {
    let mut p = serde_json::from_str::<Provenance>(json).unwrap_or_default();
    if !p.assessed {
        let was = p.reclassified;
        p = Provenance::default();
        p.reclassified = was || identity == "validated";
        p.version = if version == "known" { VersionState::Known } else { VersionState::Unknown };
        return p;
    }
    if p.identity == IdentityState::Validated && p.artifact_auth == super::types::ArtifactAuth::None {
        p.identity = if p.install_record { IdentityState::EvidenceAvailable } else { IdentityState::Unknown };
        p.reclassified = true;
    }
    if p.version_trust == super::types::VersionTrust::Validated && p.artifact_auth == super::types::ArtifactAuth::None {
        p.version_trust = super::types::VersionTrust::InstallerRecorded;
    }
    p
}

/// The privacy gate on the way out: whatever a (legacy or tampered) row holds, the UI and headless output only ever
/// receive text that matches the closed grammars.
fn sanitize_on_read(o: &mut StoredObservation) {
    if o.location.as_deref().map(|l| !privacy::is_safe_location(l)).unwrap_or(false) {
        o.location = Some(privacy::LOCATION_REMOVED.to_string());
    }
    if o.version.as_deref().map(|v| !privacy::is_documented_version(v)).unwrap_or(false)
        || o.version_source.as_deref().map(|s| !privacy::VERSION_SOURCES.contains(&s)).unwrap_or(false)
        || (o.version.is_none() && o.version_source.is_some())
    {
        o.version = None;
        o.version_source = None;
    }
    if o.project_alias.as_deref().map(|a| !privacy::is_safe_project_alias(a)).unwrap_or(false) {
        o.project_alias = None;
    }
    let clean = |t: &str| {
        let stripped = privacy::strip_server_names(t);
        if privacy::text_is_clean(&stripped) {
            stripped
        } else {
            privacy::scrub_text(&stripped)
        }
    };
    o.evidence = clean(&o.evidence);
    o.limitations = o.limitations.iter().map(|l| clean(l)).collect();
    o.functionality_tested = false;
}

/// Did the latest scan look at the area this observation belongs to? Derived from the method and scope, the
/// permissions the scan had, and — for project-level data — whether that very project appears in the latest scan.
fn latest_scan_checked(o: &StoredObservation, last: &ScanSummary, latest_aliases: &BTreeSet<String>) -> bool {
    let method = [
        Method::PathExecutable,
        Method::NativeInstallLocation,
        Method::LegacyLocalInstall,
        Method::ClaudeSettingsFile,
        Method::ClaudePluginDeclaration,
        Method::ClaudeMcpConfig,
    ]
    .into_iter()
    .find(|m| m.as_str() == o.method);
    let Some(method) = method else { return true }; // no-detector / not-documented / unsupported-platform are emitted by every scan
    match method {
        Method::PathExecutable | Method::NativeInstallLocation | Method::LegacyLocalInstall => last.authorized_path,
        _ => {
            let project_seen = o.project_alias.as_deref().map(|a| latest_aliases.contains(a)).unwrap_or(false);
            match o.scope.as_str() {
                s if s == Scope::Project.as_str() => last.authorized_project && project_seen,
                s if s == Scope::Local.as_str() => {
                    last.authorized_project && project_seen && (method != Method::ClaudeMcpConfig || last.authorized_user_config)
                }
                _ => last.authorized_user_config,
            }
        }
    }
}

fn reason_not_current(o: &StoredObservation, last: &ScanSummary, in_latest: bool, checked: bool) -> Option<String> {
    if o.platform_target != last.platform_target {
        return Some(format!(
            "Bu gözlem başka bir işletim sistemi/mimari için ({}); bu bilgisayar ({}) için geçerli değil.",
            o.platform_target, last.platform_target
        ));
    }
    if !in_latest {
        return Some(if checked {
            "Son taramada bu gözlem üretilmedi; bu, eski bir taramanın sonucudur.".to_string()
        } else if o.project_alias.is_some() {
            "Son taramada bu projenin klasörüne bakılmadı (yetki verilmedi veya başka proje seçildi); bu, eski bir taramanın sonucudur."
                .to_string()
        } else {
            "Son taramada bu bölüm denetlenmedi (yetki verilmedi); bu, eski bir taramanın sonucudur.".to_string()
        });
    }
    if o.stale {
        return Some(format!("Son tarama {STALE_AFTER_HOURS} saatten eski; yeniden tarayın."));
    }
    // A row that claims to come from the latest scan for an area that scan was not allowed to look at is inconsistent
    // (corrupted, migrated or injected). A stored "current" can never stand in for provenance: it is not current.
    if !checked {
        return Some(
            "Bu kayıt son taramadan görünüyor, ancak o tarama bu alana bakma iznine sahip değildi; tutarsız kayıt güvenilmez sayıldı."
                .to_string(),
        );
    }
    None
}

/// The newest observation for each (subject, platform, surface, method, scope, project) key — history stays in the
/// database — with provenance and currentness derived against the latest scan.
pub fn latest_observations(conn: &Connection) -> AppResult<Vec<StoredObservation>> {
    let references = super::reference::ReferenceSet::production(conn)?;
    latest_observations_with(conn, &references)
}

/// Same, judging stored artifact verdicts against an explicit reference set (production passes the real one; tests pass a
/// synthetic authenticated set to prove the verified path end to end).
pub fn latest_observations_with(conn: &Connection, references: &super::reference::ReferenceSet) -> AppResult<Vec<StoredObservation>> {
    let last = last_scan(conn)?;
    let aliases: BTreeSet<String> = match &last {
        Some(l) => {
            let mut stmt =
                conn.prepare("SELECT DISTINCT project_alias FROM discovery_observations WHERE scan_id = ?1 AND project_alias IS NOT NULL")?;
            let rows = stmt.query_map([&l.scan_id], |r| r.get::<_, String>(0))?;
            rows.collect::<Result<BTreeSet<_>, _>>()?
        }
        None => BTreeSet::new(),
    };
    let mut stmt = conn.prepare(
        "SELECT o.id, o.scan_id, o.subject, o.platform_target, o.agent_surface, o.method, o.presence, o.configuration, o.version, o.version_source,
                o.scope, o.project_alias, o.confidence, o.evidence, o.location, o.limitations_json, o.identity_verified, o.functionality_tested, o.observed_at,
                o.record_version, s.finished_at, s.authorized_path, s.authorized_user_config, s.authorized_project,
                o.identity_state, o.version_state, o.provenance_json, s.authorized_fingerprint
         FROM discovery_observations o JOIN discovery_scans s ON s.scan_id = o.scan_id
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
            identity_verified: false, // derived from the ladder below, never trusted from the column
            functionality_tested: r.get::<_, i64>(17)? == 1,
            observed_at,
            stale: age >= STALE_AFTER_HOURS,
            age_hours: age.min(i64::from(i32::MAX)),
            legacy: r.get::<_, i64>(19)? < RECORD_VERSION,
            scan_finished_at: r.get(20)?,
            scan_authorized_path: r.get::<_, i64>(21)? == 1,
            scan_authorized_user_config: r.get::<_, i64>(22)? == 1,
            scan_authorized_project: r.get::<_, i64>(23)? == 1,
            scan_authorized_fingerprint: r.get::<_, i64>(27)? == 1,
            provenance: String::new(),
            latest_scan_checked: false,
            current: false,
            not_current_reason: None,
            ladder: stored_provenance(&r.get::<_, String>(26)?, &r.get::<_, String>(24)?, &r.get::<_, String>(25)?),
        })
    })?;
    let mut out = rows.collect::<Result<Vec<_>, _>>()?;
    let epoch = references.epoch();
    for o in &mut out {
        sanitize_on_read(o);
        match &last {
            Some(l) => {
                let in_latest = o.scan_id == l.scan_id;
                let checked = latest_scan_checked(o, l, &aliases);
                o.provenance = if in_latest { "latest_scan" } else { "earlier_scan" }.to_string();
                o.latest_scan_checked = checked;
                o.not_current_reason = reason_not_current(o, l, in_latest, checked);
                o.current = o.not_current_reason.is_none();
            }
            None => {
                o.provenance = "earlier_scan".to_string();
                o.not_current_reason = Some("Henüz geçerli bir tarama yok.".to_string());
            }
        }
    }
    // Artifact verdicts are re-derived from the stored digest against TODAY's code-embedded references, so a hand-edited row
    // can never claim a higher trust than the evidence allows, and a verdict made under another policy or reference set is void.
    for o in &mut out {
        apply_artifact_rules(o, references, &epoch);
        o.identity_verified = o.ladder.identity == IdentityState::Validated && o.current && !o.ladder.artifact.invalidated;
    }
    Ok(out)
}

fn apply_artifact_rules(o: &mut StoredObservation, references: &super::reference::ReferenceSet, epoch: &str) {
    use super::reference::{evaluate, MatchResult, ReferenceState, POLICY_VERSION};
    use super::types::{ArtifactAuth, FingerprintState, VersionTrust};
    let a = &mut o.ladder.artifact;
    let judged =
        a.fingerprint == FingerprintState::Computed || a.reference != ReferenceState::None || a.match_result != MatchResult::NotApplicable;
    if judged && (a.policy_version != POLICY_VERSION || a.reference_epoch.as_deref() != Some(epoch)) {
        a.invalidated = true;
        a.match_result = MatchResult::NotApplicable;
        a.reference = ReferenceState::None;
        a.reference_kind = None;
    } else if let (FingerprintState::Computed, Some(sha)) = (a.fingerprint, a.sha256.clone()) {
        // same policy, same reference set: the verdict must still follow from the digest
        let observed = super::reference::ObservedArtifact {
            entry_id: &o.subject,
            platform_target: &o.platform_target,
            format: a.verified_format.unwrap_or_default(),
            sha256: &sha,
        };
        let v = evaluate(&observed, references);
        if v.result != a.match_result {
            a.invalidated = true;
            a.match_result = MatchResult::NotApplicable;
            a.reference = ReferenceState::None;
            a.reference_kind = None;
        }
    }
    let authenticated = o.ladder.artifact.fingerprint == FingerprintState::Computed
        && o.ladder.artifact.match_result == MatchResult::MatchesAuthenticated
        && !o.ladder.artifact.invalidated;
    if !authenticated {
        if o.ladder.identity == IdentityState::Validated {
            o.ladder.identity = if o.ladder.install_record { IdentityState::EvidenceAvailable } else { IdentityState::Unknown };
            o.ladder.reclassified = true;
        }
        o.ladder.artifact_auth = ArtifactAuth::None;
        if o.ladder.version_trust == VersionTrust::Validated {
            o.ladder.version_trust = VersionTrust::Declared;
        }
        // A version that came only from an authenticated reference falls with that reference.
        if o.version_source.as_deref() == Some("yetkili referans özeti") {
            o.version = None;
            o.version_source = None;
            o.ladder.version = VersionState::Unknown;
            o.ladder.version_trust = VersionTrust::None;
        }
    }
}

fn rank(presence: &str) -> u8 {
    match presence {
        "present" => 3,
        "absent" => 2,
        _ => 1,
    }
}

fn word(rank: u8) -> &'static str {
    match rank {
        3 => "present",
        2 => "absent",
        _ => "unknown",
    }
}

fn scan_ref(o: &StoredObservation) -> ScanRef {
    ScanRef {
        scan_id: o.scan_id.clone(),
        finished_at: o.scan_finished_at.clone(),
        authorized_path: o.scan_authorized_path,
        authorized_user_config: o.scan_authorized_user_config,
        authorized_project: o.scan_authorized_project,
        authorized_fingerprint: o.scan_authorized_fingerprint,
    }
}

/// Describes the observation that was selected for a preference. `o` has already passed the relevance filter of `classify_request`, so its
/// `project_alias` is either None or equal to the preference's alias, and its surface is either the requested one or `local-cli`.
fn origin_of(o: &StoredObservation, requested_surface: &str) -> EvidenceOrigin {
    let valid = chrono::DateTime::parse_from_rfc3339(&o.observed_at).is_ok();
    EvidenceOrigin {
        project_relation: if o.project_alias.is_some() { ProjectRelation::ProjectSpecific } else { ProjectRelation::ComputerWide },
        surface_relation: if o.agent_surface == requested_surface {
            SurfaceRelation::ExactSurface
        } else {
            SurfaceRelation::LocalCliFallback
        },
        observed_at: valid.then(|| o.observed_at.clone()),
        age_hours: (valid && o.age_hours >= 0).then_some(o.age_hours),
        method: OriginMethod::from_row(&o.method),
    }
}

/// Pure classification of one accepted preference against the stored observations. The fourth member describes the selected observation's origin;
/// it never takes part in the selection.
#[allow(clippy::too_many_arguments)]
pub fn classify_request(
    entry_id: &str,
    project_alias: &str,
    surface: &str,
    platform: &str,
    observations: &[StoredObservation],
) -> (String, String, Option<ScanRef>, Option<EvidenceOrigin>) {
    // An observation belongs to this preference when it is about the same entry, for this project (or computer-wide),
    // on the same surface — or a computer-level program lookup (`local-cli`), which is not tied to one agent surface.
    let relevant = |o: &&StoredObservation| {
        o.subject == entry_id
            && o.project_alias.as_deref().map(|a| a == project_alias).unwrap_or(true)
            && (o.agent_surface == surface || o.agent_surface == "local-cli")
    };
    let mine: Vec<&StoredObservation> = observations.iter().filter(relevant).collect();
    let same_platform: Vec<&StoredObservation> = mine.iter().copied().filter(|o| o.platform_target == platform).collect();
    if same_platform.is_empty() {
        return if mine.is_empty() {
            (
                "not_scanned".into(),
                "Bu proje, ajan yüzeyi ve platform için henüz bir gözlem yok. Bu, aracın olmadığı anlamına gelmez.".into(),
                None,
                None,
            )
        } else {
            (
                "platform_mismatch".into(),
                "Yalnızca başka bir işletim sistemi/mimari için gözlem var; bu tercihin platformu için hiçbir şey gözlenmedi.".into(),
                None,
                None,
            )
        };
    }
    let best = |current: bool| same_platform.iter().copied().filter(|o| o.current == current).max_by_key(|o| (rank(&o.presence), o.id));
    if let Some(o) = best(true) {
        let r = rank(&o.presence);
        let note = match r {
            3 => "Son taramada bu bilgisayarda bulundu (bulunması, çalıştığı veya projede etkin olduğu anlamına gelmez).",
            2 => "Etkinleştirmek istediniz, ancak son taramada bu bilgisayarda bulunamadı.",
            _ => "Etkinleştirmek istediniz; son taramada bu bilgisayarda olup olmadığı belirlenemedi.",
        };
        return (format!("{}_current", word(r)), note.into(), Some(scan_ref(o)), Some(origin_of(o, surface)));
    }
    let o = best(false).expect("same_platform is not empty and none is current");
    let r = rank(&o.presence);
    let why = o.not_current_reason.clone().unwrap_or_else(|| "Güncel değil.".to_string());
    (
        format!("previously_{}", word(r)),
        format!("Eski sonuç ({why}) Yeniden tarayın; eski bir sonuç hiçbir işlem için dayanak sayılmaz."),
        Some(scan_ref(o)),
        Some(origin_of(o, surface)),
    )
}

fn requests(conn: &Connection, observations: &[StoredObservation], last: Option<&ScanSummary>) -> AppResult<Vec<RequestVsObservation>> {
    let mut stmt = conn.prepare(
        "SELECT e.entry_id, e.display_name, r.alias, r.display_name, p.agent_surface, p.platform_target
         FROM project_preferences p
         JOIN catalog_entries e ON e.entry_id = p.entry_id AND e.removed_at IS NULL
         JOIN project_registry r ON r.project_id = p.project_id AND r.removed_at IS NULL
         WHERE p.decision = 'accepted'
         ORDER BY e.display_name COLLATE NOCASE, r.alias, p.agent_surface, p.platform_target",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (entry_id, entry_name, project_alias, project_name, agent_surface, platform_target) = row?;
        let (status, note, basis, origin) = classify_request(&entry_id, &project_alias, &agent_surface, &platform_target, observations);
        // independent of the presence selection above: nothing in `classify_request` reads or depends on it
        let scoped = declarations::scoped_declarations(&entry_id, &project_alias, &agent_surface, &platform_target, observations, last);
        out.push(RequestVsObservation {
            entry_id,
            entry_name,
            project_alias,
            project_name,
            agent_surface,
            platform_target,
            status,
            note,
            basis,
            origin,
            declaration_detector: scoped.detector,
            declarations: scoped.readings,
        });
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
    let requests = requests(conn, &observations, last.as_ref())?;
    Ok(Inventory { last_scan: last, observations, requests, not_checked, stale_after_hours: STALE_AFTER_HOURS })
}
