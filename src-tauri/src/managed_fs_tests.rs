//! Phase 3C tests of the handle-relative file-system layer. Everything happens in temporary directories with made-up names and
//! content; nothing is downloaded, started or installed. Windows and macOS run the same behavioural tests against their own
//! implementation (`managed_fs_windows.rs` / `managed_fs_unix.rs`); differences in what the platform can guarantee are asserted
//! explicitly, not hidden.

use super::*;
use std::{fs, path::PathBuf};

struct Bed {
    _tmp: tempfile::TempDir,
    base: PathBuf,
    anchor: Dir,
}

fn bed_in(sub: &str) -> Bed {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap().join(sub);
    fs::create_dir_all(&base).unwrap();
    let anchor = Dir::open_anchor(&base).expect("anchor");
    Bed { _tmp: tmp, base, anchor }
}

fn bed() -> Bed {
    hook::clear();
    bed_in("anchor")
}

fn put(dir: &Dir, name: &str, content: &[u8]) -> Identity {
    let mut f = dir.create_file(name).unwrap();
    f.write_all(content).unwrap();
    f.sync_all().unwrap();
    f.stat().unwrap().identity
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

fn make_file_link(link: &Path, target: &Path) -> bool {
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link).is_ok()
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
}

// ---- the basics ------------------------------------------------------------------------------------------------------------------

#[test]
fn create_write_sync_stat_list_read_hash_and_verified_removal_work() {
    let b = bed();
    let sub = b.anchor.create_child("sub").unwrap();
    let id = put(&sub, "a.bin", b"hello managed world");
    let s = sub.stat("a.bin").unwrap().unwrap();
    assert_eq!((s.kind, s.links, s.size, s.identity), (Kind::File, 1, 19, id));
    assert_eq!(sub.list().unwrap(), vec![("a.bin".to_string(), Kind::File)]);
    assert_eq!(b.anchor.list().unwrap(), vec![("sub".to_string(), Kind::Dir)]);
    let mut f = sub.open_file("a.bin").unwrap();
    assert_eq!(f.read_bounded(1024).unwrap(), b"hello managed world");
    let (sha, n) = f.sha256_bounded(1024).unwrap();
    assert_eq!((sha.as_str(), n), (sha256_hex(b"hello managed world").as_str(), 19));
    assert_eq!(f.read_bounded(5).unwrap_err(), FsError::TooLarge, "never truncated silently");
    assert_eq!(f.sha256_bounded(5).unwrap_err(), FsError::TooLarge);
    drop(f);
    let expect = Expect { identity: Some(id), size: Some(19), sha256: Some(sha256_hex(b"hello managed world")) };
    sub.remove_file_verified("a.bin", &expect).unwrap();
    assert!(sub.stat("a.bin").unwrap().is_none());
    drop(sub);
    b.anchor.remove_dir("sub").unwrap();
    assert!(b.anchor.list().unwrap().is_empty());
    // the real folder on disk agrees (the layer wrote where it says)
    assert_eq!(fs::read_dir(&b.base).unwrap().count(), 0);
}

#[test]
fn creation_is_exclusive_and_never_adopts_what_is_there() {
    let b = bed();
    fs::write(b.base.join("foreign.txt"), b"not mine").unwrap();
    fs::create_dir(b.base.join("foreign-dir")).unwrap();
    assert_eq!(b.anchor.create_file("foreign.txt").err(), Some(FsError::Exists));
    assert_eq!(b.anchor.create_child("foreign-dir").err(), Some(FsError::Exists));
    assert_eq!(fs::read(b.base.join("foreign.txt")).unwrap(), b"not mine");
    put(&b.anchor, "mine.bin", b"x");
    assert_eq!(b.anchor.create_file("mine.bin").err(), Some(FsError::Exists));
}

#[test]
fn only_plain_names_are_ever_used() {
    let b = bed();
    let long = "a".repeat(101);
    for bad in
        ["", ".", "..", "a/b", "a\\b", "a:b", "CON", "nul.txt", "COM1", "lpt9.bin", "trailing.", "-leading", "ü", "a b", "x..y", &long]
    {
        assert!(!valid_name(bad), "{bad:?}");
        assert_eq!(b.anchor.create_file(bad).err(), Some(FsError::InvalidName), "{bad:?}");
        assert_eq!(b.anchor.child(bad).err(), Some(FsError::InvalidName), "{bad:?}");
        assert_eq!(b.anchor.stat(bad).err(), Some(FsError::InvalidName), "{bad:?}");
        assert_eq!(b.anchor.remove_file_verified(bad, &Expect::default()).err(), Some(FsError::InvalidName), "{bad:?}");
        assert_eq!(b.anchor.remove_dir(bad).err(), Some(FsError::InvalidName), "{bad:?}");
    }
    for good in ["a", "artifact.bin.part", "synthetic-tool.json", "1.0.0", "a_b-c+d", "0123456789abcdef"] {
        assert!(valid_name(good), "{good}");
    }
}

