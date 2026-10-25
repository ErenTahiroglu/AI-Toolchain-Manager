//! PRODUCT-07 H: scoped MCP-configuration readings beside the presence verdict. Synthetic rows and synthetic temporary environments only - nothing here reads
//! the machine's real Claude Code files, runs a program, scans a real project or opens a network connection. Every "secret" is an obviously fake sentinel.
//!
//! Two promises are tested with the most force:
//!  1. the legacy presence verdict (status, note, basis, the four PRODUCT-06 origin members) is IDENTICAL to a frozen copy of the pre-change code, over an
//!     exhaustive pair space and a seeded random space (`origin.method` is the one additive member);
//!  2. a missing reading is never a negative, and nothing but closed vocabularies crosses the wire.

use super::*;
use crate::discovery::declarations::{
    scoped_declarations, DeclarationConfiguration as Cfg, DeclarationFreshness as Fresh, DeclarationScope as Scope3, DetectorKind,
    ScopeCoverage as Cov,
};
use crate::discovery::store::{classify_request, OriginMethod, ProjectRelation, ScanSummary, StoredObservation, SurfaceRelation};

const CC: &str = "claude-code-terminal";
const MCP: &str = "codebase-memory-mcp";
const STAMP: &str = "2026-10-17T00:00:00Z";
const MCP_METHOD: &str = "claude-mcp-config";

fn win() -> &'static str {
    PLATFORM_WINDOWS
}

/// One synthetic stored observation. Free-text fields carry sentinels so any leak is visible.
#[allow(clippy::too_many_arguments)]
fn row(
    id: i64,
    subject: &str,
    surface: &str,
    method: &str,
    scope: &str,
    alias: Option<&str>,
    presence: &str,
    configuration: &str,
    current: bool,
) -> StoredObservation {
    StoredObservation {
        id,
        scan_id: format!("scan-{id}"),
        subject: subject.into(),
        platform_target: win().into(),
        agent_surface: surface.into(),
        method: method.into(),
        presence: presence.into(),
        configuration: configuration.into(),
        version: Some("9.9.9-SENTINEL".into()),
        version_source: Some("SENTINEL-VERSION-SOURCE".into()),
        scope: scope.into(),
        project_alias: alias.map(str::to_string),
        confidence: "low".into(),
        evidence: "SENTINEL-EVIDENCE-TEXT server-name-SENTINEL".into(),
        location: Some("C:\\Users\\SENTINEL\\secret\\tool.exe".into()),
        limitations: vec!["SENTINEL-LIMITATION".into()],
        identity_verified: false,
        functionality_tested: false,
        observed_at: STAMP.into(),
        stale: !current,
        age_hours: 5,
        legacy: false,
        scan_finished_at: STAMP.into(),
        scan_authorized_path: true,
        scan_authorized_user_config: true,
        scan_authorized_project: true,
        scan_authorized_fingerprint: false,
        provenance: if current { "latest_scan" } else { "earlier_scan" }.into(),
        latest_scan_checked: current,
        current,
        not_current_reason: if current { None } else { Some("eski".into()) },
        ladder: crate::discovery::types::Provenance::default(),
    }
}

fn mcp(id: i64, scope: &str, alias: Option<&str>, configuration: &str, current: bool) -> StoredObservation {
    row(id, MCP, CC, MCP_METHOD, scope, alias, "unknown", configuration, current)
}

fn path_row(id: i64, presence: &str, current: bool) -> StoredObservation {
    row(id, MCP, "local-cli", "path-executable", "global", None, presence, "not_applicable", current)
}

fn last_scan(platform: &str, path: bool, user: bool, project: bool) -> ScanSummary {
    ScanSummary {
        scan_id: "latest".into(),
        started_at: STAMP.into(),
        finished_at: STAMP.into(),
        platform_target: platform.into(),
        authorized_path: path,
        authorized_user_config: user,
        authorized_project: project,
        authorized_fingerprint: false,
        status: "completed".into(),
        observation_count: 0,
    }
}

fn decl(obs: &[StoredObservation]) -> crate::discovery::declarations::ScopedDeclarations {
    scoped_declarations(MCP, "p", CC, win(), obs, Some(&last_scan(win(), true, true, true)))
}

// ======================================================================================================================================
// 1. The legacy verdict is identical to the pre-change code
// ======================================================================================================================================

type FrozenOrigin = (bool, bool, Option<String>, Option<i64>);

/// `classify_request` + `origin_of` of commit b7eff9e (before PRODUCT-07 H), copied verbatim in behaviour. Returns the selected scan id and the four
/// PRODUCT-06 origin members. Do NOT "fix" this function: it is the oracle.
fn frozen(
    entry_id: &str,
    project_alias: &str,
    surface: &str,
    platform: &str,
    observations: &[StoredObservation],
) -> (String, String, Option<String>, Option<FrozenOrigin>) {
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
    let origin = |o: &StoredObservation| -> FrozenOrigin {
        let valid = chrono::DateTime::parse_from_rfc3339(&o.observed_at).is_ok();
        (
            o.project_alias.is_some(),
            o.agent_surface == surface,
            valid.then(|| o.observed_at.clone()),
            (valid && o.age_hours >= 0).then_some(o.age_hours),
        )
    };
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
        return (format!("{}_current", word(r)), note.into(), Some(o.scan_id.clone()), Some(origin(o)));
    }
    let o = best(false).expect("same_platform is not empty and none is current");
    let r = rank(&o.presence);
    let why = o.not_current_reason.clone().unwrap_or_else(|| "Güncel değil.".to_string());
    (
        format!("previously_{}", word(r)),
        format!("Eski sonuç ({why}) Yeniden tarayın; eski bir sonuç hiçbir işlem için dayanak sayılmaz."),
        Some(o.scan_id.clone()),
        Some(origin(o)),
    )
}

