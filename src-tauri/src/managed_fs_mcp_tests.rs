//! CONFIG-02B tests of the file-system half of the one-file transaction PROTOTYPE. Everything happens in directories this file creates
//! under the test runner's temporary area, with made-up names and content; nothing real is read or written, nothing is downloaded,
//! started or installed. Windows and macOS run the same behavioural tests against their own `managed_fs` implementation; where a case
//! cannot be built (for example a file symlink without the privilege to create one) the test says `CONFIG02B_SKIPPED: ...` on stderr and
//! returns: a skipped case is reported as SKIPPED in the evidence, never as passed.

use super::*;
use crate::config_txn::{
    test_grant, Approval, Ledger, Plan, Platform, Presence, Refusal, Request, State, CATALOG_PIN, ENTRY_ID, REPOSITORY_URL, SURFACE,
};
use std::{collections::BTreeMap, fs, path::PathBuf};

const ALIAS: &str = "benim-sitem";
const NOW: u64 = 1000;
const SENTINEL: &str = "SENTINEL-NOT-A-REAL-TOKEN-0123456789";
const ORIGINAL: &str = "{\n  \"mcpServers\": {\n    \"other\": {\n      \"command\": \"x\",\n      \"env\": {\n        \"K\": \"${SOME_VAR}\"\n      }\n    }\n  },\n  \"theme\": \"dark\"\n}\n";

/// The stated guarantees, read through `black_box` so that the comparisons are made at run time, not folded away at compile time.
fn stated() -> Guarantees {
    std::hint::black_box(GUARANTEES)
}

fn skipped(case: &str, why: &str) {
    eprintln!("CONFIG02B_SKIPPED: {case}: {why}");
}

fn reset() {
    hook::clear();
    hook::set_unsupported(false);
    fail_at(None);
}

struct Bed {
    root: ProjectRoot,
    data: Dir,
    anchor: Dir,
    base: PathBuf,
    root_path: PathBuf,
    data_path: PathBuf,
    outside: PathBuf,
    _tmp: tempfile::TempDir, // last: the handles above must be closed before the folder is removed
}

impl Drop for Bed {
    fn drop(&mut self) {
        reset();
    }
}

fn bed() -> Bed {
    reset();
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let root_path = base.join("project");
    let appdata = base.join("appdata");
    let outside = base.join("outside");
    for d in [&root_path, &appdata, &outside] {
        fs::create_dir(d).unwrap();
    }
    fs::write(outside.join("canary.txt"), b"canary: must never change").unwrap();
    let anchor = Dir::open_anchor(&appdata).unwrap();
    let data = anchor.create_owner_only_child("backups").unwrap();
    let data_path = appdata.join("backups");
    let root = ProjectRoot::open(&root_path).unwrap();
    Bed { root, data, anchor, base, root_path, data_path, outside, _tmp: tmp }
}

fn host() -> Platform {
    Platform::host().expect("a supported test host (Windows x64 or macOS arm64)")
}

fn target(b: &Bed) -> PathBuf {
    b.root_path.join(".mcp.json")
}

fn request(b: &Bed, now: u64) -> Request<'static> {
    Request {
        alias: ALIAS,
        platform: host(),
        surface: SURFACE,
        entry_id: ENTRY_ID,
        catalog_pin: CATALOG_PIN,
        repository_url: REPOSITORY_URL,
        root: b.root.identity(),
        now,
        ttl: 300,
    }
}

fn plan_at(b: &Bed, now: u64) -> Plan {
    b.root.prepare(&request(b, now), Some(host())).expect("plan")
}

fn plan(b: &Bed) -> Plan {
    plan_at(b, NOW)
}

fn commit(b: &Bed, p: &Plan) -> Result<Receipt, Failure> {
    let a = test_grant(p, 1);
    commit_with(b, p, ALIAS, Some(&a), &mut Ledger::default(), NOW)
}

fn commit_with(b: &Bed, p: &Plan, alias: &str, a: Option<&Approval>, l: &mut Ledger, now: u64) -> Result<Receipt, Failure> {
    b.root.commit(p, alias, a, l, &b.data, now)
}

fn names(b: &Bed) -> Vec<String> {
    b.root.entry_names()
}

fn stray(b: &Bed) -> Vec<String> {
    names(b).into_iter().filter(|n| n.starts_with(STAGE_PREFIX)).collect()
}

fn backup_path(b: &Bed, p: &Plan) -> PathBuf {
    b.data_path.join(format!("backup-{}.json", &p.digest()[..16]))
}

