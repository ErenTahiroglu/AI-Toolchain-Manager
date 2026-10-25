//! PRODUCT-07 H: scoped MCP-configuration readings placed BESIDE the presence verdict of `store::classify_request`.
//!
//! * Pure functions over rows the store already loaded (and privacy-sanitised). No I/O, no database, no scan, no clock, no write.
//! * It never touches the presence selection: `classify_request` is unchanged and does not call anything here.
//! * Only the existing Claude MCP detector (`claude-mcp-config` rows of the catalogued MCP entry on the Claude Code terminal surface) can produce a
//!   reading. Every other request gets `NotImplemented` - a statement about the application, never about the tool.
//! * A missing reading is NEVER a negative. `ScopeCoverage` says why nothing was read; only a CURRENT reading whose configuration is `NotConfigured`
//!   is a confirmed negative, and only for its own scope.
//! * The wire carries closed enums and a validated timestamp/age. No server name, entry, command, URL, token, path, evidence text, fingerprint, version,
//!   scan id or row id ever leaves this module. See docs/product-07/H_WIRE_CONTRACT.md.

use super::detect;
use super::store::{ScanSummary, StoredObservation};
use serde::Serialize;

/// The only method whose rows may become a reading (the existing Claude MCP detector).
const MCP_METHOD: &str = "claude-mcp-config";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectorKind {
    /// The Claude MCP reader exists for this entry and surface.
    ClaudeMcp,
    /// No detector reads MCP definitions for this entry/surface. NOT "not configured", NOT "supported".
    NotImplemented,
}

/// Display order, also the output order: project, local, global. A display order, not a precedence claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationScope {
    Project,
    Local,
    Global,
}