fn assert_same_as_frozen(obs: &[StoredObservation], requested: &str, alias: &str, platform: &str, label: &str) {
    let (status, note, basis, origin) = classify_request("x", alias, requested, platform, obs);
    let (f_status, f_note, f_scan, f_origin) = frozen("x", alias, requested, platform, obs);
    assert_eq!((&status, &note), (&f_status, &f_note), "{label}: status/note");
    assert_eq!(basis.as_ref().map(|b| b.scan_id.clone()), f_scan, "{label}: a different row was selected");
    match (&basis, &origin, &f_origin) {
        (None, None, None) => {}
        (Some(b), Some(o), Some(f)) => {
            let w = obs.iter().find(|x| x.scan_id == b.scan_id).unwrap();
            assert_eq!(
                (b.finished_at.as_str(), b.authorized_path, b.authorized_user_config, b.authorized_project, b.authorized_fingerprint),
                (
                    w.scan_finished_at.as_str(),
                    w.scan_authorized_path,
                    w.scan_authorized_user_config,
                    w.scan_authorized_project,
                    w.scan_authorized_fingerprint
                ),
                "{label}: basis"
            );
            assert_eq!(o.project_relation == ProjectRelation::ProjectSpecific, f.0, "{label}");
            assert_eq!(o.surface_relation == SurfaceRelation::ExactSurface, f.1, "{label}");
            assert_eq!((o.observed_at.clone(), o.age_hours), (f.2.clone(), f.3), "{label}");
            // the one additive member: derived from the WINNER's own method through the closed whitelist
            assert_eq!(o.method, OriginMethod::from_row(&w.method), "{label}: method");
        }
        other => panic!("{label}: basis/origin presence must agree, got {other:?}"),
    }
}

/// (presence, current, project alias, surface, method, platform)
type PairRow<'a> = (&'a str, bool, Option<&'a str>, &'a str, &'a str, &'a str);

#[test]
fn the_legacy_verdict_equals_the_frozen_pre_change_code_for_every_ordered_pair_of_rows() {
    let mut rows: Vec<PairRow> = Vec::new();
    for presence in ["present", "absent", "unknown", "unsupported"] {
        for current in [true, false] {
            for alias in [None, Some("p"), Some("q")] {
                for surface in [CC, "local-cli", "antigravity-ide"] {
                    for method in ["path-executable", MCP_METHOD, "claude-plugin-declaration"] {
                        for platform in [PLATFORM_WINDOWS, PLATFORM_MACOS] {
                            rows.push((presence, current, alias, surface, method, platform));
                        }
                    }
                }
            }
        }
    }
    assert_eq!(rows.len(), 432);
    let build = |id: i64, r: &PairRow| {
        let mut o = row(id, "x", r.3, r.4, if r.2.is_some() { "project" } else { "global" }, r.2, r.0, "not_applicable", r.1);
        o.platform_target = r.5.into();
        o
    };
    let mut pairs = 0u64;
    for a in &rows {
        for b in &rows {
            let obs = [build(1, a), build(2, b)];
            for requested in [CC, "local-cli"] {
                assert_same_as_frozen(&obs, requested, "p", PLATFORM_WINDOWS, "pair");
            }
            pairs += 1;
        }
    }
    assert_eq!(pairs, 432 * 432);
}

#[test]
fn the_legacy_verdict_equals_the_frozen_code_over_a_seeded_random_space_including_configuration_values_and_other_subjects() {
    let mut state: u64 = 0xC0FF_EE00_D15C_0001;
    let mut next = move |n: u64| {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (state >> 33) % n
    };
    let mut reached = [0u32; 4];
    for round in 0..20_000 {
        let count = next(8) as usize;
        let mut obs: Vec<StoredObservation> = Vec::new();
        for i in 0..count {
            let alias = [None, Some("p"), Some("q")][next(3) as usize];
            let surface = [CC, "local-cli", "antigravity-ide", "codex-cli"][next(4) as usize];
            let method = [
                "path-executable",
                MCP_METHOD,
                "claude-plugin-declaration",
                "claude-settings-file",
                "no-detector",
                "not-documented",
                "garbage",
            ][next(7) as usize];
            let presence = ["present", "absent", "unknown", "unsupported"][next(4) as usize];
            let configuration = ["not_applicable", "configured", "not_configured", "malformed", "unknown", "garbage"][next(6) as usize];
            let scope = ["global", "project", "local", "unknown"][next(4) as usize];
            let subject = ["x", "x", "x", "y"][next(4) as usize];
            let mut o = row(100 + i as i64, subject, surface, method, scope, alias, presence, configuration, next(2) == 0);
            o.platform_target = [PLATFORM_WINDOWS, PLATFORM_WINDOWS, PLATFORM_MACOS][next(3) as usize].into();
            o.observed_at =
                ["2026-10-17T00:00:00Z", "2026-09-01T08:00:00+03:00", "garbage", "2099-01-01T00:00:00Z"][next(4) as usize].into();
            o.age_hours = [0, 40, -1, i64::from(i32::MAX)][next(4) as usize];
            o.id = 1 + (i as i64) * 5 + next(5) as i64;
            o.scan_id = format!("scan-{}", o.id);
            if obs.iter().any(|x| x.id == o.id) {
                o.id += 1000 + i as i64;
                o.scan_id = format!("scan-{}", o.id);
            }
            obs.push(o);
        }
        for requested in [CC, "local-cli", "codex-cli", "antigravity-ide"] {
            assert_same_as_frozen(&obs, requested, "p", PLATFORM_WINDOWS, &format!("round {round}"));
            let status = classify_request("x", "p", requested, PLATFORM_WINDOWS, &obs).0;
            reached[if status.starts_with("present") {
                0
            } else if status.starts_with("absent") {
                1
            } else if status.starts_with("previously") {
                2
            } else {
                3
            }] += 1;
        }
    }
    assert!(reached.iter().all(|n| *n > 500), "the generator must reach every family of outcomes: {reached:?}");
}

#[test]
fn the_selection_code_does_not_read_configuration_scope_or_declarations() {
    let text = include_str!("store.rs");
    let start = text.find("pub fn classify_request").expect("classify_request exists");
    let end = text[start..].find("fn requests(").expect("requests follows") + start;
    let body = &text[start..end];
    for forbidden in ["configuration", ".scope", "declaration", "method"] {
        assert!(!body.contains(forbidden), "the presence selection must not read `{forbidden}`");
    }
    assert!(body.contains("max_by_key(|o| (rank(&o.presence), o.id))"), "the ranking expression is unchanged");
}

// ======================================================================================================================================
// 2. Coexistence: machine presence and scoped configuration are both reported, neither overrides the other
// ======================================================================================================================================

