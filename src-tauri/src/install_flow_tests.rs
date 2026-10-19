//! Phase 3D-2: the installation workflow service, exercised the way the interface drives it (identifiers in, structured answers out),
//! with SYNTHETIC tools, SYNTHETIC bytes and isolated temporary data directories only. No real tool, no network, no process.

use super::supply::{self, HELLO, LICENSE_PENDING, UNVERIFIED, VERSION};
use super::*;
use crate::{
    catalog, db,
    lifecycle::{Fault, Point},
};
use chrono::{Duration, TimeZone};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2027, 1, 1, 12, 0, 0).unwrap()
}

struct Lab {
    _tmp: tempfile::TempDir,
    base: PathBuf,
    data: PathBuf,
    db: Mutex<Option<Connection>>,
    svc: InstallService,
    clock: Arc<Mutex<DateTime<Utc>>>,
}

fn lab_with(sources: Sources, fault: Option<FaultHook>) -> Lab {
    crate::managed_fs::hook::clear();
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let data = base.join("appdata");
    fs::create_dir_all(&data).unwrap();
    fs::write(base.join("canary.txt"), b"untouched").unwrap();
    let conn = db::open_file(&data.join("atm.sqlite"), None).unwrap();
    catalog::seed_all(&conn).unwrap();
    supply::seed_catalog(&conn).unwrap();
    let clock = Arc::new(Mutex::new(t0()));
    let c2 = clock.clone();
    let mut svc = InstallService::with_sources(data.clone(), sources, Arc::new(move || *c2.lock().unwrap()));
    svc.fault = fault;
    Lab { _tmp: tmp, base, data, db: Mutex::new(Some(conn)), svc, clock }
}

fn lab() -> Lab {
    lab_with(supply::sources(), None)
}

fn restart(l: &Lab, fault: Option<FaultHook>) -> InstallService {
    // a new process: a new consent book, no busy set; the same data directory and database
    let c2 = l.clock.clone();
    let mut svc = InstallService::with_sources(l.data.clone(), supply::sources(), Arc::new(move || *c2.lock().unwrap()));
    svc.fault = fault;
    svc
}

impl Lab {
    fn with<T>(&self, f: impl FnOnce(&Connection) -> T) -> T {
        let g = self.db.lock().unwrap();
        f(g.as_ref().unwrap())
    }

    fn try_preview(&self, tool: &str, action: Action) -> AppResult<InstallPreview> {
        self.with(|c| self.svc.prepare(c, &PrepareRequest { entry_id: tool.to_string(), action }))
    }

    fn preview(&self, tool: &str, action: Action) -> InstallPreview {
        self.try_preview(tool, action).unwrap_or_else(|e| panic!("preview of {tool} ({action:?}): {e}"))
    }

    fn confirm(&self, p: &InstallPreview) -> InstallResult {
        self.svc.confirm(
            &self.db,
            &ConfirmRequest { offer_id: p.offer_id.clone(), preview_digest: p.preview_digest.clone(), acknowledged: true },
        )
    }

    fn state(&self, tool: &str) -> ToolState {
        self.with(|c| self.svc.state(c, tool).unwrap())
    }

    fn install(&self) -> InstallResult {
        let p = self.preview(HELLO, Action::Install);
        self.confirm(&p)
    }

    fn remove(&self) -> InstallResult {
        let p = self.preview(HELLO, Action::Uninstall);
        self.confirm(&p)
    }

    fn root(&self) -> PathBuf {
        root_of(&self.data)
    }

    fn events(&self) -> Vec<Value> {
        self.with(|c| audit::list_ascending(c).unwrap())
    }

    fn advance(&self, seconds: i64) {
        let mut c = self.clock.lock().unwrap();
        *c += Duration::seconds(seconds);
    }
}

fn root_of(data: &Path) -> PathBuf {
    let container = data.join("atm-managed");
    let mut roots: Vec<PathBuf> = fs::read_dir(&container).unwrap().flatten().map(|e| e.path()).collect();
    assert_eq!(roots.len(), 1, "exactly one manager folder");
    roots.remove(0)
}

