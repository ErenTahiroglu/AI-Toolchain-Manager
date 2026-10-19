// Phase 3D-3 tests, included at the end of install_flow_tests.rs (they share its lab). Fixture bytes only; no real tool, no network, no process.
//
//   (1) Issue #22: audit failures are never ignored: a mandatory pre-installation row stops the operation, a missing outcome row is reported as
//       `needs_attention` and later reconciled from manager-owned evidence, without ever touching a file or fabricating a record.
//   (2) The real-recipe transition (the jq candidate, with FIXTURE bytes) through the whole workflow and the retained-file provider.
//   (3) Post-operation read-back and filesystem inventories.

const JQ: &str = "jq";
const JQ_FILE: &str = "jq-windows-amd64.exe";
const CANDIDATE_JSON: &str = include_str!("../../docs/phase-3d3/jq-1.8.2-windows-x64.recipe.candidate.json");
const ATTESTATION_JSON: &str = include_str!("../../docs/phase-3d3/evidence/jq-1.8.2-windows-x64.source-attestation.json");
const LOCAL_USE_JSON: &str = include_str!("../../docs/phase-3d3/evidence/jq-1.8.2-windows-x64.local-use-license.json");

// ---- fixtures -------------------------------------------------------------------------------------------------------------------------------------

struct Pilot {
    sources: Sources,
    bytes: Vec<u8>,
    input: PathBuf,
}

/// The jq candidate with FIXTURE bytes: the same structure (real recipe, source attestation, local-use licence, notices, retained-file provider,
/// enablement), the file digest, size, format and platform replaced by those of made-up bytes so the checks can pass on any machine.
fn pilot_sources(input: &Path, payload: &[u8], enabled: bool, with_attestation: bool, with_license: bool) -> Pilot {
    let platform = HostFacts::detect().platform;
    let bytes = crate::synthetic::artifact_bytes(&platform, payload);
    let sha = crate::synthetic::sha256_of(&bytes);
    let mut rr: crate::recipe::ReviewedRecipe = serde_json::from_str(CANDIDATE_JSON).unwrap();
    rr.recipe.target.platform_target = platform.clone();
    rr.recipe.artifact.sha256 = sha.clone();
    rr.recipe.artifact.size_bytes = bytes.len() as u64;
    rr.recipe.artifact.format = crate::discovery::reference::native_executable_format(&platform).as_str().to_string();
    rr.recipe.steps[1]["expectedSha256"] = serde_json::json!(sha);
    let mut att: crate::attestation::SourceAttestation = serde_json::from_str(ATTESTATION_JSON).unwrap();
    att.platform_target = platform.clone();
    att.artifact.sha256 = sha.clone();
    att.artifact.size_bytes = bytes.len() as u64;
    att.artifact.format = rr.recipe.artifact.format.clone();
    let notice = NoticeBundle {
        tool_id: JQ.into(),
        version: rr.recipe.version.clone(),
        title: "FIXTURE notice bundle".into(),
        components: vec!["fixture-component - fixture licence".into()],
        unresolved: vec!["fixture: nothing here is a real legal statement".into()],
        text: "FIXTURE NOTICE TEXT\n".into(),
    };
    let mut lic: crate::attestation::LocalUseLicense = serde_json::from_str(LOCAL_USE_JSON).unwrap();
    lic.platform_target = platform.clone();
    lic.artifact_sha256 = sha.clone();
    lic.notice_bundle_sha256 = audit::sha256_hex(&notice.text);
    // an independent review of THIS content, with one independent staging check (test copy: the real record needs a real reviewer)
    rr.review.state = "reviewed".into();
    rr.review.reviewer_id = "fixture-reviewer".into();
    rr.review.reviewed_recipe_digest = crate::recipe::recipe_digest(&rr.recipe);
    rr.review.staging_evidence =
        vec![crate::recipe::StagingEvidence { check_id: "fixture-stage".into(), state: "passed".into(), independent: true, evidence_digest: "e".repeat(64) }];
    let gate = PilotRecipe {
        tool_id: JQ.into(),
        version: rr.recipe.version.clone(),
        platform_target: platform.clone(),
        recipe_digest: crate::recipe::recipe_digest(&rr.recipe),
        artifact_sha256: sha.clone(),
    };
    let provider = crate::artifact_provider::RetainedFileProvider::new(
        input.to_path_buf(),
        vec![crate::artifact_provider::RetainedEntry {
            tool_id: JQ.into(),
            version: rr.recipe.version.clone(),
            platform_target: platform.clone(),
            file_name: JQ_FILE.into(),
            size_bytes: bytes.len() as u64,
        }],
    );
    let reference = crate::synthetic::reference_for(JQ, &rr.recipe.version, &platform, &bytes);
    let mut s = supply::sources();
    s.registry = RecipeRegistry::for_tests(vec![rr]);
    s.extra_authenticated = vec![reference];
    s.artifacts = Box::new(provider);
    s.notices = vec![notice];
    s.evidence = crate::attestation::Evidence {
        attestations: if with_attestation { vec![att] } else { vec![] },
        local_use_licenses: if with_license { vec![lic] } else { vec![] },
    };
    s.pilot_gate = if enabled { vec![gate] } else { vec![] };
    Pilot { sources: s, bytes, input: input.to_path_buf() }
}