#[test]
fn a_path_hit_and_a_not_configured_project_file_are_both_reported_and_neither_hides_the_other() {
    let obs = vec![
        path_row(10, "present", true),
        mcp(14, "project", Some("p"), "not_configured", true),
        mcp(12, "global", None, "not_configured", true),
    ];
    let (status, _, _, origin) = classify_request(MCP, "p", CC, win(), &obs);
    assert_eq!(status, "present_current", "the legacy verdict is unchanged: the PATH name match still wins the presence headline");
    let origin = origin.unwrap();
    assert_eq!(
        (origin.project_relation, origin.surface_relation, origin.method),
        (ProjectRelation::ComputerWide, SurfaceRelation::LocalCliFallback, Some(OriginMethod::PathExecutable))
    );
    let d = decl(&obs);
    assert_eq!(d.detector.kind, DetectorKind::ClaudeMcp);
    let summary: Vec<_> = d.readings.iter().map(|r| (r.scope, r.configuration, r.freshness)).collect();
    assert_eq!(summary, vec![(Scope3::Project, Cfg::NotConfigured, Fresh::Current), (Scope3::Global, Cfg::NotConfigured, Fresh::Current)]);
    let cov = d.detector.coverage.unwrap();
    assert_eq!(
        (cov.project, cov.local, cov.global),
        (Cov::Read, Cov::NoRecord, Cov::Read),
        "the local scope has NO reading: that is not 'not configured'"
    );
}

#[test]
fn a_path_miss_and_a_configured_project_file_are_both_reported() {
    let obs = vec![path_row(10, "absent", true), mcp(14, "project", Some("p"), "configured", true)];
    assert_eq!(classify_request(MCP, "p", CC, win(), &obs).0, "absent_current");
    let d = decl(&obs);
    assert_eq!(d.readings.len(), 1);
    assert_eq!(
        (d.readings[0].scope, d.readings[0].configuration, d.readings[0].freshness),
        (Scope3::Project, Cfg::Configured, Fresh::Current)
    );
}

#[test]
fn one_scope_not_configured_and_another_configured_are_reported_per_scope_never_aggregated() {
    let obs = vec![mcp(1, "project", Some("p"), "not_configured", true), mcp(2, "global", None, "configured", true)];
    let d = decl(&obs);
    let by: Vec<_> = d.readings.iter().map(|r| (r.scope, r.configuration)).collect();
    assert_eq!(by, vec![(Scope3::Project, Cfg::NotConfigured), (Scope3::Global, Cfg::Configured)]);
}

#[test]
fn mcp_rows_never_carry_a_presence_so_the_declaration_never_creates_one() {
    // a reading is derived from `configuration` only; whatever `presence` the row has, the legacy verdict follows the unchanged rule
    for presence in ["unknown", "present", "absent"] {
        let obs = vec![row(1, MCP, CC, MCP_METHOD, "project", Some("p"), presence, "configured", true)];
        let (status, ..) = classify_request(MCP, "p", CC, win(), &obs);
        assert_eq!(status, format!("{presence}_current"));
        assert_eq!(decl(&obs).readings[0].configuration, Cfg::Configured);
    }
}

// ======================================================================================================================================
// 3. Scoped readings: 0..=3, order, duplicates, eligibility
// ======================================================================================================================================

#[test]
fn zero_rows_give_the_detector_and_a_reason_per_scope_but_no_reading() {
    let none: Vec<StoredObservation> = vec![];
    let d = scoped_declarations(MCP, "p", CC, win(), &none, None);
    assert_eq!(d.detector.kind, DetectorKind::ClaudeMcp);
    assert!(d.readings.is_empty());
    let c = d.detector.coverage.unwrap();
    assert_eq!((c.project, c.local, c.global), (Cov::NoScan, Cov::NoScan, Cov::NoScan));
}

#[test]
fn one_two_and_three_readings_keep_the_order_project_local_global_whatever_the_row_order_or_ids() {
    let p = mcp(30, "project", Some("p"), "configured", true);
    let l = mcp(20, "local", Some("p"), "not_configured", true);
    let g = mcp(10, "global", None, "not_configured", true);
    for obs in [vec![g.clone(), l.clone(), p.clone()], vec![p.clone(), g.clone(), l.clone()], vec![l.clone(), p.clone(), g.clone()]] {
        let d = decl(&obs);
        assert_eq!(d.readings.iter().map(|r| r.scope).collect::<Vec<_>>(), vec![Scope3::Project, Scope3::Local, Scope3::Global]);
        let c = d.detector.coverage.unwrap();
        assert_eq!((c.project, c.local, c.global), (Cov::Read, Cov::Read, Cov::Read));
    }
    assert_eq!(decl(std::slice::from_ref(&l)).readings.len(), 1);
    assert_eq!(decl(&[g.clone(), p.clone()]).readings.iter().map(|r| r.scope).collect::<Vec<_>>(), vec![Scope3::Project, Scope3::Global]);
}

#[test]
fn duplicate_rows_in_one_scope_resolve_by_a_documented_rule_current_first_then_highest_id() {
    // both current: the higher id
    let d = decl(&[mcp(1, "project", Some("p"), "configured", true), mcp(2, "project", Some("p"), "not_configured", true)]);
    assert_eq!((d.readings.len(), d.readings[0].configuration), (1, Cfg::NotConfigured));
    // a current row beats a newer non-current one
    let d = decl(&[mcp(1, "project", Some("p"), "configured", true), mcp(9, "project", Some("p"), "not_configured", false)]);
    assert_eq!((d.readings[0].configuration, d.readings[0].freshness), (Cfg::Configured, Fresh::Current));
    // both non-current: the higher id, still marked previous
    let d = decl(&[mcp(1, "project", Some("p"), "configured", false), mcp(9, "project", Some("p"), "not_configured", false)]);
    assert_eq!((d.readings[0].configuration, d.readings[0].freshness), (Cfg::NotConfigured, Fresh::PreviousAreaNotChecked));
    // a missing scope is never filled in from a duplicate of another scope
    assert!(decl(&[mcp(1, "project", Some("p"), "configured", true), mcp(2, "project", Some("p"), "configured", true)])
        .readings
        .iter()
        .all(|r| r.scope == Scope3::Project));
}

