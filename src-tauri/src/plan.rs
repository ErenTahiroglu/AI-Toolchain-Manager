//! Phase 2D: the installation PLAN preview. A plan is reviewable data, never a permission and never an action.
//!
//! * Nothing here runs a program, opens a network connection, downloads, installs, edits an agent configuration or writes
//!   to disk. [`build_plan`] is a pure function of (catalog entry, request, host facts, stored observations).
//! * Operations come from a CLOSED vocabulary that mirrors `schemas/install-recipe.schema.json`. A catalog install-method
//!   name is looked up in a compiled table; anything not in the table is "unsupported" and yields no operation. Text from a
//!   README, `package.json` or any other untrusted source is displayed (sanitised) but is never turned into a command,
//!   argument, path or recipe.
//! * No approved recipe store exists in this phase, so every plan is `blocked`: `can_execute` is false by construction and
//!   [`InstallPlan::check_invariants`] rejects any plan (for example one read back from JSON) that claims otherwise.

use crate::{
    audit::{canonical_json, sha256_hex},
    catalog::{self, CatalogEntry},
    discovery::store::{self, StoredObservation},
    error::{AppError, AppResult},
    redact,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const PLAN_SCHEMA_VERSION: u32 = 1;
pub const PLANNER_VERSION: &str = "1.0.0";

/// Shown with every plan, verbatim.
pub const PREVIEW_STATEMENT: &str =
    "Bu bir ÖNİZLEMEDİR. Hiçbir şey kurulmadı, indirilmedi, çalıştırılmadı veya değiştirilmedi; üçüncü taraf araçlara dokunulmadı. \
Bu plan bir izin değildir: onaylanmış bir tarif ve ayrı bir kullanıcı onayı olmadan hiçbir adım uygulanamaz.";

pub const PLATFORMS: &[&str] = &["windows-x64", "macos-arm64"];
pub const SURFACES: &[&str] =
    &["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "antigravity-cli", "antigravity-2", "local-cli"];
pub const SCOPES: &[&str] = &["user", "project"];

// ---------------------------------------------------------------------------------------------------------------------
// Request and host facts
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanRequest {
    pub entry_id: String,
    pub platform_target: String,
    pub agent_surface: String,
    /// `user` | `project`
    pub scope: String,
    /// One of the entry's declared install methods; `None` = the first one this planner supports.
    pub method_id: Option<String>,
}

/// What the planner may know about the computer it runs on. Injected so tests can describe any machine; nothing here is
/// probed by running a program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFacts {
    /// `windows-x64`, `macos-arm64` or `unsupported`.
    pub platform: String,
    /// `None` = not checked (checking needs an operating-system call this phase does not make).
    pub elevated: Option<bool>,
}

impl HostFacts {
    pub fn detect() -> HostFacts {
        let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => "windows-x64",
            ("macos", "aarch64") => "macos-arm64",
            _ => "unsupported",
        };
        HostFacts { platform: platform.to_string(), elevated: None }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Closed vocabularies
// ---------------------------------------------------------------------------------------------------------------------

/// The seven operation kinds of the install-recipe schema, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationKind {
    #[serde(rename = "fetch-release-artifact")]
    FetchReleaseArtifact,
    #[serde(rename = "verify-artifact-digest")]
    VerifyArtifactDigest,
    #[serde(rename = "stage-private-copy")]
    StagePrivateCopy,
    #[serde(rename = "activate-staged-artifact")]
    ActivateStagedArtifact,
    #[serde(rename = "install-pinned-package")]
    InstallPinnedPackage,
    #[serde(rename = "register-native-plugin")]
    RegisterNativePlugin,
    #[serde(rename = "write-managed-instruction-block")]
    WriteManagedInstructionBlock,
}

impl OperationKind {
    #[allow(dead_code)]
    pub const ALL: [OperationKind; 7] = [
        OperationKind::FetchReleaseArtifact,
        OperationKind::VerifyArtifactDigest,
        OperationKind::StagePrivateCopy,
        OperationKind::ActivateStagedArtifact,
        OperationKind::InstallPinnedPackage,
        OperationKind::RegisterNativePlugin,
        OperationKind::WriteManagedInstructionBlock,
    ];

    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            OperationKind::FetchReleaseArtifact => "fetch-release-artifact",
            OperationKind::VerifyArtifactDigest => "verify-artifact-digest",
            OperationKind::StagePrivateCopy => "stage-private-copy",
            OperationKind::ActivateStagedArtifact => "activate-staged-artifact",
            OperationKind::InstallPinnedPackage => "install-pinned-package",
            OperationKind::RegisterNativePlugin => "register-native-plugin",
            OperationKind::WriteManagedInstructionBlock => "write-managed-instruction-block",
        }
    }

    /// Fixed Turkish description of what the step WOULD do (never what it did).
    fn summary(self) -> &'static str {
        match self {
            OperationKind::FetchReleaseArtifact => "Yayıncının sürüm dosyası, onaylı tarifte yazılı adresten indirilecekti.",
            OperationKind::VerifyArtifactDigest => "İndirilen dosyanın SHA-256 özeti, tarifte yazılı beklenen değerle karşılaştırılacaktı.",
            OperationKind::StagePrivateCopy => "Dosya, yöneticinin kendi hazırlık klasörüne özel bir kopya olarak alınacaktı.",
            OperationKind::ActivateStagedArtifact => {
                "Hazırlanan dosya, yöneticinin sürümlü klasöründe etkinleştirilecekti (önceki sürüme dönüş için atomik geçiş)."
            }
            OperationKind::InstallPinnedPackage => {
                "Belirli bir sürüme sabitlenmiş paket, paket yöneticisiyle kurulacaktı (yaşam döngüsü betikleri kapalı)."
            }
            OperationKind::RegisterNativePlugin => "Eklenti, ajanın kendi eklenti yöneticisine kaydedilecekti.",
            OperationKind::WriteManagedInstructionBlock => {
                "Bir talimat dosyasına işaretçilerle sınırlanmış yönetilen bir blok yazılacaktı."
            }
        }
    }
}

