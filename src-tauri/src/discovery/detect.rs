//! Read-only detectors. They may check that a named file exists in a named place, and they may read
//! and parse a small set of documented Claude Code configuration files. They never execute a program,
//! start an MCP server or an agent, open a network connection or write anything.
//!
//! Content of configuration files is untrusted data: only a few typed fields are extracted, nothing else
//! is kept, and no file content ever reaches an observation except those fields.

use super::{
    fsx::{self, FinalKind, ReadOutcome, Skip, Walk, MAX_CONFIG_BYTES, MAX_SMALL_BYTES},
    privacy,
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
    /// Folders owned by the operating system or a package manager. Used only to *classify* a PATH folder in a
    /// location label ("system or package manager folder"); the folder's own path is never shown.
    pub system_dirs: Vec<PathBuf>,
    /// Catalog-pinned provenance: the source repository each catalog entry names. Loaded from the catalog by the
    /// caller; the detectors never reach the database themselves.
    pub pins: Vec<CatalogPin>,
}

/// What the catalog pins about an entry's origin: `owner/repo` of its repository, lower-case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPin {
    pub entry_id: String,
    pub repo_slug: String,
}

impl CatalogPin {
    /// `https://github.com/Owner/Repo(.git)` -> `owner/repo`. Anything else yields None.
    pub fn from_repo_url(entry_id: &str, url: &str) -> Option<CatalogPin> {
        normalise_repo(url).map(|repo_slug| CatalogPin { entry_id: entry_id.to_string(), repo_slug })
    }
}

/// Normalises the repository spellings found in package metadata and in the catalog to `owner/repo` (GitHub only).
pub fn normalise_repo(raw: &str) -> Option<String> {
    let mut t = raw.trim().trim_end_matches('/').to_ascii_lowercase();
    for prefix in ["git+", "github:"] {
        if let Some(r) = t.strip_prefix(prefix) {
            t = r.to_string();
        }
    }
    for prefix in
        ["https://github.com/", "http://github.com/", "ssh://git@github.com/", "git://github.com/", "git@github.com:", "www.github.com/"]
    {
        if let Some(r) = t.strip_prefix(prefix) {
            t = r.to_string();
            break;
        }
    }
    let t = t.strip_suffix(".git").unwrap_or(&t).trim_end_matches('/').to_string();
    let (owner, repo) = t.split_once('/')?;
    let ok = |x: &str| !x.is_empty() && x.len() <= 100 && x.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
    (ok(owner) && ok(repo)).then(|| format!("{owner}/{repo}"))
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
        let system_dirs = if cfg!(windows) {
            ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"]
                .iter()
                .filter_map(std::env::var_os)
                .map(PathBuf::from)
                .collect()
        } else {
            ["/usr", "/bin", "/sbin", "/opt", "/System", "/Library", "/Applications"].iter().map(PathBuf::from).collect()
        };
        Roots { platform: platform.to_string(), home, path_entries, system_dirs, pins: Vec::new() }
    }

    /// A label for a PATH folder that carries no private folder name: its position in PATH and a coarse category.
    fn path_label(&self, index: usize, dir: &Path, file: &str) -> String {
        // Windows paths are case-insensitive; compare folded text so `c:\users\x` and `C:\Users\X` are one place.
        let fold = |p: &Path| if self.is_windows() { PathBuf::from(p.to_string_lossy().to_lowercase()) } else { p.to_path_buf() };
        let dir = fold(dir);
        let under = |base: &Path| dir.starts_with(fold(base));
        let category = if self.home.as_deref().map(under).unwrap_or(false) {
            "kullanıcı klasörü altında"
        } else if self.system_dirs.iter().any(|d| under(d)) {
            "sistem veya paket yöneticisi klasörü"
        } else {
            "başka bir klasör"
        };
        format!("PATH girdisi #{} · {} · {}", index + 1, category, file.to_ascii_lowercase())
    }

    /// `~/<fixed documented components>` — only ever built from constants, never from a directory listing.
    fn home_label(rel: &[&str]) -> String {
        format!("~/{}", rel.join("/"))
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
    /// 1-based position of the folder in PATH is `index + 1`.
    index: usize,
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
    for (index, dir) in roots.path_entries.iter().take(MAX_PATH_ENTRIES).enumerate() {
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
                    list.push(Hit { index, dir: dir.clone(), file: candidate, link: is_link });
                    break;
                }
            }
        }
    }
    scan
}

