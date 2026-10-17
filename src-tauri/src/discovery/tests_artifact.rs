//! Phase 2C: passive artifact fingerprints and the reference-digest trust contract. Nothing here executes a program; the
//! "references" are synthetic. A fingerprint is a property of bytes, never proof of origin: only an AUTHENTICATED
//! reference of an exact-file kind can verify an artifact, and the compiled-in authenticated list is empty.

use super::*;
use crate::discovery::{
    detect::CatalogPin,
    fsx::{self, HashLimits, HashOutcome, HashUnknown},
    reference::{self, ArtifactKind, MatchResult, ReferenceDigest, ReferenceSet, ReferenceState},
    types::{ArtifactAuth, FingerprintState, IdentityState, NotPerformed, RecordLink, VersionState, VersionTrust},
};
use sha2::{Digest, Sha256};
use std::sync::atomic::AtomicBool;

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn reference_for(entry: &str, kind: ArtifactKind, version: &str, bytes: &[u8]) -> ReferenceDigest {
    ReferenceDigest {
        entry_id: entry.to_string(),
        kind,
        format: if cfg!(windows) { "pe-exe" } else { "mach-o" }.to_string(),
        platform_target: platform().to_string(),
        version: version.to_string(),
        sha256: sha(bytes),
        source: "synthetic test fixture".to_string(),
    }
}

fn fp_auth() -> Authorization {
    Authorization { fingerprint_artifacts: true, ..all_auth(None) }
}

fn path_obs<'a>(obs: &'a [Observation], subject: &str) -> &'a Observation {
    find(obs, subject, Scope::Global, Method::PathExecutable)
}

fn write_tool(env: &Env, base: &str, bytes: &[u8]) {
    fs::write(env.bin.join(exe(base)), bytes).unwrap();
}

fn no_cancel() -> AtomicBool {
    AtomicBool::new(false)
}

// ---- hashing the opened handle ---------------------------------------------------------------------------------------

#[test]
fn the_digest_matches_published_test_vectors_and_an_independent_computation_over_many_chunks() {
    let env = Env::new();
    fs::write(env.root.join("abc"), b"abc").unwrap();
    fs::write(env.root.join("empty"), b"").unwrap();
    let big: Vec<u8> = (0..5 * 1024 * 1024 + 123).map(|i| (i % 251) as u8).collect();
    fs::write(env.root.join("big"), &big).unwrap();
    let small_chunks = HashLimits { chunk: 4096, ..HashLimits::default() };
    assert_eq!(
        fsx::hash_file(&env.root, &["abc"], HashLimits::default(), &no_cancel()),
        HashOutcome::Computed { sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(), bytes: 3 }
    );
    assert_eq!(
        fsx::hash_file(&env.root, &["empty"], HashLimits::default(), &no_cancel()),
        HashOutcome::Computed { sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(), bytes: 0 }
    );
    assert_eq!(
        fsx::hash_file(&env.root, &["big"], small_chunks, &no_cancel()),
        HashOutcome::Computed { sha256: sha(&big), bytes: big.len() as u64 }
    );
    assert_eq!(
        fsx::hash_file(&env.root, &["big"], HashLimits::default(), &no_cancel()),
        HashOutcome::Computed { sha256: sha(&big), bytes: big.len() as u64 },
        "the chunk size does not change the digest"
    );
}

