//! Phase 3A synthetic-lifecycle tests. Every test runs on isolated temporary directories with made-up artifacts: nothing is
//! downloaded, started or installed, and nothing outside the sandbox directory is ever written.

use super::*;
use crate::{
    approval::{Action, Approval},
    synthetic::testkit::*,
};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    sync::{Arc, Barrier},
};
use tempfile::TempDir;

const COMPUTER: &str = "synthetic-computer-1";
const TOOL: &str = "synthetic-tool";

/// A temporary world: a base directory (with a canary beside the sandbox), one sandbox, and two versions of one synthetic tool.
struct Lab {
    _tmp: TempDir,
    base: PathBuf,
    root: SandboxRoot,
    v1: Fx,
    v2: Fx,
    refs: ReferenceSet,
}

fn lab_in(sub: &str) -> Lab {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap().join(sub);
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("canary.txt"), b"untouched").unwrap();
    let root = SandboxRoot::create(&base, COMPUTER).unwrap();
    let v1 = fx(TOOL, "1.0.0", b"payload one");
    let v2 = fx(TOOL, "2.0.0", b"payload two");
    let mut refs = v1.sc.refs.clone();
    refs.authenticated.extend(v2.sc.refs.authenticated.clone());
    Lab { _tmp: tmp, base, root, v1, v2, refs }
}

fn lab() -> Lab {
    lab_in("base")
}

fn clock() -> impl Fn() -> DateTime<Utc> + Sync {
    t0
}

impl Lab {
    fn engine<'a>(&'a self, clock: ClockFn<'a>, fault: Option<FaultFn<'a>>) -> Engine<'a> {
        Engine { root: &self.root, refs: &self.refs, host_platform: this_platform(), clock, fault }
    }

    fn approval(&self, f: &Fx, action: Action, from: Option<&str>) -> VerifiedApproval {
        let a: Approval = make_approval(f, action, from, COMPUTER, t0());
        verified(f, &a, action, from, COMPUTER, t0())
    }

    fn install(&self, f: &Fx, fault: Option<FaultFn>) -> Result<Outcome, TxnError> {
        let ap = self.approval(f, Action::Install, None);
        let c = clock();
        self.engine(&c, fault).install(&Authority { recipe: &f.valid, approval: &ap }, &f.sc.bytes)
    }

    fn update(&self, f: &Fx, from: &str, fault: Option<FaultFn>) -> Result<Outcome, TxnError> {
        let ap = self.approval(f, Action::Update, Some(from));
        let c = clock();
        self.engine(&c, fault).update(&Authority { recipe: &f.valid, approval: &ap }, &f.sc.bytes)
    }

    fn uninstall(&self, f: &Fx, fault: Option<FaultFn>) -> Result<Outcome, TxnError> {
        let ap = self.approval(f, Action::Uninstall, None);
        let c = clock();
        self.engine(&c, fault).uninstall(&Authority { recipe: &f.valid, approval: &ap })
    }

    fn recover(&self, fault: Option<FaultFn>) -> Vec<Recovered> {
        let c = clock();
        self.engine(&c, fault).recover().unwrap()
    }

    fn active(&self) -> Option<String> {
        let c = clock();
        self.engine(&c, None).active_version(TOOL).unwrap()
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.root.path().join(rel)
    }

    fn journals(&self) -> Vec<Journal> {
        let mut v: Vec<Journal> = fs::read_dir(self.p("journal"))
            .unwrap()
            .flatten()
            .filter_map(|e| serde_json::from_str(&fs::read_to_string(e.path()).ok()?).ok())
            .collect();
        v.sort_by(|a, b| a.started_at.cmp(&b.started_at).then(a.txn_id.cmp(&b.txn_id)));
        v
    }

    fn files(&self, dir: &str) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(self.p(dir))
            .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    /// Nothing left over of an unfinished transaction: no staging, no snapshots, no locks.
    fn assert_clean(&self) {
        assert!(self.files("staging").is_empty(), "staging: {:?}", self.files("staging"));
        assert!(self.files("snapshots").is_empty(), "snapshots: {:?}", self.files("snapshots"));
        assert!(self.files("locks").is_empty(), "locks: {:?}", self.files("locks"));
    }

    fn artifact_of(&self, version: &str) -> Option<Vec<u8>> {
        fs::read(self.p(&format!("versions/{TOOL}/{version}/artifact.bin"))).ok()
    }

    /// Everything below the base directory except the sandbox: must never change.
    fn outside_is_untouched(&self) {
        let mut names: Vec<String> =
            fs::read_dir(&self.base).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names.contains(&"canary.txt".to_string()) && names.iter().any(|n| n.starts_with(SANDBOX_PREFIX)));
        assert_eq!(fs::read(self.base.join("canary.txt")).unwrap(), b"untouched");
    }
}

fn failed(r: Result<Outcome, TxnError>) -> (&'static str, RollbackReport) {
    match r {
        Err(TxnError::Failed { code, rollback }) => (code, rollback),
        other => panic!("expected a failed transaction, got {other:?}"),
    }
}

fn refused(r: Result<Outcome, TxnError>) -> &'static str {
    match r {
        Err(TxnError::Refused(c)) => c,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn at(point: Point, fault: Fault) -> impl Fn(Point) -> Option<Fault> + Sync {
    move |p| (p == point).then_some(fault)
}

const INSTALL_POINTS: &[Point] = &[
    Point::AfterLock,
    Point::AfterSnapshot,
    Point::DuringStageWrite,
    Point::AfterStageWrite,
    Point::AfterVerify,
    Point::BeforeVersionDirCreate,
    Point::AfterVersionDirCreate,
    Point::BeforePointerWrite,
    Point::AfterPointerSwitch,
    Point::BeforeReceiptWrite,
    Point::AfterReceiptWrite,
    Point::BeforeJournalCommit,
];

// ---- the successful lifecycle ------------------------------------------------------------------------------------------------

#[test]
fn an_install_stages_verifies_activates_and_leaves_a_durable_receipt_without_touching_anything_else() {
    let l = lab();
    let out = l.install(&l.v1, None).unwrap();
    assert!(!out.already_applied);
    let r = &out.receipt;
    assert_eq!(
        (r.action, r.tool_id.as_str(), r.to_version.as_deref(), r.from_version.clone()),
        (Action::Install, TOOL, Some("1.0.0"), None)
    );
    assert!(r.synthetic && r.state == "committed" && r.rollback.status == "not_needed");
    assert_eq!(r.artifact_sha256.as_deref(), Some(l.v1.valid.recipe().artifact.sha256.as_str()));
    assert_eq!(l.active().as_deref(), Some("1.0.0"));
    assert_eq!(l.artifact_of("1.0.0").unwrap(), l.v1.sc.bytes);
    let c = clock();
    assert_eq!(l.engine(&c, None).read_receipt(&r.txn_id).as_ref(), Some(r), "the receipt is durable and its digest holds");
    let j = &l.journals()[0];
    assert_eq!((j.state, j.txn_id.as_str()), (TxnState::Committed, r.txn_id.as_str()));
    assert!(j.intents.contains(&"pointer_switch".to_string()));
    l.assert_clean();
    l.outside_is_untouched();
}

#[test]
fn repeating_an_install_is_a_no_op_that_returns_the_same_receipt_and_creates_nothing() {
    let l = lab();
    let first = l.install(&l.v1, None).unwrap();
    let before = (l.files("journal"), l.files("receipts"), l.files("versions"), fs::read(l.p("state/synthetic-tool.json")).unwrap());
    for _ in 0..3 {
        let again = l.install(&l.v1, None).unwrap();
        assert!(again.already_applied);
        assert_eq!(again.receipt, first.receipt);
    }
    assert_eq!(before, (l.files("journal"), l.files("receipts"), l.files("versions"), fs::read(l.p("state/synthetic-tool.json")).unwrap()));
    l.assert_clean();
}

#[test]
fn an_update_switches_the_pointer_atomically_keeps_the_previous_version_and_records_where_it_came_from() {
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let out = l.update(&l.v2, "1.0.0", None).unwrap();
    assert_eq!((out.receipt.from_version.as_deref(), out.receipt.to_version.as_deref()), (Some("1.0.0"), Some("2.0.0")));
    assert_eq!(l.active().as_deref(), Some("2.0.0"));
    assert_eq!(l.artifact_of("2.0.0").unwrap(), l.v2.sc.bytes);
    assert_eq!(l.artifact_of("1.0.0").unwrap(), l.v1.sc.bytes, "the previous version stays in place");
    // idempotent
    let again = l.update(&l.v2, "1.0.0", None).unwrap();
    assert!(again.already_applied && again.receipt == out.receipt);
    l.assert_clean();
}

#[test]
fn an_update_needs_an_installed_tool_at_exactly_the_approved_source_version() {
    let l = lab();
    assert_eq!(refused(l.update(&l.v2, "1.0.0", None)), "not_installed");
    l.install(&l.v1, None).unwrap();
    assert_eq!(refused(l.update(&l.v2, "0.5.0", None)), "installed_version_differs_from_the_approved_update_source");
    assert_eq!(refused(l.install(&l.v2, None)), "already_installed_use_update");
    assert_eq!(l.active().as_deref(), Some("1.0.0"));
    assert!(l.artifact_of("2.0.0").is_none());
    l.assert_clean();
}

#[test]
fn an_uninstall_removes_the_managed_version_folders_and_pointer_but_never_the_tools_own_data() {
    let l = lab();
    fs::create_dir_all(l.p(&format!("data/{TOOL}"))).unwrap();
    fs::write(l.p(&format!("data/{TOOL}/index.db")), b"user's own data").unwrap();
    l.install(&l.v1, None).unwrap();
    l.update(&l.v2, "1.0.0", None).unwrap();
    let out = l.uninstall(&l.v2, None).unwrap();
    assert_eq!(out.receipt.action, Action::Uninstall);
    assert_eq!(out.receipt.kept, vec!["user_data".to_string()]);
    assert_eq!(l.active(), None);
    assert!(!l.p(&format!("versions/{TOOL}")).exists());
    assert_eq!(fs::read(l.p(&format!("data/{TOOL}/index.db"))).unwrap(), b"user's own data");
    l.assert_clean();
    // not installed any more: a second uninstall changes nothing and says so
    let before = (l.files("journal"), l.files("receipts"));
    assert_eq!(refused(l.uninstall(&l.v2, None)), "not_installed");
    assert_eq!(before, (l.files("journal"), l.files("receipts")));
    // and a fresh install works again
    assert_eq!(l.install(&l.v1, None).unwrap().receipt.to_version.as_deref(), Some("1.0.0"));
}

// ---- preflight: identity, platform, authority ---------------------------------------------------------------------------------

#[test]
fn a_corrupted_wrong_sized_or_unauthenticated_artifact_never_reaches_staging() {
    let l = lab();
    let ap = l.approval(&l.v1, Action::Install, None);
    let c = clock();
    let good = l.v1.sc.bytes.clone();
    let mut flipped = good.clone();
    *flipped.last_mut().unwrap() ^= 1;
    let mut extended = good.clone();
    extended.push(0);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("flipped byte", flipped),
        ("truncated", good[..good.len() - 1].to_vec()),
        ("extended", extended),
        ("empty", vec![]),
        ("another version's artifact", l.v2.sc.bytes.clone()),
        ("a header-less file of the same length", vec![b'x'; good.len()]),
    ];
    for (name, bytes) in cases {
        let r = l.engine(&c, None).install(&Authority { recipe: &l.v1.valid, approval: &ap }, &bytes);
        assert_eq!(refused(r), "artifact_does_not_match_recipe", "{name}");
    }
    // the right bytes, but this machine has no authenticated reference for them
    let empty = ReferenceSet::default();
    let e = Engine { root: &l.root, refs: &empty, host_platform: this_platform(), clock: &c, fault: None };
    assert_eq!(
        refused(e.install(&Authority { recipe: &l.v1.valid, approval: &ap }, &good)),
        "artifact_not_authenticated_for_this_platform"
    );
    // a reference for another version only
    let other_version = ReferenceSet { authenticated: l.v2.sc.refs.authenticated.clone(), unauthenticated: vec![] };
    let e = Engine { root: &l.root, refs: &other_version, host_platform: this_platform(), clock: &c, fault: None };
    assert_eq!(
        refused(e.install(&Authority { recipe: &l.v1.valid, approval: &ap }, &good)),
        "artifact_not_authenticated_for_this_platform"
    );
    assert!(l.files("journal").is_empty() && l.files("staging").is_empty() && l.active().is_none(), "nothing was started");
    l.outside_is_untouched();
}