fn is_link(m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        m.file_attributes() & 0x400 != 0
    }
    #[cfg(unix)]
    {
        m.file_type().is_symlink()
    }
}

/// Every regular file below `base` (links are not followed): relative path -> bytes.
fn snapshot(base: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            let m = fs::symlink_metadata(&p).unwrap();
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
            if is_link(&m) {
                out.insert(format!("{rel} -> link"), Vec::new());
            } else if m.is_dir() {
                walk(&p, base, out);
            } else {
                out.insert(rel, fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(base, base, &mut out);
    out
}

fn changed(before: &BTreeMap<String, Vec<u8>>, after: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    let mut v: Vec<String> = after.keys().chain(before.keys()).filter(|k| before.get(*k) != after.get(*k)).cloned().collect();
    v.sort();
    v.dedup();
    v
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

fn canary_untouched(b: &Bed) {
    assert_eq!(fs::read(b.outside.join("canary.txt")).unwrap(), b"canary: must never change");
    assert_eq!(fs::read_dir(&b.outside).unwrap().count(), 1, "nothing was created next to the canary");
}

// ---- the happy paths -----------------------------------------------------------------------------------------------------------------

#[test]
fn the_atomic_primitives_this_prototype_needs_are_present_on_this_volume() {
    let b = bed();
    // the probe writes scratch files, so it runs in the test-owned appdata folder, never in a project root
    assert_eq!(b.anchor.probe_atomic_rename(), Ok(()));
}

#[test]
fn an_absent_file_is_created_atomically_with_exactly_the_stanza_and_nothing_else_changes() {
    let b = bed();
    let before = snapshot(&b.base);
    let p = plan(&b);
    assert_eq!(p.expected(), &Presence::Absent);
    let r = commit(&b, &p).expect("commit");
    assert_eq!(r.trace, ["prepared", "authorized", "target-matches-expected", "staged", "final-recheck", "swapped", "verified"]);
    assert_eq!(r.backup_name, None);
    assert_eq!(fs::read(target(&b)).unwrap(), p.new_bytes());
    assert_eq!(names(&b), [".mcp.json"], "no staging file is left behind");
    let after = snapshot(&b.base);
    assert_eq!(changed(&before, &after), ["project/.mcp.json"], "no other byte of the test area changed");
    canary_untouched(&b);
}

#[test]
fn an_existing_file_is_replaced_keeping_every_other_member_and_the_original_is_backed_up_outside_the_project() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let before = snapshot(&b.base);
    let p = plan(&b);
    let r = commit(&b, &p).expect("commit");
    assert_eq!(
        r.trace,
        ["prepared", "authorized", "target-matches-expected", "backed-up", "staged", "final-recheck", "swapped", "verified"]
    );
    let now: serde_json::Value = serde_json::from_slice(&fs::read(target(&b)).unwrap()).unwrap();
    let mut rest = now.clone();
    let exa = rest["mcpServers"].as_object_mut().unwrap().remove("exa").unwrap();
    assert_eq!(exa, serde_json::json!({"type": "http", "url": "https://mcp.exa.ai/mcp"}));
    assert_eq!(rest, serde_json::from_str::<serde_json::Value>(ORIGINAL).unwrap(), "unrelated fields are unchanged");
    assert_eq!(fs::read(backup_path(&b, &p)).unwrap(), ORIGINAL.as_bytes(), "the backup is the exact original bytes");
    assert_eq!(names(&b), [".mcp.json"], "no staging file, no displaced original is left in the project");
    assert!(!r.displaced_kept);
    let after = snapshot(&b.base);
    let mut expected = vec!["project/.mcp.json".to_string(), format!("appdata/backups/backup-{}.json", &p.digest()[..16])];
    expected.sort();
    assert_eq!(changed(&before, &after), expected);
    canary_untouched(&b);
}

#[test]
fn applying_the_same_operation_twice_writes_once() {
    let b = bed();
    let p = plan(&b);
    commit(&b, &p).expect("first");
    let written = fs::read(target(&b)).unwrap();
    // the same plan again: the file is no longer what the plan expected, and the approval is spent
    let again = commit(&b, &p).err().unwrap();
    assert_eq!(again.refusal, Refusal::TargetAppeared);
    // a NEW plan for the file as it is now is refused: the owned stanza is already there
    assert_eq!(b.root.prepare(&request(&b, NOW + 1), Some(host())).err(), Some(Refusal::ServerNameConflict));
    assert_eq!(fs::read(target(&b)).unwrap(), written);
    assert_eq!(names(&b), [".mcp.json"]);
}

// ---- rollback: only what this transaction wrote, only while it is still exactly that ------------------------------------------------

#[test]
fn rollback_restores_the_exact_original_bytes_of_an_existing_file() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let p = plan(&b);
    let r = commit(&b, &p).unwrap();
    b.root.rollback(&p, &r, &b.data).expect("rollback");
    assert_eq!(fs::read(target(&b)).unwrap(), ORIGINAL.as_bytes());
    assert_eq!(names(&b), [".mcp.json"]);
    // it cannot be run twice: the file is no longer the one that commit wrote
    assert_eq!(b.root.rollback(&p, &r, &b.data).err(), Some(Refusal::RollbackRefusedLaterEdit));
}

#[test]
fn rollback_of_a_created_file_removes_only_that_file() {
    let b = bed();
    let p = plan(&b);
    let r = commit(&b, &p).unwrap();
    b.root.rollback(&p, &r, &b.data).expect("rollback");
    assert!(names(&b).is_empty());
    assert_eq!(b.root.rollback(&p, &r, &b.data).err(), Some(Refusal::RollbackNotNeeded));
}

#[test]
fn rollback_never_overwrites_a_later_edit() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let p = plan(&b);
    let r = commit(&b, &p).unwrap();
    let mut edited = fs::read(target(&b)).unwrap();
    edited.extend_from_slice(b"\n");
    fs::write(target(&b), &edited).unwrap();
    assert_eq!(b.root.rollback(&p, &r, &b.data).err(), Some(Refusal::RollbackRefusedLaterEdit));
    assert_eq!(fs::read(target(&b)).unwrap(), edited, "the user's edit survives");
    // a created file that was edited afterwards is not removed either
    let b2 = bed();
    let p2 = plan(&b2);
    let r2 = commit(&b2, &p2).unwrap();
    fs::write(target(&b2), b"{}\n").unwrap();
    assert_eq!(b2.root.rollback(&p2, &r2, &b2.data).err(), Some(Refusal::RollbackRefusedLaterEdit));
    assert_eq!(fs::read(target(&b2)).unwrap(), b"{}\n");
}