fn tree(dir: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, Option<Vec<u8>>>) {
        let mut entries: Vec<_> = fs::read_dir(dir).map(|r| r.flatten().collect()).unwrap_or_default();
        entries.sort_by_key(|e| e.path());
        for e in entries {
            let rel = e.path().strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
            let ft = fs::symlink_metadata(e.path()).unwrap().file_type();
            if ft.is_dir() {
                out.insert(format!("{rel}/"), None);
                walk(base, &e.path(), out);
            } else {
                out.insert(rel, Some(fs::read(e.path()).unwrap_or_default()));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn artifact_path(l: &Lab) -> PathBuf {
    l.root().join(format!("versions/{HELLO}/{VERSION}/artifact.bin"))
}

fn expected_bytes() -> Vec<u8> {
    let platform = HostFacts::detect().platform;
    crate::synthetic::artifact_bytes(&platform, &supply::payload(HELLO))
}

fn code(r: &InstallResult) -> &str {
    r.code.as_deref().unwrap_or("")
}

// =====================================================================================================================================
// The release supply is empty
// =====================================================================================================================================

#[test]
fn the_release_supply_offers_nothing_every_real_entry_is_verification_incomplete_and_no_folder_is_created() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().canonicalize().unwrap().join("appdata");
    fs::create_dir_all(&data).unwrap();
    let conn = db::open_file(&data.join("atm.sqlite"), None).unwrap();
    catalog::seed_all(&conn).unwrap();
    let svc = InstallService::new(data.clone());
    let states = svc.list_states(&conn).unwrap();
    assert!(states.len() >= 13, "all seeded catalog entries are listed: {}", states.len());
    for s in &states {
        assert_eq!(s.status, Status::VerificationIncomplete, "{}", s.entry_id);
        assert!(!s.can_install && !s.can_remove && s.version.is_none());
        assert_eq!(s.reasons[0].code, "no_reviewed_recipe");
    }
    let before = tree(&data);
    for s in &states {
        let r = svc.prepare(&conn, &PrepareRequest { entry_id: s.entry_id.clone(), action: Action::Install });
        assert_eq!(r.unwrap_err().code, "INSTALL_NOT_AVAILABLE", "{}", s.entry_id);
        assert!(svc.details(&conn, &s.entry_id).unwrap().receipt.is_none());
    }
    // a confirmation of anything is refused, and nothing was created: no manager folder, no file
    let db = Mutex::new(Some(conn));
    let r = svc.confirm(&db, &ConfirmRequest { offer_id: "a".repeat(64), preview_digest: "b".repeat(64), acknowledged: true });
    assert_eq!(r.outcome, "refused");
    assert!(!data.join("atm-managed").exists());
    let after: BTreeMap<_, _> = tree(&data).into_iter().filter(|(k, _)| !k.starts_with("atm.sqlite")).collect();
    let before: BTreeMap<_, _> = before.into_iter().filter(|(k, _)| !k.starts_with("atm.sqlite")).collect();
    assert_eq!(before, after);
    assert_eq!(svc.sources().registry.len(), 0);
    assert!(svc.sources().artifacts.bytes("synthetic-hello", "1.0.0", "windows-x64").is_err());
    assert!(svc.recover(db.lock().unwrap().as_ref().unwrap()).unwrap().notes.is_empty());
}

#[test]
fn the_three_situations_are_told_apart_and_a_real_entry_never_passes_even_with_the_synthetic_supply() {
    let l = lab();
    assert_eq!(l.state(HELLO).status, Status::Available);
    let u = l.state(UNVERIFIED);
    assert_eq!(u.status, Status::VerificationIncomplete);
    assert!(u.reasons.iter().any(|r| r.code == "artifact_not_authenticated"), "{u:?}");
    assert!(!u.can_install);
    let p = l.state(LICENSE_PENDING);
    assert_eq!(p.status, Status::LicenseReviewRequired, "{p:?}");
    assert!(p.reasons.iter().any(|r| r.code == "license_not_accepted") && !p.can_install);
    // a real catalog entry has no recipe in the synthetic supply either
    let g = l.state("gitleaks");
    assert_eq!(g.status, Status::VerificationIncomplete);
    assert!(l.try_preview("gitleaks", Action::Install).is_err());
    assert!(l.try_preview(UNVERIFIED, Action::Install).is_err());
    assert!(l.try_preview(LICENSE_PENDING, Action::Install).is_err());
    assert!(!l.data.join("atm-managed").exists(), "nothing was created by looking");
}

// =====================================================================================================================================
// The vertical slice: catalog -> preview -> confirm -> result -> installed details -> removal
// =====================================================================================================================================

#[test]
fn the_whole_workflow_installs_verifies_shows_details_removes_and_reinstalls() {
    let l = lab();
    assert_eq!(l.state(HELLO).status, Status::Available);
    let p = l.preview(HELLO, Action::Install);
    assert_eq!(p.content.version, VERSION);
    assert_eq!(p.content.platform_target, HostFacts::detect().platform);
    assert_eq!(p.content.artifact.sha256, crate::synthetic::sha256_of(&expected_bytes()));
    assert!(p.content.synthetic && !p.content.network_used);
    assert!(!p.content.manager_folder_exists);
    assert_eq!(l.state(HELLO).status, Status::ReadyForApproval, "a preview waits for the confirmation");
    assert!(!l.data.join("atm-managed").exists(), "a preview creates nothing");

    let r = l.confirm(&p);
    assert_eq!(r.outcome, "completed", "{r:?}");
    assert_eq!(r.state.as_ref().unwrap().status, Status::InstalledVerified);
    assert_eq!(r.receipt.as_ref().unwrap().action, Action::Install);
    assert_eq!(fs::read(artifact_path(&l)).unwrap(), expected_bytes());
    // the version folder holds the one file and nothing else
    let folder: Vec<String> = fs::read_dir(artifact_path(&l).parent().unwrap())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(folder, vec!["artifact.bin".to_string()]);

    let d = l.with(|c| l.svc.details(c, HELLO).unwrap());
    assert_eq!(d.state.status, Status::InstalledVerified);
    assert_eq!(d.version.as_deref(), Some(VERSION));
    assert_eq!(d.location.as_deref(), Some(&*format!("versions/{HELLO}/{VERSION}/artifact.bin")));
    assert!(d.checks.len() >= 5 && d.checks.iter().all(|c| c.passed));
    assert_eq!(d.history.len(), 1);
    assert!(d.last_result.is_some());
    assert!(!d.location.unwrap().contains(':') && !d.never.is_empty());

    // a second install of the same tool is refused before anything is offered
    assert_eq!(l.try_preview(HELLO, Action::Install).unwrap_err().code, "INSTALL_ALREADY_INSTALLED");

    // removal goes through the same preview and confirmation
    let rp = l.preview(HELLO, Action::Uninstall);
    assert!(rp.content.effects.iter().any(|e| e.change == "remove"));
    let rr = l.confirm(&rp);
    assert_eq!(rr.outcome, "completed", "{rr:?}");
    assert_eq!(rr.state.as_ref().unwrap().status, Status::Removed);
    assert!(!artifact_path(&l).exists() && !l.root().join(format!("versions/{HELLO}")).exists());
    assert_eq!(l.try_preview(HELLO, Action::Uninstall).unwrap_err().code, "INSTALL_NOT_INSTALLED");

    // removed tools can be installed again
    assert_eq!(l.install().outcome, "completed");
    assert_eq!(l.state(HELLO).status, Status::InstalledVerified);
    assert_eq!(l.with(|c| l.svc.details(c, HELLO).unwrap()).history.len(), 3);
}

#[test]
fn everything_the_preview_promises_is_what_happens_to_the_file_system_at_every_moment() {
    // At every injection point of a real run the manager folder is walked; every path seen must be covered by a declared effect.
    let seen: Arc<Mutex<std::collections::BTreeSet<String>>> = Arc::default();
    let data_cell: Arc<Mutex<Option<PathBuf>>> = Arc::default();
    let (s2, d2) = (seen.clone(), data_cell.clone());
    let hook: FaultHook = Arc::new(move |_p: Point| {
        if let Some(data) = d2.lock().unwrap().clone() {
            if data.join("atm-managed").exists() {
                let root = root_of(&data);
                for k in tree(&root).keys() {
                    s2.lock().unwrap().insert(k.clone());
                }
            }
        }
        None
    });
    let l = lab_with(supply::sources(), Some(hook));
    *data_cell.lock().unwrap() = Some(l.data.clone());

    let normalise = |rel: &str| -> String {
        let re = regex::Regex::new(r"txn-[0-9a-f]{32}").unwrap();
        re.replace_all(rel, "<işlem-kimliği>").to_string()
    };
    let covered = |seen: &str, declared: &[String]| -> bool {
        let s = seen.trim_end_matches('/');
        declared.iter().any(|d| {
            let d = d.trim_end_matches('/');
            // `*` stands for the random part of a temporary file name
            let re = regex::Regex::new(&format!("^{}(/.*)?$", regex::escape(d).replace("\\*", "[^/]*"))).unwrap();
            re.is_match(s) || d.starts_with(&format!("{s}/"))
        })
    };

    for action in [Action::Install, Action::Uninstall] {
        let p = l.preview(HELLO, action);
        let declared: Vec<String> = p
            .content
            .effects
            .iter()
            .filter(|e| e.change != "read")
            .map(|e| e.place.clone())
            .chain(["journal/<işlem-kimliği>".to_string(), "locks".to_string()])
            .collect();
        seen.lock().unwrap().clear();
        let r = l.confirm(&p);
        assert_eq!(r.outcome, "completed", "{r:?}");
        let seen_now: Vec<String> = seen.lock().unwrap().iter().map(|s| normalise(s)).collect();
        for s in &seen_now {
            // the fixed layout folders are always present
            if ["data/", "journal/", "locks/", "receipts/", "snapshots/", "staging/", "state/", "versions/"].contains(&s.as_str()) {
                continue;
            }
            assert!(covered(s, &declared), "{action:?}: {s} is not covered by the preview: {declared:?}");
        }
        // and what REMAINS afterwards is exactly what the preview says remains
        let after: Vec<String> = tree(&l.root()).keys().map(|k| normalise(k)).collect();
        for e in p.content.effects.iter().filter(|e| e.remains && e.change != "read") {
            let place = e.place.trim_end_matches('/');
            if place.starts_with('(') {
                continue;
            }
            assert!(
                after.iter().any(|a| a.trim_end_matches('/') == place
                    || a.starts_with(&format!("{place}/"))
                    || place.contains("<") && a.starts_with(place.split('<').next().unwrap())),
                "{action:?}: promised to remain: {place}; found {after:?}"
            );
        }
        for a in &after {
            if ["data/", "journal/", "locks/", "receipts/", "snapshots/", "staging/", "state/", "versions/"].contains(&a.as_str()) {
                continue;
            }
            let remains: Vec<String> = p.content.effects.iter().filter(|e| e.remains).map(|e| e.place.clone()).collect();
            assert!(covered(a, &remains), "{action:?}: {a} remains but the preview did not say so: {remains:?}");
        }
    }
}

#[test]
fn the_preview_states_what_will_not_happen_and_what_the_confirmation_is_and_is_not() {
    let l = lab();
    let p = l.preview(HELLO, Action::Install);
    let never = p.content.not_done.join(" ");
    for needle in ["çalıştırılmaz", "indirilmez", "PATH", "ajanın ayar", "Yönetici yetkisi", "Akıllı Uygulama Denetimi"] {
        assert!(never.contains(needle), "{needle}");
    }
    let s = &p.content.consent_statement;
    assert!(s.contains("kimlik kanıtı DEĞİLDİR") && s.contains("aynı Windows hesabında") && s.contains("tek kullanımlıktır"), "{s}");
    for e in &p.content.effects {
        assert!(
            !e.place.contains(':') && !e.place.starts_with('/') && !e.place.starts_with('\\'),
            "no absolute path in the preview: {}",
            e.place
        );
    }
    assert!(
        p.content.effects.iter().any(|e| e.place.starts_with("(yöneticinin kendi klasörü")),
        "the first use creates the folder and says so"
    );
    assert!(
        p.content.rollback.text.contains("SENTETİK")
            && !p.content.rollback.undoes.is_empty()
            && !p.content.rollback.does_not_undo.is_empty()
    );
    assert_eq!(p.content.authentication, "authenticated_reference");
    assert_eq!(p.content.artifact.digest_scope, "exact_file");
}

// =====================================================================================================================================
// Consent: replay, expiry, cancel, forged payloads
// =====================================================================================================================================

#[test]
fn a_confirmation_replay_changes_nothing_and_is_audited() {
    let l = lab();
    let p = l.preview(HELLO, Action::Install);
    assert_eq!(l.confirm(&p).outcome, "completed");
    let snapshot = tree(&l.root());
    let again = l.confirm(&p);
    assert_eq!((again.outcome.as_str(), code(&again)), ("refused", "consent_already_used"));
    assert_eq!(tree(&l.root()), snapshot);
    let installs = l.events().iter().filter(|e| e["eventType"] == "install" && e["outcome"] == "passed").count();
    assert_eq!(installs, 1);
    assert!(l
        .events()
        .iter()
        .any(|e| e["eventType"] == "approve" && e["outcome"] == "blocked" && e["errorCode"] == "CONSENT_ALREADY_USED"));
}

#[test]
fn an_expired_preview_is_refused_and_nothing_is_created() {
    let l = lab();
    let p = l.preview(HELLO, Action::Install);
    l.advance(crate::consent::OFFER_TTL_SECONDS);
    let r = l.confirm(&p);
    assert_eq!((r.outcome.as_str(), code(&r)), ("refused", "consent_expired"), "{r:?}");
    assert!(!l.data.join("atm-managed").exists());
    assert_eq!(l.state(HELLO).status, Status::Available);
    // a fresh preview works again
    assert_eq!(l.install().outcome, "completed");
}

#[test]
fn a_cancelled_preview_can_never_be_confirmed_and_leaves_no_trace_but_the_audit_row() {
    let l = lab();
    let p = l.preview(HELLO, Action::Install);
    assert_eq!(l.state(HELLO).status, Status::ReadyForApproval);
    l.with(|c| l.svc.cancel(c, &CancelRequest { offer_id: p.offer_id.clone() })).unwrap();
    assert_eq!(l.state(HELLO).status, Status::Available, "cancelling returns the tool to the state it was in");
    let r = l.confirm(&p);
    assert_eq!((r.outcome.as_str(), code(&r)), ("refused", "consent_cancelled"));
    assert!(!l.data.join("atm-managed").exists());
    assert!(l.events().iter().any(|e| e["eventType"] == "reject" && e["outcome"] == "passed"));
    // cancelling twice, or something that never existed, is an honest error
    assert!(l.with(|c| l.svc.cancel(c, &CancelRequest { offer_id: p.offer_id.clone() })).is_err());
    assert!(l.with(|c| l.svc.cancel(c, &CancelRequest { offer_id: "0".repeat(64) })).is_err());
}

#[test]
fn forged_ipc_payloads_are_refused_at_every_layer() {
    // 1. the request types accept identifiers and one acknowledgement, nothing else (unknown fields are an error)
    for forged in [
        serde_json::json!({ "offerId": "a".repeat(64), "previewDigest": "b".repeat(64), "acknowledged": true, "path": "C:\\Windows\\System32" }),
        serde_json::json!({ "offerId": "a".repeat(64), "previewDigest": "b".repeat(64), "acknowledged": true, "sha256": "c".repeat(64) }),
        serde_json::json!({ "offerId": "a".repeat(64), "previewDigest": "b".repeat(64), "acknowledged": true, "bytes": [1, 2, 3] }),
        serde_json::json!({ "offerId": "a".repeat(64), "previewDigest": "b".repeat(64), "acknowledged": true, "command": "cmd /c calc" }),
        serde_json::json!({ "offerId": "a".repeat(64), "previewDigest": "b".repeat(64) }),
        serde_json::json!({ "offerId": 5, "previewDigest": "b".repeat(64), "acknowledged": true }),
    ] {
        assert!(serde_json::from_value::<ConfirmRequest>(forged.clone()).is_err(), "{forged}");
    }
    for forged in [
        serde_json::json!({ "entryId": HELLO, "action": "install", "url": "https://example.com/x.exe" }),
        serde_json::json!({ "entryId": HELLO, "action": "install", "destination": "C:\\Users" }),
        serde_json::json!({ "entryId": HELLO, "action": "run" }),
        serde_json::json!({ "entryId": HELLO }),
    ] {
        assert!(serde_json::from_value::<PrepareRequest>(forged.clone()).is_err(), "{forged}");
    }
    assert!(serde_json::from_value::<CancelRequest>(serde_json::json!({ "offerId": "a".repeat(64), "force": true })).is_err());

    // 2. well-formed requests with forged content
    let l = lab();
    let p = l.preview(HELLO, Action::Install);
    for bad in ["", "x", &"A".repeat(64), "..\\..\\x", &format!("{}'; DROP TABLE audit_events; --", p.offer_id)] {
        let r = l
            .svc
            .confirm(&l.db, &ConfirmRequest { offer_id: bad.to_string(), preview_digest: p.preview_digest.clone(), acknowledged: true });
        assert_eq!((r.outcome.as_str(), code(&r)), ("refused", "consent_bad_identifier"), "{bad:?}");
    }
    let r =
        l.svc.confirm(&l.db, &ConfirmRequest { offer_id: "0".repeat(64), preview_digest: p.preview_digest.clone(), acknowledged: true });
    assert_eq!(code(&r), "consent_unknown");
    // a wrong preview digest burns the real offer
    let r = l.svc.confirm(&l.db, &ConfirmRequest { offer_id: p.offer_id.clone(), preview_digest: "f".repeat(64), acknowledged: true });
    assert_eq!(code(&r), "consent_preview_changed");
    assert_eq!(code(&l.confirm(&p)), "consent_cancelled", "the right digest no longer helps: the offer is burned");
    // not acknowledged
    let p2 = l.preview(HELLO, Action::Install);
    let r = l
        .svc
        .confirm(&l.db, &ConfirmRequest { offer_id: p2.offer_id.clone(), preview_digest: p2.preview_digest.clone(), acknowledged: false });
    assert_eq!(code(&r), "consent_not_acknowledged");
    // an entry id that is not an entry
    for bad in ["../../etc/passwd", "C:\\Windows", "", "no-such-tool"] {
        assert!(l.try_preview(bad, Action::Install).is_err(), "{bad:?}");
    }
    assert_eq!(l.try_preview(HELLO, Action::Update).unwrap_err().code, "INSTALL_ACTION_UNSUPPORTED");
    assert!(!l.data.join("atm-managed").exists(), "none of this created anything");
    // the audit chain survived the hostile input
    assert!(l.with(|c| audit::verify(c).unwrap()).ok);
}

#[test]
fn a_preview_for_one_tool_cannot_confirm_another_and_a_newer_preview_replaces_the_older() {
    let l = lab();
    let first = l.preview(HELLO, Action::Install);
    let second = l.preview(HELLO, Action::Install);
    assert_ne!(first.offer_id, second.offer_id);
    assert_eq!(code(&l.confirm(&first)), "consent_superseded");
    assert_eq!(l.confirm(&second).outcome, "completed");
}

// =====================================================================================================================================
// Wrong tool, version, digest, platform; stale plans
// =====================================================================================================================================

struct Swappable(Arc<Mutex<Vec<u8>>>);

impl ArtifactProvider for Swappable {
    fn bytes(&self, _: &str, _: &str, _: &str) -> Result<Vec<u8>, &'static str> {
        Ok(self.0.lock().unwrap().clone())
    }
}

fn lab_with_swappable_bytes() -> (Lab, Arc<Mutex<Vec<u8>>>) {
    let mut sources = supply::sources();
    let cell = Arc::new(Mutex::new(expected_bytes()));
    sources.artifacts = Box::new(Swappable(cell.clone()));
    (lab_with(sources, None), cell)
}

#[test]
fn wrong_bytes_are_refused_before_a_preview_and_swapped_bytes_are_refused_before_the_first_change() {
    let (l, cell) = lab_with_swappable_bytes();
    // wrong digest from the start: no preview is even offered
    let mut wrong = expected_bytes();
    *wrong.last_mut().unwrap() ^= 1;
    *cell.lock().unwrap() = wrong.clone();
    assert_eq!(l.try_preview(HELLO, Action::Install).unwrap_err().code, "INSTALL_ARTIFACT_REJECTED");
    // wrong length, wrong format (not an executable header), empty
    for bad in [expected_bytes()[..10].to_vec(), b"plain text, not a program".to_vec(), Vec::new()] {
        *cell.lock().unwrap() = bad;
        assert_eq!(l.try_preview(HELLO, Action::Install).unwrap_err().code, "INSTALL_ARTIFACT_REJECTED");
    }
    // a good preview, then the bytes change before the confirmation: nothing is applied
    *cell.lock().unwrap() = expected_bytes();
    let p = l.preview(HELLO, Action::Install);
    *cell.lock().unwrap() = wrong;
    let r = l.confirm(&p);
    assert_eq!(r.outcome, "refused", "{r:?}");
    assert!(matches!(code(&r), "artifact_does_not_match_recipe" | "artifact_format_mismatch"), "{r:?}");
    assert!(!l.data.join("atm-managed").exists(), "nothing was created");
    assert_eq!(l.state(HELLO).status, Status::Available);
}

#[test]
fn a_recipe_for_another_platform_or_another_version_never_makes_a_tool_installable_here() {
    // the supply only knows the OTHER platform
    let other = if HostFacts::detect().platform == "windows-x64" { "macos-arm64" } else { "windows-x64" };
    let mut s = supply::sources();
    let all = s.registry.find(HELLO, other, SURFACE, SCOPE).into_iter().cloned().collect::<Vec<_>>();
    s.registry = RecipeRegistry::synthetic(all).unwrap();
    let l = lab_with(s, None);
    let st = l.state(HELLO);
    assert_eq!(st.status, Status::VerificationIncomplete);
    assert_eq!(st.reasons[0].code, "no_reviewed_recipe");
    assert!(l.try_preview(HELLO, Action::Install).is_err());

    // the authenticated reference names another version than the recipe
    let mut s = supply::sources();
    for r in s.extra_authenticated.iter_mut().filter(|r| r.entry_id == HELLO) {
        r.version = "9.9.9".to_string();
    }
    let l = lab_with(s, None);
    assert_eq!(l.state(HELLO).status, Status::VerificationIncomplete);
    assert!(l.try_preview(HELLO, Action::Install).is_err());

    // the authenticated reference names another digest than the recipe
    let mut s = supply::sources();
    for r in s.extra_authenticated.iter_mut().filter(|r| r.entry_id == HELLO) {
        r.sha256 = "0".repeat(64);
    }
    let l = lab_with(s, None);
    assert_eq!(l.state(HELLO).status, Status::VerificationIncomplete);
}

#[test]
fn a_change_after_the_preview_makes_the_confirmation_stale_and_nothing_is_applied() {
    // (a) the catalog entry is quarantined after the preview
    let l = lab();
    let p = l.preview(HELLO, Action::Install);
    l.with(|c| c.execute("UPDATE catalog_entries SET review_state = 'quarantined' WHERE entry_id = ?1", [HELLO]).unwrap());
    let r = l.confirm(&p);
    assert_eq!((r.outcome.as_str(), code(&r)), ("refused", "preview_stale"), "{r:?}");
    assert!(!l.data.join("atm-managed").exists());
    assert_eq!(l.state(HELLO).status, Status::VerificationIncomplete);

    // (b) the manager's folder state changes between preview and confirmation (a first install elsewhere created the folder)
    let l = lab();
    let p = l.preview(HELLO, Action::Install);
    assert!(!p.content.manager_folder_exists);
    let other = l.preview(HELLO, Action::Install); // supersedes p, same content
    assert_eq!(l.confirm(&other).outcome, "completed");
    let r = l.confirm(&p);
    assert_eq!(code(&r), "consent_superseded");

    // (c) the same tool is removed between a removal preview and its confirmation
    let l = lab();
    assert_eq!(l.install().outcome, "completed");
    let p = l.preview(HELLO, Action::Uninstall);
    // the installed file is tampered with after the preview
    let f = artifact_path(&l);
    let mut bytes = fs::read(&f).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(&f, &bytes).unwrap();
    let r = l.confirm(&p);
    assert_eq!((r.outcome.as_str(), code(&r)), ("refused", "needs_attention"), "{r:?}");
    assert_eq!(fs::read(&f).unwrap(), bytes, "nothing was repaired, replaced or deleted");
}

// =====================================================================================================================================
// Concurrency
// =====================================================================================================================================

#[test]
fn many_concurrent_confirmations_of_one_preview_install_exactly_once() {
    let l = Arc::new(lab());
    let p = l.preview(HELLO, Action::Install);
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let (l, p, b) = (l.clone(), p.clone(), barrier.clone());
            std::thread::spawn(move || {
                b.wait();
                l.confirm(&p)
            })
        })
        .collect();
    let results: Vec<InstallResult> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.outcome == "completed").count(), 1, "{results:?}");
    assert!(results.iter().filter(|r| r.outcome != "completed").all(|r| code(r) == "consent_already_used"));
    assert_eq!(l.events().iter().filter(|e| e["eventType"] == "install" && e["outcome"] == "passed").count(), 1);
    assert_eq!(l.state(HELLO).status, Status::InstalledVerified);
}

