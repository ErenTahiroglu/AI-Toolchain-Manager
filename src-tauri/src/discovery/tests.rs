//! Discovery tests run on synthetic temporary environments only. They never depend on the machine's real
//! Claude Code installation, account data or PATH.

use super::{
    detect::{self, Authorization, ProjectScan, Roots, PLATFORM_MACOS, PLATFORM_UNSUPPORTED, PLATFORM_WINDOWS},
    store,
    types::*,
    validate_project_dir, ScanRequest,
};
use crate::{audit, catalog, db, projects};
use rusqlite::Connection;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

#[path = "tests_fs.rs"]
mod fs_security;
#[path = "tests_history.rs"]
mod history;
#[path = "tests_privacy.rs"]
mod privacy_gate;
#[path = "tests_provenance.rs"]
mod provenance;

/// A dummy token used to prove nothing from configuration files leaks. It is not a real credential.
const SENTINEL: &str = "ghp_SENTINELSENTINELSENTINELSENTINEL12";
const SENTINEL2: &str = "sk-SENTINEL-SECRET-VALUE-0123456789abcdef";

fn platform() -> &'static str {
    if cfg!(windows) {
        PLATFORM_WINDOWS
    } else {
        PLATFORM_MACOS
    }
}

fn exe(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

struct Env {
    _dir: TempDir,
    root: PathBuf,
    home: PathBuf,
    bin: PathBuf,
}

impl Env {
    fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let home = root.join("home");
        let bin = root.join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin).unwrap();
        Env { _dir: dir, root, home, bin }
    }

    fn roots(&self) -> Roots {
        Roots {
            platform: platform().to_string(),
            home: Some(self.home.clone()),
            path_entries: vec![self.bin.clone()],
            system_dirs: vec![],
            pins: vec![],
        }
    }

    fn write(&self, rel_from_root: &str, content: &str) -> PathBuf {
        let p = self.root.join(rel_from_root);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, content).unwrap();
        p
    }

    fn home_write(&self, rel: &str, content: &str) -> PathBuf {
        self.write(&format!("home/{rel}"), content)
    }

    fn fake_exe(&self, base: &str) {
        self.write(&format!("bin/{}", exe(base)), "not a real program");
    }

    fn project(&self) -> PathBuf {
        let p = self.root.join("project");
        fs::create_dir_all(&p).unwrap();
        p
    }
}

fn all_auth(project: Option<&Path>) -> Authorization {
    Authorization {
        path_executables: true,
        user_config: true,
        project: project.map(|p| ProjectScan { alias: "proje".to_string(), dir: p.to_path_buf() }),
    }
}

fn find<'a>(obs: &'a [Observation], subject: &str, scope: Scope, method: Method) -> &'a Observation {
    obs.iter().find(|o| o.subject == subject && o.scope == scope && o.method == method).unwrap_or_else(|| {
        panic!(
            "no observation {subject} {scope:?} {method:?} in {:#?}",
            obs.iter().map(|o| (&o.subject, o.scope, o.method)).collect::<Vec<_>>()
        )
    })
}

fn plugin<'a>(obs: &'a [Observation], subject: &str, scope: Scope) -> &'a Observation {
    find(obs, subject, scope, Method::ClaudePluginDeclaration)
}

fn registry(entries: &[(&str, &str, &str)]) -> String {
    let mut plugins = serde_json::Map::new();
    for (key, scope, version) in entries {
        plugins.insert(
            (*key).to_string(),
            serde_json::json!([{ "scope": scope, "installPath": "ignored-by-the-detector", "version": version }]),
        );
    }
    serde_json::json!({ "version": 2, "plugins": plugins }).to_string()
}

fn settings(enabled: &[(&str, bool)]) -> String {
    let map: BTreeMap<&str, bool> = enabled.iter().copied().collect();
    serde_json::json!({ "enabledPlugins": map, "env": { "SECRET_TOKEN": SENTINEL }, "apiKeyHelper": SENTINEL2 }).to_string()
}

fn cache(env: &Env, market: &str, name: &str, version: &str) {
    fs::create_dir_all(env.home.join(".claude/plugins/cache").join(market).join(name).join(version)).unwrap();
}

const FD: &str = "anthropic-frontend-design";
const FD_KEY: &str = "frontend-design@claude-plugins-official";

fn json_of(obs: &[Observation]) -> String {
    serde_json::to_string(obs).unwrap()
}

// ------------------------------------------------------------------------------------------------
// Presence, absence, unknown — executables
// ------------------------------------------------------------------------------------------------

#[test]
fn a_program_name_in_path_is_present_but_only_low_confidence_and_never_verified() {
    let env = Env::new();
    env.fake_exe("gitleaks");
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let g = find(&obs, "gitleaks", Scope::Global, Method::PathExecutable);
    assert_eq!((g.presence, g.configuration, g.confidence), (Presence::Present, Configuration::NotApplicable, Confidence::Low));
    assert!(!g.identity_verified && !g.functionality_tested);
    assert!(g.version.is_none() && g.limitations.iter().any(|l| l.contains("çalıştırılmadı")));
    assert!(g.limitations.iter().any(|l| l.contains("Ad eşleşmesi")));
    assert!(g.location.as_deref().unwrap().contains("gitleaks"));
    // A tool that is not there is Absent (only PATH was searched), never "Present" or "verified".
    let s = find(&obs, "strix", Scope::Global, Method::PathExecutable);
    assert_eq!(s.presence, Presence::Absent);
    assert!(s.limitations.iter().any(|l| l.contains("PATH dışına")));
}

#[test]
fn without_any_usable_path_folder_the_answer_is_unknown_not_absent() {
    let env = Env::new();
    let mut roots = env.roots();
    roots.path_entries = vec![env.root.join("does-not-exist"), PathBuf::from("relative/dir"), PathBuf::new()];
    let obs = detect::detect_all(&roots, &all_auth(None));
    let g = find(&obs, "gitleaks", Scope::Global, Method::PathExecutable);
    assert_eq!(g.presence, Presence::Unknown);
    assert!(g.limitations.iter().any(|l| l.contains("atlandı")), "{:?}", g.limitations);
}

#[test]
fn claude_code_is_looked_for_in_its_documented_native_location_and_in_path() {
    let env = Env::new();
    // Nothing installed: Absent with an honest scope note.
    let none = detect::detect_all(&env.roots(), &all_auth(None));
    let a = find(&none, "agent:claude-code", Scope::Global, Method::PathExecutable);
    assert_eq!(a.presence, Presence::Absent);
    assert!(a.limitations.iter().any(|l| l.contains("VS Code")));
    // Documented native location (~/.local/bin/claude[.exe]) and PATH.
    env.home_write(&format!(".local/bin/{}", exe("claude")), "stub");
    env.fake_exe("claude");
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let native = find(&obs, "agent:claude-code", Scope::Global, Method::NativeInstallLocation);
    assert_eq!((native.presence, native.confidence), (Presence::Present, Confidence::Medium));
    assert!(native.location.as_deref().unwrap().starts_with("~/.local/bin"));
    assert!(!native.location.as_deref().unwrap().contains("home"), "the home prefix is collapsed: {:?}", native.location);
    assert_eq!(find(&obs, "agent:claude-code", Scope::Global, Method::PathExecutable).presence, Presence::Present);
    // A found file is NOT a working Claude Code: nothing was run and nothing was tested.
    assert!(obs.iter().filter(|o| o.subject == "agent:claude-code").all(|o| !o.functionality_tested && o.version.is_none()));
}

