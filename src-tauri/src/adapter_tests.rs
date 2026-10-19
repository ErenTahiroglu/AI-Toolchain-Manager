//! Phase 3C tests of the isolated, test-mode, one-artifact adapter on the handle-relative file-system layer. Everything runs in
//! temporary directories with made-up artifacts and fake key/presence/ledger/audit doubles: nothing is downloaded, started or
//! installed, and no real approval exists. Windows and macOS run the same behavioural tests against their own `managed_fs`.

use super::*;
use crate::{
    approval::{Action, Approval},
    device_trust::{
        fakes::{FakeKeyStore, FakePresence, InMemoryLedger, VecAudit},
        Presence,
    },
    synthetic::testkit::*,
};
use chrono::{DateTime, Utc};
use std::{fs, path::PathBuf};

const COMPUTER: &str = "synthetic-computer-1";
const TOOL: &str = "synthetic-tool";

struct Lab {
    _tmp: tempfile::TempDir,
    base: PathBuf,
    anchor: crate::managed_root::AppDataAnchor,
    conn: rusqlite::Connection,
    root: ManagedRoot,
    v1: Fx,
    v2: Fx,
    refs: ReferenceSet,
    keys: FakeKeyStore,
    ledger: InMemoryLedger,
    audit: VecAudit,
}

fn lab_in(sub: &str) -> Lab {
    crate::managed_fs::hook::clear();
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap().join(sub);
    let data = base.join("appdata");
    fs::create_dir_all(&data).unwrap();
    fs::write(base.join("canary.txt"), b"untouched").unwrap();
    let anchor = crate::managed_root::AppDataAnchor::from_resolved_data_dir(data);
    let conn = crate::db::open_file(&base.join("atm.sqlite"), None).unwrap();
    let root = ManagedRoot::establish(&anchor, &conn, COMPUTER, "2026-10-18T10:00:00Z").unwrap();
    let v1 = fx(TOOL, "1.0.0", b"payload one");
    let v2 = fx(TOOL, "2.0.0", b"payload two");
    let mut refs = v1.sc.refs.clone();
    refs.authenticated.extend(v2.sc.refs.authenticated.clone());
    Lab {
        _tmp: tmp,
        base,
        anchor,
        conn,
        root,
        v1,
        v2,
        refs,
        keys: FakeKeyStore::new("fake-key-1"),
        ledger: InMemoryLedger::default(),
        audit: VecAudit::default(),
    }
}

fn lab() -> Lab {
    lab_in("base")
}

fn clock() -> impl Fn() -> DateTime<Utc> + Sync {
    t0
}

type Auth = (crate::approval::VerifiedApproval, DeviceApproval);

impl Lab {
    fn adapter<'a>(&'a self, clock: ClockFn<'a>, fault: Option<FaultFn<'a>>) -> Adapter<'a> {
        Adapter::test_mode(
            &self.root,
            &self.refs,
            this_platform(),
            clock,
            fault,
            Trust { keys: &self.keys, ledger: &self.ledger, audit: &self.audit, policy: TrustPolicy::test_doubles() },
        )
    }

    fn approval(&self, f: &Fx, action: Action, from: Option<&str>) -> Approval {
        make_approval(f, action, from, COMPUTER, t0())
    }

    fn device(&self, f: &Fx, a: &Approval) -> DeviceApproval {
        device_trust::issue(
            a,
            &f.valid.recipe().artifact.sha256,
            &f.valid.permissions_digest(),
            &self.keys,
            &FakePresence(Presence::Confirmed),
            &self.ledger,
            &self.audit,
            TrustPolicy::test_doubles(),
        )
        .unwrap()
    }

    fn auth(&self, f: &Fx, action: Action, from: Option<&str>) -> Auth {
        let a = self.approval(f, action, from);
        let v = verified(f, &a, action, from, COMPUTER, t0());
        let dev = self.device(f, &a);
        (v, dev)
    }

    fn install(&self, f: &Fx, fault: Option<FaultFn>) -> R<Outcome> {
        let (v, dev) = self.auth(f, Action::Install, None);
        let c = clock();
        self.adapter(&c, fault).install(&Authority { recipe: &f.valid, approval: &v }, &dev, &f.sc.bytes)
    }

    fn update(&self, f: &Fx, from: &str, fault: Option<FaultFn>) -> R<Outcome> {
        let (v, dev) = self.auth(f, Action::Update, Some(from));
        let c = clock();
        self.adapter(&c, fault).update(&Authority { recipe: &f.valid, approval: &v }, &dev, &f.sc.bytes)
    }

    fn uninstall(&self, f: &Fx, fault: Option<FaultFn>) -> R<Outcome> {
        let (v, dev) = self.auth(f, Action::Uninstall, None);
        let c = clock();
        self.adapter(&c, fault).uninstall(&Authority { recipe: &f.valid, approval: &v }, &dev)
    }

    fn recover(&self, fault: Option<FaultFn>) -> Vec<Recovered> {
        let c = clock();
        self.adapter(&c, fault).recover().unwrap()
    }

    fn active(&self) -> Option<String> {
        let c = clock();
        self.adapter(&c, None).active_version(TOOL).unwrap()
    }

    fn root_path(&self) -> PathBuf {
        self.base.join("appdata").join(crate::managed_root::CONTAINER).join(format!("root-{}", self.root.root_id()))
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.root_path().join(rel)
    }

    fn files(&self, dir: &str) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(self.p(dir))
            .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    fn journals(&self) -> Vec<Txn> {
        let mut v: Vec<Txn> = fs::read_dir(self.p("journal"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".json"))
            .filter_map(|e| serde_json::from_slice(&fs::read(e.path()).ok()?).ok())
            .collect();
        v.sort_by(|a, b| a.started_at.cmp(&b.started_at).then(a.txn_id.cmp(&b.txn_id)));
        v
    }

    fn assert_clean(&self) {
        assert!(self.files("staging").is_empty(), "staging: {:?}", self.files("staging"));
        assert!(self.files("snapshots").is_empty(), "snapshots: {:?}", self.files("snapshots"));
        assert!(self.files("locks").is_empty(), "locks: {:?}", self.files("locks"));
    }

    fn artifact_of(&self, version: &str) -> Option<Vec<u8>> {
        fs::read(self.p(&format!("versions/{TOOL}/{version}/artifact.bin"))).ok()
    }

    fn outside_is_untouched(&self) {
        let mut names: Vec<String> =
            fs::read_dir(&self.base).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.retain(|n| !n.starts_with("atm.sqlite"));
        names.sort();
        assert_eq!(names, vec!["appdata".to_string(), "canary.txt".to_string()]);
        assert_eq!(fs::read(self.base.join("canary.txt")).unwrap(), b"untouched");
        let data: Vec<String> =
            fs::read_dir(self.base.join("appdata")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(data, vec![crate::managed_root::CONTAINER.to_string()], "nothing else below the data directory");
    }
}

fn failed(r: R<Outcome>) -> (&'static str, RollbackReport) {
    match r {
        Err(TxnError::Failed { code, rollback }) => (code, rollback),
        other => panic!("expected a failed transaction, got {other:?}"),
    }
}

