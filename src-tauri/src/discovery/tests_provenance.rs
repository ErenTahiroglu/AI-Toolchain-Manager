//! Phase 2B: the passive evidence ladder. A file name, an installation record, publisher metadata, validated identity,
//! a version from metadata and a functional test are six different facts; only the first five are ever assessed, and
//! nothing is executed to learn any of them. Everything here is synthetic.

use super::*;
use crate::discovery::{
    detect::{normalise_repo, pe_signature, CatalogPin},
    types::{ArtifactAuth, IdentityState, NotPerformed, Provenance, Publisher, SignatureBlock, VersionState, VersionTrust},
};

fn pins() -> Vec<CatalogPin> {
    vec![CatalogPin::from_repo_url("agent-browser", "https://github.com/vercel-labs/agent-browser").unwrap()]
}

fn roots_with_pins(env: &Env) -> Roots {
    let mut r = env.roots();
    r.pins = pins();
    r
}

fn path_obs<'a>(obs: &'a [Observation], subject: &str) -> &'a Observation {
    find(obs, subject, Scope::Global, Method::PathExecutable)
}

/// Where a global npm package's metadata sits relative to the PATH folder `env.bin`.
fn npm_package_json(env: &Env, package: &str) -> PathBuf {
    if cfg!(windows) {
        env.bin.join("node_modules").join(package).join("package.json")
    } else {
        env.root.join("lib/node_modules").join(package).join("package.json")
    }
}

fn write_npm(env: &Env, package: &str, json: &str) {
    let p = npm_package_json(env, package);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, json).unwrap();
}

fn good_package_json() -> String {
    serde_json::json!({ "name": "agent-browser", "version": "0.39.1", "repository": { "type": "git", "url": "git+https://github.com/Vercel-Labs/agent-browser.git" } }).to_string()
}

// ---- 1. a file name is only a file name ------------------------------------------------------------------------

#[test]
fn a_matching_file_name_is_detected_but_never_identity_or_version_or_function() {
    let env = Env::new();
    env.fake_exe("gitleaks");
    let obs = detect::detect_all(&roots_with_pins(&env), &all_auth(None));
    let g = path_obs(&obs, "gitleaks");
    let p = &g.provenance;
    assert!(p.assessed && p.filename_detected);
    assert!(!p.install_record);
    assert_eq!(
        (p.publisher, p.identity, p.version, p.version_trust),
        (Publisher::None, IdentityState::Unknown, VersionState::Unknown, VersionTrust::None)
    );
    assert!(!g.identity_verified && !g.functionality_tested && g.version.is_none());
    for expected in [NotPerformed::ActiveVersionProbe, NotPerformed::InstalledFileDigest, NotPerformed::NoPackageDetectorForTool] {
        assert!(p.not_performed.contains(&expected), "{expected:?} in {:?}", p.not_performed);
    }
    // An absent program has no ladder to climb.
    let s = path_obs(&obs, "strix");
    assert_eq!((s.presence, s.provenance.filename_detected, s.provenance.identity), (Presence::Absent, false, IdentityState::NotAssessed));
}

// ---- 2. Claude Code plugin records -------------------------------------------------------------------------------

#[test]
fn a_matching_record_with_files_present_is_corroborated_metadata_never_validated_identity() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", "1.0.0")]));
    cache(&env, "claude-plugins-official", "frontend-design", "1.0.0");
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let p = plugin(&obs, FD, Scope::Global);
    assert_eq!(p.provenance.identity, IdentityState::RecordCorroborated);
    assert!(!p.identity_verified, "identity_verified is reserved for authenticated artifact evidence");
    assert!(p.provenance.install_record && p.provenance.installed_files_seen);
    assert_eq!(p.provenance.artifact_auth, ArtifactAuth::None);
    assert!(p.provenance.not_performed.contains(&NotPerformed::ArtifactAuthentication));
    assert_eq!(
        (p.provenance.publisher, p.provenance.version, p.provenance.version_trust),
        (Publisher::Marketplace, VersionState::Known, VersionTrust::InstallerRecorded)
    );
    assert_eq!(p.version.as_deref(), Some("1.0.0"), "the installer-recorded version stays visible, labelled as a record's declaration");
    assert!(p.limitations.iter().any(|l| l.contains("yerel olarak düzenlenebilir") && l.contains("doğrulanmadı")));
    assert!(!p.functionality_tested);
}

#[test]
fn a_forged_registry_record_without_any_installed_files_is_only_evidence() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", "1.0.0")]));
    // no cache folder at all: anyone can write this JSON
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let p = plugin(&obs, FD, Scope::Global);
    assert_eq!(p.provenance.identity, IdentityState::EvidenceAvailable);
    assert!(!p.provenance.installed_files_seen && !p.identity_verified);
    assert_ne!(p.presence, Presence::Present);
    assert!(p.limitations.iter().any(|l| l.contains("dosyalar önbellekte görünmüyor")));
}