#[test]
fn rows_of_another_project_scope_inconsistent_rows_and_unrelated_methods_are_ignored() {
    let ignored = vec![
        mcp(1, "project", Some("q"), "configured", true), // another project
        mcp(2, "local", Some("q"), "configured", true),   // another project
        mcp(3, "project", None, "configured", true),      // a project row without the project: inconsistent
        mcp(4, "global", Some("p"), "configured", true),  // a global row that claims a project: inconsistent
        mcp(5, "unknown", Some("p"), "configured", true), // unknown scope
        mcp(6, "bogus", None, "configured", true),        // not a scope at all
        row(7, MCP, CC, "claude-plugin-declaration", "project", Some("p"), "present", "configured", true), // plugin rows are not eligible in this phase
        row(8, MCP, CC, "claude-settings-file", "project", Some("p"), "unknown", "configured", true),
        row(9, MCP, CC, "path-executable", "global", None, "present", "configured", true),
        row(10, MCP, "local-cli", MCP_METHOD, "project", Some("p"), "unknown", "configured", true), // the local-cli fallback is never a project MCP declaration
        row(11, "other-tool", CC, MCP_METHOD, "project", Some("p"), "unknown", "configured", true), // another entry
    ];
    let d = decl(&ignored);
    assert!(d.readings.is_empty(), "{:?}", d.readings);
    let mut other_os = mcp(12, "project", Some("p"), "configured", true);
    other_os.platform_target = PLATFORM_MACOS.into();
    assert!(decl(&[other_os]).readings.is_empty());
    // the same project's rows ARE used
    assert_eq!(decl(&[mcp(13, "project", Some("p"), "configured", true)]).readings.len(), 1);
}

#[test]
fn configuration_values_map_through_a_closed_list_and_an_unexpected_value_is_not_assessed() {
    for (stored, want) in [
        ("configured", Cfg::Configured),
        ("not_configured", Cfg::NotConfigured),
        ("malformed", Cfg::Malformed),
        ("unknown", Cfg::Unreadable),
        ("disabled", Cfg::NotAssessed),
        ("referenced_not_present", Cfg::NotAssessed),
        ("not_applicable", Cfg::NotAssessed),
        ("", Cfg::NotAssessed),
        ("CONFIGURED", Cfg::NotAssessed),
        ("<script>", Cfg::NotAssessed),
        ("C:\\Users\\SENTINEL", Cfg::NotAssessed),
    ] {
        assert_eq!(decl(&[mcp(1, "project", Some("p"), stored, true)]).readings[0].configuration, want, "{stored:?}");
    }
}

// ======================================================================================================================================
// 4. Currentness: an old scan is old, even if it says configured
// ======================================================================================================================================

#[test]
fn freshness_is_derived_from_provenance_staleness_and_whether_the_latest_scan_checked_the_area() {
    let mut r = mcp(1, "project", Some("p"), "configured", true);
    assert_eq!(decl(&[r.clone()]).readings[0].freshness, Fresh::Current);
    // an earlier scan, latest scan did not check the area
    r.current = false;
    r.stale = false;
    r.provenance = "earlier_scan".into();
    r.latest_scan_checked = false;
    assert_eq!(decl(&[r.clone()]).readings[0].freshness, Fresh::PreviousAreaNotChecked);
    // an earlier scan although the latest one checked the area
    r.latest_scan_checked = true;
    assert_eq!(decl(&[r.clone()]).readings[0].freshness, Fresh::PreviousOther);
    // the latest scan, but older than the limit
    r.provenance = "latest_scan".into();
    r.stale = true;
    assert_eq!(decl(&[r.clone()]).readings[0].freshness, Fresh::PreviousStale);
    // inconsistent: from the latest scan, fresh, not current
    r.stale = false;
    assert_eq!(decl(&[r.clone()]).readings[0].freshness, Fresh::PreviousOther);
    // in no case is a non-current `configured` row presented as current
    assert_eq!(decl(&[r]).readings[0].configuration, Cfg::Configured);
}

#[test]
fn an_old_project_scan_stays_previous_while_a_current_user_level_row_exists() {
    let obs = vec![
        mcp(1, "project", Some("p"), "configured", false),
        mcp(2, "local", Some("p"), "configured", false),
        mcp(3, "global", None, "not_configured", true),
    ];
    let d = decl(&obs);
    let by: Vec<_> = d.readings.iter().map(|r| (r.scope, r.configuration, r.freshness)).collect();
    assert_eq!(
        by,
        vec![
            (Scope3::Project, Cfg::Configured, Fresh::PreviousAreaNotChecked),
            (Scope3::Local, Cfg::Configured, Fresh::PreviousAreaNotChecked),
            (Scope3::Global, Cfg::NotConfigured, Fresh::Current)
        ]
    );
}

#[test]
fn timestamps_follow_the_origin_rules_a_future_stamp_has_no_age_and_a_bad_stamp_has_neither() {
    let mut future = mcp(1, "project", Some("p"), "configured", true);
    future.observed_at = "2099-01-01T00:00:00Z".into();
    future.age_hours = -1; // what the production age computation yields for any instant after `now`
    let r = &decl(&[future]).readings[0];
    assert_eq!((r.observed_at.as_deref(), r.age_hours), (Some("2099-01-01T00:00:00Z"), None));
    let mut bad = mcp(1, "project", Some("p"), "configured", true);
    bad.observed_at = "garbage".into();
    bad.age_hours = i64::from(i32::MAX);
    let r = &decl(&[bad]).readings[0];
    assert_eq!((r.observed_at.clone(), r.age_hours), (None, None));
    let mut ok = mcp(1, "project", Some("p"), "configured", true);
    ok.observed_at = "2026-10-01T12:30:00+02:00".into();
    ok.age_hours = 400;
    let r = &decl(&[ok]).readings[0];
    assert_eq!((r.observed_at.as_deref(), r.age_hours), (Some("2026-10-01T12:30:00+02:00"), Some(400)), "as stored, not converted");
}

// ======================================================================================================================================
// 5. Coverage: why a scope has no reading - and that it is never a negative
// ======================================================================================================================================