fn refused(r: R<Outcome>) -> &'static str {
    match r {
        Err(TxnError::Refused(c)) => c,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn at(point: Point, fault: Fault) -> impl Fn(Point) -> Option<Fault> + Sync {
    move |p| (p == point).then_some(fault)
}

fn replace_file(path: &std::path::Path, content: &[u8]) {
    fs::remove_file(path).unwrap();
    fs::write(path, content).unwrap();
}

fn make_dir_link(link: &std::path::Path, target: &std::path::Path) -> bool {
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

fn remove_dir_link(link: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd").args(["/C", "rmdir"]).arg(link).output();
    }
    #[cfg(unix)]
    {
        let _ = fs::remove_file(link);
    }
}

const FRESH_POINTS: &[Point] = &[
    Point::AfterLock,
    Point::AfterSnapshot,
    Point::StageBeforeCreate,
    Point::StageAfterCreate,
    Point::DuringStageWrite,
    Point::StageAfterSync,
    Point::AfterStageWrite,
    Point::AfterVerify,
    Point::BeforeVersionDirCreate,
    Point::AfterVersionDirCreate,
    Point::BeforePointerWrite,
    Point::PointerBeforeCreate,
    Point::PointerAfterCreate,
    Point::PointerAfterSync,
    Point::AfterPointerSwitch,
    Point::BeforeReceiptWrite,
    Point::ReceiptAfterCreate,
    Point::ReceiptAfterSync,
    Point::AfterReceiptWrite,
    Point::BeforeJournalCommit,
];

const UPDATE_EXTRA: &[Point] = &[
    Point::SnapshotBeforeCreate,
    Point::SnapshotAfterCreate,
    Point::SnapshotDuringWrite,
    Point::SnapshotAfterSync,
    Point::SnapshotAfterRecord,
];

/// Crash points inside the window between an exclusive create and the durable identity record.
fn ambiguous_window(p: Point) -> bool {
    matches!(p, Point::StageAfterCreate | Point::PointerAfterCreate | Point::ReceiptAfterCreate | Point::SnapshotAfterCreate)
}

/// Crash points after which the work is complete except for the final bookkeeping.
fn rolls_forward(p: Point) -> bool {
    matches!(p, Point::ReceiptAfterSync | Point::AfterReceiptWrite | Point::BeforeJournalCommit)
}

// ---- production is closed ----------------------------------------------------------------------------------------------------------

#[test]
fn the_production_adapter_refuses_everything_and_creates_nothing() {
    let l = lab();
    let c = clock();
    let p = Adapter::production(
        &l.root,
        &l.refs,
        this_platform(),
        &c,
        Trust { keys: &l.keys, ledger: &l.ledger, audit: &l.audit, policy: TrustPolicy::production() },
    );
    let (v, dev) = l.auth(&l.v1, Action::Install, None);
    let authority = Authority { recipe: &l.v1.valid, approval: &v };
    assert_eq!(refused(p.install(&authority, &dev, &l.v1.sc.bytes)), "production_adapter_not_enabled");
    assert_eq!(refused(p.update(&authority, &dev, &l.v1.sc.bytes)), "production_adapter_not_enabled");
    assert_eq!(refused(p.uninstall(&authority, &dev)), "production_adapter_not_enabled");
    assert!(matches!(
        p.preflight(&authority, Action::Install, Some(&l.v1.sc.bytes)),
        Err(TxnError::Refused("production_adapter_not_enabled"))
    ));
    assert!(matches!(p.recover(), Err(TxnError::Refused("production_adapter_not_enabled"))));
    assert!(matches!(p.active_version(TOOL), Err(TxnError::Refused("production_adapter_not_enabled"))));
    assert!(l.files("journal").is_empty() && l.files("locks").is_empty());
    l.outside_is_untouched();
    // the approval was never consumed by the refusing adapter
    assert_eq!(l.ledger.status(&dev.approval.approval_id).unwrap(), Some(device_trust::LedgerStatus::Issued));
    // and the production lists are empty: nothing real can be authenticated or reviewed
    assert!(crate::discovery::reference::ReferenceSet::default().authenticated.is_empty());
    assert!(crate::recipe::RecipeRegistry::production().len() == 0);
}

#[test]
fn a_fake_key_store_cannot_authorize_even_the_test_adapter_under_the_production_trust_policy() {
    let l = lab();
    let (v, dev) = l.auth(&l.v1, Action::Install, None);
    let c = clock();
    let a = Adapter::test_mode(
        &l.root,
        &l.refs,
        this_platform(),
        &c,
        None,
        Trust { keys: &l.keys, ledger: &l.ledger, audit: &l.audit, policy: TrustPolicy::production() },
    );
    assert_eq!(refused(a.install(&Authority { recipe: &l.v1.valid, approval: &v }, &dev, &l.v1.sc.bytes)), "approval_key_not_os_protected");
    assert_eq!(l.active(), None);
    l.assert_clean();
    assert!(l.journals().iter().all(|j| j.state == TxnState::RolledBack), "the aborted transaction is closed");
    assert!(l.files("receipts").is_empty() && !l.p(&format!("versions/{TOOL}")).exists());
}

#[test]
fn a_real_catalog_entry_or_a_non_synthetic_recipe_never_passes_the_trust_gate() {
    let l = lab();
    let mut real_looking = l.v1.valid.recipe().clone();
    real_looking.synthetic = false;
    real_looking.tool_id = "gitleaks".to_string();
    let mut rr = l.v1.sc.reviewed.clone();
    rr.recipe = real_looking;
    crate::synthetic::rereview(&mut rr);
    let mut refs = l.refs.clone();
    refs.authenticated.push(crate::discovery::reference::ReferenceDigest {
        entry_id: "gitleaks".to_string(),
        version: "1.0.0".to_string(),
        ..l.refs.authenticated[0].clone()
    });
    rr.recipe.source.source_attested = false;
    rr.recipe.authentication.retain(|e| e.kind != "synthetic_source_attestation");
    crate::synthetic::rereview(&mut rr);
    if let Ok(v) = crate::recipe::validate(&rr, &refs) {
        let (ap, dev) = l.auth(&l.v1, Action::Install, None);
        let c = clock();
        let r = l.adapter(&c, None).install(&Authority { recipe: &v, approval: &ap }, &dev, &l.v1.sc.bytes);
        assert_eq!(refused(r), "not_a_synthetic_recipe");
    }
    assert!(l.files("journal").is_empty() && l.active().is_none());
    l.outside_is_untouched();
}

// ---- the lifecycle -----------------------------------------------------------------------------------------------------------------

#[test]
fn preflight_is_offline_and_changes_nothing() {
    let l = lab();
    let (v, _dev) = l.auth(&l.v1, Action::Install, None);
    let c = clock();
    let a = l.adapter(&c, None);
    let before = (l.files("journal"), l.files("staging"), l.files("state"), l.files("locks"));
    let pf = a.preflight(&Authority { recipe: &l.v1.valid, approval: &v }, Action::Install, Some(&l.v1.sc.bytes)).unwrap();
    assert_eq!((pf.tool_id.as_str(), pf.version.as_str(), pf.installed_version, pf.already_applied), (TOOL, "1.0.0", None, false));
    assert_eq!(pf.artifact_sha256.as_deref(), Some(l.v1.valid.recipe().artifact.sha256.as_str()));
    assert_eq!(before, (l.files("journal"), l.files("staging"), l.files("state"), l.files("locks")));
    // every wrong artifact is refused before anything is started
    let good = l.v1.sc.bytes.clone();
    let mut flipped = good.clone();
    *flipped.last_mut().unwrap() ^= 1;
    for bytes in [flipped, good[..good.len() - 1].to_vec(), vec![], l.v2.sc.bytes.clone()] {
        let r = a.preflight(&Authority { recipe: &l.v1.valid, approval: &v }, Action::Install, Some(&bytes));
        assert!(matches!(r, Err(TxnError::Refused("artifact_does_not_match_recipe"))));
    }
    // a machine without an authenticated reference for the right bytes
    let empty = ReferenceSet::default();
    let none = Adapter::test_mode(
        &l.root,
        &empty,
        this_platform(),
        &c,
        None,
        Trust { keys: &l.keys, ledger: &l.ledger, audit: &l.audit, policy: TrustPolicy::test_doubles() },
    );
    let r = none.preflight(&Authority { recipe: &l.v1.valid, approval: &v }, Action::Install, Some(&good));
    assert!(matches!(r, Err(TxnError::Refused("artifact_not_authenticated_for_this_platform"))));
    // the other platform's artifact and recipe
    let foreign = fx_on(other_platform(), TOOL, "1.0.0", b"payload one");
    let fa = {
        let a = make_approval(&foreign, Action::Install, None, COMPUTER, t0());
        verified(&foreign, &a, Action::Install, None, COMPUTER, t0())
    };
    let r = l.adapter(&c, None).preflight(&Authority { recipe: &foreign.valid, approval: &fa }, Action::Install, Some(&foreign.sc.bytes));
    assert!(matches!(r, Err(TxnError::Refused("approval_does_not_match"))));
    let r = l.adapter(&c, None).preflight(&Authority { recipe: &l.v1.valid, approval: &v }, Action::Install, Some(&foreign.sc.bytes));
    assert!(matches!(r, Err(TxnError::Refused("artifact_does_not_match_recipe"))));
    // the presence of an artifact must match the action
    assert!(matches!(a.preflight(&Authority { recipe: &l.v1.valid, approval: &v }, Action::Install, None), Err(TxnError::Refused(_))));
    assert_eq!(before, (l.files("journal"), l.files("staging"), l.files("state"), l.files("locks")));
    l.outside_is_untouched();
}

#[test]
fn install_update_and_uninstall_work_leave_receipts_and_never_touch_the_tools_user_data() {
    let l = lab();
    // the tool's own user data exists before anything happens
    fs::create_dir_all(l.p(&format!("data/{TOOL}"))).unwrap();
    fs::write(l.p(&format!("data/{TOOL}/settings.db")), b"user data").unwrap();
    let data_before = fs::read(l.p(&format!("data/{TOOL}/settings.db"))).unwrap();

    let out = l.install(&l.v1, None).unwrap();
    assert!(!out.already_applied);
    assert_eq!((out.receipt.action, out.receipt.to_version.as_deref(), out.receipt.synthetic), (Action::Install, Some("1.0.0"), true));
    assert_eq!(out.receipt.kept, vec!["user_data".to_string()]);
    assert_eq!(receipt_digest(&out.receipt), out.receipt.receipt_digest);
    assert_eq!(l.active().as_deref(), Some("1.0.0"));
    assert_eq!(l.artifact_of("1.0.0").unwrap(), l.v1.sc.bytes);
    l.assert_clean();
    // repeating it is a no-op that returns the same receipt and creates nothing
    let journals = l.journals().len();
    let again = l.install(&l.v1, None).unwrap();
    assert!(again.already_applied && again.receipt == out.receipt);
    assert_eq!(l.journals().len(), journals);

    let up = l.update(&l.v2, "1.0.0", None).unwrap();
    assert_eq!((up.receipt.from_version.as_deref(), up.receipt.to_version.as_deref()), (Some("1.0.0"), Some("2.0.0")));
    assert_eq!(l.active().as_deref(), Some("2.0.0"));
    assert!(l.artifact_of("1.0.0").is_some() && l.artifact_of("2.0.0").is_some(), "the previous version is kept for rollback");
    l.assert_clean();

    let un = l.uninstall(&l.v2, None).unwrap();
    assert_eq!(un.receipt.action, Action::Uninstall);
    assert_eq!(l.active(), None);
    assert!(!l.p(&format!("versions/{TOOL}")).exists() && !l.p(&format!("state/{TOOL}.json")).exists());
    assert_eq!(fs::read(l.p(&format!("data/{TOOL}/settings.db"))).unwrap(), data_before, "user data is never part of any transaction");
    l.assert_clean();
    assert!(l.journals().iter().all(|j| j.state == TxnState::Committed));
    assert_eq!(l.files("receipts").len(), 3);
    // uninstalling again: nothing installed
    assert_eq!(refused(l.uninstall(&l.v2, None)), "not_installed");
    // the world is complete again
    assert!(l.install(&l.v1, None).is_ok());
    l.outside_is_untouched_but_data();
}

impl Lab {
    fn outside_is_untouched_but_data(&self) {
        assert_eq!(fs::read(self.base.join("canary.txt")).unwrap(), b"untouched");
    }
}

#[test]
fn a_repeat_with_a_valid_approval_but_the_wrong_precondition_is_refused() {
    let l = lab();
    assert_eq!(refused(l.update(&l.v2, "1.0.0", None)), "not_installed");
    l.install(&l.v1, None).unwrap();
    assert_eq!(refused(l.install(&l.v2, None)), "already_installed_use_update");
    assert_eq!(refused(l.update(&l.v2, "9.9.9", None)), "installed_version_differs_from_the_approved_update_source");
    assert_eq!(refused(l.uninstall(&l.v2, None)), "installed_version_differs_from_the_approved_uninstall");
    assert_eq!(l.active().as_deref(), Some("1.0.0"));
    l.assert_clean();
}

// ---- approval replay, expiry, computer ---------------------------------------------------------------------------------------------

#[test]
fn a_device_approval_is_single_use_and_bound_to_its_own_approval() {
    let l = lab();
    let (v, dev) = l.auth(&l.v1, Action::Install, None);
    let c = clock();
    let a = l.adapter(&c, None);
    let authority = Authority { recipe: &l.v1.valid, approval: &v };
    assert!(a.install(&authority, &dev, &l.v1.sc.bytes).is_ok());
    assert_eq!(
        l.ledger.status(&dev.approval.approval_id).unwrap(),
        Some(device_trust::LedgerStatus::Consumed { txn_id: l.journals()[0].txn_id.clone() })
    );
    // the same approval cannot start a second transaction (uninstall + reinstall): replay
    let (uv, udev) = l.auth(&l.v1, Action::Uninstall, None);
    a.uninstall(&Authority { recipe: &l.v1.valid, approval: &uv }, &udev).unwrap();
    let r = a.install(&authority, &dev, &l.v1.sc.bytes);
    assert_eq!(refused(r), "approval_replayed");
    assert_eq!(l.active(), None, "a replayed approval changed nothing");
    l.assert_clean();
    // a device approval issued for ANOTHER phase-3A approval does not fit this one
    let (v2, _own) = l.auth(&l.v1, Action::Install, None);
    let stranger = l.auth(&l.v1, Action::Install, None).1;
    assert_eq!(
        refused(a.install(&Authority { recipe: &l.v1.valid, approval: &v2 }, &stranger, &l.v1.sc.bytes)),
        "approval_binding_mismatch"
    );
    // a device approval for another version of the tool
    let other = l.auth(&l.v2, Action::Install, None).1;
    assert_eq!(refused(a.install(&authority, &other, &l.v1.sc.bytes)), "approval_binding_mismatch");
    // revoked before use
    let (v3, dev3) = l.auth(&l.v1, Action::Install, None);
    l.ledger.revoke(&dev3.approval.approval_id).unwrap();
    assert_eq!(refused(a.install(&Authority { recipe: &l.v1.valid, approval: &v3 }, &dev3, &l.v1.sc.bytes)), "approval_revoked");
    assert_eq!(l.active(), None);
    l.assert_clean();
    assert!(l.journals().iter().filter(|j| j.state != TxnState::Committed).all(|j| j.state == TxnState::RolledBack));
}

#[test]
fn expiry_computer_and_every_authority_check_stop_the_transaction_before_the_first_change() {
    let l = lab();
    let (v, dev) = l.auth(&l.v1, Action::Install, None);
    let authority = Authority { recipe: &l.v1.valid, approval: &v };
    // expired by the time the adapter runs
    let late = || t0() + chrono::Duration::hours(1);
    assert_eq!(refused(l.adapter(&late, None).install(&authority, &dev, &l.v1.sc.bytes)), "approval_expired");
    // a root that belongs to another computer
    let other = ManagedRoot::establish(&l.anchor, &l.conn, "synthetic-computer-2", "2026-10-18T10:00:00Z").unwrap();
    let c = clock();
    let on_other = Adapter::test_mode(
        &other,
        &l.refs,
        this_platform(),
        &c,
        None,
        Trust { keys: &l.keys, ledger: &l.ledger, audit: &l.audit, policy: TrustPolicy::test_doubles() },
    );
    assert_eq!(refused(on_other.install(&authority, &dev, &l.v1.sc.bytes)), "approval_does_not_match");
    // another platform claimed by the machine, a different action, a different recipe version
    let claims_other = Adapter::test_mode(
        &l.root,
        &l.refs,
        other_platform(),
        &c,
        None,
        Trust { keys: &l.keys, ledger: &l.ledger, audit: &l.audit, policy: TrustPolicy::test_doubles() },
    );
    assert_eq!(refused(claims_other.install(&authority, &dev, &l.v1.sc.bytes)), "approval_does_not_match");
    assert_eq!(refused(l.adapter(&c, None).update(&authority, &dev, &l.v1.sc.bytes)), "approval_does_not_match");
    assert_eq!(
        refused(l.adapter(&c, None).install(&Authority { recipe: &l.v2.valid, approval: &v }, &dev, &l.v2.sc.bytes)),
        "approval_does_not_match"
    );
    assert!(l.files("journal").is_empty() && l.active().is_none());
    assert_eq!(
        l.ledger.status(&dev.approval.approval_id).unwrap(),
        Some(device_trust::LedgerStatus::Issued),
        "nothing consumed the approval"
    );
    l.outside_is_untouched();
}

// ---- failures, rollback, crashes --------------------------------------------------------------------------------------------------

#[test]
fn a_failure_at_any_point_of_an_install_is_rolled_back_completely_and_a_retry_succeeds() {
    for &point in FRESH_POINTS {
        let l = lab();
        let (code, report) = failed(l.install(&l.v1, Some(&at(point, Fault::Fail))));
        assert_eq!(code, "injected_failure", "{point:?}");
        assert!(matches!(report.status.as_str(), "restored" | "not_needed") && report.not_undone.is_empty(), "{point:?}: {report:?}");
        assert_eq!(l.active(), None, "{point:?}");
        assert!(l.artifact_of("1.0.0").is_none() && !l.p(&format!("versions/{TOOL}")).exists(), "{point:?}");
        assert!(l.files("receipts").is_empty() && l.files("state").is_empty(), "{point:?}");
        l.assert_clean();
        assert_eq!(l.journals()[0].state, TxnState::RolledBack, "{point:?}");
        l.outside_is_untouched();
        assert_eq!(l.install(&l.v1, None).unwrap().receipt.to_version.as_deref(), Some("1.0.0"), "{point:?}");
    }
}

#[test]
fn a_failed_update_puts_the_previous_version_back_exactly() {
    for &point in FRESH_POINTS.iter().chain(UPDATE_EXTRA) {
        let l = lab();
        l.install(&l.v1, None).unwrap();
        let pointer_before = fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap();
        let receipts_before = l.files("receipts");
        let (_, report) = failed(l.update(&l.v2, "1.0.0", Some(&at(point, Fault::Fail))));
        assert!(matches!(report.status.as_str(), "restored" | "not_needed"), "{point:?}: {report:?}");
        assert_eq!(l.active().as_deref(), Some("1.0.0"), "{point:?}");
        assert_eq!(fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap(), pointer_before, "{point:?}: byte-identical pointer");
        assert_eq!(l.artifact_of("1.0.0").unwrap(), l.v1.sc.bytes, "{point:?}");
        assert!(l.artifact_of("2.0.0").is_none(), "{point:?}");
        assert_eq!(l.files("receipts"), receipts_before, "{point:?}");
        assert_eq!(l.files("state"), vec![format!("{TOOL}.json")], "{point:?}: no leftover pointer file");
        l.assert_clean();
        assert_eq!(l.update(&l.v2, "1.0.0", None).unwrap().receipt.to_version.as_deref(), Some("2.0.0"), "{point:?}");
    }
}

#[test]
fn a_crash_at_any_point_of_an_install_is_recovered_to_a_consistent_state_or_reported_honestly() {
    for &point in FRESH_POINTS {
        let l = lab();
        let r = l.install(&l.v1, Some(&at(point, Fault::Crash)));
        assert!(matches!(r, Err(TxnError::Interrupted(p)) if p == point), "{point:?}: {r:?}");
        assert_eq!(l.files("locks").len(), 1, "{point:?}: the lock is still there");
        assert!(matches!(l.install(&l.v1, None), Err(TxnError::Busy)), "{point:?}: a stale lock blocks new work until recovery");
        let out = l.recover(None);
        if ambiguous_window(point) {
            // a file whose creation was never recorded: kept, the transaction needs attention, nothing else is touched
            assert!(
                matches!(&out[..], [Recovered::NeedsAttention(_, report)] if report.not_undone.iter().any(|n| n.why.contains("ambiguous orphan") || n.why.contains("cannot be proven"))),
                "{point:?}: {out:?}"
            );
            assert_eq!(l.files("locks").len(), 1, "{point:?}");
            for _ in 0..2 {
                assert!(
                    matches!(&l.recover(None)[..], [Recovered::NeedsAttention(..)]),
                    "{point:?}: repeated recovery gives the same answer"
                );
            }
            assert_eq!(l.active(), None, "{point:?}");
            resolve_orphans(&l);
            let out = l.recover(None);
            assert!(out.iter().any(|r| matches!(r, Recovered::RolledBack(_))), "{point:?}: after explicit resolution: {out:?}");
        } else if rolls_forward(point) {
            assert!(matches!(&out[..], [Recovered::RolledForward(_)]), "{point:?}: {out:?}");
            assert_eq!(l.active().as_deref(), Some("1.0.0"), "{point:?}");
            assert_eq!(l.files("receipts").len(), 1);
        } else {
            assert!(matches!(&out[..], [Recovered::RolledBack(_)]), "{point:?}: {out:?}");
            assert_eq!(l.active(), None, "{point:?}");
            assert!(l.artifact_of("1.0.0").is_none() && l.files("receipts").is_empty(), "{point:?}");
        }
        l.assert_clean();
        assert!(l.journals().iter().all(|j| j.state.terminal()), "{point:?}");
        assert!(l.recover(None).is_empty(), "{point:?}: recovery is idempotent");
        l.outside_is_untouched();
        let again = l.install(&l.v1, None).unwrap();
        assert_eq!(again.already_applied, rolls_forward(point), "{point:?}");
    }
}

/// The owner's explicit resolution of ambiguous orphans: remove whatever unrecorded file is left where a creation was only intended.
fn resolve_orphans(l: &Lab) {
    for dir in ["staging", "snapshots", "state", "receipts", "locks"] {
        let base = l.p(dir);
        let Ok(rd) = fs::read_dir(&base) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                for inner in fs::read_dir(&path).into_iter().flatten().flatten() {
                    let _ = fs::remove_file(inner.path());
                }
            } else if dir == "state" && !e.file_name().to_string_lossy().ends_with(".json") {
                let _ = fs::remove_file(&path);
            } else if dir == "state" {
                // an orphan pointer: only one that is not an installed state (fresh install in this helper's callers)
                let _ = fs::remove_file(&path);
            } else {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

#[test]
fn a_crash_during_an_update_never_leaves_a_mix_of_versions_and_a_snapshot_orphan_is_kept_until_resolved() {
    for &point in FRESH_POINTS.iter().chain(UPDATE_EXTRA) {
        let l = lab();
        l.install(&l.v1, None).unwrap();
        let before = fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap();
        assert!(matches!(l.update(&l.v2, "1.0.0", Some(&at(point, Fault::Crash))), Err(TxnError::Interrupted(_))), "{point:?}");
        let out = l.recover(None);
        if ambiguous_window(point) {
            assert!(matches!(&out[..], [Recovered::NeedsAttention(..)]), "{point:?}: {out:?}");
            // whatever is half done, the ACTIVE pointer is either the original or the new complete one, never garbage
            let active = l.active().unwrap();
            assert!(active == "1.0.0" || active == "2.0.0", "{point:?}");
            // the owner resolves: remove the orphan(s) the report keeps
            for dir in ["staging", "snapshots", "receipts"] {
                for e in fs::read_dir(l.p(dir)).unwrap().flatten() {
                    if e.path().is_dir() {
                        for inner in fs::read_dir(e.path()).unwrap().flatten() {
                            let n = inner.file_name().to_string_lossy().into_owned();
                            if n.ends_with(".part") || (dir == "snapshots" && n == "pointer.json") {
                                let _ = fs::remove_file(inner.path());
                            }
                        }
                    } else {
                        let _ = fs::remove_file(e.path());
                    }
                }
            }
            for e in fs::read_dir(l.p("state")).unwrap().flatten() {
                if e.file_name().to_string_lossy().ends_with(".new") {
                    let _ = fs::remove_file(e.path());
                }
            }
            let _ = l.recover(None);
        }
        let active = l.active().unwrap();
        let expect = if active == "1.0.0" { &l.v1.sc.bytes } else { &l.v2.sc.bytes };
        assert_eq!(&l.artifact_of(&active).unwrap(), expect, "{point:?}: active {active}");
        if active == "1.0.0" {
            assert!(l.artifact_of("2.0.0").is_none(), "{point:?}");
            assert_eq!(fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap(), before, "{point:?}: the original pointer, byte-identical");
        }
        l.assert_clean();
        assert!(l.journals().iter().all(|j| j.state.terminal()), "{point:?}");
    }
}

#[test]
fn a_crash_during_an_uninstall_is_recovered_either_back_to_installed_or_forward_to_removed() {
    for point in [
        Point::AfterLock,
        Point::AfterSnapshot,
        Point::UninstallAfterPointerMoved,
        Point::UninstallAfterVersionsMoved,
        Point::BeforeReceiptWrite,
        Point::ReceiptAfterSync,
        Point::AfterReceiptWrite,
        Point::BeforeJournalCommit,
    ] {
        let l = lab();
        l.install(&l.v1, None).unwrap();
        l.update(&l.v2, "1.0.0", None).unwrap();
        assert!(matches!(l.uninstall(&l.v2, Some(&at(point, Fault::Crash))), Err(TxnError::Interrupted(_))), "{point:?}");
        let out = l.recover(None);
        if rolls_forward(point) {
            assert!(matches!(&out[..], [Recovered::RolledForward(_)]), "{point:?}: {out:?}");
            assert_eq!(l.active(), None);
            assert!(!l.p(&format!("versions/{TOOL}")).exists());
        } else {
            assert!(matches!(&out[..], [Recovered::RolledBack(_)]), "{point:?}: {out:?}");
            assert_eq!(l.active().as_deref(), Some("2.0.0"), "{point:?}");
            assert_eq!(l.artifact_of("1.0.0").unwrap(), l.v1.sc.bytes, "{point:?}");
            assert_eq!(l.artifact_of("2.0.0").unwrap(), l.v2.sc.bytes, "{point:?}");
        }
        l.assert_clean();
        assert!(l.recover(None).is_empty());
    }
}

#[test]
fn a_failed_uninstall_puts_everything_back() {
    for point in [
        Point::AfterSnapshot,
        Point::UninstallAfterPointerMoved,
        Point::UninstallAfterVersionsMoved,
        Point::BeforeReceiptWrite,
        Point::BeforeJournalCommit,
    ] {
        let l = lab();
        l.install(&l.v1, None).unwrap();
        l.update(&l.v2, "1.0.0", None).unwrap();
        let pointer = fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap();
        let (_, report) = failed(l.uninstall(&l.v2, Some(&at(point, Fault::Fail))));
        assert!(matches!(report.status.as_str(), "restored" | "not_needed"), "{point:?}: {report:?}");
        assert_eq!(l.active().as_deref(), Some("2.0.0"), "{point:?}");
        assert_eq!(fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap(), pointer, "{point:?}");
        assert_eq!(l.artifact_of("1.0.0").unwrap(), l.v1.sc.bytes);
        assert_eq!(l.artifact_of("2.0.0").unwrap(), l.v2.sc.bytes);
        l.assert_clean();
        assert!(l.uninstall(&l.v2, None).is_ok(), "{point:?}: retry works");
    }
}

#[test]
fn a_rollback_that_cannot_undo_something_says_so_keeps_the_lock_and_recovery_finishes_it() {
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let pointer_v1 = fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap();
    let f = |p: Point| match p {
        Point::AfterPointerSwitch => Some(Fault::Fail),
        Point::RollbackPointerRestore => Some(Fault::Fail),
        _ => None,
    };
    let (_, report) = failed(l.update(&l.v2, "1.0.0", Some(&f)));
    assert_eq!(report.status, "partial", "{report:?}");
    assert!(report.not_undone.iter().any(|n| n.what.contains("pointer")));
    assert!(!report.undone.iter().any(|u| u.contains("pointer")), "it must not claim the pointer was restored");
    assert!(!report.undone.iter().any(|u| u.contains("version folder")), "the version the pointer may still name is kept: {report:?}");
    assert_eq!(l.active().as_deref(), Some("2.0.0"));
    assert!(l.artifact_of("2.0.0").is_some(), "no dangling pointer");
    assert_eq!(l.files("locks").len(), 1);
    assert!(matches!(l.update(&l.v2, "1.0.0", None), Err(TxnError::Busy)));
    assert!(l.files("snapshots").len() == 1, "the snapshot a retry needs is kept");
    let out = l.recover(None);
    assert!(out.iter().any(|r| matches!(r, Recovered::RolledBack(_))), "{out:?}");
    assert_eq!(l.active().as_deref(), Some("1.0.0"));
    assert_eq!(fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap(), pointer_v1);
    l.assert_clean();
}

// ---- adversarial: substitution, links, swaps ---------------------------------------------------------------------------------------

fn install_with_obstruction(l: &Lab, inject: impl Fn(&std::path::Path) + Sync, then: Fault) -> R<Outcome> {
    let vdir = l.p(&format!("versions/{TOOL}/1.0.0"));
    let hook = move |p: Point| {
        if p == Point::AfterPointerSwitch {
            inject(&vdir);
            Some(then)
        } else {
            None
        }
    };
    l.install(&l.v1, Some(&hook))
}

fn assert_kept(report: &RollbackReport) {
    assert!(matches!(report.status.as_str(), "partial" | "failed"), "{report:?}");
    assert!(report.not_undone.iter().any(|n| n.what.contains("version folder 1.0.0")), "{report:?}");
    assert!(!report.undone.iter().any(|u| u.contains("version folder")), "{report:?}");
}

#[test]
fn unexpected_files_same_name_replacements_and_aliases_in_the_version_folder_are_never_deleted() {
    // an unrelated file next to the matching artifact: everything stays (all or nothing)
    let l = lab();
    let (_, report) = failed(install_with_obstruction(&l, |d| fs::write(d.join("notes.txt"), b"unrelated, mine").unwrap(), Fault::Fail));
    assert_kept(&report);
    assert!(report.not_undone.iter().any(|n| n.why.contains("unexpected entr")), "{report:?}");
    let d = l.p(&format!("versions/{TOOL}/1.0.0"));
    assert_eq!(fs::read(d.join("notes.txt")).unwrap(), b"unrelated, mine");
    assert_eq!(fs::read(d.join("artifact.bin")).unwrap(), l.v1.sc.bytes);
    assert_eq!(l.journals()[0].state, TxnState::RollbackFailed);
    assert_eq!(l.files("locks").len(), 1);
    l.outside_is_untouched();

    // the same content, but a different file (new identity)
    let l = lab();
    let bytes = l.v1.sc.bytes.clone();
    let (_, report) = failed(install_with_obstruction(&l, |d| replace_file(&d.join("artifact.bin"), &bytes), Fault::Fail));
    assert_kept(&report);
    assert!(report.not_undone.iter().any(|n| n.why.contains("no longer the file this transaction wrote")), "{report:?}");
    assert_eq!(l.artifact_of("1.0.0").unwrap(), bytes);

    // another content under the expected name
    let l = lab();
    let (_, report) =
        failed(install_with_obstruction(&l, |d| replace_file(&d.join("artifact.bin"), b"someone else's content"), Fault::Fail));
    assert_kept(&report);
    assert_eq!(l.artifact_of("1.0.0").unwrap(), b"someone else's content");

    // an in-place edit that keeps size and identity: caught by the digest
    let l = lab();
    let len = l.v1.sc.bytes.len();
    let (_, report) = failed(install_with_obstruction(
        &l,
        |d| {
            let mut f = fs::OpenOptions::new().write(true).open(d.join("artifact.bin")).unwrap();
            std::io::Write::write_all(&mut f, &vec![b'X'; len]).unwrap();
        },
        Fault::Fail,
    ));
    assert_kept(&report);
    assert_eq!(l.artifact_of("1.0.0").unwrap(), vec![b'X'; len]);

    // a hard-linked alias
    let l = lab();
    let alias = l.base.join("alias.bin");
    let a2 = alias.clone();
    let linked = std::sync::atomic::AtomicBool::new(true);
    let (_, report) = failed(install_with_obstruction(
        &l,
        |d| {
            if fs::hard_link(d.join("artifact.bin"), &a2).is_err() {
                linked.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        },
        Fault::Fail,
    ));
    if linked.load(std::sync::atomic::Ordering::SeqCst) {
        assert_kept(&report);
        assert_eq!(fs::read(&alias).unwrap(), l.v1.sc.bytes, "the alias is untouched");
    }

    // an unexpected sub-folder with nested data is never removed recursively
    let l = lab();
    let (_, report) = failed(install_with_obstruction(
        &l,
        |d| {
            fs::create_dir_all(d.join("cache/deep")).unwrap();
            fs::write(d.join("cache/deep/precious.db"), b"precious").unwrap();
        },
        Fault::Fail,
    ));
    assert_kept(&report);
    assert_eq!(fs::read(l.p(&format!("versions/{TOOL}/1.0.0/cache/deep/precious.db"))).unwrap(), b"precious");
}

#[test]
fn symlinks_junctions_and_parent_redirection_never_lead_outside_the_root() {
    // a link inside the version folder
    let l = lab();
    let outside = l.base.join("outside-dir");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), b"outside content").unwrap();
    let made = std::sync::atomic::AtomicBool::new(true);
    let o2 = outside.clone();
    let (_, report) = failed(install_with_obstruction(
        &l,
        |d| {
            if !make_dir_link(&d.join("redirect"), &o2) {
                made.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        },
        Fault::Fail,
    ));
    if made.load(std::sync::atomic::Ordering::SeqCst) {
        assert_kept(&report);
        assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"outside content");
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
        remove_dir_link(&l.p(&format!("versions/{TOOL}/1.0.0/redirect")));
    }

    // the version folder itself replaced by a link to a foreign folder
    let l = lab();
    let outside = l.base.join("outside-dir");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), b"outside content").unwrap();
    let made = std::sync::atomic::AtomicBool::new(true);
    let (o2, vdir) = (outside.clone(), l.p(&format!("versions/{TOOL}/1.0.0")));
    let v2 = vdir.clone();
    let hook = move |p: Point| {
        if p == Point::AfterPointerSwitch {
            fs::remove_dir_all(&v2).unwrap();
            if !make_dir_link(&v2, &o2) {
                made.store(false, std::sync::atomic::Ordering::SeqCst);
            }
            return Some(Fault::Fail);
        }
        None
    };
    let (_, report) = failed(l.install(&l.v1, Some(&hook)));
    assert_kept(&report);
    assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"outside content");
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1, "nothing was written through the link");
    remove_dir_link(&vdir);

    // the staging folder of the transaction replaced by a link
    let l = lab();
    let outside = l.base.join("outside-dir");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), b"outside content").unwrap();
    let stage_root = l.p("staging");
    let o2 = outside.clone();
    let swapped = std::sync::atomic::AtomicBool::new(false);
    let hook = |p: Point| {
        if p == Point::AfterVerify {
            let dir = fs::read_dir(&stage_root).unwrap().flatten().next().unwrap().path();
            // Windows: the adapter holds the folder open without delete sharing, so the swap is PREVENTED by the operating system;
            // macOS: the swap succeeds, but the held descriptor keeps designating the original folder and the link is never followed.
            if fs::remove_dir_all(&dir).is_ok() && make_dir_link(&dir, &o2) {
                swapped.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            return Some(Fault::Fail);
        }
        None
    };
    let (_, report) = failed(l.install(&l.v1, Some(&hook)));
    if swapped.load(std::sync::atomic::Ordering::SeqCst) {
        assert!(report.not_undone.iter().any(|n| n.what == "staging folder"), "{report:?}");
        if let Some(n) = l.files("staging").first() {
            remove_dir_link(&l.p(&format!("staging/{n}")));
        }
    } else {
        #[cfg(windows)]
        assert!(report.not_undone.is_empty(), "the held handle prevented the swap, so the rollback is complete: {report:?}");
    }
    assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"outside content");
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);

    // a managed folder (versions) replaced by a link BEFORE the transaction: nothing is written anywhere
    let l = lab();
    let outside = l.base.join("outside-dir");
    fs::create_dir(&outside).unwrap();
    fs::remove_dir(l.p("versions")).unwrap();
    if make_dir_link(&l.p("versions"), &outside) {
        let r = l.install(&l.v1, None);
        assert!(matches!(r, Err(TxnError::Failed { .. }) | Err(TxnError::Refused(_))), "{r:?}");
        assert!(fs::read_dir(&outside).unwrap().next().is_none(), "nothing was written through the link");
        let c = clock();
        assert!(
            matches!(l.adapter(&c, None).active_version(TOOL), Err(TxnError::Refused("fs_link_refused"))),
            "a redirected layout is refused, not followed"
        );
        remove_dir_link(&l.p("versions"));
    }
}

