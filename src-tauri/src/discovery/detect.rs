//! Read-only detectors. They may check that a named file exists in a named place, and they may read
//! and parse a small set of documented Claude Code configuration files. They never execute a program,
//! start an MCP server or an agent, open a network connection or write anything.
//!
//! Content of configuration files is untrusted data: only a few typed fields are extracted, nothing else
//! is kept, and no file content ever reaches an observation except those fields.

use super::{
    fsx::{self, FinalKind, ReadOutcome, Skip, Walk, MAX_CONFIG_BYTES, MAX_SMALL_BYTES},
    types::*,
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub const PLATFORM_WINDOWS: &str = "windows-x64";
pub const PLATFORM_MACOS: &str = "macos-arm64";
pub const PLATFORM_UNSUPPORTED: &str = "unsupported";

const SURFACE_CLAUDE: &str = "claude-code-terminal";
const SURFACE_CLI: &str = "local-cli";
const SURFACE_ANTIGRAVITY_IDE: &str = "antigravity-ide";
const AGENT_CLAUDE: &str = "agent:claude-code";
const AGENT_ANTIGRAVITY_IDE: &str = "agent:antigravity-ide";
const MAX_PATH_ENTRIES: usize = 64;
const MAX_HITS_PER_TOOL: usize = 3;

/// Command names as documented in each project's own README (checked 2026-10-17).
const CLI_TOOLS: &[(&str, &str)] = &[
    ("gitleaks", "gitleaks"),
    ("betterleaks", "betterleaks"),
    ("agent-browser", "agent-browser"),
    ("strix", "strix"),
    ("markitdown", "markitdown"),
    ("graphify", "graphify"),
    ("codebase-memory-mcp", "codebase-memory-mcp"),
];

/// Entries with no safe, documented detection method yet. They are reported as such, never guessed.
const NO_DETECTOR: &[&str] = &["cloudflare-security-audit-skill", "agentic-awesome-skills"];

struct Tracked {
    entry_id: &'static str,
    name: &'static str,
    /// When known, the exact marketplace that makes the identity certain.
    marketplace: Option<&'static str>,
}

const TRACKED_PLUGINS: &[Tracked] = &[
    Tracked { entry_id: "anthropic-frontend-design", name: "frontend-design", marketplace: Some("claude-plugins-official") },
    Tracked { entry_id: "superpowers", name: "superpowers", marketplace: None },
    Tracked { entry_id: "ponytail", name: "ponytail", marketplace: None },
    Tracked { entry_id: "caveman", name: "caveman", marketplace: None },
];

const MCP_ENTRY: &str = "codebase-memory-mcp";

#[derive(Debug, Clone)]
pub struct Roots {
    pub platform: String,
    pub home: Option<PathBuf>,
    pub path_entries: Vec<PathBuf>,
}

impl Roots {
    pub fn from_environment() -> Roots {
        let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => PLATFORM_WINDOWS,
            ("macos", "aarch64") => PLATFORM_MACOS,
            _ => PLATFORM_UNSUPPORTED,
        };
        let home_var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let home = std::env::var_os(home_var).map(PathBuf::from).filter(|p| p.is_absolute());
        let path_entries = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
        Roots { platform: platform.to_string(), home, path_entries }
    }

    fn is_windows(&self) -> bool {
        self.platform == PLATFORM_WINDOWS
    }
}