#[test]
fn an_artifact_of_the_wrong_platform_or_architecture_is_refused() {
    let l = lab();
    let ap = l.approval(&l.v1, Action::Install, None);
    let c = clock();
    // this machine claims to be the other platform: the approval (bound to the real one) no longer matches
    let e = Engine { root: &l.root, refs: &l.refs, host_platform: other_platform(), clock: &c, fault: None };
    assert_eq!(refused(e.install(&Authority { recipe: &l.v1.valid, approval: &ap }, &l.v1.sc.bytes)), "approval_does_not_match");
    // a recipe + approval for the other platform on this machine
    let foreign = fx_on(other_platform(), TOOL, "1.0.0", b"payload one");
    let fa = {
        let a = make_approval(&foreign, Action::Install, None, COMPUTER, t0());
        verified(&foreign, &a, Action::Install, None, COMPUTER, t0())
    };
    assert_eq!(
        refused(l.engine(&c, None).install(&Authority { recipe: &foreign.valid, approval: &fa }, &foreign.sc.bytes)),
        "approval_does_not_match"
    );
    // the other platform's binary offered for this platform's recipe (same name, different header → different digest and format)
    assert_eq!(
        refused(l.engine(&c, None).install(&Authority { recipe: &l.v1.valid, approval: &ap }, &foreign.sc.bytes)),
        "artifact_does_not_match_recipe"
    );
    assert!(l.active().is_none() && l.files("journal").is_empty());
}

#[test]
fn the_engine_checks_authority_itself_computer_action_recipe_and_expiry() {
    let l = lab();
    let c = clock();
    let ap = l.approval(&l.v1, Action::Install, None);
    // a different action than the approval's
    assert_eq!(
        refused(l.engine(&c, None).update(&Authority { recipe: &l.v1.valid, approval: &ap }, &l.v1.sc.bytes)),
        "approval_does_not_match"
    );
    assert_eq!(refused(l.engine(&c, None).uninstall(&Authority { recipe: &l.v1.valid, approval: &ap })), "approval_does_not_match");
    // an approval for another recipe revision / version
    assert_eq!(
        refused(l.engine(&c, None).install(&Authority { recipe: &l.v2.valid, approval: &ap }, &l.v2.sc.bytes)),
        "approval_does_not_match"
    );
    // an approval for another computer: a sandbox that belongs to a different machine
    let other_root = SandboxRoot::create(&l.base, "synthetic-computer-2").unwrap();
    let e = Engine { root: &other_root, refs: &l.refs, host_platform: this_platform(), clock: &c, fault: None };
    assert_eq!(refused(e.install(&Authority { recipe: &l.v1.valid, approval: &ap }, &l.v1.sc.bytes)), "approval_does_not_match");
    // expired by the time the engine runs
    let late = || t0() + chrono::Duration::hours(1);
    let e = Engine { root: &l.root, refs: &l.refs, host_platform: this_platform(), clock: &late, fault: None };
    assert_eq!(refused(e.install(&Authority { recipe: &l.v1.valid, approval: &ap }, &l.v1.sc.bytes)), "approval_expired");
    assert!(l.active().is_none() && l.files("journal").is_empty());
}

#[test]
fn a_non_synthetic_recipe_can_never_run_even_with_a_matching_approval() {
    let l = lab();
    let c = clock();
    let mut real_looking = l.v1.valid.recipe().clone();
    real_looking.synthetic = false;
    real_looking.tool_id = "gitleaks".to_string();
    // The engine is handed a validated recipe that is not synthetic; whatever approval accompanies it cannot match it.
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
        let ap = l.approval(&l.v1, Action::Install, None);
        assert_eq!(refused(l.engine(&c, None).install(&Authority { recipe: &v, approval: &ap }, &l.v1.sc.bytes)), "not_a_synthetic_recipe");
    }
    assert!(l.active().is_none() && l.files("journal").is_empty());
    // a registry refuses real-looking recipes outright
    assert!(crate::recipe::RecipeRegistry::synthetic(vec![rr]).is_err());
}

// ---- failures, rollback ------------------------------------------------------------------------------------------------------

#[test]
fn a_failure_at_any_point_of_an_install_is_rolled_back_completely_and_a_retry_succeeds() {
    for &point in INSTALL_POINTS {
        let l = lab();
        let f = at(point, Fault::Fail);
        let (code, report) = failed(l.install(&l.v1, Some(&f)));
        assert_eq!(code, "injected_failure", "{point:?}");
        assert!(matches!(report.status.as_str(), "restored" | "not_needed"), "{point:?}: {report:?}");
        assert!(report.not_undone.is_empty(), "{point:?}");
        assert_eq!(l.active(), None, "{point:?}");
        assert!(l.artifact_of("1.0.0").is_none() && !l.p(&format!("versions/{TOOL}")).exists(), "{point:?}");
        assert!(l.files("receipts").is_empty(), "{point:?}: a receipt of a transaction that did not commit");
        l.assert_clean();
        let j = &l.journals()[0];
        assert_eq!(j.state, TxnState::RolledBack, "{point:?}");
        assert_eq!(j.rollback.as_ref().map(|r| r.status.as_str()), Some(report.status.as_str()));
        l.outside_is_untouched();
        // the world is as before: the same install now succeeds
        assert_eq!(l.install(&l.v1, None).unwrap().receipt.to_version.as_deref(), Some("1.0.0"), "{point:?}");
    }
}

#[test]
fn a_failed_update_puts_the_previous_version_back_exactly() {
    for &point in INSTALL_POINTS {
        let l = lab();
        l.install(&l.v1, None).unwrap();
        let pointer_before = fs::read(l.p("state/synthetic-tool.json")).unwrap();
        let receipts_before = l.files("receipts");
        let f = at(point, Fault::Fail);
        let (_, report) = failed(l.update(&l.v2, "1.0.0", Some(&f)));
        assert!(matches!(report.status.as_str(), "restored" | "not_needed"), "{point:?}: {report:?}");
        assert_eq!(l.active().as_deref(), Some("1.0.0"), "{point:?}");
        assert_eq!(fs::read(l.p("state/synthetic-tool.json")).unwrap(), pointer_before, "{point:?}: byte-identical pointer");
        assert_eq!(l.artifact_of("1.0.0").unwrap(), l.v1.sc.bytes, "{point:?}");
        assert!(l.artifact_of("2.0.0").is_none(), "{point:?}");
        assert_eq!(l.files("receipts"), receipts_before, "{point:?}");
        l.assert_clean();
        // and the update still works afterwards
        assert_eq!(l.update(&l.v2, "1.0.0", None).unwrap().receipt.to_version.as_deref(), Some("2.0.0"), "{point:?}");
    }
}

#[test]
fn a_failed_uninstall_puts_everything_back() {
    for point in [
        Point::AfterLock,
        Point::AfterSnapshot,
        Point::UninstallAfterPointerMoved,
        Point::UninstallAfterVersionsMoved,
        Point::BeforeReceiptWrite,
        Point::AfterReceiptWrite,
        Point::BeforeJournalCommit,
    ] {
        let l = lab();
        fs::create_dir_all(l.p(&format!("data/{TOOL}"))).unwrap();
        fs::write(l.p(&format!("data/{TOOL}/keep.txt")), b"mine").unwrap();
        l.install(&l.v1, None).unwrap();
        l.update(&l.v2, "1.0.0", None).unwrap();
        let pointer_before = fs::read(l.p("state/synthetic-tool.json")).unwrap();
        let f = at(point, Fault::Fail);
        let (_, report) = failed(l.uninstall(&l.v2, Some(&f)));
        assert!(matches!(report.status.as_str(), "restored" | "not_needed"), "{point:?}: {report:?}");
        assert_eq!(l.active().as_deref(), Some("2.0.0"), "{point:?}");
        assert_eq!(fs::read(l.p("state/synthetic-tool.json")).unwrap(), pointer_before, "{point:?}");
        assert_eq!(l.artifact_of("1.0.0").unwrap(), l.v1.sc.bytes, "{point:?}");
        assert_eq!(l.artifact_of("2.0.0").unwrap(), l.v2.sc.bytes, "{point:?}");
        assert_eq!(fs::read(l.p(&format!("data/{TOOL}/keep.txt"))).unwrap(), b"mine");
        assert!(report.not_covered.iter().any(|n| n.contains("user data")), "rollback says what it does not cover: {report:?}");
        l.assert_clean();
    }
}

#[test]
fn approval_that_lapses_between_checks_stops_the_transaction_before_the_first_change_to_managed_state() {
    let l = lab();
    let ap = l.approval(&l.v1, Action::Install, None);
    // the clock reads "valid" for the first calls and "expired" afterwards
    let calls = AtomicUsize::new(0);
    let clock = || if calls.fetch_add(1, Ordering::SeqCst) < 2 { t0() } else { t0() + chrono::Duration::hours(1) };
    let e = l.engine(&clock, None);
    let (code, report) = failed(e.install(&Authority { recipe: &l.v1.valid, approval: &ap }, &l.v1.sc.bytes));
    assert_eq!(code, "approval_expired");
    assert!(matches!(report.status.as_str(), "restored" | "not_needed"));
    assert!(l.active().is_none() && l.artifact_of("1.0.0").is_none());
    l.assert_clean();
}

#[test]
fn rollback_that_cannot_undo_something_says_so_keeps_the_lock_and_can_be_finished_later() {
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let pointer_v1 = fs::read(l.p("state/synthetic-tool.json")).unwrap();
    // the update fails after the pointer was switched, and then the pointer cannot be restored either
    let f = |p: Point| match p {
        Point::AfterPointerSwitch => Some(Fault::Fail),
        Point::RollbackPointerRestore => Some(Fault::Fail),
        _ => None,
    };
    let (_, report) = failed(l.update(&l.v2, "1.0.0", Some(&f)));
    assert_eq!(report.status, "partial", "{report:?}");
    assert!(report.not_undone.iter().any(|n| n.what.contains("pointer")), "{report:?}");
    assert!(!report.undone.iter().any(|u| u.contains("pointer")), "it must not claim the pointer was restored");
    assert!(report.undone.iter().any(|u| u.contains("staging")), "what was undone is stated: {report:?}");
    // the truth on disk matches the report: the pointer still points at 2.0.0, and that folder is NOT removed under it
    assert!(!report.undone.iter().any(|u| u.contains("version folder")), "{report:?}");
    assert!(report.not_undone.iter().any(|n| n.what.contains("version folder 2.0.0")), "{report:?}");
    assert_eq!(l.active().as_deref(), Some("2.0.0"));
    assert!(l.artifact_of("2.0.0").is_some(), "no dangling pointer");
    let update_journal = l.journals().into_iter().find(|j| j.action == Action::Update).unwrap();
    assert_eq!(update_journal.state, TxnState::RollbackFailed);
    assert_eq!(l.files("locks"), vec!["synthetic-tool.lock".to_string()], "the lock stays: a person or the next start has to finish this");
    // nothing new can start on this tool meanwhile
    assert!(matches!(l.update(&l.v2, "1.0.0", None), Err(TxnError::Busy)));
    // recovery on a later start, without the fault, finishes the job
    let recovered = l.recover(None);
    assert!(recovered.iter().any(|r| matches!(r, Recovered::RolledBack(_))), "{recovered:?}");
    assert_eq!(l.active().as_deref(), Some("1.0.0"));
    assert_eq!(fs::read(l.p("state/synthetic-tool.json")).unwrap(), pointer_v1);
    l.assert_clean();
}