#[test]
fn a_substituted_staged_file_or_a_swap_between_check_and_activation_is_caught() {
    for point in [Point::AfterVerify, Point::AfterVersionDirCreate] {
        let l = lab();
        let stage_root = l.p("staging");
        let hook = move |p: Point| {
            if p == point {
                let dir = fs::read_dir(&stage_root).unwrap().flatten().next().unwrap().path();
                replace_file(&dir.join("artifact.bin"), b"alien content under an expected name");
            }
            None
        };
        let (code, report) = failed(l.install(&l.v1, Some(&hook)));
        assert_eq!(code, "staged_file_is_not_the_file_this_transaction_wrote", "{point:?}");
        assert!(
            report.not_undone.iter().any(|n| n.what == "staging folder" && n.why.contains("no longer the file")),
            "{point:?}: {report:?}"
        );
        let staged = l.p(&format!("staging/{}", l.files("staging")[0]));
        assert_eq!(fs::read(staged.join("artifact.bin")).unwrap(), b"alien content under an expected name");
        assert!(l.artifact_of("1.0.0").is_none(), "the alien file never became an installed version");
        assert_eq!(l.active(), None);
        // the owner removes it; the next recovery completes the cleanup
        fs::remove_file(staged.join("artifact.bin")).unwrap();
        let out = l.recover(None);
        assert!(out.iter().any(|r| matches!(r, Recovered::RolledBack(_))), "{point:?}: {out:?}");
        l.assert_clean();
    }
}