// ---------------------------------------------------------------------------------------------
// Phase 2B: passive provenance (nothing is executed; only metadata files are read)
// ---------------------------------------------------------------------------------------------

/// Catalog entries whose program is distributed as a global npm package: (entry id, npm package name). The package
/// name is the one the project's own README documents (evidence R28); it is not proof of identity by itself.
const NPM_PACKAGES: &[(&str, &str)] = &[("agent-browser", "agent-browser")];
const VERSION_SOURCE_NPM: &str = "npm paket kaydı";

#[derive(Debug, PartialEq, Eq)]
enum RepoClaim {
    Absent,
    NoPin,
    Matches,
    Differs,
}

#[derive(Debug)]
struct NpmRecord {
    name_ok: bool,
    version: Option<String>,
    repo: RepoClaim,
}

enum NpmLookup {
    Record(NpmRecord),
    Missing,
    Unreadable,
}

/// `package.json` of a global npm package next to a PATH hit: Windows keeps `node_modules` beside the shims, macOS
/// keeps it in `<prefix>/lib/node_modules` with the shim in `<prefix>/bin`.
fn npm_lookup(roots: &Roots, hit: &Hit, package: &str, pin: Option<&str>) -> NpmLookup {
    let read = if roots.is_windows() {
        fsx::read_text(&hit.dir, &["node_modules", package, "package.json"], MAX_SMALL_BYTES)
    } else {
        match hit.dir.parent() {
            Some(prefix) => fsx::read_text(prefix, &["lib", "node_modules", package, "package.json"], MAX_SMALL_BYTES),
            None => return NpmLookup::Missing,
        }
    };
    let text = match read {
        ReadOutcome::Text(t) => t,
        ReadOutcome::NotFound => return NpmLookup::Missing,
        ReadOutcome::Skipped(_) => return NpmLookup::Unreadable,
    };
    let Ok(Value::Object(doc)) = serde_json::from_str::<Value>(&text) else { return NpmLookup::Unreadable };
    let name_ok = doc.get("name").and_then(Value::as_str) == Some(package);
    let version = doc.get("version").and_then(Value::as_str).filter(|v| privacy::is_documented_version(v)).map(str::to_string);
    let claimed = match doc.get("repository") {
        Some(Value::String(u)) => normalise_repo(u),
        Some(Value::Object(o)) => o.get("url").and_then(Value::as_str).and_then(normalise_repo),
        _ => None,
    };
    let repo = match (claimed, pin) {
        (None, _) => RepoClaim::Absent,
        (Some(_), None) => RepoClaim::NoPin,
        (Some(c), Some(p)) if c == p => RepoClaim::Matches,
        (Some(_), Some(_)) => RepoClaim::Differs,
    };
    NpmLookup::Record(NpmRecord { name_ok, version, repo })
}

/// Reads the PE header and reports whether an embedded signature block is present. It does not validate it.
pub fn pe_signature(header: &[u8]) -> SignatureBlock {
    let u16_at = |o: usize| header.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |o: usize| header.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if header.get(0..2) != Some(b"MZ") {
        return SignatureBlock::Unreadable;
    }
    let Some(pe) = u32_at(0x3c).map(|v| v as usize) else { return SignatureBlock::Unreadable };
    if header.get(pe..pe + 4) != Some(b"PE\0\0") {
        return SignatureBlock::Unreadable;
    }
    let optional = pe + 24;
    let directory = match u16_at(optional) {
        Some(0x10b) => optional + 0x60,
        Some(0x20b) => optional + 0x70,
        _ => return SignatureBlock::Unreadable,
    };
    // Data directory 4 is the certificate table: (file offset, size).
    match (u32_at(directory + 4 * 8), u32_at(directory + 4 * 8 + 4)) {
        (Some(offset), Some(size)) if offset > 0 && size > 0 => SignatureBlock::PresentUnvalidated,
        (Some(_), Some(_)) => SignatureBlock::Absent,
        _ => SignatureBlock::Unreadable,
    }
}