fn pilot_lab(p: Pilot) -> (Lab, Vec<u8>, PathBuf) {
    fs::write(p.input.join(JQ_FILE), &p.bytes).unwrap();
    let (bytes, input) = (p.bytes.clone(), p.input.clone());
    let l = lab_with(p.sources, None);
    l.with(|c| supply::seed_one(c, JQ, "jq (test row)").unwrap());
    (l, bytes, input)
}

fn input_dir() -> (tempfile::TempDir, PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let d = t.path().canonicalize().unwrap().join("pilot-input");
    fs::create_dir_all(&d).unwrap();
    (t, d)
}

fn jq_artifact(l: &Lab) -> PathBuf {
    l.root().join("versions/jq/1.8.2/artifact.bin")
}

fn receipts_json(l: &Lab) -> Vec<Value> {
    let dir = l.root().join("receipts");
    let mut v: Vec<Value> = fs::read_dir(dir).unwrap().flatten().filter_map(|e| serde_json::from_slice(&fs::read(e.path()).ok()?).ok()).collect();
    v.sort_by_key(|r| r["completedAt"].as_str().unwrap_or("").to_string());
    v
}

fn jq_install(l: &Lab) -> InstallResult {
    let p = l.preview(JQ, Action::Install);
    l.confirm(&p)
}

fn jq_remove(l: &Lab) -> InstallResult {
    let p = l.preview(JQ, Action::Uninstall);
    l.confirm(&p)
}

fn fail_audit_when(l: &Lab, event_type: &str, name: &str) {
    l.with(|c| {
        c.execute_batch(&format!(
            "CREATE TRIGGER {name} BEFORE INSERT ON audit_events WHEN json_extract(NEW.record_json, '$.eventType') = '{event_type}'
               AND json_extract(NEW.record_json, '$.outcome') = 'passed' BEGIN SELECT RAISE(ABORT, 'audit write refused'); END;"
        ))
        .unwrap()
    });
}

fn drop_trigger(l: &Lab, name: &str) {
    l.with(|c| c.execute_batch(&format!("DROP TRIGGER {name};")).unwrap());
}

fn audit_rows(l: &Lab, check: &str) -> Vec<Value> {
    l.events().into_iter().filter(|e| e["checkId"] == check).collect()
}

fn tree_without_database(base: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    tree(base).into_iter().filter(|(k, _)| !k.contains("atm.sqlite")).collect()
}

// =====================================================================================================================================
// (1) Issue #22: audit failures are never ignored
// =====================================================================================================================================

