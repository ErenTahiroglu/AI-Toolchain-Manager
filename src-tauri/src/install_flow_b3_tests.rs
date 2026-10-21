// Phase 3D-5 (Issue #24) tests, included at the end of install_flow_b2_tests.rs (they share its lab and helpers). SYNTHETIC bytes only.
//
// The pilot must be a ONE-installation capability, not a persistent one: one completed installation consumes it durably; a restart, a removal, a second
// preview or a fresh confirmation cannot reopen it; concurrent requests cannot spend it twice; anything uncertain is `needs_attention` and nothing is
// silently reset.

use crate::pilot_allowance::{self, Record};

/// The scope identifier of the ONE data directory the user authorized for the pilot (Phase 3D-6): the digest of the normalised path text. The path itself is
/// never committed (the digest is a scope identifier, not a secret and not an authentication credential).
const PILOT_DIR_DIGEST: &str = "158c279e2151088c91783a5d788cc5877cc417cb23e0bff69dbeade99a7ee8a5";

/// What the real supply answers in an (isolated, temporary) data directory that is NOT the armed one.
fn expected_refusal_in_a_foreign_directory() -> &'static str {
    if jq_pilot::B3_ARMED_DATA_DIR_SHA256.is_some() {
        "pilot_wrong_data_directory"
    } else {
        "pilot_not_armed"
    }
}

fn pilot_key(l: &Lab) -> String {
    l.svc.sources.one_time.as_ref().unwrap().key()
}

fn record_raw(l: &Lab) -> Option<String> {
    l.with(|c| c.query_row("SELECT value FROM settings WHERE key = ?1", [pilot_key(l)], |r| r.get(0)).ok())
}

fn record_state(l: &Lab) -> String {
    serde_json::from_str::<Record>(&record_raw(l).expect("a record exists")).unwrap().state
}

fn put_record(l: &Lab, raw: &str) {
    l.with(|c| c.execute("INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, 't')", rusqlite::params![pilot_key(l), raw]).unwrap());
}

fn delete_record(l: &Lab) {
    l.with(|c| c.execute("DELETE FROM settings WHERE key = ?1", [pilot_key(l)]).unwrap());
}

fn confirm_on(svc: &InstallService, l: &Lab, p: &InstallPreview) -> InstallResult {
    svc.confirm(&l.db, &ConfirmRequest { offer_id: p.offer_id.clone(), preview_digest: p.preview_digest.clone(), acknowledged: true })
}

fn prepare_on(svc: &InstallService, l: &Lab, action: Action) -> AppResult<InstallPreview> {
    l.with(|c| svc.prepare(c, &PrepareRequest { entry_id: JQ.into(), action }))
}

fn state_on(svc: &InstallService, l: &Lab) -> ToolState {
    l.with(|c| svc.state(c, JQ).unwrap())
}

#[test]
fn b3_one_installation_survives_a_restart_and_a_second_one_cannot_even_be_previewed() {
    let (l, b, _k, input) = b2_lab(b"b3 one");
    let inventory_before = tree_without_database(&l.base);
    assert_eq!(l.state(JQ).status, Status::Available);
    assert!(record_raw(&l).is_none(), "looking and previewing create no allowance record");
    let p = l.preview(JQ, Action::Install);
    assert!(record_raw(&l).is_none(), "a preview reserves nothing");
    assert_eq!(l.confirm(&p).outcome, "completed");
    assert_eq!(record_state(&l), "consumed");
    // restart: the allowance is a durable fact, not process memory
    let svc2 = b2_restart(&l, &b, &input);
    let st = state_on(&svc2, &l);
    assert_eq!(st.status, Status::InstalledVerified, "{st:?}");
    assert_eq!(prepare_on(&svc2, &l, Action::Install).unwrap_err().code, "INSTALL_ALREADY_INSTALLED");
    // the record names the installation: the receipt's approval, transaction and digest
    let rec: Record = serde_json::from_str(&record_raw(&l).unwrap()).unwrap();
    let receipt = &receipts_json(&l)[0];
    assert_eq!((rec.attempts, rec.approval_id.as_str()), (1, receipt["approvalId"].as_str().unwrap()));
    assert_eq!((rec.txn_id.as_deref(), rec.receipt_digest.as_deref()), (receipt["txnId"].as_str(), receipt["receiptDigest"].as_str()));
    assert_eq!(rec.artifact_sha256, crate::synthetic::sha256_of(&b.bytes));
    // the allowance is only ever written inside the application's own database; nothing else moved
    let after = tree_without_database(&l.base);
    for (k, v) in &inventory_before {
        assert_eq!(after.get(k), Some(v), "{k}");
    }
    assert_eq!(fs::read(l.base.join("canary.txt")).unwrap(), b"untouched");
    assert_eq!(fs::read(input.join(JQ_FILE)).unwrap(), b.bytes);
}

