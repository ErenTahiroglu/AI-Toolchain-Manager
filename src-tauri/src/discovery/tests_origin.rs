//! PRODUCT-06: the evidence-origin descriptor attached to a request-vs-observation result.
//! It describes the ONE observation `classify_request` selected (project-specific or computer-wide; exact surface or `local-cli` fallback;
//! its own timestamp and age) and must never change which observation is selected or which status comes out.

use super::*;
use crate::discovery::store::{
    classify_request, EvidenceOrigin, OriginMethod, ProjectRelation, RequestVsObservation, StoredObservation, SurfaceRelation,
};

const SURFACE: &str = "claude-code-terminal";
const OTHER_SURFACE: &str = "antigravity-ide";
const STAMP: &str = "2026-10-17T00:00:00Z";

/// A synthetic stored observation. Only the fields `classify_request` reads matter; the rest carry sentinels so a leak would be visible.
fn ob(id: i64, alias: Option<&str>, surface: &str, platform: &str, presence: &str, current: bool) -> StoredObservation {
    StoredObservation {
        id,
        scan_id: format!("scan-{id}"),
        subject: "x".into(),
        platform_target: platform.into(),
        agent_surface: surface.into(),
        method: "path-executable".into(),
        presence: presence.into(),
        configuration: "not_applicable".into(),
        version: Some("9.9.9-SENTINEL".into()),
        version_source: Some("SENTINEL-VERSION-SOURCE".into()),
        scope: if alias.is_some() { "project" } else { "global" }.into(),
        project_alias: alias.map(str::to_string),
        confidence: "low".into(),
        evidence: "SENTINEL-EVIDENCE-TEXT".into(),
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
        scan_authorized_user_config: false,
        scan_authorized_project: false,
        scan_authorized_fingerprint: false,
        provenance: if current { "latest_scan" } else { "earlier_scan" }.into(),
        latest_scan_checked: current,
        current,
        not_current_reason: if current { None } else { Some("eski".into()) },
        ladder: crate::discovery::types::Provenance::default(),
    }
}

fn origin(project: ProjectRelation, surface: SurfaceRelation) -> EvidenceOrigin {
    // `ob()` rows are PATH lookups, so the winner's technique is `path-executable` (PRODUCT-07 H added `method`; the four older members are unchanged)
    EvidenceOrigin {
        project_relation: project,
        surface_relation: surface,
        observed_at: Some(STAMP.into()),
        age_hours: Some(5),
        method: Some(OriginMethod::PathExecutable),
    }
}

fn classify(obs: &[StoredObservation]) -> (String, String, Option<store::ScanRef>, Option<EvidenceOrigin>) {
    classify_request("x", "p", SURFACE, PLATFORM_WINDOWS, obs)
}

// ---- 1 + 2: the four combinations ---------------------------------------------------------------------------------------------------------

#[test]
fn each_combination_of_project_and_surface_is_described_from_the_selected_row() {
    use ProjectRelation::*;
    use SurfaceRelation::*;
    let cases = [
        (Some("p"), SURFACE, ProjectSpecific, ExactSurface),
        (Some("p"), "local-cli", ProjectSpecific, LocalCliFallback),
        (None, SURFACE, ComputerWide, ExactSurface),
        (None, "local-cli", ComputerWide, LocalCliFallback),
    ];
    for (alias, surface, project, surf) in cases {
        let (status, _, basis, got) = classify(&[ob(1, alias, surface, PLATFORM_WINDOWS, "present", true)]);
        assert_eq!(status, "present_current", "{alias:?}/{surface}");
        assert_eq!(basis.unwrap().scan_id, "scan-1");
        assert_eq!(got, Some(origin(project, surf)), "{alias:?}/{surface}");
    }
}

#[test]
fn a_local_cli_row_for_a_local_cli_request_is_the_exact_surface() {
    let (_, _, _, got) =
        classify_request("x", "p", "local-cli", PLATFORM_WINDOWS, &[ob(1, None, "local-cli", PLATFORM_WINDOWS, "present", true)]);
    assert_eq!(got.unwrap().surface_relation, SurfaceRelation::ExactSurface, "nothing is a fallback when the request itself is local-cli");
}