#[test]
fn recovery_that_still_cannot_undo_reports_needs_attention_and_leaves_the_lock() {
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let crash = at(Point::AfterPointerSwitch, Fault::Crash);
    assert!(matches!(l.update(&l.v2, "1.0.0", Some(&crash)), Err(TxnError::Interrupted(Point::AfterPointerSwitch))));
    let still_broken = at(Point::RollbackPointerRestore, Fault::Fail);
    let out = l.recover(Some(&still_broken));
    match &out[..] {
        [Recovered::NeedsAttention(_, report)] => {
            assert!(report.not_undone.iter().any(|n| n.what.contains("pointer")));
            assert_ne!(report.status, "restored");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(l.files("locks").len(), 1);
    assert_eq!(l.active().as_deref(), Some("2.0.0"), "reported honestly: still the new pointer");
    // a later start, with the obstacle gone
    assert!(matches!(&l.recover(None)[..], [Recovered::RolledBack(_)]));
    assert_eq!(l.active().as_deref(), Some("1.0.0"));
    l.assert_clean();
}

#[test]
fn rollback_never_deletes_a_version_folder_that_no_longer_holds_what_it_wrote() {
    let l = lab();
    let version_dir = l.p(&format!("versions/{TOOL}/1.0.0/artifact.bin"));
    // right after the artifact was placed, something else rewrites the file; then the install fails
    let tamper = {
        let target = version_dir.clone();
        move |p: Point| {
            if p == Point::AfterPointerSwitch {
                fs::write(&target, b"someone else's content").unwrap();
                Some(Fault::Fail)
            } else {
                None
            }
        }
    };
    let (_, report) = failed(l.install(&l.v1, Some(&tamper)));
    assert!(report.not_undone.iter().any(|n| n.what.contains("version folder") && n.why.contains("left untouched")), "{report:?}");
    assert_eq!(fs::read(&version_dir).unwrap(), b"someone else's content", "foreign content survives");
    assert_ne!(report.status, "restored");
    assert_eq!(l.active(), None, "the pointer was still restored (removed)");
}

#[test]
fn rollback_reports_a_removal_that_failed_and_does_not_pretend() {
    let l = lab();
    let f = |p: Point| match p {
        Point::AfterVerify => Some(Fault::Fail),
        Point::RollbackRemoveStaging => Some(Fault::Fail),
        _ => None,
    };
    let (_, report) = failed(l.install(&l.v1, Some(&f)));
    assert_eq!(report.status, "failed", "{report:?}");
    assert!(report.not_undone.iter().any(|n| n.what == "staging folder"));
    assert!(report.undone.is_empty());
    assert_eq!(l.files("staging").len(), 1, "the leftover is real and reported");
    l.recover(None);
    l.assert_clean();
}

// ---- crash recovery --------------------------------------------------------------------------------------------------------

#[test]
fn a_crash_at_any_point_of_an_install_is_recovered_to_a_consistent_state() {
    for &point in INSTALL_POINTS {
        let l = lab();
        let crash = at(point, Fault::Crash);
        assert!(matches!(l.install(&l.v1, Some(&crash)), Err(TxnError::Interrupted(p)) if p == point), "{point:?}");
        // right after the "crash" the engine cleaned nothing: the lock and journal are there
        assert_eq!(l.files("locks").len(), 1, "{point:?}");
        assert!(
            matches!(l.install(&l.v1, None), Err(TxnError::Busy)),
            "{point:?}: a stale lock blocks new work until recovery, even when the pointer already shows the tool"
        );
        // a restart: a fresh sandbox handle, a fresh engine
        let reopened = SandboxRoot::open(l.root.path()).unwrap();
        let c = clock();
        let e = Engine { root: &reopened, refs: &l.refs, host_platform: this_platform(), clock: &c, fault: None };
        let out = e.recover().unwrap();
        let committed_already = matches!(point, Point::AfterReceiptWrite | Point::BeforeJournalCommit);
        if committed_already {
            assert!(matches!(&out[..], [Recovered::RolledForward(_)]), "{point:?}: {out:?}");
            assert_eq!(e.active_version(TOOL).unwrap().as_deref(), Some("1.0.0"), "{point:?}");
            assert_eq!(l.files("receipts").len(), 1);
            assert!(l.artifact_of("1.0.0").is_some());
        } else {
            assert!(matches!(&out[..], [Recovered::RolledBack(_)]), "{point:?}: {out:?}");
            assert_eq!(e.active_version(TOOL).unwrap(), None, "{point:?}");
            assert!(l.artifact_of("1.0.0").is_none(), "{point:?}");
            assert!(l.files("receipts").is_empty(), "{point:?}");
        }
        l.assert_clean();
        assert!(l.journals().iter().all(|j| j.state.terminal()), "{point:?}");
        // recovery is idempotent
        assert!(e.recover().unwrap().is_empty(), "{point:?}");
        l.outside_is_untouched();
        // and normal work resumes: install is a no-op if it was completed, a normal install otherwise
        let again = l.install(&l.v1, None).unwrap();
        assert_eq!(again.already_applied, committed_already, "{point:?}");
    }
}

#[test]
fn a_crash_during_an_update_never_leaves_a_mix_of_versions() {
    for &point in INSTALL_POINTS {
        let l = lab();
        l.install(&l.v1, None).unwrap();
        let crash = at(point, Fault::Crash);
        assert!(matches!(l.update(&l.v2, "1.0.0", Some(&crash)), Err(TxnError::Interrupted(_))), "{point:?}");
        l.recover(None);
        let active = l.active().unwrap();
        // whichever version is active, its folder holds exactly its own artifact, and the other one is not half-present
        let expect = if active == "1.0.0" { &l.v1.sc.bytes } else { &l.v2.sc.bytes };
        assert_eq!(&l.artifact_of(&active).unwrap(), expect, "{point:?}: active {active}");
        if active == "1.0.0" {
            assert!(l.artifact_of("2.0.0").is_none(), "{point:?}");
        }
        l.assert_clean();
        assert!(l.journals().iter().all(|j| j.state.terminal()));
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
        Point::AfterReceiptWrite,
        Point::BeforeJournalCommit,
    ] {
        let l = lab();
        l.install(&l.v1, None).unwrap();
        let crash = at(point, Fault::Crash);
        assert!(matches!(l.uninstall(&l.v1, Some(&crash)), Err(TxnError::Interrupted(_))), "{point:?}");
        let out = l.recover(None);
        match point {
            Point::AfterReceiptWrite | Point::BeforeJournalCommit => {
                assert!(matches!(&out[..], [Recovered::RolledForward(_)]), "{point:?}: {out:?}");
                assert_eq!(l.active(), None);
                assert!(!l.p(&format!("versions/{TOOL}")).exists());
            }
            _ => {
                assert!(matches!(&out[..], [Recovered::RolledBack(_)]), "{point:?}: {out:?}");
                assert_eq!(l.active().as_deref(), Some("1.0.0"), "{point:?}");
                assert_eq!(l.artifact_of("1.0.0").unwrap(), l.v1.sc.bytes, "{point:?}");
            }
        }
        l.assert_clean();
    }
}

#[test]
fn a_lock_without_a_journal_is_removed_and_a_corrupt_journal_is_reported_and_blocks_new_work() {
    // crash right after locking, before the journal was written
    let l = lab();
    fs::write(l.p("locks/synthetic-tool.lock"), "txn-never-journaled").unwrap();
    assert!(matches!(l.install(&l.v1, None), Err(TxnError::Busy)));
    assert!(matches!(&l.recover(None)[..], [Recovered::OrphanLockRemoved(_)]));
    assert!(l.install(&l.v1, None).is_ok());
    // a journal damaged by a partial write
    let l = lab();
    let crash = at(Point::AfterStageWrite, Fault::Crash);
    assert!(l.install(&l.v1, Some(&crash)).is_err());
    let jpath = fs::read_dir(l.p("journal")).unwrap().flatten().next().unwrap().path();
    let text = fs::read_to_string(&jpath).unwrap();
    fs::write(&jpath, &text[..text.len() / 2]).unwrap();
    let out = l.recover(None);
    assert!(matches!(&out[..], [Recovered::CorruptJournal(_)]), "{out:?}");
    assert_eq!(l.files("locks").len(), 1, "the lock stays; nothing is guessed");
    assert!(l.p(&format!("staging/{}", l.files("staging")[0])).exists(), "nothing was deleted on a guess");
    assert!(matches!(l.install(&l.v1, None), Err(TxnError::Busy) | Err(TxnError::Pending(_))));
    // a journal whose content was edited (digest no longer matches) is just as untrusted
    let l = lab();
    assert!(l.install(&l.v1, Some(&at(Point::AfterVerify, Fault::Crash))).is_err());
    let jpath = fs::read_dir(l.p("journal")).unwrap().flatten().next().unwrap().path();
    let edited = fs::read_to_string(&jpath).unwrap().replace("\"intents\":[", "\"intents\":[\"pointer_switch\",");
    fs::write(&jpath, edited).unwrap();
    assert!(matches!(&l.recover(None)[..], [Recovered::CorruptJournal(_)]));
}

#[test]
fn stray_temporary_files_from_an_interrupted_atomic_write_are_ignored_and_cleaned() {
    let l = lab();
    fs::write(l.p("journal/txn-x.tmp"), b"half a journal").unwrap();
    fs::write(l.p("state/synthetic-tool.tmp"), b"half a pointer").unwrap();
    assert!(l.install(&l.v1, None).is_ok(), "a stray temporary file does not block work");
    assert!(l.recover(None).is_empty());
    assert!(!l.p("journal/txn-x.tmp").exists());
}

// ---- concurrency ------------------------------------------------------------------------------------------------------------

#[test]
fn two_transactions_for_the_same_tool_cannot_run_at_once_and_different_tools_do_not_block_each_other() {
    let l = lab();
    let started = Arc::new(Barrier::new(2));
    let finished = Arc::new(Barrier::new(2));
    let (s1, f1) = (started.clone(), finished.clone());
    let hold = move |p: Point| {
        if p == Point::AfterLock {
            s1.wait(); // the first transaction holds the lock …
            f1.wait(); // … until the second has tried
        }
        None
    };
    let ap = l.approval(&l.v1, Action::Install, None);
    let clk = clock();
    let (root, refs, recipe, bytes) = (&l.root, &l.refs, &l.v1.valid, &l.v1.sc.bytes);
    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            let e = Engine { root, refs, host_platform: this_platform(), clock: &clk, fault: Some(&hold) };
            e.install(&Authority { recipe, approval: &ap }, bytes)
        });
        started.wait();
        let e2 = Engine { root, refs, host_platform: this_platform(), clock: &clk, fault: None };
        let second = e2.install(&Authority { recipe, approval: &ap }, bytes);
        finished.wait(); // release the first transaction before asserting, so a failure cannot leave it waiting
        let first = first.join().unwrap();
        assert!(matches!(second, Err(TxnError::Busy)), "{second:?}");
        assert!(first.is_ok());
    });
    assert_eq!(l.active().as_deref(), Some("1.0.0"));
    l.assert_clean();
    // another tool in the same sandbox runs while this one is held
    let other = fx("synthetic-other", "1.0.0", b"another tool");
    let refs = ReferenceSet {
        authenticated: [l.refs.authenticated.clone(), other.sc.refs.authenticated.clone()].concat(),
        unauthenticated: vec![],
    };
    let c = clock();
    let e = Engine { root: &l.root, refs: &refs, host_platform: this_platform(), clock: &c, fault: None };
    let ap = {
        let a = make_approval(&other, Action::Install, None, COMPUTER, t0());
        verified(&other, &a, Action::Install, None, COMPUTER, t0())
    };
    assert!(e.install(&Authority { recipe: &other.valid, approval: &ap }, &other.sc.bytes).is_ok());
    assert_eq!(e.active_version("synthetic-other").unwrap().as_deref(), Some("1.0.0"));
    assert_eq!(e.active_version(TOOL).unwrap().as_deref(), Some("1.0.0"));
}