#[test]
fn b3_a_removal_never_reopens_the_installation_and_a_stale_offer_cannot_use_a_spent_allowance() {
    let (l, b, _k, input) = b2_lab(b"b3 remove");
    // an offer opened BEFORE the installation, on another service (another process): it will go stale
    let other = b2_restart(&l, &b, &input);
    let stale = prepare_on(&other, &l, Action::Install).unwrap();
    assert_eq!(jq_install(&l).outcome, "completed");
    // removal is a separate preview and a separate consent
    let rp = l.preview(JQ, Action::Uninstall);
    assert!(rp.content.effects.iter().any(|e| e.change == "remove"));
    assert_eq!(l.confirm(&rp).outcome, "completed");
    assert_eq!(record_state(&l), "consumed", "the removal did not touch the allowance");
    let st = l.state(JQ);
    assert_eq!((st.status, st.can_install, st.can_remove, st.reasons[0].code.as_str()), (Status::Removed, false, false, "pilot_allowance_consumed"), "{st:?}");
    // every way to a second installation is closed
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_PILOT_ALLOWANCE_CONSUMED");
    assert_eq!(prepare_on(&other, &l, Action::Install).unwrap_err().code, "INSTALL_PILOT_ALLOWANCE_CONSUMED");
    let r = confirm_on(&other, &l, &stale);
    assert_eq!(r.outcome, "refused", "{r:?}");
    assert!(matches!(code(&r), "preview_stale" | "pilot_allowance_consumed"), "{r:?}");
    assert!(!jq_artifact(&l).exists());
    // a restart changes nothing about it
    let svc3 = b2_restart(&l, &b, &input);
    let st3 = state_on(&svc3, &l);
    assert_eq!((st3.status, st3.can_install), (Status::Removed, false), "{st3:?}");
    assert_eq!(prepare_on(&svc3, &l, Action::Install).unwrap_err().code, "INSTALL_PILOT_ALLOWANCE_CONSUMED");
    assert_eq!(receipts_json(&l).len(), 2, "one installation and one removal, nothing else");
    assert_eq!(l.events().iter().filter(|e| e["eventType"] == "install" && e["outcome"] == "passed").count(), 1);
    // recovery neither reopens nor deletes anything
    let snapshot = tree_without_database(&l.base);
    l.with(|c| svc3.recover(c)).unwrap();
    assert_eq!(tree_without_database(&l.base), snapshot);
    assert_eq!(record_state(&l), "consumed");
}

#[test]
fn b3_eight_services_with_their_own_consent_books_race_for_the_one_installation_and_exactly_one_wins() {
    let (l, b, _k, input) = b2_lab(b"b3 race");
    let svcs: Vec<Arc<InstallService>> = (0..8).map(|_| Arc::new(b2_restart(&l, &b, &input))).collect();
    // eight valid, independent previews (each service has its own consent book, as eight processes would)
    let previews: Vec<InstallPreview> = svcs.iter().map(|s| prepare_on(s, &l, Action::Install).unwrap()).collect();
    let l = Arc::new(l);
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let handles: Vec<_> = svcs
        .into_iter()
        .zip(previews)
        .map(|(svc, p)| {
            let (l, bar) = (l.clone(), barrier.clone());
            std::thread::spawn(move || {
                bar.wait();
                confirm_on(&svc, &l, &p)
            })
        })
        .collect();
    let results: Vec<InstallResult> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.outcome == "completed").count(), 1, "{results:?}");
    assert!(results.iter().filter(|r| r.outcome != "completed").all(|r| r.outcome == "refused"), "{results:?}");
    assert_eq!(record_state(&l), "consumed");
    assert_eq!(serde_json::from_str::<Record>(&record_raw(&l).unwrap()).unwrap().attempts, 1, "no loser ever held the allowance");
    assert_eq!(receipts_json(&l).len(), 1);
    assert_eq!(l.events().iter().filter(|e| e["eventType"] == "install" && e["outcome"] == "passed").count(), 1);
    assert_eq!(fs::read(jq_artifact(&l)).unwrap(), b.bytes);
    assert_eq!(l.state(JQ).status, Status::InstalledVerified);
}