// ---- 3: several candidates - the metadata describes the winner ----------------------------------------------------------------------------

#[test]
fn the_metadata_describes_the_winning_observation_not_a_similar_loser() {
    let w = PLATFORM_WINDOWS;
    // rank beats specificity: a project-specific `absent` loses to a computer-wide `present`
    let (status, _, basis, got) = classify(&[ob(1, Some("p"), SURFACE, w, "absent", true), ob(2, None, "local-cli", w, "present", true)]);
    assert_eq!(status, "present_current");
    assert_eq!(basis.unwrap().scan_id, "scan-2");
    assert_eq!(got, Some(origin(ProjectRelation::ComputerWide, SurfaceRelation::LocalCliFallback)));

    // equal rank: the higher id wins, whichever kind it is
    let (_, _, basis, got) = classify(&[ob(4, Some("p"), SURFACE, w, "present", true), ob(9, None, SURFACE, w, "present", true)]);
    assert_eq!(basis.unwrap().scan_id, "scan-9");
    assert_eq!(got.unwrap().project_relation, ProjectRelation::ComputerWide);
    let (_, _, basis, got) = classify(&[ob(9, Some("p"), SURFACE, w, "present", true), ob(4, None, SURFACE, w, "present", true)]);
    assert_eq!(basis.unwrap().scan_id, "scan-9");
    assert_eq!(got.unwrap().project_relation, ProjectRelation::ProjectSpecific);
    let (_, _, basis, got) = classify(&[ob(3, None, SURFACE, w, "present", true), ob(8, None, "local-cli", w, "present", true)]);
    assert_eq!(basis.unwrap().scan_id, "scan-8");
    assert_eq!(got.unwrap().surface_relation, SurfaceRelation::LocalCliFallback);
    let (_, _, basis, got) = classify(&[ob(8, None, SURFACE, w, "present", true), ob(3, None, "local-cli", w, "present", true)]);
    assert_eq!(basis.unwrap().scan_id, "scan-8");
    assert_eq!(got.unwrap().surface_relation, SurfaceRelation::ExactSurface);

    // a current answer beats an old one even if the old one is project-specific and says `present`
    let (status, _, basis, got) = classify(&[ob(7, Some("p"), SURFACE, w, "present", false), ob(1, None, SURFACE, w, "absent", true)]);
    assert_eq!(status, "absent_current");
    assert_eq!(basis.unwrap().scan_id, "scan-1");
    assert_eq!(got.unwrap().project_relation, ProjectRelation::ComputerWide);

    // another platform's row, another project's row and another surface's row never take part
    let (_, _, basis, got) = classify(&[
        ob(1, None, SURFACE, w, "unknown", true),
        ob(50, None, SURFACE, PLATFORM_MACOS, "present", true),
        ob(51, Some("other"), SURFACE, w, "present", true),
        ob(52, None, OTHER_SURFACE, w, "present", true),
    ]);
    assert_eq!(basis.unwrap().scan_id, "scan-1");
    assert_eq!(got, Some(origin(ProjectRelation::ComputerWide, SurfaceRelation::ExactSurface)));
}

// ---- 4: no selected observation, no origin ------------------------------------------------------------------------------------------------

#[test]
fn not_scanned_and_platform_mismatch_carry_no_origin() {
    let (status, _, basis, got) = classify(&[]);
    assert_eq!((status.as_str(), basis.is_none(), got), ("not_scanned", true, None));
    // rows that exist only for another project / another surface are not a basis either
    let (status, _, basis, got) = classify(&[
        ob(1, Some("other"), SURFACE, PLATFORM_WINDOWS, "present", true),
        ob(2, None, OTHER_SURFACE, PLATFORM_WINDOWS, "present", true),
    ]);
    assert_eq!((status.as_str(), basis.is_none(), got), ("not_scanned", true, None));
    let (status, _, basis, got) =
        classify(&[ob(1, None, SURFACE, PLATFORM_MACOS, "present", true), ob(2, Some("p"), "local-cli", PLATFORM_MACOS, "absent", true)]);
    assert_eq!((status.as_str(), basis.is_none(), got), ("platform_mismatch", true, None));
}

