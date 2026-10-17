//! RT-2A-01: history is kept, but only an observation from the latest, fresh scan on this platform describes today.
//! Earlier results are shown as earlier results, with the reason, and never stand in for a current answer.

use super::*;
use crate::discovery::store::{classify_request, StoredObservation};

fn other_platform() -> &'static str {
    if cfg!(windows) {
        PLATFORM_MACOS
    } else {
        PLATFORM_WINDOWS
    }
}

fn auth(path: bool, user: bool, project: Option<(&str, &Path)>) -> Authorization {
    Authorization {
        path_executables: path,
        user_config: user,
        project: project.map(|(alias, dir)| ProjectScan { alias: alias.to_string(), dir: dir.to_path_buf() }),
    }
}

fn setup() -> (Env, Connection) {
    let env = Env::new();
    let conn = seeded();
    projects::add_reference(&conn, "proje", "Proje", "").unwrap();
    projects::add_reference(&conn, "proje-b", "Proje B", "").unwrap();
    projects::set_preference(&conn, "proje", "gitleaks", "local-cli", platform(), "accepted", None).unwrap();
    (env, conn)
}

fn request<'a>(inv: &'a store::Inventory, entry: &str, alias: &str) -> &'a store::RequestVsObservation {
    inv.requests.iter().find(|r| r.entry_id == entry && r.project_alias == alias).unwrap_or_else(|| panic!("no request {entry}/{alias}"))
}

fn gitleaks_path(inv: &store::Inventory) -> &StoredObservation {
    inv.observations.iter().find(|o| o.subject == "gitleaks" && o.method == "path-executable").unwrap()
}

#[test]
fn a_path_result_stops_being_current_when_the_next_scan_did_not_look_at_path() {
    let (env, conn) = setup();
    env.fake_exe("gitleaks");
    let first = crate::discovery::run_scan(&conn, &env.roots(), &auth(true, true, None)).unwrap();
    let inv = store::inventory(&conn).unwrap();
    assert!(gitleaks_path(&inv).current);
    assert_eq!(request(&inv, "gitleaks", "proje").status, "present_current");

    // The user now scans with fewer permissions: PATH is not looked at.
    let second = crate::discovery::run_scan(&conn, &env.roots(), &auth(false, true, None)).unwrap();
    let inv = store::inventory(&conn).unwrap();
    let g = gitleaks_path(&inv);
    assert_eq!(g.scan_id, first.scan_id, "the old row is kept as history");
    assert_eq!(g.provenance, "earlier_scan");
    assert!(!g.current && !g.latest_scan_checked);
    assert!(g.not_current_reason.as_deref().unwrap().contains("denetlenmedi"), "{:?}", g.not_current_reason);
    assert!(
        g.scan_authorized_path && g.scan_authorized_user_config && !g.scan_authorized_project,
        "permissions of the scan that produced it"
    );
    let r = request(&inv, "gitleaks", "proje");
    assert_eq!(r.status, "previously_present", "an old PATH hit must not read as present now");
    assert!(r.note.contains("dayanak sayılmaz"), "{}", r.note);
    assert_eq!(r.basis.as_ref().unwrap().scan_id, first.scan_id);
    assert_eq!(inv.last_scan.as_ref().unwrap().scan_id, second.scan_id);
}

#[test]
fn present_then_absent_and_absent_then_present_follow_the_latest_scan() {
    let (env, conn) = setup();
    let file = env.bin.join(exe("gitleaks"));
    env.fake_exe("gitleaks");
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, false, None)).unwrap();
    fs::remove_file(&file).unwrap();
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, false, None)).unwrap();
    let inv = store::inventory(&conn).unwrap();
    assert_eq!(gitleaks_path(&inv).presence, "absent");
    assert!(gitleaks_path(&inv).current);
    assert_eq!(request(&inv, "gitleaks", "proje").status, "absent_current");
    env.fake_exe("gitleaks");
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, false, None)).unwrap();
    assert_eq!(request(&store::inventory(&conn).unwrap(), "gitleaks", "proje").status, "present_current");
}

#[test]
fn a_result_older_than_24_hours_is_previous_not_current() {
    let (env, conn) = setup();
    env.fake_exe("gitleaks");
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, false, None)).unwrap();
    let old = (chrono::Utc::now() - chrono::Duration::hours(30)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    conn.execute("UPDATE discovery_scans SET finished_at = ?1", [&old]).unwrap();
    conn.execute("UPDATE discovery_observations SET observed_at = ?1", [&old]).unwrap();
    let inv = store::inventory(&conn).unwrap();
    let g = gitleaks_path(&inv);
    assert!(g.stale && !g.current);
    assert!(g.not_current_reason.as_deref().unwrap().contains("24 saat"), "{:?}", g.not_current_reason);
    assert_eq!(request(&inv, "gitleaks", "proje").status, "previously_present");
    // A fresh scan makes it current again.
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, false, None)).unwrap();
    assert_eq!(request(&store::inventory(&conn).unwrap(), "gitleaks", "proje").status, "present_current");
}

