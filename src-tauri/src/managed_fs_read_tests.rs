//! CONFIG-02C tests of the read-only file-system half (threat rows B and D of the test plan). Every directory is created by the test under the
//! runner's temporary area with made-up names and content; nothing real is read. Each case writes one `CONFIG02C_RESULT` line (see
//! `config02c_support`): a case that cannot be built here says SKIPPED with a reason code and is never counted as PASS.
//! These tests exercise THIS module through `mcp_read::read_once`; results of the 02B prototype tests never count toward them.
use super::*;
use crate::{
    config02c_support::{bed, case, make_dir_link, make_file_link, remove_dir_link, sentinel, Bed, Verdict, CANARY},
    mcp_read::{self, ReadMcpConfigResult, Reason, Status},
};
use std::{cell::Cell, fs, rc::Rc};

const ALIAS: &str = "benim-sitem";
const VALID: &[u8] = b"{\"mcpServers\":{}}";

fn run(b: &Bed) -> ReadMcpConfigResult {
    mcp_read::read_once(ALIAS, b.root.to_str().unwrap())
}

fn open_counter() -> Rc<Cell<u32>> {
    let count = Rc::new(Cell::new(0u32));
    let seen = count.clone();
    hook::set(move |stage| {
        if stage == "mcpread:before-root" {
            seen.set(seen.get() + 1);
        }
    });
    count
}

fn is(r: &ReadMcpConfigResult, status: Status, reason: Option<Reason>) {
    assert_eq!((r.status, r.reason), (status, reason), "{r:?}");
}

fn nothing_read(r: &ReadMcpConfigResult) {
    assert!(r.size_bytes.is_none() && r.sha256.is_none() && r.server_count.is_none() && r.reviewed_declarations.is_none(), "{r:?}");
}

fn symlink_skip(why: &'static str) -> Verdict {
    Verdict::Skipped(why)
}

// ---- B: filesystem origin ---------------------------------------------------------------------------------------------------------------

#[test]
fn config02c_b1_a_root_that_is_a_link_is_refused_and_nothing_below_it_is_read() {
    case("B1", || {
        let b = bed();
        fs::write(b.outside.join(".mcp.json"), VALID).unwrap();
        let link = b.base.join("rootlink");
        if !make_dir_link(&link, &b.outside) {
            return symlink_skip("cannot_create_directory_link");
        }
        let r = mcp_read::read_once(ALIAS, link.to_str().unwrap());
        remove_dir_link(&link);
        is(&r, Status::UnsafePath, Some(Reason::Link));
        nothing_read(&r);
        assert_eq!(fs::read(b.outside.join(".mcp.json")).unwrap(), VALID, "the file behind the link is unchanged");
        Verdict::Pass
    });
}

#[test]
fn config02c_b2_a_link_in_the_middle_of_the_chain_is_refused() {
    case("B2", || {
        let b = bed();
        fs::create_dir(b.outside.join("inner")).unwrap();
        fs::write(b.outside.join("inner").join(".mcp.json"), VALID).unwrap();
        let link = b.base.join("midlink");
        if !make_dir_link(&link, &b.outside) {
            return symlink_skip("cannot_create_directory_link");
        }
        let r = mcp_read::read_once(ALIAS, link.join("inner").to_str().unwrap());
        remove_dir_link(&link);
        is(&r, Status::UnsafePath, Some(Reason::Link));
        nothing_read(&r);
        Verdict::Pass
    });
}

#[test]
fn config02c_b3_a_file_symlink_at_the_name_is_refused_and_the_outside_file_is_never_read() {
    case("B3", || {
        let b = bed();
        let marker = sentinel("b3");
        fs::write(b.base.join("secret.json"), format!("{{\"mcpServers\":{{\"{marker}\":{{}}}}}}")).unwrap();
        if !make_file_link(&b.target(), &b.base.join("secret.json")) {
            return symlink_skip("cannot_create_file_symlink");
        }
        let r = run(&b);
        is(&r, Status::UnsafePath, Some(Reason::Link));
        nothing_read(&r);
        assert!(!format!("{r:?}{}", serde_json::to_string(&r).unwrap()).contains(&marker));
        b.canary_untouched();
        Verdict::Pass
    });
}

