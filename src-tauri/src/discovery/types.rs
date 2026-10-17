//! Vocabulary of the discovery engine. Four facts are kept strictly apart and are never merged into a
//! single "ok": *presence* (something was found), *configuration* (something refers to it), *identity*
//! (we know it is the catalogued tool) and *functionality* (it was tested — always `false` in Phase 2A).

use crate::redact;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    Present,
    Absent,
    Unknown,
    /// The platform (or surface) is outside what the detectors support. Never "absent".
    Unsupported,
}

impl Presence {
    pub fn as_str(self) -> &'static str {
        match self {
            Presence::Present => "present",
            Presence::Absent => "absent",
            Presence::Unknown => "unknown",
            Presence::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Configuration {
    Configured,
    Disabled,
    NotConfigured,
    /// Declared in a settings file, but the plugin was not fetched to disk ("referenced but not present").
    ReferencedNotPresent,
    Malformed,
    NotApplicable,
    Unknown,
}

impl Configuration {
    pub fn as_str(self) -> &'static str {
        match self {
            Configuration::Configured => "configured",
            Configuration::Disabled => "disabled",
            Configuration::NotConfigured => "not_configured",
            Configuration::ReferencedNotPresent => "referenced_not_present",
            Configuration::Malformed => "malformed",
            Configuration::NotApplicable => "not_applicable",
            Configuration::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Global,
    Project,
    Local,
    Unknown,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Global => "global",
            Scope::Project => "project",
            Scope::Local => "local",
            Scope::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
    None,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
            Confidence::None => "none",
        }
    }
}

/// How an observation was made. Every method is read-only; none executes anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    PathExecutable,
    NativeInstallLocation,
    LegacyLocalInstall,
    ClaudeSettingsFile,
    ClaudePluginDeclaration,
    ClaudeMcpConfig,
    /// No safe, documented detection method exists for this entry yet.
    NoDetector,
    /// The method is not documented, so nothing is guessed.
    NotDocumented,
    UnsupportedPlatform,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::PathExecutable => "path-executable",
            Method::NativeInstallLocation => "native-install-location",
            Method::LegacyLocalInstall => "legacy-local-install",
            Method::ClaudeSettingsFile => "claude-settings-file",
            Method::ClaudePluginDeclaration => "claude-plugin-declaration",
            Method::ClaudeMcpConfig => "claude-mcp-config",
            Method::NoDetector => "no-detector",
            Method::NotDocumented => "not-documented",
            Method::UnsupportedPlatform => "unsupported-platform",
        }
    }
}

// ---- Phase 2B: passive provenance ladder --------------------------------------------------------------------------
// Six separate facts, never collapsed into one "ok": (1) a file name was seen, (2) an installation record exists,
// (3) publisher/source metadata exists, (4) identity evidence was validated against a trust anchor, (5) a version came
// from trusted metadata, (6) functionality was tested. The sixth is `functionality_tested` and stays false.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IdentityState {
    /// No provenance check applies to this observation (or none ran).
    #[default]
    NotAssessed,
    /// Checks ran and found nothing beyond a file name.
    Unknown,
    /// A record or declaration exists, but nothing that proves it belongs to the catalogued tool.
    EvidenceAvailable,
    /// An installation record from a trusted installer names the catalog-pinned source. Not a statement about safety or function.
    Validated,
    /// Sources disagree (different publisher, name, or duplicate copies with differing records).
    Conflicted,
}

impl IdentityState {
    pub fn as_str(self) -> &'static str {
        match self {
            IdentityState::NotAssessed => "not_assessed",
            IdentityState::Unknown => "unknown",
            IdentityState::EvidenceAvailable => "evidence_available",
            IdentityState::Validated => "validated",
            IdentityState::Conflicted => "conflicted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VersionState {
    #[default]
    Unknown,
    Known,
    Conflicted,
}

impl VersionState {
    pub fn as_str(self) -> &'static str {
        match self {
            VersionState::Unknown => "unknown",
            VersionState::Known => "known",
            VersionState::Conflicted => "conflicted",
        }
    }
}

