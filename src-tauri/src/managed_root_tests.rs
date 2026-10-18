//! Phase 3C tests of the manager-owned root and its independent record in SQLite. Temporary directories and an isolated database
//! only; nothing outside them is created.

use super::*;
use std::{fs, path::Path};

const COMPUTER: &str = "computer-one";
const NOW: &str = "2026-10-18T10:00:00Z";

struct Bed {
    _tmp: tempfile::TempDir,
    base: PathBuf,
    anchor: AppDataAnchor,
    conn: Connection,
}

fn bed() -> Bed {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let data = base.join("appdata");
    fs::create_dir_all(&data).unwrap();
    let conn = crate::db::open_file(&base.join("atm.sqlite"), None).unwrap();
    Bed { _tmp: tmp, anchor: AppDataAnchor::from_resolved_data_dir(data), base, conn }
}

fn container(b: &Bed) -> PathBuf {
    b.base.join("appdata").join(CONTAINER)
}

fn root_dir(b: &Bed, m: &ManagedRoot) -> PathBuf {
    container(b).join(format!("root-{}", m.root_id()))
}

fn make_dir_link(link: &Path, target: &Path) -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
}

fn remove_dir_link(link: &Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd").args(["/C", "rmdir"]).arg(link).output();
    }
    #[cfg(unix)]
    {
        let _ = fs::remove_file(link);
    }
}

#[test]
fn establishing_creates_an_owner_only_root_with_the_fixed_layout_and_records_its_identity_in_the_database() {
    let b = bed();
    let m = ManagedRoot::establish(&b.anchor, &b.conn, COMPUTER, NOW).unwrap();
    assert!(m.dir().is_owner_only());
    let names: Vec<String> = m.dir().list().unwrap().into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, LAYOUT.map(String::from).to_vec());
    let (state, volume, object): (String, String, String) = b
        .conn
        .query_row("SELECT state, volume_id, object_id FROM managed_roots WHERE root_id = ?1", [m.root_id()], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(state, "active");
    assert_eq!((volume, object), (m.dir().identity().volume.to_string(), m.dir().identity().object.to_string()));
    // the container is owner-only as well, and the folder really is below the application's own data directory
    assert!(Dir::open_anchor(&b.base.join("appdata")).unwrap().child(CONTAINER).unwrap().is_owner_only());
    assert!(root_dir(&b, &m).is_dir());
    // reopening finds the same root
    drop(m);
    let again = ManagedRoot::open(&b.anchor, &b.conn, COMPUTER).unwrap();
    assert_eq!(again.dir().list().unwrap().len(), LAYOUT.len());
    // nothing was created outside the data directory
    let mut outside: Vec<String> = fs::read_dir(&b.base).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    outside.retain(|n| !n.starts_with("atm.sqlite"));
    assert_eq!(outside, vec!["appdata".to_string()]);
}

#[test]
fn a_folder_that_merely_looks_like_a_root_is_never_adopted() {
    let b = bed();
    // no record: even a perfect lookalike (right names, any marker) is not a root
    let fake = container(&b);
    fs::create_dir_all(&fake).unwrap();
    let lookalike = fake.join("root-00000000000000000000000000000001");
    fs::create_dir(&lookalike).unwrap();
    for n in LAYOUT {
        fs::create_dir(lookalike.join(n)).unwrap();
    }
    fs::write(lookalike.join("marker.json"), b"{\"rootId\":\"copied\"}").unwrap();
    assert_eq!(ManagedRoot::open(&b.anchor, &b.conn, COMPUTER).err(), Some(RootError::NotRegistered));
    // with a record: replacing the real root by a lookalike of the same name is an identity mismatch
    fs::remove_dir_all(&fake).unwrap();
    let m = ManagedRoot::establish(&b.anchor, &b.conn, COMPUTER, NOW).unwrap();
    let real = root_dir(&b, &m);
    drop(m);
    fs::remove_dir_all(&real).unwrap();
    fs::create_dir(&real).unwrap();
    for n in LAYOUT {
        fs::create_dir(real.join(n)).unwrap();
    }
    let r = ManagedRoot::open(&b.anchor, &b.conn, COMPUTER);
    // (the copy is not owner-only on Windows either; whichever check fires first, it is refused)
    assert!(matches!(r.err(), Some(RootError::IdentityMismatch) | Some(RootError::Permissions)), "a lookalike must be refused");
}