// ---- 5: historical vs current, timestamp and age ------------------------------------------------------------------------------------------

#[test]
fn a_historical_row_keeps_its_status_and_describes_itself_with_its_own_time() {
    let mut old = ob(3, Some("p"), SURFACE, PLATFORM_WINDOWS, "present", false);
    old.observed_at = "2026-10-01T12:30:00+02:00".into();
    old.age_hours = 400;
    let (status, note, basis, got) = classify(&[old]);
    assert_eq!(status, "previously_present", "metadata does not make an old result current");
    assert!(note.contains("dayanak sayılmaz"), "{note}");
    assert_eq!(basis.unwrap().scan_id, "scan-3");
    assert_eq!(
        got,
        Some(EvidenceOrigin {
            project_relation: ProjectRelation::ProjectSpecific,
            surface_relation: SurfaceRelation::ExactSurface,
            observed_at: Some("2026-10-01T12:30:00+02:00".into()), // as stored: not converted, not rewritten
            age_hours: Some(400),
            method: Some(OriginMethod::PathExecutable),
        })
    );
}

#[test]
fn a_malformed_or_future_timestamp_gives_no_age_and_never_a_made_up_one() {
    for (stamp, age, want_at, want_age) in [
        ("yesterday", i64::from(i32::MAX), None, None),
        ("", i64::from(i32::MAX), None, None),
        ("2026-10-17", 3, None, None),          // a date without a time is not RFC 3339
        ("2026-10-17T00:00:00", 3, None, None), // no offset
        ("2999-01-01T00:00:00Z", -9000, Some("2999-01-01T00:00:00Z"), None), // valid but in the future: negative age is not an age
        ("2026-10-17T00:00:00Z", 0, Some("2026-10-17T00:00:00Z"), Some(0)),
    ] {
        let mut o = ob(1, None, SURFACE, PLATFORM_WINDOWS, "present", true);
        o.observed_at = stamp.into();
        o.age_hours = age;
        let (status, _, basis, got) = classify(&[o]);
        assert_eq!(status, "present_current", "a bad timestamp does not change the status ({stamp:?})");
        assert!(basis.is_some());
        let got = got.expect("the observation was selected, so the origin exists");
        assert_eq!((got.observed_at.as_deref(), got.age_hours), (want_at, want_age), "{stamp:?}");
        assert_eq!(got.project_relation, ProjectRelation::ComputerWide);
    }
}

#[test]
fn the_time_and_age_belong_to_the_winner() {
    let w = PLATFORM_WINDOWS;
    let mut loser = ob(1, None, SURFACE, w, "present", true);
    loser.observed_at = "2026-10-10T00:00:00Z".into();
    loser.age_hours = 200;
    let mut winner = ob(2, None, SURFACE, w, "present", true);
    winner.observed_at = "2026-10-16T00:00:00Z".into();
    winner.age_hours = 7;
    let (_, _, _, got) = classify(&[loser, winner]);
    let got = got.unwrap();
    assert_eq!((got.observed_at.as_deref(), got.age_hours), (Some("2026-10-16T00:00:00Z"), Some(7)));
}

// ---- 6: serialization ---------------------------------------------------------------------------------------------------------------------

