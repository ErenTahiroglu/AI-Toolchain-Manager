// Phase 3D-4 (B2) tests, included at the end of install_flow_pilot_tests.rs (they share its lab and helpers).
//
// They exercise the REAL pilot wiring (`jq_pilot`: the committed recipe, B1 binding, reference, attestation, licence record, notices, catalog row,
// retained-file provider and activation entry) through the very same assembly code, with SYNTHETIC bytes only: a consistent copy of those files is
// re-pinned to the digest, size, format and platform of made-up bytes (`b2_with`). The real jq bytes are never read, copied, hashed or executed here;
// the only "real file" these tests touch is a zero-filled stand-in of the real size inside a temporary folder.
//
//   (1) the committed pieces fit together and the candidate recipe is unchanged;  (2) every tampered piece closes the supply with its own reason;
//   (3) the real bytes are LOCKED;  (4) the whole path on synthetic bytes;  (5) consent, concurrency, audit, filesystem safety;  (6) release stays closed.

use super::jq_pilot::Spec;
use serde_json::json;

const REFERENCE_ENTRY_JSON: &str = include_str!("../../docs/phase-3d3b/evidence/reference-entry.json");
const CANDIDATE_DIGEST_JSON: &str = include_str!("../../docs/phase-3d3b/evidence/candidate-recipe-digest.json");