#[test]
fn b3_a_crash_at_any_point_ends_either_installed_and_consumed_or_unresolved_and_never_open() {
    for point in [
        Point::AfterLock,
        Point::AfterSnapshot,
        Point::StageAfterCreate,
        Point::DuringStageWrite,
        Point::AfterStageWrite,
        Point::AfterVerify,
        Point::AfterVersionDirCreate,
        Point::BeforePointerWrite,
        Point::PointerAfterCreate,
        Point::AfterPointerSwitch,
        Point::BeforeReceiptWrite,
        Point::AfterReceiptWrite,
        Point::BeforeJournalCommit,
    ] {
        let (keep, input) = input_dir();
        let b = b2(b"b3 crash");
        fs::write(input.join(JQ_FILE), &b.bytes).unwrap();
        let hook: FaultHook = Arc::new(move |p| (p == point).then_some(Fault::Crash));
        let l = lab_armed(lab_with(b2_sources(&b, &input, true), Some(hook)));
        l.with(|c| jq_pilot::seed_catalog(c).unwrap());
        let p = l.preview(JQ, Action::Install);
        let r = l.confirm(&p);
        assert_ne!(r.outcome, "completed", "{point:?}: {r:?}");
        // the process is gone: a new one starts, recovers, and looks
        let svc = b2_restart(&l, &b, &input);
        l.with(|c| svc.recover(c)).unwrap();
        let st = state_on(&svc, &l);
        let state = record_state(&l);
        match st.status {
            Status::InstalledVerified => assert_eq!(state, "consumed", "{point:?}: installed means spent"),
            _ => {
                assert_eq!(state, "reserved", "{point:?}: not installed but never open either: {st:?}");
                assert_eq!(st.status, Status::FailedNeedsAttention, "{point:?}: {st:?}");
                assert!(!st.can_install && !st.can_remove);
            }
        }
        // whatever happened, a SECOND installation is impossible
        assert!(prepare_on(&svc, &l, Action::Install).is_err(), "{point:?}");
        assert_eq!(fs::read(l.base.join("canary.txt")).unwrap(), b"untouched");
        assert_eq!(fs::read(input.join(JQ_FILE)).unwrap(), b.bytes, "{point:?}: the retained file is untouched");
        // and a repeat of the same recovery / restart neither resets nor changes it
        let again = b2_restart(&l, &b, &input);
        l.with(|c| again.recover(c)).unwrap();
        assert_eq!(record_state(&l), state, "{point:?}");
        drop(keep);
    }
}

#[test]
fn b3_a_crash_after_the_commit_is_rolled_forward_and_the_allowance_is_consumed_from_the_receipt() {
    let (keep, input) = input_dir();
    let b = b2(b"b3 roll forward");
    fs::write(input.join(JQ_FILE), &b.bytes).unwrap();
    let hook: FaultHook = Arc::new(|p| (p == Point::BeforeJournalCommit).then_some(Fault::Crash));
    let l = lab_armed(lab_with(b2_sources(&b, &input, true), Some(hook)));
    l.with(|c| jq_pilot::seed_catalog(c).unwrap());
    let p = l.preview(JQ, Action::Install);
    assert_ne!(l.confirm(&p).outcome, "completed");
    assert_eq!(record_state(&l), "reserved");
    let svc = b2_restart(&l, &b, &input);
    let rec = l.with(|c| svc.recover(c)).unwrap();
    let st = state_on(&svc, &l);
    if st.status == Status::InstalledVerified {
        // rolled forward: the committed receipt of THAT reservation settles it as consumed, once
        assert!(rec.notes.iter().any(|n| n.contains("Tek seferlik izin")), "{rec:?}");
        assert_eq!(record_state(&l), "consumed");
        assert!(l.events().iter().any(|e| e["checkId"] == "pilot-allowance-reconciled"));
        assert!(prepare_on(&svc, &l, Action::Install).is_err());
    } else {
        // rolled back instead: then it is NOT silently reopened
        assert_eq!(record_state(&l), "reserved", "{st:?}");
    }
    drop(keep);
}