#[test]
fn a_missing_reading_is_explained_by_the_latest_scan_never_turned_into_not_configured() {
    let none: Vec<StoredObservation> = vec![];
    let cov = |last: Option<&ScanSummary>, obs: &[StoredObservation], platform: &str| {
        scoped_declarations(MCP, "p", CC, platform, obs, last).detector.coverage.unwrap()
    };
    // no scan at all
    let c = cov(None, &none, win());
    assert_eq!((c.project, c.local, c.global), (Cov::NoScan, Cov::NoScan, Cov::NoScan));
    // the latest scan is for another platform
    let other = last_scan(PLATFORM_MACOS, true, true, true);
    let c = cov(Some(&other), &none, win());
    assert_eq!((c.project, c.local, c.global), (Cov::PlatformNotScanned, Cov::PlatformNotScanned, Cov::PlatformNotScanned));
    // the latest scan was not allowed to read anything of this kind
    let nothing = last_scan(win(), true, false, false);
    let c = cov(Some(&nothing), &none, win());
    assert_eq!((c.project, c.local, c.global), (Cov::NotAuthorized, Cov::NotAuthorized, Cov::NotAuthorized));
    let user_only = last_scan(win(), true, true, false);
    let c = cov(Some(&user_only), &none, win());
    assert_eq!(
        (c.project, c.local, c.global),
        (Cov::NotAuthorized, Cov::NotAuthorized, Cov::NoRecord),
        "the user files were allowed but nothing was recorded"
    );
    let project_only = last_scan(win(), true, false, true);
    let c = cov(Some(&project_only), &none, win());
    assert_eq!((c.local, c.global), (Cov::NotAuthorized, Cov::NotAuthorized), "the local scope needs both permissions");
    // authorised, but another project's folder was the one scanned (one scan holds at most one project)
    let all = last_scan(win(), true, true, true);
    let c = cov(Some(&all), &none, win());
    assert_eq!((c.project, c.local, c.global), (Cov::ProjectNotInLatestScan, Cov::ProjectNotInLatestScan, Cov::NoRecord));
    // the project WAS in the latest scan but no row exists for this scope
    let seen = vec![row(1, "other-tool", CC, "claude-plugin-declaration", "project", Some("p"), "absent", "not_configured", true)];
    let c = cov(Some(&all), &seen, win());
    assert_eq!((c.project, c.local, c.global), (Cov::NoRecord, Cov::NoRecord, Cov::NoRecord));
    // a project seen only by an EARLIER scan does not count as "in the latest scan"
    let old = vec![row(1, "other-tool", CC, "claude-plugin-declaration", "project", Some("p"), "absent", "not_configured", false)];
    assert_eq!(cov(Some(&all), &old, win()).project, Cov::ProjectNotInLatestScan);
}

#[test]
fn coverage_never_lets_an_empty_list_read_as_a_confirmed_negative() {
    let d = scoped_declarations(MCP, "p", CC, win(), &[], Some(&last_scan(win(), true, true, true)));
    assert!(d.readings.is_empty());
    // the only confirmed negative is a CURRENT reading of its own scope
    let d = decl(&[mcp(1, "project", Some("p"), "not_configured", true)]);
    assert_eq!((d.readings[0].configuration, d.readings[0].freshness), (Cfg::NotConfigured, Fresh::Current));
    let d = decl(&[mcp(1, "project", Some("p"), "not_configured", false)]);
    assert_ne!(d.readings[0].freshness, Fresh::Current, "an old negative is not today's negative");
}

// ======================================================================================================================================
// 6. Other agents, other entries: the application says "no detector", never invents support
// ======================================================================================================================================

#[test]
fn only_the_claude_mcp_detector_for_the_mcp_entry_on_the_claude_terminal_can_populate_readings() {
    assert!(detect::has_declaration_detector(MCP, CC));
    for surface in ["codex-cli", "antigravity-ide", "claude-desktop-code", "local-cli", "", "Claude-Code-Terminal", "codex"] {
        assert!(!detect::has_declaration_detector(MCP, surface), "{surface:?}");
    }
    for entry in [
        "gitleaks",
        "betterleaks",
        "agent-browser",
        "strix",
        "markitdown",
        "graphify",
        "anthropic-frontend-design",
        "superpowers",
        "ponytail",
        "caveman",
        "cloudflare-security-audit-skill",
        "microsoft-playwright-mcp",
        "",
        "agent:claude-code",
    ] {
        assert!(!detect::has_declaration_detector(entry, CC), "{entry:?}");
    }
    // even with eligible-looking Claude rows present, an unsupported host gets nothing and is NOT described as scanned
    let rows =
        vec![mcp(1, "project", Some("p"), "configured", true), mcp(2, "global", None, "configured", true), path_row(3, "present", true)];
    for surface in ["codex-cli", "antigravity-ide", "claude-desktop-code", "local-cli"] {
        let d = scoped_declarations(MCP, "p", surface, win(), &rows, Some(&last_scan(win(), true, true, true)));
        assert_eq!(d.detector.kind, DetectorKind::NotImplemented, "{surface}");
        assert!(d.detector.coverage.is_none() && d.readings.is_empty(), "{surface}");
    }
    // and a CLI-only tool on the Claude surface
    let d = scoped_declarations(
        "gitleaks",
        "p",
        CC,
        win(),
        &[row(1, "gitleaks", CC, MCP_METHOD, "project", Some("p"), "unknown", "configured", true)],
        None,
    );
    assert_eq!(d.detector.kind, DetectorKind::NotImplemented);
}

#[test]
fn a_local_cli_machine_lookup_for_a_codex_request_is_a_fallback_and_never_a_declaration() {
    let rows = vec![path_row(1, "present", true), mcp(2, "project", Some("p"), "configured", true)];
    let (status, _, _, origin) = classify_request(MCP, "p", "codex-cli", win(), &rows);
    assert_eq!(status, "present_current");
    let origin = origin.unwrap();
    assert_eq!(origin.surface_relation, SurfaceRelation::LocalCliFallback, "labelled as the stand-in it is");
    assert_eq!(origin.method, Some(OriginMethod::PathExecutable));
    let d = scoped_declarations(MCP, "p", "codex-cli", win(), &rows, None);
    assert!(d.readings.is_empty() && d.detector.kind == DetectorKind::NotImplemented);
}