#[test]
fn versions_are_never_obtained_by_running_programs() {
    let env = Env::new();
    env.fake_exe("gitleaks");
    env.fake_exe("claude");
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    assert!(
        obs.iter().all(|o| o.version.is_none() || o.method == Method::ClaudePluginDeclaration),
        "only the plugin registry may supply a version"
    );
    assert!(obs.iter().filter(|o| o.presence == Presence::Present).all(|o| !o.functionality_tested));
}

// ------------------------------------------------------------------------------------------------
// Plugins: declared / fetched / loaded
// ------------------------------------------------------------------------------------------------

#[test]
fn an_official_plugin_that_is_declared_recorded_and_cached_is_present_and_configured_but_not_verified() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", "1.2.3")]));
    cache(&env, "claude-plugins-official", "frontend-design", "1.2.3");
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let p = plugin(&obs, FD, Scope::Global);
    assert_eq!((p.presence, p.configuration, p.confidence), (Presence::Present, Configuration::Configured, Confidence::High));
    assert_eq!(p.version.as_deref(), Some("1.2.3"));
    assert_eq!(p.version_source.as_deref(), Some("Claude Code kurulum kaydı"));
    assert!(!p.identity_verified, "a matching local record is corroborated metadata, not authenticated identity (Phase 2B closure)");
    assert_eq!(p.provenance.identity, crate::discovery::types::IdentityState::RecordCorroborated);
    assert!(!p.functionality_tested);
    assert!(p.limitations.iter().any(|l| l.contains("Çalışan bir Claude Code oturumunda")), "the loaded stage is never claimed");
}

#[test]
fn a_plugin_referenced_in_settings_but_not_actually_present_is_reported_as_such() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[]));
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let p = plugin(&obs, FD, Scope::Global);
    assert_eq!((p.presence, p.configuration), (Presence::Absent, Configuration::ReferencedNotPresent));
    assert!(p.evidence.contains("başvuru var, dosyalar yok"));
    // And with no registry file at all.
    let env2 = Env::new();
    env2.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    let p2 = detect::detect_all(&env2.roots(), &all_auth(None));
    assert_eq!(plugin(&p2, FD, Scope::Global).configuration, Configuration::ReferencedNotPresent);
}

#[test]
fn a_record_without_cached_files_is_unknown_not_present() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", "1.2.3")]));
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let p = plugin(&obs, FD, Scope::Global);
    assert_eq!((p.presence, p.configuration), (Presence::Unknown, Configuration::Configured));
    assert!(p.evidence.contains("dosyalar önbellekte görünmüyor"));
    assert!(p.version.is_none());
}

#[test]
fn disabled_and_undeclared_installs_are_distinguished() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, false)]));
    env.home_write(
        ".claude/plugins/installed_plugins.json",
        &registry(&[(FD_KEY, "user", "1.0.0"), ("superpowers@some-market", "user", "7.0.0")]),
    );
    cache(&env, "claude-plugins-official", "frontend-design", "1.0.0");
    cache(&env, "some-market", "superpowers", "7.0.0");
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let fd = plugin(&obs, FD, Scope::Global);
    assert_eq!((fd.presence, fd.configuration), (Presence::Present, Configuration::Disabled));
    let sp = plugin(&obs, "superpowers", Scope::Global);
    assert_eq!((sp.presence, sp.configuration), (Presence::Present, Configuration::NotConfigured), "installed but never declared");
}

#[test]
fn a_same_named_plugin_from_another_marketplace_is_not_the_official_one() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[("frontend-design@evil-market", true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[("frontend-design@evil-market", "user", "9.9.9")]));
    cache(&env, "evil-market", "frontend-design", "9.9.9");
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let p = plugin(&obs, FD, Scope::Global);
    assert_eq!((p.presence, p.configuration), (Presence::Absent, Configuration::NotConfigured));
    assert!(!p.identity_verified && p.version.is_none());
    assert!(p.limitations.iter().any(|l| l.contains("başka bir pazar yerinden")));
}

#[test]
fn plugins_without_a_known_marketplace_are_matched_by_name_only_and_say_so() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[("superpowers@whoever", true), ("ponytail@someone", true)]));
    env.home_write(
        ".claude/plugins/installed_plugins.json",
        &registry(&[("superpowers@whoever", "user", "7.0.0"), ("ponytail@someone", "user", "5.1.0")]),
    );
    cache(&env, "whoever", "superpowers", "7.0.0");
    cache(&env, "someone", "ponytail", "5.1.0");
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    for subject in ["superpowers", "ponytail"] {
        let p = plugin(&obs, subject, Scope::Global);
        assert_eq!((p.presence, p.confidence, p.identity_verified), (Presence::Present, Confidence::Medium, false), "{subject}");
        assert!(p.limitations.iter().any(|l| l.contains("yalnızca eklenti adı eşleşti")), "{subject}");
    }
}

#[test]
fn global_installation_and_project_scoped_configuration_are_kept_apart() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", "1.0.0")]));
    cache(&env, "claude-plugins-official", "frontend-design", "1.0.0");
    let project = env.project();
    fs::create_dir_all(project.join(".claude")).unwrap();
    fs::write(project.join(".claude/settings.json"), settings(&[(FD_KEY, false)])).unwrap();
    fs::write(project.join(".claude/settings.local.json"), settings(&[(FD_KEY, true)])).unwrap();
    let obs = detect::detect_all(&env.roots(), &all_auth(Some(&project)));
    let g = plugin(&obs, FD, Scope::Global);
    let p = plugin(&obs, FD, Scope::Project);
    let l = plugin(&obs, FD, Scope::Local);
    assert_eq!(
        (g.configuration, p.configuration, l.configuration),
        (Configuration::Configured, Configuration::Disabled, Configuration::Configured)
    );
    assert_eq!((g.project_alias.as_deref(), p.project_alias.as_deref(), l.project_alias.as_deref()), (None, Some("proje"), Some("proje")));
    assert!(
        p.limitations.iter().any(|x| x.contains("projeye özel bir kurulum olarak doğrulanmadı")),
        "project scope never claims an isolated install"
    );
}

#[test]
fn project_files_are_read_only_when_a_project_is_authorized_and_nothing_else_in_it() {
    let env = Env::new();
    let project = env.project();
    fs::create_dir_all(project.join(".claude")).unwrap();
    fs::write(project.join(".claude/settings.json"), settings(&[(FD_KEY, true)])).unwrap();
    fs::write(project.join("secrets.env"), SENTINEL).unwrap();
    fs::write(project.join("src.rs"), SENTINEL2).unwrap();
    let without = detect::detect_all(&env.roots(), &all_auth(None));
    assert!(
        without.iter().all(|o| o.project_alias.is_none() && o.scope != Scope::Project && o.scope != Scope::Local),
        "no project observation without authorization"
    );
    let with = detect::detect_all(&env.roots(), &all_auth(Some(&project)));
    assert!(with.iter().any(|o| o.scope == Scope::Project));
    let json = json_of(&with);
    assert!(!json.contains("SENTINEL"), "unrelated project files are never read");
}