#[test]
fn an_unfinished_journal_of_the_same_tool_blocks_a_new_transaction_even_without_a_lock() {
    let l = lab();
    assert!(l.install(&l.v1, Some(&at(Point::AfterSnapshot, Fault::Crash))).is_err());
    fs::remove_file(l.p("locks/synthetic-tool.lock")).unwrap();
    assert!(matches!(l.install(&l.v1, None), Err(TxnError::Pending(_))));
}

// ---- confinement ---------------------------------------------------------------------------------------------------------------

#[test]
fn only_plain_single_path_components_are_ever_joined_and_hostile_names_are_refused() {
    for ok in ["tool", "synthetic-tool", "1.0.0", "a_b-c+d", "x1"] {
        assert!(safe_component(ok), "{ok}");
    }
    for bad in [
        "",
        ".",
        "..",
        "a/b",
        "a\\b",
        "C:",
        "C:\\x",
        "/etc",
        "~",
        "a b",
        "a..b",
        "name.",
        "NUL",
        "con",
        "Aux.txt",
        "COM1",
        "lpt9.log",
        &"a".repeat(65),
        "é",
        "a\u{0}b",
        "a\nb",
        "$HOME",
        "-rf",
        ".hidden",
        "tool;rm",
    ] {
        assert!(!safe_component(bad), "{bad:?}");
    }
    let l = lab();
    // an engine path can only be built from those components, below a known top-level folder
    assert!(l.root.join("versions", &["..", "x"]).is_err());
    assert!(l.root.join("versions", &["a/b"]).is_err());
    assert!(l.root.join("elsewhere", &["x"]).is_err());
    assert!(l.root.join("versions", &["ok", "1.0.0"]).unwrap().starts_with(l.root.path()));
}

#[test]
fn a_tool_or_version_name_that_is_unsafe_as_a_path_is_refused_before_anything_is_written() {
    let c = conn();
    let sc = crate::synthetic::Scenario::new(&base(&c), "synthetic-a..b", "1.0.0", this_platform(), b"x");
    // is_id accepts the dots, the path layer does not
    let l = lab();
    let valid = sc.validated();
    let fx = Fx { conn: c, plan: sc.plan(), valid, sc };
    let ap = {
        let a = make_approval(&fx, Action::Install, None, COMPUTER, t0());
        verified(&fx, &a, Action::Install, None, COMPUTER, t0())
    };
    let clk = clock();
    let e = Engine { root: &l.root, refs: &fx.sc.refs, host_platform: this_platform(), clock: &clk, fault: None };
    assert_eq!(refused(e.install(&Authority { recipe: &fx.valid, approval: &ap }, &fx.sc.bytes)), "unsafe_path_component");
    assert!(l.files("journal").is_empty() && l.files("versions").is_empty());
}

#[test]
fn a_sandbox_is_created_fresh_below_a_plain_directory_and_an_existing_folder_is_never_adopted() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    // a file, a missing folder, a bad computer id
    let file = base.join("file");
    fs::write(&file, b"x").unwrap();
    assert!(SandboxRoot::create(&file, COMPUTER).is_err());
    assert!(SandboxRoot::create(&base.join("missing"), COMPUTER).is_err());
    assert!(SandboxRoot::create(&base, "../x").is_err());
    // an ordinary folder that merely looks right is not a sandbox
    let lookalike = base.join(format!("{SANDBOX_PREFIX}abc"));
    fs::create_dir(&lookalike).unwrap();
    assert_eq!(SandboxRoot::open(&lookalike).unwrap_err(), "no_marker");
    assert!(SandboxRoot::open(&base).is_err(), "wrong name");
    fs::write(lookalike.join(MARKER), "{\"magic\":\"something-else\"}").unwrap();
    assert_eq!(SandboxRoot::open(&lookalike).unwrap_err(), "bad_marker");
    // a real one opens again and knows its computer
    let real = SandboxRoot::create(&base, COMPUTER).unwrap();
    let reopened = SandboxRoot::open(real.path()).unwrap();
    assert_eq!(reopened.computer_id(), COMPUTER);
    // a damaged layout is refused
    fs::remove_dir(real.path().join("locks")).unwrap();
    assert_eq!(SandboxRoot::open(real.path()).unwrap_err(), "layout_damaged_or_redirected");
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
fn a_redirected_sandbox_folder_or_a_redirected_sandbox_is_refused_and_the_outside_is_never_written() {
    let l = lab();
    let outside = l.base.join("outside-target");
    fs::create_dir(&outside).unwrap();
    // 1. the versions folder replaced by a link to somewhere else
    fs::remove_dir(l.p("versions")).unwrap();
    if !make_dir_link(&l.p("versions"), &outside) {
        eprintln!("directory links are not available here; case not exercised");
        return;
    }
    let r = l.install(&l.v1, None);
    assert!(
        matches!(&r, Err(TxnError::Failed { rollback, .. }) if rollback.status != "partial") || matches!(&r, Err(TxnError::Refused(_))),
        "{r:?}"
    );
    assert!(fs::read_dir(&outside).unwrap().next().is_none(), "nothing was written through the link");
    assert_eq!(SandboxRoot::open(l.root.path()).unwrap_err(), "layout_damaged_or_redirected");
    remove_dir_link(&l.p("versions"));
    fs::create_dir(l.p("versions")).unwrap();
    // 2. the tool's own folder replaced by a link
    fs::create_dir_all(l.p(&format!("versions/{TOOL}"))).unwrap();
    fs::remove_dir(l.p(&format!("versions/{TOOL}"))).unwrap();
    assert!(make_dir_link(&l.p(&format!("versions/{TOOL}")), &outside));
    let r = l.install(&l.v1, None);
    assert!(r.is_err(), "{r:?}");
    assert!(fs::read_dir(&outside).unwrap().next().is_none(), "nothing was written through the link");
    remove_dir_link(&l.p(&format!("versions/{TOOL}")));
    // 3. the whole sandbox is a link to another place
    let linked = l.base.join(format!("{SANDBOX_PREFIX}linked"));
    if make_dir_link(&linked, l.root.path()) {
        assert!(SandboxRoot::open(&linked).is_err());
        remove_dir_link(&linked);
    }
    l.outside_is_untouched_except(&["outside-target"]);
}

impl Lab {
    fn outside_is_untouched_except(&self, extra: &[&str]) {
        let names: Vec<String> =
            fs::read_dir(&self.base).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        for n in &names {
            assert!(n == "canary.txt" || n.starts_with(SANDBOX_PREFIX) || extra.contains(&n.as_str()), "unexpected entry {n}");
        }
        assert_eq!(fs::read(self.base.join("canary.txt")).unwrap(), b"untouched");
    }
}

#[test]
fn nothing_outside_the_sandbox_is_created_by_a_full_lifecycle_and_nothing_inside_names_a_path_or_account() {
    let l = lab_in("Ayşe Çelik/deep root with spaces");
    fs::create_dir_all(l.p(&format!("data/{TOOL}"))).unwrap();
    l.install(&l.v1, None).unwrap();
    l.update(&l.v2, "1.0.0", None).unwrap();
    l.uninstall(&l.v2, None).unwrap();
    l.install(&l.v1, None).unwrap();
    l.outside_is_untouched();
    // the managed files carry no path, account name or sandbox location
    let base_text = l.base.to_string_lossy().into_owned();
    let mut checked = 0;
    for dir in ["journal", "receipts", "state"] {
        for e in fs::read_dir(l.p(dir)).unwrap().flatten() {
            let text = fs::read_to_string(e.path()).unwrap();
            assert!(!text.contains(&base_text) && !text.contains("Ayşe") && !text.contains("deep root"), "{dir}: {text}");
            assert!(!text.contains(":\\") && !text.contains("/Users/") && !text.contains("/tmp"), "{dir}");
            checked += 1;
        }
    }
    assert!(checked >= 5);
}

// ---- different roots, account names, permissions, restart -------------------------------------------------------------------

#[test]
fn the_lifecycle_works_below_other_account_names_and_install_roots() {
    for sub in ["alice", "Ayşe Çelik", "bob the builder 2", "a/b/c/d/e/f", "ünïcödé/küçük harf"] {
        let l = lab_in(sub);
        l.install(&l.v1, None).unwrap();
        l.update(&l.v2, "1.0.0", None).unwrap();
        assert_eq!(l.active().as_deref(), Some("2.0.0"), "{sub}");
        l.uninstall(&l.v2, None).unwrap();
        assert_eq!(l.active(), None, "{sub}");
        l.outside_is_untouched();
    }
}

#[test]
fn a_restart_finds_the_same_state_and_a_tampered_receipt_is_not_trusted_for_idempotency() {
    let l = lab();
    let out = l.install(&l.v1, None).unwrap();
    let reopened = SandboxRoot::open(l.root.path()).unwrap();
    let c = clock();
    let e = Engine { root: &reopened, refs: &l.refs, host_platform: this_platform(), clock: &c, fault: None };
    assert_eq!(e.active_version(TOOL).unwrap().as_deref(), Some("1.0.0"));
    assert_eq!(e.read_receipt(&out.receipt.txn_id), Some(out.receipt.clone()));
    // edit the receipt: its digest no longer holds, so the installed state has no valid receipt
    let rpath = l.p(&format!("receipts/{}.json", out.receipt.txn_id));
    let text = fs::read_to_string(&rpath).unwrap().replace("1.0.0", "9.9.9");
    fs::write(&rpath, text).unwrap();
    assert_eq!(e.read_receipt(&out.receipt.txn_id), None);
    assert_eq!(refused(l.install(&l.v1, None)), "installed_state_has_no_valid_receipt");
}