#[test]
fn a_substituted_pointer_or_snapshot_is_never_overwritten_restored_or_deleted() {
    // the active pointer is replaced by a foreign file after the switch; the rollback must not delete or overwrite it
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let ptr = l.p(&format!("state/{TOOL}.json"));
    let hook = {
        let ptr = ptr.clone();
        move |p: Point| {
            if p == Point::AfterPointerSwitch {
                replace_file(&ptr, b"{\"foreign\":true}");
                return Some(Fault::Fail);
            }
            None
        }
    };
    let (_, report) = failed(l.update(&l.v2, "1.0.0", Some(&hook)));
    assert!(report.not_undone.iter().any(|n| n.what == "active-version pointer"), "{report:?}");
    assert_eq!(fs::read(&ptr).unwrap(), b"{\"foreign\":true}", "the foreign pointer survives");
    assert!(l.artifact_of("2.0.0").is_some(), "the version the pointer may name is kept");

    // a forged saved pointer is never restored as the active one
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let snaps = l.p("snapshots");
    let hook = move |p: Point| {
        if p == Point::AfterPointerSwitch {
            let dir = fs::read_dir(&snaps).unwrap().flatten().next().unwrap().path();
            replace_file(&dir.join("pointer.json"), b"{\"forged\":true}");
            return Some(Fault::Fail);
        }
        None
    };
    let (_, report) = failed(l.update(&l.v2, "1.0.0", Some(&hook)));
    assert!(report.not_undone.iter().any(|n| n.what == "active-version pointer"), "{report:?}");
    assert_eq!(l.active().as_deref(), Some("2.0.0"));
    assert!(!fs::read_to_string(l.p(&format!("state/{TOOL}.json"))).unwrap().contains("forged"));

    // a snapshot file substituted before the cleanup of a committed uninstall is kept, with the recovery reporting it every time
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let snaps = l.p("snapshots");
    let hook = move |p: Point| {
        if p == Point::BeforeJournalCommit {
            let dir = fs::read_dir(&snaps).unwrap().flatten().next().unwrap().path();
            replace_file(&dir.join("versions/1.0.0/artifact.bin"), b"alien in the snapshot");
        }
        None
    };
    l.uninstall(&l.v1, Some(&hook)).unwrap();
    let snap_dir = l.p(&format!("snapshots/{}", l.files("snapshots")[0]));
    assert_eq!(fs::read(snap_dir.join("versions/1.0.0/artifact.bin")).unwrap(), b"alien in the snapshot");
    assert!(snap_dir.join("pointer.json").exists(), "all or nothing");
    for _ in 0..2 {
        let rec = l.recover(None);
        assert!(matches!(&rec[..], [Recovered::LeftoversRetained(_, why)] if why.contains("snapshot")), "{rec:?}");
    }
}

