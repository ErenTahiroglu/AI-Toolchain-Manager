//! Reference digests and the validation policy (version 1). Read-only data handling; nothing is fetched or executed.
//!
//! # The trust contract
//!
//! A SHA-256 **fingerprint** computed from a local file says what bytes are there. It says nothing about where they came
//! from. Identity can only be *verified* by comparing the fingerprint of the exact artifact with a **reference digest
//! that was authenticated independently of the file and of every locally editable record**. This policy therefore
//! separates three kinds of reference:
//!
//! * `stored` — rows in the local database (`reference_digests`), which also hold values copied from a README,
//!   `package.json`, a repository or typed by a user. The database is user-writable, so these are **never** trusted,
//!   whatever a row says (the table's CHECK constraint does not even allow "authenticated").
//! * `built_in` — digests compiled into the application and covered by code review ([`BUILT_IN_AUTHENTICATED`]). This is
//!   the only source that can authenticate. It is **empty** in this phase: no authoritative, independently signed digest
//!   evidence for an installed file exists in the catalog, and the application does not fetch any.
//! * nothing — then the answer stays UNKNOWN / evidence available.
//!
//! A digest applies to one kind of artifact. An **archive** or installer digest is never the expected digest of the
//! executable extracted from it, and one file digest does not authenticate a plugin directory or a skill collection:
//! only `executable`, `script` and `plugin_file` references (an exact file) are compared with an exact file.
//!
//! Evidence is bound to the policy version and to the exact reference set it was judged against (`epoch`); when either
//! changes, previously stored verdicts are invalidated when read.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

/// Bump when the matching rules change: stored verdicts made under another version are invalidated on read.
pub const POLICY_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// A download archive (zip, tar.gz, …). Never the expected digest of an extracted file.
    Archive,
    Installer,
    /// An exact installed program file.
    Executable,
    /// An exact installed script file.
    Script,
    /// One file of a plugin. Says nothing about the rest of the plugin.
    PluginFile,
    /// A whole package or directory tree. Not comparable with a single file.
    Package,
}

impl ArtifactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ArtifactKind::Archive => "archive",
            ArtifactKind::Installer => "installer",
            ArtifactKind::Executable => "executable",
            ArtifactKind::Script => "script",
            ArtifactKind::PluginFile => "plugin_file",
            ArtifactKind::Package => "package",
        }
    }

    pub fn parse(s: &str) -> Option<ArtifactKind> {
        [
            ArtifactKind::Archive,
            ArtifactKind::Installer,
            ArtifactKind::Executable,
            ArtifactKind::Script,
            ArtifactKind::PluginFile,
            ArtifactKind::Package,
        ]
        .into_iter()
        .find(|k| k.as_str() == s)
    }

    /// May a reference of this kind be compared with the fingerprint of one installed file?
    pub fn is_exact_file(self) -> bool {
        matches!(self, ArtifactKind::Executable | ArtifactKind::Script | ArtifactKind::PluginFile)
    }
}

/// One reference digest with everything the contract requires to judge it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceDigest {
    pub entry_id: String,
    pub kind: ArtifactKind,
    /// File format, e.g. `pe-exe`, `mach-o`, `js`, `zip`.
    pub format: String,
    pub platform_target: String,
    pub version: String,
    pub sha256: String,
    /// Where the value came from (sanitized label, never a path or URL with credentials).
    pub source: String,
}

/// Compiled-in, code-reviewed, independently authenticated references. EMPTY in this phase (see the module docs).
pub const BUILT_IN_AUTHENTICATED: &[BuiltIn] = &[];

/// Static form of an authenticated reference (so the list can be a `const`).
pub struct BuiltIn {
    pub entry_id: &'static str,
    pub kind: ArtifactKind,
    pub format: &'static str,
    pub platform_target: &'static str,
    pub version: &'static str,
    pub sha256: &'static str,
    /// How the digest was authenticated, independently of the file (for the review record).
    pub authentication: &'static str,
}

