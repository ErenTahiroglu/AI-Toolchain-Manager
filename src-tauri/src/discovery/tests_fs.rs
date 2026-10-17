//! RT-2A-03: the whole ancestor chain is validated, and a change between validation and use fails closed.
//! All cases run on synthetic temporary folders.

use super::*;
use crate::discovery::fsx::{self, FinalKind, ReadOutcome, Skip, Walk};
use std::{cell::RefCell, rc::Rc};

/// A directory link: a junction on Windows (needs no privilege), a symbolic link elsewhere.
fn dir_link(link: &Path, target: &Path) {
    #[cfg(windows)]
    make_junction(link, target);
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
}

fn remove_dir_link(link: &Path) {
    #[cfg(windows)]
    remove_junction(link);
    #[cfg(unix)]
    let _ = fs::remove_file(link);
}

/// `<root>/real/sub/project/.mcp.json` holds a sentinel; `<root>/outside/...` is where a redirect would lead.
fn layout(env: &Env) -> PathBuf {
    let project = env.root.join("real/sub/project");
    fs::create_dir_all(project.join(".claude")).unwrap();
    fs::write(
        project.join(".mcp.json"),
        serde_json::json!({ "mcpServers": { "codebase-memory": { "env": { "K": SENTINEL } } } }).to_string(),
    )
    .unwrap();
    fs::write(project.join(".claude/settings.json"), settings(&[(FD_KEY, true)])).unwrap();
    project
}

fn code(path: &Path) -> &'static str {
    validate_project_dir(&path.to_string_lossy()).unwrap_err().code
}