#[test]
fn config02c_b4_a_directory_or_directory_link_at_the_name_is_refused() {
    case("B4", || {
        let b = bed();
        fs::create_dir(b.target()).unwrap();
        let r = run(&b);
        is(&r, Status::UnsafePath, Some(Reason::NotRegularFile));
        nothing_read(&r);
        fs::remove_dir(b.target()).unwrap();
        // a directory link at the name: refused as a link (where one can be created)
        if !make_dir_link(&b.target(), &b.outside) {
            return symlink_skip("cannot_create_directory_link");
        }
        let r = run(&b);
        remove_dir_link(&b.target());
        is(&r, Status::UnsafePath, Some(Reason::Link));
        nothing_read(&r);
        b.canary_untouched();
        Verdict::Pass
    });
}

#[test]
fn config02c_b5_a_hard_linked_target_is_refused_and_both_names_stay_as_they_were() {
    case("B5", || {
        let b = bed();
        b.put(VALID);
        let twin = b.base.join("twin.json");
        if fs::hard_link(b.target(), &twin).is_err() {
            return Verdict::Skipped("cannot_create_hard_link");
        }
        let r = run(&b);
        is(&r, Status::UnsafePath, Some(Reason::HardLinked));
        nothing_read(&r);
        assert_eq!(fs::read(b.target()).unwrap(), VALID);
        assert_eq!(fs::read(&twin).unwrap(), VALID);
        // and a hard link to a file OUTSIDE the project
        fs::remove_file(b.target()).unwrap();
        fs::hard_link(b.outside.join("canary.txt"), b.target()).unwrap();
        let r = run(&b);
        is(&r, Status::UnsafePath, Some(Reason::HardLinked));
        b.canary_untouched();
        Verdict::Pass
    });
}

#[test]
fn config02c_b6a_a_case_variant_name_alone_is_refused_as_an_alias() {
    case("B6a", || {
        let b = bed();
        fs::write(b.root.join(".MCP.JSON"), VALID).unwrap();
        let r = run(&b);
        is(&r, Status::UnsafePath, Some(Reason::CaseAlias));
        nothing_read(&r);
        Verdict::Pass
    });
}

#[test]
fn config02c_b6b_a_case_variant_next_to_the_exact_name_is_refused() {
    case("B6b", || {
        let b = bed();
        b.put(VALID);
        fs::write(b.root.join(".MCP.JSON"), b"{\"other\":1}").unwrap();
        // on a case-insensitive volume the second write replaced the first: there is only one entry, so there is no pair to test
        let names: Vec<String> = fs::read_dir(&b.root).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        if names.len() < 2 {
            return Verdict::Skipped("case_insensitive_volume_cannot_hold_both");
        }
        let r = run(&b);
        is(&r, Status::UnsafePath, Some(Reason::CaseAlias));
        nothing_read(&r);
        Verdict::Pass
    });
}

#[test]
fn config02c_b7a_a_network_or_device_spelling_is_refused_before_anything_is_opened() {
    case("B7a", || {
        if !cfg!(windows) {
            return Verdict::Skipped("no_unc_or_device_spelling_on_this_platform");
        }
        let _b = bed();
        let opens = open_counter();
        for text in [r"\\host\share\project", "//host/share/project", r"\\.\PhysicalDrive0", r"\\?\UNC\host\share\project"] {
            let r = mcp_read::read_once(ALIAS, text);
            is(&r, Status::UnsupportedFs, None);
            nothing_read(&r);
        }
        assert_eq!(opens.get(), 0, "no root was opened");
        Verdict::Pass
    });
}

#[test]
fn config02c_b7b_a_real_network_mount_cannot_be_provided_here() {
    case("B7b", || Verdict::Skipped("no_network_volume_available_on_the_runner"));
}

#[test]
fn config02c_b8_a_fifo_at_the_name_is_refused_without_hanging() {
    case("B8", || {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let b = bed();
            let c = std::ffi::CString::new(b.target().as_os_str().as_bytes()).unwrap();
            // SAFETY: a valid NUL-terminated path in the test's own directory.
            if unsafe { libc::mkfifo(c.as_ptr(), 0o600) } != 0 {
                return Verdict::Skipped("cannot_create_fifo");
            }
            let started = std::time::Instant::now();
            let r = run(&b);
            assert!(started.elapsed().as_secs() < 2, "the read must not wait on a FIFO");
            is(&r, Status::UnsafePath, Some(Reason::NotRegularFile));
            Verdict::Pass
        }
        #[cfg(not(unix))]
        {
            Verdict::Skipped("fifo_is_a_unix_case")
        }
    });
}