#[test]
fn a_second_request_for_a_tool_that_is_being_installed_is_refused_and_the_first_finishes() {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let entered_tx = Mutex::new(entered_tx);
    let release_rx = Mutex::new(release_rx);
    let hook: FaultHook = Arc::new(move |p: Point| {
        if p == Point::AfterLock {
            entered_tx.lock().unwrap().send(()).unwrap();
            release_rx.lock().unwrap().recv().unwrap();
        }
        None
    });
    let l = Arc::new(lab_with(supply::sources(), Some(hook)));
    let p = l.preview(HELLO, Action::Install);
    let l2 = l.clone();
    let first = std::thread::spawn(move || l2.confirm(&p));
    entered_rx.recv().unwrap();
    // the first request holds the tool's lock: "installing", and a second preview/confirmation is refused without touching anything
    assert_eq!(l.state(HELLO).status, Status::Installing);
    assert!(l.try_preview(HELLO, Action::Install).is_err());
    assert!(l.try_preview(HELLO, Action::Uninstall).is_err());
    release_tx.send(()).unwrap();
    let r = first.join().unwrap();
    assert_eq!(r.outcome, "completed", "{r:?}");
    assert_eq!(l.state(HELLO).status, Status::InstalledVerified);
    assert_eq!(l.events().iter().filter(|e| e["eventType"] == "install" && e["outcome"] == "passed").count(), 1);
}