fn json_of(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn real_artifact_sha() -> String {
    json_of(REFERENCE_ENTRY_JSON)["sha256"].as_str().unwrap().to_string()
}

fn real_recipe_digest() -> String {
    json_of(CANDIDATE_DIGEST_JSON)["recipeDigest"].as_str().unwrap().to_string()
}

fn edit_json(text: &mut String, f: impl FnOnce(&mut Value)) {
    let mut v: Value = serde_json::from_str(text).unwrap();
    f(&mut v);
    *text = serde_json::to_string_pretty(&v).unwrap();
}

/// Re-pins the binding to the (possibly modified) capture of the decision, as a deliberate test copy.
fn repin_decision(spec: &mut Spec) {
    let h = audit::sha256_hex(&spec.decision);
    edit_json(&mut spec.binding, |v| v["source"]["snapshotSha256"] = json!(h));
}

struct B2 {
    spec: Spec,
    /// The digest of the (synthetic-bytes) recipe of this copy.
    digest: String,
    bytes: Vec<u8>,
}

/// A consistent copy of every committed piece, re-pinned to made-up bytes. `mutate_recipe` / `mutate_binding` are applied BEFORE the digest is taken,
/// so a deliberate change of the recipe is still consistently bound (and has to be caught by the validator, not by the digest).
fn b2_with(payload: &[u8], mutate_recipe: &dyn Fn(&mut Value), mutate_binding: &dyn Fn(&mut Value)) -> B2 {
    let c = Spec::committed();
    let platform = HostFacts::detect().platform;
    let bytes = crate::synthetic::artifact_bytes(&platform, payload);
    let sha = crate::synthetic::sha256_of(&bytes);
    let format = crate::discovery::reference::native_executable_format(&platform).as_str().to_string();
    let (real_sha, real_digest) = (real_artifact_sha(), real_recipe_digest());
    let swap = |t: &str| -> String {
        t.replace(&real_sha, &sha)
            .replace("1035264", &bytes.len().to_string())
            .replace("\"platformTarget\": \"windows-x64\"", &format!("\"platformTarget\": \"{platform}\""))
            .replace("\"format\": \"pe-x64\"", &format!("\"format\": \"{format}\""))
    };
    let mut candidate = swap(&c.candidate);
    edit_json(&mut candidate, |v| mutate_recipe(&mut v["recipe"]));
    let reviewed: crate::recipe::ReviewedRecipe = serde_json::from_str(&candidate).unwrap();
    let digest = crate::recipe::recipe_digest(&reviewed.recipe);
    let decision = c.decision.replace(&real_sha, &sha).replace(&real_digest, &digest);
    let mut binding = swap(&c.binding).replace(&real_digest, &digest);
    edit_json(&mut binding, |v| {
        v["source"]["snapshotSha256"] = json!(audit::sha256_hex(&decision));
        mutate_binding(v);
    });
    let spec = Spec {
        candidate,
        binding,
        decision,
        attestation: swap(&c.attestation),
        license: swap(&c.license),
        reference: swap(&c.reference),
        notice_manifest: c.notice_manifest.clone(),
        notice_text: c.notice_text.clone(),
    };
    B2 { spec, digest, bytes }
}

fn b2(payload: &[u8]) -> B2 {
    b2_with(payload, &|_| {}, &|_| {})
}

/// Arms the one-installation allowance for the lab's data directory (the real build is armed only by the separately authorized B3 commit).
fn lab_armed(mut l: Lab) -> Lab {
    let dir = l.data.clone();
    l.svc.sources.one_time.as_mut().expect("the pilot supply carries the one-time rule").armed_data_dir_sha256 = Some(crate::pilot_allowance::data_dir_digest(&dir));
    l
}

fn b2_sources(b: &B2, input: &Path, unlocked: bool) -> Sources {
    jq_pilot::sources(&b.spec, input.to_path_buf(), if unlocked { Some(b.digest.as_str()) } else { None })
}

/// A lab running the jq pilot wiring on synthetic bytes, with the real catalog row seeded by the pilot's own seeding.
fn b2_lab(payload: &[u8]) -> (Lab, B2, tempfile::TempDir, PathBuf) {
    let (keep, input) = input_dir();
    let b = b2(payload);
    fs::write(input.join(JQ_FILE), &b.bytes).unwrap();
    let l = lab_armed(lab_with(b2_sources(&b, &input, true), None));
    l.with(|c| jq_pilot::seed_catalog(c).unwrap());
    (l, b, keep, input)
}

fn b2_restart(l: &Lab, b: &B2, input: &Path) -> InstallService {
    let c2 = l.clock.clone();
    let mut s = b2_sources(b, input, true);
    s.one_time.as_mut().unwrap().armed_data_dir_sha256 = Some(crate::pilot_allowance::data_dir_digest(&l.data));
    InstallService::with_sources(l.data.clone(), s, Arc::new(move || *c2.lock().unwrap()))
}

fn closed(s: &Sources) -> bool {
    s.registry.len() == 0 && s.extra_authenticated.is_empty() && s.pilot_gate.is_empty() && s.notices.is_empty() && s.one_time.is_none()
        && s.evidence.attestations.is_empty() && s.evidence.local_use_licenses.is_empty()
}

// =====================================================================================================================================
// (1) The committed pieces fit together; the candidate recipe is unchanged; the review block comes from the binding only
// =====================================================================================================================================

#[test]
fn b2_the_committed_files_assemble_into_the_exact_reviewed_pilot_and_the_candidate_recipe_is_unchanged() {
    let a = jq_pilot::assemble(&Spec::committed()).expect("the committed binding, recipe, records and notices fit together");
    let want = real_recipe_digest();
    assert_eq!(a.activation.recipe_digest, want);
    assert_eq!(want, "9c7a020d65d233e86577464a06c32b8f08fd4fd8403110e68fd551b674f7cca1", "the digest the independent reviewer accepted");
    // the recipe of the supply IS the candidate's recipe, field for field; only the review block differs (draft on disk, reviewed through the binding)
    let cand: crate::recipe::ReviewedRecipe = serde_json::from_str(&Spec::committed().candidate).unwrap();
    assert_eq!(a.reviewed.recipe, cand.recipe);
    assert_eq!(crate::recipe::recipe_digest(&cand.recipe), want);
    assert_eq!((cand.review.state.as_str(), cand.review.reviewer_id.as_str()), ("draft", "none-yet"), "the candidate file itself was not touched");
    // the review block: reviewed by the named reviewer over this digest, with the reviewer's own two published digests as the ONLY independent evidence
    let r = &a.reviewed.review;
    assert_eq!((r.state.as_str(), r.reviewed_recipe_digest.as_str()), ("reviewed", want.as_str()));
    assert_eq!(r.reviewer_id, "chatgpt-red-team-issue-23-c6299270819");
    assert_eq!(r.staging_evidence.len(), 2);
    assert!(r.staging_evidence.iter().all(|e| e.independent && e.state == "passed"));
    let digests: Vec<&str> = r.staging_evidence.iter().map(|e| e.evidence_digest.as_str()).collect();
    assert_eq!(
        digests,
        ["b553523ec91b75170f3b4f0c07fdab3e1fe125cf6a91338370c61e8b2440cf14", "98ad1f02a7be3f170d9ba6e0d4fdf6865fa6c5051b91a9a989bd26910916bddb"]
    );
    // the supply pieces
    assert_eq!((a.evidence.attestations.len(), a.evidence.local_use_licenses.len()), (1, 1));
    assert_eq!(a.evidence.local_use_licenses[0].decision, "LICENSE_PASS_LOCAL_USE");
    assert_eq!(a.reference.entry_id, "jq");
    assert_eq!(a.reference.sha256, real_artifact_sha());
    assert_eq!(a.activation.artifact_sha256, real_artifact_sha());
    assert_eq!((a.activation.tool_id.as_str(), a.activation.version.as_str(), a.activation.platform_target.as_str()), ("jq", "1.8.2", "windows-x64"));
    assert_eq!(a.retained.file_name, "jq-windows-amd64.exe");
    assert_eq!(a.retained.size_bytes, 1_035_264);
    assert_eq!(audit::sha256_hex(&a.notice.text), a.evidence.local_use_licenses[0].notice_bundle_sha256);
    assert_eq!(a.notice.components.len(), 7);
    let unresolved = a.notice.unresolved.join("\n");
    for q in ["Q1", "Q2", "Q3", "yeniden dağıtım", "DEĞİLDİR"] {
        assert!(unresolved.contains(q), "{q}: {unresolved}");
    }
    // the source of the review is the real, recorded one and the capture's own hash is pinned
    let binding: Value = json_of(&Spec::committed().binding);
    assert_eq!(binding["source"]["issue"], 23);
    assert_eq!(binding["source"]["commentId"], 6299270819u64);
    assert_eq!(audit::sha256_hex(&Spec::committed().decision), binding["source"]["snapshotSha256"].as_str().unwrap());
    assert_eq!(binding["supportingCi"]["independent"], false);
    assert!(binding["reviewer"]["signature"].as_str().unwrap().contains("none"), "no signature is claimed");
}

#[test]
fn b2_the_synthetic_copy_is_consistent_and_differs_from_the_real_pieces_only_in_what_the_bytes_decide() {
    let b = b2(b"b2 consistency");
    let a = jq_pilot::assemble(&b.spec).expect("the consistent synthetic copy assembles through the same code");
    assert_ne!(b.digest, real_recipe_digest());
    assert_ne!(a.reference.sha256, real_artifact_sha());
    assert_eq!(a.reference.sha256, crate::synthetic::sha256_of(&b.bytes));
    assert_eq!(a.reviewed.recipe.artifact.size_bytes, b.bytes.len() as u64);
    // everything that is not decided by the bytes is the committed recipe's: same tool, version, repository, commit, origin, permissions, effects
    let cand: crate::recipe::ReviewedRecipe = serde_json::from_str(&Spec::committed().candidate).unwrap();
    let (x, y) = (&a.reviewed.recipe, &cand.recipe);
    assert_eq!((&x.tool_id, &x.version, &x.source, &x.permissions, &x.filesystem_changes, &x.network, &x.lifecycle_actions, &x.rollback),
               (&y.tool_id, &y.version, &y.source, &y.permissions, &y.filesystem_changes, &y.network, &y.lifecycle_actions, &y.rollback));
    assert_eq!(x.artifact.origin, y.artifact.origin);
    assert!(x.network.is_empty() && !x.permissions.iter().any(|p| p == "network_download"));
}

// =====================================================================================================================================
// (2) Tampering: every changed piece closes the supply, each with its own reason
// =====================================================================================================================================

type Tamper = Box<dyn Fn(&mut Spec)>;
type TamperCase = (&'static str, Option<&'static str>, Tamper);
type ValueEdit = Box<dyn Fn(&mut Value)>;
type ValidatorCase = (&'static str, ValueEdit, ValueEdit);

fn tamper_binding(f: impl Fn(&mut Value) + 'static) -> Tamper {
    Box::new(move |s: &mut Spec| edit_json(&mut s.binding, |v| f(v)))
}

#[test]
fn b2_missing_or_modified_b1_review_evidence_and_tampered_pieces_close_the_supply_with_the_right_reason() {
    let base = b2(b"b2 tamper");
    assert!(jq_pilot::assemble(&base.spec).is_ok());
    let nine = "9".repeat(64);
    let cases: Vec<TamperCase> = vec![
        // ---- the B1 review evidence
        ("review block draft", Some("binding_review_block"), tamper_binding(|v| v["review"]["state"] = json!("draft"))),
        ("another reviewer", Some("binding_review_block"), tamper_binding(|v| v["review"]["reviewerId"] = json!("someone-else"))),
        ("review over another digest", Some("binding_review_block"), tamper_binding(|v| v["review"]["reviewedRecipeDigest"] = json!("0".repeat(64)))),
        ("evidence digest invented", Some("binding_evidence_not_the_reviewers"), tamper_binding(|v| v["review"]["stagingEvidence"][0]["evidenceDigest"] = json!("e".repeat(64)))),
        ("developer evidence added as independent", Some("binding_evidence_not_the_reviewers"), tamper_binding({
            let nine = nine.clone();
            move |v| v["review"]["stagingEvidence"].as_array_mut().unwrap().push(json!({ "checkId": "dev-made", "state": "passed", "independent": true, "evidenceDigest": nine }))
        })),
        ("evidence no longer independent", Some("binding_evidence_not_the_reviewers"), tamper_binding(|v| v["review"]["stagingEvidence"][1]["independent"] = json!(false))),
        ("evidence removed", Some("binding_evidence_incomplete"), tamper_binding(|v| { v["review"]["stagingEvidence"].as_array_mut().unwrap().pop(); })),
        ("CI declared independent", Some("binding_overclaims"), tamper_binding(|v| v["supportingCi"]["independent"] = json!(true))),
        ("Rust/frontend/GUI limitation dropped", Some("binding_overclaims"), tamper_binding(|v| v["notIndependentlyExecuted"] = json!([]))),
        ("source URL altered", Some("binding_source"), tamper_binding(|v| v["source"]["url"] = json!("https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/23#issuecomment-1"))),
        ("accepted digest not in the decision", Some("binding_not_in_decision"), tamper_binding({
            let nine = nine.clone();
            move |v| v["accepted"]["recipeDigest"] = json!(nine)
        })),
        ("capture changed after it was pinned", Some("binding_capture_changed"), Box::new(|s: &mut Spec| s.decision.push_str("one more line\n"))),
        ("capture without the B3 no-go", Some("binding_not_in_decision"), Box::new(|s: &mut Spec| {
            s.decision = s.decision.replace("B3 real installation NO-GO", "B3 real installation GO");
            repin_decision(s);
        })),
        ("capture without the limitation", Some("binding_not_in_decision"), Box::new(|s: &mut Spec| {
            s.decision = s.decision.replace("NOT independently run by ChatGPT", "run by ChatGPT");
            repin_decision(s);
        })),
        // ---- the candidate recipe (content changed after the review)
        ("recipe gained a network permission", Some("binding_recipe_differs"), Box::new(|s: &mut Spec| {
            edit_json(&mut s.candidate, |v| v["recipe"]["permissions"].as_array_mut().unwrap().push(json!("network_download")))
        })),
        ("recipe origin changed", Some("binding_recipe_differs"), Box::new(|s: &mut Spec| edit_json(&mut s.candidate, |v| v["recipe"]["artifact"]["origin"] = json!("remote_url")))),
        ("recipe revision changed", Some("binding_recipe_differs"), Box::new(|s: &mut Spec| edit_json(&mut s.candidate, |v| v["recipe"]["revision"] = json!(3)))),
        // ---- the reference
        ("reference digest differs", Some("reference_differs"), Box::new(|s: &mut Spec| edit_json(&mut s.reference, |v| v["sha256"] = json!("1".repeat(64))))),
        ("reference is an archive", Some("reference_differs"), Box::new(|s: &mut Spec| edit_json(&mut s.reference, |v| v["kind"] = json!("archive")))),
        ("reference for another tool", Some("reference_differs"), Box::new(|s: &mut Spec| edit_json(&mut s.reference, |v| v["entryId"] = json!("yq")))),
        // ---- the evidence records and the notices
        ("attestation for another commit", None, Box::new(|s: &mut Spec| edit_json(&mut s.attestation, |v| v["source"]["commit"] = json!("1".repeat(40))))),
        ("attestation verification incomplete", None, Box::new(|s: &mut Spec| edit_json(&mut s.attestation, |v| v["verification"]["officialVerifierPassed"] = json!(false)))),
        ("licence scope is redistribution", None, Box::new(|s: &mut Spec| edit_json(&mut s.license, |v| v["scope"] = json!("redistribution")))),
        ("licence decision is not the local-use pass", None, Box::new(|s: &mut Spec| edit_json(&mut s.license, |v| v["decision"] = json!("LICENSE_PASS")))),
        ("licence for another file", None, Box::new(|s: &mut Spec| edit_json(&mut s.license, |v| v["artifactSha256"] = json!("1".repeat(64))))),
        ("an open question blocks the local use", None, Box::new(|s: &mut Spec| edit_json(&mut s.license, |v| v["openQuestions"][0]["blocksLocalUse"] = json!(true)))),
        ("notice text changed", Some("notices_do_not_match_the_license"), Box::new(|s: &mut Spec| s.notice_text.push_str("an added line\n"))),
        ("notice manifest names another bundle hash", Some("notices_do_not_match_the_license"), Box::new(|s: &mut Spec| edit_json(&mut s.notice_manifest, |v| v["bundleSha256"] = json!("2".repeat(64))))),
        ("notices written into the tool folder", Some("notices_do_not_match_the_license"), Box::new(|s: &mut Spec| edit_json(&mut s.notice_manifest, |v| v["writtenIntoToolFolder"] = json!(true)))),
        ("a piece is unreadable", Some("candidate_unreadable"), Box::new(|s: &mut Spec| s.candidate = "{".to_string())),
    ];
    for (name, expected, tamper) in &cases {
        let mut spec = base.spec.clone();
        tamper(&mut spec);
        let got = jq_pilot::assemble(&spec).err();
        match expected {
            Some(code) => assert_eq!(got, Some(*code), "{name}"),
            None => assert!(got.is_some(), "{name}: must not assemble"),
        }
        // and whatever the reason, the supply that results is the EMPTY one: nothing is offered, enabled or readable
        let (_k, input) = input_dir();
        let s = jq_pilot::sources(&spec, input, Some(&base.digest));
        assert!(closed(&s), "{name}");
    }
}

#[test]
fn b2_wrong_tool_version_architecture_source_origin_and_network_permissions_are_refused_by_the_validator_even_when_consistently_bound() {
    // each change is re-bound (digest, capture, binding) so that the BINDING accepts it: the validator and the evidence records have to refuse it
    let cases: Vec<ValidatorCase> = vec![
        ("unsupported origin", Box::new(|r: &mut Value| r["artifact"]["origin"] = json!("remote_url")), Box::new(|v: &mut Value| v["accepted"]["origin"] = json!("remote_url"))),
        (
            "network destination and permission added",
            Box::new(|r: &mut Value| {
                r["network"] = json!(["github_release_download"]);
                r["permissions"].as_array_mut().unwrap().push(json!("network_download"));
            }),
            Box::new(|_: &mut Value| {}),
        ),
        ("another tool", Box::new(|r: &mut Value| r["toolId"] = json!("yq")), Box::new(|v: &mut Value| v["accepted"]["toolId"] = json!("yq"))),
        ("another version", Box::new(|r: &mut Value| r["version"] = json!("1.8.3")), Box::new(|v: &mut Value| v["accepted"]["version"] = json!("1.8.3"))),
        ("another file name", Box::new(|r: &mut Value| r["artifact"]["fileName"] = json!("jq-other.exe")), Box::new(|v: &mut Value| v["accepted"]["fileName"] = json!("jq-other.exe"))),
        (
            "an update action",
            Box::new(|r: &mut Value| r["lifecycleActions"] = json!(["install", "update", "uninstall"])),
            Box::new(|_: &mut Value| {}),
        ),
        ("another architecture", Box::new(|r: &mut Value| r["target"]["platformTarget"] = json!("windows-arm64")), Box::new(|v: &mut Value| v["accepted"]["platformTarget"] = json!("windows-arm64"))),
        ("an unreviewed rollback claim", Box::new(|r: &mut Value| r["rollback"]["state"] = json!("verified")), Box::new(|_: &mut Value| {})),
    ];
    for (name, edit, rebind) in cases {
        let b = b2_with(b"b2 validator", &*edit, &*rebind);
        let err = jq_pilot::assemble(&b.spec).err();
        assert!(err.is_some(), "{name}: a consistently re-bound but wrong recipe must still be refused");
        assert_ne!(err, Some("binding_recipe_differs"), "{name}: it got past the binding, so the validator or an evidence record refused it");
        let (_k, input) = input_dir();
        assert!(closed(&jq_pilot::sources(&b.spec, input, Some(&b.digest))), "{name}");
    }
}

// =====================================================================================================================================
// (3) The real bytes are LOCKED
// =====================================================================================================================================

#[test]
fn b2_the_unlock_is_one_constant_naming_the_exact_recipe_digest_and_it_is_off() {
    let d = real_recipe_digest();
    assert_eq!(jq_pilot::B3_UNLOCK, Some(d.as_str()), "the B3 activation candidate names exactly the reviewed recipe digest, nothing else");
    assert_eq!(jq_pilot::B3_ARMED_DATA_DIR_SHA256, None, "...and the one-installation allowance is still INACTIVE");
    assert!(!jq_pilot::unlocked_for(None, &d));
    assert!(!jq_pilot::unlocked_for(Some(&"0".repeat(64)), &d));
    assert!(!jq_pilot::unlocked_for(Some(""), &d));
    assert!(jq_pilot::unlocked_for(Some(&d), &d));
}

#[test]
fn b2_with_the_lock_on_a_perfect_file_is_neither_read_previewed_nor_installed() {
    let (_keep, input) = input_dir();
    let b = b2(b"b2 locked");
    fs::write(input.join(JQ_FILE), &b.bytes).unwrap();
    let source_before = (fs::read(input.join(JQ_FILE)).unwrap(), fs::metadata(input.join(JQ_FILE)).unwrap().modified().unwrap());
    // a build whose lock is on: the pieces are all valid and bound, the provider and the enablement list are not
    let s = b2_sources(&b, &input, false);
    assert_eq!((s.registry.len(), s.extra_authenticated.len(), s.pilot_gate.len(), s.notices.len()), (1, 1, 0, 1));
    assert_eq!(s.artifacts.bytes("jq", "1.8.2", &HostFacts::detect().platform), Err("real_install_locked"), "the provider refuses without touching the file");
    let l = lab_with(s, None);
    l.with(|c| jq_pilot::seed_catalog(c).unwrap());
    let st = l.state(JQ);
    assert_eq!((st.status, st.reasons[0].code.as_str()), (Status::VerificationIncomplete, "recipe_not_enabled"), "{st:?}");
    assert!(!st.can_install && !st.can_remove);
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_NOT_AVAILABLE");
    assert_eq!(l.try_preview(JQ, Action::Uninstall).unwrap_err().code, "INSTALL_NOT_AVAILABLE");
    let r = l.svc.confirm(&l.db, &ConfirmRequest { offer_id: "a".repeat(64), preview_digest: "b".repeat(64), acknowledged: true });
    assert_eq!(r.outcome, "refused");
    assert!(!l.data.join("atm-managed").exists(), "not even the manager's own folder was created");
    assert_eq!((fs::read(input.join(JQ_FILE)).unwrap(), fs::metadata(input.join(JQ_FILE)).unwrap().modified().unwrap()), source_before);
    // the adapter would refuse the recipe too: with a lock-on gate (empty) a real recipe is not a synthetic one
    let valid = {
        let a = jq_pilot::assemble(&b.spec).unwrap();
        let refs = ReferenceSet { authenticated: vec![a.reference.clone()], unauthenticated: vec![] };
        crate::recipe::validate_with(&a.reviewed, &refs, &a.evidence).unwrap()
    };
    assert!(!l.svc.sources().pilot_gate.iter().any(|g| g.allows(&valid)));
    assert!(!l.svc.sources().registry.find(JQ, &HostFacts::detect().platform, SURFACE, SCOPE).is_empty(), "visible, but not enabled");
}

#[test]
fn b2_the_real_committed_supply_is_windows_only_unlocked_for_the_candidate_but_inactive_and_creates_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join(jq_pilot::INPUT_FOLDER);
    let s = jq_pilot::sources_for_app(folder.clone());
    if cfg!(windows) {
        // the B3 activation CANDIDATE: unlocked (the enablement entry and the retained-file provider exist) but the one-time allowance is INACTIVE
        assert_eq!((s.registry.len(), s.extra_authenticated.len(), s.pilot_gate.len(), s.notices.len()), (1, 1, 1, 1), "pieces and the one enablement entry");
        assert_eq!(s.evidence.attestations.len() + s.evidence.local_use_licenses.len(), 2);
        let ot = s.one_time.as_ref().expect("the one-time rule is part of the pilot supply");
        assert_eq!(ot.armed_data_dir_sha256, None, "not armed for any data directory");
        assert_eq!((ot.recipe_digest.as_str(), s.pilot_gate[0].recipe_digest.as_str()), (real_recipe_digest().as_str(), real_recipe_digest().as_str()));
        // (the folder does not exist: nothing real can be read here, and nothing is created)
        assert_eq!(s.artifacts.bytes("jq", "1.8.2", "windows-x64"), Err("artifact_not_available"));
    } else {
        assert!(closed(&s), "off Windows the pilot supply is the empty release supply");
        assert_eq!(s.artifacts.bytes("jq", "1.8.2", "windows-x64"), Err("artifact_not_available"));
    }
    assert!(!folder.exists() && fs::read_dir(tmp.path()).unwrap().count() == 0, "building the supply creates no folder and reads no file");
    // a wrong digest unlocks nothing: with the real spec and a lock that names another digest the supply stays locked
    let (_k, input) = input_dir();
    let wrong = jq_pilot::sources(&Spec::committed(), input.clone(), Some(&"0".repeat(64)));
    assert!(wrong.pilot_gate.is_empty() && wrong.artifacts.bytes("jq", "1.8.2", "windows-x64") == Err("real_install_locked"));
}

#[test]
fn b2_an_unlocked_test_build_still_refuses_any_file_that_is_not_exactly_the_real_one() {
    if !cfg!(windows) {
        return; // the real recipe is for windows-x64 only; on another host it is not even applicable
    }
    // the REAL recipe, unlocked by the test through the very constant B3 would change; the folder holds stand-ins only
    let (_k, input) = input_dir();
    let unlocked = jq_pilot::sources(&Spec::committed(), input.clone(), Some(&real_recipe_digest()));
    assert_eq!(unlocked.pilot_gate.len(), 1);
    let l = lab_armed(lab_with(unlocked, None));
    l.with(|c| jq_pilot::seed_catalog(c).unwrap());
    assert_eq!(l.state(JQ).status, Status::Available);
    // no file, a file of the wrong size, a file of exactly the right size but other content
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_ARTIFACT_UNAVAILABLE");
    fs::write(input.join(JQ_FILE), b"a stand-in").unwrap();
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_ARTIFACT_UNAVAILABLE");
    fs::write(input.join(JQ_FILE), vec![0u8; 1_035_264]).unwrap();
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_ARTIFACT_REJECTED");
    assert!(!l.data.join("atm-managed").exists());
}

// =====================================================================================================================================
// (4) The whole path on synthetic bytes: catalog -> details -> preview -> consent -> result -> installed details -> removal
// =====================================================================================================================================

#[test]
fn b2_the_whole_pilot_path_runs_on_synthetic_bytes_and_the_preview_tells_the_truth() {
    let (l, b, _keep, input) = b2_lab(b"b2 fixture payload");
    let env_before: Vec<_> = std::env::vars_os().collect();
    let cwd_before = std::env::current_dir().unwrap();
    let inventory_before = tree_without_database(&l.base);
    let source_before = (fs::read(input.join(JQ_FILE)).unwrap(), fs::metadata(input.join(JQ_FILE)).unwrap().modified().unwrap());

    // catalog identity: the row's id is the recipe's tool id, and the row says only what is true of a catalog row
    let row = l.with(|c| catalog::get_entry(c, JQ).unwrap());
    assert_eq!((row.entry_id.as_str(), row.kind.as_str(), row.review_state.as_str()), (JQ, "executable_tool", "unreviewed"));
    assert_eq!((row.execution_policy.as_str(), row.license_state.as_str(), row.repo_url.as_str()), ("manual-only", "review-required", "https://github.com/jqlang/jq"));
    assert!(row.display_name.contains("1.8.2") && row.display_name.contains("pilot") && !row.record.to_string().contains(&crate::synthetic::sha256_of(&b.bytes)));
    assert_eq!((row.install_state.as_str(), row.verification_state.as_str()), ("not_managed", "unknown"));
    assert_eq!(l.state(JQ).status, Status::Available);

    // ---- the preview
    let p = l.preview(JQ, Action::Install);
    let c = &p.content;
    assert_eq!((c.entry_id.as_str(), c.version.as_str(), c.platform_target.as_str()), (JQ, "1.8.2", HostFacts::detect().platform.as_str()));
    assert_eq!(c.source.repository, "jqlang/jq");
    assert_eq!(c.source.immutable_commit, "34f7186b86743a083a589741b6cea95293524108");
    assert!(c.source.source_attested && !c.synthetic && !c.network_used);
    assert_eq!((c.artifact.file_name.as_str(), c.artifact.sha256.as_str(), c.artifact.digest_scope.as_str()), (JQ_FILE, crate::synthetic::sha256_of(&b.bytes).as_str(), "exact_file"));
    assert_eq!(c.artifact.size_bytes, b.bytes.len() as u64);
    assert_eq!(c.artifact_origin, "local_retained_file");
    for part in ["YEREL", "Hiçbir şey indirilmez", "ağ kullanılmaz", "yeniden doğrulanır"] {
        assert!(c.origin_text.contains(part), "{part}: {}", c.origin_text);
    }
    assert_eq!((c.license_state.as_str(), c.license_spdx.as_str()), ("accepted_local_use", "LicenseRef-jq-1.8.2-win-x64"));
    for part in ["YALNIZCA", "yerel", "DEĞİLDİR", "yeniden dağıtım"] {
        assert!(c.license_scope_text.contains(part), "{part}: {}", c.license_scope_text);
    }
    // the installed name and what the file is NOT
    for part in ["artifact.bin", JQ_FILE, "komut olmaz", "PATH", "hiçbir zaman çalıştırılmaz"] {
        assert!(c.installed_as_text.contains(part), "{part}: {}", c.installed_as_text);
    }
    // notices and the unresolved redistribution questions
    let n = c.notices.as_ref().expect("the notice bundle is part of the preview");
    assert_eq!(n.components.len(), 7);
    let unresolved = n.unresolved.join("\n");
    for q in ["Q1", "Q2", "Q3", "yeniden dağıtım"] {
        assert!(unresolved.contains(q), "{q}");
    }
    // every allowed filesystem effect: the declared set, all inside the manager's own folder, the installed file under its fixed name
    assert!(c.effects.iter().any(|e| e.place == "versions/jq/1.8.2/artifact.bin" && e.change == "create" && e.remains));
    assert!(c.effects.iter().any(|e| e.place == "state/jq.json" && e.change == "create" && e.remains));
    assert!(c.effects.iter().any(|e| e.place == "data/jq/" && e.change == "read"), "the tool's own data is read-only: not touched");
    assert!(c.effects.iter().all(|e| !e.place.contains(':') && !e.place.starts_with('/') && !e.place.contains("..") && !e.place.contains("PATH")), "{:?}", c.effects);
    // explicitly excluded operations
    let not_done = c.not_done.join("\n");
    for part in ["Hiçbir program çalıştırılmaz", "indirilmez", "PATH", "ajanın ayar dosyası", "Yönetici yetkisi", "Akıllı Uygulama Denetimi", "dışında hiçbir dosya"] {
        assert!(not_done.contains(part), "{part}");
    }
    // rollback and recovery expectations, and what the confirmation is NOT
    assert_eq!(c.rollback.reliability, "designed");
    assert!(c.rollback.text.contains("TASARLANMIŞ") && c.rollback.text.contains("dokunulmaz"));
    assert!(c.rollback.does_not_undo.iter().any(|x| x == "tool_user_data") && c.rollback.does_not_undo.iter().any(|x| x == "agent_configuration"));
    assert!(c.consent_statement.contains("kimlik kanıtı DEĞİLDİR") && c.consent_statement.contains("tek kullanımlıktır"), "{}", c.consent_statement);
    assert_eq!(c.recipe_digest, b.digest);
    // the new statements are part of what the confirmation is bound to
    for tweak in 0..3 {
        let mut x = p.content.clone();
        match tweak {
            0 => x.installed_as_text.push('x'),
            1 => x.license_scope_text.push('x'),
            _ => x.origin_text.push('x'),
        }
        assert_ne!(InstallService::preview_digest(&x), p.preview_digest, "tweak {tweak}");
    }
    assert!(!l.data.join("atm-managed").exists(), "a preview creates nothing");

    // ---- consent, result, installed details
    let r = l.confirm(&p);
    assert_eq!(r.outcome, "completed", "{r:?}");
    assert!(r.audit_recorded);
    assert_eq!(fs::read(jq_artifact(&l)).unwrap(), b.bytes, "installed exactly as previewed, as a plain stored file");
    assert_eq!(receipts_json(&l)[0]["synthetic"], false, "the receipt says it is a REAL (non-synthetic) recipe");
    assert_eq!(l.state(JQ).status, Status::InstalledVerified);
    let folder: Vec<String> = fs::read_dir(jq_artifact(&l).parent().unwrap()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(folder, vec!["artifact.bin".to_string()]);
    assert!(!l.root().join("versions/jq/1.8.2").join(JQ_FILE).exists(), "nothing is stored under the original, executable-looking name");
    let d = l.with(|c| l.svc.details(c, JQ).unwrap());
    assert_eq!(d.location.as_deref(), Some("versions/jq/1.8.2/artifact.bin"));
    assert!(d.checks.iter().all(|c| c.passed) && d.notices.is_some());
    assert_eq!(audit::sha256_hex(&d.notices.as_ref().unwrap().text), jq_pilot_notice_sha(&b));
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_ALREADY_INSTALLED");

    // ---- a restart (new process, no memory) sees the same truth, and the removal works from it
    let svc2 = b2_restart(&l, &b, &input);
    assert_eq!(l.with(|c| svc2.state(c, JQ).unwrap()).status, Status::InstalledVerified);
    let rp = l.with(|c| svc2.prepare(c, &PrepareRequest { entry_id: JQ.into(), action: Action::Uninstall })).unwrap();
    assert!(rp.content.effects.iter().any(|e| e.change == "remove" && e.place == "versions/jq/1.8.2/"));
    let rr = svc2.confirm(&l.db, &ConfirmRequest { offer_id: rp.offer_id, preview_digest: rp.preview_digest, acknowledged: true });
    assert_eq!(rr.outcome, "completed", "{rr:?}");
    assert!(!l.root().join("versions/jq").exists());
    assert_eq!(l.state(JQ).status, Status::Removed);
    // the ONE installation is spent: a removal did not reopen it, a restart does not, and no preview of a second installation can even be opened
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_PILOT_ALLOWANCE_CONSUMED");
    let again = l.state(JQ);
    assert_eq!((again.status, again.can_install, again.reasons[0].code.as_str()), (Status::Removed, false, "pilot_allowance_consumed"), "{again:?}");

    // ---- nothing outside the manager's folder changed; the retained file is exactly as it was; no environment, search path or directory moved
    assert_eq!((fs::read(input.join(JQ_FILE)).unwrap(), fs::metadata(input.join(JQ_FILE)).unwrap().modified().unwrap()), source_before);
    assert_eq!(fs::read_dir(&input).unwrap().count(), 1);
    let after = tree_without_database(&l.base);
    for (k, v) in &inventory_before {
        assert_eq!(after.get(k), Some(v), "{k} existed before and is gone or changed");
    }
    let leftovers: Vec<&String> = after.keys().filter(|k| !inventory_before.contains_key(*k) && !k.ends_with('/')).collect();
    assert!(leftovers.iter().all(|k| k.starts_with("appdata/atm-managed/") && (k.contains("/journal/") || k.contains("/receipts/"))), "{leftovers:?}");
    assert_eq!(fs::read(l.base.join("canary.txt")).unwrap(), b"untouched");
    assert_eq!(std::env::vars_os().collect::<Vec<_>>(), env_before);
    assert_eq!(std::env::current_dir().unwrap(), cwd_before);
    assert!(l.events().iter().filter(|e| e["eventType"] == "install" && e["outcome"] == "passed").count() == 1);
    assert!(l.with(|c| audit::verify(c).unwrap()).ok);
}

#[test]
fn b2_the_catalog_identity_is_the_recipes_tool_id_and_nothing_else_is_seeded() {
    let (l, _b, _k, _i) = b2_lab(b"b2 catalog");
    // the default catalog seed has no jq row; only the pilot's own seeding adds it, once, and a removal by the person is respected
    let fresh = db::open_memory().unwrap();
    catalog::seed_all(&fresh).unwrap();
    assert!(catalog::get_entry(&fresh, JQ).is_err(), "the release catalog does not contain jq");
    jq_pilot::seed_catalog(&fresh).unwrap();
    jq_pilot::seed_catalog(&fresh).unwrap();
    let n: i64 = fresh.query_row("SELECT count(*) FROM catalog_entries WHERE entry_id = 'jq'", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1, "idempotent");
    fresh.execute("UPDATE catalog_entries SET removed_at = '2027-01-01T00:00:00Z' WHERE entry_id = 'jq'", []).unwrap();
    jq_pilot::seed_catalog(&fresh).unwrap();
    assert!(catalog::get_entry(&fresh, JQ).unwrap().removed_at.is_some(), "not brought back");
    // a catalog row with another id never gets the recipe; a missing row is an error, not an installation
    l.with(|c| supply::seed_one(c, "jq-other", "jq other (test row)").unwrap());
    let st = l.state("jq-other");
    assert_eq!((st.status, st.reasons[0].code.as_str()), (Status::VerificationIncomplete, "no_reviewed_recipe"), "{st:?}");
    assert!(l.try_preview("jq-other", Action::Install).is_err());
    let (keep, input) = input_dir();
    let b = b2(b"b2 no row");
    fs::write(input.join(JQ_FILE), &b.bytes).unwrap();
    let no_row = lab_with(b2_sources(&b, &input, true), None);
    assert!(no_row.with(|c| no_row.svc.state(c, JQ)).is_err(), "no catalog row, no state");
    assert!(no_row.try_preview(JQ, Action::Install).is_err());
    assert!(!no_row.data.join("atm-managed").exists());
    drop(keep);
}

// =====================================================================================================================================
// (5) Consent, concurrency, audit, filesystem safety
// =====================================================================================================================================

#[test]
fn b2_consent_is_single_use_bound_to_the_preview_and_replay_or_a_stale_offer_is_refused() {
    let (l, _b, _k, _i) = b2_lab(b"b2 consent");
    // an offer opened first is superseded by a newer one; only the newest installs, once
    let p1 = l.preview(JQ, Action::Install);
    let p2 = l.preview(JQ, Action::Install);
    assert_eq!(code(&l.confirm(&p1)), "consent_superseded");
    assert!(!l.data.join("atm-managed").exists());
    let ok = l.confirm(&p2);
    assert_eq!(ok.outcome, "completed", "{ok:?}");
    assert_eq!(code(&l.confirm(&p2)), "consent_already_used", "replay");
    assert_eq!(receipts_json(&l).len(), 1);
    // a different digest, no acknowledgement, an unknown offer, an expired offer
    let p3 = l.preview(JQ, Action::Uninstall);
    let bad = l.svc.confirm(&l.db, &ConfirmRequest { offer_id: p3.offer_id.clone(), preview_digest: "0".repeat(64), acknowledged: true });
    assert_eq!((bad.outcome.as_str(), code(&bad)), ("refused", "consent_preview_changed"), "{bad:?}");
    let p4 = l.preview(JQ, Action::Uninstall);
    let no_ack = l.svc.confirm(&l.db, &ConfirmRequest { offer_id: p4.offer_id.clone(), preview_digest: p4.preview_digest.clone(), acknowledged: false });
    assert_eq!(code(&no_ack), "consent_not_acknowledged");
    assert!(jq_artifact(&l).exists(), "nothing was removed by a refused confirmation");
    let unknown = l.svc.confirm(&l.db, &ConfirmRequest { offer_id: "7".repeat(64), preview_digest: p4.preview_digest.clone(), acknowledged: true });
    assert_eq!(unknown.outcome, "refused");
    let p5 = l.preview(JQ, Action::Uninstall);
    l.advance(1_000);
    assert_eq!(code(&l.svc.confirm(&l.db, &ConfirmRequest { offer_id: p5.offer_id, preview_digest: p5.preview_digest, acknowledged: true })), "consent_expired");
    assert!(jq_artifact(&l).exists(), "nothing was removed by any refused confirmation");
}

#[test]
fn b2_concurrent_confirmations_of_one_preview_install_exactly_once() {
    let (l, b, _k, _i) = b2_lab(b"b2 concurrency");
    let l = Arc::new(l);
    let p = l.preview(JQ, Action::Install);
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let (l, p, bar) = (l.clone(), p.clone(), barrier.clone());
            std::thread::spawn(move || {
                bar.wait();
                l.confirm(&p)
            })
        })
        .collect();
    let results: Vec<InstallResult> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.outcome == "completed").count(), 1, "{results:?}");
    assert!(results.iter().filter(|r| r.outcome != "completed").all(|r| code(r) == "consent_already_used"));
    assert_eq!(fs::read(jq_artifact(&l)).unwrap(), b.bytes);
    assert_eq!(l.events().iter().filter(|e| e["eventType"] == "install" && e["outcome"] == "passed").count(), 1);
    assert_eq!(receipts_json(&l).len(), 1);
}