#[derive(Debug, Clone)]
pub struct ProjectScan {
    pub alias: String,
    pub dir: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct Authorization {
    /// Look for known program names in PATH folders and in Claude Code's documented install locations.
    pub path_executables: bool,
    /// Read Claude Code's user-level files below the home folder.
    pub user_config: bool,
    /// Read three named files below one project folder the user chose.
    pub project: Option<ProjectScan>,
}

// ---------------------------------------------------------------------------------------------
// JSON documents
// ---------------------------------------------------------------------------------------------

enum Doc {
    Missing,
    Parsed(Value),
    Malformed,
    Skipped(Skip),
}

fn load_json(root: &Path, rel: &[&str], max: u64) -> Doc {
    match fsx::read_text(root, rel, max) {
        ReadOutcome::Text(text) => serde_json::from_str::<Value>(&text).map(Doc::Parsed).unwrap_or(Doc::Malformed),
        ReadOutcome::NotFound => Doc::Missing,
        ReadOutcome::Skipped(s) => Doc::Skipped(s),
    }
}

fn doc_note(doc: &Doc) -> Option<String> {
    match doc {
        Doc::Skipped(s) => Some(format!("Dosya okunmadı: {}.", s.describe())),
        Doc::Malformed => Some("Dosya geçerli JSON değil.".to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// PATH executables
// ---------------------------------------------------------------------------------------------

struct Hit {
    dir: PathBuf,
    file: String,
    link: bool,
}

struct PathScan {
    usable: usize,
    skipped: usize,
    hits: BTreeMap<String, Vec<Hit>>,
}

fn candidates(base: &str, roots: &Roots) -> Vec<String> {
    if roots.is_windows() {
        vec![format!("{base}.exe"), format!("{base}.cmd"), format!("{base}.bat")]
    } else {
        vec![base.to_string()]
    }
}

/// Checks only whether specific file NAMES exist in PATH folders. It never lists a folder, never crawls,
/// skips network and relative entries, and never follows links out of a folder.
fn scan_path(roots: &Roots, bases: &[&str]) -> PathScan {
    let mut scan = PathScan { usable: 0, skipped: 0, hits: BTreeMap::new() };
    for dir in roots.path_entries.iter().take(MAX_PATH_ENTRIES) {
        if dir.as_os_str().is_empty() || !dir.is_absolute() || fsx::is_network_or_device_path(dir) {
            scan.skipped += 1;
            continue;
        }
        match fsx::walk(dir, &[], FinalKind::Dir, false) {
            Walk::Found { .. } => scan.usable += 1,
            Walk::NotFound => continue,
            Walk::Skipped(_) => {
                scan.skipped += 1;
                continue;
            }
        }
        for base in bases {
            let list = scan.hits.entry((*base).to_string()).or_default();
            if list.len() >= MAX_HITS_PER_TOOL {
                continue;
            }
            for candidate in candidates(base, roots) {
                if let Walk::Found { is_link, .. } = fsx::walk(dir, &[candidate.as_str()], FinalKind::File, true) {
                    list.push(Hit { dir: dir.clone(), file: candidate, link: is_link });
                    break;
                }
            }
        }
    }
    scan
}

const NO_VERSION_NOTE: &str =
    "Sürüm öğrenilmedi: kimliği doğrulanmamış bir programı çalıştırmak güvenli sayılmadığı için hiçbir program çalıştırılmadı.";

fn path_observation(subject: &str, surface: &str, base: &str, roots: &Roots, scan: &PathScan) -> Observation {
    let mut obs = Observation::new(subject, &roots.platform, surface, Method::PathExecutable).scope(Scope::Global).limit(NO_VERSION_NOTE);
    match scan.hits.get(base).filter(|h| !h.is_empty()) {
        Some(hits) => {
            let first = &hits[0];
            obs = obs
                .presence(Presence::Present)
                .configuration(Configuration::NotApplicable)
                .confidence(Confidence::Low)
                .location(Some(fsx::collapse_home(&first.dir.join(&first.file), roots.home.as_deref())))
                .evidence(format!(
                    "PATH klasörlerinden birinde '{base}' adlı bir dosya bulundu. Bu yalnızca bir ad eşleşmesidir; dosyanın gerçekten bu araç olduğu doğrulanmadı."
                ))
                .limit("Ad eşleşmesi: aynı adlı başka bir program olabilir.");
            if first.link {
                obs = obs.limit("Dosya bir sembolik bağlantı/yönlendirme; hedefi izlenmedi ve doğrulanmadı.");
            }
            if hits.len() > 1 {
                obs = obs.limit(format!("PATH içinde {} farklı kopya bulundu; hangisinin kullanılacağı bilinmiyor.", hits.len()));
            }
        }
        None if scan.usable == 0 => {
            obs = obs
                .presence(Presence::Unknown)
                .confidence(Confidence::None)
                .evidence("Denetlenebilir hiçbir PATH klasörü yok; bu nedenle bulunup bulunmadığı bilinmiyor.");
        }
        None => {
            obs = obs
                .presence(Presence::Absent)
                .configuration(Configuration::NotApplicable)
                .confidence(Confidence::Low)
                .evidence(format!("'{base}' adlı dosya denetlenen {} PATH klasöründe bulunamadı.", scan.usable))
                .limit("Yalnızca PATH klasörlerine bakıldı; PATH dışına veya başka bir yöntemle kurulmuş olabilir.");
        }
    }
    if scan.skipped > 0 {
        obs = obs.limit(format!("{} PATH girdisi atlandı (ağ yolu, göreli yol, bağlantı veya erişilemeyen klasör).", scan.skipped));
    }
    obs
}

fn claude_install_observations(roots: &Roots, scan: &PathScan) -> Vec<Observation> {
    let mut out = Vec::new();
    let exe = if roots.is_windows() { "claude.exe" } else { "claude" };
    let mut found_any = false;

    if let Some(home) = &roots.home {
        // Documented native-installer location: ~/.local/bin/claude (macOS: a symlink by design) or
        // %USERPROFILE%\.local\bin\claude.exe (Windows).
        if let Walk::Found { path, is_link } = fsx::walk(home, &[".local", "bin", exe], FinalKind::File, true) {
            found_any = true;
            let mut o = Observation::new(AGENT_CLAUDE, &roots.platform, SURFACE_CLAUDE, Method::NativeInstallLocation)
                .presence(Presence::Present)
                .configuration(Configuration::NotApplicable)
                .scope(Scope::Global)
                .confidence(Confidence::Medium)
                .location(Some(fsx::collapse_home(&path, Some(home))))
                .evidence("Claude Code'un belgelenmiş yerel kurulum konumunda 'claude' dosyası bulundu. Program çalıştırılmadı.")
                .limit(NO_VERSION_NOTE)
                .limit("Dosyanın varlığı çalıştığını göstermez.");
            if is_link {
                o = o.limit("Bu dosya bir sembolik bağlantı (resmî kurulum aracı bunu böyle oluşturur); hedefi izlenmedi.");
            }
            out.push(o);
        }
        // Legacy local npm install created by older versions: ~/.claude/local/claude
        for candidate in candidates("claude", roots) {
            if let Walk::Found { path, .. } = fsx::walk(home, &[".claude", "local", candidate.as_str()], FinalKind::File, true) {
                found_any = true;
                out.push(
                    Observation::new(AGENT_CLAUDE, &roots.platform, SURFACE_CLAUDE, Method::LegacyLocalInstall)
                        .presence(Presence::Present)
                        .configuration(Configuration::NotApplicable)
                        .scope(Scope::Global)
                        .confidence(Confidence::Low)
                        .location(Some(fsx::collapse_home(&path, Some(home))))
                        .evidence("Eski sürümlerin oluşturduğu yerel kurulum konumunda 'claude' dosyası bulundu. Program çalıştırılmadı.")
                        .limit(NO_VERSION_NOTE),
                );
                break;
            }
        }
    }

    let mut p = path_observation(AGENT_CLAUDE, SURFACE_CLAUDE, "claude", roots, scan)
        .limit("Claude Code'un VS Code uzantısının kendi özel kopyası bu yöntemle algılanmaz.");
    if p.presence == Presence::Absent && found_any {
        p = p.limit("Başka bir konumda bulundu; PATH'e eklenmemiş olabilir.");
    }
    out.push(p);
    out
}

// ---------------------------------------------------------------------------------------------
// Claude Code settings, plugins and MCP references
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Declared {
    Enabled,
    Disabled,
    NotListed,
}

struct DeclaredInfo {
    exact: Declared,
    /// A plugin with the same name from a different marketplace is listed (a different identity).
    other_marketplace: bool,
}

fn declared_in(doc: &Doc, t: &Tracked) -> DeclaredInfo {
    let mut info = DeclaredInfo { exact: Declared::NotListed, other_marketplace: false };
    let Doc::Parsed(v) = doc else { return info };
    let Some(map) = v.get("enabledPlugins").and_then(Value::as_object) else { return info };
    for (key, value) in map {
        let Some((name, market)) = key.split_once('@') else { continue };
        if name != t.name {
            continue;
        }
        let exact = t.marketplace.map(|m| m == market).unwrap_or(true);
        if exact {
            info.exact = if value.as_bool() == Some(true) { Declared::Enabled } else { Declared::Disabled };
        } else {
            info.other_marketplace = true;
        }
    }
    info
}

struct Record {
    name: String,
    market: String,
    scope: Scope,
    version: Option<String>,
}

enum Registry {
    NotLoaded,
    Missing,
    Malformed,
    Skipped(Skip),
    Loaded(Vec<Record>),
}

fn scope_of(s: Option<&str>) -> Scope {
    match s {
        Some("user") => Scope::Global,
        Some("project") => Scope::Project,
        Some("local") => Scope::Local,
        _ => Scope::Unknown,
    }
}

fn parse_registry(doc: Doc) -> Registry {
    match doc {
        Doc::Missing => Registry::Missing,
        Doc::Malformed => Registry::Malformed,
        Doc::Skipped(s) => Registry::Skipped(s),
        Doc::Parsed(v) => {
            let map = match v.get("plugins").and_then(Value::as_object).or_else(|| v.as_object()) {
                Some(m) => m,
                None => return Registry::Malformed,
            };
            let mut records = Vec::new();
            for (key, value) in map {
                let Some((name, market)) = key.split_once('@') else { continue };
                let list: Vec<&Value> = match value {
                    Value::Array(a) => a.iter().collect(),
                    Value::Object(_) => vec![value],
                    _ => continue,
                };
                for r in list {
                    let version = r.get("version").and_then(Value::as_str).map(str::to_string).filter(|v| is_plain_version(v));
                    records.push(Record {
                        name: name.to_string(),
                        market: market.to_string(),
                        scope: scope_of(r.get("scope").and_then(Value::as_str)),
                        version,
                    });
                }
            }
            Registry::Loaded(records)
        }
    }
}

fn safe_component(s: &str) -> bool {
    !s.is_empty() && s.len() <= 100 && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+'))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Fetch {
    /// A record exists and the version directory is on disk under `plugins/cache/`.
    Fetched(Option<String>),
    /// A record exists but its files were not found in the cache (a local-marketplace plugin may load in place).
    RecordOnly,
    NoRecord,
    Unknown,
}

fn fetch_state(registry: &Registry, plugins_root: Option<&Path>, t: &Tracked, only_scope: Option<Scope>) -> Fetch {
    let Registry::Loaded(records) = registry else {
        return match registry {
            Registry::Missing => Fetch::NoRecord,
            _ => Fetch::Unknown,
        };
    };
    let matching: Vec<&Record> = records
        .iter()
        .filter(|r| {
            r.name == t.name && t.marketplace.map(|m| m == r.market).unwrap_or(true) && only_scope.map(|s| r.scope == s).unwrap_or(true)
        })
        .collect();
    if matching.is_empty() {
        return Fetch::NoRecord;
    }
    for r in &matching {
        if let (Some(root), Some(version)) = (plugins_root, r.version.as_deref()) {
            if safe_component(&r.market) && safe_component(&r.name) && safe_component(version) {
                if let Walk::Found { .. } = fsx::dir_exists(root, &["cache", &r.market, &r.name, version]) {
                    return Fetch::Fetched(Some(version.to_string()));
                }
            }
        }
    }
    Fetch::RecordOnly
}

/// What every plugin observation shares: where we are and what the user-level plugin registry said.
struct PluginCtx<'a> {
    roots: &'a Roots,
    registry: &'a Registry,
    plugins_root: Option<&'a Path>,
}

fn plugin_observation(
    cx: &PluginCtx,
    t: &Tracked,
    scope: Scope,
    settings: &Doc,
    project_alias: Option<&str>,
    settings_label: &str,
) -> Observation {
    let (roots, registry, plugins_root) = (cx.roots, cx.registry, cx.plugins_root);
    let info = declared_in(settings, t);
    // Global scope uses only user-scope install records; project and local scopes see only whether the files
    // exist anywhere on disk, because the registry does not say which project a project-scope install is for.
    let fetch = fetch_state(registry, plugins_root, t, if scope == Scope::Global { Some(Scope::Global) } else { None });

    let presence = match &fetch {
        Fetch::Fetched(_) => Presence::Present,
        Fetch::RecordOnly | Fetch::Unknown => Presence::Unknown,
        Fetch::NoRecord => match registry {
            Registry::NotLoaded => Presence::Unknown,
            _ => Presence::Absent,
        },
    };
    let configuration = match settings {
        Doc::Malformed => Configuration::Malformed,
        Doc::Skipped(_) => Configuration::Unknown,
        _ => match info.exact {
            Declared::Enabled if presence == Presence::Absent => Configuration::ReferencedNotPresent,
            Declared::Enabled => Configuration::Configured,
            Declared::Disabled => Configuration::Disabled,
            Declared::NotListed => Configuration::NotConfigured,
        },
    };
    let identity = t.marketplace.is_some() && (info.exact != Declared::NotListed || matches!(fetch, Fetch::Fetched(_) | Fetch::RecordOnly));
    let version = match &fetch {
        Fetch::Fetched(v) => v.clone(),
        _ => None,
    };
    let confidence = match (presence, identity) {
        (Presence::Present, true) => Confidence::High,
        (Presence::Present, false) => Confidence::Medium,
        (Presence::Absent, _) if matches!(settings, Doc::Parsed(_) | Doc::Missing) => Confidence::Medium,
        _ => Confidence::Low,
    };
    let mut evidence = match (presence, configuration) {
        (Presence::Present, Configuration::Configured) => format!(
            "Eklenti, {settings_label} içinde açık olarak bildirilmiş ve Claude Code'un kurulum kaydı ile önbellekte (diskte) bulundu."
        ),
        (Presence::Present, Configuration::Disabled) => {
            format!("Eklenti diskte bulundu ancak {settings_label} içinde kapalı olarak işaretli.")
        }
        (Presence::Present, _) => {
            "Eklenti dosyaları diskte bulundu, ancak bu kapsamın ayar dosyasında açık olarak bildirilmemiş.".to_string()
        }
        (_, Configuration::ReferencedNotPresent) => format!(
            "Eklenti {settings_label} içinde bildirilmiş, ancak kurulum kaydında/önbellekte bulunamadı (başvuru var, dosyalar yok)."
        ),
        (Presence::Unknown, Configuration::Configured) => {
            format!("Eklenti {settings_label} içinde bildirilmiş; kurulum durumu doğrulanamadı.")
        }
        (Presence::Absent, _) => "Eklenti ne ayar dosyasında ne de kurulum kaydında bulundu.".to_string(),
        _ => "Eklenti durumu kesinleştirilemedi.".to_string(),
    };
    if matches!(fetch, Fetch::RecordOnly) {
        evidence.push_str(" Kurulum kaydı var ama dosyalar önbellekte görünmüyor (yerinde yüklenen bir eklenti olabilir).");
    }
    let mut obs = Observation::new(t.entry_id, &roots.platform, SURFACE_CLAUDE, Method::ClaudePluginDeclaration)
        .presence(presence)
        .configuration(configuration)
        .scope(scope)
        .confidence(confidence)
        .project(project_alias)
        .identity(identity)
        .version(version, "Claude Code kurulum kaydı")
        .evidence(evidence)
        .limit("Çalışan bir Claude Code oturumunda yüklü olup olmadığı bilinmiyor; işlev test edilmedi.");
    if t.marketplace.is_none() {
        obs = obs.limit("Pazar yeri kimliği kataloğumuzda doğrulanmadı: yalnızca eklenti adı eşleşti.");
    }
    if info.other_marketplace {
        obs = obs.limit("Aynı adlı eklenti başka bir pazar yerinden listelenmiş; bu, katalogdaki eklentiyle aynı sayılmadı.");
    }
    if scope != Scope::Global && presence == Presence::Present {
        obs = obs.limit("Dosyalar kullanıcı önbelleğinde; bu projeye özel bir kurulum olarak doğrulanmadı.");
    }
    if let Some(n) = doc_note(settings) {
        obs = obs.limit(n);
    }
    match registry {
        Registry::Malformed => obs = obs.limit("Eklenti kurulum kaydı geçerli JSON değil."),
        Registry::Skipped(s) => obs = obs.limit(format!("Eklenti kurulum kaydı okunmadı: {}.", s.describe())),
        Registry::NotLoaded => obs = obs.limit("Kullanıcı eklenti kayıtları okunmadı (yetki verilmedi)."),
        _ => {}
    }
    obs
}

fn basename_without_ext(command: &str) -> String {
    let base = command.rsplit(['/', '\\']).next().unwrap_or(command).to_ascii_lowercase();
    for ext in [".exe", ".cmd", ".bat"] {
        if let Some(stripped) = base.strip_suffix(ext) {
            return stripped.to_string();
        }
    }
    base
}

/// Matches a configured MCP server to the catalogued one by server name or by the command's file name.
/// Arguments, environment and URLs are never read.
fn mcp_matches(servers: &Value) -> (Vec<String>, usize) {
    let Some(map) = servers.as_object() else { return (Vec::new(), 0) };
    let mut matched = Vec::new();
    for (name, def) in map {
        let command = def.get("command").and_then(Value::as_str).map(basename_without_ext).unwrap_or_default();
        if name.to_ascii_lowercase().contains("codebase-memory") || command == MCP_ENTRY {
            matched.push(name.chars().take(60).collect::<String>());
        }
    }
    (matched, map.len())
}

fn mcp_observation(
    roots: &Roots,
    scope: Scope,
    doc: &Doc,
    servers: Option<&Value>,
    label: &str,
    project_alias: Option<&str>,
) -> Observation {
    let mut obs = Observation::new(MCP_ENTRY, &roots.platform, SURFACE_CLAUDE, Method::ClaudeMcpConfig)
        .scope(scope)
        .project(project_alias)
        .presence(Presence::Unknown)
        .limit("Başvurunun varlığı programın kurulu olduğunu göstermez; sunucu başlatılmadı ve komutun varlığı doğrulanmadı.")
        .limit("Projeye ait .mcp.json sunucuları kullanıcı onayı gerektirebilir; onay ve devre dışı bırakma listeleri denetlenmedi.");
    match doc {
        Doc::Parsed(_) => match servers {
            Some(s) => {
                let (matched, total) = mcp_matches(s);
                if matched.is_empty() {
                    obs = obs
                        .configuration(Configuration::NotConfigured)
                        .confidence(Confidence::Medium)
                        .evidence(format!("{label} okundu: {total} MCP sunucusu tanımlı, katalogdaki araçla eşleşen yok."));
                } else {
                    obs = obs
                        .configuration(Configuration::Configured)
                        .confidence(Confidence::Medium)
                        .evidence(format!(
                            "{label} içinde katalogdaki araçla eşleşen {} MCP sunucu tanımı var (ad: {}). Sunucu çalıştırılmadı.",
                            matched.len(),
                            matched.join(", ")
                        ))
                        .limit("Eşleşme sunucu adına/komut dosyası adına dayanır; kimliği doğrulanmadı.");
                }
            }
            None => {
                obs = obs
                    .configuration(Configuration::NotConfigured)
                    .confidence(Confidence::Medium)
                    .evidence(format!("{label} okundu: MCP sunucu tanımı yok."));
            }
        },
        Doc::Missing => {
            obs = obs.configuration(Configuration::NotConfigured).confidence(Confidence::Medium).evidence(format!("{label} bulunamadı."))
        }
        Doc::Malformed => {
            obs = obs.configuration(Configuration::Malformed).confidence(Confidence::Low).evidence(format!("{label} geçerli JSON değil."))
        }
        Doc::Skipped(s) => {
            obs = obs
                .configuration(Configuration::Unknown)
                .confidence(Confidence::None)
                .evidence(format!("{label} okunmadı: {}.", s.describe()))
        }
    }
    obs
}

fn settings_file_observation(roots: &Roots, scope: Scope, doc: &Doc, label: &str, project_alias: Option<&str>) -> Observation {
    let mut obs = Observation::new(AGENT_CLAUDE, &roots.platform, SURFACE_CLAUDE, Method::ClaudeSettingsFile)
        .scope(scope)
        .project(project_alias)
        .presence(Presence::Unknown)
        .limit("Yapılandırma dosyasının varlığı Claude Code'un kurulu olduğunu göstermez.")
        .limit("Dosyanın içeriği saklanmadı; yalnızca geçerli olup olmadığına ve eklenti/MCP bildirimlerine bakıldı.");
    match doc {
        Doc::Parsed(v) if v.is_object() => {
            obs = obs
                .configuration(Configuration::Configured)
                .confidence(Confidence::Medium)
                .evidence(format!("{label} bulundu ve geçerli JSON."));
        }
        Doc::Parsed(_) | Doc::Malformed => {
            obs = obs
                .configuration(Configuration::Malformed)
                .confidence(Confidence::Low)
                .evidence(format!("{label} bulundu ama beklenen biçimde değil."));
        }
        Doc::Missing => {
            obs = obs.configuration(Configuration::NotConfigured).confidence(Confidence::Medium).evidence(format!("{label} yok."))
        }
        Doc::Skipped(s) => {
            obs = obs
                .configuration(Configuration::Unknown)
                .confidence(Confidence::None)
                .evidence(format!("{label} okunmadı: {}.", s.describe()))
        }
    }
    obs
}

fn norm_project_key(s: &str, windows: bool) -> String {
    let mut n = s.replace('\\', "/");
    while n.len() > 1 && n.ends_with('/') {
        n.pop();
    }
    if windows {
        n.to_ascii_lowercase()
    } else {
        n
    }
}

// ---------------------------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------------------------

fn antigravity_ide_observation(roots: &Roots) -> Observation {
    Observation::new(AGENT_ANTIGRAVITY_IDE, &roots.platform, SURFACE_ANTIGRAVITY_IDE, Method::NotDocumented)
        .presence(Presence::Unknown)
        .configuration(Configuration::Unknown)
        .confidence(Confidence::None)
        .evidence("Antigravity masaüstü IDE'si için doğrulanmış, belgelenmiş bir kurulum veya yapılandırma algılama yöntemi yok; bu yüzden konum tahmin edilmedi.")
        .limit("Antigravity CLI ayrı bir üründür; onun varlığı IDE için kanıt sayılmaz.")
        .limit("Uyumluluk iddiası yapılmadı.")
}

fn no_detector_observation(roots: &Roots, entry: &str) -> Observation {
    Observation::new(entry, &roots.platform, SURFACE_CLAUDE, Method::NoDetector)
        .presence(Presence::Unknown)
        .confidence(Confidence::None)
        .evidence("Bu kayıt için güvenli ve belgelenmiş bir algılama yöntemi henüz eklenmedi; tahmin edilmedi.")
}

pub fn detect_all(roots: &Roots, auth: &Authorization) -> Vec<Observation> {
    let mut out: Vec<Observation> = Vec::new();

    if roots.platform == PLATFORM_UNSUPPORTED {
        let mut subjects: Vec<(&str, &str)> = vec![(AGENT_CLAUDE, SURFACE_CLAUDE), (AGENT_ANTIGRAVITY_IDE, SURFACE_ANTIGRAVITY_IDE)];
        subjects.extend(CLI_TOOLS.iter().map(|(e, _)| (*e, SURFACE_CLI)));
        subjects.extend(TRACKED_PLUGINS.iter().map(|t| (t.entry_id, SURFACE_CLAUDE)));
        for (subject, surface) in subjects {
            out.push(
                Observation::new(subject, &roots.platform, surface, Method::UnsupportedPlatform)
                    .presence(Presence::Unsupported)
                    .configuration(Configuration::Unknown)
                    .confidence(Confidence::None)
                    .evidence("Bu işletim sistemi/mimari desteklenen hedefler (Windows x64, macOS Apple Silicon) arasında değil; hiçbir şey okunmadı."),
            );
        }
        return out.into_iter().map(Observation::finalize).collect();
    }

    out.push(antigravity_ide_observation(roots));
    for entry in NO_DETECTOR {
        out.push(no_detector_observation(roots, entry));
    }

    if auth.path_executables {
        let mut bases: Vec<&str> = CLI_TOOLS.iter().map(|(_, b)| *b).collect();
        bases.push("claude");
        let scan = scan_path(roots, &bases);
        out.extend(claude_install_observations(roots, &scan));
        for (entry, base) in CLI_TOOLS {
            out.push(path_observation(entry, SURFACE_CLI, base, roots, &scan));
        }
    }

    // User-level Claude Code files.
    let user_home = if auth.user_config { roots.home.as_deref() } else { None };
    let user_settings = user_home.map(|h| load_json(h, &[".claude", "settings.json"], MAX_SMALL_BYTES));
    let claude_json = user_home.map(|h| load_json(h, &[".claude.json"], MAX_CONFIG_BYTES));
    let registry = match user_home {
        Some(h) => parse_registry(load_json(h, &[".claude", "plugins", "installed_plugins.json"], MAX_SMALL_BYTES)),
        None => Registry::NotLoaded,
    };
    let plugins_root: Option<PathBuf> = user_home.map(|h| h.join(".claude").join("plugins"));
    if auth.user_config && user_home.is_none() {
        out.push(
            Observation::new(AGENT_CLAUDE, &roots.platform, SURFACE_CLAUDE, Method::ClaudeSettingsFile)
                .scope(Scope::Global)
                .evidence("Kullanıcı klasörü belirlenemedi; Claude Code kullanıcı dosyaları okunmadı."),
        );
    }

    // Project files (only the three named files below the folder the user chose).
    let project = auth.project.as_ref().map(|p| {
        (
            p,
            load_json(&p.dir, &[".claude", "settings.json"], MAX_SMALL_BYTES),
            load_json(&p.dir, &[".claude", "settings.local.json"], MAX_SMALL_BYTES),
            load_json(&p.dir, &[".mcp.json"], MAX_SMALL_BYTES),
        )
    });

    if let Some(doc) = &user_settings {
        out.push(settings_file_observation(roots, Scope::Global, doc, "Kullanıcı ayar dosyası (~/.claude/settings.json)", None));
    }
    if let Some((p, shared, local, _)) = &project {
        out.push(settings_file_observation(roots, Scope::Project, shared, "Proje ayar dosyası (.claude/settings.json)", Some(&p.alias)));
        out.push(settings_file_observation(
            roots,
            Scope::Local,
            local,
            "Projeye özel yerel ayar dosyası (.claude/settings.local.json)",
            Some(&p.alias),
        ));
    }

    let cx = PluginCtx { roots, registry: &registry, plugins_root: plugins_root.as_deref() };
    for t in TRACKED_PLUGINS {
        if let Some(doc) = &user_settings {
            out.push(plugin_observation(&cx, t, Scope::Global, doc, None, "kullanıcı ayarları"));
        }
        if let Some((p, shared, local, _)) = &project {
            out.push(plugin_observation(&cx, t, Scope::Project, shared, Some(&p.alias), "proje ayarları"));
            out.push(plugin_observation(&cx, t, Scope::Local, local, Some(&p.alias), "yerel proje ayarları"));
        }
    }

    // MCP references.
    if let Some(doc) = &claude_json {
        let servers = match doc {
            Doc::Parsed(v) => v.get("mcpServers"),
            _ => None,
        };
        out.push(mcp_observation(roots, Scope::Global, doc, servers, "Kullanıcı MCP yapılandırması (~/.claude.json)", None));
        if let Some((p, _, _, _)) = &project {
            let wanted = norm_project_key(&p.dir.to_string_lossy(), roots.is_windows());
            let local_servers = match doc {
                Doc::Parsed(v) => v
                    .get("projects")
                    .and_then(Value::as_object)
                    .and_then(|m| m.iter().find(|(k, _)| norm_project_key(k, roots.is_windows()) == wanted))
                    .and_then(|(_, entry)| entry.get("mcpServers")),
                _ => None,
            };
            out.push(mcp_observation(
                roots,
                Scope::Local,
                doc,
                local_servers,
                "Bu projeye özel yerel MCP yapılandırması (~/.claude.json)",
                Some(&p.alias),
            ));
        }
    }
    if let Some((p, _, _, mcp)) = &project {
        let servers = match mcp {
            Doc::Parsed(v) => v.get("mcpServers"),
            _ => None,
        };
        out.push(mcp_observation(roots, Scope::Project, mcp, servers, "Proje MCP dosyası (.mcp.json)", Some(&p.alias)));
    }

    out.into_iter().map(Observation::finalize).collect()
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn command_basenames_ignore_directories_and_windows_extensions() {
        assert_eq!(basename_without_ext(r"C:\Tools\Codebase-Memory-MCP.EXE"), "codebase-memory-mcp");
        assert_eq!(basename_without_ext("/usr/local/bin/codebase-memory-mcp"), "codebase-memory-mcp");
        assert_eq!(basename_without_ext("npx"), "npx");
    }

    #[test]
    fn project_keys_compare_across_separators() {
        assert_eq!(norm_project_key(r"C:\Work\App\", true), norm_project_key("c:/work/app", true));
        assert_ne!(norm_project_key("/Users/a/App", false), norm_project_key("/users/a/app", false));
    }
}