#[test]
fn an_uninstall_is_refused_before_anything_moves_when_installed_content_is_not_what_a_receipt_vouches_for() {
    let setup = || {
        let l = lab();
        l.install(&l.v1, None).unwrap();
        l.update(&l.v2, "1.0.0", None).unwrap();
        l
    };
    let untouched = |l: &Lab| {
        assert_eq!(l.active().as_deref(), Some("2.0.0"));
        assert!(l.artifact_of("1.0.0").is_some() && l.artifact_of("2.0.0").is_some());
        assert!(l.files("snapshots").is_empty() && l.files("locks").is_empty(), "nothing was left started");
    };
    let l = setup();
    fs::write(l.p(&format!("versions/{TOOL}/1.0.0/artifact.bin")), b"tampered").unwrap();
    assert_eq!(refused(l.uninstall(&l.v2, None)), "installed_content_has_no_matching_receipt");
    untouched(&l);
    let l = setup();
    fs::write(l.p(&format!("versions/{TOOL}/2.0.0/notes.txt")), b"mine").unwrap();
    assert_eq!(refused(l.uninstall(&l.v2, None)), "unrecognised_installed_content");
    untouched(&l);
    let l = setup();
    fs::create_dir(l.p(&format!("versions/{TOOL}/9.9.9"))).unwrap();
    fs::write(l.p(&format!("versions/{TOOL}/9.9.9/artifact.bin")), b"handmade").unwrap();
    assert_eq!(refused(l.uninstall(&l.v2, None)), "installed_content_has_no_matching_receipt");
    let l = setup();
    let alias = l.base.join("alias.bin");
    if fs::hard_link(l.p(&format!("versions/{TOOL}/2.0.0/artifact.bin")), &alias).is_ok() {
        assert_eq!(refused(l.uninstall(&l.v2, None)), "installed_content_has_no_matching_receipt");
        assert_eq!(fs::read(&alias).unwrap(), l.v2.sc.bytes);
    }
    // a corrupt receipt: the digest no longer matches, so it vouches for nothing and the install is not "already done" either
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let receipt = l.p(&format!("receipts/{}", l.files("receipts")[0]));
    let mut r: Receipt = serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
    r.to_version = Some("6.6.6".into());
    fs::write(&receipt, serde_json::to_vec(&r).unwrap()).unwrap();
    assert_eq!(refused(l.install(&l.v1, None)), "installed_state_has_no_valid_receipt");
    assert_eq!(refused(l.uninstall(&l.v1, None)), "installed_content_has_no_matching_receipt");
    assert_eq!(l.active().as_deref(), Some("1.0.0"));
}

#[test]
fn a_forged_journal_cannot_make_the_adapter_delete_a_foreign_file_and_a_corrupt_one_blocks_new_work() {
    let l = lab();
    // a crash leaves a journal behind
    assert!(matches!(l.install(&l.v1, Some(&at(Point::AfterVerify, Fault::Crash))), Err(TxnError::Interrupted(_))));
    let stage = l.p(&format!("staging/{}", l.files("staging")[0]));
    // a foreign file in the transaction's staging folder
    fs::write(stage.join("foreign.bin"), b"foreign").unwrap();
    // forge the journal: claim ownership of the foreign file with an invented identity (digest recomputed, as anyone could)
    let jpath = l.p(&format!("journal/{}", l.files("journal").into_iter().find(|n| n.ends_with(".json")).unwrap()));
    let mut t: Txn = serde_json::from_slice(&fs::read(&jpath).unwrap()).unwrap();
    t.owned.push(OwnedFile {
        role: "forged".into(),
        phase: Phase::Final,
        rels: vec![format!("staging/{}/foreign.bin", t.txn_id)],
        sha256: Some(sha256_hex_bytes(b"foreign")),
        size: Some(7),
        identity: Some([1, 2]),
    });
    t.journal_digest = txn_digest(&t);
    fs::write(&jpath, serde_json::to_vec(&t).unwrap()).unwrap();
    let out = l.recover(None);
    assert!(matches!(&out[..], [Recovered::NeedsAttention(..)]), "{out:?}");
    assert_eq!(fs::read(stage.join("foreign.bin")).unwrap(), b"foreign", "a claim with the wrong identity does not make it deletable");
    // an unreadable / tampered journal (digest mismatch) is reported and blocks new work
    let l = lab();
    assert!(matches!(l.install(&l.v1, Some(&at(Point::AfterVerify, Fault::Crash))), Err(TxnError::Interrupted(_))));
    let jpath = l.p(&format!("journal/{}", l.files("journal").into_iter().find(|n| n.ends_with(".json")).unwrap()));
    let mut text = fs::read_to_string(&jpath).unwrap();
    text = text.replace("\"state\":\"verified\"", "\"state\":\"committed\"");
    fs::write(&jpath, text).unwrap();
    let out = l.recover(None);
    assert!(out.iter().any(|r| matches!(r, Recovered::CorruptJournal(_))), "{out:?}");
    assert!(
        out.iter().any(|r| matches!(r, Recovered::LeftoversRetained(who, _) if who == "locks")),
        "a lock that no readable journal accounts for is kept and reported: {out:?}"
    );
    assert!(matches!(l.install(&l.v1, None), Err(TxnError::Busy)), "the stale lock and the corrupt journal block new work");
    assert_eq!(l.active(), None);
}

#[test]
fn a_swap_between_the_check_and_the_deletion_never_deletes_the_newcomer() {
    // the managed_fs hook fires between the verification and the unlink of a verified deletion inside a real cleanup
    let l = lab();
    let (v, dev) = l.auth(&l.v1, Action::Install, None);
    let c = clock();
    let target = std::sync::Arc::new(std::sync::Mutex::new(None::<PathBuf>));
    let t2 = target.clone();
    // the lock file is deleted last, by identity; swap it right before
    let lock_path = l.p(&format!("locks/{TOOL}.lock"));
    crate::managed_fs::hook::set(move |stage| {
        if stage == "before-delete" {
            if let Some(p) = t2.lock().unwrap().clone() {
                if p.exists() {
                    let moved = p.with_extension("moved");
                    let _ = fs::rename(&p, &moved);
                    let _ = fs::write(&p, b"alien newcomer");
                }
            }
        }
    });
    *target.lock().unwrap() = Some(lock_path.clone());
    let r = l.adapter(&c, None).install(&Authority { recipe: &l.v1.valid, approval: &v }, &dev, &l.v1.sc.bytes);
    crate::managed_fs::hook::clear();
    assert!(r.is_ok(), "{r:?}");
    // whatever the platform did with the original, the newcomer under the lock's name was not deleted by the engine
    if lock_path.exists() {
        assert_eq!(fs::read(&lock_path).unwrap(), b"alien newcomer");
    }
}

#[cfg(windows)]
#[test]
fn a_locked_file_makes_the_operation_fail_honestly_and_everything_is_retried_after_it_is_released() {
    use std::os::windows::fs::OpenOptionsExt;
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let artifact = l.p(&format!("versions/{TOOL}/1.0.0/artifact.bin"));
    // a running program: holds the installed file open without allowing delete/rename
    let held = fs::OpenOptions::new().read(true).share_mode(1).open(&artifact).unwrap();
    let (code, report) = failed(l.uninstall(&l.v1, None));
    assert_eq!(code, "fs_locked_by_another_process", "{report:?}");
    assert!(matches!(report.status.as_str(), "restored" | "partial"), "{report:?}");
    assert_eq!(l.active().as_deref(), Some("1.0.0"), "the pointer was put back");
    assert_eq!(l.artifact_of("1.0.0").unwrap(), l.v1.sc.bytes);
    drop(held);
    l.recover(None);
    l.assert_clean();
    assert!(l.uninstall(&l.v1, None).is_ok(), "after the program released the file the uninstall works");
}

#[cfg(unix)]
#[test]
fn an_inaccessible_state_folder_is_a_failed_transaction_with_an_honest_rollback() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("running as root: permission denial cannot be exercised");
        return;
    }
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let before = fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap();
    fs::set_permissions(l.p("state"), fs::Permissions::from_mode(0o500)).unwrap();
    let r = l.update(&l.v2, "1.0.0", None);
    fs::set_permissions(l.p("state"), fs::Permissions::from_mode(0o700)).unwrap();
    let (_, report) = failed(r);
    assert!(matches!(report.status.as_str(), "restored" | "partial" | "failed"), "{report:?}");
    assert_eq!(l.active().as_deref(), Some("1.0.0"));
    assert_eq!(fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap(), before);
    let _ = l.recover(None);
    l.assert_clean();
    assert!(l.update(&l.v2, "1.0.0", None).is_ok());
}

// ---- portability, hygiene ----------------------------------------------------------------------------------------------------------------

#[test]
fn the_adapter_works_below_other_account_names_unicode_and_spaces_and_depends_on_no_developer_layout() {
    for sub in ["Üser Ünïcode adı", "other-account", "name with spaces", "Kullanıcı Ağır Şeyler"] {
        let l = lab_in(sub);
        l.install(&l.v1, None).unwrap();
        l.update(&l.v2, "1.0.0", None).unwrap();
        l.uninstall(&l.v2, None).unwrap();
        l.assert_clean();
        l.outside_is_untouched();
        let text = l.journals().iter().map(|j| serde_json::to_string(j).unwrap()).collect::<String>();
        let receipts: String = l.files("receipts").iter().map(|n| fs::read_to_string(l.p(&format!("receipts/{n}"))).unwrap()).collect();
        for needle in [sub, "C:\\", "/Users/", "/home/", "erent"] {
            assert!(!text.contains(needle) && !receipts.contains(needle), "{needle:?} must not appear in journals or receipts");
        }
    }
}

#[test]
fn nothing_outside_the_root_changes_and_the_process_environment_is_untouched() {
    let path_before = std::env::var_os("PATH");
    let cwd_before = std::env::current_dir().unwrap();
    let vars_before: Vec<(std::ffi::OsString, std::ffi::OsString)> = std::env::vars_os().collect();
    let l = lab();
    l.install(&l.v1, None).unwrap();
    l.update(&l.v2, "1.0.0", None).unwrap();
    let _ = l.update(&l.v2, "9.9.9", None);
    let _ = l.install(&l.v1, Some(&at(Point::AfterPointerSwitch, Fault::Fail)));
    l.uninstall(&l.v2, None).unwrap();
    l.outside_is_untouched();
    assert_eq!(std::env::var_os("PATH"), path_before);
    assert_eq!(std::env::current_dir().unwrap(), cwd_before);
    assert_eq!(std::env::vars_os().collect::<Vec<_>>(), vars_before);
}