/// How far a version can be believed. Never obtained by running anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VersionTrust {
    #[default]
    None,
    /// Written by a package's own metadata (the package author controls it).
    Declared,
    /// Written by the agent's own installer record (Claude Code plugin registry).
    Trusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Publisher {
    #[default]
    None,
    /// The package or record names its own source; the author controls that claim.
    SelfDeclared,
    /// The installer recorded the marketplace the plugin came from.
    Marketplace,
}

/// An embedded Windows (Authenticode) signature block seen in the file header. Presence is NOT validity: the chain,
/// the signer, revocation and the file hash are not checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SignatureBlock {
    #[default]
    NotAssessed,
    Absent,
    PresentUnvalidated,
    /// Not a readable PE file.
    Unreadable,
}

/// Closed list of checks that were NOT performed, each with a fixed explanation in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotPerformed {
    ActiveVersionProbe,
    SignatureChainValidation,
    SignatureParsingUnsupported,
    InstalledFileDigest,
    PackageMetadataNotFound,
    NoPackageDetectorForTool,
    RecordToFileLinkage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Provenance {
    /// False for rows written before Phase 2B: their ladder was never evaluated.
    pub assessed: bool,
    pub filename_detected: bool,
    pub install_record: bool,
    pub publisher: Publisher,
    pub identity: IdentityState,
    pub version: VersionState,
    pub version_trust: VersionTrust,
    pub signature_block: SignatureBlock,
    pub not_performed: Vec<NotPerformed>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    /// A catalog entry id, or `agent:claude-code` / `agent:antigravity-ide` for the agents themselves.
    pub subject: String,
    pub platform_target: String,
    pub agent_surface: String,
    pub method: Method,
    pub presence: Presence,
    pub configuration: Configuration,
    pub version: Option<String>,
    pub version_source: Option<String>,
    pub scope: Scope,
    pub project_alias: Option<String>,
    pub confidence: Confidence,
    /// Why this was concluded. Plain sentences, no file contents, no secrets.
    pub evidence: String,
    /// Where it was seen, with the user's home folder collapsed to `~`.
    pub location: Option<String>,
    pub limitations: Vec<String>,
    /// We know it is the catalogued tool (exact identity match), not just a similar name.
    pub identity_verified: bool,
    /// Always false: nothing is executed, so nothing can have been functionally tested.
    pub functionality_tested: bool,
    pub provenance: Provenance,
}

impl Observation {
    pub fn new(subject: &str, platform: &str, surface: &str, method: Method) -> Self {
        Observation {
            subject: subject.to_string(),
            platform_target: platform.to_string(),
            agent_surface: surface.to_string(),
            method,
            presence: Presence::Unknown,
            configuration: Configuration::Unknown,
            version: None,
            version_source: None,
            scope: Scope::Unknown,
            project_alias: None,
            confidence: Confidence::None,
            evidence: String::new(),
            location: None,
            limitations: Vec::new(),
            identity_verified: false,
            functionality_tested: false,
            provenance: Provenance::default(),
        }
    }