#[test]
fn the_anchor_may_live_below_unicode_and_space_named_folders_but_every_name_below_it_is_plain() {
    let b = bed_in("Üser Ünïcode adı");
    let inner = b.anchor.create_child("data").unwrap();
    put(&inner, "x.bin", b"1");
    assert_eq!(inner.list().unwrap().len(), 1);
}

// ---- links, junctions, reparse points ------------------------------------------------------------------------------------------

#[test]
fn a_link_in_place_of_a_folder_is_reported_as_a_link_and_never_followed() {
    let b = bed();
    let outside = b.base.parent().unwrap().join("outside-dir");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), b"outside content").unwrap();
    assert!(make_dir_link(&b.base.join("lnk"), &outside));
    assert_eq!(b.anchor.stat("lnk").unwrap().unwrap().kind, Kind::Link);
    assert_eq!(b.anchor.child("lnk").err(), Some(FsError::Link));
    assert_eq!(b.anchor.remove_dir("lnk").err(), Some(FsError::Link));
    assert_eq!(b.anchor.create_child("lnk").err(), Some(FsError::Exists));
    assert!(b.anchor.open_file("lnk").is_err());
    assert_eq!(b.anchor.list().unwrap(), vec![("lnk".to_string(), Kind::Link)]);
    assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"outside content", "the target was not touched");
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
    remove_dir_link(&b.base.join("lnk"));
}

#[test]
fn a_link_inside_the_anchors_own_path_is_refused() {
    hook::clear();
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let real = base.join("real");
    fs::create_dir_all(real.join("inner")).unwrap();
    assert!(make_dir_link(&base.join("redirect"), &real));
    let r = Dir::open_anchor(&base.join("redirect").join("inner"));
    assert_eq!(r.err(), Some(FsError::Link), "a link in an ancestor of the anchor");
    // and the plain route works
    assert!(Dir::open_anchor(&real.join("inner")).is_ok());
    remove_dir_link(&base.join("redirect"));
}

#[test]
fn a_swap_of_a_folder_for_a_link_between_two_components_is_caught() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let outside = base.join("outside");
    fs::create_dir_all(outside.join("inner")).unwrap();
    fs::create_dir_all(base.join("a/inner")).unwrap();
    let (swap_from, swap_to, out2) = (base.join("a"), base.join("a-moved"), outside.clone());
    let mut n = 0;
    hook::set(move |stage| {
        if stage == "between-components" {
            n += 1;
            // after the handle for `a` is held, replace the folder `a` by a link to the outside; the held handle is unaffected
            if swap_from.exists() && !swap_to.exists() && n > 0 {
                let _ = fs::rename(&swap_from, &swap_to);
                let _ = make_dir_link(&swap_from, &out2);
            }
        }
    });
    let r = Dir::open_anchor(&base.join("a").join("inner"));
    hook::clear();
    // Whatever order the swap took effect in, the result is either the REAL folder (opened before the swap) or a refusal;
    // it is never the outside folder reached through the link.
    if let Ok(d) = r {
        d.create_file("probe.bin").unwrap();
        assert!(!outside.join("inner/probe.bin").exists(), "nothing was written through the redirect");
        assert!(base.join("a-moved/inner/probe.bin").exists() || base.join("a/inner/probe.bin").exists());
    }
    remove_dir_link(&base.join("a"));
}

#[test]
fn a_file_link_is_never_opened_or_deleted_through() {
    let b = bed();
    fs::write(b.base.join("target.txt"), b"target").unwrap();
    if !make_file_link(&b.base.join("flink"), &b.base.join("target.txt")) {
        eprintln!("file symlinks need a privilege that is not available here; case not exercised");
        return;
    }
    assert_eq!(b.anchor.stat("flink").unwrap().unwrap().kind, Kind::Link);
    assert!(b.anchor.open_file("flink").is_err());
    assert!(b.anchor.remove_file_verified("flink", &Expect::default()).is_err());
    assert_eq!(fs::read(b.base.join("target.txt")).unwrap(), b"target");
    assert!(b.base.join("flink").symlink_metadata().is_ok(), "the link itself is left in place");
}