#[test]
fn a_blocked_write_is_a_failed_transaction_with_an_honest_rollback_not_a_crash() {
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let pointer = l.p("state/synthetic-tool.json");
    let before = fs::read(&pointer).unwrap();
    #[cfg(windows)]
    {
        // a read-only pointer file cannot be replaced by a rename on Windows
        let mut perm = fs::metadata(&pointer).unwrap().permissions();
        perm.set_readonly(true);
        fs::set_permissions(&pointer, perm).unwrap();
        let (_, report) = failed(l.update(&l.v2, "1.0.0", None));
        let mut perm = fs::metadata(&pointer).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        fs::set_permissions(&pointer, perm).unwrap();
        assert!(matches!(report.status.as_str(), "restored" | "partial" | "failed"), "{report:?}");
        // whatever the report says is true on disk: the old version is active and its content intact
        assert_eq!(l.active().as_deref(), Some("1.0.0"));
        assert_eq!(fs::read(&pointer).unwrap(), before);
        assert_eq!(l.artifact_of("1.0.0").unwrap(), l.v1.sc.bytes);
        let _ = l.recover(None);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("running as root: permission denial cannot be exercised");
            return;
        }
        let state = l.p("state");
        fs::set_permissions(&state, fs::Permissions::from_mode(0o500)).unwrap();
        let r = l.update(&l.v2, "1.0.0", None);
        fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
        let (_, report) = failed(r);
        assert!(matches!(report.status.as_str(), "restored" | "partial" | "failed"), "{report:?}");
        assert_eq!(l.active().as_deref(), Some("1.0.0"));
        assert_eq!(fs::read(&pointer).unwrap(), before);
        let _ = l.recover(None);
    }
    // the world can be completed afterwards
    l.assert_clean();
    assert_eq!(l.update(&l.v2, "1.0.0", None).unwrap().receipt.to_version.as_deref(), Some("2.0.0"));
}

#[test]
fn an_inaccessible_layout_is_a_refusal_not_a_panic() {
    let l = lab();
    // a FILE where the staging folder should be
    fs::remove_dir(l.p("staging")).unwrap();
    fs::write(l.p("staging"), b"not a folder").unwrap();
    let r = l.install(&l.v1, None);
    assert!(matches!(r, Err(TxnError::Failed { .. }) | Err(TxnError::Refused(_))), "{r:?}");
    assert_eq!(l.active(), None);
    assert!(l.artifact_of("1.0.0").is_none());
}

// ---- no real external effects ----------------------------------------------------------------------------------------------

#[test]
fn the_synthetic_engine_has_no_way_to_start_programs_use_the_network_change_the_environment_or_touch_agent_files() {
    // each needle is assembled so that this file does not need to contain a flagged name verbatim
    let needles: Vec<String> = [
        ("std::pro", "cess"),
        ("Command", "::new"),
        ("std::", "net"),
        ("Tcp", "Stream"),
        ("req", "west"),
        ("env::", "set_var"),
        ("set_current", "_dir"),
        (".cla", "ude"),
        ("CLA", "UDE.md"),
        ("GEM", "INI.md"),
        ("tauri_plugin_", "shell"),
        ("tauri_plugin_", "http"),
        ("home_", "dir"),
        ("USER", "PROFILE"),
        ("\"PA", "TH\""),
    ]
    .iter()
    .map(|(a, b)| format!("{a}{b}"))
    .collect();
    for (name, source) in [
        ("lifecycle.rs", include_str!("lifecycle.rs")),
        ("approval.rs", include_str!("approval.rs")),
        ("recipe.rs", include_str!("recipe.rs")),
    ] {
        // the test module is declared at the end of each file; only the production part is searched
        let production = source.split("#[cfg(test)]").next().unwrap();
        for n in &needles {
            assert!(!production.contains(n.as_str()), "{name} contains {n}");
        }
    }
}

#[test]
fn nothing_connects_the_synthetic_engine_to_a_command_a_permission_or_the_user_interface() {
    let commands = include_str!("commands.rs");
    let lib = include_str!("lib.rs");
    let build = include_str!("../build.rs");
    let capabilities = include_str!("../capabilities/default.json");
    for (name, text) in [("commands.rs", commands), ("lib.rs", lib), ("build.rs", build), ("capabilities", capabilities)] {
        for needle in [
            "lifecycle::",
            "Engine {",
            "SandboxRoot",
            "install_tool",
            "run_install",
            "approve_",
            "start_install",
            "uninstall",
            "apply_plan",
        ] {
            let mentions = text
                .lines()
                .filter(|l| l.contains(needle) && !l.trim_start().starts_with("//") && !l.trim_start().starts_with("mod "))
                .count();
            assert_eq!(mentions, 0, "{name} mentions {needle}");
        }
    }
    // the only invoke handlers are the ones listed in build.rs, and none of them installs
    for c in crate::health::ALLOWED_PERMISSIONS {
        assert!(!c.contains("install-tool") && !c.contains("approve") && !c.contains("uninstall") && !c.contains("update-tool"), "{c}");
    }
}

#[test]
fn the_process_environment_and_working_directory_are_unchanged_by_a_full_lifecycle() {
    let cwd = std::env::current_dir().unwrap();
    let path = std::env::var_os("PATH");
    let vars: Vec<(std::ffi::OsString, std::ffi::OsString)> = {
        let mut v: Vec<_> = std::env::vars_os().collect();
        v.sort();
        v
    };
    let l = lab();
    l.install(&l.v1, None).unwrap();
    l.update(&l.v2, "1.0.0", None).unwrap();
    l.uninstall(&l.v2, None).unwrap();
    assert_eq!(std::env::current_dir().unwrap(), cwd);
    assert_eq!(std::env::var_os("PATH"), path);
    let mut after: Vec<_> = std::env::vars_os().collect();
    after.sort();
    assert_eq!(after, vars);
}

// =====================================================================================================================
// Phase 3A closure (Issue #13, RT-3A-02): rollback and cleanup delete only what this module created, in the shape it
// created it
// =====================================================================================================================

/// A failing install whose version folder is tampered with right after the pointer switch (`inject` runs once, there).
fn install_with_obstruction(l: &Lab, inject: impl Fn(&Path) + Sync, then: Fault) -> Result<Outcome, TxnError> {
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

fn only_partial_failure(report: &RollbackReport) {
    assert!(matches!(report.status.as_str(), "partial" | "failed"), "{report:?}");
    assert!(report.not_undone.iter().any(|n| n.what.contains("version folder 1.0.0")), "{report:?}");
    assert!(!report.undone.iter().any(|u| u.contains("version folder")), "it must not claim the folder was removed: {report:?}");
}

#[test]
fn a_clean_rollback_removes_the_artifact_and_the_folder_and_nothing_else() {
    let l = lab();
    let (_, report) = failed(install_with_obstruction(&l, |_| {}, Fault::Fail));
    assert_eq!(report.status, "restored", "{report:?}");
    assert!(report.undone.iter().any(|u| u.contains("version folder 1.0.0 removed")));
    assert!(!l.p(&format!("versions/{TOOL}")).exists(), "the empty tool folder goes too");
    l.assert_clean();
    l.outside_is_untouched();
    // an empty folder that this transaction created is handled as well
    let l = lab();
    let (_, report) = failed(l.install(&l.v1, Some(&at(Point::AfterVersionDirCreate, Fault::Fail))));
    assert_eq!(report.status, "restored", "{report:?}");
    assert!(!l.p(&format!("versions/{TOOL}")).exists());
}

#[test]
fn an_unrelated_file_next_to_a_matching_artifact_survives_rollback_and_nothing_is_deleted() {
    let l = lab();
    let (_, report) = failed(install_with_obstruction(&l, |d| fs::write(d.join("notes.txt"), b"unrelated, mine").unwrap(), Fault::Fail));
    only_partial_failure(&report);
    assert!(report.not_undone.iter().any(|n| n.why.contains("1 unexpected entry") && n.why.contains("nothing was deleted")), "{report:?}");
    let d = l.p(&format!("versions/{TOOL}/1.0.0"));
    assert_eq!(fs::read(d.join("notes.txt")).unwrap(), b"unrelated, mine");
    assert_eq!(fs::read(d.join("artifact.bin")).unwrap(), l.v1.sc.bytes, "all or nothing: the matching artifact stays too");
    // the pointer, which could be restored, was
    assert_eq!(l.active(), None);
    assert!(report.undone.iter().any(|u| u.contains("pointer")));
    // the journal and the lock are kept for recovery, and new work is refused
    let j = l.journals().remove(0);
    assert_eq!(j.state, TxnState::RollbackFailed);
    assert_eq!(l.files("locks").len(), 1);
    assert!(matches!(l.install(&l.v1, None), Err(TxnError::Busy)));
    l.outside_is_untouched();
}

#[test]
fn an_unexpected_subdirectory_is_never_deleted_recursively() {
    let l = lab();
    let (_, report) = failed(install_with_obstruction(
        &l,
        |d| {
            fs::create_dir_all(d.join("cache/deep")).unwrap();
            fs::write(d.join("cache/deep/precious.db"), b"precious").unwrap();
        },
        Fault::Fail,
    ));
    only_partial_failure(&report);
    assert!(report.not_undone.iter().any(|n| n.why.contains("1 folder(s)")), "{report:?}");
    assert_eq!(fs::read(l.p(&format!("versions/{TOOL}/1.0.0/cache/deep/precious.db"))).unwrap(), b"precious");
    assert!(l.artifact_of("1.0.0").is_some());
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

#[test]
fn a_symbolic_link_inside_the_version_folder_stops_the_cleanup_and_its_target_is_untouched() {
    let l = lab();
    let outside = l.base.join("outside-file.txt");
    fs::write(&outside, b"outside target").unwrap();
    let link_ok = std::sync::atomic::AtomicBool::new(true);
    let out2 = outside.clone();
    let (_, report) = failed(install_with_obstruction(
        &l,
        |d| {
            if !make_file_link(&d.join("shortcut"), &out2) {
                link_ok.store(false, Ordering::SeqCst);
            }
        },
        Fault::Fail,
    ));
    if !link_ok.load(Ordering::SeqCst) {
        eprintln!("file symlinks need a privilege that is not available here; case not exercised");
        return;
    }
    only_partial_failure(&report);
    assert!(report.not_undone.iter().any(|n| n.why.contains("1 link(s)")), "{report:?}");
    assert_eq!(fs::read(&outside).unwrap(), b"outside target", "the link target is untouched");
    assert!(fs::symlink_metadata(l.p(&format!("versions/{TOOL}/1.0.0/shortcut"))).is_ok(), "the link itself is left in place");
    assert!(l.artifact_of("1.0.0").is_some());
}

#[test]
fn a_junction_or_directory_link_inside_the_version_folder_stops_the_cleanup_and_the_outside_stays_byte_identical() {
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
                made.store(false, Ordering::SeqCst);
            }
        },
        Fault::Fail,
    ));
    if !made.load(Ordering::SeqCst) {
        eprintln!("directory links are not available here; case not exercised");
        return;
    }
    only_partial_failure(&report);
    assert!(report.not_undone.iter().any(|n| n.why.contains("1 link(s)")), "{report:?}");
    assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"outside content");
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1, "nothing was added or removed outside");
    remove_dir_link(&l.p(&format!("versions/{TOOL}/1.0.0/redirect")));
}

#[test]
fn a_version_folder_that_is_itself_a_link_is_refused_and_never_followed() {
    let l = lab();
    let outside = l.base.join("outside-dir");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), b"outside content").unwrap();
    let made = std::sync::atomic::AtomicBool::new(true);
    let o2 = outside.clone();
    let vdir = l.p(&format!("versions/{TOOL}/1.0.0"));
    let hook = {
        let vdir = vdir.clone();
        move |p: Point| {
            if p == Point::AfterPointerSwitch {
                // the folder the engine made is swapped for a link to a foreign folder
                fs::remove_dir_all(&vdir).unwrap();
                if !make_dir_link(&vdir, &o2) {
                    made.store(false, Ordering::SeqCst);
                }
                return Some(Fault::Fail);
            }
            None
        }
    };
    let (_, report) = failed(l.install(&l.v1, Some(&hook)));
    only_partial_failure(&report);
    assert!(report.not_undone.iter().any(|n| n.why.contains("left untouched")), "{report:?}");
    assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"outside content");
    remove_dir_link(&vdir);
}

