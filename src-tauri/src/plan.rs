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
    discovery::reference::ReferenceSet,
    discovery::store::{self, StoredObservation},
    error::{AppError, AppResult},
    recipe::{self, RecipeRegistry, ReviewedRecipe, ValidatedRecipe},
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
    /// Required for `scope = project` (a project reference of this application), forbidden otherwise.
    #[serde(default)]
    pub project_alias: Option<String>,
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
    ManagerState,
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
            WriteArea::ManagerState => "Yöneticinin kendi durum, günce ve makbuz klasörleri",
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Blocker {
    pub code: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Prerequisite {
    pub id: String,
    pub text: String,
    /// `met` | `unmet` | `unknown`
    pub state: String,
    /// `observed_scan` | `publisher_declared` | `not_checked`
    pub source: String,
    /// What the state rests on: `current_scan` (a current, applicable observation), `no_current_evidence` (nothing current and
    /// applicable exists; unknown, not absent), `publisher_declared`, `not_checked`.
    pub basis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Conflict {
    pub code: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    /// `none` | `malformed` | `declared_unverified`. A well-formed commit id is only a DECLARED pin: no length or
    /// pattern check can prove that it belongs to the repository or was reviewed.
    pub source_pin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Eligibility {
    /// Always `blocked` in this phase.
    pub status: String,
    pub can_execute: bool,
    pub blockers: Vec<Blocker>,
    pub warnings: Vec<Blocker>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkInfo {
    /// Would the (future) steps use the network?
    pub planned_use: bool,
    pub destinations: Vec<String>,
    /// Always false: building this preview made no connection.
    pub used_to_build_preview: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Disclosure {
    /// Closed word, see the builder.
    pub state: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigChange {
    pub area: WriteArea,
    /// `create_or_merge` | `unknown`
    pub change: String,
    pub text: String,
}

/// What the plan knows about a reviewed recipe for this exact question. Recipe review is not authorization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeStatus {
    /// `none` (no recipe exists for this question) | `invalid` (a recipe exists but fails validation) | `validated`
    pub state: String,
    pub recipe_id: Option<String>,
    pub revision: Option<u32>,
    pub recipe_digest: Option<String>,
    pub synthetic: bool,
    /// Closed problem codes of `recipe::problems` (empty when validated).
    pub problems: Vec<String>,
    /// `designed` | `simulated` (never `verified` in this phase), only for a validated recipe.
    pub rollback_state: Option<String>,
    /// `none` | `authenticated_reference`
    pub authentication: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RollbackInfo {
    /// Always false in this phase: nothing is implemented.
    pub available: bool,
    /// `none_implemented`
    pub reliability: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    pub recipe: RecipeStatus,
    /// Always `not_given`. A plan is a draft; an approval is a separate, bound object (`approval.rs`) and is never part of a plan.
    pub approval_state: String,
}

impl InstallPlan {
    /// A plan is data. Anything that claims to be executable, executed, or in a state other than `preview_only` is
    /// rejected, whether it was built here or read back from JSON.
    pub fn check_invariants(&self) -> Result<(), &'static str> {
        if self.schema_version != PLAN_SCHEMA_VERSION {
            return Err("unsupported_schema_version");
        }
        if self.planner_version != PLANNER_VERSION {
            return Err("unsupported_planner_version");
        }
        if self.approval_state != "not_given" {
            return Err("plan_claims_approval");
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

/// SHA-256 over the canonical plan content with the id blanked: exactly how [`build_plan`] derived `plan_id`.
fn content_digest(plan: &InstallPlan) -> Result<String, &'static str> {
    let mut blank = plan.clone();
    blank.plan_id = String::new();
    let value = serde_json::to_value(&blank).map_err(|_| "plan_not_readable")?;
    Ok(sha256_hex(&canonical_json(&value)))
}

const ACTIVATION_SLOTS: &[&str] = &["manager-versioned-binary", "native-plugin-manager", "managed-instruction-block"];
const ACTIVATION_METHODS: &[&str] = &["atomic-version-switch", "host-native-registration", "marker-delimited-merge"];
const PACKAGE_MANAGERS: &[&str] = &["homebrew", "winget", "uv-tool", "pipx", "go-install", "native-plugin-manager"];
const PLUGIN_HOSTS: &[&str] = &["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "antigravity-cli", "antigravity-2"];
const INSTRUCTION_TARGETS: &[&str] = &["CLAUDE.md", "GEMINI.md", "AGENTS.md", "agents-rules"];
const INSTRUCTION_TEMPLATES: &[&str] = &["response-profile-caveman", "response-profile-ste-oriented", "tool-discovery-pointer"];

fn is_package_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 180 && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'@' | b'.' | b'_' | b'/' | b'-'))
}

/// A typed operation must be exactly the schema's shape for its kind: the declared key set, closed values, valid patterns.
pub(crate) fn typed_is_valid(kind: OperationKind, typed: &Value, target_surface: &str) -> bool {
    let Some(obj) = typed.as_object() else { return false };
    let keys: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
    let want = |names: &[&str]| keys == names.iter().copied().collect::<std::collections::BTreeSet<&str>>();
    let text = |k: &str| obj.get(k).and_then(Value::as_str);
    if text("operation") != Some(kind.as_str()) {
        return false;
    }
    match kind {
        OperationKind::FetchReleaseArtifact => want(&["operation", "artifactId"]) && text("artifactId") == Some("primary"),
        OperationKind::VerifyArtifactDigest => {
            want(&["operation", "artifactId", "expectedSha256"])
                && text("artifactId") == Some("primary")
                && text("expectedSha256").map(is_sha256).unwrap_or(false)
        }
        OperationKind::StagePrivateCopy => {
            want(&["operation", "artifactId", "destinationId"])
                && text("artifactId") == Some("primary")
                && text("destinationId") == Some("manager-owned-staging")
        }
        OperationKind::ActivateStagedArtifact => {
            want(&["operation", "artifactId", "activationSlot", "activationMethod"])
                && text("artifactId") == Some("primary")
                && text("activationSlot").map(|v| ACTIVATION_SLOTS.contains(&v)).unwrap_or(false)
                && text("activationMethod").map(|v| ACTIVATION_METHODS.contains(&v)).unwrap_or(false)
        }
        OperationKind::InstallPinnedPackage => {
            want(&["operation", "packageManager", "packageName", "version", "lifecycleScripts"])
                && text("packageManager").map(|v| PACKAGE_MANAGERS.contains(&v)).unwrap_or(false)
                && text("packageName").map(is_package_name).unwrap_or(false)
                && text("version").map(is_pinned_version).unwrap_or(false)
                && text("lifecycleScripts") == Some("disabled")
        }
        OperationKind::RegisterNativePlugin => {
            want(&["operation", "host", "pluginId"])
                && text("host").map(|v| PLUGIN_HOSTS.contains(&v) && v == target_surface).unwrap_or(false)
                && text("pluginId").map(is_plugin_id).unwrap_or(false)
        }
        OperationKind::WriteManagedInstructionBlock => {
            want(&["operation", "instructionTarget", "templateId"])
                && text("instructionTarget").map(|v| INSTRUCTION_TARGETS.contains(&v)).unwrap_or(false)
                && text("templateId").map(|v| INSTRUCTION_TEMPLATES.contains(&v)).unwrap_or(false)
        }
    }
}

fn step_expects_network(kind: OperationKind) -> NetworkUse {
    match kind {
        OperationKind::FetchReleaseArtifact | OperationKind::InstallPinnedPackage | OperationKind::RegisterNativePlugin => NetworkUse::Yes,
        _ => NetworkUse::No,
    }
}

impl InstallPlan {
    /// Internal consistency a plan built by [`build_plan`] always has. A parsed plan that fails any of these was not produced
    /// by this planner (or was edited afterwards).
    pub fn check_semantics(&self) -> Result<(), &'static str> {
        let t = &self.target;
        if !PLATFORMS.contains(&t.platform_target.as_str())
            || !SURFACES.contains(&t.agent_surface.as_str())
            || !SCOPES.contains(&t.scope.as_str())
            || !(PLATFORMS.contains(&t.this_computer.as_str()) || t.this_computer == "unsupported")
        {
            return Err("target_not_in_vocabulary");
        }
        if t.recipe_target_id.as_deref() != recipe_target(&t.platform_target, &t.agent_surface)
            || t.this_computer_matches != (t.this_computer == t.platform_target)
        {
            return Err("target_inconsistent");
        }
        if !matches!(self.entry.source_pin.as_str(), "none" | "malformed" | "declared_unverified" | "authenticated") {
            return Err("source_pin_not_in_vocabulary");
        }
        let validated = self.recipe.state == "validated";
        if !matches!(self.recipe.state.as_str(), "none" | "invalid" | "validated")
            || (validated && !(self.recipe.recipe_digest.is_some() && self.recipe.recipe_id.is_some()))
            || (self.entry.source_pin == "authenticated" && !validated)
            || (validated && !self.recipe.problems.is_empty())
            || (!validated && self.recipe.rollback_state.is_some())
            || !matches!(self.recipe.authentication.as_str(), "none" | "authenticated_reference")
        {
            return Err("recipe_status_inconsistent");
        }
        let codes: Vec<&str> = self.eligibility.blockers.iter().map(|b| b.code.as_str()).collect();
        let unique = |v: &[&str]| v.iter().collect::<std::collections::BTreeSet<_>>().len() == v.len();
        let recipe_blockers_ok = if validated {
            codes.contains(&"user_approval_missing") && !codes.contains(&"no_approved_recipe")
        } else {
            codes.contains(&"no_approved_recipe") && !codes.contains(&"user_approval_missing")
        };
        if !recipe_blockers_ok || !unique(&codes) {
            return Err("blockers_inconsistent");
        }
        let expected_reliability =
            if validated { self.recipe.rollback_state.as_deref().unwrap_or("none_implemented") } else { "none_implemented" };
        if self.rollback.reliability != expected_reliability
            || !matches!(expected_reliability, "none_implemented" | "designed" | "simulated")
        {
            return Err("rollback_inconsistent");
        }
        // steps: sequential, a known template, typed form consistent with the kind, derived texts unchanged
        let kinds: Vec<OperationKind> = self.steps.iter().map(|s| s.kind).collect();
        let template_ok = kinds.is_empty()
            || kinds == [OperationKind::RegisterNativePlugin]
            || kinds == [OperationKind::InstallPinnedPackage]
            || kinds
                == [
                    OperationKind::FetchReleaseArtifact,
                    OperationKind::VerifyArtifactDigest,
                    OperationKind::StagePrivateCopy,
                    OperationKind::ActivateStagedArtifact,
                ];
        if !template_ok {
            return Err("step_order_not_a_known_template");
        }
        for (i, s) in self.steps.iter().enumerate() {
            if s.index != i as u32 + 1 {
                return Err("step_index_inconsistent");
            }
            // A catalog-derived plan follows the fixed per-kind network rule; a recipe-backed plan follows what its recipe declares
            // (the fetch step uses the network only if destinations are declared; nothing else ever does).
            let network_ok = if validated {
                match s.kind {
                    OperationKind::FetchReleaseArtifact => (s.network == NetworkUse::Yes) == !self.network.destinations.is_empty(),
                    _ => s.network == NetworkUse::No,
                }
            } else {
                s.network == step_expects_network(s.kind)
            };
            if s.write_labels != s.writes.iter().map(|w| w.label().to_string()).collect::<Vec<_>>() || !network_ok {
                return Err("step_effects_inconsistent");
            }
            match &s.typed {
                Some(v) => {
                    if !s.missing_parameters.is_empty() || !typed_is_valid(s.kind, v, &t.agent_surface) {
                        return Err("typed_operation_invalid");
                    }
                }
                None => {
                    if s.missing_parameters.is_empty() {
                        return Err("untyped_step_without_missing_parameters");
                    }
                }
            }
        }
        if validated
            && (self.steps.len() != 4
                || self.steps.iter().any(|s| s.typed.is_none())
                || self.chosen_method.as_deref() != Some("verified-release-archive"))
        {
            return Err("validated_recipe_without_complete_steps");
        }
        if self.network.planned_use != self.steps.iter().any(|s| s.network == NetworkUse::Yes) {
            return Err("network_inconsistent");
        }
        let methods_ok = self.methods.iter().all(|m| !m.supported || method_spec(&m.method_id).is_some());
        let chosen_ok = match &self.chosen_method {
            Some(c) => self.methods.iter().any(|m| &m.method_id == c) && method_spec(c).is_some(),
            None => self.steps.is_empty(),
        };
        if !methods_ok || !chosen_ok {
            return Err("method_inconsistent");
        }
        Ok(())
    }
}

/// Reads a plan from JSON and rejects anything that is not a well-formed, versioned, non-executing preview whose content
/// still matches its id. A matching id shows the content was not altered after generation; it is NOT tamper-proof against a
/// writer who recomputes it, and it is never an approval or an authorization.
#[allow(dead_code)] // exercised by the tests and the self-check; no command accepts a plan from outside
pub fn parse_plan(json: &str) -> Result<InstallPlan, &'static str> {
    let plan: InstallPlan = serde_json::from_str(json).map_err(|_| "plan_not_readable")?;
    plan.check_invariants()?;
    plan.check_semantics()?;
    if plan.plan_id.len() != 64 || content_digest(&plan)? != plan.plan_id {
        return Err("plan_id_does_not_match_content");
    }
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
    match (r.scope.as_str(), &r.project_alias) {
        ("project", Some(a)) if crate::discovery::privacy::is_safe_project_alias(a) => {}
        ("project", _) => return Err(invalid("proje kapsamı için geçerli bir proje takma adı gerekir")),
        (_, Some(_)) => return Err(invalid("proje takma adı yalnızca proje kapsamında verilebilir")),
        _ => {}
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

/// A git object id as the recipe schema writes it: exactly 40 or 64 LOWER-case hexadecimal characters.
fn is_commit_id(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
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

/// Which part of the stored observations may speak about the question being asked.
#[derive(Clone, Copy)]
enum Want<'a> {
    /// computer-wide (not tied to a project)
    Global,
    Project(&'a str),
}

/// What the observations of one subject say, sorted by how far they can be trusted for THIS question.
#[derive(Default)]
struct Evidence<'a> {
    /// current (from the latest scan, fresh), same platform, same surface, same scope: may satisfy a prerequisite
    current_present: Vec<&'a StoredObservation>,
    /// current, applicable and from a scan that was authorised to look: the only thing that can mean "not there"
    current_absent: Vec<&'a StoredObservation>,
    /// applicable but not current (older scan, stale, or the latest scan did not look): history only
    historical_present: usize,
    /// present, but recorded for another platform
    foreign_platform_present: usize,
    /// present on this platform but for another agent surface (never merged with the asked one)
    other_surface_present: usize,
    /// rows that say `current` but contradict their own provenance (unauthorised area, not from the latest scan)
    inconsistent: usize,
}

fn scan_looked(o: &StoredObservation, want: Want) -> bool {
    match o.method.as_str() {
        "path-executable" | "native-install-location" | "legacy-local-install" => o.scan_authorized_path,
        "claude-settings-file" | "claude-plugin-declaration" | "claude-mcp-config" => match want {
            Want::Global => o.scan_authorized_user_config,
            Want::Project(_) => o.scan_authorized_project,
        },
        _ => true,
    }
}

/// One rule for positive and negative evidence alike: a current row from the latest scan, for an area that scan checked, with
/// the scan's own permission flags agreeing. A stored `current` flag alone proves nothing.
fn provenance_ok(o: &StoredObservation, want: Want) -> bool {
    o.current && o.provenance == "latest_scan" && o.latest_scan_checked && scan_looked(o, want)
}

fn in_scope(o: &StoredObservation, want: Want) -> bool {
    match want {
        Want::Global => o.scope == "global" && o.project_alias.is_none(),
        Want::Project(alias) => o.scope == "project" && o.project_alias.as_deref() == Some(alias),
    }
}

fn gather<'a>(obs: &'a [StoredObservation], subject: &str, platform: &str, surface: Option<&str>, want: Want) -> Evidence<'a> {
    let mut e = Evidence::default();
    for o in obs.iter().filter(|o| o.subject == subject && in_scope(o, want)) {
        let present = o.presence == "present";
        if o.platform_target != platform {
            e.foreign_platform_present += present as usize;
            continue;
        }
        if surface.map(|s| o.agent_surface != s).unwrap_or(false) {
            e.other_surface_present += present as usize;
            continue;
        }
        // Positive and negative evidence need the SAME provenance: a current row from the latest scan, for an area that
        // scan checked, with the scan's own permission flags agreeing. A stored `current` flag alone proves nothing.
        let provenance_ok = provenance_ok(o, want);
        if provenance_ok {
            if present {
                e.current_present.push(o);
            } else if o.presence == "absent" {
                e.current_absent.push(o);
            }
        } else if o.current {
            // claims to be current but fails its own provenance: untrusted, kept only as a caveat
            e.inconsistent += 1;
        } else if present {
            e.historical_present += 1;
        }
    }
    e
}

/// The one agent a surface can be judged by. Surfaces without a detector (Claude Desktop, Antigravity CLI / 2.0) are never
/// borrowed from another surface's evidence.
fn agent_subject(surface: &str) -> Option<&'static str> {
    match surface {
        "claude-code-terminal" => Some("agent:claude-code"),
        "antigravity-ide" => Some("agent:antigravity-ide"),
        _ => None,
    }
}

fn evidence_notes(e: &Evidence, what: &str, warnings: &mut Vec<Blocker>) {
    if e.current_present.is_empty() && e.historical_present > 0 {
        warnings.push(blocker(
            "historical_presence_not_current",
            &format!("{what}: eski bir taramada bulunmuştu, ancak son taramada doğrulanmadı. Eski kayıt güncel kanıt sayılmaz."),
        ));
    }
    if e.inconsistent > 0 {
        warnings.push(blocker(
            "inconsistent_observation_ignored",
            &format!("{what}: kayıt son taramadan görünüyor ancak taramanın izni/kapsamıyla çelişiyor; güvenilmez sayıldı ve kanıt olarak kullanılmadı."),
        ));
    }
    if e.foreign_platform_present > 0 {
        warnings.push(blocker(
            "foreign_platform_record",
            &format!("{what}: başka bir platform için kayıt var; seçilen hedef için kanıt sayılmaz."),
        ));
    }
    if e.other_surface_present > 0 {
        warnings.push(blocker(
            "other_surface_record",
            &format!("{what}: başka bir ajan yüzeyi için kayıt var; seçilen yüzeyle birleştirilmedi."),
        ));
    }
}

fn build_draft(entry: &CatalogEntry, req: &PlanRequest, host: &HostFacts, observations: &[StoredObservation]) -> AppResult<InstallPlan> {
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
    let source_pin = match record.pointer("/sourceSnapshot/immutableCommit").and_then(Value::as_str) {
        None => "none",
        Some(c) if is_commit_id(c) => "declared_unverified",
        Some(_) => "malformed",
    };
    match source_pin {
        "declared_unverified" => blockers.push(blocker(
            "source_origin_unverified",
            "Katalogda bir commit numarası bildirilmiş, ancak bu yalnızca beyandır: numaranın bu depoya ait olduğu veya incelendiği doğrulanmadı.",
        )),
        "malformed" => {
            blockers.push(blocker("source_pin_malformed", "Katalogdaki commit numarası geçerli bir 40 veya 64 haneli onaltılık değer değil; yok sayıldı."));
            blockers.push(blocker("source_not_pinned", "Kaynak geçerli bir commit'e sabitlenmemiş; içerik zamanla değişebilir."));
        }
        _ => blockers.push(blocker("source_not_pinned", "Kaynak değişmez bir commit'e sabitlenmemiş; içerik zamanla değişebilir.")),
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
                    &[WriteArea::ManagerVersionedFolder, WriteArea::ManagerState],
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
                    basis: "not_checked".to_string(),
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
    let project_want = req.project_alias.as_deref().map(Want::Project).unwrap_or(Want::Global);
    if req.agent_surface != "local-cli" {
        let subject = agent_subject(&req.agent_surface);
        let (state, text, basis) = match subject {
            None => {
                ("unknown", "Bu ajan yüzeyi için bir algılayıcı yok; başka bir yüzeyin kaydı yerine kullanılmaz.", "no_current_evidence")
            }
            Some(subject) => {
                // the agent itself is machine-wide: global observations of exactly this surface
                let e = gather(observations, subject, &req.platform_target, Some(&req.agent_surface), Want::Global);
                evidence_notes(&e, "Ajan", &mut warnings);
                if !e.current_present.is_empty() {
                    ("met", "Ajan, GÜNCEL taramada bulundu (yalnızca varlığı; çalıştığı doğrulanmadı).", "current_scan")
                } else if !e.current_absent.is_empty() {
                    ("unmet", "Ajan, güncel ve izinli taramada arandı ve bulunamadı.", "current_scan")
                } else {
                    (
                        "unknown",
                        "Ajanın bu hedefte olup olmadığı bilinmiyor: bu hedef için güncel, uygulanabilir ve izinli bir tarama kanıtı yok.",
                        "no_current_evidence",
                    )
                }
            }
        };
        prerequisites.push(Prerequisite {
            id: "agent_present".to_string(),
            text: text.to_string(),
            state: state.to_string(),
            source: "observed_scan".to_string(),
            basis: basis.to_string(),
        });
        if subject == Some("agent:claude-code") {
            let settings_any = observations.iter().find(|o| {
                o.subject == "agent:claude-code"
                    && o.method == "claude-settings-file"
                    && o.platform_target == req.platform_target
                    && o.agent_surface == req.agent_surface
                    && in_scope(o, project_want)
                    && provenance_ok(o, project_want)
            });
            let which = if req.project_alias.is_some() { "Projenin" } else { "Kullanıcının" };
            let (state, text, basis) = match settings_any.map(|o| (o.configuration.as_str(), scan_looked(o, project_want))) {
                Some(("malformed", true)) => (
                    "unmet",
                    format!("{which} Claude Code ayar dosyası bozuk görünüyor; plan bunu düzeltmeyi önermez ve üzerine yazmaz."),
                    "current_scan",
                ),
                Some(("configured" | "disabled" | "referenced_not_present", true)) => {
                    ("met", format!("{which} Claude Code ayar dosyası güncel taramada okunabildi."), "current_scan")
                }
                _ => (
                    "unknown",
                    format!("{which} Claude Code ayar dosyasının durumu bilinmiyor (güncel taramada okunmadı veya izin verilmedi)."),
                    "no_current_evidence",
                ),
            };
            prerequisites.push(Prerequisite {
                id: "agent_config_readable".to_string(),
                text,
                state: state.to_string(),
                source: "observed_scan".to_string(),
                basis: basis.to_string(),
            });
        }
        if prerequisites.iter().find(|p| p.id == "agent_present").map(|p| p.state != "met").unwrap_or(true) {
            blockers.push(blocker("agent_not_confirmed", "Hedef ajanın bu hedefte bulunduğu güncel kanıtla doğrulanmadı."));
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
            basis: "publisher_declared".to_string(),
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
    let tool = gather(observations, &entry.entry_id, &req.platform_target, None, Want::Global);
    evidence_notes(&tool, "Araç", &mut warnings);
    let mine = &tool.current_present;
    if !mine.is_empty() {
        conflicts.push(Conflict {
            code: "existing_installation_found".to_string(),
            text: "Son (güncel) tarama bu aracın bu hedefte zaten bulunduğunu gösteriyor; yeni kurulum mevcut kopyayla çakışabilir ve mevcut kopya değiştirilmez.".to_string(),
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
                text: "Aynı araç için güncel kayıtlarda çelişen sürümler var; hangisinin doğru olduğu bilinmiyor.".to_string(),
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
        source_pin: source_pin.to_string(),
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
        recipe: RecipeStatus {
            state: "none".to_string(),
            recipe_id: None,
            revision: None,
            recipe_digest: None,
            synthetic: false,
            problems: Vec::new(),
            rollback_state: None,
            authentication: "none".to_string(),
        },
        approval_state: "not_given".to_string(),
    };
    let content = serde_json::to_value(&plan).map_err(|_| AppError::new("PLAN_SERIALIZE", "Plan serileştirilemedi."))?;
    plan.plan_id = sha256_hex(&canonical_json(&content));
    plan.check_invariants().map_err(|_| AppError::new("PLAN_INVARIANT", "Plan değişmezleri sağlanamadı."))?;
    Ok(plan)
}

/// The read-only entry point: loads the catalog entry and the latest stored observations and builds the preview.
/// Writes nothing, reads no file and starts no process.
/// The plan for a question, with no recipe: what the catalog alone can say. Always blocked by `no_approved_recipe`.
#[allow(dead_code)] // the catalog-only form is what most tests and the draft preview use
pub fn build_plan(entry: &CatalogEntry, req: &PlanRequest, host: &HostFacts, observations: &[StoredObservation]) -> AppResult<InstallPlan> {
    build_plan_with(entry, req, host, observations, &[], &ReferenceSet::default())
}

const RECIPE_REPLACES: &[&str] = &[
    "no_approved_recipe",
    "metadata_unreviewed",
    "license_not_accepted",
    "source_not_pinned",
    "source_pin_malformed",
    "source_origin_unverified",
    "no_artifact_digest",
    "package_name_unknown",
    "version_not_pinned",
    "target_not_in_recipe_schema",
    "compatibility_not_verified",
    "no_supported_method",
    "method_not_declared",
    "method_not_for_platform",
    "method_not_for_surface",
    "unsupported_install_method",
];

fn permission_text(id: &str) -> &'static str {
    match id {
        "current_user_account" => "Geçerli kullanıcı hesabı (yönetici yetkisi gerekmez)",
        "network_download" => "İnternet erişimi (yalnızca onaylı bir uygulama aşamasında)",
        _ => "Yöneticinin kendi klasörlerine yazma",
    }
}

fn network_text(id: &str) -> &'static str {
    match id {
        "github_release_download" => "GitHub sürüm indirme adresi (tarifte yazılı)",
        "package_registry" => "Paket yöneticisinin paket kaynağı",
        _ => "Ajanın eklenti/pazaryeri kaynağı",
    }
}

/// The plan for a question, looking at the reviewed recipes offered for it. A recipe changes what the plan says only when it
/// is *validated* (reviewed at its current content, independently authenticated artifact, accepted license, real rollback
/// contract, complete typed steps) AND matches the exact question. Even then the plan stays a blocked draft: the only new
/// blocker is `user_approval_missing`, and nothing in the plan is an approval.
pub fn build_plan_with(
    entry: &CatalogEntry,
    req: &PlanRequest,
    host: &HostFacts,
    observations: &[StoredObservation],
    candidates: &[&ReviewedRecipe],
    refs: &ReferenceSet,
) -> AppResult<InstallPlan> {
    let mut plan = build_draft(entry, req, host, observations)?;
    // Several candidate recipes for one question are ambiguous: none is used.
    let candidate = match candidates {
        [] => return Ok(plan),
        [one] => *one,
        _ => {
            plan.recipe =
                RecipeStatus { state: "invalid".to_string(), problems: vec!["ambiguous_recipes".to_string()], ..plan.recipe.clone() };
            return finish(plan);
        }
    };
    match recipe::validate(candidate, refs) {
        Err(problems) => {
            plan.recipe = RecipeStatus {
                state: "invalid".to_string(),
                recipe_id: Some(candidate.recipe.recipe_id.clone()),
                revision: Some(candidate.recipe.revision),
                recipe_digest: Some(recipe::recipe_digest(&candidate.recipe)),
                synthetic: candidate.recipe.synthetic,
                problems: problems.iter().map(|p| p.to_string()).collect(),
                rollback_state: None,
                authentication: "none".to_string(),
            };
            finish(plan)
        }
        Ok(valid) => {
            let r = valid.recipe();
            let exact = r.tool_id == entry.entry_id
                && r.target.platform_target == req.platform_target
                && r.target.agent_surface == req.agent_surface
                && r.target.scope == req.scope
                && matches!(req.method_id.as_deref(), None | Some("verified-release-archive"));
            if !exact {
                plan.recipe = RecipeStatus {
                    state: "invalid".to_string(),
                    recipe_id: Some(r.recipe_id.clone()),
                    revision: Some(r.revision),
                    recipe_digest: Some(valid.digest().to_string()),
                    synthetic: r.synthetic,
                    problems: vec!["recipe_does_not_match_the_question".to_string()],
                    rollback_state: None,
                    authentication: "none".to_string(),
                };
                return finish(plan);
            }
            overlay(&mut plan, &valid);
            finish(plan)
        }
    }
}

fn finish(mut plan: InstallPlan) -> AppResult<InstallPlan> {
    plan.plan_id = content_digest(&plan).map_err(|_| AppError::new("PLAN_SERIALIZE", "Plan serileştirilemedi."))?;
    plan.check_invariants().map_err(|_| AppError::new("PLAN_INVARIANT", "Plan değişmezleri sağlanamadı."))?;
    plan.check_semantics().map_err(|e| AppError::new("PLAN_INVARIANT", format!("Plan tutarlılığı sağlanamadı: {e}.")))?;
    Ok(plan)
}

fn overlay(plan: &mut InstallPlan, valid: &ValidatedRecipe) {
    let r = valid.recipe();
    plan.eligibility.blockers.retain(|b| !RECIPE_REPLACES.contains(&b.code.as_str()));
    plan.eligibility.blockers.push(blocker(
        "user_approval_missing",
        "Tarif incelenmiş olsa da bu plan için kullanıcı onayı yok. Onay, tarife ve bu tam plana bağlı ayrı bir işlemdir; plan kimliği bir onay değildir.",
    ));
    // the source: a commit is attested only by independent evidence, which today only a synthetic recipe can carry
    if r.source.source_attested {
        plan.entry.source_pin = "authenticated".to_string();
    } else {
        plan.entry.source_pin = "declared_unverified".to_string();
        plan.eligibility.blockers.push(blocker(
            "source_origin_unverified",
            "Tarifteki commit numarası bağımsız bir kanıtla depoya bağlanmadı; yalnızca beyandır.",
        ));
    }
    plan.methods = vec![MethodChoice {
        method_id: "verified-release-archive".to_string(),
        label: "Doğrulanmış sürüm dosyası (yöneticinin kendi klasörüne)".to_string(),
        supported: true,
        note: String::new(),
    }];
    plan.chosen_method = Some("verified-release-archive".to_string());
    let admin = AdminNeed::No;
    let kinds = [
        (OperationKind::FetchReleaseArtifact, NetworkUse::Yes, vec![WriteArea::ManagerStaging], Reversibility::NothingToUndo),
        (OperationKind::VerifyArtifactDigest, NetworkUse::No, vec![], Reversibility::NothingToUndo),
        (OperationKind::StagePrivateCopy, NetworkUse::No, vec![WriteArea::ManagerStaging], Reversibility::NothingToUndo),
        (
            OperationKind::ActivateStagedArtifact,
            NetworkUse::No,
            vec![WriteArea::ManagerVersionedFolder, WriteArea::ManagerState],
            Reversibility::VersionSwitch,
        ),
    ];
    plan.steps = kinds
        .into_iter()
        .zip(r.steps.iter())
        .enumerate()
        .map(|(i, ((kind, net, writes, rev), typed))| {
            let mut st = step(i as u32 + 1, kind, net, &writes, admin, rev);
            st.typed = Some(typed.clone());
            st
        })
        .collect();
    // the fetch step uses a network only if the recipe declares destinations for it (a synthetic recipe declares none)
    for st in plan.steps.iter_mut() {
        st.network = if st.kind == OperationKind::FetchReleaseArtifact && !r.network.is_empty() { NetworkUse::Yes } else { NetworkUse::No };
    }
    plan.config_changes = Vec::new();
    plan.permissions = r.permissions.iter().map(|p| permission_text(p).to_string()).collect();
    let uses_network = plan.steps.iter().any(|s| s.network == NetworkUse::Yes);
    plan.network.planned_use = uses_network;
    plan.network.destinations =
        if r.network.is_empty() { Vec::new() } else { r.network.iter().map(|n| network_text(n).to_string()).collect() };
    plan.uninstall = Disclosure {
        state: "manager_owned".to_string(),
        text: "Yöneticinin sürümlü klasörü kaldırılır; aracın kendi ürettiği kullanıcı verileri kaldırma ve geri alma kapsamında DEĞİLDİR ve kalır.".to_string(),
    };
    let designed = r.rollback.state == "designed";
    plan.rollback = RollbackInfo {
        available: false,
        reliability: r.rollback.state.clone(),
        text: format!(
            "Geri alma sözleşmesi {}; bu sürümde gerçek bir araçta DOĞRULANMADI. Geri alınabilenler: {}. Geri alınmayanlar: {}.",
            if designed { "yalnızca TASARLANMIŞ" } else { "yalnızca SENTETİK olarak benzetildi" },
            r.rollback.undoes.join(", "),
            r.rollback.does_not_undo.join(", ")
        ),
    };
    plan.unknowns.push(if r.synthetic {
        "Bu tarif SENTETİKTİR: gerçek bir araca ait değildir ve hiçbir gerçek kurulumu temsil etmez.".to_string()
    } else {
        "Tarif incelenmiş olsa da gerçek bir hedefte denenmedi.".to_string()
    });
    plan.recipe = RecipeStatus {
        state: "validated".to_string(),
        recipe_id: Some(r.recipe_id.clone()),
        revision: Some(r.revision),
        recipe_digest: Some(valid.digest().to_string()),
        synthetic: r.synthetic,
        problems: Vec::new(),
        rollback_state: Some(r.rollback.state.clone()),
        authentication: "authenticated_reference".to_string(),
    };
}

pub fn plan_for(conn: &Connection, req: &PlanRequest, host: &HostFacts) -> AppResult<InstallPlan> {
    validate_request(req)?;
    // A syntactically valid alias is not a project: it must name a registered one.
    if let Some(alias) = &req.project_alias {
        if !crate::projects::is_registered(conn, alias) {
            return Err(AppError::new("PLAN_REQUEST_INVALID", "Plan isteği geçersiz: böyle bir proje kaydı yok."));
        }
    }
    let entry = catalog::get_entry(conn, &req.entry_id)?;
    let observations = store::latest_observations(conn)?;
    // Production: the compiled-in registry, which is empty, and the compiled-in authenticated references, which are empty.
    let registry = RecipeRegistry::production();
    let refs = ReferenceSet::production(conn)?;
    let found = registry.find(&req.entry_id, &req.platform_target, &req.agent_surface, &req.scope);
    build_plan_with(&entry, req, host, &observations, &found, &refs)
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