// ---- identity, hard links, substitution ------------------------------------------------------------------------------------

#[test]
fn deletion_requires_the_same_file_the_same_content_and_a_single_link() {
    let b = bed();
    let content = b"payload one";
    let want = |id| Expect { identity: Some(id), size: Some(content.len() as u64), sha256: Some(sha256_hex(content)) };

    // same content, different file (new identity)
    let id = put(&b.anchor, "f.bin", content);
    fs::remove_file(b.base.join("f.bin")).unwrap();
    fs::write(b.base.join("f.bin"), content).unwrap();
    assert_eq!(b.anchor.remove_file_verified("f.bin", &want(id)).err(), Some(FsError::Changed));
    assert_eq!(fs::read(b.base.join("f.bin")).unwrap(), content, "the replacement survives");
    fs::remove_file(b.base.join("f.bin")).unwrap();

    // same file edited in place, same size, other bytes
    let id = put(&b.anchor, "g.bin", content);
    {
        let mut f = fs::OpenOptions::new().write(true).open(b.base.join("g.bin")).unwrap();
        std::io::Write::write_all(&mut f, b"payload two").unwrap();
    }
    assert_eq!(b.anchor.remove_file_verified("g.bin", &want(id)).err(), Some(FsError::Changed));
    assert_eq!(fs::read(b.base.join("g.bin")).unwrap(), b"payload two");
    fs::remove_file(b.base.join("g.bin")).unwrap();

    // another size
    let id = put(&b.anchor, "h.bin", b"short");
    assert_eq!(b.anchor.remove_file_verified("h.bin", &want(id)).err(), Some(FsError::Changed));
    assert!(b.anchor.stat("h.bin").unwrap().is_some());
    fs::remove_file(b.base.join("h.bin")).unwrap();

    // a hard-linked alias
    let id = put(&b.anchor, "i.bin", content);
    fs::hard_link(b.base.join("i.bin"), b.base.join("alias.bin")).unwrap();
    assert_eq!(b.anchor.stat("i.bin").unwrap().unwrap().links, 2);
    assert_eq!(b.anchor.remove_file_verified("i.bin", &want(id)).err(), Some(FsError::Changed));
    assert_eq!(fs::read(b.base.join("alias.bin")).unwrap(), content);
    assert_eq!(fs::read(b.base.join("i.bin")).unwrap(), content);
    fs::remove_file(b.base.join("alias.bin")).unwrap();

    // the right file goes
    b.anchor.remove_file_verified("i.bin", &want(id)).unwrap();

    // a folder under the name of a file is not a file
    fs::create_dir(b.base.join("d")).unwrap();
    assert_eq!(b.anchor.remove_file_verified("d", &Expect::default()).err(), Some(FsError::NotFile));
    assert!(b.base.join("d").is_dir());
}

#[test]
fn a_swap_between_the_check_and_the_deletion_never_deletes_the_newcomer() {
    let b = bed();
    let id = put(&b.anchor, "victim.bin", b"original");
    let (name, moved) = (b.base.join("victim.bin"), b.base.join("original-moved.bin"));
    hook::set(move |stage| {
        if stage == "before-delete" {
            // another process of the same user swaps the name between our verification and our unlink
            let _ = fs::rename(&name, &moved);
            let _ = fs::write(&name, b"alien newcomer");
        }
    });
    let expect = Expect { identity: Some(id), size: Some(8), sha256: Some(sha256_hex(b"original")) };
    let r = b.anchor.remove_file_verified("victim.bin", &expect);
    hook::clear();
    // The newcomer under the expected name is never deleted, whatever the platform does with the original:
    //  * Windows deletes THROUGH THE VERIFIED HANDLE, so the original (wherever it was moved) is the object removed;
    //  * macOS re-confirms (dev, ino) of the name immediately before unlinkat and refuses.
    assert_eq!(fs::read(b.base.join("victim.bin")).unwrap(), b"alien newcomer", "{r:?}");
    #[cfg(windows)]
    {
        r.unwrap();
        assert!(!b.base.join("original-moved.bin").exists(), "the verified object itself was deleted by handle");
    }
    #[cfg(unix)]
    {
        assert_eq!(r.err(), Some(FsError::Changed));
        assert_eq!(fs::read(b.base.join("original-moved.bin")).unwrap(), b"original");
    }
}