#[test]
fn a_changed_artifact_is_never_removed_on_the_assumption_that_it_is_ours() {
    let l = lab();
    let (_, report) =
        failed(install_with_obstruction(&l, |d| fs::write(d.join("artifact.bin"), b"someone else's content").unwrap(), Fault::Fail));
    only_partial_failure(&report);
    assert_eq!(fs::read(l.p(&format!("versions/{TOOL}/1.0.0/artifact.bin"))).unwrap(), b"someone else's content");
}

#[test]
fn a_crash_followed_by_recovery_with_unexpected_content_keeps_it_and_repeating_recovery_changes_nothing() {
    let l = lab();
    let outside = l.base.join("sentinel.bin");
    fs::write(&outside, [7u8; 64]).unwrap();
    let r = install_with_obstruction(&l, |d| fs::write(d.join("notes.txt"), b"unrelated, mine").unwrap(), Fault::Crash);
    assert!(matches!(r, Err(TxnError::Interrupted(Point::AfterPointerSwitch))), "{r:?}");
    let notes = l.p(&format!("versions/{TOOL}/1.0.0/notes.txt"));
    // first recovery: the pointer is restored, the folder is NOT removed, the transaction needs attention
    let first = l.recover(None);
    match &first[..] {
        [Recovered::NeedsAttention(_, report)] => only_partial_failure(report),
        other => panic!("{other:?}"),
    }
    assert_eq!(l.active(), None, "what could be undone was undone");
    assert_eq!(l.files("locks").len(), 1);
    assert_eq!(l.journals().remove(0).state, TxnState::RollbackFailed);
    // repeating recovery any number of times: same answer, same files, no growth
    let snapshot = |l: &Lab| (l.files("journal"), l.files("receipts"), fs::read(&notes).unwrap(), l.artifact_of("1.0.0"));
    // (an empty staging/snapshot folder may be removed by a later recovery; what matters is that nothing foreign or recorded changes)
    let before = snapshot(&l);
    for _ in 0..3 {
        assert!(matches!(&l.recover(None)[..], [Recovered::NeedsAttention(..)]));
        assert_eq!(snapshot(&l), before);
    }
    assert_eq!(fs::read(&notes).unwrap(), b"unrelated, mine");
    assert_eq!(fs::read(&outside).unwrap(), [7u8; 64], "the sentinel outside the sandbox is byte-identical");
    // the obstruction is resolved by its owner; the next start finishes the rollback
    fs::remove_file(&notes).unwrap();
    assert!(matches!(&l.recover(None)[..], [Recovered::RolledBack(_)]));
    assert!(!l.p(&format!("versions/{TOOL}")).exists());
    assert_eq!(l.active(), None);
    l.assert_clean();
    l.outside_is_untouched_except(&["sentinel.bin"]);
    // and work resumes
    assert!(l.install(&l.v1, None).is_ok());
}

#[test]
fn staging_and_snapshot_cleanup_keep_anything_foreign_and_report_it_on_recovery() {
    // a foreign file inside the transaction's staging folder
    let l = lab();
    let stage_root = l.p("staging");
    let hook = move |p: Point| {
        if p == Point::AfterVerify {
            let dir = fs::read_dir(&stage_root).unwrap().flatten().next().unwrap().path();
            fs::write(dir.join("extra.txt"), b"mine").unwrap();
            return Some(Fault::Fail);
        }
        None
    };
    let (_, report) = failed(l.install(&l.v1, Some(&hook)));
    assert!(report.not_undone.iter().any(|n| n.what == "staging folder" && n.why.contains("unexpected")), "{report:?}");
    let staged = l.p(&format!("staging/{}", l.files("staging")[0]));
    assert_eq!(fs::read(staged.join("extra.txt")).unwrap(), b"mine");
    assert!(staged.join("artifact.bin").exists(), "all or nothing: the staged artifact stays as well");
    assert_eq!(l.journals().remove(0).state, TxnState::RollbackFailed);
    fs::remove_file(staged.join("extra.txt")).unwrap();
    assert!(matches!(&l.recover(None)[..], [Recovered::RolledBack(_)]));
    l.assert_clean();

    // a foreign file inside the snapshot of a committed uninstall
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let snaps = l.p("snapshots");
    let hook = move |p: Point| {
        if p == Point::BeforeJournalCommit {
            let dir = fs::read_dir(&snaps).unwrap().flatten().next().unwrap().path();
            fs::write(dir.join("versions/1.0.0/extra.txt"), b"mine").unwrap();
        }
        None
    };
    let out = l.uninstall(&l.v1, Some(&hook)).unwrap();
    assert!(!out.already_applied);
    assert_eq!(l.active(), None, "the uninstall itself is complete");
    let snap_dir = l.p(&format!("snapshots/{}", l.files("snapshots")[0]));
    assert_eq!(fs::read(snap_dir.join("versions/1.0.0/extra.txt")).unwrap(), b"mine", "the foreign file survives the cleanup");
    for _ in 0..2 {
        let rec = l.recover(None);
        assert!(
            matches!(&rec[..], [Recovered::LeftoversRetained(_, why)] if why.contains("snapshot") && why.contains("unexpected")),
            "{rec:?}"
        );
    }
    fs::remove_file(snap_dir.join("versions/1.0.0/extra.txt")).unwrap();
    assert!(l.recover(None).is_empty(), "nothing is retained any more");
    l.assert_clean();
}

/// Creates `dir/artifact.bin` the way the engine does (exclusive create) and returns the manifest entry the journal would hold.
fn owned_artifact(base: &Path, dir: &Path, content: &[u8]) -> Vec<OwnedFile> {
    let mut f = fs::OpenOptions::new().write(true).create_new(true).open(dir.join("artifact.bin")).unwrap();
    std::io::Write::write_all(&mut f, content).unwrap();
    let (identity, _) = identity_and_links(&f).unwrap();
    vec![OwnedFile {
        role: "t".into(),
        phase: Phase::Final,
        rels: vec![rel_of(base, &dir.join("artifact.bin")).unwrap()],
        sha256: Some(sha256_hex_bytes(content)),
        size: Some(content.len() as u64),
        identity: Some(identity),
    }]
}

#[test]
fn verified_directory_removal_never_recurses_and_never_follows_links() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let dir = base.join("d");
    fs::create_dir(&dir).unwrap();
    let owned = owned_artifact(&base, &dir, b"a");
    // the recorded file only: removed, folder gone
    assert!(remove_verified_dir(&base, &dir, &owned).is_ok() && !dir.exists());
    // a file that is not recorded, a sub-folder, a name that merely looks recorded (case), all keep everything
    for (name, is_dir) in [("other", false), ("sub", true), ("ARTIFACT.BIN.x", false)] {
        fs::create_dir(&dir).unwrap();
        let owned = owned_artifact(&base, &dir, b"a");
        if is_dir {
            fs::create_dir(dir.join(name)).unwrap();
        } else {
            fs::write(dir.join(name), b"x").unwrap();
        }
        assert!(remove_verified_dir(&base, &dir, &owned).is_err(), "{name}");
        assert!(dir.join("artifact.bin").exists() && dir.join(name).exists(), "{name}: nothing deleted");
        fs::remove_dir_all(&dir).unwrap();
    }
    // the recorded name present as a FOLDER is not the recorded file
    fs::create_dir_all(dir.join("artifact.bin")).unwrap();
    let owned = vec![OwnedFile {
        role: "t".into(),
        phase: Phase::Final,
        rels: vec!["d/artifact.bin".into()],
        sha256: None,
        size: None,
        identity: None,
    }];
    assert!(remove_verified_dir(&base, &dir, &owned).is_err() && dir.join("artifact.bin").is_dir());
}

// ---- the sandbox boundary ------------------------------------------------------------------------------------------------------

#[test]
fn a_populated_user_folder_with_a_copied_marker_and_the_right_folder_names_is_not_adopted() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let real = SandboxRoot::create(&base, COMPUTER).unwrap();
    let real_marker = fs::read_to_string(real.path().join(MARKER)).unwrap();
    // 1. the marker of a real sandbox copied into another folder that carries the sandbox name pattern
    let copy = base.join(format!("{SANDBOX_PREFIX}00000000000000000000000000000000"));
    fs::create_dir(&copy).unwrap();
    fs::write(copy.join(MARKER), &real_marker).unwrap();
    for d in SUBDIRS {
        fs::create_dir(copy.join(d)).unwrap();
    }
    assert_eq!(SandboxRoot::open(&copy).unwrap_err(), "marker_does_not_belong_to_this_folder");
    // 2. a user folder: marker and layout forged correctly, but it already holds the user's own files
    let user = base.join(format!("{SANDBOX_PREFIX}1234567890abcdef1234567890abcdef"));
    fs::create_dir(&user).unwrap();
    fs::write(
        user.join(MARKER),
        serde_json::json!({ "magic": MARKER_MAGIC, "computerId": COMPUTER, "rootId": "1234567890abcdef1234567890abcdef" }).to_string(),
    )
    .unwrap();
    for d in SUBDIRS {
        fs::create_dir(user.join(d)).unwrap();
    }
    assert!(SandboxRoot::open(&user).is_ok(), "control: an exactly shaped, empty folder is what a sandbox looks like");
    fs::write(user.join("my-thesis.docx"), b"precious").unwrap();
    assert_eq!(SandboxRoot::open(&user).unwrap_err(), "unexpected_top_level_entry");
    fs::remove_file(user.join("my-thesis.docx")).unwrap();
    fs::remove_dir(user.join("locks")).unwrap();
    assert_eq!(SandboxRoot::open(&user).unwrap_err(), "layout_damaged_or_redirected");
    // 3. create() makes a new folder and never adopts: it is not affected by what already exists
    let second = SandboxRoot::create(&base, COMPUTER).unwrap();
    assert_ne!(second.path(), real.path());
}

#[test]
fn no_recursive_delete_exists_anywhere_in_the_production_engine() {
    let production = include_str!("lifecycle.rs").split("#[cfg(test)]").next().unwrap();
    let needle = format!("{}{}", "remove_dir", "_all");
    assert!(!production.contains(&needle), "the engine must remove named files and then empty folders, never a tree");
}

// =====================================================================================================================
// Phase 3B (Issue #14): the engine deletes a file only if it is the very file this transaction created -- same identity,
// one link, same content -- never merely because a file of an expected name is there
// =====================================================================================================================

/// What an unrelated program (or an attacker running as the same user) can do to a name: replace the file by another one.
fn replace_file(path: &Path, content: &[u8]) {
    fs::remove_file(path).unwrap();
    fs::write(path, content).unwrap();
}

fn assert_not_ours(report: &RollbackReport, what: &str) {
    assert!(
        report.not_undone.iter().any(|n| n.what.contains(what) && n.why.contains("no longer the file this transaction wrote")),
        "{report:?}"
    );
}