#[test]
fn project_data_is_current_only_while_that_project_is_in_the_latest_scan() {
    let (env, conn) = setup();
    let a = env.root.join("proj-a");
    let b = env.root.join("proj-b");
    for p in [&a, &b] {
        fs::create_dir_all(p.join(".claude")).unwrap();
        fs::write(p.join(".claude/settings.json"), settings(&[(FD_KEY, true)])).unwrap();
    }
    crate::discovery::run_scan(&conn, &env.roots(), &auth(false, false, Some(("proje", &a)))).unwrap();
    let inv = store::inventory(&conn).unwrap();
    let of = |inv: &store::Inventory, alias: &str| {
        inv.observations.iter().filter(|o| o.project_alias.as_deref() == Some(alias)).cloned().collect::<Vec<_>>()
    };
    assert!(of(&inv, "proje").iter().all(|o| o.current), "project A is in the latest scan");
    // The next scan looks at project B only.
    crate::discovery::run_scan(&conn, &env.roots(), &auth(false, false, Some(("proje-b", &b)))).unwrap();
    let inv = store::inventory(&conn).unwrap();
    assert!(of(&inv, "proje-b").iter().all(|o| o.current));
    let old = of(&inv, "proje");
    assert!(!old.is_empty() && old.iter().all(|o| !o.current && o.provenance == "earlier_scan" && !o.latest_scan_checked));
    assert!(
        old.iter().all(|o| o.not_current_reason.as_deref().unwrap().contains("projenin klasörüne bakılmadı")),
        "{:?}",
        old[0].not_current_reason
    );
    // And a scan without any project permission also leaves project B's data as history.
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, false, None)).unwrap();
    assert!(of(&store::inventory(&conn).unwrap(), "proje-b").iter().all(|o| !o.current));
}

fn so(subject: &str, alias: Option<&str>, surface: &str, platform: &str, presence: &str, current: bool) -> StoredObservation {
    StoredObservation {
        id: 1,
        scan_id: "s".into(),
        subject: subject.into(),
        platform_target: platform.into(),
        agent_surface: surface.into(),
        method: "claude-plugin-declaration".into(),
        presence: presence.into(),
        configuration: "configured".into(),
        version: None,
        version_source: None,
        scope: "project".into(),
        project_alias: alias.map(str::to_string),
        confidence: "low".into(),
        evidence: String::new(),
        location: None,
        limitations: vec![],
        identity_verified: false,
        functionality_tested: false,
        observed_at: "2026-10-17T00:00:00Z".into(),
        stale: !current,
        age_hours: 0,
        legacy: false,
        scan_finished_at: "2026-10-17T00:00:00Z".into(),
        scan_authorized_path: false,
        scan_authorized_user_config: true,
        scan_authorized_project: true,
        provenance: if current { "latest_scan" } else { "earlier_scan" }.into(),
        latest_scan_checked: current,
        current,
        not_current_reason: if current { None } else { Some("eski".into()) },
    }
}

#[test]
fn a_request_is_only_matched_with_the_same_project_surface_and_platform() {
    let win = PLATFORM_WINDOWS;
    let mac = PLATFORM_MACOS;
    let obs = vec![
        so("sp", Some("a"), "claude-code-terminal", win, "present", true),
        so("sp", None, "antigravity-ide", win, "present", true),
        so("cli", None, "local-cli", win, "present", true),
    ];
    let status = |entry: &str, alias: &str, surface: &str, platform: &str| classify_request(entry, alias, surface, platform, &obs).0;
    assert_eq!(status("sp", "a", "claude-code-terminal", win), "present_current");
    assert_eq!(status("sp", "b", "claude-code-terminal", win), "not_scanned", "project B is not project A");
    assert_eq!(status("sp", "a", "claude-desktop-code", win), "not_scanned", "another agent surface is not covered");
    assert_eq!(status("sp", "a", "antigravity-ide", win), "present_current", "the computer-wide antigravity row applies to that surface");
    assert_eq!(status("sp", "a", "claude-code-terminal", mac), "platform_mismatch", "only another OS was observed");
    assert_eq!(status("cli", "anything", "claude-code-terminal", win), "present_current", "a program lookup is computer-wide (local-cli)");
    assert_eq!(status("nothing", "a", "claude-code-terminal", win), "not_scanned");
}