#[test]
fn a_regular_nested_folder_is_accepted() {
    let env = Env::new();
    let project = layout(&env);
    assert!(validate_project_dir(&project.to_string_lossy()).is_ok());
    match fsx::read_text(&project, &[".mcp.json"], fsx::MAX_SMALL_BYTES) {
        ReadOutcome::Text(t) => assert!(t.contains("codebase-memory")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_link_in_a_parent_of_the_project_folder_is_refused_and_nothing_below_it_is_opened() {
    let env = Env::new();
    layout(&env);
    let link = env.root.join("lnk");
    dir_link(&link, &env.root.join("real"));
    let through_link = link.join("sub/project");
    assert_eq!(code(&through_link), "PROJECT_DIR_LINK");

    // The detector itself also refuses to read through the link, independently of the validator.
    let stages = Rc::new(RefCell::new(Vec::<String>::new()));
    let seen = stages.clone();
    fsx::hook::set(move |stage| seen.borrow_mut().push(stage.to_string()));
    // Only the project permission is given, so every read in this scan would be below the redirected folder.
    let only_project = Authorization {
        path_executables: false,
        user_config: false,
        project: Some(ProjectScan { alias: "proje".to_string(), dir: through_link.clone() }),
        ..Default::default()
    };
    let obs = detect::detect_all(&env.roots(), &only_project);
    fsx::hook::clear();
    assert!(!stages.borrow().iter().any(|s| s == "before-open"), "no file below the redirect was even opened: {:?}", stages.borrow());
    assert!(!json_of(&obs).contains("SENTINEL"));
    let mcp = find(&obs, "codebase-memory-mcp", Scope::Project, Method::ClaudeMcpConfig);
    assert_eq!(mcp.configuration, Configuration::Unknown);
    remove_dir_link(&link);
}

#[test]
fn nested_links_in_several_ancestors_are_refused() {
    let env = Env::new();
    layout(&env);
    fs::create_dir_all(env.root.join("real2/inner")).unwrap();
    dir_link(&env.root.join("real/hop"), &env.root.join("real2"));
    let first = env.root.join("first");
    dir_link(&first, &env.root.join("real"));
    assert_eq!(code(&first.join("hop/inner")), "PROJECT_DIR_LINK");
    assert_eq!(code(&env.root.join("real/hop/inner")), "PROJECT_DIR_LINK");
    remove_dir_link(&first);
    remove_dir_link(&env.root.join("real/hop"));
}

#[test]
fn the_folder_itself_being_a_link_is_still_refused() {
    let env = Env::new();
    let project = layout(&env);
    let link = env.root.join("direct");
    dir_link(&link, &project);
    assert_eq!(code(&link), "PROJECT_DIR_LINK");
    remove_dir_link(&link);
}

#[test]
fn a_link_above_the_home_folder_or_a_path_folder_is_not_followed() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    let link = env.root.join("home-link");
    dir_link(&link, &env.root);
    let mut roots = env.roots();
    roots.home = Some(link.join("home"));
    let obs = detect::detect_all(&roots, &all_auth(None));
    let s = find(&obs, "agent:claude-code", Scope::Global, Method::ClaudeSettingsFile);
    assert_eq!(s.configuration, Configuration::Unknown, "{}", s.evidence);
    env.fake_exe("gitleaks");
    let mut roots = env.roots();
    roots.path_entries = vec![link.join("bin")];
    let obs = detect::detect_all(&roots, &all_auth(None));
    assert_eq!(find(&obs, "gitleaks", Scope::Global, Method::PathExecutable).presence, Presence::Unknown);
    remove_dir_link(&link);
}

#[test]
fn walking_below_a_root_refuses_a_link_in_the_middle_of_the_relative_part() {
    let env = Env::new();
    fs::create_dir_all(env.root.join("elsewhere")).unwrap();
    fs::write(env.root.join("elsewhere/f.txt"), "x").unwrap();
    fs::create_dir_all(env.root.join("base")).unwrap();
    dir_link(&env.root.join("base/mid"), &env.root.join("elsewhere"));
    assert!(matches!(fsx::walk(&env.root.join("base"), &["mid", "f.txt"], FinalKind::File, false), Walk::Skipped(Skip::Link)));
    assert!(matches!(fsx::read_text(&env.root.join("base"), &["mid", "f.txt"], 100), ReadOutcome::Skipped(Skip::Link)));
    remove_dir_link(&env.root.join("base/mid"));
}

#[test]
fn traversal_components_are_never_accepted_by_the_file_layer() {
    let env = Env::new();
    for bad in ["..", ".", "a/b", "a\\b", ""] {
        assert!(matches!(fsx::read_text(&env.root, &[bad], 100), ReadOutcome::Skipped(_)), "{bad:?}");
    }
}

#[cfg(windows)]
#[test]
fn a_local_folder_reached_through_a_redirected_drive_letter_is_refused() {
    // `subst` makes a drive letter that LOOKS local but redirects to another folder (the same shape as a mapped
    // network drive: the path text hides where the data really lives).
    let env = Env::new();
    layout(&env);
    let Some(letter) = ('R'..='Z').rev().find(|c| !Path::new(&format!("{c}:\\")).exists()) else { return };
    let drive = format!("{letter}:");
    let ok = std::process::Command::new("subst").arg(&drive).arg(env.root.join("real")).status().map(|s| s.success()).unwrap_or(false);
    if !ok {
        eprintln!("subst unavailable; redirected-drive case not exercised");
        return;
    }
    let redirected = PathBuf::from(format!("{drive}\\sub\\project"));
    let result = validate_project_dir(&redirected.to_string_lossy());
    let read = fsx::read_text(&redirected, &[".mcp.json"], 1000);
    let _ = std::process::Command::new("subst").arg(&drive).arg("/d").status();
    assert_eq!(result.unwrap_err().code, "PROJECT_DIR_LINK", "a substituted drive must be refused");
    assert!(matches!(read, ReadOutcome::Skipped(_)), "nothing may be read through a substituted drive: {read:?}");
}

#[cfg(windows)]
#[test]
fn a_folder_swapped_for_a_junction_after_validation_is_caught_before_any_byte_is_read() {
    let env = Env::new();
    let project = layout(&env);
    let outside = env.root.join("outside");
    fs::create_dir_all(outside.join(".claude")).unwrap();
    fs::write(outside.join(".mcp.json"), "{\"SENTINEL-OUTSIDE\":1}").unwrap();
    let moved = env.root.join("real/sub/project-moved");
    let (target, o, m) = (project.clone(), outside.clone(), moved.clone());
    // At the exact moment between "the chain was checked" and "the file is opened", the folder becomes a junction.
    fsx::hook::set(move |stage| {
        if stage == "before-open" && !m.exists() {
            fs::rename(&target, &m).unwrap();
            make_junction(&target, &o);
        }
    });
    let outcome = fsx::read_text(&project, &[".mcp.json"], 1000);
    fsx::hook::clear();
    assert!(matches!(outcome, ReadOutcome::Skipped(Skip::Link)), "{outcome:?}");
    remove_junction(&project);
}

#[cfg(windows)]
#[test]
fn the_project_validator_also_fails_closed_when_the_folder_is_swapped_mid_validation() {
    let env = Env::new();
    let project = layout(&env);
    let outside = env.root.join("outside");
    fs::create_dir_all(&outside).unwrap();
    let moved = env.root.join("real/sub/project-moved");
    let (t, o, m) = (project.clone(), outside.clone(), moved.clone());
    fsx::hook::set(move |stage| {
        if stage == "after-chain-check" && !m.exists() {
            fs::rename(&t, &m).unwrap();
            make_junction(&t, &o);
        }
    });
    let result = validate_project_dir(&project.to_string_lossy());
    fsx::hook::clear();
    assert_eq!(result.unwrap_err().code, "PROJECT_DIR_LINK");
    remove_junction(&project);
}

#[cfg(unix)]
#[test]
fn a_file_swapped_for_a_symlink_between_the_checks_and_the_open_is_refused() {
    let env = Env::new();
    let project = layout(&env);
    let secret = env.root.join("outside-secret.json");
    fs::write(&secret, "{\"SENTINEL-OUTSIDE\":1}").unwrap();
    let (v, s) = (project.join(".mcp.json"), secret.clone());
    fsx::hook::set(move |stage| {
        if stage == "before-open" {
            let _ = fs::remove_file(&v);
            std::os::unix::fs::symlink(&s, &v).unwrap();
        }
    });
    let outcome = fsx::read_text(&project, &[".mcp.json"], 1000);
    fsx::hook::clear();
    assert!(matches!(outcome, ReadOutcome::Skipped(Skip::Link)), "{outcome:?}");
}

#[cfg(unix)]
#[test]
fn a_folder_swapped_for_a_symlink_while_the_chain_is_being_opened_is_refused() {
    let env = Env::new();
    let project = layout(&env);
    let outside = env.root.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("settings.json"), "{\"SENTINEL-OUTSIDE\":1}").unwrap();
    let normals = project.components().count() - 1; // every component except the root "/"
    let (c, o) = (project.join(".claude"), outside.clone());
    let fired = Rc::new(RefCell::new(0usize));
    let f = fired.clone();
    // The (normals + 1)th component is ".claude": swap it just before it is opened.
    fsx::hook::set(move |stage| {
        if stage == "between-components" {
            *f.borrow_mut() += 1;
            if *f.borrow() == normals + 1 {
                fs::remove_dir_all(&c).unwrap();
                std::os::unix::fs::symlink(&o, &c).unwrap();
            }
        }
    });
    let outcome = fsx::read_text(&project, &[".claude", "settings.json"], 1000);
    fsx::hook::clear();
    assert!(matches!(outcome, ReadOutcome::Skipped(Skip::Link)), "{outcome:?}");
}

#[cfg(unix)]
#[test]
fn an_unreadable_parent_folder_is_reported_not_crashed() {
    use std::os::unix::fs::PermissionsExt;
    let env = Env::new();
    let project = layout(&env);
    let parent = env.root.join("real");
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o000)).unwrap();
    let blocked = fs::read_dir(&parent).is_err(); // running as root ignores the mode
    let result = validate_project_dir(&project.to_string_lossy());
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
    if blocked {
        assert_eq!(result.unwrap_err().code, "PROJECT_DIR_UNREADABLE");
    }
}