#[test]
fn the_journal_records_the_identity_and_digest_of_what_the_transaction_created() {
    let l = lab();
    let hook = |p: Point| (p == Point::BeforeJournalCommit).then_some(Fault::Crash);
    assert!(matches!(l.install(&l.v1, Some(&hook)), Err(TxnError::Interrupted(Point::BeforeJournalCommit))));
    let j = l.journals().remove(0);
    let staged = j.owned.iter().find(|e| e.role == "staged_artifact").expect("manifest entry");
    assert!(staged.identity.is_some() && staged.size == Some(l.v1.sc.bytes.len() as u64));
    assert_eq!(staged.sha256.as_deref(), Some(sha256_hex_bytes(&l.v1.sc.bytes).as_str()));
    assert!(staged.rels.iter().any(|r| r == &format!("versions/{TOOL}/1.0.0/artifact.bin")));
    // the manifest holds relative places only: no absolute path, no account name
    let text = serde_json::to_string(&j.owned).unwrap();
    assert!(!text.contains(l.base.to_string_lossy().as_ref()), "{text}");
}

#[test]
fn a_replacement_with_identical_content_is_a_different_file_and_is_not_deleted() {
    let l = lab();
    let bytes = l.v1.sc.bytes.clone();
    let (_, report) = failed(install_with_obstruction(&l, |d| replace_file(&d.join("artifact.bin"), &bytes), Fault::Fail));
    only_partial_failure(&report);
    assert_not_ours(&report, "version folder 1.0.0");
    assert_eq!(l.artifact_of("1.0.0").unwrap(), bytes, "left exactly where it is");
    assert_eq!(l.journals().remove(0).state, TxnState::RollbackFailed);
    l.outside_is_untouched();
}

#[test]
fn a_hard_linked_alias_of_the_artifact_is_never_deleted_through() {
    let l = lab();
    let alias = l.base.join("alias.bin");
    let a2 = alias.clone();
    let linked = std::sync::atomic::AtomicBool::new(true);
    let (_, report) = failed(install_with_obstruction(
        &l,
        |d| {
            if fs::hard_link(d.join("artifact.bin"), &a2).is_err() {
                linked.store(false, Ordering::SeqCst);
            }
        },
        Fault::Fail,
    ));
    if !linked.load(Ordering::SeqCst) {
        eprintln!("hard links are not available here; case not exercised");
        return;
    }
    only_partial_failure(&report);
    assert_not_ours(&report, "version folder 1.0.0");
    assert_eq!(fs::read(&alias).unwrap(), l.v1.sc.bytes);
    assert!(l.artifact_of("1.0.0").is_some());
}

#[test]
fn a_file_substituted_under_the_expected_name_in_staging_is_refused_and_survives() {
    let l = lab();
    let stage_root = l.p("staging");
    let hook = move |p: Point| {
        if p == Point::AfterVerify {
            let dir = fs::read_dir(&stage_root).unwrap().flatten().next().unwrap().path();
            replace_file(&dir.join("artifact.bin"), b"alien content under an expected name");
        }
        None
    };
    let (code, report) = failed(l.install(&l.v1, Some(&hook)));
    assert_eq!(code, "staged_file_is_not_the_file_this_transaction_wrote");
    assert_not_ours(&report, "staging folder");
    let staged = l.p(&format!("staging/{}", l.files("staging")[0]));
    assert_eq!(fs::read(staged.join("artifact.bin")).unwrap(), b"alien content under an expected name");
    assert!(l.artifact_of("1.0.0").is_none(), "the alien file never became an installed version");
    assert_eq!(l.active(), None);
    // the owner removes it; the next recovery completes the cleanup
    fs::remove_file(staged.join("artifact.bin")).unwrap();
    assert!(matches!(&l.recover(None)[..], [Recovered::RolledBack(_)]));
    l.assert_clean();
    l.outside_is_untouched();
}

#[test]
fn a_swap_between_the_check_and_the_activation_is_caught_by_the_second_check() {
    let l = lab();
    let stage_root = l.p("staging");
    let hook = move |p: Point| {
        if p == Point::AfterVersionDirCreate {
            let dir = fs::read_dir(&stage_root).unwrap().flatten().next().unwrap().path();
            replace_file(&dir.join("artifact.bin"), b"swapped in after the first check");
        }
        None
    };
    let (code, report) = failed(l.install(&l.v1, Some(&hook)));
    assert_eq!(code, "staged_file_is_not_the_file_this_transaction_wrote");
    assert_not_ours(&report, "staging folder");
    assert!(l.artifact_of("1.0.0").is_none(), "nothing was activated");
    assert_eq!(l.active(), None);
}

#[test]
fn a_crashed_install_leaves_a_partial_file_that_recovery_deletes_only_while_it_is_still_the_same_file() {
    // owned partial file: recovery removes it and the staging folder
    let l = lab();
    let r = l.install(&l.v1, Some(&at(Point::DuringStageWrite, Fault::Crash)));
    assert!(matches!(r, Err(TxnError::Interrupted(Point::DuringStageWrite))), "{r:?}");
    let stage = l.p(&format!("staging/{}", l.files("staging")[0]));
    assert!(stage.join("artifact.bin.part").exists());
    assert!(matches!(&l.recover(None)[..], [Recovered::RolledBack(_)]));
    l.assert_clean();

    // substituted partial file: kept, reported on every recovery, nothing else touched
    let l = lab();
    let sentinel = l.base.join("sentinel.bin");
    fs::write(&sentinel, [9u8; 32]).unwrap();
    let r = l.install(&l.v1, Some(&at(Point::DuringStageWrite, Fault::Crash)));
    assert!(matches!(r, Err(TxnError::Interrupted(Point::DuringStageWrite))), "{r:?}");
    let stage = l.p(&format!("staging/{}", l.files("staging")[0]));
    replace_file(&stage.join("artifact.bin.part"), b"alien");
    for _ in 0..3 {
        match &l.recover(None)[..] {
            [Recovered::NeedsAttention(_, report)] => assert_not_ours(report, "staging folder"),
            other => panic!("{other:?}"),
        }
        assert_eq!(fs::read(stage.join("artifact.bin.part")).unwrap(), b"alien");
    }
    assert_eq!(fs::read(&sentinel).unwrap(), [9u8; 32]);
    fs::remove_file(stage.join("artifact.bin.part")).unwrap();
    assert!(matches!(&l.recover(None)[..], [Recovered::RolledBack(_)]));
    l.assert_clean();
    l.outside_is_untouched_except(&["sentinel.bin"]);
}

#[test]
fn a_staging_folder_replaced_by_a_link_is_refused_and_the_outside_stays_byte_identical() {
    let l = lab();
    let outside = l.base.join("outside-dir");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), b"outside content").unwrap();
    let made = std::sync::atomic::AtomicBool::new(true);
    let o2 = outside.clone();
    let stage_root = l.p("staging");
    let hook = |p: Point| {
        if p == Point::AfterVerify {
            let dir = fs::read_dir(&stage_root).unwrap().flatten().next().unwrap().path();
            fs::remove_dir_all(&dir).unwrap();
            if !make_dir_link(&dir, &o2) {
                made.store(false, Ordering::SeqCst);
            }
            return Some(Fault::Fail);
        }
        None
    };
    let (_, report) = failed(l.install(&l.v1, Some(&hook)));
    assert!(report.not_undone.iter().any(|n| n.what == "staging folder") || !made.load(Ordering::SeqCst), "{report:?}");
    assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"outside content");
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
    let names = l.files("staging");
    if let Some(n) = names.first() {
        let link = l.p(&format!("staging/{n}"));
        if fs::symlink_metadata(&link).is_ok() {
            remove_dir_link(&link);
        }
    }
}

#[test]
fn a_substituted_file_in_the_snapshot_of_a_committed_update_or_uninstall_is_kept() {
    // update: the saved pointer is replaced before the cleanup
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let snaps = l.p("snapshots");
    let hook = move |p: Point| {
        if p == Point::BeforeJournalCommit {
            let dir = fs::read_dir(&snaps).unwrap().flatten().next().unwrap().path();
            replace_file(&dir.join("pointer.json"), b"not the pointer this transaction saved");
        }
        None
    };
    l.update(&l.v2, "1.0.0", Some(&hook)).unwrap();
    let snap_dir = l.p(&format!("snapshots/{}", l.files("snapshots")[0]));
    assert_eq!(fs::read(snap_dir.join("pointer.json")).unwrap(), b"not the pointer this transaction saved");
    for _ in 0..2 {
        let rec = l.recover(None);
        assert!(
            matches!(&rec[..], [Recovered::LeftoversRetained(_, why)] if why.contains("snapshot") && why.contains("no longer the file")),
            "{rec:?}"
        );
    }
    assert_eq!(l.active().as_deref(), Some("2.0.0"));

    // uninstall: a version artifact in the snapshot is replaced; the pointer beside it stays as well (all or nothing)
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
    let rec = l.recover(None);
    assert!(matches!(&rec[..], [Recovered::LeftoversRetained(_, why)] if why.contains("snapshot")), "{rec:?}");
    l.outside_is_untouched();
}

#[test]
fn a_saved_pointer_that_is_not_the_one_the_transaction_saved_is_never_restored_as_the_active_pointer() {
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
    assert!(
        report.not_undone.iter().any(|n| n.what == "active-version pointer" && n.why.contains("not the file this transaction wrote")),
        "{report:?}"
    );
    assert_eq!(l.active().as_deref(), Some("2.0.0"), "the forged pointer was not made active");
    assert!(l.artifact_of("2.0.0").is_some(), "the version the pointer still names was not removed");
    assert!(report.not_undone.iter().any(|n| n.what.contains("version folder 2.0.0")), "{report:?}");
    assert!(!fs::read_to_string(l.p(&format!("state/{TOOL}.json"))).unwrap().contains("forged"));
}

#[test]
fn an_uninstall_is_refused_before_anything_moves_when_the_installed_content_is_not_what_a_receipt_vouches_for() {
    let setup = || {
        let l = lab();
        l.install(&l.v1, None).unwrap();
        l.update(&l.v2, "1.0.0", None).unwrap();
        l
    };
    let untouched = |l: &Lab| {
        assert_eq!(l.active().as_deref(), Some("2.0.0"));
        assert!(l.artifact_of("1.0.0").is_some() && l.artifact_of("2.0.0").is_some());
        assert!(l.files("snapshots").is_empty() && l.files("locks").is_empty(), "nothing was started");
    };
    // an older version's artifact was changed
    let l = setup();
    fs::write(l.p(&format!("versions/{TOOL}/1.0.0/artifact.bin")), b"tampered").unwrap();
    assert_eq!(refused(l.uninstall(&l.v2, None)), "installed_content_has_no_matching_receipt");
    untouched(&l);
    // a foreign file next to an artifact
    let l = setup();
    fs::write(l.p(&format!("versions/{TOOL}/2.0.0/notes.txt")), b"mine").unwrap();
    assert_eq!(refused(l.uninstall(&l.v2, None)), "unrecognised_installed_content");
    untouched(&l);
    // a version folder nobody installed
    let l = setup();
    fs::create_dir(l.p(&format!("versions/{TOOL}/9.9.9"))).unwrap();
    fs::write(l.p(&format!("versions/{TOOL}/9.9.9/artifact.bin")), b"handmade").unwrap();
    assert_eq!(refused(l.uninstall(&l.v2, None)), "installed_content_has_no_matching_receipt");
    // a hard-linked artifact
    let l = setup();
    let alias = l.base.join("alias.bin");
    if fs::hard_link(l.p(&format!("versions/{TOOL}/2.0.0/artifact.bin")), &alias).is_ok() {
        assert_eq!(refused(l.uninstall(&l.v2, None)), "installed_content_has_no_matching_receipt");
        assert_eq!(fs::read(&alias).unwrap(), l.v2.sc.bytes);
    }
}