#[test]
fn b3_a_release_needs_its_own_audit_row_and_without_it_the_allowance_stays_reserved() {
    let (keep, input) = input_dir();
    let b = b2(b"b3 release audit");
    fs::write(input.join(JQ_FILE), &b.bytes).unwrap();
    let hook: FaultHook = Arc::new(|p| (p == Point::AfterStageWrite).then_some(Fault::Fail));
    let l = lab_armed(lab_with(b2_sources(&b, &input, true), Some(hook)));
    l.with(|c| jq_pilot::seed_catalog(c).unwrap());
    fail_audit_when(&l, "rollback", "refuse_release_row");
    let r = jq_install(&l);
    assert_eq!(r.outcome, "failed", "{r:?}");
    assert!(r.message.contains("SERBEST BIRAKILMADI"), "{}", r.message);
    assert_eq!(record_state(&l), "reserved", "no audit row, no release");
    let st = l.state(JQ);
    assert_eq!((st.status, st.reasons[0].code.as_str()), (Status::FailedNeedsAttention, "pilot_allowance_unresolved"), "{st:?}");
    assert!(l.try_preview(JQ, Action::Install).is_err());
    // nothing resets it later either: not a restart, not a recovery, not the audit trail coming back
    drop_trigger(&l, "refuse_release_row");
    let svc = b2_restart(&l, &b, &input);
    l.with(|c| svc.recover(c)).unwrap();
    assert_eq!(record_state(&l), "reserved");
    assert!(prepare_on(&svc, &l, Action::Install).is_err());
    drop(keep);
}

#[test]
fn b3_a_corrupted_missing_forged_or_foreign_allowance_state_fails_closed_and_is_never_repaired() {
    let (l, b, _k, input) = b2_lab(b"b3 corrupt");
    assert_eq!(jq_install(&l).outcome, "completed");
    let good = record_raw(&l).unwrap();
    let snapshot = tree_without_database(&l.base);
    let expect_attention = |l: &Lab, why: &str, code: &str| {
        let st = l.state(JQ);
        assert_eq!((st.status, st.reasons[0].code.as_str()), (Status::FailedNeedsAttention, code), "{why}: {st:?}");
        assert!(!st.can_install && !st.can_remove, "{why}");
        assert!(l.try_preview(JQ, Action::Install).is_err() && l.try_preview(JQ, Action::Uninstall).is_err(), "{why}");
    };
    // damaged / edited records
    for (why, bad) in [("not json", "{".to_string()), ("emptied", String::new()), ("state edited", good.replace("\"consumed\"", "\"released\"")), ("count edited", good.replace("\"attempts\":1", "\"attempts\":0"))] {
        put_record(&l, &bad);
        expect_attention(&l, why, "pilot_allowance_state_invalid");
    }
    // a record that is internally valid (digest recomputed) but says "released" while the receipts and the audit trail say "installed"
    let mut forged: Record = serde_json::from_str(&good).unwrap();
    forged.state = "released".into();
    put_record(&l, &pilot_allowance::write_record(forged));
    expect_attention(&l, "forged release", "pilot_allowance_state_contradicts_receipts");
    // a record that belongs to another data directory / computer (a copied database)
    let mut other_dir: Record = serde_json::from_str(&good).unwrap();
    other_dir.data_dir_sha256 = "f".repeat(64);
    put_record(&l, &pilot_allowance::write_record(other_dir));
    expect_attention(&l, "another directory", "pilot_allowance_state_for_another_installation");
    let mut other_pc: Record = serde_json::from_str(&good).unwrap();
    other_pc.computer_id = "computer-other".into();
    put_record(&l, &pilot_allowance::write_record(other_pc));
    expect_attention(&l, "another computer", "pilot_allowance_state_for_another_installation");
    // the record deleted: the receipts and the audit trail still say it was installed
    delete_record(&l);
    expect_attention(&l, "deleted record", "pilot_allowance_record_missing");
    // the receipt deleted or altered as well: the record (restored) still says consumed, and the installed state itself needs attention
    put_record(&l, &good);
    assert_eq!(l.state(JQ).status, Status::InstalledVerified);
    let receipt_file = fs::read_dir(l.root().join("receipts")).unwrap().flatten().next().unwrap().path();
    let receipt_text = fs::read_to_string(&receipt_file).unwrap();
    fs::write(&receipt_file, receipt_text.replace("\"committed\"", "\"started\"")).unwrap();
    let altered = l.state(JQ);
    assert_eq!(altered.status, Status::FailedNeedsAttention, "{altered:?}");
    assert!(l.try_preview(JQ, Action::Install).is_err());
    fs::write(&receipt_file, &receipt_text).unwrap();
    assert_eq!(l.state(JQ).status, Status::InstalledVerified);
    fs::remove_file(&receipt_file).unwrap();
    let gone = l.state(JQ);
    assert_eq!(gone.status, Status::FailedNeedsAttention, "{gone:?}");
    assert_eq!(record_raw(&l).as_deref(), Some(good.as_str()), "the record itself was not touched");
    // receipt AND record gone: the audit trail alone still refuses to call the allowance open
    delete_record(&l);
    fs::remove_dir_all(l.root().join("journal")).ok();
    let st = l.state(JQ);
    assert_eq!(st.status, Status::FailedNeedsAttention, "{st:?}");
    assert!(l.try_preview(JQ, Action::Install).is_err());
    // nothing was deleted or repaired by looking at any of it, and no new installation happened
    assert_eq!(fs::read(jq_artifact(&l)).unwrap(), b.bytes);
    assert_eq!(l.events().iter().filter(|e| e["eventType"] == "install" && e["outcome"] == "passed").count(), 1);
    let _ = (snapshot, input);
}