// ------------------------------------------------------------------------------------------------
// Issue #6 / RT-2A-R1: the file-system type of EVERY directory is judged, and "cannot tell" means "do not read".
// The lookup is simulated through a test seam keyed by inode. These are NOT live network-mount tests: no network
// volume was mounted or accessed.
// ------------------------------------------------------------------------------------------------

#[cfg(unix)]
mod fstype_seam {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn ino(p: &Path) -> u64 {
        fs::metadata(p).unwrap().ino()
    }

    fn stages_during(f: impl FnOnce()) -> Vec<String> {
        let seen = Rc::new(RefCell::new(Vec::<String>::new()));
        let s = seen.clone();
        fsx::hook::set(move |stage| s.borrow_mut().push(stage.to_string()));
        f();
        let out = seen.borrow().clone();
        out
    }

    #[test]
    fn a_normal_local_folder_is_read_through_the_real_lookup() {
        let env = Env::new();
        let project = layout(&env);
        fsx::hook::clear();
        assert!(matches!(fsx::read_text(&project, &[".mcp.json"], 1000), ReadOutcome::Text(_)));
        assert!(matches!(fsx::dir_exists(&project, &[".claude"]), Walk::Found { .. }));
        assert!(validate_project_dir(&project.to_string_lossy()).is_ok(), "legitimate local installations keep working");
    }