/// Where a step would write. Closed identifiers; the UI turns them into plain text and never shows a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteArea {
    ManagerStaging,
    ManagerVersionedFolder,
    PackageManagerEnvironment,
    AgentPluginRegistryClaude,
    AgentPluginRegistryAntigravity,
    ProjectAgentSettings,
}

impl WriteArea {
    fn label(self) -> &'static str {
        match self {
            WriteArea::ManagerStaging => "Yöneticinin kendi hazırlık klasörü",
            WriteArea::ManagerVersionedFolder => "Yöneticinin sürümlü program klasörü",
            WriteArea::PackageManagerEnvironment => "Paket yöneticisinin kendi ortamı (aracın kendi kurulum adımı ek dosya yazabilir)",
            WriteArea::AgentPluginRegistryClaude => {
                "Claude Code eklenti kaydı (~/.claude/settings.json ve ~/.claude/plugins/installed_plugins.json)"
            }
            WriteArea::AgentPluginRegistryAntigravity => "Antigravity eklenti kaydı (dosya konumu bu sürümde bilinmiyor)",
            WriteArea::ProjectAgentSettings => "Projenin kendi .claude/settings.json dosyası",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminNeed {
    /// Normal user rights are documented as enough.
    No,
    /// Depends on the package or machine; not decidable without running something.
    Maybe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reversibility {
    /// The host's own tooling can remove it; user data it created may remain.
    HostRemovable,
    /// The manager's own versioned folder allows switching back (design intent; nothing is implemented).
    VersionSwitch,
    /// Nothing is changed (download/verify only) or the change is private to the manager.
    NothingToUndo,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkUse {
    Yes,
    No,
}

// ---------------------------------------------------------------------------------------------------------------------
// Install methods the planner understands (compiled; the catalog can name others, which stay unsupported)
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum MethodKind {
    NativePlugin,
    ReleaseArchive,
    Package(&'static str),
}

struct MethodSpec {
    id: &'static str,
    label: &'static str,
    platforms: &'static [&'static str],
    kind: MethodKind,
    admin: AdminNeed,
}

const BOTH: &[&str] = &["windows-x64", "macos-arm64"];

const METHODS: &[MethodSpec] = &[
    MethodSpec {
        id: "native-plugin-manager",
        label: "Ajanın kendi eklenti yöneticisi",
        platforms: BOTH,
        kind: MethodKind::NativePlugin,
        admin: AdminNeed::No,
    },
    MethodSpec {
        id: "verified-release-archive",
        label: "Doğrulanmış sürüm dosyası (yöneticinin kendi klasörüne)",
        platforms: BOTH,
        kind: MethodKind::ReleaseArchive,
        admin: AdminNeed::No,
    },
    MethodSpec {
        id: "homebrew",
        label: "Homebrew",
        platforms: &["macos-arm64"],
        kind: MethodKind::Package("homebrew"),
        admin: AdminNeed::Maybe,
    },
    MethodSpec { id: "winget", label: "winget", platforms: &["windows-x64"], kind: MethodKind::Package("winget"), admin: AdminNeed::Maybe },
    MethodSpec { id: "uv-tool", label: "uv tool", platforms: BOTH, kind: MethodKind::Package("uv-tool"), admin: AdminNeed::No },
    MethodSpec { id: "pipx", label: "pipx", platforms: BOTH, kind: MethodKind::Package("pipx"), admin: AdminNeed::No },
    MethodSpec { id: "go-install", label: "go install", platforms: BOTH, kind: MethodKind::Package("go-install"), admin: AdminNeed::No },
];

/// Method names the catalog may declare that this phase deliberately does not turn into operations.
const KNOWN_UNSUPPORTED: &[(&str, &str)] = &[
    ("skills-cli", "Üçüncü taraf 'skills' komut satırı aracı: tipli tarif türü yok."),
    ("pip", "Doğrudan pip: tarif şeması bunu desteklemiyor (yalnızca yalıtılmış uv/pipx)."),
    ("docker", "Docker: tarif şeması bunu desteklemiyor."),
    ("manual-instructions", "Elle yapılan adımlar: yönetici bunları uygulamaz."),
    ("unknown", "Yöntem belirtilmemiş."),
];

fn method_spec(id: &str) -> Option<&'static MethodSpec> {
    METHODS.iter().find(|m| m.id == id)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MethodChoice {
    pub method_id: String,
    pub label: String,
    /// Does the planner know how to describe it (and for this platform)?
    pub supported: bool,
    pub note: String,
}

// ---------------------------------------------------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlannedStep {
    pub index: u32,
    pub kind: OperationKind,
    pub summary: String,
    /// The schema-shaped operation, only when every parameter is known and valid. Otherwise `None`.
    pub typed: Option<Value>,
    /// Parameters a recipe would need that the catalog does not provide (so no typed operation exists).
    pub missing_parameters: Vec<String>,
    pub network: NetworkUse,
    pub writes: Vec<WriteArea>,
    pub write_labels: Vec<String>,
    pub admin: AdminNeed,
    pub reversibility: Reversibility,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Blocker {
    pub code: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Prerequisite {
    pub id: String,
    pub text: String,
    /// `met` | `unmet` | `unknown`
    pub state: String,
    /// `observed_scan` | `publisher_declared` | `not_checked`
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub code: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanEntry {
    pub entry_id: String,
    pub display_name: String,
    pub kind: String,
    /// `owner/name` of the declared repository (display only).
    pub source: String,
    pub review_state: String,
    pub execution_policy: String,
    pub license_state: String,
    pub revision: String,
    pub immutable_commit_pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanTarget {
    pub platform_target: String,
    pub agent_surface: String,
    pub scope: String,
    pub this_computer: String,
    pub this_computer_matches: bool,
    /// The install-recipe schema's target id, if one exists for this platform and surface.
    pub recipe_target_id: Option<String>,
    /// `vendor-declared` | `unknown` | … from the catalog (publisher claim, not a test).
    pub catalog_compatibility: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Eligibility {
    /// Always `blocked` in this phase.
    pub status: String,
    pub can_execute: bool,
    pub blockers: Vec<Blocker>,
    pub warnings: Vec<Blocker>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInfo {
    /// Would the (future) steps use the network?
    pub planned_use: bool,
    pub destinations: Vec<String>,
    /// Always false: building this preview made no connection.
    pub used_to_build_preview: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Disclosure {
    /// Closed word, see the builder.
    pub state: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigChange {
    pub area: WriteArea,
    /// `create_or_merge` | `unknown`
    pub change: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RollbackInfo {
    /// Always false in this phase: nothing is implemented.
    pub available: bool,
    /// `none_implemented`
    pub reliability: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlan {
    pub schema_version: u32,
    pub planner_version: String,
    /// SHA-256 of the canonical plan content (no timestamps): the same inputs give the same id.
    pub plan_id: String,
    /// Always `preview_only`. There is no "installed", "running" or "approved" state in this type.
    pub state: String,
    pub executable: bool,
    pub executed: bool,
    pub modifies_anything: bool,
    pub statement: String,
    pub entry: PlanEntry,
    pub target: PlanTarget,
    pub eligibility: Eligibility,
    pub methods: Vec<MethodChoice>,
    pub chosen_method: Option<String>,
    pub prerequisites: Vec<Prerequisite>,
    /// Dependency lines copied from the publisher's record: sanitised, shown as claims, never executed or parsed.
    pub declared_dependencies: Vec<String>,
    pub steps: Vec<PlannedStep>,
    pub permissions: Vec<String>,
    pub network: NetworkInfo,
    pub telemetry: Disclosure,
    pub data_and_credentials: Disclosure,
    pub disk_usage: Disclosure,
    pub config_changes: Vec<ConfigChange>,
    pub conflicts: Vec<Conflict>,
    pub uninstall: Disclosure,
    pub rollback: RollbackInfo,
    pub unknowns: Vec<String>,
}

impl InstallPlan {
    /// A plan is data. Anything that claims to be executable, executed, or in a state other than `preview_only` is
    /// rejected, whether it was built here or read back from JSON.
    pub fn check_invariants(&self) -> Result<(), &'static str> {
        if self.schema_version != PLAN_SCHEMA_VERSION {
            return Err("unsupported_schema_version");
        }
        if self.state != "preview_only" || self.executable || self.executed || self.modifies_anything {
            return Err("plan_claims_execution");
        }
        if self.eligibility.can_execute || self.eligibility.status != "blocked" {
            return Err("plan_claims_eligibility");
        }
        if self.statement != PREVIEW_STATEMENT || self.network.used_to_build_preview || self.rollback.available {
            return Err("plan_claims_effects");
        }
        if self.steps.iter().any(|s| s.summary != s.kind.summary()) {
            return Err("step_text_not_from_vocabulary");
        }
        Ok(())
    }
}

/// Reads a plan from JSON and rejects anything that is not a well-formed, versioned, non-executing preview.
#[allow(dead_code)] // exercised by the tests and the self-check; no command accepts a plan from outside
pub fn parse_plan(json: &str) -> Result<InstallPlan, &'static str> {
    let plan: InstallPlan = serde_json::from_str(json).map_err(|_| "plan_not_readable")?;
    plan.check_invariants()?;
    Ok(plan)
}

// ---------------------------------------------------------------------------------------------------------------------
// Building a plan (pure)
// ---------------------------------------------------------------------------------------------------------------------

fn invalid(what: &str) -> AppError {
    AppError::new("PLAN_REQUEST_INVALID", format!("Plan isteği geçersiz: {what}."))
}

fn validate_request(r: &PlanRequest) -> AppResult<()> {
    if !PLATFORMS.contains(&r.platform_target.as_str()) {
        return Err(invalid("desteklenen platformlar Windows x64 ve macOS Apple Silicon'dur"));
    }
    if !SURFACES.contains(&r.agent_surface.as_str()) {
        return Err(invalid("bilinmeyen ajan yüzeyi"));
    }
    if !SCOPES.contains(&r.scope.as_str()) {
        return Err(invalid("kapsam 'user' veya 'project' olmalı"));
    }
    if let Some(m) = &r.method_id {
        if m.len() > 64 || !m.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
            return Err(invalid("yöntem adı geçersiz"));
        }
    }
    Ok(())
}

/// The recipe schema's `targetId` for a platform and surface, if the schema can express it at all.
fn recipe_target(platform: &str, surface: &str) -> Option<&'static str> {
    match (platform, surface) {
        ("macos-arm64", "claude-code-terminal" | "claude-desktop-code") => Some("macos-arm64-claude-code"),
        ("macos-arm64", "antigravity-ide") => Some("macos-arm64-antigravity-ide"),
        ("windows-x64", "claude-code-terminal") => Some("windows-x64-claude-code"),
        _ => None,
    }
}

fn blocker(code: &str, text: &str) -> Blocker {
    Blocker { code: code.to_string(), text: text.to_string() }
}

fn sanitize_claim(text: &str) -> String {
    redact::sanitize(text, 160)
}

fn str_at<'a>(v: &'a Value, pointer: &str) -> &'a str {
    v.pointer(pointer).and_then(Value::as_str).unwrap_or_default()
}

fn is_plugin_id(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 80
        && b[0].is_ascii_alphanumeric()
        && b.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}

fn is_pinned_version(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 128
        && b[0].is_ascii_alphanumeric()
        && b.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'+' | b'_' | b'-'))
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn step(index: u32, kind: OperationKind, network: NetworkUse, writes: &[WriteArea], admin: AdminNeed, rev: Reversibility) -> PlannedStep {
    PlannedStep {
        index,
        kind,
        summary: kind.summary().to_string(),
        typed: None,
        missing_parameters: Vec::new(),
        network,
        writes: writes.to_vec(),
        write_labels: writes.iter().map(|w| w.label().to_string()).collect(),
        admin,
        reversibility: rev,
    }
}

/// Catalog compatibility state for (platform, surface), as the publisher-claim words the catalog already uses.
fn catalog_compat(entry: &CatalogEntry, platform: &str, surface: &str) -> String {
    entry
        .record
        .get("targetEvidence")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter().find(|t| str_at(t, "/platformTarget") == platform && str_at(t, "/agentSurface") == surface).map(|t| {
                let s = str_at(t, "/compatibilityState");
                if matches!(s, "vendor-declared" | "unknown" | "unsupported" | "verified") {
                    s.to_string()
                } else {
                    "unknown".to_string()
                }
            })
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn declared_methods(entry: &CatalogEntry) -> Vec<(String, Option<String>)> {
    entry
        .record
        .get("installMethods")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    let id = str_at(m, "/methodId");
                    // only well-formed method names are even carried along; they are looked up, never interpreted
                    (!id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
                        .then(|| (id.to_string(), m.get("publisherPin").and_then(Value::as_str).map(str::to_string)))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn method_choice(id: &str, platform: &str) -> MethodChoice {
    if let Some(spec) = method_spec(id) {
        let ok = spec.platforms.contains(&platform);
        return MethodChoice {
            method_id: id.to_string(),
            label: spec.label.to_string(),
            supported: ok,
            note: if ok { String::new() } else { "Bu yöntem seçilen platformda kullanılamaz.".to_string() },
        };
    }
    let note = KNOWN_UNSUPPORTED
        .iter()
        .find(|(k, _)| *k == id)
        .map(|(_, t)| (*t).to_string())
        .unwrap_or_else(|| "Bu yöntem adı tanınmıyor; hiçbir işleme çevrilmedi.".to_string());
    MethodChoice { method_id: id.to_string(), label: "Desteklenmeyen yöntem".to_string(), supported: false, note }
}

fn observed<'a>(obs: &'a [StoredObservation], subject: &str) -> Vec<&'a StoredObservation> {
    obs.iter().filter(|o| o.subject == subject).collect()
}

fn agent_subject(surface: &str) -> Option<&'static str> {
    match surface {
        "claude-code-terminal" | "claude-desktop-code" => Some("agent:claude-code"),
        "antigravity-ide" | "antigravity-cli" | "antigravity-2" => Some("agent:antigravity-ide"),
        _ => None,
    }
}

pub fn build_plan(entry: &CatalogEntry, req: &PlanRequest, host: &HostFacts, observations: &[StoredObservation]) -> AppResult<InstallPlan> {
    validate_request(req)?;
    let mut blockers: Vec<Blocker> = Vec::new();
    let mut warnings: Vec<Blocker> = Vec::new();
    let mut unknowns: Vec<String> = Vec::new();
    let mut conflicts: Vec<Conflict> = Vec::new();
    let mut prerequisites: Vec<Prerequisite> = Vec::new();
    let record = &entry.record;

    // ---- entry-level blockers -------------------------------------------------------------------------------------
    if entry.removed_at.is_some() {
        blockers.push(blocker("entry_removed", "Bu kayıt katalogdan kaldırılmış."));
    }
    if entry.execution_policy == "blocked-until-scoped-authorization" {
        blockers.push(blocker(
            "execution_policy_blocked",
            "Bu kayıt için kapsamlı yazılı yetki olmadan hiçbir işlem planlanamaz (etkin güvenlik testi aracı).",
        ));
    }
    if matches!(entry.review_state.as_str(), "quarantined" | "blocked") {
        blockers.push(blocker("entry_quarantined", "Kayıt karantinada veya engelli."));
    }
    // This phase has no approved-recipe store: that alone keeps every plan blocked, whatever else is true.
    blockers.push(blocker(
        "no_approved_recipe",
        "Bu araç için onaylanmış, tipli bir kurulum tarifi yok ve bu sürümde tarif onaylama/saklama mekanizması da yok.",
    ));
    if entry.review_state != "metadata-reviewed" {
        blockers.push(blocker("metadata_unreviewed", "Katalog bilgileri bağımsız olarak incelenmedi; yayıncı beyanlarına dayanıyor."));
    }
    if entry.license_state != "known" || entry.license_spdx.is_none() {
        blockers.push(blocker("license_not_accepted", "Lisans kabul edilmiş bir incelemeden geçmedi."));
    } else {
        blockers
            .push(blocker("license_not_accepted", "Lisans beyan edilmiş ancak tarifin gerektirdiği 'kabul edildi' incelemesi yapılmadı."));
    }
    let pinned =
        record.pointer("/sourceSnapshot/immutableCommit").and_then(Value::as_str).map(|c| c.len() == 40 || c.len() == 64).unwrap_or(false);
    if !pinned {
        blockers.push(blocker("source_not_pinned", "Kaynak değişmez bir commit'e sabitlenmemiş; içerik zamanla değişebilir."));
    }

    // ---- target ---------------------------------------------------------------------------------------------------
    let recipe_target_id = recipe_target(&req.platform_target, &req.agent_surface);
    if recipe_target_id.is_none() {
        blockers.push(blocker(
            "target_not_in_recipe_schema",
            "Tarif şeması bu platform/ajan yüzeyi birleşimini ifade edemiyor (şu an yalnızca Claude Code ve Antigravity IDE hedefleri var).",
        ));
    }
    let compat = catalog_compat(entry, &req.platform_target, &req.agent_surface);
    if compat != "verified" {
        let text = match compat.as_str() {
            "vendor-declared" => "Uyumluluk yalnızca yayıncı tarafından beyan edilmiş; bu bilgisayarda doğrulanmadı.",
            "unsupported" => "Katalog bu platform/ajan birleşimini desteklenmiyor olarak işaretliyor.",
            _ => "Bu platform/ajan birleşimi için uyumluluk bilinmiyor.",
        };
        blockers.push(blocker(if compat == "unsupported" { "unsupported_combination" } else { "compatibility_not_verified" }, text));
    }
    let this_matches = host.platform == req.platform_target;
    if host.platform == "unsupported" {
        blockers.push(blocker(
            "unsupported_host",
            "Bu bilgisayarın işletim sistemi/mimarisi desteklenmiyor (yalnızca Windows x64 ve macOS Apple Silicon).",
        ));
    } else if !this_matches {
        blockers.push(blocker(
            "different_computer_platform",
            "Seçilen platform bu bilgisayarınkinden farklı: plan incelenebilir, ancak burada uygulanamaz.",
        ));
    }

    // ---- method ---------------------------------------------------------------------------------------------------
    let declared = declared_methods(entry);
    let methods: Vec<MethodChoice> = declared.iter().map(|(id, _)| method_choice(id, &req.platform_target)).collect();
    let chosen: Option<(String, Option<String>)> = match &req.method_id {
        Some(m) => declared.iter().find(|(id, _)| id == m).cloned(),
        None => declared
            .iter()
            .find(|(id, _)| method_spec(id).map(|s| s.platforms.contains(&req.platform_target.as_str())).unwrap_or(false))
            .cloned(),
    };
    if req.method_id.is_some() && chosen.is_none() {
        blockers.push(blocker("method_not_declared", "İstenen kurulum yöntemi bu kayıtta tanımlı değil."));
    }
    let spec = chosen.as_ref().and_then(|(id, _)| method_spec(id));
    if let (Some((id, _)), None) = (&chosen, spec) {
        let _ = id;
        blockers.push(blocker(
            "unsupported_install_method",
            "Seçilen yöntem bu sürümde tipli bir işleme çevrilemiyor; hiçbir komut tahmin edilmez.",
        ));
    }
    if chosen.is_none() && req.method_id.is_none() {
        blockers
            .push(blocker("no_supported_method", "Kaydın tanımladığı yöntemlerden hiçbiri bu platformda tipli bir işleme çevrilemiyor."));
    }
    if let Some(s) = spec {
        if !s.platforms.contains(&req.platform_target.as_str()) {
            blockers.push(blocker("method_not_for_platform", "Seçilen yöntem bu platformda kullanılamaz."));
        }
    }

    // ---- steps ----------------------------------------------------------------------------------------------------
    let mut steps: Vec<PlannedStep> = Vec::new();
    let mut config_changes: Vec<ConfigChange> = Vec::new();
    let mut uninstall =
        Disclosure { state: "unknown".to_string(), text: "Kaldırma davranışı bu yöntem için tanımlanmadı.".to_string() };
    let publisher_pin = chosen.as_ref().and_then(|(_, pin)| pin.clone());
    if let Some(s) = spec.filter(|s| s.platforms.contains(&req.platform_target.as_str())) {
        match s.kind {
            MethodKind::NativePlugin => {
                let host_ok = req.agent_surface != "local-cli";
                if !host_ok {
                    blockers
                        .push(blocker("method_not_for_surface", "Ajan eklentisi yöntemi, 'yerel komut satırı' yüzeyi için anlamlı değil."));
                } else {
                    let (area, change_text) = match (req.agent_surface.as_str(), req.scope.as_str()) {
                        ("claude-code-terminal" | "claude-desktop-code", "project") => {
                            (WriteArea::ProjectAgentSettings, "Projenin Claude ayar dosyasına eklenti bildirimi eklenebilir.")
                        }
                        ("claude-code-terminal" | "claude-desktop-code", _) => (
                            WriteArea::AgentPluginRegistryClaude,
                            "Claude Code ayar ve eklenti kayıt dosyalarına eklenti bildirimi eklenebilir.",
                        ),
                        _ => (
                            WriteArea::AgentPluginRegistryAntigravity,
                            "Antigravity eklenti kaydının konumu ve biçimi bu sürümde bilinmiyor.",
                        ),
                    };
                    let mut st =
                        step(1, OperationKind::RegisterNativePlugin, NetworkUse::Yes, &[area], s.admin, Reversibility::HostRemovable);
                    let plugin = entry.plugin_identity.clone().unwrap_or_default();
                    if is_plugin_id(&plugin) {
                        st.typed = Some(json!({ "operation": "register-native-plugin", "host": req.agent_surface, "pluginId": plugin }));
                    } else {
                        st.missing_parameters.push("pluginId".to_string());
                    }
                    steps.push(st);
                    config_changes.push(ConfigChange {
                        area,
                        change: if area == WriteArea::AgentPluginRegistryAntigravity { "unknown" } else { "create_or_merge" }.to_string(),
                        text: change_text.to_string(),
                    });
                    uninstall = Disclosure {
                        state: "host_tool".to_string(),
                        text: "Kaldırma ajanın kendi eklenti yöneticisiyle yapılır; eklentinin oluşturduğu kullanıcı verileri ve ayarları kalabilir.".to_string(),
                    };
                }
            }
            MethodKind::ReleaseArchive => {
                let sha = record.pointer("/artifactEvidence/sha256").and_then(Value::as_str).filter(|d| is_sha256(d));
                if record.get("artifactEvidence").map(Value::is_null).unwrap_or(true) || sha.is_none() {
                    blockers.push(blocker("no_artifact_digest", "Kayıtta indirilecek dosya için doğrulanmış bir SHA-256 özeti yok."));
                }
                let mut fetch = step(
                    1,
                    OperationKind::FetchReleaseArtifact,
                    NetworkUse::Yes,
                    &[WriteArea::ManagerStaging],
                    s.admin,
                    Reversibility::NothingToUndo,
                );
                fetch.typed = Some(json!({ "operation": "fetch-release-artifact", "artifactId": "primary" }));
                let mut verify = step(2, OperationKind::VerifyArtifactDigest, NetworkUse::No, &[], s.admin, Reversibility::NothingToUndo);
                match sha {
                    Some(d) => {
                        verify.typed = Some(json!({ "operation": "verify-artifact-digest", "artifactId": "primary", "expectedSha256": d }))
                    }
                    None => verify.missing_parameters.push("expectedSha256".to_string()),
                }
                let mut stage = step(
                    3,
                    OperationKind::StagePrivateCopy,
                    NetworkUse::No,
                    &[WriteArea::ManagerStaging],
                    s.admin,
                    Reversibility::NothingToUndo,
                );
                stage.typed =
                    Some(json!({ "operation": "stage-private-copy", "artifactId": "primary", "destinationId": "manager-owned-staging" }));
                let mut activate = step(
                    4,
                    OperationKind::ActivateStagedArtifact,
                    NetworkUse::No,
                    &[WriteArea::ManagerVersionedFolder],
                    s.admin,
                    Reversibility::VersionSwitch,
                );
                activate.typed = Some(json!({
                    "operation": "activate-staged-artifact", "artifactId": "primary",
                    "activationSlot": "manager-versioned-binary", "activationMethod": "atomic-version-switch"
                }));
                steps.extend([fetch, verify, stage, activate]);
                unknowns.push("Programın komut satırından nasıl bulunacağı (PATH bağlantısı) tarifte tanımlı değil; bu plan PATH'i değiştirdiğini iddia etmez.".to_string());
                uninstall = Disclosure {
                    state: "manager_owned".to_string(),
                    text: "Yöneticinin sürümlü klasörü silinebilir; aracın kendi ürettiği dizinler/veriler (ör. indeksler) kullanıcıya ait sayılır ve kalır.".to_string(),
                };
            }
            MethodKind::Package(manager) => {
                let version = publisher_pin.clone().filter(|v| is_pinned_version(v));
                let mut st = step(
                    1,
                    OperationKind::InstallPinnedPackage,
                    NetworkUse::Yes,
                    &[WriteArea::PackageManagerEnvironment],
                    s.admin,
                    Reversibility::Unknown,
                );
                // The catalog records no package name; a name from a README is never accepted as one.
                st.missing_parameters.push("packageName".to_string());
                if version.is_none() {
                    st.missing_parameters.push("version".to_string());
                    blockers.push(blocker("version_not_pinned", "Paket sürümü sabitlenmemiş; 'en son sürüm' kurulumu planlanamaz."));
                }
                blockers.push(blocker(
                    "package_name_unknown",
                    "Katalog bu yöntem için doğrulanmış bir paket adı içermiyor; README'den ad alınmaz.",
                ));
                let _ = manager;
                steps.push(st);
                prerequisites.push(Prerequisite {
                    id: "package_manager".to_string(),
                    text: format!(
                        "{} bu bilgisayarda kurulu olmalı. Kurulu olup olmadığı denetlenmedi (hiçbir program çalıştırılmaz).",
                        s.label
                    ),
                    state: "unknown".to_string(),
                    source: "not_checked".to_string(),
                });
                unknowns
                    .push("Paketin kendi kurulum adımının hangi ek dosyaları veya ajan ayarlarını değiştireceği bilinmiyor.".to_string());
                uninstall = Disclosure {
                    state: "package_manager".to_string(),
                    text: "Kaldırma paket yöneticisiyle yapılır; paketin ürettiği veriler kalabilir. Geri alma güvenilir biçimde tanımlı değil.".to_string(),
                };
            }
        }
    }

    // ---- prerequisites ---------------------------------------------------------------------------------------------
    if let Some(subject) = agent_subject(&req.agent_surface) {
        let seen = observed(observations, subject);
        let present = seen.iter().any(|o| o.presence == "present");
        let absent = seen.iter().any(|o| o.presence == "absent");
        let (state, text) = if present {
            ("met", "Ajan, son tarama kaydında bulundu (yalnızca varlığı; çalıştığı doğrulanmadı).")
        } else if absent {
            ("unmet", "Ajan, son tarama kaydında bulunamadı.")
        } else {
            ("unknown", "Ajanın bu bilgisayarda olup olmadığı bilinmiyor: izin verilmiş bir tarama kaydı yok.")
        };
        prerequisites.push(Prerequisite {
            id: "agent_present".to_string(),
            text: text.to_string(),
            state: state.to_string(),
            source: "observed_scan".to_string(),
        });
        if subject == "agent:claude-code" {
            let settings = seen.iter().find(|o| o.method == "claude-settings-file" && o.scope == "global");
            let (state, text) = match settings.map(|o| o.configuration.as_str()) {
                Some("malformed") => {
                    ("unmet", "Claude Code kullanıcı ayar dosyası bozuk görünüyor; plan bunu düzeltmeyi önermez ve üzerine yazmaz.")
                }
                Some("configured" | "disabled" | "referenced_not_present") => {
                    ("met", "Claude Code kullanıcı ayar dosyası son taramada okunabildi.")
                }
                _ => ("unknown", "Claude Code ayar dosyasının durumu bilinmiyor (dosya okunmadı veya taranmadı)."),
            };
            prerequisites.push(Prerequisite {
                id: "agent_config_readable".to_string(),
                text: text.to_string(),
                state: state.to_string(),
                source: "observed_scan".to_string(),
            });
        }
        if !present {
            blockers.push(blocker("agent_not_confirmed", "Hedef ajanın bu bilgisayarda bulunduğu doğrulanmadı."));
        }
        // An open agent session could be using what an install would touch; the manager cannot see sessions yet.
        conflicts.push(Conflict {
            code: "active_session_unknown".to_string(),
            text: "Ajanın açık bir oturumu olup olmadığı algılanamıyor. Açık oturum, eklenti veya ayar değişikliğinden etkilenebilir."
                .to_string(),
        });
        blockers.push(blocker(
            "active_session_unknown",
            "Ajan oturumlarının kapalı olduğu kanıtlanamıyor; bu yüzden değişiklik planı uygulanamaz.",
        ));
    }
    let mut declared_dependencies: Vec<String> = Vec::new();
    for d in record.get("dependencies").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).take(12) {
        let clean = sanitize_claim(d);
        if clean.is_empty() {
            continue;
        }
        declared_dependencies.push(clean.clone());
        prerequisites.push(Prerequisite {
            id: "declared_dependency".to_string(),
            text: format!("Yayıncı şunu gerektirdiğini belirtiyor: {clean}"),
            state: "unknown".to_string(),
            source: "publisher_declared".to_string(),
        });
    }
    if prerequisites.iter().any(|p| p.state != "met") {
        blockers.push(blocker("prerequisites_unverified", "Ön koşullardan en az biri karşılanmadı veya doğrulanamadı."));
    }

    // ---- permissions, admin ---------------------------------------------------------------------------------------
    let needs_admin_maybe = steps.iter().any(|s| s.admin == AdminNeed::Maybe);
    let mut permissions = vec!["Geçerli kullanıcı hesabı (yönetici yetkisi gerekmediği varsayılır)".to_string()];
    if steps.iter().any(|s| s.network == NetworkUse::Yes) {
        permissions.push("İnternet erişimi (yalnızca onaylı bir uygulama aşamasında; bu önizleme bağlantı kurmadı)".to_string());
    }
    if steps.iter().any(|s| !s.writes.is_empty()) {
        permissions.push("Yukarıda sayılan klasörlere/dosyalara yazma".to_string());
    }
    if needs_admin_maybe {
        match host.elevated {
            Some(false) => warnings.push(blocker(
                "admin_may_be_needed",
                "Yönetici izniniz yok; bu paket yöneticisi bazı paketlerde izin isteyebilir. Yönetici izni verilmez.",
            )),
            Some(true) => warnings.push(blocker(
                "elevated_session",
                "Uygulama yükseltilmiş yetkiyle çalışıyor; kurulumun normal kullanıcı hesabıyla yapılması önerilir.",
            )),
            None => warnings.push(blocker("admin_unknown", "Yönetici izni gerekip gerekmediği bu yöntem için önceden bilinemez.")),
        }
    }

    // ---- network, telemetry, data ---------------------------------------------------------------------------------
    let mut destinations: Vec<String> = Vec::new();
    for s in &steps {
        if s.network == NetworkUse::Yes {
            let d = match s.kind {
                OperationKind::FetchReleaseArtifact => "GitHub sürüm indirme adresi (tarifte yazılı)",
                OperationKind::InstallPinnedPackage => "Paket yöneticisinin paket kaynağı",
                OperationKind::RegisterNativePlugin => "Ajanın eklenti/pazaryeri kaynağı",
                _ => "Ağ kullanımı",
            };
            if !destinations.iter().any(|x| x == d) {
                destinations.push(d.to_string());
            }
        }
    }
    let network = NetworkInfo { planned_use: !destinations.is_empty(), destinations, used_to_build_preview: false };
    let telemetry_word = record.pointer("/privacy/telemetry").and_then(Value::as_str).unwrap_or("unknown");
    let telemetry = match telemetry_word {
        "none-declared" => Disclosure {
            state: "none_declared".to_string(),
            text: "Yayıncı telemetri beyan etmiyor (bağımsız doğrulanmadı).".to_string(),
        },
        "opt-out" => Disclosure {
            state: "on_by_default".to_string(),
            text: "Yayıncıya göre telemetri varsayılan olarak AÇIK; kapatmak kullanıcıya bırakılmış.".to_string(),
        },
        "opt-in" => Disclosure {
            state: "opt_in".to_string(),
            text: "Yayıncıya göre telemetri yalnızca açıkça izin verilirse çalışır.".to_string(),
        },
        _ => Disclosure { state: "unknown".to_string(), text: "Telemetri durumu bilinmiyor.".to_string() },
    };
    let transmits = record.pointer("/privacy/mayTransmitProjectContent").and_then(Value::as_bool);
    let mentions_credentials = record
        .get("dependencies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|d| d.to_ascii_lowercase().contains("api key"));
    let data_and_credentials = Disclosure {
        state: match (transmits, mentions_credentials) {
            (_, true) => "credentials_declared",
            (Some(true), _) => "may_transmit",
            (Some(false), _) => "no_transmission_declared",
            (None, _) => "unknown",
        }
        .to_string(),
        text: format!(
            "{}{}",
            match transmits {
                Some(true) => "Yayıncıya göre proje içeriği dışarı gönderilebilir. ",
                Some(false) => "Yayıncı proje içeriği gönderimi beyan etmiyor (doğrulanmadı). ",
                None => "Proje içeriğinin gönderilip gönderilmediği bilinmiyor. ",
            },
            if mentions_credentials {
                "Yayıncı bir API anahtarı/kimlik bilgisi gerektirdiğini belirtiyor; bu uygulama anahtar istemez, saklamaz veya göndermez."
            } else {
                "Bu uygulama hiçbir kimlik bilgisi istemez veya saklamaz."
            }
        ),
    };
    let disk_usage = Disclosure {
        state: "unknown".to_string(),
        text: "Disk kullanımı güvenilir biçimde bilinmiyor (katalogda boyut bilgisi yok).".to_string(),
    };
    unknowns.push("Aracın işlevinin bu bilgisayarda çalıştığı doğrulanmadı ve bu planla doğrulanmayacak.".to_string());

    // ---- conflicts from stored observations (existing installations, copies, versions) ---------------------------
    let mine: Vec<&StoredObservation> = observed(observations, &entry.entry_id).into_iter().filter(|o| o.presence == "present").collect();
    if !mine.is_empty() {
        conflicts.push(Conflict {
            code: if this_matches { "existing_installation_found" } else { "existing_installation_other_platform" }.to_string(),
            text: if this_matches {
                "Son tarama bu aracın bu bilgisayarda zaten bulunduğunu gösteriyor; yeni kurulum mevcut kopyayla çakışabilir ve mevcut kopya değiştirilmez.".to_string()
            } else {
                "Bu bilgisayarda bu aracın bir kopyası bulunuyor, ancak plan başka bir platform içindir; sürüm/mimari uyumsuzluğu olabilir.".to_string()
            },
        });
        let locations: std::collections::BTreeSet<&str> =
            mine.iter().filter(|o| o.method == "path-executable").filter_map(|o| o.location.as_deref()).collect();
        let said_so = mine.iter().any(|o| o.limitations.iter().any(|l| l.contains("farklı kopya bulundu")));
        if locations.len() > 1 || said_so {
            conflicts.push(Conflict {
                code: "multiple_copies_found".to_string(),
                text: "PATH üzerinde birden çok kopya bulundu; hangisinin çalışacağı PATH sırasına bağlı.".to_string(),
            });
        }
        let versions: std::collections::BTreeSet<&str> = mine.iter().filter_map(|o| o.version.as_deref()).collect();
        if versions.len() > 1 {
            conflicts.push(Conflict {
                code: "conflicting_version_records".to_string(),
                text: "Aynı araç için çelişen sürüm kayıtları var; hangisinin doğru olduğu bilinmiyor.".to_string(),
            });
        }
        if mine.iter().any(|o| o.ladder.identity == crate::discovery::types::IdentityState::Conflicted) {
            conflicts.push(Conflict {
                code: "conflicting_identity_records".to_string(),
                text: "Aynı ada sahip dosya doğrulanmış bir referansla çelişiyor; bu araç adına güvenilmemeli.".to_string(),
            });
        }
    }
    for c in &conflicts {
        if c.code != "active_session_unknown" {
            warnings.push(blocker(&c.code, &c.text));
        }
    }

    // ---- rollback ---------------------------------------------------------------------------------------------------
    let rollback = RollbackInfo {
        available: false,
        reliability: "none_implemented".to_string(),
        text: match spec.map(|s| s.kind) {
            Some(MethodKind::ReleaseArchive) => "Geri alma bu sürümde YOK. Tasarım amacı: sürümlü klasörde önceki sürüme atomik dönüş; aracın kendi verileri (indeksler vb.) bu kapsamda değil.",
            Some(MethodKind::NativePlugin) => "Geri alma bu sürümde YOK. Eklenti ajanın kendi yöneticisiyle kaldırılabilir; ayar/kayıt dosyalarının önceki hâlini geri yükleme henüz yok.",
            Some(MethodKind::Package(_)) => "Geri alma bu sürümde YOK ve paket yöneticisi için güvenilir biçimde tanımlanamıyor.",
            None => "Geri alma bu sürümde YOK; plan için bir yöntem de seçilemedi.",
        }
        .to_string(),
    };

    // ---- assemble ---------------------------------------------------------------------------------------------------
    let repo = format!("{}/{}", str_at(record, "/repository/owner"), str_at(record, "/repository/name"));
    let plan_entry = PlanEntry {
        entry_id: entry.entry_id.clone(),
        display_name: sanitize_claim(&entry.display_name),
        kind: entry.kind.clone(),
        source: if repo.len() > 1 { sanitize_claim(&repo) } else { "bilinmiyor".to_string() },
        review_state: entry.review_state.clone(),
        execution_policy: entry.execution_policy.clone(),
        license_state: entry.license_state.clone(),
        revision: sanitize_claim(&entry.revision),
        immutable_commit_pinned: pinned,
    };
    let target = PlanTarget {
        platform_target: req.platform_target.clone(),
        agent_surface: req.agent_surface.clone(),
        scope: req.scope.clone(),
        this_computer: host.platform.clone(),
        this_computer_matches: this_matches,
        recipe_target_id: recipe_target_id.map(str::to_string),
        catalog_compatibility: compat,
    };
    // de-duplicate blockers by code (a code is one reason, stated once)
    let mut seen = std::collections::BTreeSet::new();
    blockers.retain(|b| seen.insert(b.code.clone()));
    let mut seen_w = std::collections::BTreeSet::new();
    warnings.retain(|b| seen_w.insert(b.code.clone()));

    let mut plan = InstallPlan {
        schema_version: PLAN_SCHEMA_VERSION,
        planner_version: PLANNER_VERSION.to_string(),
        plan_id: String::new(),
        state: "preview_only".to_string(),
        executable: false,
        executed: false,
        modifies_anything: false,
        statement: PREVIEW_STATEMENT.to_string(),
        entry: plan_entry,
        target,
        eligibility: Eligibility { status: "blocked".to_string(), can_execute: false, blockers, warnings },
        methods,
        chosen_method: chosen.map(|(id, _)| id).filter(|id| method_spec(id).is_some()),
        prerequisites,
        declared_dependencies,
        steps,
        permissions,
        network,
        telemetry,
        data_and_credentials,
        disk_usage,
        config_changes,
        conflicts,
        uninstall,
        rollback,
        unknowns,
    };
    let content = serde_json::to_value(&plan).map_err(|_| AppError::new("PLAN_SERIALIZE", "Plan serileştirilemedi."))?;
    plan.plan_id = sha256_hex(&canonical_json(&content));
    plan.check_invariants().map_err(|_| AppError::new("PLAN_INVARIANT", "Plan değişmezleri sağlanamadı."))?;
    Ok(plan)
}

/// The read-only entry point: loads the catalog entry and the latest stored observations and builds the preview.
/// Writes nothing, reads no file and starts no process.
pub fn plan_for(conn: &Connection, req: &PlanRequest, host: &HostFacts) -> AppResult<InstallPlan> {
    validate_request(req)?;
    let entry = catalog::get_entry(conn, &req.entry_id)?;
    let observations = store::latest_observations(conn)?;
    build_plan(&entry, req, host, &observations)
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
