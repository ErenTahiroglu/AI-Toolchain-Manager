//! Strict, offline GitHub repository URL parser (Phase 0 §4: canonical `https://github.com/{owner}/{repo}` only).
//! It never resolves, fetches or follows anything; it only decides whether a string is acceptable
//! and derives the canonical form. Anything ambiguous is rejected rather than "fixed".

use crate::error::{AppError, AppResult};
use serde::Serialize;

const MAX_URL_CHARS: usize = 200;

/// Paths under github.com that are site pages, not user/organisation names. A link such as
/// `https://github.com/settings/profile` must not be mistaken for a repository.
const RESERVED_OWNERS: &[&str] = &[
    "about",
    "apps",
    "codespaces",
    "collections",
    "contact",
    "customer-stories",
    "enterprise",
    "events",
    "explore",
    "features",
    "issues",
    "login",
    "logout",
    "marketplace",
    "new",
    "notifications",
    "orgs",
    "pricing",
    "pulls",
    "readme",
    "search",
    "security",
    "settings",
    "signup",
    "sponsors",
    "team",
    "topics",
    "trending",
    "users",
    "site",
    "git-guides",
    "resources",
    "partners",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedRepo {
    pub owner: String,
    pub repo: String,
    /// Always `https://github.com/{owner}/{repo}` with the case the user typed.
    pub canonical_url: String,
    /// Lower-case `owner/repo`; GitHub names are case-insensitive, so this is the duplicate key.
    pub source_key: String,
}

fn err(code: &'static str, message: &str) -> AppError {
    AppError::new(code, message)
}

fn valid_owner(owner: &str) -> bool {
    let b = owner.as_bytes();
    !b.is_empty()
        && b.len() <= 39
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'-')
        && b[0] != b'-'
        && b[b.len() - 1] != b'-'
        && !owner.contains("--")
}