#[test]
fn b2_audit_write_failure_after_the_installation_is_needs_attention_with_the_true_state_and_recovery_reconciles_it() {
    let (l, b, _k, _i) = b2_lab(b"b2 audit");
    // the mandatory PRE-installation row: when it cannot be written nothing happens at all and the one-time allowance is NOT touched
    let p0 = l.preview(JQ, Action::Install);
    fail_audit_when(&l, "approve", "refuse_approve");
    let r0 = l.confirm(&p0);
    assert_eq!((r0.outcome.as_str(), code(&r0)), ("refused", "audit_unavailable"), "{r0:?}");
    assert!(!l.data.join("atm-managed").exists());
    assert_eq!(l.state(JQ).status, Status::Available, "the allowance was not consumed or reserved by a refused confirmation");
    drop_trigger(&l, "refuse_approve");
    let p = l.preview(JQ, Action::Install);
    fail_audit_when(&l, "install", "refuse_install_row");
    let r = l.confirm(&p);
    assert_eq!((r.outcome.as_str(), code(&r)), ("needs_attention", "audit_outcome_not_persisted"), "{r:?}");
    assert!(!r.audit_recorded && !r.message.contains("Kurulum tamamlandı ve doğrulandı"));
    assert_eq!(fs::read(jq_artifact(&l)).unwrap(), b.bytes, "the file system tells the truth");
    let st = l.state(JQ);
    assert_eq!((st.status, st.reasons[0].code.as_str()), (Status::FailedNeedsAttention, "audit_outcome_missing"), "{st:?}");
    assert_eq!(l.try_preview(JQ, Action::Uninstall).unwrap_err().code, "INSTALL_NEEDS_ATTENTION");
    drop_trigger(&l, "refuse_install_row");
    let rec = l.with(|c| l.svc.recover(c)).unwrap();
    assert_eq!((rec.audit_reconciled, rec.audit_unreconciled), (1, 0), "{rec:?}");
    assert_eq!(l.state(JQ).status, Status::InstalledVerified);
    // the installation was consumed even though its outcome row had to be reconstructed: removal, then no second installation
    assert_eq!(jq_remove(&l).outcome, "completed");
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_PILOT_ALLOWANCE_CONSUMED");
}