#[test]
fn config02c_b9a_an_unreadable_target_is_denied_never_absent() {
    case("B9a", || {
        let b = bed();
        b.put(VALID);
        #[cfg(windows)]
        {
            let user = std::env::var("USERNAME").unwrap_or_default();
            let deny = std::process::Command::new("icacls").arg(b.target()).arg("/deny").arg(format!("{user}:(RD)")).output();
            if !deny.map(|o| o.status.success()).unwrap_or(false) {
                return Verdict::Skipped("cannot_set_a_deny_entry");
            }
            let r = run(&b);
            let _ = std::process::Command::new("icacls").arg(b.target()).arg("/remove:d").arg(&user).output();
            is(&r, Status::Denied, Some(Reason::AccessDenied));
            nothing_read(&r);
            Verdict::Pass
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // SAFETY: plain libc call without arguments.
            if unsafe { libc::geteuid() } == 0 {
                return Verdict::Skipped("running_as_root_ignores_permissions");
            }
            fs::set_permissions(b.target(), fs::Permissions::from_mode(0o000)).unwrap();
            let r = run(&b);
            fs::set_permissions(b.target(), fs::Permissions::from_mode(0o600)).unwrap();
            is(&r, Status::Denied, Some(Reason::AccessDenied));
            nothing_read(&r);
            Verdict::Pass
        }
    });
}

#[test]
fn config02c_b9b_a_target_held_without_sharing_is_denied_as_locked() {
    case("B9b", || {
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let b = bed();
            b.put(VALID);
            let held = fs::OpenOptions::new().read(true).write(true).share_mode(0).open(b.target()).unwrap();
            let r = run(&b);
            drop(held);
            is(&r, Status::Denied, Some(Reason::Locked));
            nothing_read(&r);
            Verdict::Pass
        }
        #[cfg(not(windows))]
        {
            Verdict::Skipped("mandatory_sharing_locks_are_a_windows_case")
        }
    });
}

#[test]
fn config02c_b11_a_missing_root_and_a_missing_file_are_different_answers() {
    case("B11", || {
        let b = bed();
        let r = run(&b);
        is(&r, Status::Absent, None);
        nothing_read(&r);
        let missing = b.base.join("no-such-folder");
        let r = mcp_read::read_once(ALIAS, missing.to_str().unwrap());
        is(&r, Status::RootNotFound, None);
        nothing_read(&r);
        // a file where a folder was typed
        fs::write(b.base.join("plain-file"), b"x").unwrap();
        let r = mcp_read::read_once(ALIAS, b.base.join("plain-file").to_str().unwrap());
        is(&r, Status::UnsafePath, Some(Reason::NotADirectory));
        Verdict::Pass
    });
}

// ---- D: races and re-pointing (stages are fired by the reader; the test acts at one of them) ----------------------------------------------

fn act_at(stage: &'static str, mut action: impl FnMut() + 'static) {
    hook::set(move |s| {
        if s == stage {
            action();
        }
    });
}

#[test]
fn config02c_d1_content_changing_after_the_first_read_returns_no_result_of_any_kind() {
    case("D1", || {
        let b = bed();
        b.put(VALID);
        let target = b.target();
        act_at("mcpread:first-read", move || {
            use std::io::Write;
            fs::OpenOptions::new().append(true).open(&target).unwrap().write_all(b" ").unwrap();
        });
        let r = run(&b);
        is(&r, Status::ChangedDuringRead, None);
        nothing_read(&r);
        Verdict::Pass
    });
}

#[test]
fn config02c_d1b_in_place_overwrite_of_equal_length_is_caught_by_the_second_read() {
    case("D1b", || {
        let b = bed();
        b.put(b"{\"mcpServers\":{\"a\":1}}");
        let target = b.target();
        act_at("mcpread:first-read", move || {
            fs::write(&target, b"{\"mcpServers\":{\"b\":2}}").unwrap();
        });
        let r = run(&b);
        is(&r, Status::ChangedDuringRead, None);
        nothing_read(&r);
        Verdict::Pass
    });
}

