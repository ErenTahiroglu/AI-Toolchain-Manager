//! The final privacy gate for observations. It runs when an observation is built, again before it is written
//! to the database, again when it is read back for the UI or headless output, and in the self-audit. It only
//! admits fields that match a small, closed grammar, so untrusted text from configuration files and arbitrary
//! local paths cannot be persisted by accident.
//!
//! * `location` is either a fixed documented place below `~`, or "PATH girdisi #N · <category> · <program file>".
//!   It never contains an absolute path, an account name or a folder name chosen by the user.
//! * `version` is a plain release number or a short commit id — nothing else.
//! * Free-text fields (evidence, limitations) must not contain absolute paths, control characters or anything
//!   the redactor would change.

use super::types::Observation;
use crate::redact;
use regex::Regex;
use std::sync::OnceLock;

pub const LOCATION_REMOVED: &str = "eski kayıt: konum gizlilik nedeniyle kaldırıldı";
pub const TEXT_REMOVED: &str = "[konum gizlendi]";
/// The only provenance label a version may carry.
pub const VERSION_SOURCES: &[&str] = &["Claude Code kurulum kaydı", "npm paket kaydı", "yetkili referans özeti"];

/// Folders below the user's home where a PATH entry is recognisable and carries no private name.
pub const KNOWN_HOME_DIRS: &[&str] = &[
    ".local/bin",
    ".cargo/bin",
    "go/bin",
    "bin",
    "scoop/shims",
    ".bun/bin",
    ".deno/bin",
    "AppData/Local/Microsoft/WinGet/Links",
    "AppData/Roaming/npm",
];
/// Fixed locations that are documented by Claude Code itself.
pub const KNOWN_HOME_FILES: &[&str] = &[
    ".local/bin/claude",
    ".local/bin/claude.exe",
    ".claude/local/claude",
    ".claude/local/claude.cmd",
    ".claude/local/claude.exe",
    ".claude/local/claude.bat",
];
pub const PATH_CATEGORIES: &[&str] = &["kullanıcı klasörü altında", "sistem veya paket yöneticisi klasörü", "başka bir klasör"];

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid privacy pattern"))
}

/// An absolute path in any common shape: a drive letter, a UNC share, or a path starting at a well-known root.
pub fn contains_absolute_path(text: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r#"(?i)(?:^|[\s(\[\"'=:,;])(?:[a-z]:[\\/]|\\\\[^\s\\]|//[^\s/]|/(?:users|home|var|tmp|etc|opt|usr|private|volumes|mnt|root|library|applications|system|windows|programdata|program files)(?:/|$))"#)
        .is_match(text)
}

fn tool_file(name: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"^[a-z0-9][a-z0-9._-]{0,40}(\.(exe|cmd|bat))?$").is_match(name)
}

/// The closed grammar for `location`.
pub fn is_safe_location(location: &str) -> bool {
    if location == LOCATION_REMOVED {
        return true;
    }
    if let Some(rest) = location.strip_prefix("~/") {
        if KNOWN_HOME_FILES.contains(&rest) {
            return true;
        }
        return rest.rsplit_once('/').map(|(dir, file)| KNOWN_HOME_DIRS.contains(&dir) && tool_file(file)).unwrap_or(false);
    }
    if let Some(rest) = location.strip_prefix("PATH girdisi #") {
        let mut parts = rest.split(" · ");
        let (Some(n), Some(category), Some(file), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else { return false };
        return n.parse::<u32>().map(|n| (1..=64).contains(&n)).unwrap_or(false) && PATH_CATEGORIES.contains(&category) && tool_file(file);
    }
    false
}

/// Documented version forms only: a release number (optionally with a well-known pre-release tag) or a short commit id.
pub fn is_documented_version(v: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    static H: OnceLock<Regex> = OnceLock::new();
    re(&R, r"^v?[0-9]{1,4}(\.[0-9]{1,4}){1,3}(-(alpha|beta|rc|pre|dev|preview|next)(\.?[0-9]{1,3})?)?$").is_match(v)
        || re(&H, r"^[0-9a-f]{7,40}$").is_match(v)
}

pub fn is_safe_project_alias(a: &str) -> bool {
    let b = a.as_bytes();
    (2..=42).contains(&b.len())
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        && b[0] != b'-'
        && b[b.len() - 1] != b'-'
}

/// Free text is clean when it has no control characters, no absolute path, and the redactor would not change it.
pub fn text_is_clean(text: &str) -> bool {
    !text.chars().any(|c| c.is_control())
        && !text.contains("(ad:")
        && !contains_absolute_path(text)
        && redact::sanitize(text, text.chars().count().max(1) + 8) == text
}

/// What is wrong with an observation's fields, as stable codes (empty = safe).
pub fn violations(o: &Observation) -> Vec<&'static str> {
    let mut v = Vec::new();
    if o.location.as_deref().map(|l| !is_safe_location(l)).unwrap_or(false) {
        v.push("location");
    }
    if o.version.as_deref().map(|x| !is_documented_version(x)).unwrap_or(false) {
        v.push("version");
    }
    if o.version_source.as_deref().map(|s| !VERSION_SOURCES.contains(&s)).unwrap_or(false)
        || (o.version.is_none() && o.version_source.is_some())
    {
        v.push("version_source");
    }
    if o.project_alias.as_deref().map(|a| !is_safe_project_alias(a)).unwrap_or(false) {
        v.push("project_alias");
    }
    if !text_is_clean(&o.evidence) {
        v.push("evidence");
    }
    if o.limitations.iter().any(|l| !text_is_clean(l)) {
        v.push("limitations");
    }
    v
}

