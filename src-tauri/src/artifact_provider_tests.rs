//! Phase 3D-3: the retained-file provider. Fixture bytes only; the provider is read-only by construction and these tests prove it.

use super::*;
use std::fs;

fn entry(size: u64) -> RetainedEntry {
    RetainedEntry {
        tool_id: "jq".into(),
        version: "1.8.2".into(),
        platform_target: "windows-x64".into(),
        file_name: "jq-windows-amd64.exe".into(),
        size_bytes: size,
    }
}

fn setup(content: &[u8]) -> (tempfile::TempDir, std::path::PathBuf, RetainedFileProvider) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap().join("input");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("jq-windows-amd64.exe"), content).unwrap();
    let p = RetainedFileProvider::new(dir.clone(), vec![entry(content.len() as u64)]);
    (tmp, dir, p)
}

fn ask(p: &RetainedFileProvider) -> Result<Vec<u8>, &'static str> {
    p.bytes("jq", "1.8.2", "windows-x64")
}

#[test]
fn the_exact_file_is_read_unchanged_and_left_exactly_as_it_was() {
    let (_t, dir, p) = setup(b"fixture bytes of the right size");
    let before = fs::metadata(dir.join("jq-windows-amd64.exe")).unwrap();
    assert_eq!(ask(&p).unwrap(), b"fixture bytes of the right size");
    assert_eq!(ask(&p).unwrap(), b"fixture bytes of the right size", "reading is repeatable and consumes nothing");
    let after = fs::metadata(dir.join("jq-windows-amd64.exe")).unwrap();
    assert_eq!((before.len(), before.modified().unwrap()), (after.len(), after.modified().unwrap()));
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "nothing was added next to it");
}

#[test]
fn only_the_recipes_it_was_built_for_can_be_asked_for() {
    let (_t, _d, p) = setup(b"x");
    for (t, v, pl) in [
        ("yq", "1.8.2", "windows-x64"),
        ("jq", "1.8.3", "windows-x64"),
        ("jq", "1.8.2", "macos-arm64"),
        ("../jq", "1.8.2", "windows-x64"),
        ("", "", ""),
    ] {
        assert_eq!(p.bytes(t, v, pl).unwrap_err(), "artifact_not_available", "{t}/{v}/{pl}");
    }
}

#[test]
fn a_missing_file_a_missing_folder_and_a_wrong_size_are_refused_before_any_byte_is_trusted() {
    let (_t, dir, p) = setup(b"0123456789");
    fs::write(dir.join("jq-windows-amd64.exe"), b"012345678").unwrap();
    assert_eq!(ask(&p).unwrap_err(), "artifact_size_mismatch");
    fs::write(dir.join("jq-windows-amd64.exe"), b"0123456789A").unwrap();
    assert_eq!(ask(&p).unwrap_err(), "artifact_size_mismatch");
    fs::remove_file(dir.join("jq-windows-amd64.exe")).unwrap();
    assert_eq!(ask(&p).unwrap_err(), "artifact_not_available");
    fs::remove_dir(&dir).unwrap();
    assert_eq!(ask(&p).unwrap_err(), "artifact_not_available");
    // a zero or absurd size in the entry is refused outright
    let (_t2, dir2, _p2) = setup(b"x");
    assert_eq!(
        RetainedFileProvider::new(dir2.clone(), vec![entry(0)]).bytes("jq", "1.8.2", "windows-x64").unwrap_err(),
        "artifact_size_mismatch"
    );
    assert_eq!(
        RetainedFileProvider::new(dir2, vec![entry(300 * 1024 * 1024)]).bytes("jq", "1.8.2", "windows-x64").unwrap_err(),
        "artifact_size_mismatch"
    );
}

#[test]
fn a_folder_a_hard_linked_file_or_a_link_in_place_of_the_file_is_not_a_plain_file() {
    // a folder under the file name
    let (_t, dir, p) = setup(b"abc");
    fs::remove_file(dir.join("jq-windows-amd64.exe")).unwrap();
    fs::create_dir(dir.join("jq-windows-amd64.exe")).unwrap();
    assert_eq!(ask(&p).unwrap_err(), "artifact_not_a_plain_file");

    // a second hard link to the same bytes (the file could then be changed through the other name)
    let (_t, dir, p) = setup(b"abc");
    fs::hard_link(dir.join("jq-windows-amd64.exe"), dir.join("other-name.bin")).unwrap();
    assert_eq!(ask(&p).unwrap_err(), "artifact_not_a_plain_file");

    // a symbolic link (needs a privilege on Windows: not exercised there when it cannot be made)
    let (_t, dir, p) = setup(b"abc");
    let real = dir.join("real.bin");
    fs::write(&real, b"abc").unwrap();
    fs::remove_file(dir.join("jq-windows-amd64.exe")).unwrap();
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(&real, dir.join("jq-windows-amd64.exe")).is_ok();
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&real, dir.join("jq-windows-amd64.exe")).is_ok();
    if made {
        assert_eq!(ask(&p).unwrap_err(), "artifact_not_a_plain_file");
    } else {
        eprintln!("file symlinks need a privilege that is not available here; case not exercised");
    }
}

#[test]
fn the_input_folder_itself_replaced_by_a_link_is_not_followed() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let outside = base.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("jq-windows-amd64.exe"), b"abc").unwrap();
    let link = base.join("input");
    #[cfg(windows)]
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&outside, &link).is_ok();
    if !made {
        eprintln!("directory links cannot be created here; case not exercised");
        return;
    }
    let p = RetainedFileProvider::new(link.clone(), vec![entry(3)]);
    assert!(ask(&p).is_err(), "the folder is a link: not followed");
    assert_eq!(fs::read(outside.join("jq-windows-amd64.exe")).unwrap(), b"abc");
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd").args(["/C", "rmdir"]).arg(&link).output();
    #[cfg(unix)]
    let _ = fs::remove_file(&link);
}

#[test]
fn the_provider_can_only_read_and_names_no_path_url_or_command() {
    let whole = include_str!("artifact_provider.rs").split("#[cfg(test)]\n#[path").next().unwrap();
    // code only: the module's own documentation says what it never does, in those very words
    let src: String = whole.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    for needle in [
        "std::fs",
        "File::create",
        "remove_file",
        "remove_dir",
        "rename",
        "write_all",
        "create_file",
        "move_file",
        "std::process",
        "Command",
        "std::net",
        "reqwest",
        "create_child",
        "replace_owned_file",
        "remove_file_verified",
    ] {
        assert!(!src.contains(needle), "{needle}");
    }
    // the trait it implements takes tool, version and platform only
    let flow = include_str!("install_flow.rs");
    assert!(flow.contains("fn bytes(&self, tool_id: &str, version: &str, platform: &str) -> Result<Vec<u8>, &'static str>;"));
}