#[test]
fn a_replaced_or_non_directory_cache_entry_never_raises_identity_and_nothing_reaches_validated() {
    // The expected version folder replaced by a plain file.
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", "1.0.0")]));
    env.home_write(".claude/plugins/cache/claude-plugins-official/frontend-design/1.0.0", "i am a file, not an install");
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let p = plugin(&obs, FD, Scope::Global);
    assert_eq!(p.provenance.identity, IdentityState::EvidenceAvailable);
    // Forged cache CONTENT: an arbitrary payload in the expected folder is still only corroborated metadata.
    let env2 = Env::new();
    env2.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env2.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", "1.0.0")]));
    env2.home_write(".claude/plugins/cache/claude-plugins-official/frontend-design/1.0.0/payload.js", "// attacker-controlled content");
    let obs2 = detect::detect_all(&env2.roots(), &all_auth(None));
    let p2 = plugin(&obs2, FD, Scope::Global);
    assert_eq!(p2.provenance.identity, IdentityState::RecordCorroborated);
    assert!(!p2.identity_verified && p2.provenance.artifact_auth == ArtifactAuth::None);
    // Content swapped after the record was written: the next scan sees the same metadata and still claims nothing more.
    fs::write(env2.home.join(".claude/plugins/cache/claude-plugins-official/frontend-design/1.0.0/payload.js"), "// swapped").unwrap();
    let obs3 = detect::detect_all(&env2.roots(), &all_auth(None));
    assert_eq!(plugin(&obs3, FD, Scope::Global).provenance.identity, IdentityState::RecordCorroborated);
    for o in obs.iter().chain(obs2.iter()).chain(obs3.iter()) {
        assert_ne!(o.provenance.identity, IdentityState::Validated, "{}", o.subject);
        assert!(!o.identity_verified, "{}", o.subject);
    }
}

#[test]
fn duplicate_marketplace_names_and_look_alike_marketplaces_cannot_corroborate() {
    let env = Env::new();
    // "claude-plugins-official" appears twice with different cases / look-alike spellings; only the exact string counts.
    for market in ["Claude-Plugins-Official", "claude-plugins-official-", "claude_plugins_official", "claude-plugins-officiaI"] {
        let key = format!("frontend-design@{market}");
        env.home_write(".claude/settings.json", &settings(&[(key.as_str(), true)]));
        env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(key.as_str(), "user", "1.0.0")]));
        cache(&env, market, "frontend-design", "1.0.0");
        let obs = detect::detect_all(&env.roots(), &all_auth(None));
        let p = plugin(&obs, FD, Scope::Global);
        assert_ne!(p.provenance.identity, IdentityState::RecordCorroborated, "{market}");
        assert!(!p.identity_verified, "{market}");
    }
}

#[test]
fn a_settings_declaration_alone_is_configuration_not_provenance() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[]));
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let p = plugin(&obs, FD, Scope::Global);
    assert_eq!(p.configuration, Configuration::ReferencedNotPresent);
    assert_eq!(p.provenance.identity, IdentityState::Unknown);
    assert!(!p.identity_verified && !p.provenance.install_record);
}

#[test]
fn an_unpinned_plugin_with_a_record_has_evidence_but_is_never_validated() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[("superpowers@some-market", true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[("superpowers@some-market", "user", "6.4.2")]));
    cache(&env, "some-market", "superpowers", "6.4.2");
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let p = plugin(&obs, "superpowers", Scope::Global);
    assert_eq!((p.provenance.identity, p.provenance.publisher), (IdentityState::EvidenceAvailable, Publisher::SelfDeclared));
    assert!(!p.identity_verified);
    assert_eq!(p.version.as_deref(), Some("6.4.2"));
}

#[test]
fn forged_publisher_metadata_cannot_validate_a_plugin_and_two_disagreeing_sources_are_a_conflict() {
    // Only an unpinned marketplace claims the name: no validation.
    let env = Env::new();
    let evil = "frontend-design@evil-market";
    env.home_write(".claude/settings.json", &settings(&[(evil, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(evil, "user", "9.9.9")]));
    cache(&env, "evil-market", "frontend-design", "9.9.9");
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let p = plugin(&obs, FD, Scope::Global);
    assert!(!p.identity_verified, "a look-alike from another marketplace is a different identity");
    assert_ne!(p.provenance.identity, IdentityState::Validated);
    assert_eq!(p.version, None);
    // The genuine record AND a same-named record from another marketplace: the sources disagree.
    let env2 = Env::new();
    env2.home_write(".claude/settings.json", &settings(&[(FD_KEY, true), (evil, true)]));
    env2.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", "1.0.0"), (evil, "user", "9.9.9")]));
    cache(&env2, "claude-plugins-official", "frontend-design", "1.0.0");
    let obs2 = detect::detect_all(&env2.roots(), &all_auth(None));
    let p2 = plugin(&obs2, FD, Scope::Global);
    assert_eq!(p2.provenance.identity, IdentityState::Conflicted);
    assert!(!p2.identity_verified, "a conflict is never reported as verified");
}

#[test]
fn a_corrupted_registry_gives_unknown_not_a_guess() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", "{ this is not json ");
    cache(&env, "claude-plugins-official", "frontend-design", "1.0.0");
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let p = plugin(&obs, FD, Scope::Global);
    assert_eq!((p.provenance.identity, p.provenance.install_record), (IdentityState::Unknown, false));
    assert!(!p.identity_verified && p.version.is_none());
    assert!(p.limitations.iter().any(|l| l.contains("geçerli JSON değil")));
}

#[test]
fn conflicting_versions_in_the_installers_records_are_reported_as_conflicted_without_picking_one() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    let two = serde_json::json!({ "version": 2, "plugins": { FD_KEY: [
        { "scope": "user", "installPath": "x", "version": "1.0.0" },
        { "scope": "project", "installPath": "x", "version": "2.0.0" }
    ] } })
    .to_string();
    env.home_write(".claude/plugins/installed_plugins.json", &two);
    cache(&env, "claude-plugins-official", "frontend-design", "1.0.0");
    cache(&env, "claude-plugins-official", "frontend-design", "2.0.0");
    // Project scope looks at every record regardless of scope.
    let project = env.project();
    let obs = detect::detect_all(&env.roots(), &all_auth(Some(&project)));
    let p = plugin(&obs, FD, Scope::Project);
    assert_eq!(p.provenance.version, VersionState::Conflicted);
    assert_eq!(p.version, None);
    assert!(p.limitations.iter().any(|l| l.contains("çelişen sürümler")));
}