#[test]
fn plugin_presence_from_the_user_cache_keeps_its_legacy_origin_but_names_its_technique_and_is_not_a_declaration() {
    let plugin = row(5, "anthropic-frontend-design", CC, "claude-plugin-declaration", "project", Some("p"), "present", "configured", true);
    let (status, _, _, origin) = classify_request("anthropic-frontend-design", "p", CC, win(), std::slice::from_ref(&plugin));
    assert_eq!(status, "present_current");
    let o = origin.unwrap();
    // legacy semantics untouched: the alias still makes the origin `project_specific` ...
    assert_eq!((o.project_relation, o.surface_relation), (ProjectRelation::ProjectSpecific, SurfaceRelation::ExactSurface));
    // ... and the additive method tells the UI that the presence comes from the plugin record/cache
    assert_eq!(o.method, Some(OriginMethod::ClaudePluginDeclaration));
    let d = scoped_declarations("anthropic-frontend-design", "p", CC, win(), &[plugin], None);
    assert!(d.readings.is_empty() && d.detector.kind == DetectorKind::NotImplemented);
}

#[test]
fn origin_method_is_a_closed_whitelist_and_pseudo_or_unknown_methods_are_null() {
    for (m, want) in [
        ("path-executable", Some(OriginMethod::PathExecutable)),
        ("native-install-location", Some(OriginMethod::NativeInstallLocation)),
        ("legacy-local-install", Some(OriginMethod::LegacyLocalInstall)),
        ("claude-settings-file", Some(OriginMethod::ClaudeSettingsFile)),
        ("claude-plugin-declaration", Some(OriginMethod::ClaudePluginDeclaration)),
        ("claude-mcp-config", Some(OriginMethod::ClaudeMcpConfig)),
        ("no-detector", None),
        ("not-documented", None),
        ("unsupported-platform", None),
        ("", None),
        ("PATH-EXECUTABLE", None),
        ("C:\\Users\\SENTINEL", None),
    ] {
        assert_eq!(OriginMethod::from_row(m), want, "{m:?}");
        let o = row(1, "x", CC, m, "global", None, "present", "not_applicable", true);
        let (_, _, _, origin) = classify_request("x", "p", CC, win(), &[o]);
        assert_eq!(origin.unwrap().method, want, "{m:?}");
    }
}

// ======================================================================================================================================
// 7. The wire: only closed vocabularies, nothing from the row
// ======================================================================================================================================

fn request_json(obs: &[StoredObservation], last: Option<&ScanSummary>) -> serde_json::Value {
    let (status, note, basis, origin) = classify_request(MCP, "p", CC, win(), obs);
    let scoped = scoped_declarations(MCP, "p", CC, win(), obs, last);
    serde_json::to_value(store::RequestVsObservation {
        entry_id: MCP.into(),
        entry_name: "M".into(),
        project_alias: "p".into(),
        project_name: "P".into(),
        agent_surface: CC.into(),
        platform_target: win().into(),
        status,
        note,
        basis,
        origin,
        declaration_detector: scoped.detector,
        declarations: scoped.readings,
    })
    .unwrap()
}

#[test]
fn the_wire_shape_is_exactly_the_contract_and_carries_nothing_from_the_row() {
    let mut weird = mcp(3, "global", None, "C:\\Users\\SENTINEL\\secret", true); // a tampered configuration string
    weird.evidence = "token ghp_SENTINEL server-name-SENTINEL https://example.invalid/mcp".into();
    let obs = vec![
        path_row(1, "present", true),
        mcp(2, "project", Some("p"), "configured", true),
        weird,
        mcp(4, "local", Some("p"), "unknown", false),
    ];
    let v = request_json(&obs, Some(&last_scan(win(), true, true, true)));
    let keys: std::collections::BTreeSet<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    let expected = [
        "agentSurface",
        "basis",
        "declarationDetector",
        "declarations",
        "entryId",
        "entryName",
        "note",
        "origin",
        "platformTarget",
        "projectAlias",
        "projectName",
        "status",
    ];
    assert_eq!(keys, expected.iter().copied().collect());
    assert_eq!(v["origin"]["method"], "path-executable");
    assert_eq!(v["declarationDetector"]["kind"], "claude_mcp");
    let cov = v["declarationDetector"]["coverage"].as_object().unwrap();
    assert_eq!(
        cov.keys().map(String::as_str).collect::<std::collections::BTreeSet<_>>(),
        ["global", "local", "project"].iter().copied().collect()
    );
    let readings = v["declarations"].as_array().unwrap();
    assert_eq!(readings.len(), 3);
    for r in readings {
        let k: std::collections::BTreeSet<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(k, ["ageHours", "configuration", "freshness", "observedAt", "scope"].iter().copied().collect(), "{r}");
    }
    assert_eq!(readings.iter().map(|r| r["scope"].as_str().unwrap()).collect::<Vec<_>>(), vec!["project", "local", "global"]);
    // every string is a member of a closed list (or a timestamp)
    let allowed_conf = ["configured", "not_configured", "malformed", "unreadable", "not_assessed"];
    let allowed_fresh = ["current", "previous_area_not_checked", "previous_stale", "previous_other"];
    let allowed_cov = ["read", "no_scan", "platform_not_scanned", "not_authorized", "project_not_in_latest_scan", "no_record"];
    for r in readings {
        assert!(allowed_conf.contains(&r["configuration"].as_str().unwrap()), "{r}");
        assert!(allowed_fresh.contains(&r["freshness"].as_str().unwrap()), "{r}");
    }
    for (_, c) in cov {
        assert!(allowed_cov.contains(&c.as_str().unwrap()));
    }
    // the members THIS change adds (and the origin it extends) are free of anything from the row; `basis.scanId` is a pre-existing PRODUCT-04 member and is not judged here
    let text = serde_json::json!([v["declarationDetector"], v["declarations"], v["origin"]]).to_string();
    for leak in [
        "SENTINEL",
        "secret",
        "ghp_",
        "example.invalid",
        "server-name",
        "C:\\\\Users",
        "9.9.9",
        "tool.exe",
        "scan-",
        "limitation",
        "evidence",
    ] {
        assert!(!text.contains(leak), "{leak} leaked into {text}");
    }
    // not-implemented requests carry an explicit null coverage and an empty list, not missing members
    let other = serde_json::to_value(scoped_declarations(MCP, "p", "codex-cli", win(), &obs, None).detector).unwrap();
    assert_eq!(other, serde_json::json!({ "kind": "not_implemented", "coverage": null }));
}