// ---- stale or surprising state ---------------------------------------------------------------------------------------------------------

#[test]
fn a_file_changed_after_the_plan_is_refused_before_anything_is_written() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let p = plan(&b);
    let edited = format!("{ORIGINAL} ");
    fs::write(target(&b), &edited).unwrap(); // an editor's save: new content, same file
    let f = commit(&b, &p).err().unwrap();
    assert_eq!((f.refusal, f.reached), (Refusal::TargetChanged, State::Authorized));
    assert_eq!(fs::read(target(&b)).unwrap(), edited.as_bytes());
    assert_eq!(names(&b), [".mcp.json"]);
    assert!(!backup_path(&b, &p).exists(), "no backup is made for an operation that was refused");
}

#[test]
fn a_file_that_appeared_after_an_absent_plan_is_never_overwritten() {
    let b = bed();
    let p = plan(&b);
    fs::write(target(&b), b"{\"mine\": true}\n").unwrap();
    let f = commit(&b, &p).err().unwrap();
    assert_eq!(f.refusal, Refusal::TargetAppeared);
    assert_eq!(fs::read(target(&b)).unwrap(), b"{\"mine\": true}\n");
    assert_eq!(names(&b), [".mcp.json"]);
}

#[test]
fn a_file_that_appears_between_the_last_check_and_the_swap_is_still_never_overwritten() {
    let b = bed();
    let p = plan(&b);
    let path = target(&b);
    hook::set(move |stage| {
        if stage == "txn:before-swap" {
            fs::write(&path, b"{\"appeared\": true}\n").unwrap();
        }
    });
    let f = commit(&b, &p).err().unwrap();
    assert_eq!(f.refusal, Refusal::TargetAppeared, "decided by the operating system's no-clobber rename");
    assert_eq!(fs::read(target(&b)).unwrap(), b"{\"appeared\": true}\n");
    assert_eq!(names(&b), [".mcp.json"], "the staging file was removed");
}

#[test]
fn an_in_place_edit_between_staging_and_the_final_recheck_is_caught_and_the_edit_is_kept() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let p = plan(&b);
    let path = target(&b);
    hook::set(move |stage| {
        if stage == "txn:before-final-recheck" {
            use std::io::Write;
            fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b" ").unwrap();
        }
    });
    let f = commit(&b, &p).err().unwrap();
    assert_eq!((f.refusal, f.reached), (Refusal::TargetChanged, State::Staged));
    assert_eq!(fs::read(target(&b)).unwrap(), format!("{ORIGINAL} ").as_bytes(), "the edit stands");
    assert!(stray(&b).is_empty(), "the staging file was removed");
}

