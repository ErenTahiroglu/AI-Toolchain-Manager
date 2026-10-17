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
    assert!(report.undone.iter().any(|u| u.contains("version folder 2.0.0")), "what was undone is stated: {report:?}");
    // the truth on disk matches the report: the pointer still points at 2.0.0, whose folder is gone
    assert_eq!(l.active().as_deref(), Some("2.0.0"));
    assert!(l.artifact_of("2.0.0").is_none());
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
