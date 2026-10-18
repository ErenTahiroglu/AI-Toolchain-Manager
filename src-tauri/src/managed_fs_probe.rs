//! Real-device acceptance probe for the handle-relative file-system layer.
//!
//! Runs inside `--self-check` (so a built binary can be taken to a second Windows computer or a real Mac and asked, with no
//! development tools, what THIS machine and THIS volume actually do). It creates one new, uniquely named folder below the
//! self-check data directory (`ATM_DATA_DIR`, an isolated folder; to test an external volume the person points `ATM_DATA_DIR` at a
//! folder on it), works only with artificial files it writes itself, removes the folder afterwards, and reports short result codes:
//! no path, no account name, no content. It starts no program, uses no network, changes no PATH or setting, and downloads nothing.
//!
//! Result vocabulary: `pass` (the guarantee held), `fail`, `observed` (the platform behaved in a way that is recorded, not judged;
//! the value says how), `not_exercised` (the condition could not be created here, with the reason).

use crate::managed_fs::{Dir, Expect, FsError, Identity, Kind, ReplaceOutcome};
use serde_json::{json, Value};
use std::{fs, path::Path};

struct Out(Vec<Value>);

impl Out {
    fn pass(&mut self, id: &str) {
        self.0.push(json!({ "id": id, "result": "pass" }));
    }
    fn fail(&mut self, id: &str, why: &str) {
        self.0.push(json!({ "id": id, "result": "fail", "why": why }));
    }
    fn observed(&mut self, id: &str, what: &str) {
        self.0.push(json!({ "id": id, "result": "observed", "value": what }));
    }
    fn skipped(&mut self, id: &str, why: &str) {
        self.0.push(json!({ "id": id, "result": "not_exercised", "why": why }));
    }
    fn check(&mut self, id: &str, ok: bool, why: &str) {
        if ok {
            self.pass(id)
        } else {
            self.fail(id, why)
        }
    }
}

fn put(dir: &Dir, name: &str, content: &[u8]) -> Result<Identity, FsError> {
    let mut f = dir.create_file(name)?;
    f.write_all(content)?;
    f.sync_all()?;
    Ok(f.stat()?.identity)
}

pub fn run(base: &Path) -> Value {
    let mut o = Out(Vec::new());
    let folder = base.join(format!("fs-acceptance-{}", uuid::Uuid::new_v4().simple()));
    let setup = fs::create_dir_all(&folder).and_then(|()| fs::canonicalize(&folder));
    let Ok(canon) = setup else {
        return json!({ "ran": false, "why": "the probe folder could not be created", "checks": [] });
    };
    let filesystem = probe(&canon, &mut o);
    // the probe folder was created seconds ago, below the isolated data directory, and holds only artificial files
    let _ = fs::remove_dir_all(&folder);
    let failed = o.0.iter().filter(|c| c["result"] == "fail").count();
    json!({
        "ran": true,
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "filesystem": filesystem,
        "failed": failed,
        "checks": o.0,
    })
}