#[test]
fn b2_a_failure_at_any_point_of_the_install_is_rolled_back_and_a_clean_retry_installs() {
    for point in [Point::DuringStageWrite, Point::AfterStageWrite, Point::AfterVersionDirCreate, Point::AfterPointerSwitch, Point::BeforeReceiptWrite] {
        let (keep, input) = input_dir();
        let b = b2(b"b2 rollback");
        fs::write(input.join(JQ_FILE), &b.bytes).unwrap();
        let hook: FaultHook = Arc::new(move |p| (p == point).then_some(Fault::Fail));
        let l = lab_armed(lab_with(b2_sources(&b, &input, true), Some(hook)));
        l.with(|c| jq_pilot::seed_catalog(c).unwrap());
        let p = l.preview(JQ, Action::Install);
        let r = l.confirm(&p);
        assert_eq!(r.outcome, "failed", "{point:?}: {r:?}");
        assert_eq!(r.rollback.as_ref().unwrap().status, "restored", "{point:?}: {r:?}");
        assert_eq!(l.state(JQ).status, Status::Available, "{point:?}");
        assert!(!jq_artifact(&l).exists(), "{point:?}");
        let leftovers: Vec<String> = tree(&l.root())
            .into_keys()
            .filter(|k| !["data/", "journal/", "locks/", "receipts/", "snapshots/", "staging/", "state/", "versions/"].contains(&k.as_str()))
            .filter(|k| !k.starts_with("journal/"))
            .collect();
        assert!(leftovers.is_empty(), "{point:?}: {leftovers:?}");
        assert_eq!(fs::read(input.join(JQ_FILE)).unwrap(), b.bytes, "{point:?}: the retained file is untouched");
        assert_eq!(fs::read(l.base.join("canary.txt")).unwrap(), b"untouched");
        let healthy = b2_restart(&l, &b, &input);
        let p2 = l.with(|c| healthy.prepare(c, &PrepareRequest { entry_id: JQ.into(), action: Action::Install })).unwrap();
        let ok = healthy.confirm(&l.db, &ConfirmRequest { offer_id: p2.offer_id, preview_digest: p2.preview_digest, acknowledged: true });
        assert_eq!(ok.outcome, "completed", "{point:?}: {ok:?}");
        drop(keep);
    }
}