#[test]
fn limits_cancellation_and_unusual_targets_are_unknown_never_verified() {
    let env = Env::new();
    fs::write(env.root.join("f"), vec![7u8; 100_000]).unwrap();
    fs::create_dir_all(env.root.join("a-dir")).unwrap();
    let h = |rel: &[&str], l: HashLimits, c: &AtomicBool| fsx::hash_file(&env.root, rel, l, c);
    assert_eq!(
        h(&["f"], HashLimits { max_bytes: 1000, ..HashLimits::default() }, &no_cancel()),
        HashOutcome::Unknown(HashUnknown::TooLarge)
    );
    assert_eq!(
        h(&["f"], HashLimits { timeout: std::time::Duration::ZERO, ..HashLimits::default() }, &no_cancel()),
        HashOutcome::Unknown(HashUnknown::TimedOut)
    );
    assert_eq!(h(&["f"], HashLimits::default(), &AtomicBool::new(true)), HashOutcome::Unknown(HashUnknown::Cancelled));
    assert_eq!(h(&["missing"], HashLimits::default(), &no_cancel()), HashOutcome::Unknown(HashUnknown::NotFound));
    assert_eq!(h(&["a-dir"], HashLimits::default(), &no_cancel()), HashOutcome::Unknown(HashUnknown::NotRegularFile));
    assert_eq!(
        h(&["..", "f"], HashLimits::default(), &no_cancel()),
        HashOutcome::Unknown(HashUnknown::Io),
        "traversal components are never accepted"
    );
    // exactly at the limit is fine
    assert!(matches!(h(&["f"], HashLimits { max_bytes: 100_000, ..HashLimits::default() }, &no_cancel()), HashOutcome::Computed { .. }));
    let codes: Vec<_> = [
        HashUnknown::NotFound,
        HashUnknown::Redirected,
        HashUnknown::NonLocal,
        HashUnknown::TooLarge,
        HashUnknown::TimedOut,
        HashUnknown::Cancelled,
        HashUnknown::ChangedWhileReading,
    ]
    .iter()
    .map(|u| u.code())
    .collect();
    assert!(codes.iter().all(|c| c.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')));
}

#[test]
fn a_file_modified_while_it_is_hashed_is_reported_and_no_digest_is_offered() {
    let env = Env::new();
    let target = env.root.join("moving");
    fs::write(&target, vec![1u8; 64 * 1024]).unwrap();
    let t = target.clone();
    let fired = std::rc::Rc::new(std::cell::Cell::new(false));
    let f = fired.clone();
    fsx::hook::set(move |stage| {
        if stage == "hash-chunk" && !f.get() {
            f.set(true);
            use std::io::Write;
            fs::OpenOptions::new().append(true).open(&t).unwrap().write_all(&[2u8; 4096]).unwrap();
        }
    });
    let out = fsx::hash_file(&env.root, &["moving"], HashLimits { chunk: 4096, ..HashLimits::default() }, &no_cancel());
    fsx::hook::clear();
    assert!(fired.get());
    assert_eq!(out, HashOutcome::Unknown(HashUnknown::ChangedWhileReading));
}

#[test]
fn the_digest_belongs_to_the_opened_handle_not_to_whatever_the_path_names_afterwards() {
    let env = Env::new();
    let target = env.root.join("tool");
    let original = vec![3u8; 64 * 1024];
    fs::write(&target, &original).unwrap();
    fs::write(env.root.join("replacement"), b"completely different content").unwrap();
    let (t, r, parked) = (target.clone(), env.root.join("replacement"), env.root.join("parked"));
    let swapped = std::rc::Rc::new(std::cell::Cell::new(false));
    let s = swapped.clone();
    fsx::hook::set(move |stage| {
        if stage == "hash-chunk" && !s.get() {
            s.set(true);
            fs::rename(&t, &parked).unwrap();
            fs::rename(&r, &t).unwrap();
        }
    });
    let out = fsx::hash_file(&env.root, &["tool"], HashLimits { chunk: 4096, ..HashLimits::default() }, &no_cancel());
    fsx::hook::clear();
    assert!(swapped.get());
    assert_eq!(
        out,
        HashOutcome::Computed { sha256: sha(&original), bytes: original.len() as u64 },
        "the path was swapped, but the bytes hashed are those of the file that was opened"
    );
}

#[test]
fn a_link_or_a_redirected_parent_is_never_hashed() {
    let env = Env::new();
    let real = env.root.join("real");
    fs::create_dir_all(&real).unwrap();
    fs::write(real.join("tool"), b"payload").unwrap();
    let link = env.root.join("lnk");
    #[cfg(windows)]
    super::make_junction(&link, &real);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert_eq!(fsx::hash_file(&link, &["tool"], HashLimits::default(), &no_cancel()), HashOutcome::Unknown(HashUnknown::Redirected));
    assert_eq!(
        fsx::hash_file(&env.root, &["lnk", "tool"], HashLimits::default(), &no_cancel()),
        HashOutcome::Unknown(HashUnknown::Redirected)
    );
    assert!(matches!(fsx::hash_file(&real, &["tool"], HashLimits::default(), &no_cancel()), HashOutcome::Computed { .. }), "control");
    #[cfg(windows)]
    super::remove_junction(&link);
    #[cfg(unix)]
    let _ = fs::remove_file(&link);
}

// ---- permission ----------------------------------------------------------------------------------------------------

#[test]
fn file_content_is_only_read_when_fingerprinting_was_allowed_and_requires_the_path_permission() {
    let env = Env::new();
    write_tool(&env, "gitleaks", b"content A");
    let plain = detect::detect_all(&env.roots(), &all_auth(None));
    let g = path_obs(&plain, "gitleaks");
    assert_eq!((g.provenance.artifact.fingerprint, g.provenance.artifact.sha256.clone()), (FingerprintState::NotRequested, None));
    assert!(g.provenance.not_performed.contains(&NotPerformed::ArtifactFingerprint));
    let with = detect::detect_all(&env.roots(), &fp_auth());
    let g = path_obs(&with, "gitleaks");
    assert_eq!(
        (g.provenance.artifact.fingerprint, g.provenance.artifact.sha256.clone()),
        (FingerprintState::Computed, Some(sha(b"content A")))
    );
    assert_eq!(g.provenance.artifact.bytes, Some(9));
    // the request layer: fingerprinting without the PATH permission is dropped, never silently widened
    let conn = seeded();
    let req = ScanRequest { path_executables: false, user_config: true, project: None, fingerprint_artifacts: true };
    assert!(!super::super::authorize(&conn, &req).unwrap().fingerprint_artifacts);
    let req = ScanRequest { path_executables: true, user_config: false, project: None, fingerprint_artifacts: true };
    assert!(super::super::authorize(&conn, &req).unwrap().fingerprint_artifacts);
}

// ---- the trust contract --------------------------------------------------------------------------------------------

#[test]
fn a_fingerprint_without_any_reference_stays_unknown_and_is_never_verified() {
    let env = Env::new();
    write_tool(&env, "gitleaks", b"whatever");
    let obs = detect::detect_all(&env.roots(), &fp_auth());
    let g = path_obs(&obs, "gitleaks");
    assert_eq!((g.provenance.artifact.match_result, g.provenance.artifact.reference), (MatchResult::NotApplicable, ReferenceState::None));
    assert_eq!(
        (g.provenance.identity, g.provenance.artifact_auth, g.identity_verified),
        (IdentityState::Unknown, ArtifactAuth::None, false)
    );
    assert!(g.provenance.not_performed.contains(&NotPerformed::AuthenticatedReference));
    assert!(g.limitations.iter().any(|l| l.contains("kaynağını veya meşruiyetini kanıtlamaz")));
    assert!(!g.functionality_tested);
}

#[test]
fn only_an_authenticated_exact_file_reference_verifies_and_it_also_supplies_a_validated_version() {
    let env = Env::new();
    write_tool(&env, "gitleaks", b"genuine build 2.0.0");
    let mut roots = env.roots();
    roots.references = ReferenceSet {
        authenticated: vec![
            reference_for("gitleaks", ArtifactKind::Executable, "1.0.0", b"genuine build 1.0.0"),
            reference_for("gitleaks", ArtifactKind::Executable, "2.0.0", b"genuine build 2.0.0"),
        ],
        unauthenticated: vec![],
    };
    let obs = detect::detect_all(&roots, &fp_auth());
    let g = path_obs(&obs, "gitleaks");
    assert_eq!(
        (g.provenance.identity, g.provenance.artifact_auth, g.identity_verified),
        (IdentityState::Validated, ArtifactAuth::DigestMatch, true)
    );
    assert_eq!(
        (g.version.as_deref(), g.version_source.as_deref()),
        (Some("2.0.0"), Some("yetkili referans özeti")),
        "same tool, different versions: the matching one is reported"
    );
    assert_eq!((g.provenance.version, g.provenance.version_trust), (VersionState::Known, VersionTrust::Validated));
    assert_eq!(g.provenance.artifact.match_result, MatchResult::MatchesAuthenticated);
    assert!(!g.functionality_tested, "verified identity is not a functional test");
    assert!(!g.provenance.install_record, "no installer record was needed or invented");
}

#[test]
fn matching_filenames_with_different_contents_are_told_apart() {
    let genuine: &[u8] = b"genuine";
    let roots_for = |bytes: &[u8]| {
        let env = Env::new();
        write_tool(&env, "gitleaks", bytes);
        let mut roots = env.roots();
        roots.references = ReferenceSet {
            authenticated: vec![reference_for("gitleaks", ArtifactKind::Executable, "1.0.0", genuine)],
            unauthenticated: vec![],
        };
        let obs = detect::detect_all(&roots, &fp_auth());
        let g = path_obs(&obs, "gitleaks").clone();
        drop(env);
        g
    };
    let real = roots_for(genuine);
    let fake = roots_for(b"same name, other bytes");
    assert_eq!((real.provenance.identity, real.identity_verified), (IdentityState::Validated, true));
    assert_eq!(
        (fake.provenance.identity, fake.identity_verified, fake.provenance.artifact.match_result),
        (IdentityState::Conflicted, false, MatchResult::MismatchesAuthenticated)
    );
    assert_ne!(real.provenance.artifact.sha256, fake.provenance.artifact.sha256);
    assert_eq!(fake.version, None);
}

#[test]
fn a_matching_digest_from_an_untrusted_source_is_evidence_of_a_claim_not_verification() {
    // README / package.json / user-typed / database values: even when they equal the local hash.
    let env = Env::new();
    write_tool(&env, "gitleaks", b"claimed build");
    let mut roots = env.roots();
    roots.references = ReferenceSet {
        authenticated: vec![],
        unauthenticated: vec![reference_for("gitleaks", ArtifactKind::Executable, "9.9.9", b"claimed build")],
    };
    let obs = detect::detect_all(&roots, &fp_auth());
    let g = path_obs(&obs, "gitleaks");
    assert_eq!(
        (g.provenance.artifact.reference, g.provenance.artifact.match_result),
        (ReferenceState::Unauthenticated, MatchResult::MatchesUnauthenticated)
    );
    assert_eq!(
        (g.provenance.identity, g.identity_verified, g.version.clone()),
        (IdentityState::Unknown, false, None),
        "a claim that matches is recorded as such (artifact.matchResult) but does not lift identity"
    );
    assert!(g.limitations.iter().any(|l| l.contains("DOĞRULANMAMIŞ")));
    // and a mismatch with an unauthenticated reference concludes nothing
    roots.references.unauthenticated = vec![reference_for("gitleaks", ArtifactKind::Executable, "9.9.9", b"something else")];
    let obs = detect::detect_all(&roots, &fp_auth());
    let g = path_obs(&obs, "gitleaks");
    assert_eq!(
        (g.provenance.artifact.match_result, g.provenance.identity),
        (MatchResult::MismatchesUnauthenticated, IdentityState::Unknown)
    );
}

#[test]
fn an_archive_installer_or_package_digest_never_authenticates_the_extracted_file_even_when_equal() {
    for kind in [ArtifactKind::Archive, ArtifactKind::Installer, ArtifactKind::Package] {
        let env = Env::new();
        write_tool(&env, "gitleaks", b"bytes");
        let mut roots = env.roots();
        roots.references =
            ReferenceSet { authenticated: vec![reference_for("gitleaks", kind, "1.0.0", b"bytes")], unauthenticated: vec![] };
        let obs = detect::detect_all(&roots, &fp_auth());
        let g = path_obs(&obs, "gitleaks");
        assert_eq!(g.provenance.artifact.match_result, MatchResult::Incomparable, "{kind:?}");
        assert!(!g.identity_verified && g.provenance.identity != IdentityState::Validated, "{kind:?}");
    }
}

#[test]
fn references_for_another_platform_or_another_tool_do_not_apply() {
    let env = Env::new();
    write_tool(&env, "gitleaks", b"bytes");
    let other_platform = if cfg!(windows) { "macos-arm64" } else { "windows-x64" };
    let mut wrong_platform = reference_for("gitleaks", ArtifactKind::Executable, "1.0.0", b"bytes");
    wrong_platform.platform_target = other_platform.to_string();
    let wrong_tool = reference_for("strix", ArtifactKind::Executable, "1.0.0", b"bytes");
    let mut roots = env.roots();
    roots.references = ReferenceSet { authenticated: vec![wrong_platform, wrong_tool], unauthenticated: vec![] };
    let obs = detect::detect_all(&roots, &fp_auth());
    let g = path_obs(&obs, "gitleaks");
    assert_eq!(
        (g.provenance.artifact.match_result, g.identity_verified),
        (MatchResult::NotApplicable, false),
        "identical bytes, but the reference is for another OS / another tool"
    );
}

#[test]
fn the_production_reference_set_is_empty_so_no_real_tool_can_be_verified_in_this_phase() {
    let env = Env::new();
    write_tool(&env, "gitleaks", b"anything");
    let conn = seeded();
    crate::discovery::run_scan(&conn, &env.roots(), &fp_auth()).unwrap();
    let inv = store::inventory(&conn).unwrap();
    assert!(inv.observations.iter().all(|o| !o.identity_verified && o.ladder.identity != IdentityState::Validated));
    let g = inv.observations.iter().find(|o| o.subject == "gitleaks" && o.method == "path-executable").unwrap();
    assert_eq!(g.ladder.artifact.fingerprint, FingerprintState::Computed);
    assert!(g.scan_authorized_fingerprint);
}

#[test]
fn a_symlinked_program_file_is_not_hashed_and_a_wrapper_with_a_package_record_is_not_linked() {
    let env = Env::new();
    // wrapper / shim: an npm package record exists next to the shim, but nothing verifiable ties the two together
    env.fake_exe("agent-browser");
    let json = serde_json::json!({ "name": "agent-browser", "version": "0.39.1", "repository": "vercel-labs/agent-browser" }).to_string();
    let p = if cfg!(windows) {
        env.bin.join("node_modules/agent-browser/package.json")
    } else {
        env.root.join("lib/node_modules/agent-browser/package.json")
    };
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, json).unwrap();
    let mut roots = env.roots();
    roots.pins = vec![CatalogPin::from_repo_url("agent-browser", "https://github.com/vercel-labs/agent-browser").unwrap()];
    let obs = detect::detect_all(&roots, &fp_auth());
    let a = path_obs(&obs, "agent-browser");
    assert!(a.provenance.install_record);
    assert_eq!(a.provenance.artifact.record_link, RecordLink::Unverifiable, "a nearby package.json is not a link to this file");
    assert_eq!((a.provenance.identity, a.identity_verified), (IdentityState::EvidenceAvailable, false));
    assert_eq!(a.provenance.artifact.fingerprint, FingerprintState::Computed, "the shim file itself is fingerprinted");
    #[cfg(unix)]
    {
        let env2 = Env::new();
        let real = env2.root.join("real-tool");
        fs::write(&real, b"x").unwrap();
        std::os::unix::fs::symlink(&real, env2.bin.join("gitleaks")).unwrap();
        let obs = detect::detect_all(&env2.roots(), &fp_auth());
        let g = path_obs(&obs, "gitleaks");
        assert_eq!(
            (g.provenance.artifact.fingerprint, g.provenance.artifact.unknown_reason.as_deref()),
            (FingerprintState::Unknown, Some("redirected"))
        );
        assert!(g.provenance.artifact.sha256.is_none());
    }
}

#[test]
fn a_plugin_directory_has_no_digest_and_is_linked_to_its_record_only_by_folder_name() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", "1.0.0")]));
    cache(&env, "claude-plugins-official", "frontend-design", "1.0.0");
    let mut roots = env.roots();
    // even an authenticated reference for the plugin entry cannot verify a directory
    roots.references = ReferenceSet {
        authenticated: vec![reference_for(FD, ArtifactKind::PluginFile, "1.0.0", b"one file of the plugin")],
        unauthenticated: vec![],
    };
    let obs = detect::detect_all(&roots, &fp_auth());
    let p = plugin(&obs, FD, Scope::Global);
    assert_eq!(p.provenance.identity, IdentityState::RecordCorroborated);
    assert_eq!(p.provenance.artifact.record_link, RecordLink::FolderByName);
    assert_eq!(p.provenance.artifact.fingerprint, FingerprintState::NotRequested);
    assert!(p.provenance.not_performed.contains(&NotPerformed::PluginDirectoryDigest));
    assert!(!p.identity_verified);
}