    #[test]
    fn a_failed_fstatfs_below_the_selected_root_refuses_the_folder_and_nothing_is_opened() {
        let env = Env::new();
        let project = layout(&env);
        let claude = ino(&project.join(".claude"));
        fsx::hook::set_fstype(move |i| (i == claude).then_some(Err(())));
        let mut outcome = None;
        let stages = stages_during(|| outcome = Some(fsx::read_text(&project, &[".claude", "settings.json"], 1000)));
        fsx::hook::clear();
        assert!(matches!(outcome, Some(ReadOutcome::Skipped(Skip::UnknownFilesystem))), "{outcome:?}");
        assert!(!stages.iter().any(|s| s == "before-open"), "no file below the unverifiable folder was opened");
    }

    #[test]
    fn a_failed_fstatfs_on_the_root_itself_refuses_everything_below_it() {
        let env = Env::new();
        let project = layout(&env);
        let root = ino(&project);
        fsx::hook::set_fstype(move |i| (i == root).then_some(Err(())));
        let outcome = fsx::read_text(&project, &[".mcp.json"], 1000);
        fsx::hook::clear();
        assert!(matches!(outcome, ReadOutcome::Skipped(Skip::UnknownFilesystem)), "{outcome:?}");
    }

    #[test]
    fn a_network_filesystem_mounted_below_the_root_is_refused_but_its_local_siblings_still_work() {
        let env = Env::new();
        let project = layout(&env);
        let claude = ino(&project.join(".claude"));
        fsx::hook::set_fstype(move |i| (i == claude).then_some(Ok("smbfs".to_string())));
        let stages = stages_during(|| {
            assert!(matches!(fsx::read_text(&project, &[".claude", "settings.json"], 1000), ReadOutcome::Skipped(Skip::Network)));
            assert!(matches!(fsx::walk(&project, &[".claude"], FinalKind::Dir, false), Walk::Skipped(Skip::Network)));
            assert!(matches!(fsx::dir_exists(&project, &[".claude"]), Walk::Skipped(Skip::Network)));
        });
        assert!(!stages.iter().any(|s| s == "before-open"), "nothing was read from the simulated network mount");
        assert!(matches!(fsx::read_text(&project, &[".mcp.json"], 1000), ReadOutcome::Text(_)), "the local sibling is unaffected");
        fsx::hook::clear();
        // The detector reports it as unread, with the reason, instead of treating it as absent.
        fsx::hook::set_fstype({
            let claude = ino(&project.join(".claude"));
            move |i| (i == claude).then_some(Ok("nfs".to_string()))
        });
        let obs = detect::detect_all(
            &env.roots(),
            &Authorization {
                path_executables: false,
                user_config: false,
                project: Some(ProjectScan { alias: "proje".into(), dir: project.clone() }),
                ..Default::default()
            },
        );
        fsx::hook::clear();
        let s = find(&obs, "agent:claude-code", Scope::Project, Method::ClaudeSettingsFile);
        assert_eq!(s.configuration, Configuration::Unknown, "{}", s.evidence);
        assert!(s.evidence.contains("ağ paylaşımı"), "{}", s.evidence);
    }

    #[test]
    fn an_unrecognised_filesystem_type_is_refused() {
        let env = Env::new();
        let project = layout(&env);
        let claude = ino(&project.join(".claude"));
        fsx::hook::set_fstype(move |i| (i == claude).then_some(Ok("macfuse".to_string())));
        let outcome = fsx::read_text(&project, &[".claude", "settings.json"], 1000);
        fsx::hook::clear();
        assert!(matches!(outcome, ReadOutcome::Skipped(Skip::UnknownFilesystem)), "{outcome:?}");
    }

    #[test]
    fn a_network_filesystem_on_a_path_folder_or_the_home_folder_is_skipped_by_the_detector() {
        let env = Env::new();
        env.fake_exe("gitleaks");
        env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
        let (bin, home) = (ino(&env.bin), ino(&env.home));
        fsx::hook::set_fstype(move |i| (i == bin || i == home).then_some(Ok("webdav".to_string())));
        let obs = detect::detect_all(&env.roots(), &all_auth(None));
        fsx::hook::clear();
        assert_eq!(find(&obs, "gitleaks", Scope::Global, Method::PathExecutable).presence, Presence::Unknown);
        assert_eq!(find(&obs, "agent:claude-code", Scope::Global, Method::ClaudeSettingsFile).configuration, Configuration::Unknown);
    }
}