#[test]
fn config02c_d1c_an_edit_that_keeps_size_and_time_before_the_second_read_is_caught_by_comparing_the_two_reads() {
    case("D1c", || {
        let b = bed();
        b.put(b"{\"mcpServers\":{\"a\":1}}");
        let target = b.target();
        let stamp = fs::metadata(&target).unwrap().modified().unwrap();
        act_at("mcpread:before-second-read", move || {
            fs::write(&target, b"{\"mcpServers\":{\"b\":2}}").unwrap();
            fs::OpenOptions::new().write(true).open(&target).unwrap().set_modified(stamp).unwrap();
        });
        let r = run(&b);
        is(&r, Status::ChangedDuringRead, None);
        nothing_read(&r);
        Verdict::Pass
    });
}

#[test]
fn config02c_d2b_a_file_replaced_after_both_reads_is_caught_by_the_last_look_at_the_name() {
    case("D2b", || {
        let b = bed();
        b.put(VALID);
        let (target, scratch) = (b.target(), b.root.join("scratch.tmp"));
        let replaced = Rc::new(Cell::new(false));
        let flag = replaced.clone();
        act_at("mcpread:before-final-stat", move || {
            fs::write(&scratch, VALID).unwrap(); // identical bytes, a different object
            flag.set(fs::rename(&scratch, &target).is_ok());
        });
        let r = run(&b);
        if !replaced.get() {
            return Verdict::Skipped("cannot_replace_an_open_file");
        }
        is(&r, Status::ChangedDuringRead, None);
        nothing_read(&r);
        Verdict::Pass
    });
}

#[test]
fn config02c_d1d_an_equal_length_edit_after_the_second_read_is_caught_by_the_last_write_time() {
    case("D1d", || {
        let b = bed();
        b.put(b"{\"mcpServers\":{\"a\":1}}");
        let target = b.target();
        // make sure the clock moves between the file's creation and the edit
        std::thread::sleep(std::time::Duration::from_millis(50));
        act_at("mcpread:second-read", move || {
            fs::write(&target, b"{\"mcpServers\":{\"b\":2}}").unwrap();
        });
        let r = run(&b);
        is(&r, Status::ChangedDuringRead, None);
        nothing_read(&r);
        Verdict::Pass
    });
}

#[test]
fn config02c_d2_a_file_replaced_by_another_between_stat_and_open_is_caught() {
    case("D2", || {
        let b = bed();
        b.put(VALID);
        let (target, scratch) = (b.target(), b.root.join("scratch.tmp"));
        act_at("mcpread:statted", move || {
            fs::write(&scratch, VALID).unwrap(); // identical content, new object
            fs::remove_file(&target).unwrap();
            fs::rename(&scratch, &target).unwrap();
        });
        let r = run(&b);
        is(&r, Status::ChangedDuringRead, None);
        nothing_read(&r);
        Verdict::Pass
    });
}

#[test]
fn config02c_d3_a_link_swapped_in_between_stat_and_open_is_refused_and_the_outside_file_stays_unread() {
    case("D3", || {
        let b = bed();
        b.put(VALID);
        let marker = sentinel("d3");
        fs::write(b.base.join("secret.json"), format!("{{\"{marker}\":1}}")).unwrap();
        let (target, secret) = (b.target(), b.base.join("secret.json"));
        let linked = Rc::new(Cell::new(false));
        let flag = linked.clone();
        act_at("mcpread:statted", move || {
            fs::remove_file(&target).unwrap();
            flag.set(make_file_link(&target, &secret));
        });
        let r = run(&b);
        if !linked.get() {
            return Verdict::Skipped("cannot_create_file_symlink");
        }
        assert!(matches!(r.status, Status::UnsafePath | Status::ChangedDuringRead), "{r:?}");
        nothing_read(&r);
        assert!(!format!("{r:?}").contains(&marker));
        b.canary_untouched();
        Verdict::Pass
    });
}

#[test]
fn config02c_d4_a_file_deleted_after_it_was_opened_never_yields_a_mixed_result() {
    case("D4", || {
        let b = bed();
        b.put(VALID);
        let target = b.target();
        act_at("mcpread:opened", move || {
            let _ = fs::remove_file(&target);
        });
        let r = run(&b);
        // either a consistent result of the object that was opened, or "changed"; the exact outcome is recorded by the line's presence
        match r.status {
            Status::ChangedDuringRead | Status::Denied => nothing_read(&r),
            Status::PresentSummarized => {
                assert_eq!(r.size_bytes, Some(VALID.len() as u64));
                assert_eq!(r.sha256.as_deref(), Some(crate::managed_fs::sha256_hex(VALID).as_str()));
            }
            other => panic!("unexpected status {other:?}"),
        }
        Verdict::Pass
    });
}

