//! Phase 3D-5: the one-installation allowance as a state machine, on an in-memory database and made-up identifiers (no file, no tool, no network).

use super::*;
use crate::{
    audit::{self, EventContext, NewEvent},
    db,
};
use serde_json::json;

const DIR: &str = "c:\\isolated\\pilot-data";

fn pilot(armed: Option<&str>) -> OneTimePilot {
    OneTimePilot {
        tool_id: "demo-tool".into(),
        version: "1.2.3".into(),
        platform_target: "windows-x64".into(),
        recipe_digest: "a".repeat(64),
        artifact_sha256: "b".repeat(64),
        armed_data_dir_sha256: armed.map(str::to_string),
    }
}

fn dir_sha() -> String {
    data_dir_digest(Path::new(DIR))
}

fn armed() -> OneTimePilot {
    pilot(Some(&dir_sha()))
}

const COMPUTER: &str = "computer-1";

fn sc<'a>(p: &'a OneTimePilot, dir: &'a str, now: &'a str) -> Scope<'a> {
    Scope { pilot: p, computer: COMPUTER, dir_sha: dir, now }
}

fn status(conn: &Connection, p: &OneTimePilot) -> Status {
    evaluate(conn, p, COMPUTER, &dir_sha(), Some(&[]))
}

fn raw(conn: &Connection, p: &OneTimePilot) -> String {
    read_raw(conn, &p.key()).unwrap().unwrap()
}

fn receipt(approval: &str) -> Receipt {
    serde_json::from_value(json!({
        "schemaVersion": 1, "txnId": "txn-1", "action": "install", "toolId": "demo-tool", "fromVersion": null, "toVersion": "1.2.3",
        "artifactSha256": "b".repeat(64), "recipeDigest": "a".repeat(64), "approvalId": approval, "platformTarget": "windows-x64",
        "computerId": COMPUTER, "synthetic": false, "completedAt": "2027-01-01T00:00:00Z", "state": "committed", "kept": [],
        "rollback": { "status": "not_needed", "undone": [], "notUndone": [], "notCovered": [] }, "receiptDigest": "c".repeat(64)
    }))
    .unwrap()
}

#[test]
fn an_unarmed_pilot_is_inactive_and_a_pilot_armed_for_another_directory_is_refused() {
    let conn = db::open_memory().unwrap();
    assert_eq!(status(&conn, &pilot(None)), Status::NotArmed);
    assert_eq!(status(&conn, &pilot(Some(&"0".repeat(64)))), Status::WrongDirectory);
    assert_eq!(status(&conn, &armed()), Status::Open { attempts: 0 });
    // the directory is identified by a normalised path: separators, case and a trailing separator do not matter; another folder does
    assert_eq!(data_dir_digest(Path::new("C:/Isolated/Pilot-Data/")), dir_sha());
    // the value `tools/pilot_data_dir_digest.py` prints for the same text (the person who names the directory uses that tool)
    assert_eq!(dir_sha(), "0cd7d19471ddccb4d25f06b5a8e55eaa17a4008163999a437d9141b515531e5e");
    assert_ne!(data_dir_digest(Path::new("c:\\isolated\\pilot-data2")), dir_sha());
    assert_ne!(data_dir_digest(Path::new("c:\\isolated")), dir_sha());
}

#[test]
fn one_reservation_wins_and_a_consumed_allowance_never_reopens() {
    let conn = db::open_memory().unwrap();
    let p = armed();
    let d = dir_sha();
    reserve(&conn, &sc(&p, &d, "t1"), "approval-1").unwrap();
    assert_eq!(status(&conn, &p), Status::Reserved { approval_id: "approval-1".into() });
    // a second reservation (another approval, a concurrent caller, a stale read) cannot win
    assert_eq!(reserve(&conn, &sc(&p, &d, "t2"), "approval-2"), Err("pilot_allowance_unavailable"));
    // only THIS reservation can be settled
    assert_eq!(consume(&conn, &sc(&p, &d, "t3"), "approval-2", "txn", "digest"), Err("pilot_allowance_unavailable"));
    consume(&conn, &sc(&p, &d, "t4"), "approval-1", "txn-1", &"c".repeat(64)).unwrap();
    assert_eq!(status(&conn, &p), Status::Consumed);
    // nothing reopens it: not a release, not a reservation, not a second consume
    assert_eq!(release(&conn, &sc(&p, &d, "t5"), "approval-1"), Err("pilot_allowance_unavailable"));
    assert_eq!(reserve(&conn, &sc(&p, &d, "t6"), "approval-3"), Err("pilot_allowance_unavailable"));
    assert_eq!(consume(&conn, &sc(&p, &d, "t7"), "approval-1", "txn-1", "digest"), Err("pilot_allowance_unavailable"));
    assert_eq!(status(&conn, &p), Status::Consumed);
    let rec: Record = serde_json::from_str(&raw(&conn, &p)).unwrap();
    assert_eq!((rec.state.as_str(), rec.attempts, rec.txn_id.as_deref()), (CONSUMED, 1, Some("txn-1")));
}

#[test]
fn a_release_reopens_it_for_one_more_attempt_and_counts_the_attempts() {
    let conn = db::open_memory().unwrap();
    let (p, d) = (armed(), dir_sha());
    reserve(&conn, &sc(&p, &d, "t1"), "approval-1").unwrap();
    release(&conn, &sc(&p, &d, "t2"), "approval-1").unwrap();
    assert_eq!(status(&conn, &p), Status::Open { attempts: 1 });
    reserve(&conn, &sc(&p, &d, "t3"), "approval-2").unwrap();
    assert_eq!(status(&conn, &p), Status::Reserved { approval_id: "approval-2".into() });
    assert_eq!(serde_json::from_str::<Record>(&raw(&conn, &p)).unwrap().attempts, 2);
}