#[test]
fn b3_the_pilot_is_bound_to_one_recipe_one_artifact_one_data_directory_and_inactive_until_armed() {
    let (keep, input) = input_dir();
    let b = b2(b"b3 binding");
    fs::write(input.join(JQ_FILE), &b.bytes).unwrap();
    let probe = |tweak: &dyn Fn(&mut Sources, &Path)| {
        let mut l = lab_armed(lab_with(b2_sources(&b, &input, true), None));
        let dir = l.data.clone();
        tweak(&mut l.svc.sources, &dir);
        l.with(|c| jq_pilot::seed_catalog(c).unwrap());
        let st = l.state(JQ);
        let preview = l.try_preview(JQ, Action::Install);
        (st, preview.map(|_| ()).map_err(|e| e.code), l)
    };
    // armed for THIS directory: available
    let (st, pv, _l) = probe(&|_, _| {});
    assert_eq!((st.status, pv), (Status::Available, Ok(())));
    // not armed: inactive, decided before any byte of the retained file is read
    let (st, pv, l) = probe(&|s, _| s.one_time.as_mut().unwrap().armed_data_dir_sha256 = None);
    assert_eq!((st.status, st.reasons[0].code.as_str()), (Status::VerificationIncomplete, "pilot_not_armed"));
    assert_eq!(pv, Err("INSTALL_NOT_AVAILABLE"));
    assert!(!l.data.join("atm-managed").exists());
    // armed for another data directory
    let (st, _, l) = probe(&|s, _| s.one_time.as_mut().unwrap().armed_data_dir_sha256 = Some(crate::pilot_allowance::data_dir_digest(Path::new("c:\\somewhere\\else"))));
    assert_eq!((st.status, st.reasons[0].code.as_str()), (Status::VerificationIncomplete, "pilot_wrong_data_directory"));
    assert!(!l.data.join("atm-managed").exists());
    // an allowance for another recipe digest or another artifact does not cover this recipe (and a real recipe without cover is not installable)
    for which in 0..4 {
        let (st, pv, l) = probe(&|s, _| {
            let p = s.one_time.as_mut().unwrap();
            match which {
                0 => p.recipe_digest = "0".repeat(64),
                1 => p.artifact_sha256 = "0".repeat(64),
                2 => p.version = "9.9.9".into(),
                _ => p.platform_target = "linux-x64".into(),
            }
        });
        assert_eq!((st.status, st.reasons[0].code.as_str()), (Status::VerificationIncomplete, "pilot_allowance_not_for_this_recipe"), "tweak {which}");
        assert!(pv.is_err() && !l.data.join("atm-managed").exists(), "tweak {which}");
    }
    drop(keep);
}