#[test]
fn b2_unexpected_files_links_and_changed_identity_fail_closed_are_reported_and_are_never_repaired_or_deleted() {
    let (l, b, _k, _i) = b2_lab(b"b2 filesystem");
    // an ownership conflict FIRST: someone else's folder sits where the installation needs its own. It fails closed, touches nothing of theirs, and --
    // because it provably changed nothing -- does not spend the one installation
    l.with(|c| drop(l.svc.open_or_establish(c, &InstallService::computer_id(c).unwrap()).unwrap()));
    let vdir = l.root().join("versions/jq/1.8.2");
    fs::create_dir_all(&vdir).unwrap();
    fs::write(vdir.join("their-notes.txt"), b"not ours").unwrap();
    let before = tree(&l.root().join("versions"));
    let r = jq_install(&l);
    assert_eq!((r.outcome.as_str(), code(&r)), ("failed", "version_folder_already_exists"), "{r:?}");
    assert_eq!(tree(&l.root().join("versions")), before, "their folder and file are byte-identical");
    assert_eq!(l.state(JQ).status, Status::Available, "a conflict that changed nothing does not spend the allowance");
    fs::remove_dir_all(l.root().join("versions/jq")).unwrap(); // the owner of that folder takes it away
    assert_eq!(jq_install(&l).outcome, "completed");
    let f = jq_artifact(&l);
    // an unexpected file next to the installed one
    fs::write(f.parent().unwrap().join("extra.txt"), b"x").unwrap();
    let st = l.state(JQ);
    assert_eq!((st.status, st.reasons[0].code.as_str()), (Status::FailedNeedsAttention, "installed_folder_has_unexpected_entries"), "{st:?}");
    assert!(l.try_preview(JQ, Action::Uninstall).is_err() && l.try_preview(JQ, Action::Install).is_err());
    assert_eq!(fs::read(f.parent().unwrap().join("extra.txt")).unwrap(), b"x", "their file is kept");
    fs::remove_file(f.parent().unwrap().join("extra.txt")).unwrap();
    // the installed file changed (same length, one bit)
    let mut tampered = b.bytes.clone();
    *tampered.last_mut().unwrap() ^= 1;
    fs::write(&f, &tampered).unwrap();
    assert_eq!(l.state(JQ).reasons[0].code, "installed_artifact_drift");
    assert_eq!(fs::read(&f).unwrap(), tampered, "not repaired, not replaced, not deleted");
    fs::write(&f, &b.bytes).unwrap();
    assert_eq!(l.state(JQ).status, Status::InstalledVerified);
    // an identity change that the bytes do not show: the pointer forged to another digest
    let pointer = l.root().join("state/jq.json");
    let good = fs::read_to_string(&pointer).unwrap();
    fs::write(&pointer, good.replace(&crate::synthetic::sha256_of(&b.bytes), &"0".repeat(64))).unwrap();
    assert_eq!(l.state(JQ).status, Status::FailedNeedsAttention);
    fs::write(&pointer, good).unwrap();
    // a junction/symlink where the tool's folder was: not followed, nothing written through it
    let outside = l.base.join("outside-dir");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), b"outside content").unwrap();
    let tool_dir = l.root().join("versions/jq");
    let moved = l.root().join("versions/moved-away");
    fs::rename(&tool_dir, &moved).unwrap();
    if make_dir_link(&tool_dir, &outside) {
        assert_eq!(l.state(JQ).status, Status::FailedNeedsAttention);
        assert!(l.try_preview(JQ, Action::Uninstall).is_err());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 1, "nothing was written through the link");
        remove_dir_link(&tool_dir);
    } else {
        eprintln!("directory links cannot be created here; case not exercised");
    }
    fs::rename(&moved, &tool_dir).unwrap();
    assert_eq!(l.state(JQ).status, Status::InstalledVerified, "once the owner puts things back the tool verifies again");
    // the removal is a separate, affirmative operation, and it does not reopen the installation
    assert_eq!(jq_remove(&l).outcome, "completed");
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_PILOT_ALLOWANCE_CONSUMED");
}