fn signature_of(roots: &Roots, root: &Path, rel: &[&str]) -> SignatureBlock {
    if !roots.is_windows() || !rel.last().map(|f| f.ends_with(".exe")).unwrap_or(false) {
        return SignatureBlock::NotAssessed;
    }
    match fsx::read_prefix(root, rel, 4096) {
        Ok(bytes) => pe_signature(&bytes),
        Err(_) => SignatureBlock::Unreadable,
    }
}

fn with_signature(mut obs: Observation, roots: &Roots, sig: SignatureBlock) -> Observation {
    obs = obs.signature(sig);
    match sig {
        SignatureBlock::PresentUnvalidated => {
            obs = obs.limit("Dosyada gömülü bir imza bloğu var; imzanın geçerliliği, imzalayan ve dosya özeti doğrulanmadı.")
        }
        SignatureBlock::Absent => obs = obs.limit("Dosyada gömülü imza bloğu yok (imzasız)."),
        SignatureBlock::Unreadable => obs = obs.limit("Dosya başlığı beklenen biçimde okunamadığı için imza durumu belirlenemedi."),
        SignatureBlock::NotAssessed if !roots.is_windows() => obs = obs.not_performed(NotPerformed::SignatureParsingUnsupported),
        SignatureBlock::NotAssessed => {}
    }
    obs
}