#[test]
fn a_mandatory_pre_installation_audit_failure_stops_everything_and_the_spent_consent_is_not_reusable() {
    let l = lab();
    let p = l.preview(HELLO, Action::Install);
    fail_audit_when(&l, "approve", "refuse_approve");
    let r = l.confirm(&p);
    assert_eq!((r.outcome.as_str(), code(&r)), ("refused", "audit_unavailable"), "{r:?}");
    assert!(!l.data.join("atm-managed").exists(), "not one filesystem change: the manager folder was not even created");
    assert!(!r.message.contains("tamamlandı"));
    // the confirmation was single-use: it is spent, and a repeat is refused for that reason
    assert_eq!(code(&l.confirm(&p)), "consent_already_used");
    assert_eq!(l.state(HELLO).status, Status::Available);
    // once the audit trail works a fresh preview and confirmation install normally
    drop_trigger(&l, "refuse_approve");
    let ok = l.install();
    assert_eq!(ok.outcome, "completed", "{ok:?}");
    assert!(ok.audit_recorded);
}

#[test]
fn an_unavailable_audit_table_stops_a_new_installation_and_is_reported_for_an_existing_one() {
    let l = lab();
    l.with(|c| c.execute_batch("ALTER TABLE audit_events RENAME TO audit_events_gone;").unwrap());
    let p = l.try_preview(HELLO, Action::Install);
    // a preview changes nothing and needs no audit row; the confirmation does
    let r = l.confirm(&p.unwrap());
    assert_eq!((r.outcome.as_str(), code(&r)), ("refused", "audit_unavailable"), "{r:?}");
    assert!(!l.data.join("atm-managed").exists());
    l.with(|c| c.execute_batch("ALTER TABLE audit_events_gone RENAME TO audit_events;").unwrap());
    assert_eq!(l.install().outcome, "completed");
    // the audit trail disappears AFTER an installation: the tool is reported as needing attention, nothing is touched, nothing new starts
    let snapshot = tree_without_database(&l.base);
    l.with(|c| c.execute_batch("ALTER TABLE audit_events RENAME TO audit_events_gone;").unwrap());
    let st = l.state(HELLO);
    assert_eq!((st.status, st.reasons[0].code.as_str()), (Status::FailedNeedsAttention, "audit_unavailable"), "{st:?}");
    assert_eq!(l.try_preview(HELLO, Action::Uninstall).unwrap_err().code, "INSTALL_NEEDS_ATTENTION");
    let rec = l.with(|c| l.svc.recover(c)).unwrap();
    assert!(rec.notes.iter().any(|n| n.contains("Denetim kaydı okunamıyor")), "{rec:?}");
    assert_eq!(tree_without_database(&l.base), snapshot, "nothing in the file system moved");
    l.with(|c| c.execute_batch("ALTER TABLE audit_events_gone RENAME TO audit_events;").unwrap());
    assert_eq!(l.state(HELLO).status, Status::InstalledVerified);
}