#[test]
fn b2_the_retained_file_provider_refuses_links_replaced_files_and_other_names_through_the_whole_service() {
    let (l, b, _k, input) = b2_lab(b"b2 provider");
    let target = input.join(JQ_FILE);
    // a second hard link to the same bytes
    fs::hard_link(&target, input.join("other-name.bin")).unwrap();
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_ARTIFACT_UNAVAILABLE");
    fs::remove_file(input.join("other-name.bin")).unwrap();
    // the file replaced by a symbolic link (needs a privilege on Windows: skipped, and said so, when it cannot be made)
    let real = input.join("real.bin");
    fs::write(&real, &b.bytes).unwrap();
    fs::remove_file(&target).unwrap();
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(&real, &target).is_ok();
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&real, &target).is_ok();
    if made {
        assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_ARTIFACT_UNAVAILABLE");
        fs::remove_file(&target).unwrap();
    } else {
        eprintln!("file symlinks need a privilege that is not available here; case not exercised");
    }
    // another file name is never looked at; a missing file, a shortened file and a same-size different file are refused
    fs::rename(&real, input.join("not-the-name.exe")).unwrap();
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_ARTIFACT_UNAVAILABLE");
    fs::write(&target, &b.bytes[..b.bytes.len() - 1]).unwrap();
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_ARTIFACT_UNAVAILABLE");
    let mut wrong = b.bytes.clone();
    *wrong.last_mut().unwrap() ^= 1;
    fs::write(&target, &wrong).unwrap();
    assert_eq!(l.try_preview(JQ, Action::Install).unwrap_err().code, "INSTALL_ARTIFACT_REJECTED");
    // a good preview, then the file is replaced: the confirmation is refused before the first change
    fs::write(&target, &b.bytes).unwrap();
    let p = l.preview(JQ, Action::Install);
    fs::write(&target, &wrong).unwrap();
    let r = l.confirm(&p);
    assert_eq!((r.outcome.as_str(), code(&r)), ("refused", "artifact_does_not_match_recipe"), "{r:?}");
    assert!(!l.data.join("atm-managed").exists());
    assert_eq!(fs::read(&target).unwrap(), wrong, "the replaced file is left exactly as found");
    assert!(input.join("not-the-name.exe").exists(), "files with other names are not touched either");
}