#[test]
fn an_in_place_edit_that_keeps_the_size_and_the_identity_is_still_detected_by_the_digest() {
    let l = lab();
    let len = l.v1.sc.bytes.len();
    let (_, report) = failed(install_with_obstruction(
        &l,
        |d| {
            // same file, same length, other bytes
            let mut f = fs::OpenOptions::new().write(true).open(d.join("artifact.bin")).unwrap();
            std::io::Write::write_all(&mut f, &vec![b'X'; len]).unwrap();
        },
        Fault::Fail,
    ));
    only_partial_failure(&report);
    assert_not_ours(&report, "version folder 1.0.0");
    assert_eq!(l.artifact_of("1.0.0").unwrap(), vec![b'X'; len]);
}

// =====================================================================================================================
// Phase 3C (Issue #15): creation of a file the transaction will own is journalled step by step -- intent (durable) ->
// exclusive create -> identity from the handle (durable) -> data -> fsync -> final record (durable) -- and a crash at any
// of those steps ends in a truthful, fail-closed state
// =====================================================================================================================

const SNAPSHOT_POINTS: [Point; 5] = [
    Point::SnapshotBeforeCreate,
    Point::SnapshotAfterCreate,
    Point::SnapshotDuringWrite,
    Point::SnapshotAfterSync,
    Point::SnapshotAfterRecord,
];

fn owned_role(l: &Lab, role: &str) -> OwnedFile {
    l.journals().into_iter().rev().flat_map(|j| j.owned).find(|e| e.role == role).unwrap_or_else(|| panic!("no manifest entry {role}"))
}

fn lab_with_v1() -> (Lab, Vec<u8>) {
    let l = lab();
    l.install(&l.v1, None).unwrap();
    let before = fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap();
    (l, before)
}

fn the_orphan_in_the_snapshot(l: &Lab) -> PathBuf {
    let dir = l.files("snapshots").into_iter().map(|n| l.p(&format!("snapshots/{n}"))).find(|p| p.is_dir()).expect("a snapshot folder");
    dir.join("pointer.json")
}

#[test]
fn every_crash_point_of_the_snapshot_pointer_creation_ends_in_a_truthful_fail_closed_state() {
    for point in SNAPSHOT_POINTS {
        let (l, before) = lab_with_v1();
        let foreign = l.p("snapshots/foreign-note.txt");
        fs::write(&foreign, b"not mine").unwrap();
        let r = l.update(&l.v2, "1.0.0", Some(&at(point, Fault::Crash)));
        assert!(matches!(r, Err(TxnError::Interrupted(p)) if p == point), "{point:?}: {r:?}");
        let entry = owned_role(&l, "snapshot_pointer");
        let expected_phase = match point {
            Point::SnapshotBeforeCreate | Point::SnapshotAfterCreate => Phase::Intended,
            Point::SnapshotDuringWrite | Point::SnapshotAfterSync => Phase::Created,
            _ => Phase::Final,
        };
        assert_eq!(entry.phase, expected_phase, "{point:?}");
        assert_eq!(entry.identity.is_some(), expected_phase != Phase::Intended, "{point:?}: identity is durable before data exists");
        // the pointer the tool is installed with was never touched
        assert_eq!(fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap(), before, "{point:?}");

        if point == Point::SnapshotAfterCreate {
            // the irreducible window: the file exists, its identity was never recorded => not provably ours => kept
            let orphan = the_orphan_in_the_snapshot(&l);
            assert!(orphan.exists());
            for round in 0..3 {
                let rec = l.recover(None);
                match &rec[..] {
                    [Recovered::NeedsAttention(_, report)] => assert!(
                        report.not_undone.iter().any(|n| n.what == "snapshot folder" && n.why.contains("ambiguous orphan")),
                        "round {round}: {report:?}"
                    ),
                    other => panic!("round {round}: {other:?}"),
                }
                assert!(orphan.exists(), "an orphan is never deleted on a guess");
                assert_eq!(l.files("locks").len(), 1, "the tool stays locked");
                assert_eq!(l.active().as_deref(), Some("1.0.0"));
                assert_eq!(fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap(), before);
            }
            assert!(matches!(l.update(&l.v2, "1.0.0", None), Err(TxnError::Busy)));
            // explicit resolution by the owner, then everything resumes
            fs::remove_file(&orphan).unwrap();
            assert!(matches!(&l.recover(None)[..], [Recovered::RolledBack(_)]));
        } else {
            let rec = l.recover(None);
            assert!(matches!(&rec[..], [Recovered::RolledBack(_)]), "{point:?}: {rec:?}");
        }
        assert_eq!(fs::read(&foreign).unwrap(), b"not mine", "{point:?}: a foreign file next to the snapshots is never touched");
        assert!(l.files("staging").is_empty() && l.files("locks").is_empty(), "{point:?}");
        assert_eq!(l.files("snapshots"), vec!["foreign-note.txt".to_string()], "{point:?}");
        assert_eq!(l.active().as_deref(), Some("1.0.0"));
        assert_eq!(fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap(), before);
        // repeating recovery changes nothing, and the next retry works
        assert!(l.recover(None).is_empty(), "{point:?}");
        assert_eq!(l.update(&l.v2, "1.0.0", None).unwrap().receipt.to_version.as_deref(), Some("2.0.0"), "{point:?}");
        l.outside_is_untouched();
    }
}

#[test]
fn a_plain_failure_at_any_snapshot_step_is_rolled_back_in_process_without_leftovers() {
    for point in SNAPSHOT_POINTS {
        let (l, before) = lab_with_v1();
        let (code, report) = failed(l.update(&l.v2, "1.0.0", Some(&at(point, Fault::Fail))));
        assert_eq!(code, "injected_failure", "{point:?}");
        assert_eq!(report.status, "restored", "{point:?}: {report:?}");
        l.assert_clean();
        assert_eq!(fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap(), before, "{point:?}");
        assert_eq!(l.active().as_deref(), Some("1.0.0"));
        assert_eq!(l.update(&l.v2, "1.0.0", None).unwrap().receipt.to_version.as_deref(), Some("2.0.0"), "{point:?}");
    }
}

#[test]
fn a_foreign_file_already_at_the_intended_snapshot_name_makes_the_create_fail_and_is_never_deleted() {
    let (l, before) = lab_with_v1();
    let snaps = l.p("snapshots");
    let hook = move |p: Point| {
        if p == Point::SnapshotBeforeCreate {
            let dir = fs::read_dir(&snaps).unwrap().flatten().next().unwrap().path();
            fs::write(dir.join("pointer.json"), b"a foreign file with the intended name").unwrap();
        }
        None
    };
    let (code, report) = failed(l.update(&l.v2, "1.0.0", Some(&hook)));
    assert_eq!(code, "io_error", "the exclusive create refuses to adopt it");
    assert!(report.not_undone.iter().any(|n| n.what == "snapshot folder" && n.why.contains("ambiguous orphan")), "{report:?}");
    let orphan = the_orphan_in_the_snapshot(&l);
    assert_eq!(fs::read(&orphan).unwrap(), b"a foreign file with the intended name");
    for _ in 0..2 {
        assert!(matches!(&l.recover(None)[..], [Recovered::NeedsAttention(..)]));
        assert_eq!(fs::read(&orphan).unwrap(), b"a foreign file with the intended name");
    }
    assert_eq!(fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap(), before);
    fs::remove_file(&orphan).unwrap();
    assert!(matches!(&l.recover(None)[..], [Recovered::RolledBack(_)]));
    l.assert_clean();
    assert!(l.update(&l.v2, "1.0.0", None).is_ok());
}

#[test]
fn a_partial_snapshot_file_replaced_by_another_file_after_a_crash_is_kept() {
    for point in [Point::SnapshotDuringWrite, Point::SnapshotAfterSync] {
        let (l, before) = lab_with_v1();
        assert!(matches!(l.update(&l.v2, "1.0.0", Some(&at(point, Fault::Crash))), Err(TxnError::Interrupted(_))));
        let orphan = the_orphan_in_the_snapshot(&l);
        replace_file(&orphan, b"alien after the crash");
        let rec = l.recover(None);
        match &rec[..] {
            [Recovered::NeedsAttention(_, report)] => assert_not_ours(report, "snapshot folder"),
            other => panic!("{point:?}: {other:?}"),
        }
        assert_eq!(fs::read(&orphan).unwrap(), b"alien after the crash");
        assert_eq!(fs::read(l.p(&format!("state/{TOOL}.json"))).unwrap(), before);
    }
}

#[test]
fn the_staged_artifact_follows_the_same_discipline_at_every_crash_point() {
    for point in [Point::StageBeforeCreate, Point::StageAfterCreate, Point::DuringStageWrite, Point::StageAfterSync, Point::AfterStageWrite]
    {
        let l = lab();
        let r = l.install(&l.v1, Some(&at(point, Fault::Crash)));
        assert!(matches!(r, Err(TxnError::Interrupted(p)) if p == point), "{point:?}: {r:?}");
        let entry = owned_role(&l, "staged_artifact");
        match point {
            Point::StageBeforeCreate | Point::StageAfterCreate => {
                assert_eq!((entry.phase, entry.identity), (Phase::Intended, None), "{point:?}")
            }
            Point::DuringStageWrite | Point::StageAfterSync => {
                assert!(entry.phase == Phase::Created && entry.identity.is_some(), "{point:?}: {entry:?}")
            }
            _ => assert!(entry.phase == Phase::Final && entry.sha256.is_some(), "{point:?}: {entry:?}"),
        }
        if point == Point::StageAfterCreate {
            let stage = l.p(&format!("staging/{}", l.files("staging")[0]));
            for _ in 0..2 {
                match &l.recover(None)[..] {
                    [Recovered::NeedsAttention(_, report)] => {
                        assert!(
                            report.not_undone.iter().any(|n| n.what == "staging folder" && n.why.contains("ambiguous orphan")),
                            "{report:?}"
                        )
                    }
                    other => panic!("{other:?}"),
                }
                assert!(stage.join("artifact.bin.part").exists());
            }
            fs::remove_file(stage.join("artifact.bin.part")).unwrap();
        }
        assert!(matches!(&l.recover(None)[..], [Recovered::RolledBack(_)]), "{point:?}");
        l.assert_clean();
        assert_eq!(l.active(), None);
        assert!(l.recover(None).is_empty());
        assert!(l.install(&l.v1, None).is_ok(), "{point:?}: a retry works");
        l.outside_is_untouched();
    }
}

#[test]
fn leftovers_after_a_recovered_rollback_are_reported_not_dropped() {
    let (l, _) = lab_with_v1();
    // a crash after the snapshot exists but before the update did anything else
    assert!(matches!(l.update(&l.v2, "1.0.0", Some(&at(Point::AfterSnapshot, Fault::Crash))), Err(TxnError::Interrupted(_))));
    let dir = l.p(&format!("snapshots/{}", l.files("snapshots")[0]));
    fs::write(dir.join("extra.txt"), b"mine").unwrap();
    let rec = l.recover(None);
    match &rec[..] {
        [Recovered::NeedsAttention(_, report)] => assert!(report.not_undone.iter().any(|n| n.what == "snapshot folder"), "{report:?}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(fs::read(dir.join("extra.txt")).unwrap(), b"mine");
    fs::remove_file(dir.join("extra.txt")).unwrap();
    assert!(matches!(&l.recover(None)[..], [Recovered::RolledBack(_)]));
    l.assert_clean();
}