// ---- 3. package-manager metadata (npm) -------------------------------------------------------------------------

#[test]
fn matching_npm_metadata_is_evidence_with_a_declared_version_but_not_validation() {
    let env = Env::new();
    env.fake_exe("agent-browser");
    write_npm(&env, "agent-browser", &good_package_json());
    let obs = detect::detect_all(&roots_with_pins(&env), &all_auth(None));
    let a = path_obs(&obs, "agent-browser");
    let p = &a.provenance;
    assert!(p.install_record);
    assert_eq!((p.publisher, p.identity), (Publisher::SelfDeclared, IdentityState::EvidenceAvailable));
    assert_eq!((a.version.as_deref(), a.version_source.as_deref()), (Some("0.39.1"), Some("npm paket kaydı")));
    assert_eq!((p.version, p.version_trust), (VersionState::Known, VersionTrust::Declared));
    assert!(!a.identity_verified, "the package author controls this file");
    assert!(p.not_performed.contains(&NotPerformed::RecordToFileLinkage));
    assert!(a.limitations.iter().any(|l| l.contains("bağlandığı doğrulanmadı")));
}

#[test]
fn forged_or_inconsistent_npm_metadata_is_a_conflict_or_unknown_never_verified() {
    let cases: [(&str, String, IdentityState); 5] = [
        (
            "package name differs",
            serde_json::json!({ "name": "agent-browser-evil", "version": "1.0.0" }).to_string(),
            IdentityState::Conflicted,
        ),
        (
            "repository differs from the catalog",
            serde_json::json!({ "name": "agent-browser", "version": "1.0.0", "repository": "https://github.com/attacker/agent-browser" })
                .to_string(),
            IdentityState::Conflicted,
        ),
        ("not JSON", "{{{".to_string(), IdentityState::Unknown),
        ("not an object", "[1,2,3]".to_string(), IdentityState::Unknown),
        (
            "no repository claim",
            serde_json::json!({ "name": "agent-browser", "version": "1.0.0" }).to_string(),
            IdentityState::EvidenceAvailable,
        ),
    ];
    for (label, json, expected) in cases {
        let env = Env::new();
        env.fake_exe("agent-browser");
        write_npm(&env, "agent-browser", &json);
        let obs = detect::detect_all(&roots_with_pins(&env), &all_auth(None));
        let a = path_obs(&obs, "agent-browser");
        assert_eq!(a.provenance.identity, expected, "{label}");
        assert!(!a.identity_verified, "{label}");
        if expected == IdentityState::Conflicted {
            assert_eq!(a.version, None, "{label}: no version is offered from metadata that contradicts itself");
        }
    }
}

#[test]
fn an_undocumented_or_token_shaped_npm_version_is_not_shown() {
    for bad in ["1.0.0-ghp-SENTINEL", "latest", "../../x", "v1.2.3.4.5"] {
        let env = Env::new();
        env.fake_exe("agent-browser");
        write_npm(
            &env,
            "agent-browser",
            &serde_json::json!({ "name": "agent-browser", "version": bad, "repository": "vercel-labs/agent-browser" }).to_string(),
        );
        let obs = detect::detect_all(&roots_with_pins(&env), &all_auth(None));
        let a = path_obs(&obs, "agent-browser");
        assert_eq!((a.version.clone(), a.provenance.version), (None, VersionState::Unknown), "{bad}");
        assert!(!json_of(&obs).contains("SENTINEL"));
    }
}

/// A separate npm "prefix" with its own PATH folder, program file and (optionally) package record. Returns the PATH folder.
fn prefix_with_program(env: &Env, name: &str, package_json: Option<&str>) -> PathBuf {
    let (bin, modules) = if cfg!(windows) {
        (env.root.join(name), env.root.join(name).join("node_modules"))
    } else {
        (env.root.join(name).join("bin"), env.root.join(name).join("lib/node_modules"))
    };
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join(exe("agent-browser")), "x").unwrap();
    if let Some(json) = package_json {
        let p = modules.join("agent-browser/package.json");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, json).unwrap();
    }
    bin
}