// =====================================================================================================================================
// Foreign content, redirection, tampering, failures
// =====================================================================================================================================

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
fn foreign_files_in_the_version_folder_stop_the_install_and_are_never_touched() {
    let l = lab();
    assert_eq!(l.install().outcome, "completed");
    assert_eq!(l.remove().outcome, "completed");
    // someone else's folder with a file appears where the next install would put its version
    let vdir = l.root().join(format!("versions/{HELLO}/{VERSION}"));
    fs::create_dir_all(&vdir).unwrap();
    fs::write(vdir.join("their-notes.txt"), b"not ours").unwrap();
    let before = tree(&l.root().join("versions"));
    let p = l.preview(HELLO, Action::Install);
    let r = l.confirm(&p);
    assert_eq!(r.outcome, "failed", "{r:?}");
    assert_eq!(code(&r), "version_folder_already_exists");
    assert_eq!(tree(&l.root().join("versions")), before, "their folder and file are byte-identical");
    assert!(r.rollback.is_some());
    assert_ne!(l.state(HELLO).status, Status::InstalledVerified);
}

#[test]
fn a_link_in_place_of_a_managed_folder_is_never_followed() {
    let l = lab();
    assert_eq!(l.install().outcome, "completed");
    let outside = l.base.join("outside-dir");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), b"outside content").unwrap();
    // the tool's version folder (as seen from versions/) is replaced by a link to a foreign folder
    let tool_dir = l.root().join(format!("versions/{HELLO}"));
    let moved = l.root().join("versions/moved-away");
    fs::rename(&tool_dir, &moved).unwrap();
    if !make_dir_link(&tool_dir, &outside) {
        eprintln!("directory links cannot be created here; case not exercised");
        fs::rename(&moved, &tool_dir).unwrap();
        return;
    }
    let st = l.state(HELLO);
    assert_eq!(st.status, Status::FailedNeedsAttention, "{st:?}");
    assert!(l.try_preview(HELLO, Action::Uninstall).is_err());
    assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"outside content");
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1, "nothing was written through the link");
    remove_dir_link(&tool_dir);
    fs::rename(&moved, &tool_dir).unwrap();
    assert_eq!(l.state(HELLO).status, Status::InstalledVerified, "once the owner puts things back the tool verifies again");
}