fn request_json(obs: &[StoredObservation]) -> serde_json::Value {
    let (status, note, basis, origin) = classify(obs);
    let scoped = crate::discovery::declarations::scoped_declarations("x", "p", SURFACE, PLATFORM_WINDOWS, obs, None);
    serde_json::to_value(RequestVsObservation {
        entry_id: "x".into(),
        entry_name: "X".into(),
        project_alias: "p".into(),
        project_name: "P".into(),
        agent_surface: SURFACE.into(),
        platform_target: PLATFORM_WINDOWS.into(),
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
fn the_wire_names_and_the_additive_shape_are_exactly_these() {
    let v = request_json(&[ob(1, Some("p"), "local-cli", PLATFORM_WINDOWS, "present", true)]);
    let keys: std::collections::BTreeSet<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    let before = ["agentSurface", "basis", "entryId", "entryName", "note", "platformTarget", "projectAlias", "projectName", "status"];
    // PRODUCT-06 added `origin`; PRODUCT-07 H adds `declarationDetector` and `declarations` (additive; nothing earlier changes)
    let expected: std::collections::BTreeSet<&str> =
        before.iter().copied().chain(["origin", "declarationDetector", "declarations"]).collect();
    assert_eq!(keys, expected, "the earlier members are unchanged and only the documented ones are added");
    // the four PRODUCT-06 members keep their values; `method` is the one additive nested member (so `origin` as a whole is NOT byte-identical to before)
    assert_eq!(
        v["origin"],
        serde_json::json!({ "projectRelation": "project_specific", "surfaceRelation": "local_cli_fallback", "observedAt": STAMP, "ageHours": 5, "method": "path-executable" })
    );
    let other = request_json(&[ob(1, None, SURFACE, PLATFORM_WINDOWS, "present", true)]);
    assert_eq!(other["origin"]["projectRelation"], "computer_wide");
    assert_eq!(other["origin"]["surfaceRelation"], "exact_surface");
    // unavailable provenance is an explicit null, not a missing member and not a guess
    for none in [request_json(&[]), request_json(&[ob(1, None, SURFACE, PLATFORM_MACOS, "present", true)])] {
        assert!(none.as_object().unwrap().contains_key("origin"));
        assert!(none["origin"].is_null());
        assert!(none["basis"].is_null());
    }
    // an unusable timestamp is null as well
    let mut bad = ob(1, None, SURFACE, PLATFORM_WINDOWS, "present", true);
    bad.observed_at = "nope".into();
    bad.age_hours = i64::from(i32::MAX);
    let v = request_json(&[bad]);
    assert!(v["origin"]["observedAt"].is_null() && v["origin"]["ageHours"].is_null());
}

#[test]
fn nothing_from_the_row_beyond_the_four_members_reaches_the_response() {
    let text = request_json(&[ob(1, Some("p"), SURFACE, PLATFORM_WINDOWS, "present", true)]).to_string();
    for secret in ["SENTINEL", "secret", "C:\\\\Users", "tool.exe", "9.9.9", "fingerprint"] {
        assert!(!text.contains(secret), "{secret} leaked into {text}");
    }
    // the technique name appears exactly once: as the closed `origin.method` value, nowhere else
    assert_eq!(text.matches("path-executable").count(), 1, "{text}");
    assert!(text.contains("\"method\":\"path-executable\""), "{text}");
    let origin_keys: Vec<String> = serde_json::to_value(origin(ProjectRelation::ComputerWide, SurfaceRelation::ExactSurface))
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(origin_keys.len(), 5, "the four PRODUCT-06 members plus the additive `method`: {origin_keys:?}");
}

// ---- the selection and the statuses are exactly what they were ---------------------------------------------------------------------------

/// The selection code of PRODUCT-05 (before the origin existed), copied verbatim, to prove the new function selects the same row and says the same.
fn legacy(
    entry_id: &str,
    project_alias: &str,
    surface: &str,
    platform: &str,
    observations: &[StoredObservation],
) -> (String, String, Option<String>) {
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
            )
        } else {
            (
                "platform_mismatch".into(),
                "Yalnızca başka bir işletim sistemi/mimari için gözlem var; bu tercihin platformu için hiçbir şey gözlenmedi.".into(),
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
        return (format!("{}_current", word(r)), note.into(), Some(o.scan_id.clone()));
    }
    let o = best(false).expect("same_platform is not empty and none is current");
    let r = rank(&o.presence);
    let why = o.not_current_reason.clone().unwrap_or_else(|| "Güncel değil.".to_string());
    (
        format!("previously_{}", word(r)),
        format!("Eski sonuç ({why}) Yeniden tarayın; eski bir sonuç hiçbir işlem için dayanak sayılmaz."),
        Some(o.scan_id.clone()),
    )
}

#[test]
fn selection_status_note_and_basis_are_identical_to_the_version_without_an_origin() {
    // a small deterministic generator: no randomness crate, no clock
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move |n: u64| {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (state >> 33) % n
    };
    let (mut selected, mut empty, mut mismatch) = (0, 0, 0);
    for round in 0..4000 {
        let count = next(7) as usize;
        let mut obs = Vec::new();
        for i in 0..count {
            let alias = [None, Some("p"), Some("q")][next(3) as usize];
            let surface = [SURFACE, "local-cli", OTHER_SURFACE][next(3) as usize];
            let platform = [PLATFORM_WINDOWS, PLATFORM_WINDOWS, PLATFORM_MACOS][next(3) as usize];
            let presence = ["present", "absent", "unknown", "unsupported"][next(4) as usize];
            let mut o = ob(1 + (i as i64) * 7 + next(7) as i64, alias, surface, platform, presence, next(2) == 0);
            o.observed_at = ["2026-10-17T00:00:00Z", "2026-09-01T08:00:00+03:00", "garbage"][next(3) as usize].into();
            o.age_hours = [0, 40, i64::from(i32::MAX)][next(3) as usize];
            if obs.iter().any(|x: &StoredObservation| x.id == o.id) {
                o.id += 1000 + i as i64; // keep ids unique so every row can be identified by its scan id
                o.scan_id = format!("scan-{}", o.id);
            }
            obs.push(o);
        }
        for requested in [SURFACE, "local-cli"] {
            let (status, note, basis, got) = classify_request("x", "p", requested, PLATFORM_WINDOWS, &obs);
            let (l_status, l_note, l_scan) = legacy("x", "p", requested, PLATFORM_WINDOWS, &obs);
            assert_eq!((&status, &note), (&l_status, &l_note), "round {round}");
            assert_eq!(basis.as_ref().map(|b| b.scan_id.clone()), l_scan, "round {round}: a different row was selected");
            match (&basis, &got) {
                (None, None) => {
                    if status == "not_scanned" {
                        empty += 1
                    } else {
                        assert_eq!(status, "platform_mismatch");
                        mismatch += 1
                    }
                }
                (Some(b), Some(origin)) => {
                    selected += 1;
                    let winner = obs.iter().find(|o| o.scan_id == b.scan_id).unwrap();
                    assert_eq!(
                        origin.project_relation == ProjectRelation::ProjectSpecific,
                        winner.project_alias.is_some(),
                        "round {round}"
                    );
                    assert_eq!(
                        origin.surface_relation == SurfaceRelation::ExactSurface,
                        winner.agent_surface == requested,
                        "round {round}"
                    );
                    let valid = chrono::DateTime::parse_from_rfc3339(&winner.observed_at).is_ok();
                    assert_eq!(origin.observed_at.is_some(), valid, "round {round}");
                    assert_eq!(origin.age_hours.is_some(), valid, "round {round}");
                    if valid {
                        assert_eq!(
                            (origin.observed_at.as_deref(), origin.age_hours),
                            (Some(winner.observed_at.as_str()), Some(winner.age_hours))
                        );
                    }
                }
                other => panic!("round {round}: basis and origin must exist together, got {other:?}"),
            }
        }
    }
    assert!(selected > 500 && empty > 100 && mismatch > 20, "the generator must reach every outcome ({selected}/{empty}/{mismatch})");
}

// ---- end to end through the real store (synthetic temp environment, as the neighbouring history tests) --------------------------------------

#[test]
fn a_stored_lookup_is_reported_computer_wide_for_every_project_that_asks() {
    let env = Env::new();
    let conn = seeded();
    projects::add_reference(&conn, "proje", "Proje", "").unwrap();
    projects::add_reference(&conn, "proje-b", "Proje B", "").unwrap();
    projects::set_preference(&conn, "proje", "agent-browser", "local-cli", platform(), "accepted", None).unwrap();
    projects::set_preference(&conn, "proje", FD, "claude-code-terminal", platform(), "accepted", None).unwrap();
    projects::set_preference(&conn, "proje-b", FD, "claude-code-terminal", platform(), "accepted", None).unwrap();
    let before = store::inventory(&conn).unwrap();
    assert!(
        before.requests.iter().all(|r| r.status == "not_scanned" && r.origin.is_none() && r.basis.is_none()),
        "nothing scanned, nothing described"
    );

    env.fake_exe("agent-browser");
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", "1.0.0")]));
    cache(&env, "claude-plugins-official", "frontend-design", "1.0.0");
    crate::discovery::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    let inv = store::inventory(&conn).unwrap();
    let find = |entry: &str, alias: &str| inv.requests.iter().find(|r| r.entry_id == entry && r.project_alias == alias).unwrap();

    for (entry, alias) in [("agent-browser", "proje"), (FD, "proje"), (FD, "proje-b")] {
        let r = find(entry, alias);
        assert_eq!(r.status, "present_current", "{entry}/{alias}");
        let o = r.origin.as_ref().unwrap_or_else(|| panic!("{entry}/{alias}: selected, so described"));
        assert_eq!(
            (o.project_relation, o.surface_relation),
            (ProjectRelation::ComputerWide, SurfaceRelation::ExactSurface),
            "{entry}/{alias}: a user-level or PATH finding was recorded without a project alias"
        );
        assert!(o.observed_at.is_some() && o.age_hours == Some(0), "{entry}/{alias}: just scanned");
    }

    // what goes to the screen contains no path of the temporary environment and no raw evidence
    let wire = serde_json::to_string(&inv.requests).unwrap();
    // (the unique name of the temporary directory is a component of every path the scan could have seen)
    let temp_name = env.root.file_name().unwrap().to_string_lossy().into_owned();
    assert!(temp_name.len() > 3 && !wire.contains(&temp_name), "{temp_name} in {wire}");
    assert!(!wire.contains("evidence") && !wire.contains("location"), "{wire}");
}

// ---- the descriptor code itself is pure ---------------------------------------------------------------------------------------------------

#[test]
fn the_origin_code_has_no_io_process_network_or_write() {
    let text = include_str!("store.rs");
    let start = text.find("pub enum ProjectRelation").expect("ProjectRelation exists");
    let end = text.find("fn requests(").expect("requests exists");
    // from the origin types through classify_request: types, origin_of, selection - all pure functions over slices
    let section = &text[start..end];
    for forbidden in ["std::process", "Command::", "std::fs", "std::net", "TcpStream", "reqwest", ".spawn(", "File::"] {
        assert!(!section.contains(forbidden), "the origin/selection code must stay pure, found {forbidden}");
    }
    let origin_fn = &text[text.find("fn origin_of").unwrap()..text.find("/// Pure classification").unwrap()];
    for forbidden in ["conn", "Connection", "execute", "query", "INSERT", "UPDATE"] {
        assert!(!origin_fn.contains(forbidden), "origin_of must not touch the database ({forbidden})");
    }
}

// ---- P06-RT-01: an observation from the future never gets an age -------------------------------------------------------------------------
// These go through the PRODUCTION age computation (store::latest_observations -> age_hours -> origin_of -> serialization): a real scan is stored, its
// `observed_at` is then moved to a chosen instant relative to the real clock, and the inventory is read again. The margins (15 min, 30 min, 49 h)
// are large against the few seconds a test takes, so there is no sub-second clock edge.

fn stamp_at(offset: chrono::Duration) -> String {
    (chrono::Utc::now() + offset).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// One accepted `local-cli` preference, one real scan of a stand-in program, then every observation's `observed_at` set to `stamp`.
fn inventory_with_stamp(stamp: &str) -> store::Inventory {
    let env = Env::new();
    let conn = seeded();
    projects::add_reference(&conn, "proje", "Proje", "").unwrap();
    projects::set_preference(&conn, "proje", "agent-browser", "local-cli", platform(), "accepted", None).unwrap();
    env.fake_exe("agent-browser");
    let auth = Authorization { path_executables: true, ..Default::default() };
    crate::discovery::run_scan(&conn, &env.roots(), &auth).unwrap();
    conn.execute("UPDATE discovery_observations SET observed_at = ?1", [stamp]).unwrap();
    store::inventory(&conn).unwrap()
}

fn agent_browser(inv: &store::Inventory) -> (&RequestVsObservation, serde_json::Value) {
    let r = inv.requests.iter().find(|r| r.entry_id == "agent-browser").expect("the request exists");
    (r, serde_json::to_value(r).unwrap())
}

#[test]
fn an_observation_from_the_near_future_has_no_age_through_the_real_computation() {
    for minutes in [15, 30] {
        let stamp = stamp_at(chrono::Duration::minutes(minutes));
        let inv = inventory_with_stamp(&stamp);
        let (r, wire) = agent_browser(&inv);
        let o = r.origin.as_ref().expect("an observation was selected");
        assert_eq!(o.observed_at.as_deref(), Some(stamp.as_str()), "{minutes} min ahead: the stamp is valid and shown as stored");
        assert_eq!(o.age_hours, None, "{minutes} min ahead: it must not say '0 hours ago'");
        assert!(wire["origin"]["ageHours"].is_null(), "{minutes} min ahead: serialized as null, got {}", wire["origin"]);
        assert_eq!(wire["origin"]["observedAt"], stamp.as_str());
        // the stored row itself must not claim an age of zero either
        assert!(inv.observations.iter().all(|s| s.age_hours < 0), "{:?}", inv.observations.iter().map(|s| s.age_hours).collect::<Vec<_>>());
        // and nothing else about the answer changed: a not-yet-stale observation is still the current one
        assert_eq!(r.status, "present_current", "{minutes} min ahead");
        assert!(r.basis.is_some());
    }
}

#[test]
fn past_observations_keep_their_whole_hour_age_and_invalid_ones_none() {
    // (offset, expected age, expected status)
    for (offset, age, status) in [
        (chrono::Duration::minutes(-30), Some(0), "present_current"),
        (chrono::Duration::hours(-2), Some(2), "present_current"),
        (chrono::Duration::hours(-49), Some(49), "previously_present"),
        (chrono::Duration::hours(-50) - chrono::Duration::minutes(30), Some(50), "previously_present"),
    ] {
        let stamp = stamp_at(offset);
        let inv = inventory_with_stamp(&stamp);
        let (r, wire) = agent_browser(&inv);
        let o = r.origin.as_ref().unwrap();
        assert_eq!((o.observed_at.as_deref(), o.age_hours), (Some(stamp.as_str()), age), "{offset:?}");
        assert_eq!(r.status, status, "{offset:?}");
        assert_eq!(wire["origin"]["ageHours"], serde_json::json!(age), "{offset:?}");
    }
    for bad in ["garbage", "", "2026-10-24", "2026-10-24T09:00:00"] {
        let inv = inventory_with_stamp(bad);
        let (r, wire) = agent_browser(&inv);
        let o = r.origin.as_ref().expect("still described");
        assert_eq!((o.observed_at.as_deref(), o.age_hours), (None, None), "{bad:?}");
        assert!(wire["origin"]["observedAt"].is_null() && wire["origin"]["ageHours"].is_null(), "{bad:?}");
        assert_eq!(r.status, "previously_present", "an unreadable stamp has always counted as not current: {bad:?}");
    }
}

// ---- the same rule, pure and deterministic (an injected reference instant: no clock at all) -----------------------------------------------

fn at(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&chrono::Utc)
}

#[test]
fn age_at_a_given_instant_orders_before_it_truncates() {
    use crate::discovery::store::age_hours_at;
    let now = at("2026-10-24T12:00:00Z");
    for (stamp, want) in [
        ("2026-10-24T12:00:00Z", 0), // equal: accepted, age 0
        ("2026-10-24T11:59:59Z", 0), // past, under an hour
        ("2026-10-24T11:30:00Z", 0),
        ("2026-10-24T11:00:00Z", 1), // exactly one hour ago
        ("2026-10-24T10:59:59Z", 1),
        ("2026-10-22T11:00:00Z", 49),     // an older past case
        ("2026-10-24T14:00:00+02:00", 0), // the same instant written with an offset
        ("2026-10-24T12:00:01Z", -1),     // one second ahead
        ("2026-10-24T12:15:00Z", -1),     // 15 minutes ahead - used to read as 0
        ("2026-10-24T12:30:00Z", -1),     // 30 minutes ahead - used to read as 0
        ("2026-10-24T12:59:59Z", -1),
        ("2026-10-24T13:00:00Z", -1),      // exactly an hour ahead
        ("2026-10-24T14:30:00Z", -2),      // further ahead keeps its (already negative) value
        ("2026-10-24T14:30:00+02:00", -1), // an offset does not hide that it is ahead
        ("2999-01-01T00:00:00Z", -(at("2999-01-01T00:00:00Z") - now).num_hours()),
    ] {
        assert_eq!(age_hours_at(stamp, now), want, "{stamp}");
    }
    for bad in ["", "garbage", "2026-10-24", "2026-10-24T11:00:00", "yesterday"] {
        assert_eq!(age_hours_at(bad, now), i64::MAX, "{bad:?} stays 'unreadable'");
    }
}

#[test]
fn only_the_sub_hour_future_case_differs_from_the_previous_computation() {
    use crate::discovery::store::age_hours_at;
    let now = at("2026-10-24T12:00:00Z");
    let previous =
        |s: &str| chrono::DateTime::parse_from_rfc3339(s).map(|t| (now - t.with_timezone(&chrono::Utc)).num_hours()).unwrap_or(i64::MAX);
    let base = at("2026-10-20T00:00:00Z");
    let mut seen_changed = 0;
    for step in 0..(8 * 24 * 60 / 7) {
        let stamp = (base + chrono::Duration::minutes(step * 7)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let (before, after) = (previous(&stamp), age_hours_at(&stamp, now));
        let future_under_an_hour = at(&stamp) > now && at(&stamp) - now < chrono::Duration::hours(1);
        if future_under_an_hour {
            assert_eq!((before, after), (0, -1), "{stamp}");
            seen_changed += 1;
        } else {
            assert_eq!(before, after, "{stamp}: nothing but the sub-hour future may change");
        }
    }
    assert!(seen_changed > 5, "the sweep reaches the case it is about ({seen_changed})");
}

#[test]
fn the_winner_decides_whether_an_age_is_shown_not_a_competing_row() {
    use crate::discovery::store::age_hours_at;
    let now = at("2026-10-24T12:00:00Z");
    let w = PLATFORM_WINDOWS;
    let with_stamp = |id: i64, alias: Option<&str>, stamp: &str| {
        let mut o = ob(id, alias, SURFACE, w, "present", true);
        o.observed_at = stamp.into();
        o.age_hours = age_hours_at(stamp, now); // the production computation, at a fixed instant
        o
    };
    let future = "2026-10-24T12:30:00Z";
    let past = "2026-10-24T09:00:00Z";

    // computer-wide winner (higher id) from the future; the project-specific loser is a normal past row
    let (status, _, basis, got) = classify(&[with_stamp(4, Some("p"), past), with_stamp(9, None, future)]);
    let got = got.unwrap();
    assert_eq!((status.as_str(), basis.unwrap().scan_id.as_str()), ("present_current", "scan-9"));
    assert_eq!((got.project_relation, got.observed_at.as_deref(), got.age_hours), (ProjectRelation::ComputerWide, Some(future), None));

    // project-specific winner from the future; the computer-wide loser is a past row with a real age
    let (_, _, basis, got) = classify(&[with_stamp(9, Some("p"), future), with_stamp(4, None, past)]);
    let got = got.unwrap();
    assert_eq!(basis.unwrap().scan_id, "scan-9");
    assert_eq!((got.project_relation, got.observed_at.as_deref(), got.age_hours), (ProjectRelation::ProjectSpecific, Some(future), None));

    // and the other way round: a past winner is described by its own age, a future loser leaks nothing into it
    let (_, _, basis, got) = classify(&[with_stamp(9, Some("p"), past), with_stamp(4, None, future)]);
    let got = got.unwrap();
    assert_eq!(basis.unwrap().scan_id, "scan-9");
    assert_eq!((got.observed_at.as_deref(), got.age_hours), (Some(past), Some(3)));

    // a historical winner (not current) keeps its status and its own age
    let mut old = with_stamp(2, None, "2026-10-22T11:00:00Z");
    old.current = false;
    old.not_current_reason = Some("eski".into());
    let (status, _, _, got) = classify(&[old]);
    assert_eq!(status, "previously_present");
    assert_eq!(got.unwrap().age_hours, Some(49));

    // status, note and selected scan do not depend on the stamp at all: only the origin's age differs
    let same = |stamp: &str| {
        let (s, n, b, _) = classify(&[with_stamp(1, None, stamp)]);
        (s, n, b.map(|b| b.scan_id))
    };
    assert_eq!(same(future), same(past));
}