const SCOPES: [DeclarationScope; 3] = [DeclarationScope::Project, DeclarationScope::Local, DeclarationScope::Global];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationConfiguration {
    /// A definition whose name or command file name MATCHES the catalogued tool was recorded in that scope. Not loaded, approved, running or installed.
    Configured,
    /// The file was read (or does not exist) and holds no matching definition IN THAT SCOPE.
    NotConfigured,
    /// The file is not valid JSON / not the expected shape.
    Malformed,
    /// The file was not read (skipped).
    Unreadable,
    /// The row carries a value the MCP detector never emits; it is not trusted to mean anything.
    NotAssessed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationFreshness {
    Current,
    /// An earlier scan, and the latest scan did not check this area.
    PreviousAreaNotChecked,
    /// From the latest scan, but older than the staleness limit.
    PreviousStale,
    /// Any other reason the row is not current.
    PreviousOther,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeCoverage {
    /// A reading for this scope exists in `declarations`.
    Read,
    NoScan,
    PlatformNotScanned,
    NotAuthorized,
    ProjectNotInLatestScan,
    NoRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationReading {
    pub scope: DeclarationScope,
    pub configuration: DeclarationConfiguration,
    pub freshness: DeclarationFreshness,
    /// The row's own `observed_at`, only when it is a valid RFC 3339 timestamp (never repaired or guessed).
    pub observed_at: Option<String>,
    /// Whole hours at the time the inventory was read, only with a valid timestamp that is not in the future.
    pub age_hours: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationCoverage {
    pub project: ScopeCoverage,
    pub local: ScopeCoverage,
    pub global: ScopeCoverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationDetector {
    pub kind: DetectorKind,
    /// None exactly when `kind` is `NotImplemented`.
    pub coverage: Option<DeclarationCoverage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedDeclarations {
    pub detector: DeclarationDetector,
    pub readings: Vec<DeclarationReading>,
}

fn configuration_of(stored: &str) -> DeclarationConfiguration {
    match stored {
        "configured" => DeclarationConfiguration::Configured,
        "not_configured" => DeclarationConfiguration::NotConfigured,
        "malformed" => DeclarationConfiguration::Malformed,
        "unknown" => DeclarationConfiguration::Unreadable,
        _ => DeclarationConfiguration::NotAssessed,
    }
}

fn freshness_of(o: &StoredObservation) -> DeclarationFreshness {
    if o.current {
        DeclarationFreshness::Current
    } else if o.provenance == "earlier_scan" && !o.latest_scan_checked {
        DeclarationFreshness::PreviousAreaNotChecked
    } else if o.provenance == "latest_scan" && o.stale {
        DeclarationFreshness::PreviousStale
    } else {
        DeclarationFreshness::PreviousOther
    }
}

/// Does the row's own scope/alias say it belongs to `scope` FOR THIS PROJECT? `project` and `local` rows must carry exactly the request's alias; `global`
/// rows must carry none. Anything else (an alias on a global row, no alias on a project row, another project, the `unknown` scope) is ignored, never repaired.
fn belongs_to(scope: DeclarationScope, o: &StoredObservation, alias: &str) -> bool {
    match (scope, o.scope.as_str()) {
        (DeclarationScope::Project, "project") | (DeclarationScope::Local, "local") => o.project_alias.as_deref() == Some(alias),
        (DeclarationScope::Global, "global") => o.project_alias.is_none(),
        _ => false,
    }
}

fn reading_of(scope: DeclarationScope, o: &StoredObservation) -> DeclarationReading {
    let valid = chrono::DateTime::parse_from_rfc3339(&o.observed_at).is_ok();
    DeclarationReading {
        scope,
        configuration: configuration_of(&o.configuration),
        freshness: freshness_of(o),
        observed_at: valid.then(|| o.observed_at.clone()),
        age_hours: (valid && o.age_hours >= 0).then_some(o.age_hours),
    }
}

/// Why is there NO reading for `scope`? Derived from the latest scan's permissions and from whether this project's folder was in it. Never a negative about the tool.
fn missing_coverage(
    scope: DeclarationScope,
    alias: &str,
    platform: &str,
    observations: &[StoredObservation],
    last: Option<&ScanSummary>,
) -> ScopeCoverage {
    let Some(last) = last else { return ScopeCoverage::NoScan };
    if last.platform_target != platform {
        return ScopeCoverage::PlatformNotScanned;
    }
    let authorised = match scope {
        DeclarationScope::Project => last.authorized_project,
        DeclarationScope::Global => last.authorized_user_config,
        DeclarationScope::Local => last.authorized_project && last.authorized_user_config,
    };
    if !authorised {
        return ScopeCoverage::NotAuthorized;
    }
    let project_seen = observations.iter().any(|o| o.project_alias.as_deref() == Some(alias) && o.provenance == "latest_scan");
    if scope != DeclarationScope::Global && !project_seen {
        return ScopeCoverage::ProjectNotInLatestScan;
    }
    ScopeCoverage::NoRecord
}

/// The scoped MCP-configuration view for ONE request. Independent of the presence selection and of `classify_request`.
///
/// Within a scope the reading is the eligible row that is current if any is current, then the highest id. Output order: project, local, global.
pub fn scoped_declarations(
    entry_id: &str,
    project_alias: &str,
    surface: &str,
    platform: &str,
    observations: &[StoredObservation],
    last: Option<&ScanSummary>,
) -> ScopedDeclarations {
    if !detect::has_declaration_detector(entry_id, surface) {
        return ScopedDeclarations {
            detector: DeclarationDetector { kind: DetectorKind::NotImplemented, coverage: None },
            readings: Vec::new(),
        };
    }
    // exact subject, EXACT surface (a `local-cli` row is never eligible), the MCP detector's method, this platform
    let eligible: Vec<&StoredObservation> = observations
        .iter()
        .filter(|o| o.subject == entry_id && o.agent_surface == surface && o.method == MCP_METHOD && o.platform_target == platform)
        .collect();
    let mut readings = Vec::new();
    let mut coverage = [ScopeCoverage::Read; 3];
    for (i, scope) in SCOPES.into_iter().enumerate() {
        let best = eligible.iter().copied().filter(|o| belongs_to(scope, o, project_alias)).max_by_key(|o| (o.current, o.id));
        match best {
            Some(o) => readings.push(reading_of(scope, o)),
            None => coverage[i] = missing_coverage(scope, project_alias, platform, observations, last),
        }
    }
    ScopedDeclarations {
        detector: DeclarationDetector {
            kind: DetectorKind::ClaudeMcp,
            coverage: Some(DeclarationCoverage { project: coverage[0], local: coverage[1], global: coverage[2] }),
        },
        readings,
    }
}