// =====================================================================================================================================
// (6) The release stays closed; the pilot configuration is narrow and has no shortcut
// =====================================================================================================================================

#[test]
fn b2_a_default_build_cannot_install_jq_and_the_pilot_build_only_shows_it_locked() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().canonicalize().unwrap().join("appdata");
    fs::create_dir_all(&data).unwrap();
    let conn = db::open_file(&data.join("atm.sqlite"), None).unwrap();
    catalog::seed_all(&conn).unwrap();
    jq_pilot::seed_catalog(&conn).unwrap();
    let svc = InstallService::for_app(data.clone());
    let st = svc.state(&conn, JQ).unwrap();
    assert_eq!(st.status, Status::VerificationIncomplete);
    assert!(!st.can_install && !st.can_remove);
    if cfg!(feature = "jq-pilot") {
        assert_eq!(st.reasons[0].code, "pilot_not_armed", "the candidate build shows jq, unlocked but INACTIVE");
    } else {
        assert_eq!(st.reasons[0].code, "no_reviewed_recipe", "an ordinary build, and the synthetic pilot, have no recipe for jq at all");
        if !cfg!(feature = "synthetic-pilot") {
            assert!(closed(svc.sources()), "the default build's supply is completely empty");
        }
    }
    assert_eq!(svc.prepare(&conn, &PrepareRequest { entry_id: JQ.into(), action: Action::Install }).unwrap_err().code, "INSTALL_NOT_AVAILABLE");
    assert!(!data.join("atm-managed").exists());
    // the release supply itself is empty whatever features the test build has
    let rel = Sources::production();
    assert!(closed(&rel) && rel.artifacts.bytes(JQ, "1.8.2", "windows-x64").is_err());
    // and the production adapter refuses a real installation outright
    assert_eq!(crate::recipe::BUILT_IN_RECIPES_JSON, "[]");
    assert!(crate::discovery::reference::BUILT_IN_AUTHENTICATED.is_empty());
    assert_eq!(crate::attestation::BUILT_IN_EVIDENCE_JSON, r#"{"attestations":[],"localUseLicenses":[]}"#);
}