#[test]
fn a_tampered_installed_file_is_reported_never_repaired_and_blocks_removal() {
    let l = lab();
    assert_eq!(l.install().outcome, "completed");
    let f = artifact_path(&l);
    let original = fs::read(&f).unwrap();
    // same length, one bit changed
    let mut tampered = original.clone();
    *tampered.last_mut().unwrap() ^= 1;
    fs::write(&f, &tampered).unwrap();
    let st = l.state(HELLO);
    assert_eq!(st.status, Status::FailedNeedsAttention);
    assert_eq!(st.reasons[0].code, "installed_artifact_drift");
    let d = l.with(|c| l.svc.details(c, HELLO).unwrap());
    assert!(d.checks.iter().any(|c| !c.passed), "{d:?}");
    assert!(l.try_preview(HELLO, Action::Uninstall).is_err());
    assert!(l.try_preview(HELLO, Action::Install).is_err());
    assert_eq!(fs::read(&f).unwrap(), tampered, "not repaired, not replaced, not deleted");

    // an extra file next to the artifact is also drift
    fs::write(&f, &original).unwrap();
    assert_eq!(l.state(HELLO).status, Status::InstalledVerified);
    fs::write(f.parent().unwrap().join("extra.txt"), b"x").unwrap();
    assert_eq!(l.state(HELLO).reasons[0].code, "installed_folder_has_unexpected_entries");
    fs::remove_file(f.parent().unwrap().join("extra.txt")).unwrap();

    // the pointer forged to another digest
    let pointer = l.root().join(format!("state/{HELLO}.json"));
    let good = fs::read_to_string(&pointer).unwrap();
    fs::write(&pointer, good.replace(&crate::synthetic::sha256_of(&expected_bytes()), &"0".repeat(64))).unwrap();
    assert_eq!(l.state(HELLO).status, Status::FailedNeedsAttention);
    fs::write(&pointer, good).unwrap();
    assert_eq!(l.state(HELLO).status, Status::InstalledVerified);

    // a deleted artifact
    fs::remove_file(&f).unwrap();
    assert_eq!(l.state(HELLO).reasons[0].code, "installed_artifact_missing");
}

#[test]
fn a_failure_in_the_middle_of_an_install_is_rolled_back_and_reported_honestly() {
    for point in [
        Point::DuringStageWrite,
        Point::AfterStageWrite,
        Point::AfterVersionDirCreate,
        Point::AfterPointerSwitch,
        Point::BeforeReceiptWrite,
    ] {
        let hook: FaultHook = Arc::new(move |p| (p == point).then_some(Fault::Fail));
        let l = lab_with(supply::sources(), Some(hook));
        let before = tree(&l.base);
        let p = l.preview(HELLO, Action::Install);
        let r = l.confirm(&p);
        assert_eq!(r.outcome, "failed", "{point:?}: {r:?}");
        assert_eq!(r.rollback.as_ref().unwrap().status, "restored", "{point:?}: {r:?}");
        assert!(r.message.contains("eksiksiz geri getirildi"), "{}", r.message);
        let st = l.state(HELLO);
        assert_eq!(st.status, Status::Available, "{point:?}: {st:?}");
        assert!(!artifact_path(&l).exists());
        // nothing is left in the manager's folder except the permanent fixed layout and the journal
        let leftovers: Vec<String> = tree(&l.root())
            .into_iter()
            .filter(|(k, _)| {
                !["data/", "journal/", "locks/", "receipts/", "snapshots/", "staging/", "state/", "versions/"].contains(&k.as_str())
            })
            .filter(|(k, _)| !k.starts_with("journal/"))
            .map(|(k, _)| k)
            .collect();
        assert!(leftovers.is_empty(), "{point:?}: {leftovers:?}");
        // the outside world is untouched
        assert_eq!(fs::read(l.base.join("canary.txt")).unwrap(), b"untouched");
        let _ = before;
        // and the same preview mechanism installs fine afterwards (new service without the fault)
        let healthy = restart(&l, None);
        let p2 = healthy
            .prepare(l.db.lock().unwrap().as_ref().unwrap(), &PrepareRequest { entry_id: HELLO.to_string(), action: Action::Install })
            .unwrap();
        let ok = healthy.confirm(&l.db, &ConfirmRequest { offer_id: p2.offer_id, preview_digest: p2.preview_digest, acknowledged: true });
        assert_eq!(ok.outcome, "completed", "{point:?}: {ok:?}");
    }
}