// ---- persistence, invalidation, tampering -------------------------------------------------------------------------

fn authenticated_roots(env: &Env, bytes: &[u8]) -> Roots {
    let mut roots = env.roots();
    roots.references =
        ReferenceSet { authenticated: vec![reference_for("gitleaks", ArtifactKind::Executable, "1.0.0", bytes)], unauthenticated: vec![] };
    roots
}

fn stored_gitleaks(conn: &Connection, set: &ReferenceSet) -> store::StoredObservation {
    store::latest_observations_with(conn, set)
        .unwrap()
        .into_iter()
        .find(|o| o.subject == "gitleaks" && o.method == "path-executable")
        .unwrap()
}

#[test]
fn a_verified_observation_is_persisted_current_and_then_loses_its_standing_when_anything_underneath_changes() {
    let env = Env::new();
    write_tool(&env, "gitleaks", b"genuine");
    let roots = authenticated_roots(&env, b"genuine");
    let conn = seeded();
    crate::discovery::run_scan(&conn, &roots, &fp_auth()).unwrap();
    let set = roots.references.clone();
    let o = stored_gitleaks(&conn, &set);
    assert_eq!((o.ladder.identity, o.identity_verified, o.current), (IdentityState::Validated, true, true));
    assert_eq!(o.ladder.artifact.reference_epoch.as_deref(), Some(set.epoch().as_str()));
    // 1. the reference set changes (a digest is edited / added): the stored verdict is void
    let mut changed = set.clone();
    changed.unauthenticated.push(reference_for("gitleaks", ArtifactKind::Executable, "7.7.7", b"another"));
    let o = stored_gitleaks(&conn, &changed);
    assert!(o.ladder.artifact.invalidated && !o.identity_verified);
    assert_eq!((o.ladder.identity, o.ladder.reclassified, o.ladder.version_trust), (IdentityState::Unknown, true, VersionTrust::None));
    // 2. production's (empty) authenticated list does not contain this synthetic reference: nothing is verified
    let prod = store::latest_observations(&conn).unwrap();
    assert!(prod.iter().all(|o| !o.identity_verified));
    // 3. the policy version changes
    conn.execute("UPDATE discovery_observations SET provenance_json = replace(provenance_json, '\"policyVersion\":1', '\"policyVersion\":0') WHERE subject = 'gitleaks'", []).unwrap();
    assert!(stored_gitleaks(&conn, &set).ladder.artifact.invalidated);
    // 4. a stale scan does not count as verified even if the ladder says so
    conn.execute("UPDATE discovery_observations SET provenance_json = replace(provenance_json, '\"policyVersion\":0', '\"policyVersion\":1') WHERE subject = 'gitleaks'", []).unwrap();
    assert!(stored_gitleaks(&conn, &set).identity_verified);
    let old = (chrono::Utc::now() - chrono::Duration::hours(30)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    conn.execute("UPDATE discovery_scans SET finished_at = ?1", [&old]).unwrap();
    conn.execute("UPDATE discovery_observations SET observed_at = ?1", [&old]).unwrap();
    let stale = stored_gitleaks(&conn, &set);
    assert!(!stale.current && !stale.identity_verified, "an old fingerprint never stands for the file as it is now");
    assert_eq!(stale.ladder.identity, IdentityState::Validated, "history keeps what was concluded then");
}

#[test]
fn a_file_changed_after_the_scan_is_re_judged_by_the_next_scan_and_the_old_row_is_history() {
    let env = Env::new();
    write_tool(&env, "gitleaks", b"genuine");
    let roots = authenticated_roots(&env, b"genuine");
    let conn = seeded();
    crate::discovery::run_scan(&conn, &roots, &fp_auth()).unwrap();
    let set = roots.references.clone();
    assert!(stored_gitleaks(&conn, &set).identity_verified);
    write_tool(&env, "gitleaks", b"replaced after the scan");
    assert!(
        stored_gitleaks(&conn, &set).identity_verified,
        "until a new scan, the store only knows what the last scan saw (shown with its time)"
    );
    crate::discovery::run_scan(&conn, &roots, &fp_auth()).unwrap();
    let now = stored_gitleaks(&conn, &set);
    assert_eq!(
        (now.ladder.identity, now.identity_verified, now.ladder.artifact.match_result),
        (IdentityState::Conflicted, false, MatchResult::MismatchesAuthenticated)
    );
    let rows: i64 = conn
        .query_row("SELECT count(*) FROM discovery_observations WHERE subject = 'gitleaks' AND method = 'path-executable'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(rows, 2, "the earlier verdict stays as history");
}

#[test]
fn a_hand_edited_row_cannot_claim_a_verification_the_evidence_does_not_support() {
    let env = Env::new();
    write_tool(&env, "gitleaks", b"unknown build");
    let conn = seeded();
    crate::discovery::run_scan(&conn, &env.roots(), &fp_auth()).unwrap();
    let production = reference::ReferenceSet::production(&conn).unwrap();
    // forge a fully "verified" ladder by editing the database directly
    let forged = format!(
        "{{\"assessed\":true,\"filenameDetected\":true,\"identity\":\"validated\",\"artifactAuth\":\"digest_match\",\"versionTrust\":\"validated\",\"version\":\"known\",\"artifact\":{{\"policyVersion\":1,\"fingerprint\":\"computed\",\"sha256\":\"{}\",\"reference\":\"authenticated\",\"referenceKind\":\"executable\",\"matchResult\":\"matches_authenticated\",\"referenceEpoch\":\"{}\"}}}}",
        sha(b"unknown build"),
        production.epoch()
    );
    conn.execute("UPDATE discovery_observations SET provenance_json = ?1, identity_state = 'validated', identity_verified = 1 WHERE subject = 'gitleaks' AND method = 'path-executable'", [&forged]).unwrap();
    let o = stored_gitleaks(&conn, &production);
    assert!(!o.identity_verified, "the verdict is re-derived from the digest and the code-embedded references");
    assert_ne!(o.ladder.identity, IdentityState::Validated);
    assert!(o.ladder.artifact.invalidated);
    // the self-audit notices the contradiction between the flag columns and the evidence ladder too
    assert_eq!(discovery_status(&conn), ("failed", Some("DISCOVERY_EVIDENCE_UNSAFE")));
}

#[test]
fn reference_rows_in_the_database_can_never_be_authenticated_and_malformed_ones_are_ignored() {
    let conn = seeded();
    reference::insert_unauthenticated(
        &conn,
        "gitleaks",
        ArtifactKind::Executable,
        "pe-exe",
        "windows-x64",
        "1.0.0",
        &sha(b"x"),
        "readme_declared",
        "README (declared)",
    )
    .unwrap();
    assert!(reference::insert_unauthenticated(
        &conn,
        "gitleaks",
        ArtifactKind::Executable,
        "pe-exe",
        "windows-x64",
        "1.0.0",
        "not-a-digest",
        "readme_declared",
        "x"
    )
    .is_err());
    assert!(reference::insert_unauthenticated(
        &conn,
        "gitleaks",
        ArtifactKind::Executable,
        "pe-exe",
        "windows-x64",
        "1.0.0",
        &sha(b"x"),
        "independently_authenticated",
        "x"
    )
    .is_err());
    // the CHECK constraints refuse an "authenticated" row even when written around the helper
    assert!(conn
        .execute(
            "INSERT INTO reference_digests (entry_id, artifact_kind, format, platform_target, version, algorithm, sha256, source_kind, source_label, authentication, added_at)
             VALUES ('gitleaks', 'executable', 'pe-exe', 'windows-x64', '1', 'sha256', ?1, 'user_provided', 'x', 'authenticated', 'now')",
            [sha(b"y")]
        )
        .is_err());
    let set = reference::ReferenceSet::production(&conn).unwrap();
    assert!(set.authenticated.is_empty(), "nothing in the database can authenticate");
    assert_eq!(set.unauthenticated.len(), 1);
    // inserting or editing a stored reference changes the epoch, so earlier verdicts are void
    let before = set.epoch();
    reference::insert_unauthenticated(
        &conn,
        "gitleaks",
        ArtifactKind::Archive,
        "zip",
        "windows-x64",
        "1.0.0",
        &sha(b"z"),
        "user_provided",
        "typed",
    )
    .unwrap();
    assert_ne!(reference::ReferenceSet::production(&conn).unwrap().epoch(), before);
    // a scan never creates a reference by hashing a local file
    let env = Env::new();
    write_tool(&env, "gitleaks", b"local file");
    let rows_before: i64 = conn.query_row("SELECT count(*) FROM reference_digests", [], |r| r.get(0)).unwrap();
    crate::discovery::run_scan(&conn, &env.roots(), &fp_auth()).unwrap();
    let rows_after: i64 = conn.query_row("SELECT count(*) FROM reference_digests", [], |r| r.get(0)).unwrap();
    assert_eq!(rows_before, rows_after);
}

#[test]
fn the_stored_ladder_still_holds_only_closed_vocabulary_and_one_hex_digest() {
    let env = Env::new();
    write_tool(&env, "gitleaks", b"x");
    let conn = seeded();
    crate::discovery::run_scan(&conn, &env.roots(), &fp_auth()).unwrap();
    assert_eq!(discovery_status(&conn).0, "passed");
    conn.execute("UPDATE discovery_observations SET provenance_json = replace(provenance_json, ?1, 'C:\\\\Users\\\\x\\\\tool.exe') WHERE subject = 'gitleaks' AND method = 'path-executable'", [sha(b"x")]).unwrap();
    assert_eq!(
        discovery_status(&conn),
        ("failed", Some("DISCOVERY_EVIDENCE_UNSAFE")),
        "a non-digest string in the digest field is rejected"
    );
}

#[test]
fn migration_8_is_additive_transactional_and_keeps_every_earlier_record() {
    let mut conn = Connection::open_in_memory().unwrap();
    super::privacy_gate::legacy_v4_database(&mut conn);
    db::migrate(&mut conn).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
    let n: i64 = conn.query_row("SELECT count(*) FROM discovery_scans WHERE authorized_fingerprint = 0", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 2, "older scans are recorded as having had no fingerprint permission");
    assert_eq!(
        conn.query_row("SELECT count(*) FROM reference_digests", [], |r| r.get::<_, i64>(0)).unwrap(),
        0,
        "no digest is created for any catalog entry"
    );
    assert!(audit::verify(&conn).unwrap().ok);
    // failure rolls back cleanly
    fn broken(tx: &rusqlite::Transaction) -> crate::error::AppResult<()> {
        tx.execute_batch(include_str!("../../migrations/0008_artifact_provenance.sql"))?;
        Err(crate::error::AppError::new("INJECTED_FAILURE", "crash after the DDL"))
    }
    let mut v7 = Connection::open_in_memory().unwrap();
    super::privacy_gate::legacy_v4_database(&mut v7);
    let mut list = db::migrations_prefix(7);
    db::migrate_with(&mut v7, &list).unwrap();
    let before = table_rows(&v7, "SELECT * FROM discovery_scans ORDER BY 1");
    list.push(db::Migration::Rust(broken));
    assert_eq!(db::migrate_with(&mut v7, &list).unwrap_err().code, "INJECTED_FAILURE");
    assert_eq!(db::user_version(&v7).unwrap(), 7);
    assert_eq!(table_rows(&v7, "SELECT * FROM discovery_scans ORDER BY 1"), before);
    assert_eq!(table_rows(&v7, "SELECT count(*) FROM sqlite_master WHERE name = 'reference_digests'"), vec!["Integer(0)".to_string()]);
}

fn table_rows(conn: &Connection, sql: &str) -> Vec<String> {
    let mut stmt = conn.prepare(sql).unwrap();
    let n = stmt.column_count();
    stmt.query_map([], |r| (0..n).map(|i| r.get::<_, rusqlite::types::Value>(i).map(|v| format!("{v:?}"))).collect::<Result<Vec<_>, _>>())
        .unwrap()
        .map(|r| r.unwrap().join("|"))
        .collect()
}

#[test]
fn no_scan_in_this_phase_runs_a_program_to_learn_anything() {
    let env = Env::new();
    let marker = env.root.join("EXECUTED.txt");
    let script = if cfg!(windows) {
        format!("@echo ran > \"{}\"\r\n", marker.display())
    } else {
        format!("#!/bin/sh\necho ran > '{}'\n", marker.display())
    };
    let p = env.bin.join(if cfg!(windows) { "gitleaks.cmd".to_string() } else { "gitleaks".to_string() });
    fs::write(&p, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let obs = detect::detect_all(&env.roots(), &fp_auth());
    assert_eq!(path_obs(&obs, "gitleaks").provenance.artifact.fingerprint, FingerprintState::Computed);
    assert!(!marker.exists(), "fingerprinting reads bytes; it never starts the file");
}