/// Counts every request for bytes: an inactive or consumed pilot must not even ask the provider.
struct CountingProvider(Arc<std::sync::atomic::AtomicUsize>);

impl ArtifactProvider for CountingProvider {
    fn bytes(&self, _: &str, _: &str, _: &str) -> Result<Vec<u8>, &'static str> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err("artifact_not_available")
    }
}

#[test]
fn b3_an_inactive_or_spent_pilot_never_asks_the_provider_for_a_single_byte() {
    let (keep, input) = input_dir();
    let b = b2(b"b3 provider untouched");
    fs::write(input.join(JQ_FILE), &b.bytes).unwrap();
    let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    // not armed
    let mut s = b2_sources(&b, &input, true);
    s.artifacts = Box::new(CountingProvider(asked.clone()));
    let l = lab_with(s, None);
    l.with(|c| jq_pilot::seed_catalog(c).unwrap());
    assert_eq!(l.state(JQ).reasons[0].code, "pilot_not_armed");
    assert!(l.try_preview(JQ, Action::Install).is_err());
    let r = l.svc.confirm(&l.db, &ConfirmRequest { offer_id: "a".repeat(64), preview_digest: "b".repeat(64), acknowledged: true });
    assert_eq!(r.outcome, "refused");
    assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 0);
    // spent: after the one installation the provider is asked at most for that installation (twice: preview and confirmation), never again
    let (l2, _b2, _k2, _i2) = b2_lab(b"b3 spent");
    assert_eq!(jq_install(&l2).outcome, "completed");
    assert_eq!(jq_remove(&l2).outcome, "completed");
    let mut svc = b2_restart(&l2, &b2(b"b3 spent"), &_i2);
    svc.sources.artifacts = Box::new(CountingProvider(asked.clone()));
    assert_eq!(prepare_on(&svc, &l2, Action::Install).unwrap_err().code, "INSTALL_PILOT_ALLOWANCE_CONSUMED");
    assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 0, "a spent pilot never reads the retained file again");
    drop(keep);
}

#[test]
fn b3_the_allowance_has_no_reset_path_no_credential_framework_no_process_network_or_environment() {
    let allowance = include_str!("pilot_allowance.rs");
    let production = allowance.split("#[cfg(test)]\n#[path").next().unwrap();
    let code: String = production.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    for needle in [
        "DELETE", "DROP", "reset", "clear", "remove_", "std::fs", "std::env", "env::var", "std::process", "Command::new", "std::net", "reqwest", "KeyCredential",
        "os_hello", "os_approval", "device_trust", "approval_store", "TPM", "biometric",
    ] {
        assert!(!code.contains(needle), "pilot_allowance.rs must not contain {needle}");
    }
    // the only writes are the two compare-and-swap statements
    assert_eq!(code.matches("INSERT INTO settings").count(), 1);
    assert_eq!(code.matches("UPDATE settings").count(), 1);
    // the arming constant: declared once, `None` until the commit that records the user's authorization, used once to build the application's supply
    let pilot = include_str!("jq_pilot.rs");
    let pilot_code: String = pilot.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    let (disarmed, armed) = (
        "pub const B3_ARMED_DATA_DIR_SHA256: Option<&str> = None;".to_string(),
        format!("pub const B3_ARMED_DATA_DIR_SHA256: Option<&str> = Some(\"{PILOT_DIR_DIGEST}\");"),
    );
    assert_eq!(pilot_code.matches(&disarmed).count() + pilot_code.matches(&armed).count(), 1, "exactly one declaration: None, or the one authorized digest");
    assert_eq!(pilot_code.lines().filter(|l| l.contains("B3_ARMED_DATA_DIR_SHA256")).count(), 2, "declaration + the one use in sources_for_app");
    // the allowance is reachable from the interface only through the install service (the WebView names no allowance, path or digest)
    for (name, text) in [("commands.rs", include_str!("commands.rs")), ("state.rs", include_str!("state.rs")), ("lib.rs", include_str!("lib.rs"))] {
        assert!(!text.replace("mod pilot_allowance;", "").contains("pilot_allowance"), "{name}");
    }
    // a release supply carries no one-time rule at all
    assert!(Sources::production().one_time.is_none());
}