#[cfg(windows)]
#[test]
fn a_file_held_open_by_another_program_stops_the_removal_and_everything_is_put_back() {
    use std::os::windows::fs::OpenOptionsExt;
    let l = lab();
    assert_eq!(l.install().outcome, "completed");
    let held = fs::OpenOptions::new().read(true).share_mode(1).open(artifact_path(&l)).unwrap();
    let p = l.preview(HELLO, Action::Uninstall);
    let r = l.confirm(&p);
    assert_eq!(r.outcome, "failed", "{r:?}");
    assert_eq!(code(&r), "fs_locked_by_another_process");
    assert!(matches!(r.rollback.as_ref().unwrap().status.as_str(), "restored" | "partial"), "{r:?}");
    drop(held);
    let _ = l.with(|c| l.svc.recover(c)).unwrap();
    assert_eq!(l.state(HELLO).status, Status::InstalledVerified);
    assert_eq!(fs::read(artifact_path(&l)).unwrap(), expected_bytes());
    assert_eq!(l.remove().outcome, "completed", "once the program released the file the removal works");
}

#[cfg(unix)]
#[test]
fn an_inaccessible_state_folder_makes_the_install_fail_with_an_honest_rollback() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("running as root: permission denial cannot be exercised");
        return;
    }
    let l = lab();
    // create the manager's folder first, then deny writing to its state folder
    assert_eq!(l.install().outcome, "completed");
    assert_eq!(l.remove().outcome, "completed");
    fs::set_permissions(l.root().join("state"), fs::Permissions::from_mode(0o500)).unwrap();
    let r = l.install();
    fs::set_permissions(l.root().join("state"), fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(r.outcome, "failed", "{r:?}");
    assert!(matches!(r.rollback.as_ref().unwrap().status.as_str(), "restored" | "partial" | "failed"), "{r:?}");
    let _ = l.with(|c| l.svc.recover(c)).unwrap();
    assert_eq!(l.install().outcome, "completed");
}

// =====================================================================================================================================
// Restart and crash recovery
// =====================================================================================================================================

#[test]
fn a_crash_at_any_point_is_recovered_after_a_restart_and_the_old_consent_is_gone() {
    let points = [
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
    ];
    for point in points {
        let hook: FaultHook = Arc::new(move |p| (p == point).then_some(Fault::Crash));
        let l = lab_with(supply::sources(), Some(hook));
        let p = l.preview(HELLO, Action::Install);
        let r = l.confirm(&p);
        assert_eq!(r.outcome, "refused", "{point:?}: {r:?}");
        assert_eq!(code(&r), "needs_attention", "{point:?}");
        // "restart": a new service on the same data and database. The unfinished transaction is visible, nothing else is assumed.
        let svc = restart(&l, None);
        let st = l.with(|c| svc.state(c, HELLO).unwrap());
        assert_eq!(st.status, Status::FailedNeedsAttention, "{point:?}: {st:?}");
        // the consent of the old process does not exist any more
        let again = svc
            .confirm(&l.db, &ConfirmRequest { offer_id: p.offer_id.clone(), preview_digest: p.preview_digest.clone(), acknowledged: true });
        assert_eq!(code(&again), "consent_unknown", "{point:?}");
        // a new preview is refused while the unfinished transaction is unresolved
        assert!(l.with(|c| svc.prepare(c, &PrepareRequest { entry_id: HELLO.to_string(), action: Action::Install })).is_err(), "{point:?}");
        // recovery undoes or finishes what the manager itself started
        let rec = l.with(|c| svc.recover(c)).unwrap();
        if matches!(point, Point::StageAfterCreate | Point::PointerAfterCreate) {
            // The documented irreducible window: the process died after a file was created and before its identity reached the journal.
            // The file cannot be proven to be this transaction's, so it is kept and reported -- never deleted on a guess -- and the tool
            // stays "needs attention" until a person looks. Nothing else is touched.
            assert!(rec.examined && rec.needs_attention >= 1, "{point:?}: {rec:?}");
            let st = l.with(|c| svc.state(c, HELLO).unwrap());
            assert_eq!(st.status, Status::FailedNeedsAttention, "{point:?}: {st:?}");
            let orphans: Vec<String> = tree(&l.root()).into_keys().filter(|k| k.ends_with(".part") || k.ends_with(".json.new")).collect();
            assert!(!orphans.is_empty(), "{point:?}: the unprovable file was kept");
            assert!(l.with(|c| svc.prepare(c, &PrepareRequest { entry_id: HELLO.to_string(), action: Action::Install })).is_err());
            assert_eq!(fs::read(l.base.join("canary.txt")).unwrap(), b"untouched");
            continue;
        }
        assert!(rec.examined && rec.needs_attention == 0, "{point:?}: {rec:?}");
        assert!(rec.rolled_back + rec.rolled_forward >= 1, "{point:?}: {rec:?}");
        let st = l.with(|c| svc.state(c, HELLO).unwrap());
        assert!(matches!(st.status, Status::Available | Status::InstalledVerified), "{point:?}: {st:?}");
        if st.status == Status::InstalledVerified {
            assert_eq!(fs::read(artifact_path(&l)).unwrap(), expected_bytes(), "{point:?}");
        } else {
            assert!(!artifact_path(&l).exists(), "{point:?}");
        }
        assert_eq!(fs::read(l.base.join("canary.txt")).unwrap(), b"untouched");
        // and the workflow works again
        if st.status == Status::Available {
            let p2 = l.with(|c| svc.prepare(c, &PrepareRequest { entry_id: HELLO.to_string(), action: Action::Install })).unwrap();
            let ok = svc.confirm(&l.db, &ConfirmRequest { offer_id: p2.offer_id, preview_digest: p2.preview_digest, acknowledged: true });
            assert_eq!(ok.outcome, "completed", "{point:?}: {ok:?}");
        }
    }
}

#[test]
fn an_installed_tool_is_still_installed_and_verified_after_a_restart() {
    let l = lab();
    assert_eq!(l.install().outcome, "completed");
    let svc = restart(&l, None);
    let st = l.with(|c| svc.state(c, HELLO).unwrap());
    assert_eq!(st.status, Status::InstalledVerified);
    let d = l.with(|c| svc.details(c, HELLO).unwrap());
    assert!(d.checks.iter().all(|c| c.passed) && d.receipt.is_some());
    assert!(d.last_result.is_none(), "the in-memory last result does not survive a restart; the receipt and the audit trail do");
    assert!(l.with(|c| svc.recover(c)).unwrap().notes.is_empty());
}

// =====================================================================================================================================
// User data, environment, processes, audit
// =====================================================================================================================================

#[test]
fn the_tools_own_user_data_and_everything_outside_the_manager_folder_are_never_modified() {
    let l = lab();
    assert_eq!(l.install().outcome, "completed");
    let userdata = l.root().join(format!("data/{HELLO}"));
    fs::create_dir_all(&userdata).unwrap();
    fs::write(userdata.join("settings.json"), b"{\"theme\":\"dark\"}").unwrap();
    fs::write(userdata.join("history.db"), b"precious").unwrap();
    let outside_before: BTreeMap<_, _> = tree(&l.base).into_iter().filter(|(k, _)| !k.starts_with("appdata/")).collect();
    let user_before = tree(&userdata);
    let p = l.preview(HELLO, Action::Uninstall);
    assert!(p.content.effects.iter().any(|e| e.place == format!("data/{HELLO}/") && e.change == "read"));
    let r = l.confirm(&p);
    assert_eq!(r.outcome, "completed");
    assert!(r.receipt.unwrap().kept.contains(&"user_data".to_string()));
    assert_eq!(tree(&userdata), user_before, "the tool's own data is byte-identical after removal");
    let outside_after: BTreeMap<_, _> = tree(&l.base).into_iter().filter(|(k, _)| !k.starts_with("appdata/")).collect();
    assert_eq!(outside_before, outside_after);
    // nothing else in the data directory but the manager's own database and folder
    let top: Vec<String> = fs::read_dir(&l.data).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert!(top.iter().all(|n| n.starts_with("atm.sqlite") || n == "atm-managed"), "{top:?}");
}