#[test]
fn the_adapter_does_not_use_the_path_based_engine_and_has_no_process_network_environment_or_agent_capability() {
    let whole = include_str!("adapter.rs");
    let production = whole.split("#[cfg(test)]\n#[path").next().unwrap();
    for needle in [
        "std::fs",
        "fs::remove",
        "fs::write",
        "fs::rename",
        "remove_dir_all",
        "atomic_write",
        "SandboxRoot",
        "Engine",
        "std::process",
        "Command::new",
        "std::net",
        "TcpStream",
        "reqwest",
        "set_var",
        "remove_var",
        "set_current_dir",
        ".claude",
        "mcp",
        "PATH",
        "winreg",
        "Registry",
    ] {
        assert!(!production.contains(needle), "{needle} must not appear in the adapter");
    }
    // the only entry points that act are the five lifecycle methods, and the production constructor cannot act
    assert!(production.contains("production_adapter_not_enabled"));
    assert_eq!(production.matches("pub fn test_mode").count(), 1);
    assert!(production.contains("#[cfg(test)]\n    pub fn test_mode"), "the acting constructor exists in test builds only");
    assert_eq!(production.matches("mode: Mode::TestSynthetic").count(), 1, "the acting mode is constructed in exactly one place");
}

// ---- RT-3C-02: "already installed" is a statement about the installed file, not about a self-consistent receipt ----------------------

/// Everything below the managed root (relative path -> content), plus the audit trail and the approval ledger state.
type Snapshot = (std::collections::BTreeMap<String, Vec<u8>>, usize);

fn snapshot(l: &Lab) -> Snapshot {
    fn walk(base: &std::path::Path, dir: &std::path::Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        for e in fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
            if p.is_dir() {
                out.insert(format!("{rel}/"), vec![]);
                walk(base, &p, out);
            } else {
                out.insert(rel, fs::read(&p).unwrap_or_default());
            }
        }
    }
    let mut files = std::collections::BTreeMap::new();
    walk(&l.root_path(), &l.root_path(), &mut files);
    (files, l.audit.events.lock().unwrap().len())
}

fn installed(l: &Lab) -> std::path::PathBuf {
    l.p(&format!("versions/{TOOL}/1.0.0/artifact.bin"))
}

/// The drift codes are the same for an install and an update request, and nothing is repaired, replaced, adopted or consumed.
fn assert_refused_without_touching(l: &Lab, code: &str) {
    let before = snapshot(l);
    let (v, dev) = l.auth(&l.v1, Action::Install, None);
    let c = clock();
    let r = l.adapter(&c, None).install(&Authority { recipe: &l.v1.valid, approval: &v }, &dev, &l.v1.sc.bytes);
    assert_eq!(refused(r), code);
    let pre = l.adapter(&c, None).preflight(&Authority { recipe: &l.v1.valid, approval: &v }, Action::Install, Some(&l.v1.sc.bytes));
    assert!(matches!(pre, Err(TxnError::Refused(c2)) if c2 == code), "{pre:?}");
    // the only change is the approval this check itself issued (audit event + ledger); no journal, file or folder moved
    let after = snapshot(l);
    assert_eq!(before.0, after.0, "{code}: nothing under the managed root changed");
    assert_eq!(
        l.ledger.status(&dev.approval.approval_id).unwrap(),
        Some(device_trust::LedgerStatus::Issued),
        "{code}: the approval was not consumed"
    );
    assert!(l.files("locks").is_empty() && l.files("staging").is_empty() && l.files("snapshots").is_empty(), "{code}");
}

#[test]
fn a_genuine_unchanged_installation_is_idempotent_and_mutates_nothing() {
    let l = lab();
    let first = l.install(&l.v1, None).unwrap();
    let (v, dev) = l.auth(&l.v1, Action::Install, None);
    let before = snapshot(&l);
    let c = clock();
    let a = l.adapter(&c, None);
    for _ in 0..3 {
        let again = a.install(&Authority { recipe: &l.v1.valid, approval: &v }, &dev, &l.v1.sc.bytes).unwrap();
        assert!(again.already_applied && again.receipt == first.receipt);
    }
    let pf = a.preflight(&Authority { recipe: &l.v1.valid, approval: &v }, Action::Install, Some(&l.v1.sc.bytes)).unwrap();
    assert!(pf.already_applied);
    // no new journal, file, folder or audit record (the extra approvals above were only issued, never used)
    let after = snapshot(&l);
    assert_eq!(before.0, after.0);
    assert_eq!(
        l.ledger.status(&dev.approval.approval_id).unwrap(),
        Some(device_trust::LedgerStatus::Issued),
        "the approval was not consumed"
    );
    assert_eq!(l.journals().len(), 1);
}

#[test]
fn a_missing_installed_artifact_is_reported_as_absence_not_as_success_and_not_reinstalled() {
    let l = lab();
    l.install(&l.v1, None).unwrap();
    fs::remove_file(installed(&l)).unwrap();
    assert_refused_without_touching(&l, "installed_artifact_missing");
    assert!(!installed(&l).exists(), "nothing was reinstalled");
    // the version folder or the tool folder gone as well
    fs::remove_dir(l.p(&format!("versions/{TOOL}/1.0.0"))).unwrap();
    assert_refused_without_touching(&l, "installed_artifact_missing");
    fs::remove_dir(l.p(&format!("versions/{TOOL}"))).unwrap();
    assert_refused_without_touching(&l, "installed_artifact_missing");
    assert_eq!(l.active().as_deref(), Some("1.0.0"), "the pointer is not silently repaired either");
}

#[test]
fn modified_or_replaced_installed_content_is_drift_whatever_the_size_or_bytes() {
    // same size, other bytes, same file
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let len = l.v1.sc.bytes.len();
    {
        let mut f = fs::OpenOptions::new().write(true).open(installed(&l)).unwrap();
        std::io::Write::write_all(&mut f, &vec![b'X'; len]).unwrap();
    }
    assert_refused_without_touching(&l, "installed_artifact_drift");
    assert_eq!(fs::read(installed(&l)).unwrap(), vec![b'X'; len], "the drifted file was left exactly as found");

    // a different size
    let l = lab();
    l.install(&l.v1, None).unwrap();
    fs::write(installed(&l), b"short").unwrap();
    assert_refused_without_touching(&l, "installed_artifact_drift");

    // identical bytes, but a different file (new identity): only the journal-recorded identity can tell
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let bytes = l.v1.sc.bytes.clone();
    replace_file(&installed(&l), &bytes);
    assert_refused_without_touching(&l, "installed_artifact_drift");
    assert_eq!(fs::read(installed(&l)).unwrap(), bytes);

    // a hard-linked alias of the right bytes
    let l = lab();
    l.install(&l.v1, None).unwrap();
    if fs::hard_link(installed(&l), l.base.join("alias.bin")).is_ok() {
        assert_refused_without_touching(&l, "installed_artifact_drift");
        assert_eq!(fs::read(l.base.join("alias.bin")).unwrap(), l.v1.sc.bytes);
    }
}

#[test]
fn a_link_or_junction_in_place_of_the_artifact_or_its_folder_is_drift_and_is_never_followed() {
    // a file symlink under the artifact name, pointing at a perfect copy elsewhere
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let copy = l.base.join("copy.bin");
    fs::write(&copy, &l.v1.sc.bytes).unwrap();
    fs::remove_file(installed(&l)).unwrap();
    let linked = {
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(&copy, installed(&l)).is_ok()
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&copy, installed(&l)).is_ok()
        }
    };
    if linked {
        assert_refused_without_touching(&l, "installed_artifact_drift");
        assert_eq!(fs::read(&copy).unwrap(), l.v1.sc.bytes, "the link target is untouched");
        let _ = fs::remove_file(installed(&l));
    }

    // the version folder replaced by a link to a perfect copy of the folder
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let vdir = l.p(&format!("versions/{TOOL}/1.0.0"));
    let copy_dir = l.base.join("copy-dir");
    fs::create_dir(&copy_dir).unwrap();
    fs::write(copy_dir.join("artifact.bin"), &l.v1.sc.bytes).unwrap();
    fs::remove_file(vdir.join("artifact.bin")).unwrap();
    fs::remove_dir(&vdir).unwrap();
    if make_dir_link(&vdir, &copy_dir) {
        assert_refused_without_touching(&l, "installed_artifact_drift");
        assert_eq!(fs::read(copy_dir.join("artifact.bin")).unwrap(), l.v1.sc.bytes);
        remove_dir_link(&vdir);
    }
}

#[test]
fn an_unexpected_second_entry_in_the_installed_version_folder_is_reported_and_left_alone() {
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let extra = l.p(&format!("versions/{TOOL}/1.0.0/notes.txt"));
    fs::write(&extra, b"mine").unwrap();
    assert_refused_without_touching(&l, "installed_folder_has_unexpected_entries");
    assert_eq!(fs::read(&extra).unwrap(), b"mine");
    fs::remove_file(&extra).unwrap();
    fs::create_dir(l.p(&format!("versions/{TOOL}/1.0.0/cache"))).unwrap();
    assert_refused_without_touching(&l, "installed_folder_has_unexpected_entries");
    fs::remove_dir(l.p(&format!("versions/{TOOL}/1.0.0/cache"))).unwrap();
    // with the folder as installed, the same request is the genuine no-op again
    assert!(l.install(&l.v1, None).unwrap().already_applied);
}

fn rewrite_pointer(l: &Lab, edit: impl Fn(&mut serde_json::Value)) {
    let path = l.p(&format!("state/{TOOL}.json"));
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    edit(&mut v);
    fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
}

fn rewrite_receipt(l: &Lab, txn: &str, edit: impl Fn(&mut Receipt), redigest: bool) {
    let path = l.p(&format!("receipts/{txn}.json"));
    let mut r: Receipt = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    edit(&mut r);
    if redigest {
        r.receipt_digest = receipt_digest(&r);
    }
    fs::write(&path, serde_json::to_vec(&r).unwrap()).unwrap();
}

#[test]
fn a_pointer_that_does_not_bind_to_its_receipt_tool_version_and_platform_is_refused() {
    // a pointer that names another tool
    let l = lab();
    l.install(&l.v1, None).unwrap();
    rewrite_pointer(&l, |v| v["tool"] = serde_json::json!("synthetic-other"));
    assert_refused_without_touching(&l, "installed_state_pointer_mismatch");

    // a stale receipt: the pointer says 2.0.0 but names the transaction that installed 1.0.0
    let l = lab();
    let first = l.install(&l.v1, None).unwrap();
    l.update(&l.v2, "1.0.0", None).unwrap();
    rewrite_pointer(&l, |v| v["txn"] = serde_json::json!(first.receipt.txn_id));
    let before = snapshot(&l);
    let (v, dev) = l.auth(&l.v2, Action::Update, Some("1.0.0"));
    let c = clock();
    let r = l.adapter(&c, None).update(&Authority { recipe: &l.v2.valid, approval: &v }, &dev, &l.v2.sc.bytes);
    assert_eq!(refused(r), "installed_state_receipt_mismatch");
    assert_eq!(before.0, snapshot(&l).0);

    // forged receipts whose digest IS valid but whose content does not bind: wrong tool, version, platform, computer, action
    type Edit = Box<dyn Fn(&mut Receipt)>;
    let edits: Vec<(&str, Edit)> = vec![
        ("tool", Box::new(|r: &mut Receipt| r.tool_id = "synthetic-other".into())),
        ("version", Box::new(|r: &mut Receipt| r.to_version = Some("6.6.6".into()))),
        ("platform", Box::new(|r: &mut Receipt| r.platform_target = other_platform().into())),
        ("computer", Box::new(|r: &mut Receipt| r.computer_id = "synthetic-computer-2".into())),
        ("action", Box::new(|r: &mut Receipt| r.action = Action::Uninstall)),
        ("artifact", Box::new(|r: &mut Receipt| r.artifact_sha256 = Some("0".repeat(64)))),
    ];
    for (what, edit) in edits {
        let l = lab();
        let out = l.install(&l.v1, None).unwrap();
        rewrite_receipt(&l, &out.receipt.txn_id, edit, true);
        assert_refused_without_touching(&l, "installed_state_receipt_mismatch");
        let _ = what;
    }

    // an altered receipt whose digest no longer matches
    let l = lab();
    let out = l.install(&l.v1, None).unwrap();
    rewrite_receipt(&l, &out.receipt.txn_id, |r| r.kept = vec!["tampered".into()], false);
    assert_refused_without_touching(&l, "installed_state_has_no_valid_receipt");
    // a missing receipt
    fs::remove_file(l.p(&format!("receipts/{}.json", out.receipt.txn_id))).unwrap();
    assert_refused_without_touching(&l, "installed_state_has_no_valid_receipt");
}