#[test]
fn b3_the_activation_candidate_is_unlocked_but_inactive_and_never_asks_for_a_byte_of_a_retained_file() {
    let (_keep, input) = input_dir();
    let tmp = tempfile::tempdir().unwrap();
    let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    // exactly what the jq-pilot build starts with (the committed files, the two constants), with the provider replaced by a counter
    let mut s = jq_pilot::sources_for_app(tmp.path().join(jq_pilot::INPUT_FOLDER));
    s.artifacts = Box::new(CountingProvider(asked.clone()));
    let l = lab_with(s, None);
    l.with(|c| jq_pilot::seed_catalog(c).unwrap());
    // even a file the person would have placed at the fixed place cannot matter: it is not asked for
    fs::write(input.join(JQ_FILE), b"a stand-in, not jq").unwrap();
    let st = l.state(JQ);
    if cfg!(windows) {
        let code = expected_refusal_in_a_foreign_directory();
        assert_eq!((st.status, st.reasons[0].code.as_str()), (Status::VerificationIncomplete, code), "{st:?}");
        assert_eq!(l.state(JQ).reasons[0].text, reason_text(code));
    } else {
        assert_eq!(st.reasons[0].code, "no_reviewed_recipe", "off Windows the pilot supply is empty");
    }
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_NOT_AVAILABLE");
    assert_eq!(l.try_preview(JQ, Action::Uninstall).unwrap_err().code, "INSTALL_NOT_AVAILABLE");
    let r = l.svc.confirm(&l.db, &ConfirmRequest { offer_id: "a".repeat(64), preview_digest: "b".repeat(64), acknowledged: true });
    assert_eq!(r.outcome, "refused");
    assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 0, "the inactive candidate never asks the provider for a single byte");
    assert!(!l.data.join("atm-managed").exists() && !l.data.join(jq_pilot::INPUT_FOLDER).exists());
    if cfg!(windows) {
        assert!(record_raw(&l).is_none(), "no allowance record was created");
    } else {
        assert!(l.svc.sources.one_time.is_none(), "off Windows the supply is the empty release supply: no one-time rule exists");
    }
}

// =====================================================================================================================================
// Phase 3D-6: the arming. The allowance is armed for ONE data directory (a digest); every other location is refused before anything is read.
// =====================================================================================================================================

fn is_lower_hex_64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[test]
fn b3_the_armed_digest_is_one_scope_identifier_that_no_ordinary_location_matches() {
    // the normalisation: separators, case and a trailing separator never matter; a different folder always does
    let sample = r"C:\Somebody\Pilot-Folder";
    let d = pilot_allowance::data_dir_digest(Path::new(sample));
    for same in ["c:/somebody/pilot-folder", r"C:\SOMEBODY\PILOT-FOLDER\", "c:/Somebody/Pilot-Folder/"] {
        assert_eq!(pilot_allowance::data_dir_digest(Path::new(same)), d, "{same}");
    }
    for other in [r"C:\Somebody\Pilot-Folder2", r"C:\Somebody", r"C:\Somebody\Pilot-Folder\jq-pilot-input", r"D:\Somebody\Pilot-Folder", r"\?\C:\Somebody\Pilot-Folder"] {
        assert_ne!(pilot_allowance::data_dir_digest(Path::new(other)), d, "{other}");
    }
    let Some(armed) = jq_pilot::B3_ARMED_DATA_DIR_SHA256 else { return };
    assert_eq!(armed, PILOT_DIR_DIGEST, "the only digest the allowance may ever be armed for");
    assert!(is_lower_hex_64(armed));
    // no ordinary place on this computer is the armed one: not the repository's work tree or what surrounds it, not the temporary directory
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work_tree = crate_dir.parent().unwrap();
    for p in [crate_dir, work_tree, work_tree.parent().unwrap(), &std::env::temp_dir(), &crate_dir.join("target"), &work_tree.join("jq-pilot-input")] {
        assert_ne!(pilot_allowance::data_dir_digest(p), armed, "{}", p.display());
    }
    let tmp = tempfile::tempdir().unwrap();
    assert_ne!(pilot_allowance::data_dir_digest(tmp.path()), armed);
}