#[test]
fn a_failed_post_installation_audit_is_reported_as_needs_attention_with_the_true_state_and_never_as_a_plain_success() {
    let l = lab();
    let p = l.preview(HELLO, Action::Install);
    fail_audit_when(&l, "install", "refuse_install_row");
    let r = l.confirm(&p);
    assert_eq!(r.outcome, "needs_attention", "{r:?}");
    assert_eq!(code(&r), "audit_outcome_not_persisted");
    assert!(!r.audit_recorded);
    assert!(r.message.contains("YAPILDI") && r.message.contains("denetim kaydı yazılamadı"), "{}", r.message);
    assert!(!r.message.contains("Kurulum tamamlandı ve doğrulandı"), "never an ordinary fully audited success");
    // the truth of the file system, the journal and the receipt is intact
    assert_eq!(fs::read(artifact_path(&l)).unwrap(), expected_bytes());
    assert!(r.receipt.is_some() && receipts_json(&l).len() == 1);
    assert!(l.events().iter().any(|e| e["eventType"] == "approve" && e["outcome"] == "passed"), "the mandatory pre-installation row is there");
    assert!(!l.events().iter().any(|e| e["eventType"] == "install" && e["outcome"] == "passed"), "no outcome row was fabricated");
    // the state says so, and nothing new may start until it is reconciled
    let st = l.state(HELLO);
    assert_eq!((st.status, st.reasons[0].code.as_str()), (Status::FailedNeedsAttention, "audit_outcome_missing"), "{st:?}");
    assert_eq!(st.installed_version.as_deref(), Some(VERSION));
    assert!(!st.can_install && !st.can_remove);
    assert_eq!(l.try_preview(HELLO, Action::Uninstall).unwrap_err().code, "INSTALL_NEEDS_ATTENTION");
    assert_eq!(l.try_preview(HELLO, Action::Install).unwrap_err().code, "INSTALL_NEEDS_ATTENTION");
    // a repeated request is refused for the true reason
    assert_eq!(code(&l.confirm(&p)), "consent_already_used");
    // recovery WITH the audit trail still failing reconciles nothing and says so
    let snapshot = tree_without_database(&l.base);
    let rec = l.with(|c| l.svc.recover(c)).unwrap();
    assert_eq!((rec.audit_reconciled, rec.audit_unreconciled), (0, 1), "{rec:?}");
    assert_eq!(l.state(HELLO).status, Status::FailedNeedsAttention);
    // the audit trail works again: recovery writes the missing row from the receipt and the committed journal -- once
    drop_trigger(&l, "refuse_install_row");
    let rec = l.with(|c| l.svc.recover(c)).unwrap();
    assert_eq!((rec.audit_reconciled, rec.audit_unreconciled), (1, 0), "{rec:?}");
    let rows = audit_rows(&l, "install-audit-reconciled");
    assert_eq!(rows.len(), 1);
    let receipt = &receipts_json(&l)[0];
    assert_eq!(rows[0]["approvalReference"], receipt["approvalId"]);
    assert_eq!(rows[0]["artifactDigest"], receipt["artifactSha256"]);
    assert_eq!(rows[0]["eventType"], "install");
    assert_eq!(l.state(HELLO).status, Status::InstalledVerified);
    // recovery is idempotent and the whole sequence never touched a file
    let again = l.with(|c| l.svc.recover(c)).unwrap();
    assert_eq!((again.audit_reconciled, again.audit_unreconciled), (0, 0));
    assert_eq!(audit_rows(&l, "install-audit-reconciled").len(), 1);
    assert_eq!(tree_without_database(&l.base), snapshot);
    assert!(l.with(|c| audit::verify(c).unwrap()).ok, "the audit chain stays valid");
    // and the tool works normally afterwards
    assert_eq!(l.remove().outcome, "completed");
}

#[test]
fn a_failed_post_removal_audit_is_reconciled_the_same_way_and_a_restart_changes_nothing() {
    let l = lab();
    assert_eq!(l.install().outcome, "completed");
    let p = l.preview(HELLO, Action::Uninstall);
    fail_audit_when(&l, "uninstall", "refuse_uninstall_row");
    let r = l.confirm(&p);
    assert_eq!((r.outcome.as_str(), code(&r)), ("needs_attention", "audit_outcome_not_persisted"), "{r:?}");
    assert!(!artifact_path(&l).exists(), "the removal DID happen");
    assert_eq!(l.state(HELLO).status, Status::FailedNeedsAttention);
    // a restart (new process, no memory) sees the same truth and the same gap
    let svc = restart(&l, None);
    let st = l.with(|c| svc.state(c, HELLO).unwrap());
    assert_eq!((st.status, st.reasons[0].code.as_str()), (Status::FailedNeedsAttention, "audit_outcome_missing"));
    drop_trigger(&l, "refuse_uninstall_row");
    let rec = l.with(|c| svc.recover(c)).unwrap();
    assert_eq!((rec.audit_reconciled, rec.audit_unreconciled), (1, 0), "{rec:?}");
    assert_eq!(l.with(|c| svc.state(c, HELLO).unwrap()).status, Status::Removed);
    let rows = audit_rows(&l, "install-audit-reconciled");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["eventType"], "uninstall");
    // a second restart and a second recovery add nothing
    let svc2 = restart(&l, None);
    assert_eq!(l.with(|c| svc2.recover(c)).unwrap().audit_reconciled, 0);
    assert_eq!(audit_rows(&l, "install-audit-reconciled").len(), 1);
}