#[test]
fn an_unfinished_creation_blocks_adoption_and_a_second_root_for_the_same_computer_is_refused() {
    let b = bed();
    b.conn
        .execute(
            "INSERT INTO managed_roots (root_id, computer_id, platform, state, created_at) VALUES (?1, ?2, ?3, 'creating', ?4)",
            rusqlite::params!["a".repeat(32), COMPUTER, platform(), NOW],
        )
        .unwrap();
    assert_eq!(ManagedRoot::establish(&b.anchor, &b.conn, COMPUTER, NOW).err(), Some(RootError::Unfinished));
    assert_eq!(ManagedRoot::open(&b.anchor, &b.conn, COMPUTER).err(), Some(RootError::Unfinished));
    assert!(!container(&b).exists(), "nothing was created while the earlier creation is unresolved");
    // another computer id is independent
    let m = ManagedRoot::establish(&b.anchor, &b.conn, "computer-two", NOW).unwrap();
    assert_eq!(ManagedRoot::establish(&b.anchor, &b.conn, "computer-two", NOW).err(), Some(RootError::AlreadyEstablished));
    assert_eq!(ManagedRoot::open(&b.anchor, &b.conn, "computer-three").err(), Some(RootError::NotRegistered));
    drop(m);
}

#[test]
fn a_root_whose_top_level_is_not_exactly_the_layout_is_refused() {
    let b = bed();
    let m = ManagedRoot::establish(&b.anchor, &b.conn, COMPUTER, NOW).unwrap();
    let dir = root_dir(&b, &m);
    drop(m);
    fs::write(dir.join("stray.txt"), b"x").unwrap();
    assert_eq!(ManagedRoot::open(&b.anchor, &b.conn, COMPUTER).err(), Some(RootError::Layout));
    fs::remove_file(dir.join("stray.txt")).unwrap();
    assert!(ManagedRoot::open(&b.anchor, &b.conn, COMPUTER).is_ok());
    // a layout folder replaced by a file
    fs::remove_dir(dir.join("data")).unwrap();
    fs::write(dir.join("data"), b"x").unwrap();
    assert_eq!(ManagedRoot::open(&b.anchor, &b.conn, COMPUTER).err(), Some(RootError::Layout));
}

#[test]
fn a_container_created_by_someone_else_without_owner_only_permissions_is_not_used() {
    let b = bed();
    fs::create_dir(container(&b)).unwrap(); // default permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(container(&b), fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert_eq!(ManagedRoot::establish(&b.anchor, &b.conn, COMPUTER, NOW).err(), Some(RootError::Permissions));
    // the attempt is on record as unfinished: nothing is silently retried against a folder we do not trust
    assert_eq!(ManagedRoot::establish(&b.anchor, &b.conn, COMPUTER, NOW).err(), Some(RootError::Unfinished));
}

#[test]
fn a_root_replaced_by_a_link_is_refused_and_the_outside_is_untouched() {
    let b = bed();
    let m = ManagedRoot::establish(&b.anchor, &b.conn, COMPUTER, NOW).unwrap();
    let real = root_dir(&b, &m);
    let moved = b.base.join("moved-root");
    drop(m);
    fs::rename(&real, &moved).unwrap();
    if !make_dir_link(&real, &moved) {
        eprintln!("directory links are not available here; case not exercised");
        fs::rename(&moved, &real).unwrap();
        return;
    }
    let r = ManagedRoot::open(&b.anchor, &b.conn, COMPUTER);
    assert_eq!(r.err(), Some(RootError::Fs(FsError::Link)));
    assert_eq!(fs::read_dir(&moved).unwrap().count(), LAYOUT.len(), "nothing was written through the link");
    remove_dir_link(&real);
}