#[test]
fn duplicate_program_names_with_differing_records_are_reported_as_conflicts() {
    let env = Env::new();
    let one = prefix_with_program(&env, "p1", Some(&good_package_json()));
    let two = prefix_with_program(
        &env,
        "p2",
        Some(&serde_json::json!({ "name": "agent-browser", "version": "0.1.0", "repository": "vercel-labs/agent-browser" }).to_string()),
    );
    let mut roots = roots_with_pins(&env);
    roots.path_entries = vec![one, two];
    let obs = detect::detect_all(&roots, &all_auth(None));
    let a = path_obs(&obs, "agent-browser");
    assert!(a.limitations.iter().any(|l| l.contains("farklı kopya")), "{:?}", a.limitations);
    assert_eq!(a.provenance.version, VersionState::Conflicted);
    assert_eq!(a.version, None, "no version is picked between conflicting records");
    assert!(!a.identity_verified);
}

#[test]
fn a_copy_with_a_package_record_next_to_one_without_is_unknown_not_assumed() {
    let env = Env::new();
    let managed = prefix_with_program(&env, "p1", Some(&good_package_json()));
    let loose = prefix_with_program(&env, "p2", None);
    let mut roots = roots_with_pins(&env);
    roots.path_entries = vec![managed, loose];
    let obs = detect::detect_all(&roots, &all_auth(None));
    let a = path_obs(&obs, "agent-browser");
    assert_eq!(a.provenance.identity, IdentityState::Unknown);
    assert_eq!(a.version, None);
}

#[test]
fn repository_spellings_are_normalised_and_nothing_else_is_accepted() {
    for ok in [
        "https://github.com/Owner/Repo.git",
        "git+https://github.com/owner/repo",
        "github:owner/repo",
        "git@github.com:owner/repo.git",
        "owner/repo",
        "https://github.com/owner/repo/",
    ] {
        assert_eq!(normalise_repo(ok).as_deref(), Some("owner/repo"), "{ok}");
    }
    for bad in ["", "https://example.com/owner/repo", "owner", "https://github.com/owner/", "https://github.com/ow ner/repo", "../x/y/z"] {
        assert_ne!(normalise_repo(bad).as_deref(), Some("owner/repo"), "{bad}");
    }
}

// ---- 4. signature metadata (header only; presence is not validity) ---------------------------------------------