#[test]
fn without_the_installing_journal_the_file_identity_cannot_be_proven_and_the_adapter_says_so() {
    let l = lab();
    let out = l.install(&l.v1, None).unwrap();
    fs::remove_file(l.p(&format!("journal/{}.json", out.receipt.txn_id))).unwrap();
    assert_refused_without_touching(&l, "installed_identity_unprovable");
}

#[test]
fn drift_messages_contain_no_paths_and_no_artifact_content() {
    let l = lab();
    l.install(&l.v1, None).unwrap();
    fs::write(installed(&l), b"SECRET-CONTENT-9f3a").unwrap();
    let (v, dev) = l.auth(&l.v1, Action::Install, None);
    let c = clock();
    let r = l.adapter(&c, None).install(&Authority { recipe: &l.v1.valid, approval: &v }, &dev, &l.v1.sc.bytes);
    let text = format!("{r:?}");
    for needle in [l.base.to_string_lossy().as_ref(), "SECRET-CONTENT", "C:\\", "/Users/", "/private/", "appdata"] {
        assert!(!text.contains(needle), "{needle}: {text}");
    }
    let audit = format!("{:?}", l.audit.events.lock().unwrap());
    assert!(!audit.contains("SECRET-CONTENT") && !audit.contains(l.base.to_string_lossy().as_ref()));
}

// ---- RT-3C-01 through the adapter: every overwrite is of a known, transaction-owned file ------------------------------------

#[test]
fn a_journal_that_was_replaced_by_another_file_is_never_overwritten_by_the_next_save() {
    let l = lab();
    let alien = std::sync::atomic::AtomicBool::new(false);
    let jroot = l.p("journal");
    let hook = |p: Point| {
        if p == Point::AfterLock && !alien.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let journal =
                fs::read_dir(&jroot).unwrap().flatten().find(|e| e.file_name().to_string_lossy().ends_with(".json")).unwrap().path();
            replace_file(&journal, b"alien journal content");
        }
        None
    };
    let r = l.install(&l.v1, Some(&hook));
    assert!(matches!(r, Err(TxnError::Failed { .. }) | Err(TxnError::Refused(_))), "{r:?}");
    let journal =
        fs::read_dir(l.p("journal")).unwrap().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "json")).unwrap();
    assert_eq!(fs::read(journal).unwrap(), b"alien journal content", "the foreign file under the journal name was not overwritten");
    assert!(l.artifact_of("1.0.0").is_none() && l.active().is_none(), "nothing was installed on top of a record that could not be written");
}

#[test]
fn a_pointer_swapped_for_another_file_before_the_switch_is_never_replaced() {
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let ptr = l.p(&format!("state/{TOOL}.json"));
    let hook = {
        let ptr = ptr.clone();
        move |p: Point| {
            if p == Point::PointerAfterSync {
                replace_file(&ptr, b"{\"alien\":true}");
            }
            None
        }
    };
    let (_, report) = failed(l.update(&l.v2, "1.0.0", Some(&hook)));
    // the alien pointer was swapped in BEFORE the switch: the replacement checks the known identity and refuses, byte-identical
    assert_eq!(fs::read(&ptr).unwrap(), b"{\"alien\":true}");
    assert!(report.not_undone.iter().any(|n| n.what == "active-version pointer" || n.what.contains("version folder")), "{report:?}");
}

#[test]
fn the_adapter_never_asks_for_an_unrestricted_overwrite() {
    let production = include_str!("adapter.rs").split("#[cfg(test)]\n#[path").next().unwrap();
    assert!(!production.contains("rename_file"), "only move_file (no-clobber) and replace_owned_file (one known file) exist");
    assert_eq!(
        production.matches("replace_owned_file").count(),
        3,
        "journal write, pointer switch, pointer restore: nothing else may overwrite"
    );
}

// =====================================================================================================================================
// Phase 3D-1: the SYNTHETIC pilot rehearsal. The adapter is driven exactly the way a first real pilot would drive it, but with
// made-up bytes and FAKE approval credentials: the durable approval ledger and append-only audit journal are real (a SQLite file),
// the approval key is a fake OS credential backend that proves presence through its signing operation, and nothing is downloaded,
// started or installed. It is a rehearsal, not a real jq installation.
// =====================================================================================================================================

use crate::{
    approval_store::{self, ApprovalStore},
    device_trust::PresenceSource,
    os_approval::{self, fakes::FakeOsBackend, Assurance, OsKeyStore},
};

struct Rehearsal {
    l: Lab,
    _ledger_dir: tempfile::TempDir,
    ledger_path: PathBuf,
    store: ApprovalStore,
    backend: FakeOsBackend,
}

fn rehearsal() -> Rehearsal {
    let l = lab_in("rehearsal");
    let dir = tempfile::tempdir().unwrap();
    let ledger_path = dir.path().join(approval_store::FILE_NAME);
    let store = ApprovalStore::open(&ledger_path).unwrap();
    let backend = FakeOsBackend::new(Assurance::Fake);
    // the person explicitly enables approval security, then the (fake) credential service creates the key once
    store.enable_approval_security(COMPUTER, &approval_store::enable_phrase(COMPUTER)).unwrap();
    os_approval::provision(&store, &backend, COMPUTER, "atm-approval-rehearsal").unwrap();
    Rehearsal { l, _ledger_dir: dir, ledger_path, store, backend }
}

impl Rehearsal {
    fn with_adapter<T>(&self, fault: Option<FaultFn>, f: impl FnOnce(&Adapter, &OsKeyStore) -> T) -> T {
        let keys = OsKeyStore::open(&self.store, &self.backend, &self.backend, COMPUTER).unwrap();
        let c = clock();
        let a = Adapter::test_mode(
            &self.l.root,
            &self.l.refs,
            this_platform(),
            &c,
            fault,
            Trust { keys: &keys, ledger: &self.store, audit: &self.store, policy: TrustPolicy::test_doubles() },
        );
        f(&a, &keys)
    }

    /// Phase 3A approval + a device approval whose presence was proven by the (fake) OS signing operation.
    fn auth(&self, f: &Fx, action: Action, from: Option<&str>) -> Auth {
        let a = make_approval(f, action, from, COMPUTER, t0());
        let v = verified(f, &a, action, from, COMPUTER, t0());
        let keys = OsKeyStore::open(&self.store, &self.backend, &self.backend, COMPUTER).unwrap();
        let dev = device_trust::issue_with(
            &a,
            &f.valid.recipe().artifact.sha256,
            &f.valid.permissions_digest(),
            &keys,
            PresenceSource::SignedByOs,
            &self.store,
            &self.store,
            TrustPolicy::test_doubles(),
        )
        .unwrap();
        (v, dev)
    }

    fn install_with(&self, f: &Fx, auth: &Auth, fault: Option<FaultFn>) -> R<Outcome> {
        self.with_adapter(fault, |a, _| a.install(&Authority { recipe: &f.valid, approval: &auth.0 }, &auth.1, &f.sc.bytes))
    }

    fn install(&self, f: &Fx, fault: Option<FaultFn>) -> R<Outcome> {
        let auth = self.auth(f, Action::Install, None);
        self.install_with(f, &auth, fault)
    }

    fn update(&self, f: &Fx, from: &str, fault: Option<FaultFn>) -> R<Outcome> {
        let (v, dev) = self.auth(f, Action::Update, Some(from));
        self.with_adapter(fault, |a, _| a.update(&Authority { recipe: &f.valid, approval: &v }, &dev, &f.sc.bytes))
    }

    fn uninstall(&self, f: &Fx, fault: Option<FaultFn>) -> R<Outcome> {
        let (v, dev) = self.auth(f, Action::Uninstall, None);
        self.with_adapter(fault, |a, _| a.uninstall(&Authority { recipe: &f.valid, approval: &v }, &dev))
    }

    /// Every file below the managed root with its content digest: "nothing changed" means this is equal.
    fn tree(&self) -> Vec<(String, String)> {
        fn walk(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<(String, String)>) {
            if let Ok(rd) = fs::read_dir(dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        walk(&p, base, out);
                    } else {
                        let rel = p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
                        out.push((rel, crate::synthetic::sha256_of(&fs::read(&p).unwrap_or_default())));
                    }
                }
            }
        }
        let mut v = Vec::new();
        walk(&self.l.root_path(), &self.l.root_path(), &mut v);
        v.sort();
        v
    }

    fn user_files(&self) -> Vec<(String, String)> {
        self.tree().into_iter().filter(|(p, _)| p.starts_with("data/")).collect()
    }

    fn ledger_rows(&self) -> Vec<(String, String)> {
        let c = rusqlite::Connection::open(&self.ledger_path).unwrap();
        let mut st = c.prepare("SELECT approval_id, status FROM approvals ORDER BY counter").unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(|r| r.unwrap()).collect()
    }

    fn assert_trust_is_consistent(&self) {
        let events = self.store.audit_events().unwrap();
        assert_eq!(self.store.verify_audit_chain(), Ok(events.len()), "the audit chain verifies");
        let consumed = self.ledger_rows().iter().filter(|(_, s)| s == "consumed").count();
        let authorized = events.iter().filter(|e| e.0 == "approval_use" && e.1 == "authorized").count();
        assert_eq!(consumed, authorized, "every consumption was audited first, and nothing was consumed unaudited");
    }
}

fn seed_user_data(r: &Rehearsal) {
    for (rel, data) in [
        (format!("data/{TOOL}/settings.db"), &b"the user data of the tool"[..]),
        ("data/another-tool/keep.txt".to_string(), &b"unrelated tool data"[..]),
    ] {
        let p = r.l.p(&rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, data).unwrap();
    }
}