#[test]
fn a_file_that_vanished_after_the_plan_is_refused() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let p = plan(&b);
    fs::remove_file(target(&b)).unwrap();
    assert_eq!(commit(&b, &p).err().unwrap().refusal, Refusal::TargetVanished);
    assert!(names(&b).is_empty());
}

#[test]
fn a_file_replaced_by_another_with_the_same_content_is_refused_by_identity() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let p = plan(&b);
    fs::remove_file(target(&b)).unwrap();
    fs::write(target(&b), ORIGINAL).unwrap(); // same bytes, different file
    let f = commit(&b, &p).err().unwrap();
    assert_eq!(f.refusal, Refusal::TargetChanged);
    assert_eq!(fs::read(target(&b)).unwrap(), ORIGINAL.as_bytes());
}

#[test]
fn two_competing_plans_exactly_one_wins_and_the_other_is_refused_without_touching_the_result() {
    // (a) sequential: B is refused because A already changed the file
    let b = bed();
    let (pa, pb) = (plan_at(&b, NOW), plan_at(&b, NOW + 1));
    assert_ne!(pa.digest(), pb.digest());
    commit(&b, &pa).expect("A");
    let after_a = fs::read(target(&b)).unwrap();
    assert_eq!(commit(&b, &pb).err().unwrap().refusal, Refusal::TargetAppeared);
    assert_eq!(fs::read(target(&b)).unwrap(), after_a);

    // (b) B commits entirely inside A's last window: A is refused by the operating system's own check and A's staging file is removed
    for existing in [false, true] {
        let b = bed();
        if existing {
            fs::write(target(&b), ORIGINAL).unwrap();
        }
        let (pa, pb) = (plan_at(&b, NOW), plan_at(&b, NOW + 1));
        let (rp, dp, pb2, ab) = (b.root_path.clone(), b.data_path.clone(), pb.clone(), test_grant(&pb, 9));
        hook::set(move |stage| {
            if stage == "txn:before-swap" {
                let r = ProjectRoot::open(&rp).unwrap();
                let d = Dir::open_anchor(&dp).unwrap();
                r.commit(&pb2, ALIAS, Some(&ab), &mut Ledger::default(), &d, NOW).expect("B");
            }
        });
        let f = commit(&b, &pa).err().unwrap();
        let want = if existing { Refusal::TargetChanged } else { Refusal::TargetAppeared };
        assert_eq!(f.refusal, want, "existing={existing}");
        assert_eq!(fs::read(target(&b)).unwrap(), pb.new_bytes(), "B's result stands, untouched");
        assert!(stray(&b).is_empty(), "A's staging file is gone");
    }
}

// ---- who, where, what ----------------------------------------------------------------------------------------------------------------

#[test]
fn a_plan_for_another_project_os_or_surface_is_refused() {
    let b = bed();
    let p = plan(&b);
    let a = test_grant(&p, 1);
    assert_eq!(commit_with(&b, &p, "baska-proje", Some(&a), &mut Ledger::default(), NOW).err().unwrap().refusal, Refusal::WrongProject);
    let other_os = if host() == Platform::WindowsX64 { Platform::MacosArm64 } else { Platform::WindowsX64 };
    assert_eq!(b.root.prepare(&Request { platform: other_os, ..request(&b, NOW) }, Some(host())).err(), Some(Refusal::WrongOs));
    for surface in ["antigravity-ide", "codex-cli", "claude-desktop-code", "local-cli"] {
        assert_eq!(b.root.prepare(&Request { surface, ..request(&b, NOW) }, Some(host())).err(), Some(Refusal::WrongSurface), "{surface}");
    }
    assert!(names(&b).is_empty(), "nothing was written for any of them");
}

#[test]
fn a_plan_made_from_one_root_cannot_be_committed_into_another() {
    let b = bed();
    let p = plan(&b);
    let other_path = b.base.join("other-project");
    fs::create_dir(&other_path).unwrap();
    let other = ProjectRoot::open(&other_path).unwrap();
    assert_ne!(other.identity(), b.root.identity());
    let a = test_grant(&p, 1);
    let f = other.commit(&p, ALIAS, Some(&a), &mut Ledger::default(), &b.data, NOW).err().unwrap();
    assert_eq!(f.refusal, Refusal::RootIdentityChanged);
    assert!(names(&b).is_empty() && other.entry_names().is_empty());
}