fn synthetic_pe(signature: Option<(u32, u32)>, plus: bool) -> Vec<u8> {
    let mut b = vec![0u8; 0x200];
    b[0] = b'M';
    b[1] = b'Z';
    b[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    b[0x80..0x84].copy_from_slice(b"PE\0\0");
    let optional = 0x80 + 24;
    b[optional..optional + 2].copy_from_slice(&(if plus { 0x20bu16 } else { 0x10bu16 }).to_le_bytes());
    let dir = optional + if plus { 0x70 } else { 0x60 } + 4 * 8;
    if let Some((offset, size)) = signature {
        b[dir..dir + 4].copy_from_slice(&offset.to_le_bytes());
        b[dir + 4..dir + 8].copy_from_slice(&size.to_le_bytes());
    }
    b
}

#[test]
fn the_signature_reader_distinguishes_present_absent_and_unreadable_without_judging_validity() {
    for plus in [true, false] {
        assert_eq!(pe_signature(&synthetic_pe(Some((0x1000, 0x400)), plus)), SignatureBlock::PresentUnvalidated);
        assert_eq!(pe_signature(&synthetic_pe(None, plus)), SignatureBlock::Absent);
    }
    assert_eq!(pe_signature(b"not a program"), SignatureBlock::Unreadable);
    assert_eq!(pe_signature(&[]), SignatureBlock::Unreadable);
    assert_eq!(pe_signature(&synthetic_pe(Some((1, 1)), true)[..0x90]), SignatureBlock::Unreadable, "truncated header");
    let mut bad_magic = synthetic_pe(Some((1, 1)), true);
    bad_magic[0x80 + 24] = 0;
    assert_eq!(pe_signature(&bad_magic), SignatureBlock::Unreadable);
    let mut far = synthetic_pe(None, true);
    far[0x3c..0x40].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
    assert_eq!(pe_signature(&far), SignatureBlock::Unreadable, "an out-of-range header offset must not panic");
}

#[test]
fn a_signature_block_is_metadata_only_and_never_raises_identity_or_adds_a_version() {
    let env = Env::new();
    fs::write(env.bin.join(exe("gitleaks")), synthetic_pe(Some((0x1000, 0x400)), true)).unwrap();
    fs::write(env.bin.join(exe("strix")), b"MZ-looking but not a PE").unwrap();
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let g = path_obs(&obs, "gitleaks");
    let s = path_obs(&obs, "strix");
    assert_eq!((g.provenance.identity, g.version.clone()), (IdentityState::Unknown, None));
    assert!(!g.identity_verified);
    if cfg!(windows) {
        assert_eq!(g.provenance.signature_block, SignatureBlock::PresentUnvalidated);
        assert!(g.provenance.not_performed.contains(&NotPerformed::SignatureChainValidation));
        assert!(g.limitations.iter().any(|l| l.contains("geçerliliği, imzalayan ve dosya özeti doğrulanmadı")));
        assert_eq!(s.provenance.signature_block, SignatureBlock::Unreadable);
    } else {
        assert_eq!(g.provenance.signature_block, SignatureBlock::NotAssessed);
        assert!(g.provenance.not_performed.contains(&NotPerformed::SignatureParsingUnsupported));
    }
}

// ---- 5. invariants, persistence, migration, self-audit ---------------------------------------------------------

#[test]
fn the_ladder_invariants_hold_whatever_a_detector_asks_for() {
    let base =
        || Observation::new("x-entry", platform(), "claude-code-terminal", Method::ClaudePluginDeclaration).presence(Presence::Present);
    // "validated" without authenticated artifact evidence is lowered, even with a record and a marketplace publisher
    let a = base().identity_state(IdentityState::Validated).finalize();
    assert_eq!((a.provenance.identity, a.identity_verified), (IdentityState::Unknown, false));
    let b = base()
        .identity_state(IdentityState::Validated)
        .install_record(true)
        .publisher(Publisher::Marketplace)
        .installed_files_seen(true)
        .finalize();
    assert_eq!(
        (b.provenance.identity, b.identity_verified),
        (IdentityState::RecordCorroborated, false),
        "record + marketplace + files is still not authenticated"
    );
    // corroboration needs ALL of: record, installer-recorded marketplace, files on disk
    let c = base().identity_state(IdentityState::RecordCorroborated).install_record(true).publisher(Publisher::Marketplace).finalize();
    assert_eq!(c.provenance.identity, IdentityState::EvidenceAvailable, "no files seen");
    let d = base()
        .identity_state(IdentityState::RecordCorroborated)
        .install_record(true)
        .publisher(Publisher::SelfDeclared)
        .installed_files_seen(true)
        .finalize();
    assert_eq!(d.provenance.identity, IdentityState::EvidenceAvailable, "self-declared publisher");
    let e = base().identity_state(IdentityState::RecordCorroborated).finalize();
    assert_eq!(e.provenance.identity, IdentityState::Unknown);
    // evidence without any record or publisher is just unknown
    assert_eq!(base().identity_state(IdentityState::EvidenceAvailable).finalize().provenance.identity, IdentityState::Unknown);
    // a token-shaped version is dropped by the privacy gate, and the ladder follows
    let f = base()
        .version(Some("ghp_SENTINELSENTINELSENTINELSENTINEL12".into()), "Claude Code kurulum kaydı")
        .version_state(VersionState::Known, VersionTrust::InstallerRecorded)
        .finalize();
    assert_eq!((f.version.clone(), f.provenance.version, f.provenance.version_trust), (None, VersionState::Unknown, VersionTrust::None));
    // a known version without a stated trust is only "declared"; "validated" trust needs artifact authentication
    let g = base().version(Some("1.2.3".into()), "npm paket kaydı").finalize();
    assert_eq!((g.provenance.version, g.provenance.version_trust), (VersionState::Known, VersionTrust::Declared));
    let h = base().version(Some("1.2.3".into()), "npm paket kaydı").version_state(VersionState::Known, VersionTrust::Validated).finalize();
    assert_eq!(h.provenance.version_trust, VersionTrust::InstallerRecorded);
    assert!(!h.functionality_tested);
}

#[test]
fn the_ladder_is_persisted_and_survives_staleness_and_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atm.sqlite");
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", "1.0.0")]));
    cache(&env, "claude-plugins-official", "frontend-design", "1.0.0");
    {
        let conn = db::open_file(&path, None).unwrap();
        catalog::seed_all(&conn).unwrap();
        crate::discovery::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
        let old = (chrono::Utc::now() - chrono::Duration::hours(40)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        conn.execute("UPDATE discovery_scans SET finished_at = ?1", [&old]).unwrap();
        conn.execute("UPDATE discovery_observations SET observed_at = ?1", [&old]).unwrap();
    }
    let conn = db::open_file(&path, None).unwrap();
    let inv = store::inventory(&conn).unwrap();
    let p = inv.observations.iter().find(|o| o.subject == FD && o.method == "claude-plugin-declaration" && o.scope == "global").unwrap();
    assert!(p.stale && !p.current, "stale is a separate fact");
    assert_eq!(
        (p.ladder.identity, p.ladder.install_record, p.ladder.assessed),
        (IdentityState::RecordCorroborated, true, true),
        "the ladder is kept, only its currentness is lost"
    );
    assert!(!p.functionality_tested);
    let identity_state: String =
        conn.query_row("SELECT identity_state FROM discovery_observations WHERE id = ?1", [p.id], |r| r.get(0)).unwrap();
    assert_eq!(identity_state, "record_corroborated");
    let verified: i64 = conn.query_row("SELECT identity_verified FROM discovery_observations WHERE id = ?1", [p.id], |r| r.get(0)).unwrap();
    assert_eq!(verified, 0);
}

#[test]
fn migrations_6_and_7_preserve_every_fact_mark_legacy_ladders_not_assessed_and_lower_old_high_trust() {
    let mut conn = Connection::open_in_memory().unwrap();
    super::privacy_gate::legacy_v4_database(&mut conn);
    // Step to v5 exactly as shipped, then plant a row the Phase 2A rules had accepted as an identity.
    let five = [
        db::Migration::Sql(include_str!("../../migrations/0001_init.sql")),
        db::Migration::Rust(|tx| {
            tx.execute_batch(include_str!("../../migrations/0002_catalog_registry.sql"))?;
            crate::catalog::migrate_phase1a_data(tx)
        }),
        db::Migration::Sql(include_str!("../../migrations/0003_scoped_suggestion_log.sql")),
        db::Migration::Sql(include_str!("../../migrations/0004_discovery.sql")),
        db::Migration::Rust(|tx| {
            tx.execute_batch(include_str!("../../migrations/0005_discovery_privacy.sql"))?;
            crate::discovery::store::scrub_legacy_observations(tx)
        }),
    ];
    db::migrate_with(&mut conn, &five).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), 5);
    conn.execute("UPDATE discovery_observations SET identity_verified = 1 WHERE subject = 'anthropic-frontend-design'", []).unwrap();
    let facts = |c: &Connection| -> Vec<String> {
        let mut s = c
            .prepare("SELECT id, scan_id, subject, presence, configuration, version, scope, project_alias, observed_at, evidence, location FROM discovery_observations ORDER BY id")
            .unwrap();
        let n = s.column_count();
        s.query_map([], |r| (0..n).map(|i| r.get::<_, rusqlite::types::Value>(i).map(|v| format!("{v:?}"))).collect::<Result<Vec<_>, _>>())
            .unwrap()
            .map(|r| r.unwrap().join("|"))
            .collect()
    };
    let before = facts(&conn);
    let prefs_before = table_text(&conn, "SELECT * FROM project_preferences ORDER BY 1,2,3,4");
    let audit_before: i64 = conn.query_row("SELECT count(*) FROM audit_events", [], |r| r.get(0)).unwrap();
    db::migrate(&mut conn).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
    assert_eq!(facts(&conn), before, "no observation fact changed");
    assert_eq!(conn.query_row("SELECT count(*) FROM audit_events", [], |r| r.get::<_, i64>(0)).unwrap(), audit_before);
    assert!(audit::verify(&conn).unwrap().ok);
    let inv = store::inventory(&conn).unwrap();
    assert!(inv.observations.iter().all(|o| !o.ladder.assessed), "legacy rows were never assessed under Phase 2B rules");
    let fd = inv.observations.iter().find(|o| o.subject == "anthropic-frontend-design").unwrap();
    assert_eq!(
        (fd.ladder.identity, fd.ladder.version, fd.ladder.reclassified),
        (IdentityState::NotAssessed, VersionState::Known, true),
        "a Phase 2A boolean is lowered and flagged, never silently upcast; the shown version stays"
    );
    assert!(!fd.identity_verified);
    assert_eq!(
        table_text(
            &conn,
            "SELECT identity_verified, identity_state FROM discovery_observations WHERE subject = 'anthropic-frontend-design'"
        )[0],
        "Integer(0)|Text(\"not_assessed\")"
    );
    let g = inv.observations.iter().find(|o| o.subject == "gitleaks").unwrap();
    assert_eq!((g.ladder.identity, g.ladder.version), (IdentityState::NotAssessed, VersionState::Unknown));
    assert_eq!(table_text(&conn, "SELECT * FROM project_preferences ORDER BY 1,2,3,4"), prefs_before, "user choices are untouched");
}