#[test]
fn no_process_is_started_and_the_environment_is_unchanged_by_a_complete_workflow() {
    let cwd = std::env::current_dir().unwrap();
    let path = std::env::var_os("PATH");
    let mut vars: Vec<_> = std::env::vars_os().collect();
    vars.sort();
    let l = lab();
    assert_eq!(l.install().outcome, "completed");
    assert_eq!(l.remove().outcome, "completed");
    let _ = l.with(|c| l.svc.recover(c));
    assert_eq!(std::env::current_dir().unwrap(), cwd);
    assert_eq!(std::env::var_os("PATH"), path);
    let mut after: Vec<_> = std::env::vars_os().collect();
    after.sort();
    assert_eq!(vars, after);
    // the workflow modules contain no way to start a program, reach the network or edit the environment
    for (name, src) in [
        ("install_flow.rs", include_str!("install_flow.rs")),
        ("consent.rs", include_str!("consent.rs")),
        ("pilot_supply.rs", include_str!("pilot_supply.rs")),
    ] {
        let production = src.split("#[cfg(test)]").next().unwrap();
        for needle in [
            "std::process",
            "Command::new",
            "std::net",
            "TcpStream",
            "reqwest",
            "set_var",
            "remove_var",
            "std::fs",
            "ShellExecute",
            "tauri_plugin",
        ] {
            assert!(!production.contains(needle), "{name} contains {needle}");
        }
    }
}

#[test]
fn the_audit_trail_records_confirmation_and_outcome_with_the_schema_fields_and_stays_a_valid_chain() {
    let l = lab();
    assert_eq!(l.install().outcome, "completed");
    assert_eq!(l.remove().outcome, "completed");
    let events = l.events();
    let find = |t: &str, o: &str| events.iter().find(|e| e["eventType"] == t && e["outcome"] == o).cloned();
    let approve = find("approve", "passed").expect("approve passed");
    assert_eq!(approve["actorType"], "user");
    assert!(approve["approvedRecipeDigest"].as_str().unwrap().len() == 64 && approve["recipeId"].is_string());
    let install = find("install", "passed").expect("install passed");
    for key in ["recipeId", "approvalReference", "artifactDigest", "toolId", "toolVersion"] {
        assert!(install[key].is_string(), "{key}");
    }
    assert_eq!(install["toolId"], HELLO);
    assert!(uuid::Uuid::parse_str(install["approvalReference"].as_str().unwrap()).is_ok());
    assert!(find("uninstall", "passed").is_some());
    assert!(l.with(|c| audit::verify(c).unwrap()).ok);
    // every row uses only values the Phase 0 audit schema allows (a real run once wrote an unlisted verdict source; the enums are pinned here)
    let code_pattern = regex::Regex::new(r"^[A-Z0-9_]{1,64}$").unwrap();
    for e in &events {
        assert!(
            ["independent-harness", "operational-health", "target-self-report", "human-review", "ai-assisted", "not-applicable"]
                .contains(&e["verdictSource"].as_str().unwrap_or("")),
            "{e}"
        );
        assert!(["user", "manager", "adapter", "test-harness", "system"].contains(&e["actorType"].as_str().unwrap_or("")), "{e}");
        assert!(["passed", "failed", "blocked", "unknown", "skipped"].contains(&e["outcome"].as_str().unwrap_or("")), "{e}");
        if let Some(c) = e.get("errorCode").and_then(Value::as_str) {
            assert!(code_pattern.is_match(c), "{c}");
        }
    }
    // a recovery that found nothing to do writes no rollback event
    assert!(l.with(|c| l.svc.recover(c)).unwrap().notes.is_empty());
    assert!(!l.events().iter().any(|e| e["eventType"] == "rollback"));
    // no path, no account name, no secret anywhere in the audit rows
    let text = serde_json::to_string(&events).unwrap();
    assert!(!text.contains("Users") && !text.contains("appdata") && !text.contains(&l.data.to_string_lossy().to_string()));
}

// =====================================================================================================================================
// Notices: presented by the application, bound into the confirmed preview, never a file in the version folder
// =====================================================================================================================================

#[test]
fn notices_are_bound_into_the_confirmation_survive_install_removal_and_reinstall_and_never_become_a_sidecar() {
    let l = lab();
    let p = l.preview(HELLO, Action::Install);
    let n = p.content.notices.clone().expect("the preview carries the notice summary");
    assert_eq!(n.sha256.len(), 64);
    assert!(n.components.len() >= 3 && !n.unresolved.is_empty());
    assert_eq!(l.confirm(&p).outcome, "completed");
    // installed: the full text is shown, the version folder holds only the artifact
    let d = l.with(|c| l.svc.details(c, HELLO).unwrap());
    let full = d.notices.clone().unwrap();
    assert_eq!(full.summary.sha256, n.sha256);
    assert!(full.text.contains("SYNTHETIC NOTICE BUNDLE"));
    assert_eq!(fs::read_dir(artifact_path(&l).parent().unwrap()).unwrap().count(), 1, "no sidecar file in the version folder");
    let every_file: Vec<String> = tree(&l.root()).keys().cloned().collect();
    assert!(!every_file.iter().any(|f| f.to_lowercase().contains("notice") || f.to_lowercase().contains("license")), "{every_file:?}");
    // removal is not blocked by the notices (there is nothing foreign to block it), and they stay presentable afterwards
    assert_eq!(l.remove().outcome, "completed");
    let after = l.with(|c| l.svc.details(c, HELLO).unwrap());
    assert_eq!(after.notices.unwrap().summary.sha256, n.sha256);
    assert!(!l.root().join(format!("versions/{HELLO}")).exists());
    // reinstalling presents the very same notice (idempotent), and the audit row carries its digest
    let again = l.preview(HELLO, Action::Install);
    assert_eq!(again.content.notices.as_ref().unwrap().sha256, n.sha256);
    assert_eq!(l.confirm(&again).outcome, "completed");
    let rows: Vec<String> = l
        .events()
        .iter()
        .filter(|e| e["eventType"] == "install" && e["outcome"] == "passed")
        .filter_map(|e| e["evidenceDigest"].as_str().map(str::to_string))
        .collect();
    assert_eq!(rows, vec![n.sha256.clone(), n.sha256.clone()]);
}

#[test]
fn a_different_notice_text_is_a_different_confirmation() {
    let l1 = lab();
    let mut s = supply::sources();
    for n in s.notices.iter_mut().filter(|n| n.tool_id == HELLO) {
        n.text.push_str("an extra line\n");
    }
    let l2 = lab_with(s, None);
    let (a, b) = (l1.preview(HELLO, Action::Install), l2.preview(HELLO, Action::Install));
    assert_ne!(a.content.notices.unwrap().sha256, b.content.notices.unwrap().sha256);
    assert_ne!(a.preview_digest, b.preview_digest);
}

// =====================================================================================================================================
// The approval policy and the closed production paths
// =====================================================================================================================================