#[test]
fn approval_is_required_bound_to_the_plan_one_shot_and_time_limited() {
    let b = bed();
    let p = plan(&b);
    let q = plan_at(&b, NOW + 1);
    let mut l = Ledger::default();
    let refuse = |a: Option<&Approval>, l: &mut Ledger, now: u64| commit_with(&b, &p, ALIAS, a, l, now).err().unwrap().refusal;
    assert_eq!(refuse(None, &mut l, NOW), Refusal::ApprovalMissing);
    assert_eq!(refuse(Some(&test_grant(&q, 2)), &mut l, NOW), Refusal::ApprovalMismatch);
    assert_eq!(refuse(Some(&test_grant(&p, 3)), &mut l, p.expires_at() + 1), Refusal::PlanExpired);
    assert!(names(&b).is_empty(), "none of those wrote anything");
    let good = test_grant(&p, 4);
    commit_with(&b, &p, ALIAS, Some(&good), &mut l, NOW).expect("the right approval");
}

#[test]
fn a_spent_approval_is_not_reusable_and_a_failed_commit_is_not_silently_retried() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let p = plan(&b);
    fs::write(target(&b), format!("{ORIGINAL} ")).unwrap();
    let a = test_grant(&p, 5);
    let mut l = Ledger::default();
    assert_eq!(commit_with(&b, &p, ALIAS, Some(&a), &mut l, NOW).err().unwrap().refusal, Refusal::TargetChanged);
    // the file is put back as it was planned for; the same approval must not work a second time
    fs::write(target(&b), ORIGINAL).unwrap();
    assert_eq!(commit_with(&b, &p, ALIAS, Some(&a), &mut l, NOW).err().unwrap().refusal, Refusal::ApprovalReused);
}

// ---- files that must be refused ------------------------------------------------------------------------------------------------------

#[test]
fn malformed_commented_or_duplicate_key_files_are_refused_and_left_alone() {
    for (text, want) in [
        ("{\"a\": 1,}", Refusal::MalformedJson),
        ("// note\n{}", Refusal::MalformedJson),
        ("{\"a\": 1, \"a\": 2}", Refusal::DuplicateKey),
        ("", Refusal::MalformedJson),
        ("[]", Refusal::RootNotObject),
    ] {
        let b = bed();
        fs::write(target(&b), text).unwrap();
        assert_eq!(b.root.prepare(&request(&b, NOW), Some(host())).err(), Some(want), "{text:?}");
        assert_eq!(fs::read(target(&b)).unwrap(), text.as_bytes());
        assert_eq!(names(&b), [".mcp.json"]);
    }
}