impl BuiltIn {
    fn to_reference(&self) -> ReferenceDigest {
        ReferenceDigest {
            entry_id: self.entry_id.to_string(),
            kind: self.kind,
            format: self.format.to_string(),
            platform_target: self.platform_target.to_string(),
            version: self.version.to_string(),
            sha256: self.sha256.to_string(),
            source: self.authentication.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferenceSet {
    /// Authenticated (compiled in). The ONLY set that can verify.
    pub authenticated: Vec<ReferenceDigest>,
    /// Unauthenticated (database rows, README/package.json/user-supplied values). Evidence of a claim, never proof.
    pub unauthenticated: Vec<ReferenceDigest>,
}

impl ReferenceSet {
    /// The production set: the compiled-in list plus whatever the local database holds (as unauthenticated).
    pub fn production(conn: &Connection) -> AppResult<ReferenceSet> {
        Ok(ReferenceSet {
            authenticated: BUILT_IN_AUTHENTICATED.iter().map(BuiltIn::to_reference).collect(),
            unauthenticated: load_stored(conn)?,
        })
    }

    /// A stable fingerprint of the whole set. Verdicts are only valid against the set they were made with.
    pub fn epoch(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        for (tag, list) in [("A", &self.authenticated), ("U", &self.unauthenticated)] {
            for r in list.iter() {
                lines.push(format!(
                    "{tag}|{}|{}|{}|{}|{}|{}|{}",
                    r.entry_id,
                    r.kind.as_str(),
                    r.format,
                    r.platform_target,
                    r.version,
                    r.sha256,
                    r.source
                ));
            }
        }
        lines.sort();
        let mut h = Sha256::new();
        h.update(format!("policy-{POLICY_VERSION}\n").as_bytes());
        h.update(lines.join("\n").as_bytes());
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MatchResult {
    /// No fingerprint, or no reference applies.
    #[default]
    NotApplicable,
    /// A reference exists but cannot be compared (e.g. only an archive digest).
    Incomparable,
    MatchesUnauthenticated,
    MismatchesUnauthenticated,
    MatchesAuthenticated,
    MismatchesAuthenticated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceState {
    #[default]
    None,
    /// A reference value exists but nothing independent authenticated it.
    Unauthenticated,
    Authenticated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub reference: ReferenceState,
    pub result: MatchResult,
    pub kind: Option<ArtifactKind>,
    /// The version of the matching AUTHENTICATED reference (only then).
    pub version: Option<String>,
}

/// Compares the fingerprint of an exact installed file with the references for this entry and platform.
pub fn evaluate(entry_id: &str, platform_target: &str, sha256: &str, set: &ReferenceSet) -> Verdict {
    let applies = |r: &&ReferenceDigest| r.entry_id == entry_id && r.platform_target == platform_target;
    let auth: Vec<&ReferenceDigest> = set.authenticated.iter().filter(applies).collect();
    let stored: Vec<&ReferenceDigest> = set.unauthenticated.iter().filter(applies).collect();
    fn exact<'a>(list: &[&'a ReferenceDigest]) -> Vec<&'a ReferenceDigest> {
        list.iter().copied().filter(|r| r.kind.is_exact_file()).collect()
    }
    let (auth_exact, stored_exact) = (exact(&auth), exact(&stored));
    if let Some(hit) = auth_exact.iter().find(|r| r.sha256 == sha256) {
        return Verdict {
            reference: ReferenceState::Authenticated,
            result: MatchResult::MatchesAuthenticated,
            kind: Some(hit.kind),
            version: Some(hit.version.clone()),
        };
    }
    if !auth_exact.is_empty() {
        return Verdict {
            reference: ReferenceState::Authenticated,
            result: MatchResult::MismatchesAuthenticated,
            kind: Some(auth_exact[0].kind),
            version: None,
        };
    }
    if let Some(hit) = stored_exact.iter().find(|r| r.sha256 == sha256) {
        return Verdict {
            reference: ReferenceState::Unauthenticated,
            result: MatchResult::MatchesUnauthenticated,
            kind: Some(hit.kind),
            version: None,
        };
    }
    if !stored_exact.is_empty() {
        return Verdict {
            reference: ReferenceState::Unauthenticated,
            result: MatchResult::MismatchesUnauthenticated,
            kind: Some(stored_exact[0].kind),
            version: None,
        };
    }
    // Only archive / installer / package references exist: they cannot be compared with one installed file.
    if !auth.is_empty() || !stored.is_empty() {
        let state = if auth.is_empty() { ReferenceState::Unauthenticated } else { ReferenceState::Authenticated };
        return Verdict {
            reference: state,
            result: MatchResult::Incomparable,
            kind: auth.first().or(stored.first()).map(|r| r.kind),
            version: None,
        };
    }
    Verdict { reference: ReferenceState::None, result: MatchResult::NotApplicable, kind: None, version: None }
}

fn load_stored(conn: &Connection) -> AppResult<Vec<ReferenceDigest>> {
    let mut stmt = conn.prepare(
        "SELECT entry_id, artifact_kind, format, platform_target, version, sha256, source_label FROM reference_digests WHERE removed_at IS NULL",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (entry_id, kind, format, platform_target, version, sha256, source) = row?;
        // A row the CHECK constraints should have rejected (tampering) is ignored, never trusted.
        if let Some(kind) = ArtifactKind::parse(&kind) {
            if is_hex_digest(&sha256) {
                out.push(ReferenceDigest { entry_id, kind, format, platform_target, version, sha256, source });
            }
        }
    }
    Ok(out)
}

pub fn is_hex_digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Where an unauthenticated reference value came from. None of these can authenticate it.
#[allow(dead_code)] // used by the (test-only for now) import path
pub const UNAUTHENTICATED_SOURCES: &[&str] =
    &["user_provided", "readme_declared", "package_metadata_declared", "catalog_unreviewed", "synthetic_test"];

/// Records a reference digest as **unauthenticated**. Used by tests and by a future, separately reviewed import path; no
/// command, UI or scan calls it. Refuses malformed values; never creates a digest by hashing a local file.
#[allow(clippy::too_many_arguments, dead_code)]
pub fn insert_unauthenticated(
    conn: &Connection,
    entry_id: &str,
    kind: ArtifactKind,
    format: &str,
    platform_target: &str,
    version: &str,
    sha256: &str,
    source_kind: &str,
    source_label: &str,
) -> AppResult<()> {
    if !is_hex_digest(sha256) || !UNAUTHENTICATED_SOURCES.contains(&source_kind) {
        return Err(AppError::new("REFERENCE_INVALID", "Referans özeti veya kaynağı geçersiz."));
    }
    conn.execute(
        "INSERT INTO reference_digests (entry_id, artifact_kind, format, platform_target, version, algorithm, sha256, source_kind, source_label, authentication, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'sha256', ?6, ?7, ?8, 'none', ?9)",
        rusqlite::params![
            entry_id,
            kind.as_str(),
            crate::redact::sanitize(format, 40),
            platform_target,
            crate::redact::sanitize(version, 64),
            sha256,
            source_kind,
            crate::redact::sanitize(source_label, 80),
            crate::audit::now_utc()
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod unit {
    use super::*;

    fn r(entry: &str, kind: ArtifactKind, version: &str, digest: &str) -> ReferenceDigest {
        ReferenceDigest {
            entry_id: entry.into(),
            kind,
            format: "pe-exe".into(),
            platform_target: "windows-x64".into(),
            version: version.into(),
            sha256: digest.into(),
            source: "test".into(),
        }
    }

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn only_an_authenticated_exact_file_reference_can_match_authenticated() {
        let set = ReferenceSet { authenticated: vec![r("t", ArtifactKind::Executable, "1.2.3", A)], unauthenticated: vec![] };
        let v = evaluate("t", "windows-x64", A, &set);
        assert_eq!(
            (v.reference, v.result, v.version.as_deref()),
            (ReferenceState::Authenticated, MatchResult::MatchesAuthenticated, Some("1.2.3"))
        );
        assert_eq!(evaluate("t", "windows-x64", B, &set).result, MatchResult::MismatchesAuthenticated);
        // other tool / other platform: nothing applies
        assert_eq!(evaluate("other", "windows-x64", A, &set).result, MatchResult::NotApplicable);
        assert_eq!(evaluate("t", "macos-arm64", A, &set).result, MatchResult::NotApplicable);
    }

    #[test]
    fn unauthenticated_references_never_verify_even_when_they_match() {
        let set = ReferenceSet { authenticated: vec![], unauthenticated: vec![r("t", ArtifactKind::Executable, "1.2.3", A)] };
        let v = evaluate("t", "windows-x64", A, &set);
        assert_eq!((v.reference, v.result, v.version), (ReferenceState::Unauthenticated, MatchResult::MatchesUnauthenticated, None));
        assert_eq!(evaluate("t", "windows-x64", B, &set).result, MatchResult::MismatchesUnauthenticated);
    }

    #[test]
    fn an_archive_installer_or_package_digest_is_never_compared_with_an_installed_file() {
        for kind in [ArtifactKind::Archive, ArtifactKind::Installer, ArtifactKind::Package] {
            let set = ReferenceSet { authenticated: vec![r("t", kind, "1.0.0", A)], unauthenticated: vec![] };
            let v = evaluate("t", "windows-x64", A, &set);
            assert_eq!(v.result, MatchResult::Incomparable, "{kind:?}: even an equal value must not authenticate an extracted file");
            assert!(v.version.is_none());
        }
    }

    #[test]
    fn the_authenticated_set_wins_over_a_conflicting_unauthenticated_one_and_the_epoch_tracks_every_change() {
        let mut set = ReferenceSet {
            authenticated: vec![r("t", ArtifactKind::Executable, "1.0.0", A)],
            unauthenticated: vec![r("t", ArtifactKind::Executable, "9.9.9", B)],
        };
        assert_eq!(
            evaluate("t", "windows-x64", B, &set).result,
            MatchResult::MismatchesAuthenticated,
            "a self-declared digest cannot override the authenticated one"
        );
        let e1 = set.epoch();
        assert_eq!(e1, set.epoch(), "stable");
        set.unauthenticated[0].sha256 = A.to_string();
        assert_ne!(set.epoch(), e1, "tampering with any reference changes the epoch");
        assert_ne!(ReferenceSet::default().epoch(), e1);
    }

    #[test]
    fn the_compiled_in_authenticated_list_is_empty_in_this_phase() {
        assert!(
            BUILT_IN_AUTHENTICATED.is_empty(),
            "no independently authenticated digest evidence exists yet; adding one needs a reviewed policy change"
        );
    }
}