#[test]
fn without_user_config_authorization_no_user_file_is_reported() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude.json", "{\"mcpServers\":{}}");
    let auth = Authorization { path_executables: true, user_config: false, project: None };
    let obs = detect::detect_all(&env.roots(), &auth);
    assert!(obs
        .iter()
        .all(|o| !matches!(o.method, Method::ClaudePluginDeclaration | Method::ClaudeMcpConfig | Method::ClaudeSettingsFile)));
    let none = Authorization { path_executables: false, user_config: false, project: None };
    let obs = detect::detect_all(&env.roots(), &none);
    assert!(obs.iter().all(|o| matches!(o.method, Method::NotDocumented | Method::NoDetector)), "only the always-unknown entries remain");
}

// ------------------------------------------------------------------------------------------------
// Malformed, oversized, odd input
// ------------------------------------------------------------------------------------------------

#[test]
fn malformed_and_hostile_configuration_never_panics_and_is_reported_honestly() {
    let nested = format!("{}{}", "[".repeat(5000), "]".repeat(5000));
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("not json", b"{this is not json".to_vec()),
        ("empty file", Vec::new()),
        ("array root", b"[1,2,3]".to_vec()),
        ("deeply nested", nested.into_bytes()),
        ("enabledPlugins is an array", b"{\"enabledPlugins\": [\"a@b\"]}".to_vec()),
        ("enabledPlugins values are objects", b"{\"enabledPlugins\": {\"frontend-design@claude-plugins-official\": {\"x\": 1}}}".to_vec()),
        ("invalid utf-8", vec![0xff, 0xfe, 0xfd, 0x80]),
        ("bom + valid", [&[0xEF, 0xBB, 0xBF][..], b"{\"enabledPlugins\": {}}"].concat()),
    ];
    for (label, bytes) in cases {
        let env = Env::new();
        let p = env.home.join(".claude/settings.json");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, &bytes).unwrap();
        let obs = detect::detect_all(&env.roots(), &all_auth(None));
        let s = find(&obs, "agent:claude-code", Scope::Global, Method::ClaudeSettingsFile);
        assert_ne!(s.presence, Presence::Present, "{label}: a config file never proves installation");
        if label == "bom + valid" {
            assert_eq!(s.configuration, Configuration::Configured, "{label}");
        } else if label == "invalid utf-8" {
            assert_eq!(s.configuration, Configuration::Unknown, "{label}");
        } else if label == "empty file" || label == "not json" || label == "deeply nested" {
            assert_eq!(s.configuration, Configuration::Malformed, "{label}");
        }
        assert!(!plugin(&obs, FD, Scope::Global).functionality_tested, "{label}");
    }
}

#[test]
fn an_oversized_config_file_is_skipped_not_read() {
    let env = Env::new();
    let big = format!("{{\"padding\":\"{}\"}}", "x".repeat(1024 * 1024 + 10));
    env.home_write(".claude/settings.json", &big);
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let s = find(&obs, "agent:claude-code", Scope::Global, Method::ClaudeSettingsFile);
    assert_eq!(s.configuration, Configuration::Unknown);
    assert!(s.evidence.contains("güvenli boyut sınırından büyük"), "{}", s.evidence);
}

#[test]
fn malformed_plugin_registry_makes_presence_unknown_but_settings_are_still_reported() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", "{broken");
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let p = plugin(&obs, FD, Scope::Global);
    assert_eq!((p.presence, p.configuration), (Presence::Unknown, Configuration::Configured));
    assert!(p.limitations.iter().any(|l| l.contains("kurulum kaydı geçerli JSON değil")));
}

#[test]
fn a_hostile_version_string_is_dropped_and_never_used_to_build_a_path() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    for version in ["../../escape", "1.0.0; rm -rf /", SENTINEL, "a/b", "..", "1 2"] {
        env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", version)]));
        let obs = detect::detect_all(&env.roots(), &all_auth(None));
        let p = plugin(&obs, FD, Scope::Global);
        assert!(p.version.is_none(), "{version}");
        assert_ne!(p.presence, Presence::Present, "{version}: a record with an unusable version proves nothing");
        assert!(!json_of(&obs).contains("SENTINEL"), "{version}");
    }
}

// ------------------------------------------------------------------------------------------------
// MCP configuration references
// ------------------------------------------------------------------------------------------------

#[test]
fn mcp_references_are_reported_as_configured_but_never_as_installed_or_working() {
    let env = Env::new();
    env.home_write(
        ".claude.json",
        &serde_json::json!({
            "oauthAccount": { "accessToken": SENTINEL },
            "mcpServers": {
                "memory": { "command": r"C:\tools\codebase-memory-mcp.exe", "args": ["--token", SENTINEL2], "env": { "API_KEY": SENTINEL } },
                "other": { "command": "npx", "args": ["x"], "env": { "T": SENTINEL } }
            }
        })
        .to_string(),
    );
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let m = find(&obs, "codebase-memory-mcp", Scope::Global, Method::ClaudeMcpConfig);
    assert_eq!((m.presence, m.configuration), (Presence::Unknown, Configuration::Configured));
    assert!(m.evidence.contains("1 MCP sunucu tanımı") && m.evidence.contains("çalıştırılmadı"));
    assert!(!m.evidence.contains("memory") && !m.evidence.contains("(ad:"), "server names are never kept: {}", m.evidence);
    assert!(m.limitations.iter().any(|l| l.contains("kurulu olduğunu göstermez")));
    assert!(!m.identity_verified);
    let json = json_of(&obs);
    assert!(
        !json.contains("SENTINEL") && !json.contains("other"),
        "arguments, environment and unrelated server names never appear: {json}"
    );
}

#[test]
fn mcp_files_without_a_matching_server_say_so_and_missing_files_are_not_configured() {
    let env = Env::new();
    env.home_write(".claude.json", "{\"mcpServers\":{\"a\":{\"command\":\"x\"},\"b\":{\"command\":\"y\"}}}");
    let project = env.project();
    let obs = detect::detect_all(&env.roots(), &all_auth(Some(&project)));
    let g = find(&obs, "codebase-memory-mcp", Scope::Global, Method::ClaudeMcpConfig);
    assert_eq!(g.configuration, Configuration::NotConfigured);
    assert!(g.evidence.contains("2 MCP sunucusu tanımlı"));
    let p = find(&obs, "codebase-memory-mcp", Scope::Project, Method::ClaudeMcpConfig);
    assert_eq!(p.configuration, Configuration::NotConfigured, ".mcp.json missing");
}

#[test]
fn project_and_local_mcp_scopes_are_separate_and_the_local_key_is_matched_across_separators() {
    let env = Env::new();
    let project = env.project();
    fs::write(project.join(".mcp.json"), "{\"mcpServers\":{\"codebase-memory\":{\"command\":\"codebase-memory-mcp\"}}}").unwrap();
    let key = project.to_string_lossy().replace('\\', "/");
    env.home_write(".claude.json", &serde_json::json!({ "projects": { key: { "mcpServers": { "my-codebase-memory-server": { "command": "x" } } }, "/elsewhere": { "mcpServers": { "codebase-memory": {} } } } }).to_string());
    let obs = detect::detect_all(&env.roots(), &all_auth(Some(&project)));
    assert_eq!(find(&obs, "codebase-memory-mcp", Scope::Project, Method::ClaudeMcpConfig).configuration, Configuration::Configured);
    assert_eq!(find(&obs, "codebase-memory-mcp", Scope::Local, Method::ClaudeMcpConfig).configuration, Configuration::Configured);
    assert_eq!(
        find(&obs, "codebase-memory-mcp", Scope::Global, Method::ClaudeMcpConfig).configuration,
        Configuration::NotConfigured,
        "other projects' entries are not this project's"
    );
}