#[test]
fn duplicate_or_case_variant_names_and_the_same_endpoint_under_another_name_are_refused() {
    for (text, want) in [
        (r#"{"mcpServers": {"exa": {"type": "http", "url": "https://other.invalid/x"}}}"#, Refusal::ServerNameConflict),
        (r#"{"mcpServers": {"EXA": {"command": "x"}}}"#, Refusal::ServerNameConflict),
        (r#"{"mcpServers": {"search": {"type": "http", "url": "https://MCP.exa.ai/mcp/"}}}"#, Refusal::EndpointAlreadyConfigured),
    ] {
        let b = bed();
        fs::write(target(&b), text).unwrap();
        assert_eq!(b.root.prepare(&request(&b, NOW), Some(host())).err(), Some(want), "{text}");
        assert_eq!(fs::read(target(&b)).unwrap(), text.as_bytes());
    }
}

#[test]
fn a_file_carrying_the_previews_mask_is_refused() {
    let b = bed();
    fs::write(target(&b), format!("{{\"mcpServers\": {{\"x\": {{\"token\": \"{}\"}}}}}}", crate::config_txn::PREVIEW_MASK)).unwrap();
    assert_eq!(b.root.prepare(&request(&b, NOW), Some(host())).err(), Some(Refusal::RedactionMarkerPresent));
}

#[test]
fn a_secret_in_an_unrelated_server_is_preserved_byte_for_byte_in_meaning_and_never_surfaces_in_a_refusal() {
    let b = bed();
    let text = format!("{{\"mcpServers\": {{\"x\": {{\"headers\": {{\"Authorization\": \"Bearer {SENTINEL}\"}}}}}}}}");
    fs::write(target(&b), &text).unwrap();
    let p = plan(&b);
    commit(&b, &p).unwrap();
    assert!(String::from_utf8(fs::read(target(&b)).unwrap()).unwrap().contains(&format!("Bearer {SENTINEL}")));
    assert!(
        String::from_utf8(fs::read(backup_path(&b, &p)).unwrap()).unwrap().contains(SENTINEL),
        "the backup is the real original (it lives in the private folder)"
    );
    // and a refusal about a broken file that contains the secret does not repeat it
    let b2 = bed();
    fs::write(target(&b2), format!("{{\"k\": \"{SENTINEL}\", \"k\": 1}}")).unwrap();
    let e = b2.root.prepare(&request(&b2, NOW), Some(host())).err().unwrap();
    assert!(!format!("{e:?} {e}").contains(SENTINEL));
}

#[test]
fn an_oversized_file_is_refused() {
    let b = bed();
    fs::write(target(&b), vec![b' '; (MAX_FILE_BYTES + 1) as usize]).unwrap();
    assert_eq!(b.root.inspect().err(), Some(Refusal::FileTooLarge));
}

// ---- links, aliases, special objects -------------------------------------------------------------------------------------------------

#[test]
fn a_directory_link_in_place_of_the_target_is_refused_and_the_outside_is_never_touched() {
    let b = bed();
    if !make_dir_link(&target(&b), &b.outside) {
        return skipped("directory link at the target", "cannot create a junction/symlink here");
    }
    assert_eq!(b.root.inspect().err(), Some(Refusal::TargetIsLink));
    assert_eq!(b.root.prepare(&request(&b, NOW), Some(host())).err(), Some(Refusal::TargetIsLink));
    canary_untouched(&b);
    remove_dir_link(&target(&b));
}

#[test]
fn a_file_symlink_in_place_of_the_target_is_refused_and_the_outside_file_is_never_read_or_changed() {
    let b = bed();
    let outside_file = b.outside.join("canary.txt");
    if !make_file_link(&target(&b), &outside_file) {
        return skipped("file symlink at the target", "creating a file symlink needs a privilege that is not available here");
    }
    assert_eq!(b.root.inspect().err(), Some(Refusal::TargetIsLink));
    canary_untouched(&b);
    let _ = fs::remove_file(target(&b));
}

#[test]
fn a_target_swapped_for_a_link_inside_the_last_window_is_refused_and_the_outside_is_untouched() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let p = plan(&b);
    if !{
        // probe that a link can be made at all before relying on it inside the hook
        let probe = b.base.join("probe-link");
        let ok = make_dir_link(&probe, &b.outside);
        if ok {
            remove_dir_link(&probe);
        }
        ok
    } {
        return skipped("link swapped in during the window", "cannot create a junction/symlink here");
    }
    let (path, outside) = (target(&b), b.outside.clone());
    hook::set(move |stage| {
        if stage == "txn:before-swap" {
            fs::remove_file(&path).unwrap();
            assert!(make_dir_link(&path, &outside));
        }
    });
    let f = commit(&b, &p).err().unwrap();
    // Windows opens the destination as a plain file and reports a directory reparse point as "not a regular file"; macOS names it a link.
    // Either way: refused, the staging file removed, the outside untouched.
    assert!(matches!(f.refusal, Refusal::TargetIsLink | Refusal::TargetNotRegularFile), "{:?}", f.refusal);
    canary_untouched(&b);
    assert!(stray(&b).is_empty());
    reset();
    remove_dir_link(&target(&b));
}

#[test]
fn a_root_that_is_a_link_or_sits_below_one_is_refused() {
    let b = bed();
    let link = b.base.join("root-link");
    if !make_dir_link(&link, &b.root_path) {
        return skipped("root reached through a link", "cannot create a junction/symlink here");
    }
    assert!(ProjectRoot::open(&link).is_err(), "the root itself is a link");
    let via_ancestor = b.base.join("ancestor-link");
    assert!(make_dir_link(&via_ancestor, &b.base));
    assert!(ProjectRoot::open(&via_ancestor.join("project")).is_err(), "a link among the ancestors");
    remove_dir_link(&link);
    remove_dir_link(&via_ancestor);
    canary_untouched(&b);
}

#[test]
fn a_hard_linked_target_is_refused_and_both_names_stay_as_they_were() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let alias = b.root_path.join("second-name.json");
    if fs::hard_link(target(&b), &alias).is_err() {
        return skipped("hard-linked target", "cannot create a hard link here");
    }
    assert_eq!(b.root.inspect().err(), Some(Refusal::TargetHardLinked));
    assert_eq!(fs::read(&alias).unwrap(), ORIGINAL.as_bytes());
    assert_eq!(fs::read(target(&b)).unwrap(), ORIGINAL.as_bytes());
}

#[test]
fn a_case_variant_of_the_target_name_is_refused() {
    let b = bed();
    fs::write(b.root_path.join(".MCP.JSON"), ORIGINAL).unwrap();
    assert_eq!(b.root.inspect().err(), Some(Refusal::CaseAliasPresent));
    assert_eq!(fs::read(b.root_path.join(".MCP.JSON")).unwrap(), ORIGINAL.as_bytes());
}

#[test]
fn a_directory_at_the_target_name_is_refused() {
    let b = bed();
    fs::create_dir(target(&b)).unwrap();
    assert_eq!(b.root.inspect().err(), Some(Refusal::TargetNotRegularFile));
}

#[test]
fn unrelated_files_and_other_agents_files_in_the_root_are_never_touched() {
    let b = bed();
    for (rel, body) in [
        ("CLAUDE.md", "# notes"),
        (".agents/mcp_config.json", "{}"),
        (".codex/config.toml", "x = 1"),
        (".claude/settings.json", "{}"),
        ("mcp.json", "{}"),
        (".mcp.json.bak", "{}"),
    ] {
        let p = b.root_path.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }
    let before = snapshot(&b.root_path);
    let p = plan(&b);
    commit(&b, &p).unwrap();
    let after = snapshot(&b.root_path);
    assert_eq!(changed(&before, &after), [".mcp.json"]);
}

// ---- failure in the middle ----------------------------------------------------------------------------------------------------------

#[test]
fn a_failure_after_the_staging_file_exists_leaves_the_original_untouched_and_no_staging_file() {
    for step in ["txn:after-stage-created", "txn:after-stage-written", "txn:before-final-recheck", "txn:before-swap"] {
        for existing in [false, true] {
            let b = bed();
            if existing {
                fs::write(target(&b), ORIGINAL).unwrap();
            }
            let p = plan(&b);
            fail_at(Some(step));
            let f = commit(&b, &p).err().unwrap();
            assert_eq!(f.refusal, Refusal::Injected, "{step}");
            assert_ne!(f.reached, State::Swapped);
            if existing {
                assert_eq!(fs::read(target(&b)).unwrap(), ORIGINAL.as_bytes(), "{step}");
                assert_eq!(names(&b), [".mcp.json"], "{step}");
            } else {
                assert!(names(&b).is_empty(), "{step}: {:?}", names(&b));
            }
        }
    }
}

#[test]
fn a_failure_before_the_backup_changes_nothing_at_all() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let p = plan(&b);
    let before = snapshot(&b.base);
    fail_at(Some("txn:before-backup"));
    assert_eq!(commit(&b, &p).err().unwrap().refusal, Refusal::Injected);
    assert_eq!(changed(&before, &snapshot(&b.base)), Vec::<String>::new());
}