#[test]
fn b2_the_pilot_module_has_no_environment_process_network_registry_or_unlock_shortcut_and_is_wired_in_exactly_one_place() {
    let src = include_str!("jq_pilot.rs");
    let code_only: String = src.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    for needle in [
        "std::env", "env::var", "var_os", "std::process", "Command::new", "std::net", "TcpStream", "reqwest", "winreg", "RegSetValue", "RegOpenKey", "set_var",
        "std::fs", "fs::write", "fs::copy", "fs::read", "allow_real", "unlock_real", "force_install", "getenv", ".claude",
    ] {
        assert!(!code_only.contains(needle), "jq_pilot.rs must not contain {needle}");
    }
    // the unlock constant is declared once, used once (to build the application's supply), and is None
    assert_eq!(code_only.matches(&format!("pub const B3_UNLOCK: Option<&str> = Some(\"{}\");", real_recipe_digest())).count(), 1);
    assert_eq!(code_only.lines().filter(|l| l.contains("B3_UNLOCK")).count(), 2, "declaration + the one use in sources_for_app");
    // no digest of the real artifact is a literal in this Rust source (the data files carry it), as the Phase 3D-0 guard requires
    assert!(!src.contains(&real_artifact_sha()[..8]));
    // wired in exactly one place, behind the feature
    let flow = include_str!("install_flow.rs");
    let production = flow.split("#[cfg(test)]\n#[path").next().unwrap();
    assert_eq!(production.matches("jq_pilot::sources_for_app").count(), 1);
    assert_eq!(production.matches("jq_pilot::seed_catalog").count(), 1);
    let at = production.find("jq_pilot::sources_for_app").unwrap();
    assert!(production[..at].rfind("#[cfg(feature = \"jq-pilot\")]").is_some_and(|p| at - p < 300));
    assert!(production.contains("#[cfg(any(test, feature = \"jq-pilot\"))]\n#[path = \"jq_pilot.rs\"]\npub mod jq_pilot;"));
    for (name, text) in [("commands.rs", include_str!("commands.rs")), ("state.rs", include_str!("state.rs")), ("lib.rs", include_str!("lib.rs"))] {
        assert!(!text.contains("jq_pilot::") && !text.contains("RetainedFileProvider"), "{name}: the interface reaches nothing of the pilot supply");
    }
    // the feature is off by default, Windows-only and never combined with the synthetic one
    let cargo = include_str!("../Cargo.toml");
    assert!(cargo.contains("\njq-pilot = []\n") && !cargo.contains("default ="));
    let lib = include_str!("lib.rs");
    assert!(lib.contains("#[cfg(all(feature = \"jq-pilot\", not(windows)))]\ncompile_error!"));
    assert!(lib.contains("#[cfg(all(feature = \"jq-pilot\", feature = \"synthetic-pilot\"))]\ncompile_error!"));
}

#[test]
fn b2_the_interface_cannot_name_a_path_a_digest_or_a_flag_for_the_pilot() {
    // requests carry identifiers and one acknowledgement only, and unknown fields are refused
    for bad in [
        r#"{"entryId":"jq","action":"install","path":"C:\\x\\jq.exe"}"#,
        r#"{"entryId":"jq","action":"install","allowRealInstall":true}"#,
        r#"{"entryId":"jq","action":"install","sha256":"00"}"#,
        r#"{"entryId":"jq","action":"install","unlock":"x"}"#,
    ] {
        assert!(serde_json::from_str::<PrepareRequest>(bad).is_err(), "{bad}");
    }
    assert!(serde_json::from_str::<ConfirmRequest>(r#"{"offerId":"a","previewDigest":"b","acknowledged":true,"force":true}"#).is_err());
    assert!(serde_json::from_str::<ConfirmRequest>(r#"{"offerId":"a","previewDigest":"b","acknowledged":true,"path":"x"}"#).is_err());
    assert!(serde_json::from_str::<CancelRequest>(r#"{"offerId":"a","extra":1}"#).is_err());
}

fn jq_pilot_notice_sha(b: &B2) -> String {
    audit::sha256_hex(&b.spec.notice_text)
}

// Phase 3D-5 (Issue #24): the one-installation allowance through the whole workflow
include!("install_flow_b3_tests.rs");