#[test]
fn the_best_current_answer_wins_and_previous_answers_never_beat_a_current_one() {
    let win = PLATFORM_WINDOWS;
    let surface = "claude-code-terminal";
    let mixed = vec![
        so("x", None, surface, win, "present", false),
        so("x", None, surface, win, "absent", true),
        so("x", None, surface, win, "unknown", true),
    ];
    assert_eq!(
        classify_request("x", "p", surface, win, &mixed).0,
        "absent_current",
        "an old `present` does not outrank a current `absent`"
    );
    let only_old = vec![so("x", None, surface, win, "absent", false), so("x", None, surface, win, "unknown", false)];
    assert_eq!(classify_request("x", "p", surface, win, &only_old).0, "previously_absent");
    let unsupported = vec![so("x", None, surface, win, "unsupported", true)];
    assert_eq!(classify_request("x", "p", surface, win, &unsupported).0, "unknown_current");
    let unknown_old = vec![so("x", None, surface, win, "unknown", false)];
    assert_eq!(classify_request("x", "p", surface, win, &unknown_old).0, "previously_unknown");
}

#[test]
fn a_preference_for_another_platform_is_reported_as_a_mismatch_not_as_present() {
    let (env, conn) = setup();
    env.fake_exe("gitleaks");
    projects::set_preference(&conn, "proje-b", "gitleaks", "local-cli", other_platform(), "accepted", None).unwrap();
    crate::discovery::run_scan(&conn, &env.roots(), &auth(true, false, None)).unwrap();
    let inv = store::inventory(&conn).unwrap();
    assert_eq!(request(&inv, "gitleaks", "proje").status, "present_current");
    assert_eq!(request(&inv, "gitleaks", "proje-b").status, "platform_mismatch");
}

#[test]
fn history_survives_a_restart_and_repeated_scans_without_changing_the_answer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atm.sqlite");
    let env = Env::new();
    env.fake_exe("gitleaks");
    let statuses = |conn: &Connection| {
        let inv = store::inventory(conn).unwrap();
        (
            inv.requests.iter().map(|r| (r.entry_id.clone(), r.project_alias.clone(), r.status.clone())).collect::<Vec<_>>(),
            inv.observations.iter().map(|o| (o.id, o.current, o.provenance.clone())).collect::<Vec<_>>(),
        )
    };
    let before = {
        let conn = db::open_file(&path, None).unwrap();
        catalog::seed_all(&conn).unwrap();
        projects::add_reference(&conn, "proje", "Proje", "").unwrap();
        projects::set_preference(&conn, "proje", "gitleaks", "local-cli", platform(), "accepted", None).unwrap();
        crate::discovery::run_scan(&conn, &env.roots(), &auth(true, true, None)).unwrap();
        crate::discovery::run_scan(&conn, &env.roots(), &auth(false, true, None)).unwrap(); // path becomes history
        crate::discovery::run_scan(&conn, &env.roots(), &auth(false, true, None)).unwrap();
        let rows: i64 = conn.query_row("SELECT count(*) FROM discovery_observations", [], |r| r.get(0)).unwrap();
        let shown = store::inventory(&conn).unwrap().observations.len() as i64;
        assert!(rows > shown, "history rows ({rows}) are kept; only the newest per key ({shown}) is shown");
        statuses(&conn)
    };
    let conn = db::open_file(&path, None).unwrap(); // application restart
    assert_eq!(statuses(&conn), before);
    let mine = before.0.iter().find(|r| r.0 == "gitleaks" && r.1 == "proje").unwrap();
    assert_eq!(mine.2, "previously_present");
}

#[test]
fn old_scans_are_pruned_but_the_newest_answer_is_still_correct() {
    let (env, conn) = setup();
    env.fake_exe("gitleaks");
    for _ in 0..(store::KEEP_SCANS + 3) {
        crate::discovery::run_scan(&conn, &env.roots(), &auth(true, false, None)).unwrap();
    }
    let scans: i64 = conn.query_row("SELECT count(*) FROM discovery_scans", [], |r| r.get(0)).unwrap();
    assert_eq!(scans, store::KEEP_SCANS);
    assert_eq!(request(&store::inventory(&conn).unwrap(), "gitleaks", "proje").status, "present_current");
    assert!(audit::verify(&conn).unwrap().ok);
}

#[test]
fn every_observation_names_the_scan_and_permissions_it_came_from() {
    let (env, conn) = setup();
    let scan = crate::discovery::run_scan(&conn, &env.roots(), &auth(true, false, None)).unwrap();
    for o in store::inventory(&conn).unwrap().observations {
        assert_eq!(o.scan_id, scan.scan_id);
        assert_eq!(o.scan_finished_at, scan.finished_at);
        assert!(o.scan_authorized_path && !o.scan_authorized_user_config && !o.scan_authorized_project);
    }
}