#[test]
fn the_full_synthetic_pilot_rehearsal_runs_from_preflight_to_recovery_and_touches_nothing_unrelated() {
    let r = rehearsal();
    seed_user_data(&r);
    let user_before = r.user_files();
    assert_eq!(user_before.len(), 2);
    let v1 = &r.l.v1;

    // 1. preflight: offline, changes nothing, creates no journal and consumes no approval
    let before = r.tree();
    let pre_approval = make_approval(v1, Action::Install, None, COMPUTER, t0());
    let v = verified(v1, &pre_approval, Action::Install, None, COMPUTER, t0());
    let pf = r
        .with_adapter(None, |a, _| a.preflight(&Authority { recipe: &v1.valid, approval: &v }, Action::Install, Some(&v1.sc.bytes)))
        .unwrap();
    assert!(!pf.already_applied && pf.installed_version.is_none());
    assert_eq!(pf.artifact_sha256.as_deref(), Some(v1.valid.recipe().artifact.sha256.as_str()));
    assert_eq!(r.tree(), before, "preflight changed nothing");
    assert!(r.ledger_rows().is_empty(), "preflight consumed and issued nothing");

    // 2-3. the reviewed recipe and its canonical plan (the plan itself is not an approval)
    assert_eq!(v1.valid.digest().len(), 64);
    assert!(v1.plan.eligibility.blockers.iter().any(|b| b.code == "user_approval_missing"));

    // 4. approval: Phase 3A binding + a device approval proven present by the signing operation; ledger and audit are durable
    let auth = r.auth(v1, Action::Install, None);
    assert!(auth.1.presence_confirmed);
    assert_eq!(r.ledger_rows(), vec![(auth.1.approval.approval_id.clone(), "issued".to_string())]);

    // 5-7. file-system preparation, atomic activation and the receipt, in one transaction
    let out = r.install_with(v1, &auth, None).unwrap();
    assert!(!out.already_applied && out.receipt.synthetic);
    assert_eq!((out.receipt.action, out.receipt.to_version.as_deref()), (Action::Install, Some("1.0.0")));
    assert_eq!(out.receipt.kept, vec!["user_data".to_string()]);
    assert_eq!(r.l.artifact_of("1.0.0").unwrap(), v1.sc.bytes);
    assert_eq!(r.l.files("receipts").len(), 1);
    r.l.assert_clean();
    assert_eq!(r.ledger_rows(), vec![(auth.1.approval.approval_id.clone(), "consumed".to_string())]);
    // presenting the same, used device approval again while the state already matches is an idempotent no-op (nothing can change)
    let replay_same_state = r.install_with(v1, &auth, None).unwrap();
    assert!(replay_same_state.already_applied && replay_same_state.receipt == out.receipt);
    let restarted = ApprovalStore::open(&r.ledger_path).unwrap();
    assert_eq!(
        restarted.status(&auth.1.approval.approval_id).unwrap(),
        Some(device_trust::LedgerStatus::Consumed { txn_id: out.receipt.txn_id.clone() })
    );

    // 8. idempotence: a fresh, valid approval for the same state changes nothing and returns the same receipt
    let tree_installed = r.tree();
    let again = r.install(v1, None).unwrap();
    assert!(again.already_applied && again.receipt == out.receipt);
    assert_eq!(r.tree(), tree_installed, "idempotent: not a byte changed");

    // 9. rollback: a failed update (injected at the moment the pointer has switched) puts version 1 back byte-for-byte
    let pointer = fs::read(r.l.p(&format!("state/{TOOL}.json"))).unwrap();
    let (code, report) = failed(r.update(&r.l.v2, "1.0.0", Some(&at(Point::AfterPointerSwitch, Fault::Fail))));
    assert_eq!(code, "injected_failure");
    assert!(matches!(report.status.as_str(), "restored" | "not_needed") && report.not_undone.is_empty(), "{report:?}");
    assert_eq!(r.l.active().as_deref(), Some("1.0.0"));
    assert_eq!(fs::read(r.l.p(&format!("state/{TOOL}.json"))).unwrap(), pointer);
    let after_rollback: Vec<_> = r.tree().into_iter().filter(|(p, _)| !p.starts_with("journal/")).collect();
    let installed_wo_journal: Vec<_> = tree_installed.iter().filter(|(p, _)| !p.starts_with("journal/")).cloned().collect();
    assert_eq!(after_rollback, installed_wo_journal, "the rolled-back update left no trace except its journal record");
    let update_ok = r.update(&r.l.v2, "1.0.0", None).unwrap();
    assert_eq!(update_ok.receipt.to_version.as_deref(), Some("2.0.0"));

    // 10. uninstall: the tool's program and pointer go, its user data stays
    let un = r.uninstall(&r.l.v2, None).unwrap();
    assert_eq!(un.receipt.action, Action::Uninstall);
    assert_eq!(r.l.active(), None);
    assert!(!r.l.p(&format!("versions/{TOOL}")).exists());
    assert_eq!(r.user_files(), user_before, "user data of every tool is byte-identical");
    // single use where it matters: the used install approval cannot be replayed to install again once the state has moved on
    assert!(r.install_with(v1, &auth, None).is_err(), "a consumed approval cannot start a new transaction");
    assert_eq!(r.l.active(), None);

    // 11. recovery: a crash in the middle of a fresh install is recovered, repeatedly, to a consistent state
    let crashed = r.install(v1, Some(&at(Point::AfterPointerSwitch, Fault::Crash)));
    assert!(matches!(crashed, Err(TxnError::Interrupted(Point::AfterPointerSwitch))));
    assert!(matches!(r.install(v1, None), Err(TxnError::Busy)), "a stale lock blocks new work until recovery");
    let rec = r.with_adapter(None, |a, _| a.recover()).unwrap();
    assert!(matches!(&rec[..], [Recovered::RolledBack(_)]), "{rec:?}");
    assert_eq!(r.l.active(), None);
    assert!(r.with_adapter(None, |a, _| a.recover()).unwrap().is_empty(), "recovery is idempotent");
    r.l.assert_clean();
    assert!(r.install(v1, None).is_ok(), "and the tool can be installed again afterwards");

    // the end: nothing unrelated changed, and the trust trail is whole
    assert_eq!(r.user_files(), user_before);
    assert_eq!(fs::read(r.l.base.join("canary.txt")).unwrap(), b"untouched");
    assert!(r.l.journals().iter().all(|j| j.state.terminal()));
    r.assert_trust_is_consistent();
    let rows = r.ledger_rows();
    let mut ids: Vec<&String> = rows.iter().map(|x| &x.0).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), rows.len(), "every approval id was issued once");
    // presence was demanded of the (fake) OS operation once for the key and once per issued approval: nothing was approved silently
    assert_eq!(r.backend.prompts.load(std::sync::atomic::Ordering::SeqCst), 1 + rows.len());
}

#[test]
fn a_fault_at_any_point_through_the_durable_trust_path_rolls_back_and_a_used_approval_never_works_twice() {
    for &point in FRESH_POINTS {
        let r = rehearsal();
        seed_user_data(&r);
        let user_before = r.user_files();
        let auth = r.auth(&r.l.v1, Action::Install, None);
        let result = r.install_with(&r.l.v1, &auth, Some(&at(point, Fault::Fail)));
        let (code, report) = failed(result);
        assert_eq!(code, "injected_failure", "{point:?}");
        assert!(matches!(report.status.as_str(), "restored" | "not_needed") && report.not_undone.is_empty(), "{point:?}: {report:?}");
        assert_eq!(r.l.active(), None, "{point:?}");
        assert!(r.l.files("receipts").is_empty() && r.l.files("state").is_empty(), "{point:?}");
        r.l.assert_clean();
        assert_eq!(r.user_files(), user_before, "{point:?}");
        // the approval is consumed by the failed transaction (before the first change): it is not a free retry
        let status = r.store.status(&auth.1.approval.approval_id).unwrap();
        if matches!(status, Some(device_trust::LedgerStatus::Consumed { .. })) {
            assert!(r.install_with(&r.l.v1, &auth, None).is_err(), "{point:?}: no replay of a consumed approval");
        }
        // a NEW approval (a new, present person) installs fine
        assert!(r.install(&r.l.v1, None).is_ok(), "{point:?}");
        r.assert_trust_is_consistent();
    }
}

#[test]
fn a_crash_at_any_point_through_the_durable_trust_path_is_recovered_and_the_ledger_stays_consistent() {
    for &point in FRESH_POINTS {
        if ambiguous_window(point) {
            continue; // covered by the Phase 3C orphan-resolution tests; needs the owner's explicit resolution
        }
        let r = rehearsal();
        seed_user_data(&r);
        let user_before = r.user_files();
        let crashed = r.install(&r.l.v1, Some(&at(point, Fault::Crash)));
        assert!(matches!(crashed, Err(TxnError::Interrupted(p)) if p == point), "{point:?}: {crashed:?}");
        let out = r.with_adapter(None, |a, _| a.recover()).unwrap();
        assert_eq!(out.len(), 1, "{point:?}: {out:?}");
        assert_eq!(r.l.active().is_some(), rolls_forward(point), "{point:?}");
        r.l.assert_clean();
        assert_eq!(r.user_files(), user_before, "{point:?}");
        assert!(r.with_adapter(None, |a, _| a.recover()).unwrap().is_empty());
        r.assert_trust_is_consistent();
        // restart: the durable ledger still knows the consumed approval
        let reopened = ApprovalStore::open(&r.ledger_path).unwrap();
        let consumed = reopened.audit_events().unwrap().iter().filter(|e| e.0 == "approval_use" && e.1 == "authorized").count();
        assert!(consumed <= 1, "{point:?}");
    }
}

#[test]
fn the_rehearsal_cannot_authorize_the_production_adapter_and_a_wrong_device_approval_changes_nothing() {
    let r = rehearsal();
    seed_user_data(&r);
    let before = r.tree();
    // the production adapter refuses everything, whatever approval it is shown
    let auth = r.auth(&r.l.v1, Action::Install, None);
    let keys = OsKeyStore::open(&r.store, &r.backend, &r.backend, COMPUTER).unwrap();
    let c = clock();
    let prod = Adapter::production(
        &r.l.root,
        &r.l.refs,
        this_platform(),
        &c,
        Trust { keys: &keys, ledger: &r.store, audit: &r.store, policy: TrustPolicy::production() },
    );
    assert_eq!(
        refused(prod.install(&Authority { recipe: &r.l.v1.valid, approval: &auth.0 }, &auth.1, &r.l.v1.sc.bytes)),
        "production_adapter_not_enabled"
    );
    assert_eq!(r.tree(), before);
    // a device approval issued for another recipe version cannot install this one
    let other = r.auth(&r.l.v2, Action::Install, None);
    let wrong = r.with_adapter(None, |a, _| a.install(&Authority { recipe: &r.l.v1.valid, approval: &auth.0 }, &other.1, &r.l.v1.sc.bytes));
    assert!(matches!(wrong, Err(TxnError::Refused(_))), "{wrong:?}");
    assert_eq!(r.l.active(), None);
    assert_eq!(r.l.files("receipts").len(), 0);
}

// ---- Phase 3D-1 closure: the future notice sidecar, tested with SYNTHETIC files only -------------------------------------------------

#[test]
fn a_synthetic_notice_sidecar_is_never_deleted_adopted_or_ignored_and_blocks_destructive_operations_until_the_adapter_owns_it() {
    let l = lab();
    l.install(&l.v1, None).unwrap();
    // a made-up notice file beside the installed artifact (NOT a real licence text; nothing third-party is involved)
    let sidecar = l.p(&format!("versions/{TOOL}/1.0.0/NOTICE.txt"));
    let notice = b"SYNTHETIC notice text for a test: not a licence";
    fs::write(&sidecar, notice).unwrap();
    let artifact_before = fs::read(installed(&l)).unwrap();
    // reported, never adopted: repeating the install is refused instead of calling the folder "installed", and nothing is consumed or changed
    assert_refused_without_touching(&l, "installed_folder_has_unexpected_entries");
    // uninstalling is refused BEFORE anything moves
    let tree_before = snapshot(&l).0;
    assert_eq!(refused(l.uninstall(&l.v1, None)), "unrecognised_installed_content");
    let tree_after = snapshot(&l).0;
    let without_journal = |t: &std::collections::BTreeMap<String, Vec<u8>>| {
        t.iter().filter(|(k, _)| !k.starts_with("journal/")).map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>()
    };
    assert_eq!(
        without_journal(&tree_before),
        without_journal(&tree_after),
        "apart from journal records of the refused attempt, the managed root is byte-identical"
    );
    assert_eq!(fs::read(&sidecar).unwrap(), notice);
    assert_eq!(fs::read(installed(&l)).unwrap(), artifact_before);
    assert_eq!(l.active().as_deref(), Some("1.0.0"));
    l.assert_clean();
    // an update to a NEW version works and leaves the old version folder, with its stray file, completely alone
    assert!(l.update(&l.v2, "1.0.0", None).is_ok());
    assert_eq!(fs::read(&sidecar).unwrap(), notice, "kept for rollback with the old version, byte for byte");
    // ... but the stray file still blocks uninstalling until the OWNER removes it: the adapter never deletes what it did not create
    assert_eq!(refused(l.uninstall(&l.v2, None)), "unrecognised_installed_content");
    assert_eq!(fs::read(&sidecar).unwrap(), notice);
    assert_eq!(l.active().as_deref(), Some("2.0.0"));
    l.assert_clean();
    l.outside_is_untouched_but_data();
    // the owner removes it by hand; the normal operations work again
    fs::remove_file(&sidecar).unwrap();
    assert!(l.uninstall(&l.v2, None).is_ok());
}