fn probe(root: &Path, o: &mut Out) -> Value {
    let anchor = match Dir::open_anchor(root) {
        Ok(a) => a,
        Err(e) => {
            o.fail("anchor_opens_without_following_links", e.code());
            return Value::Null;
        }
    };
    o.pass("anchor_opens_without_following_links");
    let filesystem = anchor.filesystem_name().map(|s| json!(s.to_ascii_lowercase())).unwrap_or(Value::Null);

    // owner-only root, created and read back
    match anchor.create_owner_only_child("managed") {
        Ok(m) => o.check("owner_only_root_is_created_and_verified", m.is_owner_only(), "read-back differs"),
        Err(e) => o.fail("owner_only_root_is_created_and_verified", e.code()),
    }
    let (Ok(src), Ok(dst)) = (anchor.create_child("src"), anchor.create_child("dst")) else {
        o.fail("folders_can_be_created", "create_child failed");
        return filesystem;
    };

    // exclusive creation and identity / link count from the handle
    let id = match put(&src, "a.bin", b"artificial payload") {
        Ok(i) => i,
        Err(e) => {
            o.fail("files_can_be_created_and_flushed", e.code());
            return filesystem;
        }
    };
    o.pass("files_can_be_created_and_flushed");
    o.check("exclusive_create_refuses_an_existing_name", src.create_file("a.bin").err() == Some(FsError::Exists), "existing name adopted");
    let st = src.stat("a.bin").ok().flatten();
    o.check(
        "identity_and_single_link_are_read_from_the_handle",
        st.is_some_and(|s| s.identity == id && s.links == 1 && s.kind == Kind::File),
        "identity differs",
    );

    // no-clobber move (the OS primitive), and what the volume supports
    match src.move_file("a.bin", &dst, "a.bin", Some(id)) {
        Ok(()) => {
            o.pass("no_clobber_move_works_on_this_volume");
            let id2 = put(&src, "b.bin", b"second").unwrap_or(id);
            o.check(
                "no_clobber_move_refuses_an_existing_destination",
                src.move_file("b.bin", &dst, "a.bin", Some(id2)).err() == Some(FsError::Exists),
                "destination overwritten",
            );
            o.check(
                "the_refused_destination_is_unchanged",
                dst.read_file("a.bin", 1024).ok().as_deref() == Some(&b"artificial payload"[..]),
                "destination content changed",
            );
            // replacement of one known file
            let wrong = Identity { volume: id.volume, object: id.object ^ 0xFFFF };
            o.check(
                "replacement_refuses_an_unknown_destination",
                src.replace_owned_file("b.bin", &dst, "a.bin", id2, wrong).err() == Some(FsError::Changed),
                "unknown destination replaced",
            );
            match src.replace_owned_file("b.bin", &dst, "a.bin", id2, id) {
                Ok(ReplaceOutcome::Done) => o.check(
                    "replacement_of_the_known_file_works",
                    dst.read_file("a.bin", 1024).ok().as_deref() == Some(&b"second"[..]),
                    "content differs",
                ),
                Ok(ReplaceOutcome::DisplacedKept) => o.observed("replacement_of_the_known_file_works", "replaced_but_old_file_kept"),
                Err(FsError::Unsupported) => o.observed("replacement_of_the_known_file_works", "unsupported_on_this_volume"),
                Err(e) => o.fail("replacement_of_the_known_file_works", e.code()),
            }
        }
        Err(FsError::Unsupported) => {
            o.observed("no_clobber_move_works_on_this_volume", "unsupported_on_this_volume_adapter_must_refuse");
            o.skipped("no_clobber_move_refuses_an_existing_destination", "primitive unsupported");
            o.skipped("replacement_of_the_known_file_works", "primitive unsupported");
        }
        Err(e) => o.fail("no_clobber_move_works_on_this_volume", e.code()),
    }

    // verified deletion: alias, same-content replacement, then the right file
    let del = anchor.create_child("del");
    if let Ok(del) = del {
        let content = b"delete me";
        if let Ok(did) = put(&del, "f.bin", content) {
            let expect = Expect { identity: Some(did), size: Some(content.len() as u64), sha256: None };
            let dirpath = root.join("del");
            if fs::hard_link(dirpath.join("f.bin"), root.join("alias.bin")).is_ok() {
                o.check(
                    "deletion_refuses_a_hard_linked_file",
                    del.remove_file_verified("f.bin", &expect).err() == Some(FsError::Changed),
                    "hard-linked file deleted",
                );
                let _ = fs::remove_file(root.join("alias.bin"));
            } else {
                o.skipped("deletion_refuses_a_hard_linked_file", "hard links not available");
            }
            let _ = fs::remove_file(dirpath.join("f.bin"));
            let _ = fs::write(dirpath.join("f.bin"), content);
            o.check(
                "deletion_refuses_a_same_content_replacement",
                del.remove_file_verified("f.bin", &expect).err() == Some(FsError::Changed),
                "replacement deleted",
            );
            let id3 = del.stat("f.bin").ok().flatten().map(|s| s.identity);
            o.check(
                "deletion_of_the_verified_file_works",
                del.remove_file_verified("f.bin", &Expect { identity: id3, size: Some(content.len() as u64), sha256: None }).is_ok(),
                "verified file not deleted",
            );
        }
    }

    // links are refused and never followed
    let lnk_ok = {
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(root.join("dst"), root.join("lnk")).is_ok()
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("dst"), root.join("lnk")).is_ok()
        }
    };
    if lnk_ok {
        o.check(
            "a_link_is_reported_and_refused",
            anchor.stat("lnk").ok().flatten().is_some_and(|s| s.kind == Kind::Link) && anchor.child("lnk").err() == Some(FsError::Link),
            "link followed",
        );
    } else {
        o.skipped("a_link_is_reported_and_refused", "creating a link needs a privilege that is not available");
    }

    // what the platform does with a held folder and an open file (recorded, not judged)
    if let Ok(held) = anchor.create_child("held") {
        let swapped = fs::remove_dir(root.join("held")).is_ok();
        o.observed(
            "a_held_folder_can_be_removed_by_another_process",
            if swapped { "yes_the_held_handle_is_not_redirected" } else { "no_prevented_by_the_os" },
        );
        drop(held);
    }
    if let Ok(busy) = anchor.create_child("busy") {
        if let Ok(bid) = put(&busy, "open.bin", b"in use") {
            #[cfg(windows)]
            let holder = {
                use std::os::windows::fs::OpenOptionsExt;
                fs::OpenOptions::new().read(true).share_mode(1).open(root.join("busy").join("open.bin"))
            };
            #[cfg(unix)]
            let holder = fs::File::open(root.join("busy").join("open.bin"));
            if holder.is_ok() {
                let r = busy.remove_file_verified("open.bin", &Expect { identity: Some(bid), size: None, sha256: None });
                o.observed(
                    "an_open_file_can_be_removed",
                    match r {
                        Ok(()) => "yes_the_name_is_removed_and_the_holder_is_unaffected",
                        Err(FsError::Locked) => "no_locked_by_the_holder",
                        Err(_) => "no_other_reason",
                    },
                );
            }
        }
    }

    // flushing
    o.observed("folder_flush", if anchor.sync() { "flushed" } else { "not_supported_best_effort" });
    filesystem
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result_of<'a>(v: &'a Value, id: &str) -> &'a Value {
        v["checks"].as_array().unwrap().iter().find(|c| c["id"] == id).unwrap_or_else(|| panic!("no check {id}"))
    }

    #[test]
    fn the_probe_runs_on_artificial_files_only_reports_codes_and_cleans_up() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let v = run(&base);
        assert_eq!(v["ran"], true, "{v}");
        assert_eq!(v["failed"], 0, "{v}");
        let text = v.to_string();
        for needle in [base.to_string_lossy().as_ref(), "Users", "artificial payload"] {
            assert!(!text.contains(needle), "{needle} must not appear in the report");
        }
        assert_eq!(fs::read_dir(&base).unwrap().count(), 0, "the probe folder was removed");
        for c in v["checks"].as_array().unwrap() {
            assert!(["pass", "observed", "not_exercised"].contains(&c["result"].as_str().unwrap()), "{c}");
        }
        for id in [
            "anchor_opens_without_following_links",
            "owner_only_root_is_created_and_verified",
            "exclusive_create_refuses_an_existing_name",
            "deletion_refuses_a_same_content_replacement",
        ] {
            assert_eq!(result_of(&v, id)["result"], "pass", "{id}");
        }
        // the volume of an ordinary test directory supports the no-clobber and replace primitives on both platforms
        for id in [
            "no_clobber_move_works_on_this_volume",
            "no_clobber_move_refuses_an_existing_destination",
            "replacement_of_the_known_file_works",
        ] {
            assert_eq!(result_of(&v, id)["result"], "pass", "{id}: {v}");
        }
        assert!(v["filesystem"].is_string(), "the volume's file-system name is recorded");
    }
}