/// Builds the evidence ladder for a program found on PATH from the metadata that sits next to it. Several copies with
/// differing records, a package whose own name or source disagrees with the catalog, or conflicting versions are
/// reported as conflicts, not resolved.
fn path_provenance(mut obs: Observation, subject: &str, roots: &Roots, hits: &[Hit]) -> Observation {
    let first = &hits[0];
    let sig = if first.link { SignatureBlock::NotAssessed } else { signature_of(roots, &first.dir, &[first.file.as_str()]) };
    obs = with_signature(obs, roots, sig);
    let Some(package) = NPM_PACKAGES.iter().find(|(e, _)| *e == subject).map(|(_, p)| *p) else {
        return obs
            .identity_state(IdentityState::Unknown)
            .not_performed(NotPerformed::NoPackageDetectorForTool)
            .limit("Bu program için desteklenen bir paket yöneticisi kaydı okuyucusu yok; yalnızca dosya adı eşleşti.");
    };
    let pin = roots.pins.iter().find(|p| p.entry_id == subject).map(|p| p.repo_slug.as_str());
    let lookups: Vec<NpmLookup> = hits.iter().map(|h| npm_lookup(roots, h, package, pin)).collect();
    let records: Vec<&NpmRecord> = lookups.iter().filter_map(|l| if let NpmLookup::Record(r) = l { Some(r) } else { None }).collect();
    if lookups.iter().any(|l| matches!(l, NpmLookup::Unreadable)) {
        obs = obs.limit("Bir npm paket kaydı okunamadı veya bozuk; bu kayıt kanıt sayılmadı.");
    }
    if records.is_empty() {
        return obs
            .identity_state(IdentityState::Unknown)
            .not_performed(NotPerformed::PackageMetadataNotFound)
            .limit("Yanındaki npm paket kaydı bulunamadı; kimlik yalnızca dosya adına dayanıyor.");
    }
    obs = obs.install_record(true).not_performed(NotPerformed::RecordToFileLinkage);
    let name_conflict = records.iter().any(|r| !r.name_ok);
    let source_conflict = records.iter().any(|r| r.repo == RepoClaim::Differs);
    let mut versions: Vec<&str> = records.iter().filter_map(|r| r.version.as_deref()).collect();
    versions.sort_unstable();
    versions.dedup();
    let mixed = hits.len() > 1 && records.len() != hits.len();
    if records.iter().any(|r| r.repo == RepoClaim::Matches) && !source_conflict {
        obs = obs.publisher(Publisher::SelfDeclared);
    }
    obs = if name_conflict || source_conflict {
        obs.identity_state(IdentityState::Conflicted).limit(if name_conflict {
            "Paket kaydındaki ad beklenen paket adıyla uyuşmuyor; kayıt güvenilmedi."
        } else {
            "Paket kaydının bildirdiği kaynak deposu katalogdaki depoyla uyuşmuyor."
        })
    } else if mixed {
        obs.identity_state(IdentityState::Unknown)
            .limit("PATH içindeki kopyaların yalnızca bir kısmının paket kaydı var; hangisinin kullanılacağı belirsiz.")
    } else {
        obs.identity_state(IdentityState::EvidenceAvailable)
            .limit("Paket kaydı paketin kendi beyanıdır ve PATH dosyasına bağlandığı doğrulanmadı; kimlik doğrulanmış sayılmaz.")
    };
    if versions.len() > 1 {
        return obs
            .version_state(VersionState::Conflicted, VersionTrust::None)
            .limit("Paket kayıtları birbiriyle çelişen sürümler bildiriyor.");
    }
    match versions.first() {
        Some(v) if !mixed && !name_conflict && !source_conflict => {
            obs.version(Some((*v).to_string()), VERSION_SOURCE_NPM).version_state(VersionState::Known, VersionTrust::Declared)
        }
        _ => obs.limit("Güvenilir bir sürüm bilgisi elde edilemedi."),
    }
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
                .location(Some(roots.path_label(first.index, &first.dir, &first.file)))
                .evidence(format!(
                    "PATH klasörlerinden birinde '{base}' adlı bir dosya bulundu. Bu yalnızca bir ad eşleşmesidir; dosyanın gerçekten bu araç olduğu doğrulanmadı."
                ))
                .limit("Ad eşleşmesi: aynı adlı başka bir program olabilir.");
            obs = path_provenance(obs, subject, roots, hits);
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
        if let Walk::Found { is_link, .. } = fsx::walk(home, &[".local", "bin", exe], FinalKind::File, true) {
            found_any = true;
            let mut o = Observation::new(AGENT_CLAUDE, &roots.platform, SURFACE_CLAUDE, Method::NativeInstallLocation)
                .presence(Presence::Present)
                .configuration(Configuration::NotApplicable)
                .scope(Scope::Global)
                .confidence(Confidence::Medium)
                .location(Some(Roots::home_label(&[".local", "bin", exe])))
                .evidence("Claude Code'un belgelenmiş yerel kurulum konumunda 'claude' dosyası bulundu. Program çalıştırılmadı.")
                .limit(NO_VERSION_NOTE)
                .limit("Dosyanın varlığı çalıştığını göstermez.")
                .identity_state(IdentityState::Unknown)
                .not_performed(NotPerformed::NoPackageDetectorForTool);
            let sig = if is_link { SignatureBlock::NotAssessed } else { signature_of(roots, home, &[".local", "bin", exe]) };
            o = with_signature(o, roots, sig);
            if is_link {
                o = o.limit("Bu dosya bir sembolik bağlantı (resmî kurulum aracı bunu böyle oluşturur); hedefi izlenmedi.");
            }
            out.push(o);
        }
        // Legacy local npm install created by older versions: ~/.claude/local/claude
        for candidate in candidates("claude", roots) {
            if let Walk::Found { .. } = fsx::walk(home, &[".claude", "local", candidate.as_str()], FinalKind::File, true) {
                found_any = true;
                out.push(
                    Observation::new(AGENT_CLAUDE, &roots.platform, SURFACE_CLAUDE, Method::LegacyLocalInstall)
                        .presence(Presence::Present)
                        .configuration(Configuration::NotApplicable)
                        .scope(Scope::Global)
                        .confidence(Confidence::Low)
                        .location(Some(Roots::home_label(&[".claude", "local", candidate.as_str()])))
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
    /// Used only to look up the cache folder; it must be a safe path component and is never displayed.
    cache_version: Option<String>,
    /// Shown to the user only when it has a documented release/commit form.
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
                    let raw = r.get("version").and_then(Value::as_str);
                    let cache_version = raw.filter(|v| safe_component(v)).map(str::to_string);
                    let version = raw.filter(|v| privacy::is_documented_version(v)).map(str::to_string);
                    records.push(Record {
                        name: name.to_string(),
                        market: market.to_string(),
                        scope: scope_of(r.get("scope").and_then(Value::as_str)),
                        cache_version,
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
    Fetched {
        version: Option<String>,
        conflict: bool,
    },
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
    // Records that name different documented versions contradict each other: report the conflict, not a guess.
    let mut documented: Vec<&str> = matching.iter().filter_map(|r| r.version.as_deref()).collect();
    documented.sort_unstable();
    documented.dedup();
    let conflict = documented.len() > 1;
    for r in &matching {
        if let (Some(root), Some(version)) = (plugins_root, r.cache_version.as_deref()) {
            if safe_component(&r.market) && safe_component(&r.name) {
                if let Walk::Found { .. } = fsx::dir_exists(root, &["cache", &r.market, &r.name, version]) {
                    return Fetch::Fetched { version: if conflict { None } else { r.version.clone() }, conflict };
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
        Fetch::Fetched { .. } => Presence::Present,
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
    // Provenance: a settings declaration is configuration, not provenance. The registry and the cache folder are
    // user-writable, so even a matching record WITH files on disk is only "record corroborated" — never "validated",
    // which is reserved for authenticated evidence bound to the installed artifact (none exists in this phase). A
    // record without installed files is only evidence; sources that disagree are a conflict.
    let has_record = matches!(fetch, Fetch::Fetched { .. } | Fetch::RecordOnly);
    let files_seen = matches!(fetch, Fetch::Fetched { .. });
    let pinned = t.marketplace.is_some();
    let identity_state = if info.other_marketplace && (has_record || info.exact != Declared::NotListed) {
        IdentityState::Conflicted
    } else if pinned && files_seen {
        IdentityState::RecordCorroborated
    } else if has_record {
        IdentityState::EvidenceAvailable
    } else if matches!(registry, Registry::NotLoaded) {
        IdentityState::NotAssessed
    } else {
        IdentityState::Unknown
    };
    let identity = identity_state == IdentityState::RecordCorroborated;
    let (version, version_conflict) = match &fetch {
        Fetch::Fetched { version, conflict } => (version.clone(), *conflict),
        _ => (None, false),
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
        .identity_state(identity_state)
        .install_record(has_record)
        .installed_files_seen(files_seen)
        .publisher(if has_record && pinned {
            Publisher::Marketplace
        } else if has_record {
            Publisher::SelfDeclared
        } else {
            Publisher::None
        })
        .version(version, "Claude Code kurulum kaydı")
        .version_state(if version_conflict { VersionState::Conflicted } else { VersionState::Unknown }, VersionTrust::InstallerRecorded)
        .evidence(evidence)
        .limit("Çalışan bir Claude Code oturumunda yüklü olup olmadığı bilinmiyor; işlev test edilmedi.");
    if t.marketplace.is_none() {
        obs = obs.limit("Pazar yeri kimliği kataloğumuzda doğrulanmadı: yalnızca eklenti adı eşleşti.");
    }
    if version_conflict {
        obs = obs.limit("Kurulum kayıtları birbiriyle çelişen sürümler bildiriyor; sürüm gösterilmedi.");
    }
    if identity_state == IdentityState::RecordCorroborated {
        obs = obs.limit("Kurulum kaydı katalogdaki pazar yeriyle uyuşuyor ve dosyalar önbellekte görünüyor; kayıt ve klasör yerel olarak düzenlenebilir ve kurulu içeriğin kimliği (özet/imza) doğrulanmadı.");
    } else if has_record && !files_seen {
        obs = obs.limit("Kurulum kaydı var ama kurulu dosyalar önbellekte görünmüyor; kayıt tek başına kimlik kanıtı sayılmaz.");
    }
    if identity_state == IdentityState::Conflicted {
        obs = obs.limit("Aynı eklenti adı için birbiriyle uyuşmayan kaynaklar var; kimlik çelişkili sayıldı.");
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
/// Only the NUMBER of matching definitions is returned: server names are user-chosen text and are never kept.
fn mcp_matches(servers: &Value) -> (usize, usize) {
    let Some(map) = servers.as_object() else { return (0, 0) };
    let mut matched = 0usize;
    for (name, def) in map {
        let command = def.get("command").and_then(Value::as_str).map(basename_without_ext).unwrap_or_default();
        if name.to_ascii_lowercase().contains("codebase-memory") || command == MCP_ENTRY {
            matched += 1;
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
                if matched == 0 {
                    obs = obs
                        .configuration(Configuration::NotConfigured)
                        .confidence(Confidence::Medium)
                        .evidence(format!("{label} okundu: {total} MCP sunucusu tanımlı, katalogdaki araçla eşleşen yok."));
                } else {
                    obs = obs
                        .configuration(Configuration::Configured)
                        .confidence(Confidence::Medium)
                        .evidence(format!(
                            "{label} içinde katalogdaki araçla eşleşen {matched} MCP sunucu tanımı var. Sunucu çalıştırılmadı."
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