#[test]
fn reconciliation_needs_the_journal_too_and_never_touches_foreign_files_or_user_data() {
    let l = lab();
    let p = l.preview(HELLO, Action::Install);
    fail_audit_when(&l, "install", "refuse_install_row");
    assert_eq!(l.confirm(&p).outcome, "needs_attention");
    // foreign files and the tool's own data sit in the manager folder
    fs::create_dir_all(l.root().join(format!("data/{HELLO}"))).unwrap();
    fs::write(l.root().join(format!("data/{HELLO}/settings.json")), b"user data").unwrap();
    fs::write(l.root().join("receipts/their-note.txt"), b"foreign").unwrap();
    fs::write(l.root().join("snapshots/their-folder-file.txt"), b"foreign").unwrap();
    drop_trigger(&l, "refuse_install_row");
    // the journal of the transaction no longer vouches for it (its digest is broken): the receipt alone is not enough
    let txn = receipts_json(&l)[0]["txnId"].as_str().unwrap().to_string();
    let jpath = l.root().join(format!("journal/{txn}.json"));
    let good = fs::read_to_string(&jpath).unwrap();
    fs::write(&jpath, good.replace("\"committed\"", "\"started\"")).unwrap();
    let before = tree_without_database(&l.base);
    let rec = l.with(|c| l.svc.recover(c)).unwrap();
    assert_eq!(rec.audit_reconciled, 0, "{rec:?}");
    assert!(rec.audit_unreconciled >= 1 && rec.notes.iter().any(|n| n.contains("bağımsız olarak doğrulamıyor") || n.contains("bozuk") || !n.is_empty()), "{rec:?}");
    assert!(audit_rows(&l, "install-audit-reconciled").is_empty(), "no record is fabricated");
    assert_eq!(l.state(HELLO).status, Status::FailedNeedsAttention);
    // recovery may complain about the damaged journal but it deletes nothing of anyone's
    for (k, v) in &before {
        if k.contains("journal/") {
            continue;
        }
        assert_eq!(tree_without_database(&l.base).get(k), Some(v), "{k} was changed or removed");
    }
    // with the journal restored the evidence is whole again and the reconciliation works
    fs::write(&jpath, good).unwrap();
    let rec = l.with(|c| l.svc.recover(c)).unwrap();
    assert_eq!(rec.audit_reconciled, 1, "{rec:?}");
    assert_eq!(fs::read(l.root().join(format!("data/{HELLO}/settings.json"))).unwrap(), b"user data");
    assert_eq!(fs::read(l.root().join("receipts/their-note.txt")).unwrap(), b"foreign");
    assert_eq!(fs::read(l.root().join("snapshots/their-folder-file.txt")).unwrap(), b"foreign");
}

#[test]
fn a_failed_audit_row_for_a_cancelled_preview_or_a_refusal_is_reported_and_never_turns_into_success() {
    let l = lab();
    let p = l.preview(HELLO, Action::Install);
    l.with(|c| c.execute_batch("CREATE TRIGGER no_rows BEFORE INSERT ON audit_events BEGIN SELECT RAISE(ABORT, 'refused'); END;").unwrap());
    // withdrawing an offer changes nothing: it still succeeds even though its informational row cannot be written
    l.with(|c| l.svc.cancel(c, &CancelRequest { offer_id: p.offer_id.clone() })).unwrap();
    // a refused confirmation says that its own row is missing
    let r = l.confirm(&p);
    assert_eq!(r.outcome, "refused");
    assert!(!r.audit_recorded && r.message.contains("YAZILAMADI"), "{r:?}");
    assert!(!l.data.join("atm-managed").exists());
    drop_trigger(&l, "no_rows");
}