#[test]
fn the_database_rejects_inconsistent_root_records() {
    let b = bed();
    let ins = |sql: &str| b.conn.execute(sql, []);
    assert!(ins(
        "INSERT INTO managed_roots (root_id, computer_id, platform, state, created_at) VALUES ('short', 'c', 'windows', 'creating', 'now')"
    )
    .is_err());
    let id = "b".repeat(32);
    assert!(ins(&format!(
        "INSERT INTO managed_roots (root_id, computer_id, platform, state, created_at) VALUES ('{id}', 'c', 'linux', 'creating', 'now')"
    ))
    .is_err());
    assert!(
        ins(&format!("INSERT INTO managed_roots (root_id, computer_id, platform, state, created_at) VALUES ('{id}', 'c', 'windows', 'active', 'now')")).is_err(),
        "an active root needs its identity"
    );
    assert!(ins(&format!(
        "INSERT INTO managed_roots (root_id, computer_id, platform, state, created_at) VALUES ('{id}', 'c', 'windows', 'creating', 'now')"
    ))
    .is_ok());
    let id2 = "c".repeat(32);
    assert!(
        ins(&format!("INSERT INTO managed_roots (root_id, computer_id, platform, state, created_at) VALUES ('{id2}', 'c', 'windows', 'creating', 'now')")).is_err(),
        "one live root per computer"
    );
    assert!(ins(&format!("INSERT INTO managed_roots (root_id, computer_id, platform, volume_id, state, created_at) VALUES ('{id2}', 'd', 'windows', '1x', 'creating', 'now')")).is_err());
}

#[test]
fn no_command_or_start_up_code_can_name_a_writable_root() {
    for (name, src) in
        [("commands.rs", include_str!("commands.rs")), ("lib.rs", include_str!("lib.rs")), ("state.rs", include_str!("state.rs"))]
    {
        for needle in ["managed_root", "managed_fs", "AppDataAnchor", "ManagedRoot", "adapter::"] {
            // lib.rs declares the modules; nothing else may use them
            let used = src
                .lines()
                .filter(|l| !l.trim_start().starts_with("mod ") && !l.trim_start().starts_with("//"))
                .any(|l| l.contains(needle));
            assert!(!used, "{name} must not use {needle}");
        }
    }
}

#[test]
fn a_volume_without_the_atomic_primitives_is_refused_and_nothing_is_left_behind() {
    let b = bed();
    let m = ManagedRoot::establish(&b.anchor, &b.conn, COMPUTER, NOW).unwrap();
    let staging = root_dir(&b, &m).join("staging");
    drop(m);
    assert_eq!(fs::read_dir(&staging).unwrap().count(), 0, "the capability probe cleaned up after itself");
    // the same root later found on (or moved to) a volume that cannot do an atomic no-clobber move: refused, fail closed
    crate::managed_fs::hook::set_unsupported(true);
    let r = ManagedRoot::open(&b.anchor, &b.conn, COMPUTER);
    crate::managed_fs::hook::set_unsupported(false);
    assert_eq!(r.err(), Some(RootError::Fs(FsError::Unsupported)));
    assert_eq!(fs::read_dir(&staging).unwrap().count(), 0, "a refused probe leaves nothing either");
    assert!(ManagedRoot::open(&b.anchor, &b.conn, COMPUTER).is_ok());
    // establishing on such a volume is refused as well (and stays on record as unfinished: nothing is adopted)
    let b2 = bed();
    crate::managed_fs::hook::set_unsupported(true);
    let r = ManagedRoot::establish(&b2.anchor, &b2.conn, COMPUTER, NOW);
    crate::managed_fs::hook::set_unsupported(false);
    assert_eq!(r.err(), Some(RootError::Fs(FsError::Unsupported)));
    assert_eq!(ManagedRoot::establish(&b2.anchor, &b2.conn, COMPUTER, NOW).err(), Some(RootError::Unfinished));
}