// ------------------------------------------------------------------------------------------------
// Never guessing: Antigravity, unsupported platform, entries without a detector
// ------------------------------------------------------------------------------------------------

#[test]
fn antigravity_ide_is_never_guessed_even_if_lookalike_folders_exist() {
    let env = Env::new();
    for dir in [".antigravity", ".gemini", ".config/Antigravity", "AppData/Roaming/Antigravity"] {
        env.home_write(&format!("{dir}/settings.json"), "{}");
    }
    env.fake_exe("agy");
    env.fake_exe("antigravity");
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let a: Vec<&Observation> = obs.iter().filter(|o| o.subject == "agent:antigravity-ide").collect();
    assert_eq!(a.len(), 1);
    assert_eq!((a[0].presence, a[0].method, a[0].agent_surface.as_str()), (Presence::Unknown, Method::NotDocumented, "antigravity-ide"));
    assert!(a[0].limitations.iter().any(|l| l.contains("Antigravity CLI ayrı bir ürün")));
    assert!(!json_of(&obs).contains("agy"), "CLI names are not even looked up");
}

#[test]
fn an_unsupported_platform_yields_unsupported_for_everything_and_reads_nothing() {
    let env = Env::new();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.fake_exe("gitleaks");
    let mut roots = env.roots();
    roots.platform = PLATFORM_UNSUPPORTED.to_string();
    let obs = detect::detect_all(&roots, &all_auth(None));
    assert!(!obs.is_empty());
    assert!(obs
        .iter()
        .all(|o| o.presence == Presence::Unsupported && o.method == Method::UnsupportedPlatform && o.platform_target == "unsupported"));
    assert!(obs.iter().any(|o| o.subject == "gitleaks" && o.presence == Presence::Unsupported), "reported as unsupported, not as absent");
}

#[test]
fn entries_without_a_documented_detector_are_reported_as_such() {
    let env = Env::new();
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    for subject in ["cloudflare-security-audit-skill", "agentic-awesome-skills"] {
        let o = obs.iter().find(|o| o.subject == subject).unwrap();
        assert_eq!((o.presence, o.method), (Presence::Unknown, Method::NoDetector));
    }
}

// ------------------------------------------------------------------------------------------------
// Secrets
// ------------------------------------------------------------------------------------------------

#[test]
fn secrets_from_every_configuration_file_never_reach_an_observation_or_the_database() {
    let env = Env::new();
    let project = env.project();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true), (&format!("{SENTINEL}@market"), true)]));
    env.home_write(
        ".claude/plugins/installed_plugins.json",
        &registry(&[(FD_KEY, "user", "1.0.0"), (&format!("superpowers@{SENTINEL}"), "user", "7.0.0")]),
    );
    env.home_write(
        ".claude.json",
        &serde_json::json!({ "oauthAccount": { "accessToken": SENTINEL }, "mcpServers": { format!("{SENTINEL}-codebase-memory"): { "command": "x", "env": { "K": SENTINEL2 } } } }).to_string(),
    );
    fs::create_dir_all(project.join(".claude")).unwrap();
    fs::write(project.join(".claude/settings.json"), settings(&[(FD_KEY, true)])).unwrap();
    fs::write(project.join(".claude/settings.local.json"), settings(&[])).unwrap();
    fs::write(project.join(".mcp.json"), serde_json::json!({ "mcpServers": { "codebase-memory": { "command": "codebase-memory-mcp", "args": [SENTINEL2], "headers": { "Authorization": format!("Bearer {SENTINEL}") } } } }).to_string()).unwrap();
    let obs = detect::detect_all(&env.roots(), &all_auth(Some(&project)));
    assert!(obs.len() > 15);
    let json = json_of(&obs);
    for secret in ["SENTINEL", "Bearer", "oauthAccount", "accessToken", "apiKeyHelper", "SECRET_TOKEN"] {
        assert!(!json.contains(secret), "{secret} leaked into: {json}");
    }
    // Same through the database.
    let conn = db::open_memory().unwrap();
    catalog::seed_all(&conn).unwrap();
    projects::add_reference(&conn, "proje", "Proje", "").unwrap();
    let roots = env.roots();
    let auth = all_auth(Some(&project));
    super::run_scan(&conn, &roots, &auth).unwrap();
    let dump: String = {
        let mut stmt = conn.prepare("SELECT * FROM discovery_observations").unwrap();
        let cols = stmt.column_count();
        let rows = stmt
            .query_map([], |r| {
                (0..cols).map(|i| r.get::<_, rusqlite::types::Value>(i).map(|v| format!("{v:?}"))).collect::<Result<Vec<_>, _>>()
            })
            .unwrap();
        rows.map(|r| r.unwrap().join("|")).collect::<Vec<_>>().join("\n")
    };
    assert!(!dump.contains("SENTINEL"), "database dump leaks");
    let diag = format!("{:?}{:?}", crate::diagnostics::list(&conn, 100).unwrap(), audit::list_ascending(&conn).unwrap());
    assert!(!diag.contains("SENTINEL"));
}

// ------------------------------------------------------------------------------------------------
// Links, junctions, inaccessible folders, network paths
// ------------------------------------------------------------------------------------------------

#[cfg(windows)]
fn make_junction(link: &Path, target: &Path) {
    let status = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(link).arg(target).output().unwrap();
    assert!(status.status.success(), "could not create a junction for the test: {}", String::from_utf8_lossy(&status.stderr));
}

#[cfg(windows)]
fn remove_junction(link: &Path) {
    // `rmdir` on a junction removes only the link, never the target's contents.
    let _ = std::process::Command::new("cmd").args(["/C", "rmdir"]).arg(link).output();
}

#[cfg(windows)]
#[test]
fn a_junction_in_place_of_the_claude_folder_is_not_followed() {
    let env = Env::new();
    let outside = env.root.join("outside");
    fs::create_dir_all(outside.join("plugins")).unwrap();
    fs::write(outside.join("settings.json"), settings(&[(FD_KEY, true)])).unwrap();
    make_junction(&env.home.join(".claude"), &outside);
    let obs = detect::detect_all(&env.roots(), &all_auth(None));
    let s = find(&obs, "agent:claude-code", Scope::Global, Method::ClaudeSettingsFile);
    assert_eq!(s.configuration, Configuration::Unknown);
    assert!(s.evidence.contains("sembolik bağlantı/junction"), "{}", s.evidence);
    assert!(!json_of(&obs).contains("SENTINEL"));
    assert_eq!(plugin(&obs, FD, Scope::Global).configuration, Configuration::Unknown);
    remove_junction(&env.home.join(".claude"));
    assert!(outside.join("settings.json").exists(), "the target of the junction is untouched");
}