// ---- locks and permissions -------------------------------------------------------------------------------------------------------

#[cfg(windows)]
#[test]
fn a_file_another_program_holds_open_is_reported_as_locked_and_stays() {
    use std::os::windows::fs::OpenOptionsExt;
    let b = bed();
    let id = put(&b.anchor, "busy.bin", b"in use");
    // an exclusive reader (share mode 1 = read only, so no delete is allowed), like a running program or a scanner
    let _held = fs::OpenOptions::new().read(true).share_mode(1).open(b.base.join("busy.bin")).unwrap();
    let r = b.anchor.remove_file_verified("busy.bin", &Expect { identity: Some(id), ..Expect::default() });
    assert_eq!(r.err(), Some(FsError::Locked));
    assert!(b.base.join("busy.bin").exists());
}

#[cfg(unix)]
#[test]
fn on_macos_a_file_open_elsewhere_can_be_unlinked_and_stays_readable_for_its_holder() {
    let b = bed();
    let id = put(&b.anchor, "busy.bin", b"in use");
    let mut held = fs::File::open(b.base.join("busy.bin")).unwrap();
    b.anchor.remove_file_verified("busy.bin", &Expect { identity: Some(id), ..Expect::default() }).unwrap();
    let mut text = String::new();
    std::io::Read::read_to_string(&mut held, &mut text).unwrap();
    assert_eq!(text, "in use", "the running holder keeps its content: the removal must be reported honestly as 'name removed'");
}

#[test]
fn a_read_only_file_is_not_deleted_and_the_refusal_is_reported() {
    let b = bed();
    let id = put(&b.anchor, "ro.bin", b"read only");
    let path = b.base.join("ro.bin");
    let mut perm = fs::metadata(&path).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(&path, perm).unwrap();
    #[cfg(windows)]
    {
        let r = b.anchor.remove_file_verified("ro.bin", &Expect { identity: Some(id), ..Expect::default() });
        assert!(r.is_err(), "{r:?}");
        assert!(path.exists());
    }
    #[cfg(unix)]
    {
        // on Unix the read-only bit of the FILE does not protect it from unlink (the folder's permissions do)
        let _ = id;
    }
    let mut perm = fs::metadata(&path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    fs::set_permissions(&path, perm).unwrap();
}

#[cfg(unix)]
#[test]
fn an_inaccessible_folder_refuses_creation_and_nothing_is_written() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("running as root: permission denial cannot be exercised");
        return;
    }
    let b = bed();
    let sub = b.anchor.create_child("locked").unwrap();
    fs::set_permissions(b.base.join("locked"), fs::Permissions::from_mode(0o500)).unwrap();
    assert_eq!(sub.create_file("x.bin").err(), Some(FsError::Denied));
    fs::set_permissions(b.base.join("locked"), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(sub.list().unwrap().is_empty());
}

#[test]
fn the_root_folder_is_created_owner_only_and_the_result_is_read_back() {
    let b = bed();
    let root = b.anchor.create_owner_only_child("managed").unwrap();
    assert!(root.is_owner_only());
    let inner = root.create_child("inner").unwrap();
    put(&inner, "x.bin", b"1");
    // a folder created the ordinary way by someone else is NOT owner-only on Windows (default inherited ACL)
    fs::create_dir(b.base.join("ordinary")).unwrap();
    let ordinary = b.anchor.child("ordinary").unwrap();
    #[cfg(windows)]
    assert!(!ordinary.is_owner_only(), "an inherited default DACL is not the protected owner-only form");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(b.base.join("ordinary"), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!ordinary.is_owner_only(), "group/other bits make it not owner-only");
    }
}

// ---- mount transitions ---------------------------------------------------------------------------------------------------------

#[test]
fn a_folder_on_another_volume_is_refused_and_a_move_between_volumes_is_never_attempted() {
    let b = bed();
    let sub = b.anchor.create_child("sub").unwrap();
    let other = b.anchor.create_child("other").unwrap();
    let sub_object = sub.identity().object;
    drop(sub);
    // SIMULATED: a real mount point is a reparse point on Windows (refused as a link) and needs a second volume on macOS
    hook::set_volume(move |object| (object == sub_object).then_some(0xDEAD_BEEF));
    assert_eq!(b.anchor.child("sub").err(), Some(FsError::CrossDevice));
    hook::clear();
    let ok = b.anchor.child("sub").unwrap();
    put(&other, "m.bin", b"1");
    hook::set_volume(move |object| (object == sub_object).then_some(0xDEAD_BEEF));
    assert_eq!(other.rename_file("m.bin", &ok, "m.bin", false, None).err(), Some(FsError::CrossDevice));
    hook::clear();
    assert!(other.stat("m.bin").unwrap().is_some(), "the file did not move");
}