#[test]
fn the_new_code_is_pure_it_has_no_io_process_network_database_or_clock() {
    let text = include_str!("declarations.rs");
    let code = &text[text.find("use super::detect;").unwrap()..];
    for forbidden in [
        "std::fs",
        "std::process",
        "Command::",
        "std::net",
        "TcpStream",
        "reqwest",
        ".spawn(",
        "File::",
        "rusqlite",
        "Connection",
        "execute(",
        "now_utc",
        "Utc::now",
        "SystemTime",
        "Instant",
        "std::env",
    ] {
        assert!(!code.contains(forbidden), "declarations.rs must stay pure, found {forbidden}");
    }
}

// ======================================================================================================================================
// 8. End to end through the real store (synthetic temporary environment, the neighbouring tests' pattern)
// ======================================================================================================================================

fn auth(path: bool, user: bool, project: Option<(&str, &Path)>) -> Authorization {
    Authorization {
        path_executables: path,
        user_config: user,
        project: project.map(|(alias, dir)| ProjectScan { alias: alias.to_string(), dir: dir.to_path_buf() }),
        ..Default::default()
    }
}

fn setup() -> (Env, Connection) {
    let env = Env::new();
    let conn = seeded();
    projects::add_reference(&conn, "proje", "Proje", "").unwrap();
    projects::add_reference(&conn, "proje-b", "Proje B", "").unwrap();
    projects::set_preference(&conn, "proje", MCP, CC, platform(), "accepted", None).unwrap();
    (env, conn)
}

/// A wish the guards would refuse (unknown compatibility): inserted directly, TEST ONLY, to look at what the inventory says for it.
fn force_accept(conn: &Connection, alias: &str, entry: &str, surface: &str) {
    conn.execute(
        "INSERT INTO project_preferences (project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint)
         SELECT r.project_id, ?1, ?2, ?3, 'accepted', 'x', 'x', 'x' FROM project_registry r WHERE r.alias = ?4",
        rusqlite::params![entry, surface, platform(), alias],
    )
    .unwrap();
}

fn request<'a>(inv: &'a store::Inventory, entry: &str, alias: &str, surface: &str) -> &'a store::RequestVsObservation {
    inv.requests
        .iter()
        .find(|r| r.entry_id == entry && r.project_alias == alias && r.agent_surface == surface)
        .unwrap_or_else(|| panic!("no request {entry}/{alias}/{surface}"))
}

fn summary(r: &store::RequestVsObservation) -> Vec<(Scope3, Cfg, Fresh)> {
    r.declarations.iter().map(|d| (d.scope, d.configuration, d.freshness)).collect()
}

#[test]
fn real_scan_a_program_on_path_and_a_project_file_without_the_server_are_both_recorded_while_mcp_presence_stays_unknown() {
    let (env, conn) = setup();
    env.fake_exe(MCP);
    let dir = env.project();
    env.write(
        "project/.mcp.json",
        &format!(r#"{{"mcpServers":{{"unrelated-SERVERNAME":{{"command":"node","env":{{"TOKEN":"{SENTINEL}"}}}}}}}}"#),
    );
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, true, Some(("proje", &dir)))).unwrap();
    let inv = store::inventory(&conn).unwrap();
    for o in inv.observations.iter().filter(|o| o.subject == MCP && o.method == MCP_METHOD) {
        assert_eq!(o.presence, "unknown", "an MCP reference never says whether the program exists");
    }
    let r = request(&inv, MCP, "proje", CC);
    assert_eq!(r.status, "present_current", "the legacy headline is unchanged");
    let origin = r.origin.as_ref().unwrap();
    assert_eq!(
        (origin.project_relation, origin.surface_relation, origin.method),
        (ProjectRelation::ComputerWide, SurfaceRelation::LocalCliFallback, Some(OriginMethod::PathExecutable))
    );
    assert_eq!(r.declaration_detector.kind, DetectorKind::ClaudeMcp);
    assert_eq!(
        summary(r),
        vec![
            (Scope3::Project, Cfg::NotConfigured, Fresh::Current),
            (Scope3::Local, Cfg::NotConfigured, Fresh::Current),
            (Scope3::Global, Cfg::NotConfigured, Fresh::Current)
        ]
    );
    let wire = serde_json::to_string(&inv.requests).unwrap();
    for leak in [SENTINEL, "unrelated-SERVERNAME", "mcpServers", "TOKEN", ".mcp.json"] {
        assert!(!wire.contains(leak), "{leak} leaked: {wire}");
    }
    let temp_name = env.root.file_name().unwrap().to_string_lossy().into_owned();
    assert!(!wire.contains(&temp_name));
}

#[test]
fn real_scan_a_missing_program_and_a_project_file_that_names_the_server_are_both_recorded() {
    let (env, conn) = setup();
    let dir = env.project();
    env.write(
        "project/.mcp.json",
        &format!(r#"{{"mcpServers":{{"codebase-memory-mcp":{{"command":"codebase-memory-mcp","env":{{"K":"{SENTINEL2}"}}}}}}}}"#),
    );
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, true, Some(("proje", &dir)))).unwrap();
    let inv = store::inventory(&conn).unwrap();
    let r = request(&inv, MCP, "proje", CC);
    assert_eq!(r.status, "absent_current");
    assert_eq!(summary(r)[0], (Scope3::Project, Cfg::Configured, Fresh::Current));
    let wire = serde_json::to_string(&inv.requests).unwrap();
    assert!(!wire.contains(SENTINEL2) && !wire.contains("codebase-memory-mcp\"command"), "{wire}");
}