#[cfg(windows)]
#[test]
fn a_junction_as_a_path_folder_or_as_the_project_folder_is_refused() {
    let env = Env::new();
    let real = env.root.join("real-bin");
    fs::create_dir_all(&real).unwrap();
    fs::write(real.join("gitleaks.exe"), "x").unwrap();
    let link = env.root.join("linked-bin");
    make_junction(&link, &real);
    let mut roots = env.roots();
    roots.path_entries = vec![link.clone()];
    let obs = detect::detect_all(&roots, &all_auth(None));
    let g = find(&obs, "gitleaks", Scope::Global, Method::PathExecutable);
    assert_eq!(g.presence, Presence::Unknown, "a junction PATH entry is skipped, so nothing usable remains");
    let project_link = env.root.join("linked-project");
    make_junction(&project_link, &env.project());
    assert_eq!(validate_project_dir(&project_link.to_string_lossy()).unwrap_err().code, "PROJECT_DIR_LINK");
    remove_junction(&link);
    remove_junction(&project_link);
}

#[cfg(unix)]
mod unix_links {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn symlinked_config_files_and_folders_are_not_followed() {
        let env = Env::new();
        let outside = env.root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("real-settings.json"), settings(&[(FD_KEY, true)])).unwrap();
        fs::create_dir_all(env.home.join(".claude")).unwrap();
        symlink(outside.join("real-settings.json"), env.home.join(".claude/settings.json")).unwrap();
        let obs = detect::detect_all(&env.roots(), &all_auth(None));
        let s = find(&obs, "agent:claude-code", Scope::Global, Method::ClaudeSettingsFile);
        assert_eq!(s.configuration, Configuration::Unknown);
        assert!(s.evidence.contains("sembolik bağlantı/junction"));
        assert!(!json_of(&obs).contains("SENTINEL"));
        // A symlinked ~/.claude directory as well.
        let env2 = Env::new();
        let out2 = env2.root.join("elsewhere");
        fs::create_dir_all(&out2).unwrap();
        fs::write(out2.join("settings.json"), settings(&[(FD_KEY, true)])).unwrap();
        symlink(&out2, env2.home.join(".claude")).unwrap();
        let obs2 = detect::detect_all(&env2.roots(), &all_auth(None));
        assert_eq!(find(&obs2, "agent:claude-code", Scope::Global, Method::ClaudeSettingsFile).configuration, Configuration::Unknown);
    }

    #[test]
    fn a_symlinked_cache_directory_is_not_accepted_as_a_fetched_plugin() {
        let env = Env::new();
        env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
        env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", "1.0.0")]));
        let real = env.root.join("real-cache-dir");
        fs::create_dir_all(&real).unwrap();
        let parent = env.home.join(".claude/plugins/cache/claude-plugins-official/frontend-design");
        fs::create_dir_all(&parent).unwrap();
        symlink(&real, parent.join("1.0.0")).unwrap();
        let obs = detect::detect_all(&env.roots(), &all_auth(None));
        assert_ne!(plugin(&obs, FD, Scope::Global).presence, Presence::Present);
    }

    #[test]
    fn a_symlink_in_path_is_present_but_flagged_and_a_symlinked_path_folder_is_skipped() {
        let env = Env::new();
        let real = env.root.join("real-tool");
        fs::write(&real, "x").unwrap();
        symlink(&real, env.bin.join("gitleaks")).unwrap();
        let obs = detect::detect_all(&env.roots(), &all_auth(None));
        let g = find(&obs, "gitleaks", Scope::Global, Method::PathExecutable);
        assert_eq!(g.presence, Presence::Present);
        assert!(g.limitations.iter().any(|l| l.contains("sembolik bağlantı")), "{:?}", g.limitations);
        let link_dir = env.root.join("link-bin");
        symlink(&env.bin, &link_dir).unwrap();
        let mut roots = env.roots();
        roots.path_entries = vec![link_dir];
        let obs = detect::detect_all(&roots, &all_auth(None));
        assert_eq!(find(&obs, "gitleaks", Scope::Global, Method::PathExecutable).presence, Presence::Unknown);
    }

    #[test]
    fn an_inaccessible_claude_folder_is_reported_not_crashed() {
        let env = Env::new();
        env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
        let dir = env.home.join(".claude");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o000)).unwrap();
        let readable = fs::read_dir(&dir).is_ok(); // running as root ignores the mode
        let obs = detect::detect_all(&env.roots(), &all_auth(None));
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        if !readable {
            let s = find(&obs, "agent:claude-code", Scope::Global, Method::ClaudeSettingsFile);
            assert_eq!(s.configuration, Configuration::Unknown);
            assert!(s.evidence.contains("erişim izni yok"), "{}", s.evidence);
        }
    }

    #[test]
    fn the_project_folder_itself_must_not_be_a_symlink() {
        let env = Env::new();
        let link = env.root.join("proj-link");
        symlink(env.project(), &link).unwrap();
        assert_eq!(validate_project_dir(&link.to_string_lossy()).unwrap_err().code, "PROJECT_DIR_LINK");
    }
}

#[test]
fn network_and_relative_path_entries_are_skipped_without_being_touched() {
    let env = Env::new();
    let mut roots = env.roots();
    let mut hostile: Vec<PathBuf> = vec![PathBuf::from("."), PathBuf::from("../bin"), PathBuf::new(), PathBuf::from("bin")];
    if cfg!(windows) {
        hostile.extend([
            PathBuf::from(r"\\attacker.invalid\share\bin"),
            PathBuf::from(r"\\?\UNC\attacker.invalid\share"),
            PathBuf::from("//attacker.invalid/share"),
        ]);
    }
    roots.path_entries = hostile;
    roots.path_entries.push(env.bin.clone());
    env.fake_exe("gitleaks");
    let obs = detect::detect_all(&roots, &all_auth(None));
    let g = find(&obs, "gitleaks", Scope::Global, Method::PathExecutable);
    assert_eq!(g.presence, Presence::Present, "the one real folder is still searched");
    assert!(g.limitations.iter().any(|l| l.contains("PATH girdisi atlandı")));
    assert!(
        super::fsx::is_network_or_device_path(Path::new(if cfg!(windows) { r"\\server\share" } else { "//server/share" })) == cfg!(windows)
    );
}

#[test]
fn path_scanning_is_bounded() {
    let env = Env::new();
    let mut roots = env.roots();
    roots.path_entries = (0..500).map(|i| env.root.join(format!("no-such-{i}"))).collect();
    roots.path_entries.push(env.bin.clone()); // beyond the 64-entry bound, so never reached
    env.fake_exe("gitleaks");
    let obs = detect::detect_all(&roots, &all_auth(None));
    assert_eq!(
        find(&obs, "gitleaks", Scope::Global, Method::PathExecutable).presence,
        Presence::Unknown,
        "only the first 64 entries are looked at"
    );
}

// ------------------------------------------------------------------------------------------------
// Read-only enforcement
// ------------------------------------------------------------------------------------------------

type FileState = (u64, Option<std::time::SystemTime>, bool, Vec<u8>);