// ---- rename ---------------------------------------------------------------------------------------------------------------------

#[test]
fn rename_preserves_identity_is_exclusive_unless_replacing_and_checks_the_expectation() {
    let b = bed();
    let src = b.anchor.create_child("src").unwrap();
    let dst = b.anchor.create_child("dst").unwrap();
    let id = put(&src, "a.bin", b"one");
    // wrong expectation: nothing moves
    let wrong = Identity { volume: id.volume, object: id.object ^ 0xFFFF };
    assert_eq!(src.rename_file("a.bin", &dst, "a.bin", false, Some(wrong)).err(), Some(FsError::Changed));
    assert!(src.stat("a.bin").unwrap().is_some() && dst.stat("a.bin").unwrap().is_none());
    // move across folders; the identity follows the file
    src.rename_file("a.bin", &dst, "a.bin", false, Some(id)).unwrap();
    assert_eq!(dst.stat("a.bin").unwrap().unwrap().identity, id);
    assert!(src.stat("a.bin").unwrap().is_none());
    // exclusive: an existing destination is refused and untouched
    put(&src, "b.bin", b"two");
    assert_eq!(src.rename_file("b.bin", &dst, "a.bin", false, None).err(), Some(FsError::Exists));
    assert_eq!(fs::read(b.base.join("dst/a.bin")).unwrap(), b"one");
    // replacing is atomic: afterwards the destination is the moved file
    let id_b = src.stat("b.bin").unwrap().unwrap().identity;
    src.rename_file("b.bin", &dst, "a.bin", true, Some(id_b)).unwrap();
    assert_eq!(fs::read(b.base.join("dst/a.bin")).unwrap(), b"two");
    assert_eq!(dst.stat("a.bin").unwrap().unwrap().identity, id_b);
    // a folder is not moved as a file
    src.create_child("f").unwrap();
    assert!(src.rename_file("f", &dst, "f", false, None).is_err());
}

#[test]
fn removing_a_folder_is_never_recursive() {
    let b = bed();
    let sub = b.anchor.create_child("sub").unwrap();
    let nested = sub.create_child("nested").unwrap();
    put(&nested, "precious.db", b"precious");
    drop(nested);
    drop(sub);
    assert_eq!(b.anchor.remove_dir("sub").err(), Some(FsError::NotEmpty));
    assert_eq!(fs::read(b.base.join("sub/nested/precious.db")).unwrap(), b"precious");
    assert_eq!(b.anchor.remove_dir("missing").err(), Some(FsError::NotFound));
}

#[test]
fn listings_are_bounded_and_unusual_names_are_flagged_not_trusted() {
    let b = bed();
    for i in 0..5 {
        put(&b.anchor, &format!("f{i}.bin"), b"1");
    }
    assert_eq!(b.anchor.imp.list(2).err(), Some(FsError::TooLarge));
    fs::write(b.base.join("odd name.txt"), b"x").unwrap();
    let l = b.anchor.list().unwrap();
    assert!(l.contains(&("<invalid>".to_string(), Kind::Other)), "{l:?}");
}

#[test]
fn the_layer_has_no_path_based_mutation_no_recursion_no_process_and_no_network() {
    let mut text = String::new();
    for src in [include_str!("managed_fs.rs"), include_str!("managed_fs_windows.rs"), include_str!("managed_fs_unix.rs")] {
        text.push_str(src.split("#[cfg(test)]").next().unwrap());
    }
    let needles = [
        "remove_dir_all",
        "fs::remove_file",
        "fs::remove_dir",
        "fs::write",
        "fs::create_dir",
        "fs::rename",
        "fs::copy",
        "fs::hard_link",
        "std::process",
        "Command::new",
        "std::net",
        "TcpStream",
        "reqwest",
        "set_var",
        "set_current_dir",
    ];
    for needle in needles {
        assert!(!text.contains(needle), "{needle} must not appear in the file-system layer");
    }
}