fn table_text(conn: &Connection, sql: &str) -> Vec<String> {
    let mut stmt = conn.prepare(sql).unwrap();
    let n = stmt.column_count();
    stmt.query_map([], |r| (0..n).map(|i| r.get::<_, rusqlite::types::Value>(i).map(|v| format!("{v:?}"))).collect::<Result<Vec<_>, _>>())
        .unwrap()
        .map(|r| r.unwrap().join("|"))
        .collect()
}

#[test]
fn the_stored_ladder_holds_only_closed_vocabulary_and_a_tampered_one_fails_the_self_audit() {
    let env = Env::new();
    env.fake_exe("agent-browser");
    write_npm(&env, "agent-browser", &good_package_json());
    let conn = seeded();
    let mut roots = env.roots();
    roots.pins = pins();
    crate::discovery::run_scan(&conn, &roots, &all_auth(None)).unwrap();
    let rows: Vec<String> = table_text(&conn, "SELECT provenance_json FROM discovery_observations");
    assert!(!rows.is_empty());
    let allowed = regex::Regex::new(r#"^[A-Za-z0-9_{}\[\]":,]*$"#).unwrap();
    for r in &rows {
        let json = r.trim_start_matches("Text(\"").trim_end_matches("\")").replace("\\\"", "\"");
        assert!(allowed.is_match(&json), "provenance JSON may hold only enum words: {json}");
        assert!(serde_json::from_str::<Provenance>(&json).is_ok());
    }
    assert_eq!(discovery_status(&conn).0, "passed");
    conn.execute("UPDATE discovery_observations SET provenance_json = '{\"identity\":\"definitely-fine\"}' WHERE id = (SELECT min(id) FROM discovery_observations)", []).unwrap();
    assert_eq!(discovery_status(&conn), ("failed", Some("DISCOVERY_EVIDENCE_UNSAFE")));
    // an identity column that contradicts the verified flag also fails
    let conn2 = seeded();
    crate::discovery::run_scan(&conn2, &roots, &all_auth(None)).unwrap();
    conn2
        .execute("UPDATE discovery_observations SET identity_verified = 1 WHERE id = (SELECT min(id) FROM discovery_observations)", [])
        .unwrap();
    assert_eq!(discovery_status(&conn2), ("failed", Some("DISCOVERY_EVIDENCE_UNSAFE")));
}

#[test]
fn discovery_never_runs_anything_to_learn_a_version() {
    // The source-scan test bans process APIs in detect.rs/fsx.rs/privacy.rs; this asserts the behavioural side: a
    // program file that would announce itself if executed is only ever read as bytes.
    let env = Env::new();
    let marker = env.root.join("EXECUTED.txt");
    let script = if cfg!(windows) {
        format!("@echo ran > \"{}\"\r\n", marker.display())
    } else {
        format!("#!/bin/sh\necho ran > '{}'\n", marker.display())
    };
    for (name, body) in [("gitleaks", script.clone()), ("graphify", script)] {
        let p = env.bin.join(if cfg!(windows) { format!("{name}.cmd") } else { name.to_string() });
        fs::write(&p, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    let obs = detect::detect_all(&roots_with_pins(&env), &all_auth(None));
    assert_eq!(path_obs(&obs, "gitleaks").presence, Presence::Present);
    assert!(!marker.exists(), "a discovered program must never be started");
    assert!(obs.iter().all(|o| o.version.is_none() || o.version_source.is_some()));
}

/// The first six migrations exactly as shipped (used to build a database as an earlier build left it).
fn migrations_up_to_6() -> Vec<db::Migration> {
    vec![
        db::Migration::Sql(include_str!("../../migrations/0001_init.sql")),
        db::Migration::Rust(|tx| {
            tx.execute_batch(include_str!("../../migrations/0002_catalog_registry.sql"))?;
            crate::catalog::migrate_phase1a_data(tx)
        }),
        db::Migration::Sql(include_str!("../../migrations/0003_scoped_suggestion_log.sql")),
        db::Migration::Sql(include_str!("../../migrations/0004_discovery.sql")),
        db::Migration::Rust(|tx| {
            tx.execute_batch(include_str!("../../migrations/0005_discovery_privacy.sql"))?;
            crate::discovery::store::scrub_legacy_observations(tx)
        }),
        db::Migration::Sql(include_str!("../../migrations/0006_passive_provenance.sql")),
    ]
}

#[test]
fn migration_7_lowers_what_the_first_phase_2b_build_stored_and_keeps_ids_history_and_the_audit_chain() {
    let mut conn = Connection::open_in_memory().unwrap();
    super::privacy_gate::legacy_v4_database(&mut conn);
    db::migrate_with(&mut conn, &migrations_up_to_6()).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), 6);
    // Rows exactly as the first Phase 2B build wrote them: "validated" from a registry record alone, trust "trusted".
    let old_ladder = |identity: &str, trust: &str| {
        format!("{{\"assessed\":true,\"filenameDetected\":false,\"installRecord\":true,\"publisher\":\"marketplace\",\"identity\":\"{identity}\",\"version\":\"known\",\"versionTrust\":\"{trust}\",\"signatureBlock\":\"not_assessed\",\"notPerformed\":[]}}")
    };
    conn.execute("INSERT INTO discovery_scans VALUES ('s-2b', '2026-10-17T10:00:00Z', '2026-10-17T10:00:00Z', 'windows-x64', 0, 1, 0, 'completed', 3)", []).unwrap();
    for (subject, verified, state, json) in [
        ("anthropic-frontend-design", 1, "validated", old_ladder("validated", "trusted")),
        ("superpowers", 0, "evidence_available", old_ladder("evidence_available", "trusted")),
        ("caveman", 1, "validated", old_ladder("validated", "trusted")),
    ] {
        conn.execute(
            "INSERT INTO discovery_observations (scan_id, subject, platform_target, agent_surface, method, presence, configuration, version, version_source, scope,
              confidence, evidence, limitations_json, identity_verified, functionality_tested, observed_at, record_version, identity_state, version_state, provenance_json)
             VALUES ('s-2b', ?1, 'windows-x64', 'claude-code-terminal', 'claude-plugin-declaration', 'present', 'configured', '1.0.0', 'Claude Code kurulum kaydı', 'global',
              'high', 'Eklenti kayıt defterinde.', '[]', ?2, 0, '2026-10-17T10:00:00Z', 2, ?3, 'known', ?4)",
            rusqlite::params![subject, verified, state, json],
        )
        .unwrap();
    }
    let top: i64 = conn.query_row("SELECT max(id) FROM discovery_observations", [], |r| r.get(0)).unwrap();
    conn.execute("DELETE FROM discovery_observations WHERE id = ?1", [top]).unwrap(); // AUTOINCREMENT high-water mark is now above max(id)
    let facts = "SELECT id, scan_id, subject, presence, configuration, version, version_source, scope, confidence, evidence, observed_at FROM discovery_observations ORDER BY id";
    let before = table_text(&conn, facts);
    let prefs = table_text(&conn, "SELECT * FROM project_preferences ORDER BY 1,2,3,4");
    let audit_count: i64 = conn.query_row("SELECT count(*) FROM audit_events", [], |r| r.get(0)).unwrap();

    db::migrate(&mut conn).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
    assert_eq!(table_text(&conn, facts), before, "ids and every observed fact are preserved by the table rebuild");
    assert_eq!(table_text(&conn, "SELECT * FROM project_preferences ORDER BY 1,2,3,4"), prefs);
    assert_eq!(conn.query_row("SELECT count(*) FROM audit_events", [], |r| r.get::<_, i64>(0)).unwrap(), audit_count);
    assert!(audit::verify(&conn).unwrap().ok);
    assert_eq!(
        table_text(&conn, "SELECT count(*) FROM discovery_observations WHERE identity_state = 'validated' OR identity_verified = 1"),
        vec!["Integer(0)".to_string()],
        "nothing is stored as validated / verified any more"
    );
    let fd = table_text(&conn, "SELECT identity_state, identity_verified, provenance_json FROM discovery_observations WHERE subject = 'anthropic-frontend-design' AND scan_id = 's-2b'");
    assert!(
        fd[0].contains("evidence_available")
            && fd[0].contains("reclassified")
            && fd[0].contains("installer_recorded")
            && !fd[0].contains("trusted"),
        "{fd:?}"
    );
    let sp = table_text(
        &conn,
        "SELECT identity_state, provenance_json FROM discovery_observations WHERE subject = 'superpowers' AND scan_id = 's-2b'",
    );
    assert!(
        sp[0].contains("evidence_available") && sp[0].contains("installer_recorded") && !sp[0].contains("reclassified"),
        "an already-modest state is only renamed: {sp:?}"
    );
    let inv = store::inventory(&conn).unwrap();
    assert!(inv.observations.iter().all(|o| !o.identity_verified && o.ladder.identity != IdentityState::Validated));
    let lowered = inv.observations.iter().find(|o| o.subject == "anthropic-frontend-design" && o.scan_id == "s-2b").unwrap();
    assert!(lowered.ladder.reclassified && lowered.ladder.version_trust == VersionTrust::InstallerRecorded);
    let note: String = conn.query_row("SELECT detail FROM diagnostics WHERE code = 'DISCOVERY_TRUST_LOWERED'", [], |r| r.get(0)).unwrap();
    assert!(!note.contains("OtherUser"));
    // the AUTOINCREMENT high-water mark survived: a new row never reuses the deleted id
    let env = Env::new();
    crate::discovery::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    let new_min: i64 = conn
        .query_row("SELECT min(id) FROM discovery_observations WHERE scan_id NOT IN ('s-2b', 'legacy-scan-1', 'legacy-scan-2')", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(new_min > top, "{new_min} must exceed the old high-water mark {top}");
    db::migrate(&mut conn).unwrap(); // idempotent
    assert_eq!(
        table_text(&conn, "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = 'discovery_observations_key'"),
        vec!["Integer(1)".to_string()],
        "the index was recreated"
    );
}

#[test]
fn a_failed_migration_7_rolls_back_to_the_untouched_v6_database() {
    fn broken(tx: &rusqlite::Transaction) -> crate::error::AppResult<()> {
        tx.execute_batch(include_str!("../../migrations/0007_identity_taxonomy.sql"))?;
        Err(crate::error::AppError::new("INJECTED_FAILURE", "crash after the rebuild"))
    }
    let mut v6 = Connection::open_in_memory().unwrap();
    super::privacy_gate::legacy_v4_database(&mut v6);
    let mut list = migrations_up_to_6();
    db::migrate_with(&mut v6, &list).unwrap();
    let before = table_text(&v6, "SELECT * FROM discovery_observations ORDER BY id");
    list.push(db::Migration::Rust(broken));
    assert_eq!(db::migrate_with(&mut v6, &list).unwrap_err().code, "INJECTED_FAILURE");
    assert_eq!(db::user_version(&v6).unwrap(), 6);
    assert_eq!(table_text(&v6, "SELECT * FROM discovery_observations ORDER BY id"), before);
    assert_eq!(
        table_text(&v6, "SELECT count(*) FROM sqlite_master WHERE name = 'discovery_observations_v7'"),
        vec!["Integer(0)".to_string()]
    );
}