#[test]
fn the_in_app_policy_accepts_only_its_own_actor_and_the_production_policy_still_accepts_nothing() {
    use crate::approval::{Actor, ApprovalPolicy};
    let in_app = crate::consent::ConsentGrant::for_test(
        crate::consent::Binding {
            action: Action::Install,
            tool_id: HELLO.into(),
            version: VERSION.into(),
            platform_target: "windows-x64".into(),
            computer_id: "c".into(),
            recipe_digest: "a".repeat(64),
            artifact_sha256: "b".repeat(64),
            effects_digest: "c".repeat(64),
            plan_id: "d".repeat(64),
            preview_digest: "e".repeat(64),
            from_version: None,
        },
        t0(),
    );
    assert_eq!(in_app.actor().kind, "in_app_confirmation");
    // the synthetic test actor is not an in-app actor and the reverse
    let synthetic = Actor::synthetic("synthetic-user-1");
    assert_ne!(synthetic.kind, in_app.actor().kind);
    // a full approval cycle through the production policy refuses the in-app actor (and the synthetic one)
    let f = crate::synthetic::testkit::fx(HELLO, VERSION, b"payload");
    let s = crate::synthetic::testkit::situation(&f, Action::Install, None, "c", crate::synthetic::testkit::t0());
    let confirm = crate::synthetic::testkit::confirm(Action::Install, &f);
    for actor in [in_app.actor(), synthetic.clone()] {
        let r = approval::approve(&s, None, actor, &confirm, ApprovalPolicy::production(), 60);
        assert!(matches!(r, Err(approval::ApprovalError::ActorNotAllowed)), "{r:?}");
    }
    assert!(approval::approve(&s, None, synthetic, &confirm, ApprovalPolicy::in_app_confirmation(), 60).is_err());
    assert!(approval::approve(&s, None, in_app.actor(), &confirm, ApprovalPolicy::in_app_confirmation(), 60).is_ok());
}

#[test]
fn a_release_build_contains_no_acting_adapter_and_the_gated_pieces_are_only_compiled_for_tests_and_the_pilot_feature() {
    let src = include_str!("install_flow.rs");
    let production = src.split("#[cfg(test)]\n#[path").next().unwrap();
    // the only call that builds an acting adapter sits behind the cfg gate; the other arm returns the closed code
    assert_eq!(production.matches("Adapter::mvp_synthetic").count(), 1);
    let gate = "#[cfg(any(test, feature = \"synthetic-pilot\"))]\n    fn adapter<'a>(";
    let at = production.find(gate).expect("gated acting constructor");
    assert!(production[at..].find("Adapter::mvp_synthetic").unwrap() < 1200);
    assert!(production.contains("#[cfg(not(any(test, feature = \"synthetic-pilot\")))]\n    fn adapter<'a>("));
    assert!(production.contains("Err(\"production_adapter_not_enabled\")"));
    assert!(production.contains("#[cfg(any(test, feature = \"synthetic-pilot\"))]\n#[path = \"pilot_supply.rs\"]"));
    let adapter = include_str!("adapter.rs");
    assert!(adapter.contains("#[cfg(any(test, feature = \"synthetic-pilot\"))]\n    pub fn mvp_synthetic"));
    // the release supply is empty
    assert!(
        Sources::production().registry.len() == 0
            && Sources::production().extra_authenticated.is_empty()
            && Sources::production().notices.is_empty()
    );
    assert!(production.contains("pub fn production() -> Sources"));
    // the feature is off by default
    let cargo = include_str!("../Cargo.toml");
    assert!(cargo.contains("synthetic-pilot = []") && !cargo.contains("default = [\"synthetic-pilot\"]"));
}

// =====================================================================================================================================
// The boundary: what the WebView can reach, and what the workflow never touches
// =====================================================================================================================================

#[test]
fn the_webview_reaches_the_installation_engine_only_through_install_flow_and_never_with_a_path_a_url_or_bytes() {
    let commands = include_str!("commands.rs");
    let state = include_str!("state.rs");
    let lib = include_str!("lib.rs");
    // (1) the file-system layer, the managed root, the adapter, the path-based engine and every credential module stay out of reach of
    //     the command layer, the shared state and the start-up code; the ONLY mention of the engine is `install_flow::`
    for (name, text) in [("commands.rs", commands), ("state.rs", state), ("lib.rs", lib)] {
        let code: Vec<&str> = text.lines().filter(|l| !l.trim_start().starts_with("//") && !l.trim_start().starts_with("mod ")).collect();
        for needle in [
            "managed_root",
            "managed_fs",
            "AppDataAnchor",
            "ManagedRoot",
            "adapter::",
            "Adapter",
            "device_trust",
            "lifecycle::",
            "Engine {",
            "SandboxRoot",
            "os_approval",
            "os_hello",
            "approval_store",
            "ApprovalStore",
            "WindowsHello",
            "consent::",
            "ConsentGrant",
        ] {
            assert!(!code.iter().any(|l| l.contains(needle)), "{name} must not use {needle}");
        }
    }
    assert!(
        commands.contains("install_flow::")
            && state.contains("install_flow::InstallService")
            && lib.contains("install_flow::InstallService")
    );

    // (2) the six commands take only the three request types, the application state/handle, or an entry id: no path, URL, command, bytes
    //     or digest parameter exists in any of them
    let section_start = commands.find("pub fn list_install_states").expect("list_install_states");
    let section_end = commands.find("pub fn list_inventory").expect("list_inventory");
    let section = &commands[section_start..section_end];
    for needle in ["path", "url", "command", "bytes", "sha256", "digest", "destination", "script", "args", "env"] {
        let param_lines = section.lines().filter(|l| l.contains("pub fn ") || l.contains("pub async fn "));
        for l in param_lines {
            assert!(!l.to_lowercase().contains(&format!("{needle}:")), "a command parameter named {needle}: {l}");
        }
    }
    for l in section.lines().filter(|l| l.contains("pub fn ") || l.contains("pub async fn ")) {
        let params = l.split('(').nth(1).unwrap_or("").split(')').next().unwrap_or("").replace("State<'_, AppState>", "State<'_>");
        for p in params.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let ok = p.starts_with("state: State<'_>")
                || p.starts_with("app: AppHandle")
                || p.starts_with("request: crate::install_flow::")
                || p == "entry_id: String";
            assert!(ok, "unexpected parameter in an installation command: {p}");
        }
    }
    // (3) the request types refuse unknown fields (checked behaviourally in the forged-payload test; here: declared)
    let flow = include_str!("install_flow.rs");
    for ty in ["PrepareRequest", "ConfirmRequest", "CancelRequest"] {
        let at = flow.find(&format!("pub struct {ty}")).unwrap();
        assert!(flow[..at].lines().rev().take(3).any(|l| l.contains("deny_unknown_fields")), "{ty}");
    }
}

#[test]
fn the_workflow_modules_never_touch_windows_hello_the_approval_ledger_or_a_key_store() {
    for (name, src) in [
        ("install_flow.rs", include_str!("install_flow.rs")),
        ("consent.rs", include_str!("consent.rs")),
        ("pilot_supply.rs", include_str!("pilot_supply.rs")),
        ("commands.rs", include_str!("commands.rs")),
    ] {
        let production = src.split("#[cfg(test)]\n#[path").next().unwrap();
        for needle in [
            "ApprovalStore",
            "approval_store",
            "os_approval",
            "os_hello",
            "WindowsHello",
            "KeyCredential",
            "KeyStore",
            "device_trust",
            "provision",
            "sign_present",
            "cng_verify",
            "TrustPolicy",
        ] {
            assert!(!production.contains(needle), "{name} contains {needle}");
        }
    }
}

include!("install_flow_pilot_tests.rs");