#[test]
fn config02c_d5_a_folder_renamed_while_open_still_yields_only_the_bytes_of_the_folder_that_was_walked() {
    case("D5", || {
        let b = bed();
        b.put(VALID);
        let decoy = sentinel("d5");
        let (root, moved) = (b.root.clone(), b.base.join("project-moved"));
        let renamed = Rc::new(Cell::new(false));
        let flag = renamed.clone();
        act_at("mcpread:opened", move || {
            if fs::rename(&root, &moved).is_ok() {
                flag.set(true);
                fs::create_dir(&root).unwrap();
                fs::write(root.join(".mcp.json"), format!("{{\"mcpServers\":{{\"{decoy}\":{{}}}}}}")).unwrap();
            }
        });
        let r = run(&b);
        // Windows keeps the folder from being renamed while the handle is held (the test records which happened); macOS allows it.
        if renamed.get() {
            assert_eq!(
                r.sha256.as_deref(),
                Some(crate::managed_fs::sha256_hex(VALID).as_str()),
                "bytes of the walked folder, not of the decoy: {r:?}"
            );
        } else {
            is(&r, Status::PresentSummarized, None);
        }
        Verdict::Pass
    });
}

#[test]
fn config02c_d6_no_value_of_a_first_call_survives_into_the_second() {
    case("D6", || {
        let b = bed();
        b.put(VALID);
        let first = run(&b);
        b.put(b"{\"mcpServers\":{\"x\":{}}}");
        let second = run(&b);
        assert_ne!(first.sha256, second.sha256);
        assert_eq!((first.server_count, second.server_count), (Some(0), Some(1)));
        Verdict::Pass
    });
}

#[test]
fn config02c_d7_every_call_opens_the_root_afresh() {
    case("D7", || {
        let b = bed();
        b.put(VALID);
        let opens = open_counter();
        let _ = run(&b);
        let _ = run(&b);
        assert_eq!(opens.get(), 2);
        Verdict::Pass
    });
}

#[test]
fn config02c_d8_a_same_size_edit_with_the_time_restored_is_not_detected_and_the_digest_is_of_the_bytes_read() {
    case("D8", || {
        let b = bed();
        let original = b"{\"mcpServers\":{\"a\":1}}".to_vec();
        b.put(&original);
        let target = b.target();
        let stamp = fs::metadata(&target).unwrap().modified().unwrap();
        act_at("mcpread:second-read", move || {
            fs::write(&target, b"{\"mcpServers\":{\"b\":2}}").unwrap();
            fs::OpenOptions::new().write(true).open(&target).unwrap().set_modified(stamp).unwrap();
        });
        let r = run(&b);
        // documented residual: the change inside the window is not seen. The only claim is what the digest is of.
        assert_eq!(r.sha256.as_deref(), Some(crate::managed_fs::sha256_hex(&original).as_str()), "{r:?}");
        Verdict::Pass
    });
}

#[test]
fn config02c_the_reader_leaves_the_folder_exactly_as_it_found_it() {
    case("D9", || {
        let b = bed();
        b.put(VALID);
        let before =
            (fs::read(b.target()).unwrap(), fs::metadata(b.target()).unwrap().modified().unwrap(), fs::read_dir(&b.root).unwrap().count());
        let _ = run(&b);
        let after =
            (fs::read(b.target()).unwrap(), fs::metadata(b.target()).unwrap().modified().unwrap(), fs::read_dir(&b.root).unwrap().count());
        assert_eq!(before, after, "content, last-write time and entry count are unchanged");
        assert_eq!(fs::read(b.outside.join("canary.txt")).unwrap(), CANARY);
        Verdict::Pass
    });
}

// ---- R1 (independent review finding F1): the read opens the root with read rights only -------------------------------------------------------