#[test]
fn a_damaged_edited_or_foreign_record_is_invalid_and_nothing_resets_it() {
    let conn = db::open_memory().unwrap();
    let (p, d) = (armed(), dir_sha());
    reserve(&conn, &sc(&p, &d, "t1"), "approval-1").unwrap();
    consume(&conn, &sc(&p, &d, "t2"), "approval-1", "txn-1", &"c".repeat(64)).unwrap();
    let good = raw(&conn, &p);
    let put = |v: &str| conn.execute("UPDATE settings SET value = ?1 WHERE key = ?2", rusqlite::params![v, p.key()]).unwrap();
    // not JSON; JSON of another shape; one field edited without the digest; a digest edited
    for bad in ["{", "[]", "", &good.replace("\"consumed\"", "\"released\""), &good.replace("\"attempts\":1", "\"attempts\":0")] {
        put(bad);
        assert_eq!(status(&conn, &p), Status::Invalid("pilot_allowance_state_invalid"), "{bad}");
        // none of the transitions touches an invalid record
        assert!(reserve(&conn, &sc(&p, &d, "t3"), "approval-9").is_err());
        assert!(release(&conn, &sc(&p, &d, "t3"), "approval-1").is_err());
        assert_eq!(read_raw(&conn, &p.key()).unwrap().as_deref(), Some(bad));
    }
    // a record that is internally valid but belongs to another computer / directory / allowance
    let far = "f".repeat(64);
    for (other_computer, other_dir) in [("computer-2", d.as_str()), (COMPUTER, far.as_str())] {
        let conn2 = db::open_memory().unwrap();
        let foreign = OneTimePilot { armed_data_dir_sha256: Some(other_dir.to_string()), ..p.clone() };
        reserve(&conn2, &Scope { pilot: &foreign, computer: other_computer, dir_sha: other_dir, now: "t" }, "a").unwrap();
        let stolen = read_raw(&conn2, &foreign.key()).unwrap();
        conn.execute("UPDATE settings SET value = ?1 WHERE key = ?2", rusqlite::params![stolen, p.key()]).unwrap();
        assert_eq!(status(&conn, &p), Status::Invalid("pilot_allowance_state_for_another_installation"));
    }
    // another recipe or artifact is another allowance with its own record
    let other = OneTimePilot { recipe_digest: "d".repeat(64), ..p.clone() };
    assert_ne!(other.key(), p.key());
    let other = OneTimePilot { artifact_sha256: "e".repeat(64), ..p.clone() };
    assert_ne!(other.key(), p.key());
    put(&good);
    assert_eq!(status(&conn, &p), Status::Consumed);
}

#[test]
fn the_receipts_and_the_audit_trail_are_witnesses_a_missing_or_open_record_cannot_contradict() {
    let conn = db::open_memory().unwrap();
    let (p, d) = (armed(), dir_sha());
    // a committed install receipt but no record: a conflict, not an allowance
    assert_eq!(evaluate(&conn, &p, COMPUTER, &d, Some(&[receipt("approval-1")])), Status::Invalid("pilot_allowance_record_missing"));
    // a receipt of ANOTHER recipe or a removal is no witness
    let mut other = receipt("approval-1");
    other.recipe_digest = "9".repeat(64);
    assert_eq!(evaluate(&conn, &p, COMPUTER, &d, Some(&[other])), Status::Open { attempts: 0 });
    let mut removal = receipt("approval-1");
    removal.action = crate::approval::Action::Uninstall;
    assert_eq!(evaluate(&conn, &p, COMPUTER, &d, Some(&[removal])), Status::Open { attempts: 0 });
    // unreadable receipts are uncertain
    assert_eq!(evaluate(&conn, &p, COMPUTER, &d, None), Status::Invalid("pilot_allowance_witness_unreadable"));
    // a released record cannot stand against a witness
    reserve(&conn, &sc(&p, &d, "t1"), "approval-1").unwrap();
    release(&conn, &sc(&p, &d, "t2"), "approval-1").unwrap();
    assert_eq!(
        evaluate(&conn, &p, COMPUTER, &d, Some(&[receipt("approval-1")])),
        Status::Invalid("pilot_allowance_state_contradicts_receipts")
    );
    assert_eq!(evaluate(&conn, &p, COMPUTER, &d, Some(&[])), Status::Open { attempts: 1 });
    // the audit trail alone is a witness too (receipts deleted, record deleted)
    conn.execute("DELETE FROM settings WHERE key = ?1", [p.key()]).unwrap();
    assert_eq!(status(&conn, &p), Status::Open { attempts: 0 });
    audit::append(
        &conn,
        &NewEvent::plain("install", "passed", "install-apply", None, "not-applicable", "adapter").for_tool(
            "demo-tool",
            EventContext { artifact_digest: Some("b".repeat(64)), approval_reference: Some("approval-1".into()), ..Default::default() },
        ),
    )
    .unwrap();
    assert_eq!(status(&conn, &p), Status::Invalid("pilot_allowance_record_missing"));
}

#[test]
fn the_record_and_its_key_are_internal_and_carry_no_path() {
    let conn = db::open_memory().unwrap();
    let (p, d) = (armed(), dir_sha());
    reserve(&conn, &sc(&p, &d, "t1"), "approval-1").unwrap();
    assert!(p.key().starts_with('_'), "internal keys are never exposed by the settings commands");
    assert!(crate::settings::get_all(&conn).unwrap().keys().all(|k| !k.starts_with('_')));
    let text = raw(&conn, &p);
    assert!(!text.to_lowercase().contains("isolated") && !text.contains("pilot-data"), "only a digest of the directory is stored");
}