    pub fn presence(mut self, p: Presence) -> Self {
        self.presence = p;
        self
    }
    pub fn configuration(mut self, c: Configuration) -> Self {
        self.configuration = c;
        self
    }
    pub fn scope(mut self, s: Scope) -> Self {
        self.scope = s;
        self
    }
    pub fn confidence(mut self, c: Confidence) -> Self {
        self.confidence = c;
        self
    }
    pub fn evidence(mut self, e: impl Into<String>) -> Self {
        self.evidence = e.into();
        self
    }
    pub fn location(mut self, l: Option<String>) -> Self {
        self.location = l;
        self
    }
    pub fn limit(mut self, l: impl Into<String>) -> Self {
        self.limitations.push(l.into());
        self
    }
    pub fn project(mut self, alias: Option<&str>) -> Self {
        self.project_alias = alias.map(str::to_string);
        self
    }
    pub fn identity_state(mut self, s: IdentityState) -> Self {
        self.provenance.identity = s;
        self
    }
    pub fn install_record(mut self, found: bool) -> Self {
        self.provenance.install_record = found;
        self
    }
    pub fn publisher(mut self, p: Publisher) -> Self {
        self.provenance.publisher = p;
        self
    }
    pub fn version_state(mut self, v: VersionState, trust: VersionTrust) -> Self {
        self.provenance.version = v;
        self.provenance.version_trust = trust;
        self
    }
    pub fn signature(mut self, b: SignatureBlock) -> Self {
        self.provenance.signature_block = b;
        self
    }
    pub fn not_performed(mut self, n: NotPerformed) -> Self {
        if !self.provenance.not_performed.contains(&n) {
            self.provenance.not_performed.push(n);
        }
        self
    }
    pub fn version(mut self, v: Option<String>, source: &str) -> Self {
        self.version_source = v.as_ref().map(|_| source.to_string());
        self.version = v;
        self
    }

    /// Every observation passes through here before it is stored or shown: text is redacted, control
    /// characters removed, lengths capped, and the privacy gate (`privacy.rs`) replaces any field that does not
    /// match its closed grammar — whatever a detector produced.
    pub fn finalize(mut self) -> Self {
        self.evidence = redact::sanitize(&self.evidence, 400);
        self.limitations = self.limitations.iter().map(|l| redact::sanitize(l, 240)).filter(|l| !l.is_empty()).collect();
        self.location = self.location.map(|l| l.chars().filter(|c| !c.is_control()).take(300).collect());
        super::privacy::scrub(&mut self);
        self.functionality_tested = false;
        self.finalize_provenance();
        self
    }
}

impl Observation {
    /// The ladder's invariants, enforced whatever a detector asked for:
    /// * "validated" needs an installation record AND a marketplace publisher recorded by the installer;
    /// * a version that did not survive the privacy gate is "unknown", and a "known" version needs a trust level;
    /// * a file name alone never raises identity above "unknown";
    /// * `identity_verified` is derived, never set on its own.
    fn finalize_provenance(&mut self) {
        let p = &mut self.provenance;
        p.assessed = true;
        p.filename_detected = self.presence == Presence::Present
            && matches!(self.method, Method::PathExecutable | Method::NativeInstallLocation | Method::LegacyLocalInstall);
        if p.identity == IdentityState::Validated && !(p.install_record && p.publisher == Publisher::Marketplace) {
            p.identity = if p.install_record { IdentityState::EvidenceAvailable } else { IdentityState::Unknown };
        }
        if p.identity == IdentityState::EvidenceAvailable && !p.install_record && p.publisher == Publisher::None {
            p.identity = IdentityState::Unknown;
        }
        match (self.version.is_some(), p.version) {
            (true, VersionState::Conflicted) => {
                self.version = None;
                self.version_source = None;
            }
            (true, _) => {
                p.version = VersionState::Known;
                if p.version_trust == VersionTrust::None {
                    p.version_trust = VersionTrust::Declared;
                }
            }
            (false, VersionState::Conflicted) => p.version_trust = VersionTrust::None,
            (false, _) => {
                p.version = VersionState::Unknown;
                p.version_trust = VersionTrust::None;
            }
        }
        if p.signature_block == SignatureBlock::PresentUnvalidated && !p.not_performed.contains(&NotPerformed::SignatureChainValidation) {
            p.not_performed.push(NotPerformed::SignatureChainValidation);
        }
        if p.filename_detected && p.version == VersionState::Unknown && !p.not_performed.contains(&NotPerformed::ActiveVersionProbe) {
            p.not_performed.push(NotPerformed::ActiveVersionProbe);
        }
        if p.filename_detected && !p.not_performed.contains(&NotPerformed::InstalledFileDigest) {
            p.not_performed.push(NotPerformed::InstalledFileDigest);
        }
        self.identity_verified = p.identity == IdentityState::Validated;
    }
}