#[test]
fn config02c_r1a_the_read_opens_the_root_with_no_add_file_or_add_subdirectory_right() {
    case("R1a", || {
        let production = include_str!("managed_fs_read.rs").split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("Dir::open_anchor_readonly("), "the reader opens its root through the read-only path");
        assert!(!production.contains("Dir::open_anchor("), "the reader never uses the writer-capable root open");
        #[cfg(windows)]
        {
            use crate::managed_fs::imp::{DIR_ACCESS, READ_ANCHOR_ACCESS};
            // FILE_WRITE_DATA (add file) 0x2, FILE_APPEND_DATA (add subdirectory) 0x4, FILE_DELETE_CHILD 0x40, FILE_WRITE_ATTRIBUTES 0x100,
            // DELETE 0x10000, WRITE_DAC 0x40000, WRITE_OWNER 0x80000, GENERIC_WRITE 0x40000000, GENERIC_ALL 0x10000000
            let write_like = 0x2 | 0x4 | 0x40 | 0x100 | 0x1_0000 | 0x4_0000 | 0x8_0000 | 0x4000_0000 | 0x1000_0000;
            assert_eq!(READ_ANCHOR_ACCESS & write_like, 0, "the read-only root mask asks for no write-like right: {READ_ANCHOR_ACCESS:#x}");
            // exactly: SYNCHRONIZE | FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES
            assert_eq!(READ_ANCHOR_ACCESS, 0x10_0000 | 0x1 | 0x20 | 0x80);
            // the writer paths are unchanged: they still ask for add-file and add-subdirectory on their own handles
            assert_eq!(DIR_ACCESS & 0x6, 0x6, "DIR_ACCESS is deliberately left as it was");
        }
        #[cfg(unix)]
        {
            let unix = include_str!("managed_fs_unix.rs");
            let flags = unix.lines().find(|l| l.contains("const DIR_FLAGS")).expect("the directory open flags are declared in one place");
            let flags_block = &unix[unix.find(flags).unwrap()..];
            let flags_stmt = flags_block.split(';').next().unwrap();
            assert!(flags_stmt.contains("O_RDONLY") && flags_stmt.contains("O_NOFOLLOW"), "{flags_stmt}");
            for forbidden in ["O_WRONLY", "O_RDWR", "O_CREAT", "O_TRUNC", "O_APPEND"] {
                assert!(!flags_stmt.contains(forbidden), "{forbidden} in the directory open flags");
            }
        }
        Verdict::Pass
    });
}

/// Restores the project folder's permissions however the test ends (a synthetic folder under the runner's temporary area only).
struct RootLock {
    root: std::path::PathBuf,
    #[cfg(windows)]
    user: String,
}

impl Drop for RootLock {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("icacls").arg(&self.root).arg("/remove:d").arg(&self.user).output();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.root, fs::Permissions::from_mode(0o755));
        }
    }
}

#[test]
fn config02c_r1b_a_project_folder_the_user_cannot_write_to_is_still_read_as_a_snapshot() {
    case("R1b", || {
        let b = bed();
        b.put(VALID);
        #[cfg(windows)]
        let _lock = {
            let user = std::env::var("USERNAME").unwrap_or_default();
            // deny only "add file" and "add subdirectory" on THIS synthetic folder; listing, traversing and reading stay allowed
            let deny = std::process::Command::new("icacls").arg(&b.root).arg("/deny").arg(format!("{user}:(WD,AD)")).output();
            let lock = RootLock { root: b.root.clone(), user };
            if !deny.map(|o| o.status.success()).unwrap_or(false) {
                return Verdict::Skipped("cannot_set_a_deny_entry");
            }
            lock
        };
        #[cfg(unix)]
        let _lock = {
            use std::os::unix::fs::PermissionsExt;
            // SAFETY: plain libc call without arguments.
            if unsafe { libc::geteuid() } == 0 {
                return Verdict::Skipped("running_as_root_ignores_permissions");
            }
            let lock = RootLock { root: b.root.clone() };
            fs::set_permissions(&b.root, fs::Permissions::from_mode(0o555)).unwrap();
            lock
        };
        // the precondition must really hold, otherwise this case proves nothing: nothing can be created in the folder ...
        let probe = b.root.join("probe.tmp");
        if fs::File::create(&probe).is_ok() {
            let _ = fs::remove_file(&probe);
            return Verdict::Skipped("folder_stayed_writable");
        }
        if fs::create_dir(b.root.join("probe-dir")).is_ok() {
            let _ = fs::remove_dir(b.root.join("probe-dir"));
            return Verdict::Skipped("folder_stayed_writable");
        }
        // ... while the user can still list it and read the file
        if fs::read_dir(&b.root).is_err() || fs::read(b.target()).ok().as_deref() != Some(VALID) {
            return Verdict::Skipped("folder_became_unreadable");
        }
        let r = run(&b);
        is(&r, Status::PresentSummarized, None);
        assert_eq!(r.sha256.as_deref(), Some(crate::managed_fs::sha256_hex(VALID).as_str()), "{r:?}");
        assert_eq!(r.size_bytes, Some(VALID.len() as u64));
        assert!(!probe.exists() && !b.root.join("probe-dir").exists(), "the reader created nothing");
        Verdict::Pass
    });
}

