//! Vocabulary of the discovery engine. Four facts are kept strictly apart and are never merged into a
//! single "ok": *presence* (something was found), *configuration* (something refers to it), *identity*
//! (we know it is the catalogued tool) and *functionality* (it was tested — always `false` in Phase 2A).

use crate::redact;
use serde::Serialize;

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
    /// Always false in Phase 2A: nothing is executed, so nothing can have been functionally tested.
    pub functionality_tested: bool,
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
    pub fn identity(mut self, verified: bool) -> Self {
        self.identity_verified = verified;
        self
    }
    pub fn version(mut self, v: Option<String>, source: &str) -> Self {
        self.version_source = v.as_ref().map(|_| source.to_string());
        self.version = v;
        self
    }

    /// Every observation passes through here before it is stored or shown: text is redacted, control
    /// characters removed and lengths capped, whatever a detector produced.
    pub fn finalize(mut self) -> Self {
        self.evidence = redact::sanitize(&self.evidence, 400);
        self.limitations = self.limitations.iter().map(|l| redact::sanitize(l, 240)).filter(|l| !l.is_empty()).collect();
        self.location = self.location.map(|l| l.chars().filter(|c| !c.is_control()).take(300).collect());
        // A version must look like one AND survive redaction unchanged (a token-shaped "version" is dropped).
        self.version = self.version.filter(|v| is_plain_version(v) && redact::sanitize(v, 64) == *v);
        if self.version.is_none() {
            self.version_source = None;
        }
        self.functionality_tested = false;
        self
    }
}

/// Versions are shown only if they look like versions; arbitrary text from a config file is dropped.
pub fn is_plain_version(v: &str) -> bool {
    (1..=64).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+' | b'_'))
}