/// Replaces every unsafe field by a safe placeholder and returns how many fields had to be replaced.
pub fn scrub(o: &mut Observation) -> usize {
    let bad = violations(o);
    for field in &bad {
        match *field {
            "location" => o.location = Some(LOCATION_REMOVED.to_string()),
            "version" | "version_source" => {
                o.version = None;
                o.version_source = None;
            }
            "project_alias" => o.project_alias = None,
            "evidence" => o.evidence = scrub_text(&o.evidence),
            "limitations" => o.limitations = o.limitations.iter().map(|l| scrub_text(l)).collect(),
            _ => {}
        }
    }
    bad.len()
}

/// Removes absolute paths and anything the redactor would remove from a free-text field.
pub fn scrub_text(text: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    let no_paths = re(&R, r#"(?i)(?:[a-z]:[\\/][^\s"'<>|]*|\\\\[^\s"'<>|]+|//[^\s"'<>|/]+/[^\s"'<>|]*|/(?:users|home|var|tmp|etc|opt|usr|private|volumes|mnt|root|library|applications|system|windows|programdata)(?:/[^\s"'<>|]*)?)"#)
        .replace_all(text, TEXT_REMOVED);
    redact::sanitize(&strip_server_names(&no_paths), 400)
}

/// Legacy rows (written before these rules) may contain a server name in "(ad: …)" form; the name is dropped.
pub fn strip_server_names(text: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r" \(ad: [^)]*\)").replace_all(text, "").into_owned()
}

/// Strict view of a stored ladder, used by the self-audit: the whole document must be valid closed vocabulary
/// (`Provenance` rejects unknown words) and its identity word is compared with the identity column.
#[derive(serde::Deserialize)]
#[serde(from = "super::types::Provenance")]
pub struct LadderCheck {
    pub identity: String,
    /// Is the stored digest (if any) a plain lower-case hex SHA-256?
    pub digest_ok: bool,
    pub sha256: Option<String>,
}

impl From<super::types::Provenance> for LadderCheck {
    fn from(p: super::types::Provenance) -> Self {
        let sha = p.artifact.sha256.clone();
        LadderCheck {
            identity: p.identity.as_str().to_string(),
            digest_ok: sha.as_deref().map(super::reference::is_hex_digest).unwrap_or(true),
            sha256: sha,
        }
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn locations_follow_a_closed_grammar() {
        for ok in [
            "~/.local/bin/claude.exe",
            "~/.local/bin/claude",
            "~/.local/bin/graphify.exe",
            "~/AppData/Local/Microsoft/WinGet/Links/gitleaks.exe",
            "PATH girdisi #3 · kullanıcı klasörü altında · gitleaks.exe",
            "PATH girdisi #64 · sistem veya paket yöneticisi klasörü · strix",
            LOCATION_REMOVED,
        ] {
            assert!(is_safe_location(ok), "{ok}");
        }
        for bad in [
            r"C:\Users\Other\Tools\gitleaks.exe",
            "/Users/other/bin/gitleaks",
            r"\\server\share\gitleaks.exe",
            "~/Clients/AcmeCorp/bin/gitleaks",
            "~/AppData/Local/Programs/codebase-memory-mcp/codebase-memory-mcp.exe",
            "~/../../etc/passwd",
            "PATH girdisi #0 · başka bir klasör · x",
            "PATH girdisi #65 · başka bir klasör · x",
            "PATH girdisi #3 · C:\\Users\\x · gitleaks.exe",
            "PATH girdisi #3 · başka bir klasör · ../gitleaks",
            "PATH girdisi #3 · başka bir klasör · a b",
            "",
        ] {
            assert!(!is_safe_location(bad), "{bad}");
        }
    }

    #[test]
    fn only_documented_version_forms_pass() {
        for ok in ["1.2.3", "v7.0.0", "6.4.2", "0.1", "1.0.0-beta.2", "2.0.0-rc1", "abc1234", "0123456789abcdef0123456789abcdef01234567"] {
            assert!(is_documented_version(ok), "{ok}");
        }
        for bad in [
            "1.0.0-ghp-SENTINEL",
            "1.0.0+build.secret",
            "ghp_SENTINELSENTINELSENTINELSENTINEL12",
            "latest",
            "../../escape",
            "1.0.0; rm -rf /",
            "my-company-internal-1",
            "1.2.3.4.5",
            "ABCDEF1",
            "",
        ] {
            assert!(!is_documented_version(bad), "{bad}");
        }
    }

    #[test]
    fn absolute_paths_are_recognised_in_all_common_shapes() {
        for bad in [
            r"see C:\Users\x\y",
            "see c:/work/app",
            r"share \\srv\x",
            "in /Users/me/app",
            "(/home/me)",
            "\"/var/tmp/x\"",
            "path=/opt/tool",
            "//server/share/x",
        ] {
            assert!(contains_absolute_path(bad), "{bad}");
        }
        for ok in [
            "~/.claude/settings.json",
            "Kullanıcı ayar dosyası (~/.claude/settings.json)",
            "PATH klasörleri",
            "a/b",
            "plugin@market",
            "7.0.0",
            "https-free text",
        ] {
            assert!(!contains_absolute_path(ok), "{ok}");
        }
    }

    #[test]
    fn scrubbing_removes_paths_and_server_names() {
        let t = scrub_text(r"found at C:\Users\x\bin\tool.exe and /Users/y/z");
        assert!(!t.contains("Users") && t.contains(TEXT_REMOVED));
        assert_eq!(
            strip_server_names("1 MCP sunucu tanımı var (ad: my-private-server). Sunucu çalıştırılmadı."),
            "1 MCP sunucu tanımı var. Sunucu çalıştırılmadı."
        );
    }
}