fn valid_repo(repo: &str) -> bool {
    !repo.is_empty()
        && repo.len() <= 100
        && repo != "."
        && repo != ".."
        && !repo.ends_with(".git")
        && repo.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

pub fn parse(input: &str) -> AppResult<ParsedRepo> {
    let text = input.trim();
    if text.is_empty() {
        return Err(err("URL_EMPTY", "Bir GitHub bağlantısı yazın."));
    }
    if text.chars().count() > MAX_URL_CHARS {
        return Err(err("URL_TOO_LONG", "Bağlantı çok uzun."));
    }
    // ASCII only and no whitespace/control characters anywhere: blocks homoglyph hosts, zero-width
    // characters, line breaks and tab tricks.
    if !text.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(err("URL_BAD_CHARS", "Bağlantıda boşluk, görünmez veya ASCII dışı karakter var."));
    }
    let lower_prefix = text.get(..8).map(str::to_ascii_lowercase);
    if lower_prefix.as_deref() != Some("https://") {
        return Err(err("URL_NOT_HTTPS", "Yalnızca https:// ile başlayan GitHub bağlantıları kabul edilir."));
    }
    let rest = &text[8..];
    let (authority, path) = match rest.find(['/', '?', '#', '\\']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if authority.contains('@') {
        return Err(err("URL_USERINFO", "Bağlantıda kullanıcı adı/parola bölümü olamaz."));
    }
    if authority.contains(':') {
        return Err(err("URL_PORT", "Bağlantıda port numarası olamaz."));
    }
    if !authority.eq_ignore_ascii_case("github.com") {
        return Err(err("URL_HOST_NOT_GITHUB", "Yalnızca github.com adresi kabul edilir."));
    }
    if path.contains(['?', '#']) {
        return Err(err("URL_QUERY_FRAGMENT", "Bağlantıda ? veya # bölümü olamaz."));
    }
    if path.contains('\\') {
        return Err(err("URL_BAD_CHARS", "Bağlantıda ters eğik çizgi olamaz."));
    }
    if path.contains('%') {
        return Err(err("URL_ENCODED", "Kodlanmış (%) karakterler kabul edilmez."));
    }
    let segments: Vec<&str> = path.trim_start_matches('/').trim_end_matches('/').split('/').collect();
    if segments.len() != 2 || segments.iter().any(|s| s.is_empty()) {
        return Err(err("URL_EXTRA_PATH", "Bağlantı tam olarak https://github.com/sahip/depo biçiminde olmalı."));
    }
    let owner = segments[0];
    let mut repo = segments[1];
    // A trailing ".git" is the canonical clone suffix; strip exactly one.
    if let Some(stripped) = repo.strip_suffix(".git") {
        repo = stripped;
    }
    if !valid_owner(owner) || RESERVED_OWNERS.contains(&owner.to_ascii_lowercase().as_str()) {
        return Err(if valid_owner(owner) {
            err("URL_RESERVED_OWNER", "Bu adres bir depo değil, GitHub'ın kendi sayfalarından biri.")
        } else {
            err("URL_BAD_OWNER", "Sahip adı geçerli bir GitHub kullanıcı/kuruluş adı değil.")
        });
    }
    if !valid_repo(repo) {
        return Err(err("URL_BAD_REPO", "Depo adı geçerli değil."));
    }
    Ok(ParsedRepo {
        owner: owner.to_string(),
        repo: repo.to_string(),
        canonical_url: format!("https://github.com/{owner}/{repo}"),
        source_key: format!("{}/{}", owner.to_ascii_lowercase(), repo.to_ascii_lowercase()),
    })
}

/// Stable identifier for a user-added entry: `owner-repo`, lowercase, `[a-z0-9-]` only, at most 64 characters.
pub fn entry_slug(parsed: &ParsedRepo) -> String {
    let raw = format!("{}-{}", parsed.owner, parsed.repo).to_ascii_lowercase();
    let mut out = String::new();
    for c in raw.chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '-' };
        if c == '-' && out.ends_with('-') {
            continue;
        }
        out.push(c);
    }
    let trimmed = out.trim_matches('-');
    let mut slug: String = trimmed.chars().take(60).collect();
    slug = slug.trim_end_matches('-').to_string();
    if slug.len() < 2 {
        slug.push_str("-x");
    }
    slug
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(input: &str) -> &'static str {
        parse(input).unwrap_err().code
    }

    #[test]
    fn accepts_canonical_urls_and_normalises() {
        let p = parse("https://github.com/vercel-labs/agent-browser").unwrap();
        assert_eq!((p.owner.as_str(), p.repo.as_str()), ("vercel-labs", "agent-browser"));
        assert_eq!(p.source_key, "vercel-labs/agent-browser");
        for same in [
            "  https://github.com/Vercel-Labs/Agent-Browser/ ",
            "HTTPS://GITHUB.COM/vercel-labs/agent-browser.git",
            "https://github.com/vercel-labs/agent-browser/",
        ] {
            assert_eq!(parse(same).unwrap().source_key, "vercel-labs/agent-browser", "{same}");
        }
        assert_eq!(parse("https://github.com/usestrix/strix").unwrap().canonical_url, "https://github.com/usestrix/strix");
    }

    #[test]
    fn rejects_non_https_and_other_hosts() {
        for bad in [
            "http://github.com/a/b",
            "git://github.com/a/b",
            "ssh://git@github.com/a/b",
            "git@github.com:a/b.git",
            "github.com/a/b",
            "ftp://github.com/a/b",
            "//github.com/a/b",
            "javascript:alert(1)",
            "file:///C:/x",
            "data:text/html,x",
        ] {
            assert!(parse(bad).is_err(), "accepted {bad}");
        }
        assert_eq!(code("http://github.com/a/b"), "URL_NOT_HTTPS");
        for bad in [
            "https://gitlab.com/a/b",
            "https://www.github.com/a/b",
            "https://github.com.evil.example/a/b",
            "https://evilgithub.com/a/b",
            "https://github.co/a/b",
            "https://githab.com/a/b",
            "https://raw.githubusercontent.com/a/b",
            "https://gist.github.com/a/b",
            "https://github.com.attacker.invalid/a/b",
            "https://1.2.3.4/a/b",
            "https://[::1]/a/b",
            "https://localhost/a/b",
        ] {
            assert!(matches!(code(bad), "URL_HOST_NOT_GITHUB" | "URL_PORT"), "{bad} -> {}", code(bad));
        }
    }

    #[test]
    fn rejects_credentials_ports_queries_and_encoding_tricks() {
        assert_eq!(code("https://user:pw@github.com/a/b"), "URL_USERINFO");
        assert_eq!(code("https://github.com@evil.example/a/b"), "URL_USERINFO");
        assert_eq!(code("https://github.com:8443/a/b"), "URL_PORT");
        assert_eq!(code("https://github.com/a/b?tab=readme"), "URL_QUERY_FRAGMENT");
        assert_eq!(code("https://github.com/a/b#readme"), "URL_QUERY_FRAGMENT");
        assert_eq!(code("https://github.com?x=/a/b"), "URL_QUERY_FRAGMENT");
        assert_eq!(code("https://github.com/a/%2e%2e"), "URL_ENCODED");
        assert_eq!(code("https://github.com/a%2fb/c"), "URL_ENCODED");
        assert_eq!(code("https://github.com\\a\\b"), "URL_BAD_CHARS");
        assert_eq!(code("https://github.com/a\\b"), "URL_BAD_CHARS");
    }

    #[test]
    fn rejects_traversal_extra_paths_and_misleading_pages() {
        for bad in [
            "https://github.com/a/b/tree/main/src",
            "https://github.com/a/b/blob/main/install.sh",
            "https://github.com/a/b/releases",
            "https://github.com/a/b/archive/main.zip",
            "https://github.com/a/b/raw/main/x.sh",
            "https://github.com/a",
            "https://github.com/",
            "https://github.com",
            "https://github.com//b",
            "https://github.com/a//b",
            "https://github.com/a/../b",
            "https://github.com/a/b/../c",
            "https://github.com/a/./b",
        ] {
            assert!(parse(bad).is_err(), "accepted {bad}");
        }
        assert_eq!(code("https://github.com/a/b/tree/main"), "URL_EXTRA_PATH");
        assert_eq!(code("https://github.com/settings/profile"), "URL_RESERVED_OWNER");
        assert_eq!(code("https://github.com/orgs/acme"), "URL_RESERVED_OWNER");
        assert_eq!(code("https://github.com/marketplace/actions"), "URL_RESERVED_OWNER");
    }

    #[test]
    fn rejects_malformed_names_and_unsafe_characters() {
        for (url, expected) in [
            ("https://github.com/-a/b", "URL_BAD_OWNER"),
            ("https://github.com/a-/b", "URL_BAD_OWNER"),
            ("https://github.com/a--b/c", "URL_BAD_OWNER"),
            ("https://github.com/a_b/c", "URL_BAD_OWNER"),
            ("https://github.com/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/c", "URL_BAD_OWNER"),
            ("https://github.com/a/..", "URL_BAD_REPO"),
            ("https://github.com/a/b c", "URL_BAD_CHARS"),
            ("https://github.com/a/b$c", "URL_BAD_REPO"),
            ("https://github.com/a/b;rm -rf", "URL_BAD_CHARS"),
            ("https://github.com/a/b|calc", "URL_BAD_REPO"),
            ("https://github.com/a/b`id`", "URL_BAD_REPO"),
            ("https://github.com/a/b.git.git", "URL_BAD_REPO"),
            ("https://github.com/ａ/b", "URL_BAD_CHARS"),
            ("https://github.com/a/b\u{200b}", "URL_BAD_CHARS"),
            ("https://github.com/a/b\nhttps://evil.example", "URL_BAD_CHARS"),
            ("", "URL_EMPTY"),
            ("   ", "URL_EMPTY"),
        ] {
            assert_eq!(code(url), expected, "{url:?}");
        }
        let long = format!("https://github.com/a/{}", "b".repeat(300));
        assert_eq!(code(&long), "URL_TOO_LONG");
    }

    #[test]
    fn slugs_are_safe_identifiers() {
        let p = parse("https://github.com/Some-Org/My.Repo_Name").unwrap();
        assert_eq!(entry_slug(&p), "some-org-my-repo-name");
        let long = parse(&format!("https://github.com/{}/{}", "a".repeat(39), "b".repeat(90))).unwrap();
        assert!(entry_slug(&long).len() <= 64);
        assert!(entry_slug(&long).bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'));
    }
}