#[test]
fn real_scan_an_older_project_scan_stays_previous_while_the_newest_scan_only_covers_the_user_files() {
    let (env, conn) = setup();
    env.fake_exe(MCP);
    let dir = env.project();
    env.write("project/.mcp.json", r#"{"mcpServers":{"codebase-memory-mcp":{"command":"codebase-memory-mcp"}}}"#);
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, true, Some(("proje", &dir)))).unwrap();
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, true, None)).unwrap(); // the newest scan does not look at any project
    let inv = store::inventory(&conn).unwrap();
    let r = request(&inv, MCP, "proje", CC);
    assert_eq!(r.status, "present_current", "unchanged: the newest PATH row is current");
    let s = summary(r);
    assert_eq!(
        s[0],
        (Scope3::Project, Cfg::Configured, Fresh::PreviousAreaNotChecked),
        "an old project scan is old even though it says configured"
    );
    assert_eq!(s[1], (Scope3::Local, Cfg::NotConfigured, Fresh::PreviousAreaNotChecked));
    assert_eq!(s[2], (Scope3::Global, Cfg::NotConfigured, Fresh::Current));
    assert!(r.declarations[0].age_hours.is_some() && r.declarations[0].observed_at.is_some());
    let c = r.declaration_detector.coverage.unwrap();
    assert_eq!((c.project, c.local, c.global), (Cov::Read, Cov::Read, Cov::Read));
}

#[test]
fn real_scan_another_projects_folder_is_never_this_projects_declaration() {
    let (env, conn) = setup();
    force_accept(&conn, "proje-b", MCP, CC);
    env.fake_exe(MCP);
    let dir = env.project();
    env.write("project/.mcp.json", r#"{"mcpServers":{"codebase-memory-mcp":{"command":"codebase-memory-mcp"}}}"#);
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, true, Some(("proje", &dir)))).unwrap();
    let inv = store::inventory(&conn).unwrap();
    assert_eq!(summary(request(&inv, MCP, "proje", CC))[0].1, Cfg::Configured);
    let b = request(&inv, MCP, "proje-b", CC);
    assert_eq!(b.status, "present_current", "the machine-level PATH row still answers the presence question for every project");
    assert_eq!(summary(b), vec![(Scope3::Global, Cfg::NotConfigured, Fresh::Current)], "only the user-level reading applies to project B");
    let c = b.declaration_detector.coverage.unwrap();
    assert_eq!((c.project, c.local, c.global), (Cov::ProjectNotInLatestScan, Cov::ProjectNotInLatestScan, Cov::Read));
}

#[test]
fn real_scan_without_permission_to_read_configuration_has_no_readings_and_says_why() {
    let (env, conn) = setup();
    env.fake_exe(MCP);
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, false, None)).unwrap();
    let inv = store::inventory(&conn).unwrap();
    let r = request(&inv, MCP, "proje", CC);
    assert_eq!(r.status, "present_current");
    assert!(r.declarations.is_empty());
    let c = r.declaration_detector.coverage.unwrap();
    assert_eq!((c.project, c.local, c.global), (Cov::NotAuthorized, Cov::NotAuthorized, Cov::NotAuthorized));
}

#[test]
fn real_scan_unreadable_and_malformed_files_are_reported_as_such_not_as_not_configured() {
    let (env, conn) = setup();
    let dir = env.project();
    env.write("project/.mcp.json", "{ this is not json");
    env.home_write(".claude.json", "[1,2");
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, true, Some(("proje", &dir)))).unwrap();
    let inv = store::inventory(&conn).unwrap();
    let s = summary(request(&inv, MCP, "proje", CC));
    assert_eq!(s[0], (Scope3::Project, Cfg::Malformed, Fresh::Current));
    assert_eq!(s[2], (Scope3::Global, Cfg::Malformed, Fresh::Current));
    assert!(s.iter().all(|(_, c, _)| *c != Cfg::NotConfigured), "{s:?}");
}

#[test]
fn real_scan_codex_and_other_hosts_get_no_declaration_and_a_machine_lookup_is_labelled_a_fallback() {
    let (env, conn) = setup();
    for surface in ["codex-cli", "antigravity-ide", "claude-desktop-code", "local-cli"] {
        force_accept(&conn, "proje", MCP, surface);
    }
    force_accept(&conn, "proje", "gitleaks", "codex-cli");
    env.fake_exe(MCP);
    env.fake_exe("gitleaks");
    let dir = env.project();
    env.write("project/.mcp.json", r#"{"mcpServers":{"codebase-memory-mcp":{"command":"codebase-memory-mcp"}}}"#);
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, true, Some(("proje", &dir)))).unwrap();
    let inv = store::inventory(&conn).unwrap();
    for surface in ["codex-cli", "antigravity-ide", "claude-desktop-code", "local-cli"] {
        let r = request(&inv, MCP, "proje", surface);
        assert_eq!(r.declaration_detector.kind, DetectorKind::NotImplemented, "{surface}");
        assert!(
            r.declaration_detector.coverage.is_none() && r.declarations.is_empty(),
            "{surface}: the Claude project file must not be presented as this host's declaration"
        );
    }
    let codex = request(&inv, MCP, "proje", "codex-cli");
    assert_eq!(codex.status, "present_current");
    assert_eq!(codex.origin.as_ref().unwrap().surface_relation, SurfaceRelation::LocalCliFallback);
    let g = request(&inv, "gitleaks", "proje", "codex-cli");
    assert_eq!((g.declaration_detector.kind, g.declarations.len()), (DetectorKind::NotImplemented, 0));
    // the exact Claude request is the only one with a reading
    assert_eq!(request(&inv, MCP, "proje", CC).declarations.len(), 3);
}

#[test]
fn real_scan_nothing_is_written_by_building_the_view_and_the_legacy_columns_are_what_they_were() {
    let (env, conn) = setup();
    env.fake_exe(MCP);
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, true, None)).unwrap();
    let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    let before = (
        count("SELECT count(*) FROM discovery_observations"),
        count("SELECT count(*) FROM discovery_scans"),
        count("SELECT count(*) FROM audit_events"),
        count("PRAGMA user_version"),
    );
    for _ in 0..3 {
        store::inventory(&conn).unwrap();
    }
    let after = (
        count("SELECT count(*) FROM discovery_observations"),
        count("SELECT count(*) FROM discovery_scans"),
        count("SELECT count(*) FROM audit_events"),
        count("PRAGMA user_version"),
    );
    assert_eq!(before, after, "reading the inventory writes nothing");
    assert_eq!(after.3, crate::db::LATEST_VERSION, "no migration was added");
}