#[test]
fn a_failure_after_the_swap_is_reported_as_needs_attention_with_the_new_file_in_place_and_the_backup_intact() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let p = plan(&b);
    fail_at(Some("txn:after-swap"));
    let f = commit(&b, &p).err().unwrap();
    assert_eq!((f.refusal, f.reached), (Refusal::Injected, State::NeedsAttention));
    assert_eq!(fs::read(target(&b)).unwrap(), p.new_bytes());
    assert_eq!(fs::read(backup_path(&b, &p)).unwrap(), ORIGINAL.as_bytes());
    // nothing is rolled back automatically: that needs a persisted recovery record, which this prototype does not have (design only)
    assert!(!stated().persistent_recovery_record);
}

#[test]
fn a_crash_leftover_is_reported_and_never_deleted_because_its_ownership_cannot_be_proven() {
    let b = bed();
    let name = format!("{STAGE_PREFIX}deadbeef00000000.tmp");
    fs::write(b.root_path.join(&name), b"leftover of an earlier run").unwrap();
    assert_eq!(b.root.inspect().unwrap().stray_stage_files, 1);
    let p = plan(&b);
    commit(&b, &p).unwrap();
    assert_eq!(fs::read(b.root_path.join(&name)).unwrap(), b"leftover of an earlier run");
    assert_eq!(stray(&b), [name]);
}

#[test]
fn a_volume_without_an_atomic_no_clobber_rename_blocks_the_write_and_nothing_falls_back() {
    for existing in [false, true] {
        let b = bed();
        if existing {
            fs::write(target(&b), ORIGINAL).unwrap();
        }
        let p = plan(&b);
        hook::set_unsupported(true);
        let f = commit(&b, &p).err().unwrap();
        assert_eq!(f.refusal.code(), "WRITE_ADAPTER_BLOCKED");
        if existing {
            assert_eq!(fs::read(target(&b)).unwrap(), ORIGINAL.as_bytes());
            assert_eq!(names(&b), [".mcp.json"]);
        } else {
            assert!(names(&b).is_empty(), "no ordinary-rename fallback created the file");
        }
    }
}