// ---- R3 (independent review finding F3): a case-only re-spelling of the name after the names were listed -----------------------------------

/// Re-spells `.mcp.json` as `.MCP.JSON` at one stage of the read. Returns whether the folder then really holds the new spelling.
fn respell_at(stage: &'static str, b: &Bed) -> Rc<Cell<bool>> {
    let (target, other) = (b.target(), b.root.join(".MCP.JSON"));
    let done = Rc::new(Cell::new(false));
    let flag = done.clone();
    act_at(stage, move || {
        flag.set(fs::rename(&target, &other).is_ok());
    });
    done
}

fn spelling_on_disk(b: &Bed) -> Vec<String> {
    fs::read_dir(&b.root).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect()
}

fn refused_without_data(r: &ReadMcpConfigResult) {
    assert!(
        matches!((r.status, r.reason), (Status::UnsafePath, Some(Reason::CaseAlias)) | (Status::ChangedDuringRead, None)),
        "a name whose spelling changed must not be summarised: {r:?}"
    );
    nothing_read(r);
}

#[test]
fn config02c_r3a_a_case_only_respelling_after_the_names_were_listed_is_not_summarised() {
    case("R3a", || {
        let b = bed();
        b.put(VALID);
        let done = respell_at("mcpread:listed", &b);
        let r = run(&b);
        if !done.get() || !spelling_on_disk(&b).iter().any(|n| n == ".MCP.JSON") {
            return Verdict::Skipped("cannot_respell_the_name_in_this_fixture");
        }
        if fs::metadata(b.target()).is_err() {
            return Verdict::Skipped("case_sensitive_volume_has_no_case_only_alias");
        }
        refused_without_data(&r);
        Verdict::Pass
    });
}

#[test]
fn config02c_r3b_a_case_only_respelling_after_both_reads_is_not_summarised() {
    case("R3b", || {
        let b = bed();
        b.put(VALID);
        let done = respell_at("mcpread:before-final-stat", &b);
        let r = run(&b);
        if !done.get() || !spelling_on_disk(&b).iter().any(|n| n == ".MCP.JSON") {
            return Verdict::Skipped("cannot_respell_the_name_in_this_fixture");
        }
        if fs::metadata(b.target()).is_err() {
            return Verdict::Skipped("case_sensitive_volume_has_no_case_only_alias");
        }
        refused_without_data(&r);
        Verdict::Pass
    });
}

// ---- G2: the module contains no write-capable call ---------------------------------------------------------------------------------------

#[test]
fn config02c_g2_the_read_only_module_contains_none_of_the_forbidden_calls() {
    case("G2", || {
        let source = include_str!("managed_fs_read.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        for token in [
            "std::process",
            "Command::new",
            "std::net",
            "reqwest",
            "tauri::",
            "invoke",
            "SystemTime",
            "chrono",
            "std::env",
            "std::fs",
            "canonicalize",
            "read_dir",
            "OpenOptions",
            "create_file",
            "create_child",
            "create_owner_only_child",
            "write_all",
            "remove_",
            "move_file",
            "replace_owned_file",
            "rename",
            "File::create",
            "set_permissions",
            "set_len",
            "diagnostics",
            "audit",
            "println!",
            "eprintln!",
            "dbg!",
            "log::",
        ] {
            assert!(!production.contains(token), "managed_fs_read.rs must not contain {token:?}");
        }
        let top = include_str!("mcp_read.rs");
        let top = top.split("#[cfg(test)]").next().unwrap();
        for token in [
            "std::process",
            "Command::new",
            "std::net",
            "reqwest",
            "std::fs",
            "OpenOptions",
            "write_all",
            "println!",
            "eprintln!",
            "dbg!",
            "diagnostics",
            "audit::",
            "rusqlite",
            "Connection",
        ] {
            assert!(!top.contains(token), "mcp_read.rs must not contain {token:?}");
        }
        Verdict::Pass
    });
}