/// LOCAL check, not run in CI: `ATM_B3_DIR_CHECK=<the directory the user named>` makes this assert that the armed digest is exactly that directory's, whatever
/// spelling of it is used, and that its neighbours are not. (The path is never written in the repository; the person or the reviewer supplies it.)
#[test]
fn b3_the_directory_named_in_the_environment_is_exactly_the_armed_one() {
    let Some(named) = std::env::var_os("ATM_B3_DIR_CHECK") else {
        eprintln!("ATM_B3_DIR_CHECK is not set: nothing to compare (a local check; it is not part of CI)");
        return;
    };
    let Some(armed) = jq_pilot::B3_ARMED_DATA_DIR_SHA256 else { return };
    let text = named.to_string_lossy().to_string();
    let digest = |s: &str| pilot_allowance::data_dir_digest(Path::new(s));
    assert_eq!(digest(&text), armed, "the named directory is the armed one");
    for spelled in [text.replace('\\', "/"), text.to_uppercase(), format!("{text}\\"), format!("{}/", text.replace('\\', "/"))] {
        assert_eq!(digest(&spelled), armed, "{spelled}");
    }
    for other in [format!("{text}2"), format!("{text}\\jq-pilot-input"), format!("{text}\\atm-managed"), format!("{text}x")] {
        assert_ne!(digest(&other), armed, "{other}");
    }
    let parent = Path::new(&text).parent().expect("a directory with a parent");
    assert_ne!(pilot_allowance::data_dir_digest(parent), armed, "the parent directory is not the armed one");
}

#[test]
fn b3_the_real_supply_is_bound_to_the_reviewed_pilot_and_refuses_every_other_data_directory_before_reading_anything() {
    let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    if cfg!(windows) {
        // the application's supply: exactly the reviewed binding, and the armed digest exactly as declared
        let s = jq_pilot::sources_for_app(PathBuf::from("not-used"));
        let ot = s.one_time.as_ref().expect("the pilot supply carries the one-time rule");
        assert_eq!((ot.tool_id.as_str(), ot.version.as_str(), ot.platform_target.as_str()), ("jq", "1.8.2", "windows-x64"));
        assert_eq!((ot.recipe_digest.clone(), ot.artifact_sha256.clone()), (real_recipe_digest(), real_artifact_sha()));
        assert_eq!(ot.armed_data_dir_sha256.as_deref(), jq_pilot::B3_ARMED_DATA_DIR_SHA256);
        assert_eq!(s.pilot_gate.len(), 1, "unlocked for exactly one recipe");
    }
    // three different isolated data directories, each with the real supply and a provider that counts requests
    for n in 0..3 {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = jq_pilot::sources_for_app(tmp.path().join(jq_pilot::INPUT_FOLDER));
        s.artifacts = Box::new(CountingProvider(asked.clone()));
        let l = lab_with(s, None);
        l.with(|c| jq_pilot::seed_catalog(c).unwrap());
        let st = l.state(JQ);
        if cfg!(windows) {
            let code = expected_refusal_in_a_foreign_directory();
            assert_eq!((st.status, st.reasons[0].code.as_str()), (Status::VerificationIncomplete, code), "directory {n}: {st:?}");
            assert_eq!(st.reasons[0].text, reason_text(code));
            assert!(!st.can_install && !st.can_remove);
        }
        assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_NOT_AVAILABLE");
        assert_eq!(l.try_preview(JQ, Action::Uninstall).unwrap_err().code, "INSTALL_NOT_AVAILABLE");
        let r = l.svc.confirm(&l.db, &ConfirmRequest { offer_id: "a".repeat(64), preview_digest: "b".repeat(64), acknowledged: true });
        assert_eq!(r.outcome, "refused");
        assert!(!l.data.join("atm-managed").exists() && !l.data.join(jq_pilot::INPUT_FOLDER).exists(), "directory {n}: nothing is created");
        assert!(record_raw_in(&l).is_none(), "directory {n}: no allowance record exists");
    }
    assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 0, "no other data directory makes the supply ask for a single byte");
}

/// `record_raw` for a lab whose supply may be the empty one (off Windows): no rule, no record.
fn record_raw_in(l: &Lab) -> Option<String> {
    l.svc.sources.one_time.as_ref().and_then(|_| record_raw(l))
}