// ---- the backup folder ---------------------------------------------------------------------------------------------------------------

#[test]
fn the_backup_folder_must_be_private_and_must_not_be_the_project() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let p = plan(&b);
    let plain = b.anchor.create_child("plain").unwrap();
    // `create_child` makes a 0700 folder on macOS (and an inherited DACL on Windows). Make the synthetic folder demonstrably NOT owner-only
    // before relying on it, so the test checks the rejection and not the platform's default.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(b.base.join("appdata").join("plain"), fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert!(!plain.is_owner_only(), "the fixture folder must not be owner-only for this case to mean anything");
    let a = test_grant(&p, 1);
    let f = b.root.commit(&p, ALIAS, Some(&a), &mut Ledger::default(), &plain, NOW).err().unwrap();
    assert_eq!(f.refusal, Refusal::BackupNotPrivate);
    let same = Dir::open_anchor(&b.root_path).unwrap();
    let a = test_grant(&p, 2);
    let f = b.root.commit(&p, ALIAS, Some(&a), &mut Ledger::default(), &same, NOW).err().unwrap();
    assert_eq!(f.refusal, Refusal::BackupIsRoot);
    assert_eq!(fs::read(target(&b)).unwrap(), ORIGINAL.as_bytes());
    assert_eq!(names(&b), [".mcp.json"]);
}

#[test]
fn an_existing_backup_of_the_same_name_is_never_overwritten() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let p = plan(&b);
    fs::write(backup_path(&b, &p), b"somebody else's file").unwrap();
    assert_eq!(commit(&b, &p).err().unwrap().refusal, Refusal::BackupExists);
    assert_eq!(fs::read(backup_path(&b, &p)).unwrap(), b"somebody else's file");
    assert_eq!(fs::read(target(&b)).unwrap(), ORIGINAL.as_bytes());
}

// ---- what each platform guarantees, stated and checked -------------------------------------------------------------------------------

#[test]
fn the_stated_guarantees_are_the_ones_this_build_has() {
    let g = stated();
    assert!(g.atomic_no_clobber_create && g.replace_checks_identity_just_before);
    assert!(!g.holds_exclusive_lock && !g.preserves_permissions && !g.persistent_recovery_record);
    assert_eq!(g.displaced_original_retained, cfg!(target_os = "macos"));
    assert_eq!(g.detects_edit_in_replace_window, cfg!(target_os = "macos"));
}

/// macOS: an in-place edit made inside the replace window is DETECTED afterwards (the displaced original is kept and compared); nothing
/// is deleted, the user's edit is preserved in the displaced file and the commit reports NeedsAttention.
#[cfg(target_os = "macos")]
#[test]
fn macos_detects_an_in_place_edit_made_inside_the_replace_window_and_keeps_it() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let p = plan(&b);
    let path = target(&b);
    hook::set(move |stage| {
        if stage == "before-rename" {
            use std::io::Write;
            let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
            f.write_all(b" ").unwrap();
        }
    });
    let f = commit(&b, &p).err().unwrap();
    assert_eq!((f.refusal, f.reached), (Refusal::TargetChanged, State::NeedsAttention));
    let kept = stray(&b);
    assert_eq!(kept.len(), 1, "the displaced original, holding the edit, is kept");
    assert_eq!(fs::read(b.root_path.join(&kept[0])).unwrap(), format!("{ORIGINAL} ").as_bytes());
}

/// Windows: the same edit is NOT detected -- the replaced file is gone and no handle on it is kept. This test pins the documented
/// limit (it is evidence of a gap, not of a guarantee); if the behaviour ever changes, the documents and `GUARANTEES` must change with it.
#[cfg(windows)]
#[test]
fn windows_does_not_detect_an_in_place_edit_made_inside_the_replace_window() {
    let b = bed();
    fs::write(target(&b), ORIGINAL).unwrap();
    let p = plan(&b);
    let path = target(&b);
    hook::set(move |stage| {
        if stage == "before-rename" {
            use std::io::Write;
            let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
            f.write_all(b" ").unwrap();
        }
    });
    let outcome = commit(&b, &p);
    assert!(!stated().detects_edit_in_replace_window);
    assert!(outcome.is_ok(), "{outcome:?}: the lost update is not noticed on Windows");
    assert_eq!(fs::read(target(&b)).unwrap(), p.new_bytes());
    assert!(stray(&b).is_empty(), "the edit is gone: nothing kept it");
}