#[test]
fn the_state_is_read_back_after_an_operation_and_a_changed_result_is_never_reported_as_a_success() {
    // the installed file is replaced right before the journal commits (the window the platform leaves open): the read-back catches it
    let data_cell: Arc<Mutex<Option<PathBuf>>> = Arc::default();
    let d2 = data_cell.clone();
    let hook: FaultHook = Arc::new(move |p: Point| {
        if p == Point::BeforeJournalCommit {
            if let Some(data) = d2.lock().unwrap().clone() {
                let f = root_of(&data).join(format!("versions/{HELLO}/{VERSION}/artifact.bin"));
                let mut b = fs::read(&f).unwrap();
                *b.last_mut().unwrap() ^= 1;
                fs::write(&f, b).unwrap();
            }
        }
        None
    });
    let l = lab_with(supply::sources(), Some(hook));
    *data_cell.lock().unwrap() = Some(l.data.clone());
    let r = l.install();
    assert_eq!(r.outcome, "needs_attention", "{r:?}");
    assert_eq!(code(&r), "post_verification_failed");
    assert!(!r.message.contains("Kurulum tamamlandı ve doğrulandı"));
    // nothing was repaired, deleted or hidden: the changed file is still there and the tool is reported as drifted
    assert_ne!(fs::read(artifact_path(&l)).unwrap(), expected_bytes());
    assert_eq!(l.state(HELLO).status, Status::FailedNeedsAttention);
    let rows: Vec<Value> = l.events().into_iter().filter(|e| e["eventType"] == "install").collect();
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0]["outcome"].as_str(), rows[0]["errorCode"].as_str()), (Some("unknown"), Some("POST_VERIFICATION_FAILED")));
}

// =====================================================================================================================================
// (2) The real-recipe transition, with fixture bytes, through the retained-file provider
// =====================================================================================================================================

#[test]
fn the_real_candidate_structure_runs_the_whole_workflow_from_the_retained_file_and_the_retained_file_is_never_touched() {
    let (_keep, input) = input_dir();
    let (l, bytes, _) = pilot_lab(pilot_sources(&input, b"fixture payload of the pilot candidate", true, true, true));
    let source_before = (fs::read(input.join(JQ_FILE)).unwrap(), fs::metadata(input.join(JQ_FILE)).unwrap().modified().unwrap());
    let inventory_before = tree_without_database(&l.base);
    assert_eq!(l.state(JQ).status, Status::Available);
    let p = l.preview(JQ, Action::Install);
    let c = &p.content;
    assert_eq!((c.entry_id.as_str(), c.version.as_str()), (JQ, "1.8.2"));
    assert_eq!(c.license_state, "accepted_local_use");
    assert!(c.source.source_attested && !c.synthetic && !c.network_used);
    assert_eq!(c.artifact.sha256, crate::synthetic::sha256_of(&bytes));
    assert!(c.notices.is_some());
    assert_eq!(l.confirm(&p).outcome, "completed");
    // installed exactly as previewed, as a plain data file; the receipt says it is a REAL (non-synthetic) recipe
    assert_eq!(fs::read(jq_artifact(&l)).unwrap(), bytes);
    assert_eq!(receipts_json(&l)[0]["synthetic"], false);
    assert_eq!(l.state(JQ).status, Status::InstalledVerified);
    let d = l.with(|c| l.svc.details(c, JQ).unwrap());
    assert!(d.checks.iter().all(|c| c.passed) && d.notices.is_some());
    // the retained file is exactly as it was: same bytes, same modification time, nothing added next to it
    assert_eq!((fs::read(input.join(JQ_FILE)).unwrap(), fs::metadata(input.join(JQ_FILE)).unwrap().modified().unwrap()), source_before);
    assert_eq!(fs::read_dir(&input).unwrap().count(), 1);
    // removal and reinstall work; the retained file is still untouched
    assert_eq!(jq_remove(&l).outcome, "completed");
    assert!(!l.root().join("versions/jq").exists());
    assert_eq!(l.state(JQ).status, Status::Removed);
    assert_eq!(jq_install(&l).outcome, "completed");
    assert_eq!(jq_remove(&l).outcome, "completed");
    assert_eq!(fs::read(input.join(JQ_FILE)).unwrap(), source_before.0);
    // BEFORE/AFTER INVENTORY of the whole test tree: after install + removals only the manager's own journal and receipts are new
    let after = tree_without_database(&l.base);
    for (k, v) in &inventory_before {
        assert_eq!(after.get(k), Some(v), "{k} existed before and is gone or changed");
    }
    for k in after.keys().filter(|k| !inventory_before.contains_key(*k)) {
        assert!(k.starts_with("appdata/atm-managed/"), "{k} is new and outside the manager folder");
        assert!(!k.contains("versions/jq/") && !k.contains("state/jq") && !k.contains("staging/") || k.ends_with('/'), "{k}");
    }
    let leftovers: Vec<&String> = after.keys().filter(|k| !inventory_before.contains_key(*k) && !k.ends_with('/')).collect();
    assert!(leftovers.iter().all(|k| k.contains("/journal/") || k.contains("/receipts/")), "{leftovers:?}");
    // no process was started, nothing was put on a search path (covered by the environment test) and the executable was never run
    assert!(l.events().iter().filter(|e| e["eventType"] == "install" && e["outcome"] == "passed").count() == 2);
}