fn snapshot(root: &Path) -> BTreeMap<String, FileState> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, FileState>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).unwrap();
            let rel = path.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            let content = if meta.is_file() { fs::read(&path).unwrap() } else { Vec::new() };
            out.insert(rel, (meta.len(), meta.modified().ok(), meta.permissions().readonly(), content));
            if meta.is_dir() {
                walk(&path, root, out);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn a_full_scan_changes_nothing_in_the_scanned_tree_not_even_timestamps() {
    let env = Env::new();
    let project = env.project();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", "1.0.0")]));
    env.home_write(".claude.json", "{\"mcpServers\":{}}");
    env.home_write(&format!(".local/bin/{}", exe("claude")), "stub");
    cache(&env, "claude-plugins-official", "frontend-design", "1.0.0");
    env.fake_exe("gitleaks");
    fs::create_dir_all(project.join(".claude")).unwrap();
    fs::write(project.join(".claude/settings.json"), settings(&[])).unwrap();
    fs::write(project.join(".mcp.json"), "{}").unwrap();
    let before = snapshot(&env.root);
    assert!(before.len() > 10);
    let conn = db::open_memory().unwrap();
    catalog::seed_all(&conn).unwrap();
    projects::add_reference(&conn, "proje", "Proje", "").unwrap();
    for _ in 0..3 {
        super::run_scan(&conn, &env.roots(), &all_auth(Some(&project))).unwrap();
    }
    assert_eq!(
        snapshot(&env.root),
        before,
        "content, size, modification time and read-only flag are identical; no file was added or removed"
    );
}

#[test]
fn the_discovery_code_contains_no_way_to_write_execute_or_use_the_network() {
    let sources = [
        ("fsx.rs", include_str!("fsx.rs")),
        ("detect.rs", include_str!("detect.rs")),
        ("types.rs", include_str!("types.rs")),
        ("privacy.rs", include_str!("privacy.rs")),
    ];
    let forbidden = [
        "Command::new",
        "std::process",
        "TcpStream",
        "UdpSocket",
        "TcpListener",
        "reqwest",
        "hyper",
        "ureq",
        "fs::write",
        "File::create",
        "create_dir",
        "remove_file",
        "remove_dir",
        "fs::rename",
        "fs::copy",
        // fsx opens files read-only through `OpenOptions`/`openat`; every way to ask for write access is banned instead.
        ".write(true",
        ".append(true",
        ".create(true",
        ".create_new(true",
        ".truncate(true",
        "O_WRONLY",
        "O_RDWR",
        "O_CREAT",
        "O_TRUNC",
        "O_APPEND",
        "libc::unlink",
        "libc::rename",
        "libc::mkdir",
        "libc::chmod",
        "libc::write",
        "libc::symlink",
        "libc::link(",
        "libc::exec",
        "libc::system",
        "libc::fork",
        "libc::socket",
        "libc::connect",
        "libc::kill",
        "set_permissions",
        "set_modified",
        "hard_link",
        "symlink_file",
        "symlink_dir",
        "fs::canonicalize",
        "read_dir",
        "env::set_var",
    ];
    for (name, text) in sources {
        // Only production code counts: strip everything from the unit-test module on. (A test-only helper module
        // that sits earlier in a file is tiny and holds no file-system calls, so it does not weaken the scan.)
        let production = text.split("mod unit {").next().unwrap();
        assert!(production.len() > 500, "{name}: the scan must really look at the production code");
        for word in forbidden {
            assert!(!production.contains(word), "{name} contains forbidden capability `{word}`");
        }
    }
    let cargo = include_str!("../../Cargo.toml");
    // The Windows API surface is limited to file-system queries: no process, threading, networking or registry features.
    for feature in ["Win32_System", "Win32_Networking", "Win32_Security", "Win32_UI", "Win32_NetworkManagement"] {
        assert!(!cargo.contains(feature), "windows-sys feature `{feature}` must not be enabled in Phase 2A");
    }
    for http in ["reqwest", "ureq", "hyper", "isahc", "surf", "attohttpc", "curl"] {
        assert!(!cargo.contains(http), "an HTTP client crate `{http}` must not be a dependency in Phase 2A");
    }
}

// ------------------------------------------------------------------------------------------------
// Project directory validation and authorization
// ------------------------------------------------------------------------------------------------

#[test]
fn project_directories_are_validated_strictly() {
    let env = Env::new();
    let project = env.project();
    assert!(validate_project_dir(&project.to_string_lossy()).is_ok());
    assert!(validate_project_dir(&format!("  {}  ", project.to_string_lossy())).is_ok(), "surrounding spaces are trimmed");
    for (bad, code) in [
        ("", "PROJECT_DIR_INVALID"),
        ("   ", "PROJECT_DIR_INVALID"),
        ("relative/dir", "PROJECT_DIR_INVALID"),
        ("~/project", "PROJECT_DIR_INVALID"),
        ("project\nother", "PROJECT_DIR_INVALID"),
    ] {
        assert_eq!(validate_project_dir(bad).unwrap_err().code, code, "{bad:?}");
    }
    let dotdot = format!("{}/../project", env.root.to_string_lossy());
    assert_eq!(validate_project_dir(&dotdot).unwrap_err().code, "PROJECT_DIR_INVALID");
    assert_eq!(validate_project_dir(&"x".repeat(300)).unwrap_err().code, "PROJECT_DIR_INVALID");
    assert_eq!(validate_project_dir(&env.root.join("missing").to_string_lossy()).unwrap_err().code, "PROJECT_DIR_NOT_FOUND");
    let file = env.write("a-file.txt", "x");
    assert_eq!(validate_project_dir(&file.to_string_lossy()).unwrap_err().code, "PROJECT_DIR_NOT_A_DIRECTORY");
    let root = if cfg!(windows) { r"C:\" } else { "/" };
    assert_eq!(validate_project_dir(root).unwrap_err().code, "PROJECT_DIR_TOO_BROAD");
    if cfg!(windows) {
        assert_eq!(validate_project_dir(r"\\attacker.invalid\share\proj").unwrap_err().code, "PROJECT_DIR_NETWORK");
        assert_eq!(validate_project_dir(r"\\?\UNC\attacker.invalid\share\proj").unwrap_err().code, "PROJECT_DIR_NETWORK");
        assert_eq!(validate_project_dir(r"\\.\PhysicalDrive0").unwrap_err().code, "PROJECT_DIR_NETWORK");
    }
}

#[test]
fn a_scan_needs_at_least_one_authorization_and_a_registered_project() {
    let env = Env::new();
    let conn = db::open_memory().unwrap();
    catalog::seed_all(&conn).unwrap();
    let nothing = ScanRequest { path_executables: false, user_config: false, project: None };
    assert_eq!(super::authorize(&conn, &nothing).unwrap_err().code, "SCAN_NOT_AUTHORIZED");
    let unknown = ScanRequest {
        path_executables: false,
        user_config: false,
        project: Some(super::ProjectRequest { alias: "yok".into(), directory: env.project().to_string_lossy().into_owned() }),
    };
    assert_eq!(super::authorize(&conn, &unknown).unwrap_err().code, "PROJECT_NOT_FOUND");
    let ok = ScanRequest {
        path_executables: false,
        user_config: false,
        project: Some(super::ProjectRequest { alias: "ornek-web-sitesi".into(), directory: env.project().to_string_lossy().into_owned() }),
    };
    assert!(super::authorize(&conn, &ok).unwrap().project.is_some());
    assert_eq!(nothing_count(&conn), 0, "refused requests leave no scan behind");
}

fn nothing_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM discovery_scans", [], |r| r.get(0)).unwrap()
}

// ------------------------------------------------------------------------------------------------
// Persistence, repeated scans, staleness, preferences untouched
// ------------------------------------------------------------------------------------------------

fn seeded() -> Connection {
    let conn = db::open_memory().unwrap();
    catalog::seed_all(&conn).unwrap();
    conn
}

#[test]
fn repeated_scans_keep_history_but_show_one_latest_row_per_key() {
    let env = Env::new();
    let conn = seeded();
    env.fake_exe("gitleaks");
    let s1 = super::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    fs::remove_file(env.bin.join(exe("gitleaks"))).unwrap();
    let s2 = super::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    assert_ne!(s1.scan_id, s2.scan_id);
    let inv = store::inventory(&conn).unwrap();
    let g: Vec<_> = inv.observations.iter().filter(|o| o.subject == "gitleaks").collect();
    assert_eq!(g.len(), 1, "one latest row per key");
    assert_eq!(g[0].presence, "absent", "the newer scan wins");
    let total: i64 = conn.query_row("SELECT count(*) FROM discovery_observations WHERE subject = 'gitleaks'", [], |r| r.get(0)).unwrap();
    assert_eq!(total, 2, "history is kept");
    assert_eq!(inv.last_scan.unwrap().scan_id, s2.scan_id);
}

#[test]
fn a_later_scan_with_fewer_authorizations_does_not_erase_earlier_observations() {
    let env = Env::new();
    let conn = seeded();
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    super::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    super::run_scan(&conn, &env.roots(), &Authorization { path_executables: true, user_config: false, project: None }).unwrap();
    let inv = store::inventory(&conn).unwrap();
    assert!(
        inv.observations.iter().any(|o| o.method == "claude-settings-file"),
        "the earlier user-config observation is still shown (with its own timestamp)"
    );
    assert!(
        inv.not_checked.iter().any(|n| n.contains("kullanıcı ayarları")),
        "and the last scan says that user settings were not checked this time"
    );
}

#[test]
fn old_observations_are_marked_stale_and_fresh_ones_are_not() {
    let env = Env::new();
    let conn = seeded();
    super::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    assert!(store::inventory(&conn).unwrap().observations.iter().all(|o| !o.stale && o.age_hours < 1));
    let old = (chrono::Utc::now() - chrono::Duration::hours(48)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    conn.execute("UPDATE discovery_observations SET observed_at = ?1", [&old]).unwrap();
    let inv = store::inventory(&conn).unwrap();
    assert!(inv.observations.iter().all(|o| o.stale && o.age_hours >= 47));
    assert_eq!(inv.stale_after_hours, 24);
}

#[test]
fn old_scans_are_pruned_with_their_observations() {
    let env = Env::new();
    let conn = seeded();
    for _ in 0..(store::KEEP_SCANS + 5) {
        super::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    }
    assert_eq!(nothing_count(&conn), store::KEEP_SCANS);
    let orphans: i64 = conn
        .query_row("SELECT count(*) FROM discovery_observations WHERE scan_id NOT IN (SELECT scan_id FROM discovery_scans)", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(orphans, 0);
}

#[test]
fn the_database_refuses_any_claim_of_testing_or_unknown_vocabulary() {
    let env = Env::new();
    let conn = seeded();
    super::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    for sql in [
        "UPDATE discovery_observations SET functionality_tested = 1",
        "UPDATE discovery_observations SET presence = 'verified'",
        "UPDATE discovery_observations SET configuration = 'working'",
        "UPDATE discovery_observations SET scope = 'everywhere'",
        "UPDATE discovery_observations SET method = 'ran-the-program'",
        "UPDATE discovery_observations SET confidence = 'certain'",
        "UPDATE discovery_observations SET agent_surface = 'vscode'",
        "UPDATE discovery_observations SET platform_target = 'linux'",
        "UPDATE discovery_observations SET evidence = (SELECT printf('%1000s', 'x'))",
    ] {
        assert!(conn.execute(sql, []).is_err(), "accepted: {sql}");
    }
}

#[test]
fn scanning_never_changes_preferences_catalog_or_audit_chain_validity() {
    let env = Env::new();
    let conn = seeded();
    projects::add_reference(&conn, "proje", "Proje", "").unwrap();
    projects::set_preference(&conn, "proje", FD, "claude-code-terminal", PLATFORM_WINDOWS, "accepted", None).unwrap();
    let prefs = |c: &Connection| -> String {
        let mut stmt = c.prepare("SELECT project_id, entry_id, agent_surface, platform_target, decision, decided_at, catalog_revision, capability_fingerprint, actual_install_state, actual_verification_state, observed_scope FROM project_preferences ORDER BY 1, 2, 3, 4").unwrap();
        stmt.query_map([], |r| {
            (0..11).map(|i| r.get::<_, rusqlite::types::Value>(i).map(|v| format!("{v:?}"))).collect::<Result<Vec<_>, _>>()
        })
        .unwrap()
        .map(|r| r.unwrap().join("|"))
        .collect::<Vec<_>>()
        .join("\n")
    };
    let catalog_state = |c: &Connection| -> String {
        c.query_row(
            "SELECT group_concat(entry_id || install_state || verification_state || capability_fingerprint) FROM catalog_entries",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    let (p0, c0) = (prefs(&conn), catalog_state(&conn));
    // A scan that finds the tool installed must not touch the user's decision or the catalog's "unknown" state.
    env.home_write(".claude/settings.json", &settings(&[(FD_KEY, true)]));
    env.home_write(".claude/plugins/installed_plugins.json", &registry(&[(FD_KEY, "user", "1.0.0")]));
    cache(&env, "claude-plugins-official", "frontend-design", "1.0.0");
    super::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    assert_eq!((prefs(&conn), catalog_state(&conn)), (p0, c0));
    assert!(audit::verify(&conn).unwrap().ok);
    let events = audit::list_ascending(&conn).unwrap();
    let scan_event = events.iter().find(|e| e["eventType"] == "inspect").expect("the scan is audited");
    assert_eq!(
        (scan_event["outcome"].as_str(), scan_event["actorType"].as_str(), scan_event["checkId"].as_str()),
        (Some("passed"), Some("user"), Some("discovery-scan"))
    );
}

#[test]
fn a_users_request_is_compared_with_the_observation_without_confusing_them() {
    let env = Env::new();
    let conn = seeded();
    projects::add_reference(&conn, "proje", "Proje", "").unwrap();
    projects::set_preference(&conn, "proje", FD, "claude-code-terminal", platform(), "accepted", None).unwrap();
    projects::set_preference(&conn, "proje", "agent-browser", "local-cli", platform(), "accepted", None).unwrap();
    let inv = store::inventory(&conn).unwrap();
    assert!(inv.requests.iter().all(|r| r.status == "not_scanned"), "before any scan nothing is claimed");
    env.fake_exe("agent-browser"); // only a name match
    super::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    let inv = store::inventory(&conn).unwrap();
    let by = |id: &str| inv.requests.iter().find(|r| r.entry_id == id).unwrap().clone();
    assert_eq!(by("agent-browser").status, "present_current");
    assert!(by("agent-browser").note.contains("çalıştığı veya projede etkin olduğu anlamına gelmez"));
    assert_eq!(by(FD).status, "absent_current");
    assert!(by(FD).note.contains("bulunamadı"));
}

#[test]
fn every_stored_observation_is_free_of_absolute_home_paths_and_secret_shapes() {
    let env = Env::new();
    let conn = seeded();
    env.fake_exe("gitleaks");
    env.home_write(&format!(".local/bin/{}", exe("claude")), "stub");
    super::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    let home = env.home.to_string_lossy().into_owned();
    for o in store::latest_observations(&conn).unwrap() {
        let text = format!("{} {} {:?} {:?}", o.evidence, o.limitations.join(" "), o.location, o.version);
        assert!(!text.contains(&home), "absolute home path leaked in {text}");
        assert_eq!(crate::redact::sanitize(&o.evidence, 400), o.evidence, "evidence must survive redaction unchanged");
        assert!(!o.functionality_tested);
    }
}

// ------------------------------------------------------------------------------------------------
// Self-audit and migration
// ------------------------------------------------------------------------------------------------

fn discovery_status(conn: &Connection) -> (&'static str, Option<&'static str>) {
    let ctx = crate::health::HealthContext {
        tray: crate::health::TrayState::NotAttempted,
        autostart: crate::health::AutostartState::NotQueried,
        previous_shutdown: crate::health::PreviousShutdown::FirstRun,
    };
    let report = crate::health::run(conn, &ctx);
    let c = report.checks.iter().find(|c| c.id == "discovery").unwrap();
    (c.status, c.error_code)
}

#[test]
fn self_audit_never_reports_discovery_as_good_before_a_scan_and_flags_stale_or_tampered_data() {
    let env = Env::new();
    let conn = seeded();
    assert_eq!(discovery_status(&conn).0, "unknown", "never scanned is unknown, not passed");
    super::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    assert_eq!(discovery_status(&conn).0, "passed");
    // Stale scan: unknown again (observations may be out of date).
    let old = (chrono::Utc::now() - chrono::Duration::hours(30)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    conn.execute("UPDATE discovery_scans SET finished_at = ?1", [&old]).unwrap();
    assert_eq!(discovery_status(&conn).0, "unknown");
    // Tampered row that bypassed the app (a token-shaped text inside the evidence) must fail the audit.
    conn.execute("UPDATE discovery_scans SET finished_at = ?1", [crate::audit::now_utc()]).unwrap();
    conn.execute(
        "UPDATE discovery_observations SET evidence = ?1 WHERE id = (SELECT min(id) FROM discovery_observations)",
        [format!("key {SENTINEL}")],
    )
    .unwrap();
    assert_eq!(discovery_status(&conn), ("failed", Some("DISCOVERY_EVIDENCE_UNSAFE")));
}

#[test]
fn overall_health_stays_unknown_or_worse_it_never_becomes_green_from_a_scan() {
    let env = Env::new();
    let conn = seeded();
    env.fake_exe("gitleaks");
    super::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    let ctx = crate::health::HealthContext {
        tray: crate::health::TrayState::NotAttempted,
        autostart: crate::health::AutostartState::NotQueried,
        previous_shutdown: crate::health::PreviousShutdown::FirstRun,
    };
    assert_ne!(crate::health::run(&conn, &ctx).overall, "passed");
}

#[test]
fn migration_4_is_additive_and_keeps_every_earlier_record() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    // Build a schema-v3 database (the state at the Phase 1B closure), put user data in it, then migrate.
    let v3 = [
        db::Migration::Sql(include_str!("../../migrations/0001_init.sql")),
        db::Migration::Rust(|tx| {
            tx.execute_batch(include_str!("../../migrations/0002_catalog_registry.sql"))?;
            crate::catalog::migrate_phase1a_data(tx)
        }),
        db::Migration::Sql(include_str!("../../migrations/0003_scoped_suggestion_log.sql")),
    ];
    db::migrate_with(&mut conn, &v3).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), 3);
    catalog::seed_all(&conn).unwrap();
    projects::add_reference(&conn, "proje", "Proje", "özel not").unwrap();
    projects::set_preference(&conn, "proje", FD, "claude-code-terminal", PLATFORM_WINDOWS, "accepted", None).unwrap();
    let dump = |c: &Connection| -> String {
        ["catalog_entries", "project_registry", "project_preferences", "preference_history", "audit_events", "settings", "diagnostics"]
            .iter()
            .map(|t| {
                let mut stmt = c.prepare(&format!("SELECT * FROM {t} ORDER BY 1, 2")).unwrap();
                let cols = stmt.column_count();
                let rows: Vec<String> = stmt
                    .query_map([], |r| {
                        (0..cols).map(|i| r.get::<_, rusqlite::types::Value>(i).map(|v| format!("{v:?}"))).collect::<Result<Vec<_>, _>>()
                    })
                    .unwrap()
                    .map(|r| r.unwrap().join("|"))
                    .collect();
                format!("{t}:{}", rows.join("\n"))
            })
            .collect::<Vec<_>>()
            .join("\n##\n")
    };
    let before = dump(&conn);
    db::migrate(&mut conn).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
    assert_eq!(dump(&conn), before, "migration 4 touched no existing table");
    for t in ["discovery_scans", "discovery_observations"] {
        assert_eq!(conn.query_row("SELECT count(*) FROM sqlite_master WHERE name = ?1", [t], |r| r.get::<_, i64>(0)).unwrap(), 1, "{t}");
    }
    // Repeated startup and repeated scanning do not disturb it.
    db::migrate(&mut conn).unwrap();
    let env = Env::new();
    super::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    super::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
    assert_eq!(conn.query_row("SELECT count(*) FROM project_preferences", [], |r| r.get::<_, i64>(0)).unwrap(), 7);
}

#[test]
fn a_failed_migration_4_rolls_back_cleanly() {
    fn broken(tx: &rusqlite::Transaction) -> crate::error::AppResult<()> {
        tx.execute_batch(include_str!("../../migrations/0004_discovery.sql"))?;
        Err(crate::error::AppError::new("INJECTED_FAILURE", "crash after creating the tables"))
    }
    let mut conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    let list = [
        db::Migration::Sql(include_str!("../../migrations/0001_init.sql")),
        db::Migration::Rust(|tx| {
            tx.execute_batch(include_str!("../../migrations/0002_catalog_registry.sql"))?;
            crate::catalog::migrate_phase1a_data(tx)
        }),
        db::Migration::Sql(include_str!("../../migrations/0003_scoped_suggestion_log.sql")),
        db::Migration::Rust(broken),
    ];
    assert_eq!(db::migrate_with(&mut conn, &list).unwrap_err().code, "INJECTED_FAILURE");
    assert_eq!(db::user_version(&conn).unwrap(), 3);
    assert_eq!(conn.query_row("SELECT count(*) FROM sqlite_master WHERE name LIKE 'discovery%'", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
}

#[test]
fn real_databases_from_earlier_phases_gain_the_discovery_tables_and_keep_working() {
    for (fixture, version) in [
        (concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/phase1a-9918e9e.sqlite.fixture"), 1),
        (concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/phase1b-c921932.sqlite.fixture"), 2),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("atm.sqlite");
        fs::copy(fixture, &path).unwrap();
        assert_eq!(db::user_version(&Connection::open(&path).unwrap()).unwrap(), version);
        let conn = db::open_file(&path, Some(&dir.path().join("backups"))).unwrap();
        assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
        catalog::seed_all(&conn).unwrap();
        let env = Env::new();
        super::run_scan(&conn, &env.roots(), &all_auth(None)).unwrap();
        assert!(!store::inventory(&conn).unwrap().observations.is_empty());
        assert!(
            audit::verify(&conn).unwrap().ok,
            "the audit chain of the old database still verifies after migration and a scan (fixture v{version})"
        );
    }
}