#[test]
fn a_real_recipe_is_not_applicable_in_a_build_that_was_not_enabled_for_exactly_that_recipe() {
    let (_keep, input) = input_dir();
    // no enablement (the release state)
    let (l, _, _) = pilot_lab(pilot_sources(&input, b"fixture one", false, true, true));
    let st = l.state(JQ);
    assert_eq!(st.status, Status::VerificationIncomplete);
    assert_eq!(st.reasons[0].code, "recipe_not_enabled");
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_NOT_AVAILABLE");
    assert!(!l.data.join("atm-managed").exists());
    // enablement for another recipe digest, another artifact digest, another version, another platform
    for tweak in 0..4 {
        let (_k2, input2) = input_dir();
        let mut p = pilot_sources(&input2, b"fixture two", true, true, true);
        match tweak {
            0 => p.sources.pilot_gate[0].recipe_digest = "0".repeat(64),
            1 => p.sources.pilot_gate[0].artifact_sha256 = "0".repeat(64),
            2 => p.sources.pilot_gate[0].version = "9.9.9".into(),
            _ => p.sources.pilot_gate[0].platform_target = "linux-x64".into(),
        }
        let (l2, _, _) = pilot_lab(p);
        assert_eq!(l2.state(JQ).status, Status::VerificationIncomplete, "tweak {tweak}");
        assert!(l2.try_preview(JQ, Action::Install).is_err());
        assert!(!l2.data.join("atm-managed").exists());
    }
}

#[test]
fn without_the_substantiating_evidence_the_real_recipe_stays_unavailable_and_says_why() {
    let (_k, input) = input_dir();
    let (l, _, _) = pilot_lab(pilot_sources(&input, b"fixture no attestation", true, false, true));
    let st = l.state(JQ);
    assert_eq!(st.status, Status::VerificationIncomplete);
    assert!(st.reasons.iter().any(|r| r.code == "source_attestation_unverifiable"), "{st:?}");
    let (_k, input) = input_dir();
    let (l, _, _) = pilot_lab(pilot_sources(&input, b"fixture no licence", true, true, false));
    let st = l.state(JQ);
    assert_eq!(st.status, Status::LicenseReviewRequired, "{st:?}");
    assert!(st.reasons.iter().any(|r| r.code == "license_not_accepted"));
    assert!(l.try_preview(JQ, Action::Install).is_err());
    // the licence covers one exact notice text: a different text in the application makes the recipe unavailable again
    let (_k, input) = input_dir();
    let mut p = pilot_sources(&input, b"fixture changed notice", true, true, true);
    p.sources.notices[0].text.push_str("one more line\n");
    let (l, _, _) = pilot_lab(p);
    let st = l.state(JQ);
    assert_eq!(st.status, Status::LicenseReviewRequired);
    assert_eq!(st.reasons[0].code, "notices_missing_or_changed");
    assert!(!l.data.join("atm-managed").exists());
}

#[test]
fn the_retained_file_provider_failures_reach_the_person_as_refusals_and_a_swapped_file_is_caught_before_the_first_change() {
    // missing
    let (_k, input) = input_dir();
    let p = pilot_sources(&input, b"fixture provider", true, true, true);
    let (l, bytes, input) = pilot_lab(p);
    fs::remove_file(input.join(JQ_FILE)).unwrap();
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_ARTIFACT_UNAVAILABLE");
    // right size, wrong content
    let mut wrong = bytes.clone();
    *wrong.last_mut().unwrap() ^= 1;
    fs::write(input.join(JQ_FILE), &wrong).unwrap();
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_ARTIFACT_REJECTED");
    // wrong size
    fs::write(input.join(JQ_FILE), &bytes[..bytes.len() - 1]).unwrap();
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_ARTIFACT_UNAVAILABLE");
    assert!(!l.data.join("atm-managed").exists());
    // a good preview, then the file is swapped for a same-size different file: the confirmation is refused and NOTHING is created
    fs::write(input.join(JQ_FILE), &bytes).unwrap();
    let p = l.preview(JQ, Action::Install);
    fs::write(input.join(JQ_FILE), &wrong).unwrap();
    let r = l.confirm(&p);
    assert_eq!(r.outcome, "refused", "{r:?}");
    assert_eq!(code(&r), "artifact_does_not_match_recipe");
    assert!(!l.data.join("atm-managed").exists());
    assert_eq!(fs::read(input.join(JQ_FILE)).unwrap(), wrong, "the file that was swapped in is left exactly as found");
}

#[test]
fn the_enablement_entry_matches_one_exact_recipe_only() {
    let (_k, input) = input_dir();
    let p = pilot_sources(&input, b"fixture enablement", true, true, true);
    let conn = db::open_memory().unwrap();
    let refs = p.sources.references(&conn).unwrap();
    let rr = p.sources.registry.find(JQ, &HostFacts::detect().platform, SURFACE, SCOPE)[0].clone();
    let valid = crate::recipe::validate_with(&rr, &refs, &p.sources.evidence).expect("the fixture variant validates");
    let g = &p.sources.pilot_gate[0];
    assert!(g.allows(&valid));
    for which in 0..5 {
        let mut x = g.clone();
        match which {
            0 => x.tool_id = "yq".into(),
            1 => x.version = "1.8.3".into(),
            2 => x.platform_target = "macos-arm64-x".into(),
            3 => x.recipe_digest = "1".repeat(64),
            _ => x.artifact_sha256 = "1".repeat(64),
        }
        assert!(!x.allows(&valid), "tweak {which}");
    }
    // a synthetic recipe is never admitted through an enablement entry
    let sc = crate::synthetic::Scenario::new(&crate::synthetic::testkit::base(&crate::synthetic::testkit::conn()), "synthetic-x", "1.0.0", HostFacts::detect().platform.as_str(), b"p");
    let sv = sc.validated();
    let entry = PilotRecipe {
        tool_id: "synthetic-x".into(),
        version: "1.0.0".into(),
        platform_target: HostFacts::detect().platform,
        recipe_digest: sv.digest().to_string(),
        artifact_sha256: sv.recipe().artifact.sha256.clone(),
    };
    assert!(!entry.allows(&sv));
}

#[test]
fn the_release_service_has_no_evidence_no_enablement_and_no_real_recipe() {
    let s = Sources::production();
    assert!(s.evidence.attestations.is_empty() && s.evidence.local_use_licenses.is_empty() && s.pilot_gate.is_empty());
    assert_eq!(s.registry.len(), 0);
    let src = include_str!("install_flow.rs");
    let production = src.split("#[cfg(test)]\n#[path").next().unwrap();
    assert!(production.contains("evidence: Evidence::production(),\n            pilot_gate: Vec::new(),"));
    // the retained-file provider is not constructed anywhere in the release supply
    for (name, text) in [("install_flow.rs", production), ("commands.rs", include_str!("commands.rs")), ("lib.rs", include_str!("lib.rs")), ("state.rs", include_str!("state.rs"))] {
        assert!(!text.contains("RetainedFileProvider"), "{name}");
    }
}
