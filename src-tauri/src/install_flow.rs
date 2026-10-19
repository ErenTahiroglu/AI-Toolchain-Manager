//! Phase 3D-2: the installation workflow the application offers its user:
//! catalog -> tool details -> installation preview -> confirmation -> result -> installed-tool details.
//!
//! Everything the WebView can ask for goes through [`InstallService`]. The WebView sends **identifiers and one acknowledgement** -- an
//! entry id and an action, an offer id with the preview digest it saw, a cancel -- and never a path, a URL, a command line, a script,
//! bytes or a digest to trust. Rust decides everything else:
//!
//! * **What can be installed at all** comes from the compiled-in supply ([`Sources`]): reviewed recipes, authenticated reference
//!   digests and the artifact provider. In a release build that supply is EMPTY (empty recipe list, empty authenticated-reference set, no
//!   artifact provider), so every tool is "verification incomplete" and nothing can be previewed for installation.
//! * **The preview** is built here, from the recipe, the authenticated references, the catalog, the manager's own state and the fixed
//!   behaviour of the adapter: exact source, version, platform, digest, every filesystem effect, the notices, the rollback contract.
//!   Its digest is what the user's confirmation is bound to ([`crate::consent`]).
//! * **The confirmation** is single-use, short-lived and plan-bound. Before anything is applied the preview is rebuilt from scratch and
//!   must still hash to the digest that was confirmed (freshness), the artifact is checked against the recipe and the authenticated
//!   reference, a Phase 3A approval is derived from the grant, and the adapter takes the tool's lock and consumes the grant.
//! * **Nothing runs.** No process is started, no network is used, no path comes from the interface. The acting adapter exists only in
//!   test builds and in a build made with the `synthetic-pilot` feature, and even there it accepts synthetic recipes only.
//!
//! The in-app confirmation is not authentication; see `docs/phase-3d2/MVP-CONSENT-MODEL.md`.

use crate::{
    adapter::{Adapter, Inspection},
    approval::{self, Action, Actor, ApprovalPolicy, Situation, UserAction},
    audit::{self, canonical_json, sha256_hex, EventContext, NewEvent},
    catalog::{self, CatalogEntry},
    consent::{Binding, ConsentBook, ConsentGrant, GRANT_TTL_SECONDS},
    discovery::{
        reference::{ReferenceDigest, ReferenceSet},
        store,
    },
    error::{AppError, AppResult},
    lifecycle::{check_artifact_for, Authority, Receipt, RollbackReport, TxnError},
    managed_root::{AppDataAnchor, ManagedRoot, RootError},
    plan::{self, HostFacts, InstallPlan, PlanRequest},
    recipe::{self, RecipeRegistry, ValidatedRecipe},
    settings,
};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};

pub const SURFACE: &str = "local-cli";
pub const SCOPE: &str = "user";

// ---------------------------------------------------------------------------------------------------------------------
// The supply: what could ever be installed (empty in a release build)
// ---------------------------------------------------------------------------------------------------------------------

/// Where the bytes of an artifact come from. There is no implementation that reads a file the interface names, downloads anything or
/// starts a program: the only implementations are "nothing" (release) and synthetic in-memory bytes (tests / `synthetic-pilot`).
pub trait ArtifactProvider: Send + Sync {
    fn bytes(&self, tool_id: &str, version: &str, platform: &str) -> Result<Vec<u8>, &'static str>;
}

#[cfg_attr(feature = "synthetic-pilot", allow(dead_code))]
struct NoArtifacts;

impl ArtifactProvider for NoArtifacts {
    fn bytes(&self, _: &str, _: &str, _: &str) -> Result<Vec<u8>, &'static str> {
        Err("artifact_not_available")
    }
}

/// Third-party notices that belong to one recipe. A notice bundle is presented by the application and bound into the confirmed preview
/// by its digest; it is never written into a tool's version folder (the adapter owns exactly one file there).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeBundle {
    pub tool_id: String,
    pub version: String,
    pub title: String,
    /// One line per component whose notice is included, e.g. "Oniguruma - BSD-2-Clause".
    pub components: Vec<String>,
    /// Legal facts that are still unresolved. Shown, never hidden.
    pub unresolved: Vec<String>,
    pub text: String,
}

impl NoticeBundle {
    pub fn digest(&self) -> String {
        sha256_hex(&canonical_json(&serde_json::to_value(self).unwrap_or(serde_json::Value::Null)))
    }
}

pub struct Sources {
    pub registry: RecipeRegistry,
    /// Authenticated references that are not compiled into the reference module (synthetic references only; empty in a release build).
    pub extra_authenticated: Vec<ReferenceDigest>,
    pub artifacts: Box<dyn ArtifactProvider>,
    pub notices: Vec<NoticeBundle>,
}

impl Sources {
    /// The release supply: nothing.
    #[cfg_attr(feature = "synthetic-pilot", allow(dead_code))]
    pub fn production() -> Sources {
        Sources {
            registry: RecipeRegistry::production(),
            extra_authenticated: Vec::new(),
            artifacts: Box::new(NoArtifacts),
            notices: Vec::new(),
        }
    }

    fn references(&self, conn: &Connection) -> AppResult<ReferenceSet> {
        let mut refs = ReferenceSet::production(conn)?;
        refs.authenticated.extend(self.extra_authenticated.iter().cloned());
        Ok(refs)
    }

    fn notices_for(&self, tool: &str, version: &str) -> Option<&NoticeBundle> {
        self.notices.iter().find(|n| n.tool_id == tool && n.version == version)
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// What the interface sees
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// No reviewed recipe, no authenticated reference, or another unmet condition: the manager cannot vouch for an installation.
    VerificationIncomplete,
    /// Everything else is in order, but the licence has not been accepted by a review.
    LicenseReviewRequired,
    /// Installable: a validated recipe, an authenticated artifact. The user may open the preview.
    Available,
    /// A preview is open and waits for the user's confirmation.
    ReadyForApproval,
    Installing,
    InstalledVerified,
    /// Something is wrong that a person has to look at (drift, an unfinished transaction, an unreadable state).
    FailedNeedsAttention,
    /// It was installed by this manager and has been removed again.
    Removed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Reason {
    pub code: String,
    pub text: String,
}

fn reason(code: &str) -> Reason {
    Reason { code: code.to_string(), text: reason_text(code) }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolState {
    pub entry_id: String,
    pub status: Status,
    pub reasons: Vec<Reason>,
    /// The exact version a validated recipe installs.
    pub version: Option<String>,
    pub installed_version: Option<String>,
    /// `install` or `uninstall` while a preview waits for confirmation.
    pub pending_action: Option<Action>,
    pub offer_expires_at: Option<String>,
    pub synthetic: bool,
    pub can_install: bool,
    pub can_remove: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Effect {
    /// Always relative to the manager's own folder, never an absolute path.
    pub place: String,
    /// `create` | `replace` | `remove` | `read`
    pub change: String,
    /// Does it still exist after a successful run?
    pub remains: bool,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceView {
    pub repository: String,
    pub immutable_commit: String,
    pub source_attested: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactView {
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub format: String,
    /// Always `exact_file`: the digest is of the very file that is installed, not of an archive.
    pub digest_scope: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeView {
    pub title: String,
    pub sha256: String,
    pub components: Vec<String>,
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackView {
    pub reliability: String,
    pub undoes: Vec<String>,
    pub does_not_undo: Vec<String>,
    pub text: String,
}

/// Everything the user is shown before confirming, and exactly what the confirmation is bound to (through its digest).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewContent {
    pub action: Action,
    pub entry_id: String,
    pub display_name: String,
    pub version: String,
    pub platform_target: String,
    pub architecture: String,
    pub source: SourceView,
    pub artifact: ArtifactView,
    pub authentication: String,
    pub license_state: String,
    pub license_spdx: String,
    pub notices: Option<NoticeView>,
    pub effects: Vec<Effect>,
    /// What the manager will NOT do, stated explicitly.
    pub not_done: Vec<String>,
    pub network_used: bool,
    pub rollback: RollbackView,
    pub synthetic: bool,
    pub installed_version: Option<String>,
    /// Does the manager's own folder exist yet? (The first installation creates it.)
    pub manager_folder_exists: bool,
    pub plan_id: String,
    pub recipe_id: String,
    pub recipe_digest: String,
    pub effects_digest: String,
    pub consent_statement: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPreview {
    pub offer_id: String,
    pub created_at: String,
    pub expires_at: String,
    pub preview_digest: String,
    pub content: PreviewContent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptView {
    pub txn_id: String,
    pub action: Action,
    pub version: Option<String>,
    pub completed_at: String,
    pub artifact_sha256: Option<String>,
    pub kept: Vec<String>,
}

fn receipt_view(r: &Receipt) -> ReceiptView {
    ReceiptView {
        txn_id: r.txn_id.clone(),
        action: r.action,
        version: r.to_version.clone().or_else(|| r.from_version.clone()),
        completed_at: r.completed_at.clone(),
        artifact_sha256: r.artifact_sha256.clone(),
        kept: r.kept.clone(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackOutcome {
    /// `not_needed` | `restored` | `partial` | `failed`
    pub status: String,
    pub undone: Vec<String>,
    pub not_undone: Vec<String>,
    pub not_covered: Vec<String>,
}

fn rollback_outcome(r: &RollbackReport) -> RollbackOutcome {
    RollbackOutcome {
        status: r.status.clone(),
        undone: r.undone.clone(),
        not_undone: r.not_undone.iter().map(|n| format!("{}: {}", n.what, n.why)).collect(),
        not_covered: r.not_covered.clone(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    /// `completed` | `already_applied` | `failed` | `refused`
    pub outcome: String,
    pub action: Option<Action>,
    pub entry_id: Option<String>,
    pub version: Option<String>,
    pub code: Option<String>,
    pub message: String,
    pub receipt: Option<ReceiptView>,
    pub rollback: Option<RollbackOutcome>,
    pub state: Option<ToolState>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub text: String,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeFull {
    #[serde(flatten)]
    pub summary: NoticeView,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledDetails {
    pub entry_id: String,
    pub display_name: String,
    pub state: ToolState,
    pub version: Option<String>,
    pub artifact: Option<ArtifactView>,
    /// Relative to the manager's own folder.
    pub location: Option<String>,
    pub receipt: Option<ReceiptView>,
    pub checks: Vec<Check>,
    pub history: Vec<ReceiptView>,
    pub notices: Option<NoticeFull>,
    pub kept: Vec<String>,
    pub never: Vec<String>,
    pub last_result: Option<InstallResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySummary {
    pub examined: bool,
    pub rolled_back: u32,
    pub rolled_forward: u32,
    pub needs_attention: u32,
    pub notes: Vec<String>,
}

// ---- requests: nothing but identifiers and one acknowledgement ----------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareRequest {
    pub entry_id: String,
    pub action: Action,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmRequest {
    pub offer_id: String,
    pub preview_digest: String,
    pub acknowledged: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelRequest {
    pub offer_id: String,
}

// ---------------------------------------------------------------------------------------------------------------------
// Texts
// ---------------------------------------------------------------------------------------------------------------------

pub fn reason_text(code: &str) -> String {
    match code {
        "no_reviewed_recipe" => "Bu araç için incelenmiş ve onaylanmış bir kurulum tarifi yok.",
        "ambiguous_recipes" => "Bu araç için birden fazla tarif var; hiçbiri kullanılmıyor.",
        "host_not_supported" => "Bu bilgisayarın platformu desteklenen platformlardan biri değil.",
        "artifact_not_authenticated" | "authenticated_references_conflict" => {
            "Dosyanın özeti, bağımsız olarak doğrulanmış bir kaynakla eşleşmiyor."
        }
        "license_not_accepted" => "Lisans, kabul edildi sonucuyla bir incelemeden geçmedi.",
        "not_reviewed" | "changed_after_review" | "reviewer" => "Tarif bağımsız bir incelemeden geçmedi (veya incelemeden sonra değişti).",
        "staging_evidence" => "Bağımsız bir deneme (staging) kanıtı yok.",
        "source_not_immutable" | "source_attestation_unverifiable" => "Kaynak, değişmez bir sürüm numarasına bağımsız olarak bağlanmadı.",
        "rollback_contract" => "Geri alma sözleşmesi eksik veya geçersiz.",
        "entry_quarantined" => "Kayıt karantinada veya engelli.",
        "execution_policy_blocked" => "Bu kayıt için kapsamlı yazılı yetki olmadan hiçbir işlem yapılamaz.",
        "entry_removed" => "Bu kayıt katalogdan kaldırılmış.",
        "unfinished_transaction" => "Yarım kalmış bir işlem var. Önce \"Kurtarmayı çalıştır\" ile durumu denetleyin.",
        "installed_artifact_drift"
        | "installed_artifact_missing"
        | "installed_state_pointer_mismatch"
        | "installed_state_receipt_mismatch"
        | "installed_state_has_no_valid_receipt"
        | "installed_identity_unprovable"
        | "installed_folder_has_unexpected_entries"
        | "installed_state_missing"
        | "installed_version_differs_from_recipe"
        | "pointer_unreadable" => "Kurulu dosya, kurulum sırasında kaydedilen durumla uyuşmuyor. Hiçbir şey otomatik düzeltilmedi.",
        "root_creation_unfinished"
        | "root_identity_mismatch"
        | "root_not_owner_only"
        | "root_layout_unexpected"
        | "root_not_registered" => "Yöneticinin kendi klasörü beklenen durumda değil. Hiçbir şey otomatik düzeltilmedi.",
        c if c.starts_with("fs_") => "Dosya sistemi işlemi güvenli biçimde tamamlanamadı.",
        _ => "Bu koşul karşılanmadı.",
    }
    .to_string()
}

fn refusal_text(code: &str) -> String {
    match code {
        "consent_bad_identifier" | "consent_unknown" => "Bu onay isteği tanınmadı. Önizlemeyi yeniden açın.",
        "consent_already_used" => "Bu onay zaten kullanıldı. Aynı onay ikinci kez kabul edilmez; önizlemeyi yeniden açın.",
        "consent_cancelled" => "Bu önizleme iptal edilmişti. Önizlemeyi yeniden açın.",
        "consent_expired" => "Önizlemenin süresi doldu. Önizlemeyi yeniden açın.",
        "consent_superseded" => "Bu araç için daha yeni bir önizleme açıldı; eskisi geçersiz.",
        "consent_preview_changed" => "Onaylanan önizleme, uygulamanın elindeki önizlemeyle aynı değil. Hiçbir şey yapılmadı.",
        "consent_not_acknowledged" => "Açık onay verilmedi. Hiçbir şey yapılmadı.",
        "preview_stale" => {
            "Önizlemeden sonra bir şey değişti (tarif, kayıt, kurulu durum veya dosyalar). Hiçbir şey yapılmadı; önizlemeyi yeniden açın."
        }
        "no_reviewed_recipe" | "not_available" => "Bu araç için kurulum yapılamaz: doğrulama tamamlanmadı.",
        "already_installed" => "Bu araç zaten kurulu.",
        "not_installed" => "Bu araç kurulu değil.",
        "needs_attention" => "Bu araç dikkat gerektiriyor; önce durumu denetleyin.",
        "busy" | "Busy" => "Bu araç için başka bir işlem sürüyor. Hiçbir şey yapılmadı.",
        "artifact_not_available" => "Bu sürümün dosyası bu bilgisayarda hazır değil. Hiçbir şey yapılmadı.",
        "artifact_does_not_match_recipe" | "artifact_format_mismatch" | "artifact_not_authenticated_for_this_platform" => {
            "Dosya, tarifteki ve doğrulanmış kaynaktaki özetle eşleşmiyor. Hiçbir şey yapılmadı."
        }
        "production_adapter_not_enabled" => "Bu sürümde gerçek kurulum kapalıdır. Hiçbir şey yapılmadı.",
        "consent_binding_mismatch" | "consent_actor_mismatch" => "Onay, bu işlemin ayrıntılarına bağlı değil. Hiçbir şey yapılmadı.",
        "approval_expired" => "Onayın süresi dolmuş. Hiçbir şey yapılmadı veya yapılan geri alındı.",
        "injected_failure" => "İşlem sırasında bir hata oluştu ve geri alındı.",
        "fs_access_denied" => "Yöneticinin klasörüne yazma izni verilmedi. İşlem geri alındı.",
        "fs_io" => "Disk yazma hatası (örneğin yer kalmadı). İşlem geri alındı.",
        _ => "İşlem reddedildi veya geri alındı.",
    }
    .to_string()
}

fn upper(code: &str) -> String {
    code.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).take(64).collect()
}

// ---------------------------------------------------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------------------------------------------------

type Clock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;
#[cfg(test)]
pub(crate) type FaultHook = Arc<dyn Fn(crate::lifecycle::Point) -> Option<crate::lifecycle::Fault> + Send + Sync>;

pub struct InstallService {
    anchor: AppDataAnchor,
    sources: Sources,
    book: ConsentBook,
    clock: Clock,
    /// Tools this process is working on right now (so "installing" is told from "left unfinished by a crash").
    busy: Mutex<HashSet<String>>,
    last: Mutex<HashMap<String, InstallResult>>,
    #[cfg(test)]
    pub(crate) fault: Option<FaultHook>,
}

struct BusyGuard<'a> {
    set: &'a Mutex<HashSet<String>>,
    tool: String,
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut s) = self.set.lock() {
            s.remove(&self.tool);
        }
    }
}

/// What was learned about one tool's recipe, plan and the catalog entry.
struct Resolved {
    entry: CatalogEntry,
    valid: ValidatedRecipe,
    refs: ReferenceSet,
    plan: InstallPlan,
    platform: String,
}

enum Unavailable {
    Verification(Vec<Reason>),
    License(Vec<Reason>),
}

/// What the manager's own folder says about one tool.
struct Probe {
    root_exists: bool,
    inspection: Option<Inspection>,
    attention: Option<String>,
}

impl InstallService {
    /// The release service: the empty supply.
    #[cfg_attr(feature = "synthetic-pilot", allow(dead_code))]
    pub fn new(data_dir: PathBuf) -> InstallService {
        InstallService::with_sources(data_dir, Sources::production(), Arc::new(Utc::now))
    }

    /// The service the application starts with: the empty release supply, or -- only in a build made with the `synthetic-pilot` feature --
    /// the synthetic one. Nothing in the interface chooses.
    pub fn for_app(data_dir: PathBuf) -> InstallService {
        #[cfg(feature = "synthetic-pilot")]
        {
            InstallService::with_sources(data_dir, supply::sources(), Arc::new(Utc::now))
        }
        #[cfg(not(feature = "synthetic-pilot"))]
        {
            InstallService::new(data_dir)
        }
    }

    /// Once at start-up, before anything else may use the service: puts the synthetic tools into the catalog of the (isolated) data
    /// directory when the pilot feature is on, and lets the manager finish or undo the unfinished work of its own earlier run.
    pub fn startup(&self, conn: &Connection) {
        #[cfg(feature = "synthetic-pilot")]
        {
            let _ = supply::seed_catalog(conn);
        }
        let _ = self.recover(conn);
    }

    pub(crate) fn with_sources(data_dir: PathBuf, sources: Sources, clock: Clock) -> InstallService {
        InstallService {
            anchor: AppDataAnchor::from_resolved_data_dir(data_dir),
            sources,
            book: ConsentBook::new(),
            clock,
            busy: Mutex::new(HashSet::new()),
            last: Mutex::new(HashMap::new()),
            #[cfg(test)]
            fault: None,
        }
    }

    #[cfg(test)]
    pub fn sources(&self) -> &Sources {
        &self.sources
    }

    fn now(&self) -> DateTime<Utc> {
        (self.clock)()
    }

    // ---- identity and the manager's folder ----------------------------------------------------------------------------

    /// A random identifier of THIS installation of the manager, made once and kept in its own database. Not a hardware identifier.
    fn computer_id(conn: &Connection) -> AppResult<String> {
        if let Some(id) = settings::get_internal(conn, "_computer_id")? {
            return Ok(id);
        }
        let id = format!("computer-{}", uuid::Uuid::new_v4().simple());
        settings::set_internal(conn, "_computer_id", &id)?;
        Ok(id)
    }

    fn root_row(conn: &Connection, computer: &str) -> AppResult<Option<String>> {
        Ok(conn
            .query_row("SELECT state FROM managed_roots WHERE computer_id = ?1 AND state IN ('creating', 'active')", [computer], |r| {
                r.get(0)
            })
            .optional()?)
    }

    fn open_root(&self, conn: &Connection, computer: &str) -> Result<ManagedRoot, RootError> {
        ManagedRoot::open(&self.anchor, conn, computer)
    }

    /// Opens the manager's folder, creating it on the first use.
    fn open_or_establish(&self, conn: &Connection, computer: &str) -> Result<ManagedRoot, RootError> {
        match Self::root_row(conn, computer).map_err(|_| RootError::Db)? {
            None => ManagedRoot::establish(&self.anchor, conn, computer, &self.now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
            Some(_) => self.open_root(conn, computer),
        }
    }

    #[cfg(any(test, feature = "synthetic-pilot"))]
    fn adapter<'a>(
        &'a self,
        root: &'a ManagedRoot,
        refs: &'a ReferenceSet,
        platform: &'a str,
        clock: &'a (dyn Fn() -> DateTime<Utc> + Sync),
    ) -> Result<Adapter<'a>, &'static str> {
        #[cfg(test)]
        let fault: Option<crate::lifecycle::FaultFn<'a>> = self.fault.as_deref().map(|f| f as crate::lifecycle::FaultFn<'a>);
        #[cfg(not(test))]
        let fault: Option<crate::lifecycle::FaultFn<'a>> = None;
        Ok(Adapter::mvp_synthetic(root, refs, platform, clock, fault))
    }

    /// A release build contains no acting adapter at all: there is nothing to construct.
    #[cfg(not(any(test, feature = "synthetic-pilot")))]
    fn adapter<'a>(
        &'a self,
        _: &'a ManagedRoot,
        _: &'a ReferenceSet,
        _: &'a str,
        _: &'a (dyn Fn() -> DateTime<Utc> + Sync),
    ) -> Result<Adapter<'a>, &'static str> {
        Err("production_adapter_not_enabled")
    }

    // ---- resolving what a tool is ------------------------------------------------------------------------------------

    fn resolve(&self, conn: &Connection, entry_id: &str) -> AppResult<Result<Resolved, Unavailable>> {
        let entry = catalog::get_entry(conn, entry_id)?;
        let host = HostFacts::detect();
        if !plan::PLATFORMS.contains(&host.platform.as_str()) {
            return Ok(Err(Unavailable::Verification(vec![reason("host_not_supported")])));
        }
        let refs = self.sources.references(conn)?;
        let candidates = self.sources.registry.find(&entry.entry_id, &host.platform, SURFACE, SCOPE);
        let candidate = match candidates.as_slice() {
            [] => return Ok(Err(Unavailable::Verification(vec![reason("no_reviewed_recipe")]))),
            [one] => *one,
            _ => return Ok(Err(Unavailable::Verification(vec![reason("ambiguous_recipes")]))),
        };
        let valid = match recipe::validate(candidate, &refs) {
            Ok(v) => v,
            Err(problems) => {
                let reasons: Vec<Reason> = problems.iter().map(|p| reason(p)).collect();
                return Ok(Err(if problems.iter().all(|p| *p == "license_not_accepted") {
                    Unavailable::License(reasons)
                } else {
                    Unavailable::Verification(reasons)
                }));
            }
        };
        let request = PlanRequest {
            entry_id: entry.entry_id.clone(),
            platform_target: host.platform.clone(),
            agent_surface: SURFACE.to_string(),
            scope: SCOPE.to_string(),
            method_id: None,
            project_alias: None,
        };
        let observations = store::latest_observations_with(conn, &refs)?;
        let plan = plan::build_plan_with(&entry, &request, &host, &observations, &[candidate], &refs)?;
        let others: Vec<Reason> = plan
            .eligibility
            .blockers
            .iter()
            .filter(|b| b.code != "user_approval_missing")
            .map(|b| Reason { code: b.code.clone(), text: if b.text.is_empty() { reason_text(&b.code) } else { b.text.clone() } })
            .collect();
        if plan.recipe.state != "validated" || !others.is_empty() {
            let mut reasons = others;
            if reasons.is_empty() {
                reasons = plan.recipe.problems.iter().map(|p| reason(p)).collect();
            }
            return Ok(Err(Unavailable::Verification(reasons)));
        }
        Ok(Ok(Resolved { entry, valid, refs, plan, platform: host.platform }))
    }

    fn probe(&self, conn: &Connection, r: &Resolved) -> AppResult<Probe> {
        let computer = Self::computer_id(conn)?;
        match Self::root_row(conn, &computer)? {
            None => {
                return Ok(Probe { root_exists: false, inspection: Some(Inspection::NotInstalled { removed: false }), attention: None })
            }
            Some(state) if state != "active" => {
                return Ok(Probe { root_exists: true, inspection: None, attention: Some("root_creation_unfinished".to_string()) })
            }
            Some(_) => {}
        }
        let root = match self.open_root(conn, &computer) {
            Ok(root) => root,
            Err(e) => return Ok(Probe { root_exists: true, inspection: None, attention: Some(e.code().to_string()) }),
        };
        let rec = r.valid.recipe();
        let clock = &*self.clock;
        let adapter = match self.adapter(&root, &r.refs, &r.platform, clock) {
            Ok(a) => a,
            Err(code) => return Ok(Probe { root_exists: true, inspection: None, attention: Some(code.to_string()) }),
        };
        match adapter.inspect(&rec.tool_id, &rec.version, rec.artifact.size_bytes) {
            Ok(i) => Ok(Probe { root_exists: true, inspection: Some(i), attention: None }),
            Err(TxnError::Refused(code)) => Ok(Probe { root_exists: true, inspection: None, attention: Some(code.to_string()) }),
            Err(_) => Ok(Probe { root_exists: true, inspection: None, attention: Some("unfinished_transaction".to_string()) }),
        }
    }

    fn is_busy_here(&self, tool: &str) -> bool {
        self.busy.lock().map(|s| s.contains(tool)).unwrap_or(false)
    }

    fn state_of(&self, conn: &Connection, entry_id: &str) -> AppResult<ToolState> {
        let blank = |status: Status, reasons: Vec<Reason>| ToolState {
            entry_id: entry_id.to_string(),
            status,
            reasons,
            version: None,
            installed_version: None,
            pending_action: None,
            offer_expires_at: None,
            synthetic: false,
            can_install: false,
            can_remove: false,
        };
        let r = match self.resolve(conn, entry_id)? {
            Err(Unavailable::Verification(reasons)) => return Ok(blank(Status::VerificationIncomplete, reasons)),
            Err(Unavailable::License(reasons)) => return Ok(blank(Status::LicenseReviewRequired, reasons)),
            Ok(r) => r,
        };
        let rec = r.valid.recipe();
        let mut st = blank(Status::Available, vec![]);
        st.version = Some(rec.version.clone());
        st.synthetic = rec.synthetic;
        let probe = self.probe(conn, &r)?;
        if let Some(code) = &probe.attention {
            st.status = Status::FailedNeedsAttention;
            st.reasons = vec![reason(code)];
            return Ok(st);
        }
        match probe.inspection {
            Some(Inspection::Busy { .. }) => {
                if self.is_busy_here(&rec.tool_id) {
                    st.status = Status::Installing;
                } else {
                    st.status = Status::FailedNeedsAttention;
                    st.reasons = vec![reason("unfinished_transaction")];
                }
            }
            Some(Inspection::Attention { code }) => {
                st.status = Status::FailedNeedsAttention;
                st.reasons = vec![reason(code)];
            }
            Some(Inspection::Installed { version, .. }) => {
                st.status = Status::InstalledVerified;
                st.installed_version = Some(version);
                st.can_remove = true;
            }
            Some(Inspection::NotInstalled { removed }) => {
                st.status = if removed { Status::Removed } else { Status::Available };
                st.can_install = true;
            }
            None => {
                st.status = Status::FailedNeedsAttention;
                st.reasons = vec![reason("unfinished_transaction")];
            }
        }
        if matches!(st.status, Status::Available | Status::Removed | Status::InstalledVerified) {
            if let Some((action, until)) = self.book.live_for(&rec.tool_id, self.now()) {
                st.pending_action = Some(action);
                st.offer_expires_at = Some(until.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
                st.status = Status::ReadyForApproval;
            }
        }
        Ok(st)
    }

    /// The state of every live catalog entry.
    pub fn list_states(&self, conn: &Connection) -> AppResult<Vec<ToolState>> {
        catalog::list_catalog(conn)?.into_iter().filter(|e| e.removed_at.is_none()).map(|e| self.state_of(conn, &e.entry_id)).collect()
    }

    #[cfg(test)]
    pub fn state(&self, conn: &Connection, entry_id: &str) -> AppResult<ToolState> {
        self.state_of(conn, entry_id)
    }

    // ---- the preview ---------------------------------------------------------------------------------------------------

    fn effects(action: Action, tool: &str, version: &str, root_exists: bool) -> Vec<Effect> {
        let e = |place: String, change: &str, remains: bool, text: &str| Effect {
            place,
            change: change.to_string(),
            remains,
            text: text.to_string(),
        };
        let mut out = Vec::new();
        if !root_exists {
            out.push(e(
                "(yöneticinin kendi klasörü, uygulama veri klasörünün içinde)".to_string(),
                "create",
                true,
                "İlk kullanımda yöneticinin kendi klasörü ve sabit alt klasörleri oluşturulur; yalnızca bu hesap erişebilir.",
            ));
        }
        out.push(e(format!("locks/{tool}.lock"), "create", false, "İşlem süresince başka bir işlemi engelleyen kilit; bitince silinir."));
        out.push(e(
            "journal/<işlem-kimliği>.json".to_string(),
            "create",
            true,
            "İşlem günlüğü: her adım yapılmadan önce yazılır; çökme sonrası kurtarma buna dayanır.",
        ));
        out.push(e(
            "journal/<işlem-kimliği>.*.tmp".to_string(),
            "create",
            false,
            "Günlüğün her güncellemesi önce geçici bir dosyaya yazılır ve tek adımda yerine konur.",
        ));
        out.push(e(
            "snapshots/<işlem-kimliği>/".to_string(),
            "create",
            false,
            "Geri alma için geçici anlık görüntü klasörü; işlem bitince silinir.",
        ));
        match action {
            Action::Install | Action::Update => {
                out.push(e("staging/<işlem-kimliği>/".to_string(), "create", false, "Dosyanın özel hazırlık kopyası (önce .part adıyla yazılır); doğrulanır ve sürüm klasörüne taşınır; klasör sonunda silinir."));
                out.push(e(
                    format!("versions/{tool}/{version}/artifact.bin"),
                    "create",
                    true,
                    "Kurulan dosyanın kendisi: bu klasörde başka hiçbir dosya olmaz.",
                ));
                out.push(e(
                    format!("state/{tool}.json.new"),
                    "create",
                    false,
                    "Etkin sürüm işaretinin geçici hazırlık dosyası; tek adımda asıl adına taşınır.",
                ));
                out.push(e(
                    format!("state/{tool}.json"),
                    "create",
                    true,
                    "Hangi sürümün etkin olduğunu gösteren küçük işaret dosyası (tek adımda değiştirilir).",
                ));
                out.push(e(
                    "receipts/<işlem-kimliği>.json".to_string(),
                    "create",
                    true,
                    "Kurulum makbuzu: ne zaman, hangi sürüm, hangi özet.",
                ));
            }
            Action::Uninstall => {
                out.push(e(format!("state/{tool}.json"), "remove", false, "Etkin sürüm işareti kaldırılır."));
                out.push(e(
                    format!("versions/{tool}/{version}/"),
                    "remove",
                    false,
                    "Yöneticinin kurduğu sürüm klasörü ve içindeki dosya kaldırılır.",
                ));
                out.push(e("receipts/<işlem-kimliği>.json".to_string(), "create", true, "Kaldırma makbuzu."));
            }
        }
        out.push(e(format!("data/{tool}/"), "read", true, "Aracın kendi kullanıcı verisi: bu işlemde okunmaz, değiştirilmez, silinmez."));
        out
    }

    fn not_done() -> Vec<String> {
        [
            "Hiçbir program çalıştırılmaz; kurulan dosya da çalıştırılmaz.",
            "Ağa bağlanılmaz, hiçbir şey indirilmez.",
            "PATH (program arama yolu) değiştirilmez.",
            "Hiçbir ajanın ayar dosyası değiştirilmez.",
            "Yönetici yetkisi istenmez; tüm bilgisayara kurulum yapılmaz.",
            "Yöneticinin kendi klasörü dışında hiçbir dosya oluşturulmaz, değiştirilmez veya silinmez.",
            "Aracın kendi kullanıcı verisine dokunulmaz.",
            "Windows güvenlik ayarları (Akıllı Uygulama Denetimi, Defender) değiştirilmez.",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn consent_statement(action: Action) -> String {
        let what = match action {
            Action::Install => "kurulumu",
            Action::Uninstall => "kaldırmayı",
            Action::Update => "güncellemeyi",
        };
        format!(
            "\"Onaylıyorum\" düğmesi, yukarıda gösterilen tam {what} bu uygulamaya yaptırır. Bu uygulama içi bir onaydır: \
             parmak izi, PIN veya donanım anahtarı gibi bir kimlik kanıtı DEĞİLDİR ve aynı Windows hesabında çalışan başka bir \
             programın bu pencereyi kullanmasına karşı koruma sağlamaz. Onay tek kullanımlıktır ve kısa süre sonra geçersiz olur."
        )
    }

    fn assemble(&self, conn: &Connection, r: &Resolved, action: Action, probe: &Probe) -> AppResult<PreviewContent> {
        let rec = r.valid.recipe();
        let (arch, _) = match r.platform.as_str() {
            "windows-x64" => ("x64", "Windows"),
            _ => ("arm64", "macOS"),
        };
        let notice = self.sources.notices_for(&rec.tool_id, &rec.version);
        let installed_version = match &probe.inspection {
            Some(Inspection::Installed { version, .. }) => Some(version.clone()),
            _ => None,
        };
        let effects = Self::effects(action, &rec.tool_id, &rec.version, probe.root_exists);
        let designed = rec.rollback.state == "designed";
        let rollback = RollbackView {
            reliability: rec.rollback.state.clone(),
            undoes: rec.rollback.undoes.clone(),
            does_not_undo: rec.rollback.does_not_undo.clone(),
            text: format!(
                "Geri alma sözleşmesi {}. Bir adım başarısız olursa yalnızca bu işlemin oluşturduğu dosyalar, kayıtla eşleşiyorlarsa geri alınır; \
                 eşleşmeyen veya yabancı dosyalara dokunulmaz ve bu açıkça bildirilir.",
                if designed { "yalnızca TASARLANMIŞ" } else { "yalnızca SENTETİK denemelerle sınandı" }
            ),
        };
        let mut content = PreviewContent {
            action,
            entry_id: r.entry.entry_id.clone(),
            display_name: r.entry.display_name.clone(),
            version: rec.version.clone(),
            platform_target: r.platform.clone(),
            architecture: arch.to_string(),
            source: SourceView {
                repository: rec.source.repository.clone(),
                immutable_commit: rec.source.immutable_commit.clone(),
                source_attested: rec.source.source_attested,
            },
            artifact: ArtifactView {
                file_name: rec.artifact.file_name.clone(),
                size_bytes: rec.artifact.size_bytes,
                sha256: rec.artifact.sha256.clone(),
                format: rec.artifact.format.clone(),
                digest_scope: "exact_file".to_string(),
            },
            authentication: "authenticated_reference".to_string(),
            license_state: rec.license.state.clone(),
            license_spdx: rec.license.spdx_id.clone(),
            notices: notice.map(|n| NoticeView {
                title: n.title.clone(),
                sha256: n.digest(),
                components: n.components.clone(),
                unresolved: n.unresolved.clone(),
            }),
            effects,
            not_done: Self::not_done(),
            network_used: !rec.network.is_empty(),
            rollback,
            synthetic: rec.synthetic,
            installed_version,
            manager_folder_exists: probe.root_exists,
            plan_id: r.plan.plan_id.clone(),
            recipe_id: rec.recipe_id.clone(),
            recipe_digest: r.valid.digest().to_string(),
            effects_digest: r.valid.permissions_digest(),
            consent_statement: Self::consent_statement(action),
        };
        // the connection is only needed for the computer identity, which is part of what is bound
        let _ = conn;
        content.consent_statement = Self::consent_statement(action);
        Ok(content)
    }

    fn preview_digest(content: &PreviewContent) -> String {
        sha256_hex(&canonical_json(&serde_json::to_value(content).unwrap_or(serde_json::Value::Null)))
    }

    fn refuse(code: &str, action: Option<Action>, entry: Option<&str>) -> InstallResult {
        InstallResult {
            outcome: "refused".to_string(),
            action,
            entry_id: entry.map(str::to_string),
            version: None,
            code: Some(code.to_string()),
            message: refusal_text(code),
            receipt: None,
            rollback: None,
            state: None,
        }
    }

    fn audit(conn: &Connection, event: NewEvent) {
        let _ = audit::append(conn, &event);
    }

    /// Opens an installation (or removal) preview and the offer its confirmation is bound to. It changes nothing.
    pub fn prepare(&self, conn: &Connection, req: &PrepareRequest) -> AppResult<InstallPreview> {
        if !matches!(req.action, Action::Install | Action::Uninstall) {
            return Err(AppError::new("INSTALL_ACTION_UNSUPPORTED", "Bu sürümde yalnızca kurulum ve kaldırma desteklenir."));
        }
        let r = match self.resolve(conn, &req.entry_id)? {
            Ok(r) => r,
            Err(Unavailable::Verification(reasons) | Unavailable::License(reasons)) => {
                let first = reasons.first().map(|r| r.code.clone()).unwrap_or_else(|| "not_available".to_string());
                return Err(AppError::new("INSTALL_NOT_AVAILABLE", format!("{} ({first})", refusal_text("not_available"))));
            }
        };
        let rec = r.valid.recipe();
        let probe = self.probe(conn, &r)?;
        if let Some(code) = &probe.attention {
            return Err(AppError::new("INSTALL_NEEDS_ATTENTION", format!("{} ({code})", refusal_text("needs_attention"))));
        }
        match (&probe.inspection, req.action) {
            (Some(Inspection::NotInstalled { .. }), Action::Install) => {}
            (Some(Inspection::Installed { .. }), Action::Uninstall) => {}
            (Some(Inspection::Installed { .. }), Action::Install) => {
                return Err(AppError::new("INSTALL_ALREADY_INSTALLED", refusal_text("already_installed")))
            }
            (Some(Inspection::NotInstalled { .. }), Action::Uninstall) => {
                return Err(AppError::new("INSTALL_NOT_INSTALLED", refusal_text("not_installed")))
            }
            _ => return Err(AppError::new("INSTALL_NEEDS_ATTENTION", refusal_text("needs_attention"))),
        }
        if req.action == Action::Install {
            // the file must be the exact file the recipe names AND the authenticated reference vouches for, before anything is offered
            let bytes = self
                .sources
                .artifacts
                .bytes(&rec.tool_id, &rec.version, &r.platform)
                .map_err(|c| AppError::new("INSTALL_ARTIFACT_UNAVAILABLE", refusal_text(c)))?;
            check_artifact_for(&r.refs, &r.platform, &r.valid, &bytes)
                .map_err(|e| AppError::new("INSTALL_ARTIFACT_REJECTED", refusal_text(txn_code(&e))))?;
        }
        let content = self.assemble(conn, &r, req.action, &probe)?;
        let digest = Self::preview_digest(&content);
        let computer = Self::computer_id(conn)?;
        let ticket = self.book.open(
            Binding {
                action: req.action,
                tool_id: rec.tool_id.clone(),
                version: rec.version.clone(),
                platform_target: r.platform.clone(),
                computer_id: computer,
                recipe_digest: r.valid.digest().to_string(),
                artifact_sha256: rec.artifact.sha256.clone(),
                effects_digest: r.valid.permissions_digest(),
                plan_id: r.plan.plan_id.clone(),
                preview_digest: digest.clone(),
                // a removal names the version it removes (the adapter records it as the version being replaced)
                from_version: if req.action == Action::Uninstall { content.installed_version.clone() } else { None },
            },
            self.now(),
        );
        Ok(InstallPreview {
            offer_id: ticket.offer_id,
            created_at: ticket.created_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            expires_at: ticket.expires_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            preview_digest: digest,
            content,
        })
    }

    pub fn cancel(&self, conn: &Connection, req: &CancelRequest) -> AppResult<()> {
        match self.book.cancel(&req.offer_id, self.now()) {
            Ok(b) => {
                Self::audit(
                    conn,
                    NewEvent::plain("reject", "passed", "install-preview-cancelled", None, "human-review", "user")
                        .for_tool(&b.tool_id, EventContext { tool_version: Some(b.version.clone()), ..Default::default() }),
                );
                Ok(())
            }
            Err(e) => Err(AppError::new("INSTALL_OFFER_UNKNOWN", refusal_text(e.code()))),
        }
    }

    // ---- confirm and apply ---------------------------------------------------------------------------------------------

    fn with_conn<T>(db: &Mutex<Option<Connection>>, f: impl FnOnce(&Connection) -> T) -> Option<T> {
        let guard = db.lock().ok()?;
        guard.as_ref().map(f)
    }

    /// The explicit affirmative action. Returns what happened; a refusal is a normal answer (`outcome == "refused"`), never a partial one.
    pub fn confirm(&self, db: &Mutex<Option<Connection>>, req: &ConfirmRequest) -> InstallResult {
        // 1. the confirmation itself: single use, bound to the preview
        let grant = match self.book.confirm(&req.offer_id, &req.preview_digest, req.acknowledged, self.now()) {
            Ok(g) => g,
            Err(e) => {
                let code = e.code();
                Self::with_conn(db, |c| {
                    Self::audit(
                        c,
                        NewEvent::plain("approve", "blocked", "install-confirmation", Some(leak(upper(code))), "human-review", "user"),
                    )
                });
                return Self::refuse(code, None, None);
            }
        };
        let b = grant.binding().clone();
        let tool = b.tool_id.clone();
        // 2. one working request per tool in this process; a second one is refused before it touches anything
        let _busy = {
            let Ok(mut set) = self.busy.lock() else { return Self::refuse("busy", Some(b.action), Some(&tool)) };
            if !set.insert(tool.clone()) {
                return Self::refuse("busy", Some(b.action), Some(&tool));
            }
            BusyGuard { set: &self.busy, tool: tool.clone() }
        };
        // 3. freshness and the Phase 3A approval, with the database (the grant is consumed here whatever the answer)
        let prepared = Self::with_conn(db, |c| {
            self.verify_and_approve(c, grant, &b).inspect_err(|code| {
                Self::audit(
                    c,
                    NewEvent::plain("approve", "blocked", "install-freshness", Some(leak(upper(code))), "not-applicable", "adapter"),
                );
            })
        });
        let p = match prepared {
            Some(Ok(p)) => p,
            Some(Err(code)) => return Self::refuse(code, Some(b.action), Some(&tool)),
            None => return Self::refuse("database_unavailable", Some(b.action), Some(&tool)),
        };
        // 4. apply (no database lock is held while the adapter works)
        let result = self.apply(db, p, &b);
        self.remember(&result);
        result
    }

    fn remember(&self, r: &InstallResult) {
        if let (Some(id), Ok(mut m)) = (&r.entry_id, self.last.lock()) {
            m.insert(id.clone(), r.clone());
        }
    }

    fn verify_and_approve(&self, conn: &Connection, grant: ConsentGrant, b: &Binding) -> Result<Prepared, &'static str> {
        let fail = |code: &'static str| Err(code);
        let r = match self.resolve(conn, &b.tool_id) {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => return fail("preview_stale"),
            Err(_) => return fail("database_unavailable"),
        };
        let computer = match Self::computer_id(conn) {
            Ok(c) => c,
            Err(_) => return fail("database_unavailable"),
        };
        let probe = match self.probe(conn, &r) {
            Ok(p) => p,
            Err(_) => return fail("database_unavailable"),
        };
        if probe.attention.is_some() || matches!(probe.inspection, Some(Inspection::Attention { .. } | Inspection::Busy { .. }) | None) {
            return fail("needs_attention");
        }
        let content = match self.assemble(conn, &r, b.action, &probe) {
            Ok(c) => c,
            Err(_) => return fail("preview_stale"),
        };
        // freshness: what would be previewed NOW must be what was confirmed
        if Self::preview_digest(&content) != b.preview_digest
            || r.plan.plan_id != b.plan_id
            || r.valid.digest() != b.recipe_digest
            || computer != b.computer_id
            || b.from_version != if b.action == Action::Uninstall { content.installed_version.clone() } else { None }
        {
            return fail("preview_stale");
        }
        let rec = r.valid.recipe();
        let bytes = if b.action == Action::Install {
            let bytes = match self.sources.artifacts.bytes(&rec.tool_id, &rec.version, &r.platform) {
                Ok(b) => b,
                Err(c) => return fail(c),
            };
            if let Err(e) = check_artifact_for(&r.refs, &r.platform, &r.valid, &bytes) {
                return fail(txn_code(&e));
            }
            Some(bytes)
        } else {
            None
        };
        let now = self.now();
        let situation = Situation {
            plan: &r.plan,
            recipe: &r.valid,
            action: b.action,
            from_version: None,
            computer_id: &computer,
            host_platform: &r.platform,
            now,
            conn,
        };
        let actor: Actor = grant.actor();
        let confirmed = UserAction {
            confirmed_text: approval::expected_confirmation(b.action, &rec.tool_id, &rec.version, &rec.target.platform_target),
        };
        let policy = ApprovalPolicy::in_app_confirmation();
        let approved = approval::approve(&situation, None, actor, &confirmed, policy, GRANT_TTL_SECONDS);
        let verified = approved.and_then(|a| approval::verify_approval(&a, &situation, policy));
        let verified = match verified {
            Ok(v) => v,
            Err(e) => return Err(e.code()),
        };
        Self::audit(
            conn,
            NewEvent::plain("approve", "passed", "install-confirmation", None, "human-review", "user").for_tool(
                &rec.tool_id,
                EventContext {
                    recipe_id: Some(rec.recipe_id.clone()),
                    tool_version: Some(rec.version.clone()),
                    approval_reference: Some(verified.approval().approval_id.clone()),
                    approved_recipe_digest: Some(r.valid.digest().to_string()),
                    artifact_digest: Some(rec.artifact.sha256.clone()),
                    evidence_digest: None,
                },
            ),
        );
        Ok(Prepared { resolved: r, verified, bytes, grant, computer })
    }

    fn apply(&self, db: &Mutex<Option<Connection>>, p: Prepared, b: &Binding) -> InstallResult {
        let Prepared { resolved: r, verified, bytes, grant, computer } = p;
        let rec = r.valid.recipe().clone();
        let action = b.action;
        // the manager's folder (created on the first installation)
        let opened = Self::with_conn(db, |conn| {
            if action == Action::Install {
                self.open_or_establish(conn, &computer)
            } else {
                self.open_root(conn, &computer)
            }
        });
        let root = match opened {
            Some(Ok(root)) => root,
            Some(Err(e)) => return Self::refuse(e.code(), Some(action), Some(&rec.tool_id)),
            None => return Self::refuse("database_unavailable", Some(action), Some(&rec.tool_id)),
        };
        let clock = &*self.clock;
        let adapter = match self.adapter(&root, &r.refs, &r.platform, clock) {
            Ok(a) => a,
            Err(code) => return Self::refuse(code, Some(action), Some(&rec.tool_id)),
        };
        let authority = Authority { recipe: &r.valid, approval: &verified };
        let outcome = match action {
            Action::Install => adapter.install_with_consent(&authority, grant, bytes.as_deref().unwrap_or(&[])),
            Action::Uninstall => adapter.uninstall_with_consent(&authority, grant),
            Action::Update => return Self::refuse("not_available", Some(action), Some(&rec.tool_id)),
        };
        let notices = self.sources.notices_for(&rec.tool_id, &rec.version).map(|n| n.digest());
        let ctx = |approval: &str| EventContext {
            recipe_id: Some(rec.recipe_id.clone()),
            tool_version: Some(rec.version.clone()),
            approval_reference: Some(approval.to_string()),
            approved_recipe_digest: None,
            artifact_digest: Some(rec.artifact.sha256.clone()),
            evidence_digest: notices.clone(),
        };
        let event_type = if action == Action::Install { "install" } else { "uninstall" };
        let approval_id = verified.approval().approval_id.clone();
        let mut result = match outcome {
            Ok(o) => InstallResult {
                outcome: if o.already_applied { "already_applied" } else { "completed" }.to_string(),
                action: Some(action),
                entry_id: Some(rec.tool_id.clone()),
                version: Some(rec.version.clone()),
                code: None,
                message: match (action, o.already_applied) {
                    (_, true) => "İstenen durum zaten geçerliydi; hiçbir şey değiştirilmedi.".to_string(),
                    (Action::Install, _) => "Kurulum tamamlandı ve doğrulandı. Hiçbir program çalıştırılmadı.".to_string(),
                    _ => "Kaldırma tamamlandı. Aracın kendi kullanıcı verisine dokunulmadı.".to_string(),
                },
                receipt: Some(receipt_view(&o.receipt)),
                rollback: None,
                state: None,
            },
            Err(TxnError::Failed { code, rollback }) => InstallResult {
                outcome: "failed".to_string(),
                action: Some(action),
                entry_id: Some(rec.tool_id.clone()),
                version: Some(rec.version.clone()),
                code: Some(code.to_string()),
                message: format!(
                    "{} Geri alma durumu: {}.",
                    refusal_text(code),
                    match rollback.status.as_str() {
                        "restored" | "not_needed" => "önceki durum eksiksiz geri getirildi",
                        "partial" => "KISMEN geri alındı; bir kişinin bakması gerekiyor",
                        _ => "geri alma TAMAMLANAMADI; bir kişinin bakması gerekiyor",
                    }
                ),
                receipt: None,
                rollback: Some(rollback_outcome(&rollback)),
                state: None,
            },
            Err(TxnError::Refused(code)) => {
                InstallResult { code: Some(code.to_string()), ..Self::refuse(code, Some(action), Some(&rec.tool_id)) }
            }
            Err(TxnError::Busy) => Self::refuse("busy", Some(action), Some(&rec.tool_id)),
            Err(TxnError::Pending(_)) => Self::refuse("needs_attention", Some(action), Some(&rec.tool_id)),
            Err(TxnError::Interrupted(_)) => Self::refuse("needs_attention", Some(action), Some(&rec.tool_id)),
        };
        drop(root);
        let state = Self::with_conn(db, |conn| {
            let (outcome_tag, code) = match result.outcome.as_str() {
                "completed" | "already_applied" => ("passed", None),
                "failed" => ("failed", result.code.as_deref().map(|c| leak(upper(c)))),
                _ => ("blocked", result.code.as_deref().map(|c| leak(upper(c)))),
            };
            Self::audit(
                conn,
                NewEvent::plain(event_type, outcome_tag, "install-apply", code, "not-applicable", "adapter")
                    .for_tool(&rec.tool_id, ctx(&approval_id)),
            );
            self.state_of(conn, &rec.tool_id).ok()
        });
        result.state = state.flatten();
        result
    }

    // ---- details and recovery ------------------------------------------------------------------------------------------

    pub fn details(&self, conn: &Connection, entry_id: &str) -> AppResult<InstalledDetails> {
        let entry = catalog::get_entry(conn, entry_id)?;
        let state = self.state_of(conn, entry_id)?;
        let last = self.last.lock().ok().and_then(|m| m.get(entry_id).cloned());
        let mut d = InstalledDetails {
            entry_id: entry.entry_id.clone(),
            display_name: entry.display_name.clone(),
            state: state.clone(),
            version: None,
            artifact: None,
            location: None,
            receipt: None,
            checks: Vec::new(),
            history: Vec::new(),
            notices: None,
            kept: Vec::new(),
            never: Self::not_done(),
            last_result: last,
        };
        let Ok(Ok(r)) = self.resolve(conn, entry_id) else { return Ok(d) };
        let rec = r.valid.recipe();
        d.notices = self.sources.notices_for(&rec.tool_id, &rec.version).map(|n| NoticeFull {
            summary: NoticeView {
                title: n.title.clone(),
                sha256: n.digest(),
                components: n.components.clone(),
                unresolved: n.unresolved.clone(),
            },
            text: n.text.clone(),
        });
        let probe = self.probe(conn, &r)?;
        if !probe.root_exists {
            return Ok(d);
        }
        let computer = Self::computer_id(conn)?;
        let Ok(root) = self.open_root(conn, &computer) else { return Ok(d) };
        let clock = &*self.clock;
        let Ok(adapter) = self.adapter(&root, &r.refs, &r.platform, clock) else { return Ok(d) };
        if let Ok(h) = adapter.history(&rec.tool_id) {
            d.history = h.iter().map(receipt_view).collect();
        }
        if let Some(Inspection::Installed { version, artifact_sha256, receipt }) = &probe.inspection {
            d.version = Some(version.clone());
            d.location = Some(format!("versions/{}/{}/artifact.bin", rec.tool_id, version));
            d.artifact = Some(ArtifactView {
                file_name: rec.artifact.file_name.clone(),
                size_bytes: rec.artifact.size_bytes,
                sha256: artifact_sha256.clone(),
                format: rec.artifact.format.clone(),
                digest_scope: "exact_file".to_string(),
            });
            d.receipt = Some(receipt_view(receipt));
            d.kept = receipt.kept.clone();
            d.checks = [
                "Etkin sürüm işareti bu araç, sürüm ve dosya özetiyle eşleşiyor.",
                "Kurulum makbuzu geçerli ve bu bilgisayara, bu platforma bağlı.",
                "Kurulu dosya düz, tek bağlantılı bir dosya ve özeti kayıtlı özetle aynı.",
                "Sürüm klasöründe başka hiçbir dosya yok.",
                "Dosya, kurulum günlüğünde kaydedilen dosyanın ta kendisi.",
            ]
            .iter()
            .map(|t| Check { text: t.to_string(), passed: true })
            .collect();
        } else if let Some(Inspection::Attention { code }) = &probe.inspection {
            d.checks = vec![Check { text: format!("Doğrulama başarısız: {}", reason_text(code)), passed: false }];
        }
        Ok(d)
    }

    /// Explicit recovery of the manager's own unfinished transactions (also run once when the application starts). It only undoes or
    /// finishes work this manager started itself, and reports anything it cannot prove to be its own instead of touching it.
    pub fn recover(&self, conn: &Connection) -> AppResult<RecoverySummary> {
        let computer = Self::computer_id(conn)?;
        let mut s = RecoverySummary { examined: false, rolled_back: 0, rolled_forward: 0, needs_attention: 0, notes: vec![] };
        if Self::root_row(conn, &computer)?.is_none() {
            return Ok(s);
        }
        let root = match self.open_root(conn, &computer) {
            Ok(r) => r,
            Err(e) => {
                s.notes.push(reason_text(e.code()));
                s.needs_attention += 1;
                return Ok(s);
            }
        };
        let refs = self.sources.references(conn)?;
        let platform = HostFacts::detect().platform;
        let clock = &*self.clock;
        let adapter = match self.adapter(&root, &refs, &platform, clock) {
            Ok(a) => a,
            Err(_) => return Ok(s),
        };
        s.examined = true;
        match adapter.recover() {
            Ok(list) => {
                use crate::lifecycle::Recovered;
                for item in list {
                    match item {
                        Recovered::RolledBack(_) => s.rolled_back += 1,
                        Recovered::RolledForward(_) => s.rolled_forward += 1,
                        Recovered::NeedsAttention(..) | Recovered::CorruptJournal(_) => s.needs_attention += 1,
                        Recovered::LeftoversRetained(_, why) => s.notes.push(format!("Dokunulmadan bırakıldı: {why}")),
                        _ => {}
                    }
                }
            }
            Err(_) => {
                s.needs_attention += 1;
                s.notes.push("Kurtarma çalıştırılamadı.".to_string());
            }
        }
        // only something that actually happened is a rollback event: an examination that found nothing is not recorded
        if s.rolled_back + s.rolled_forward + s.needs_attention > 0 {
            Self::audit(
                conn,
                NewEvent::plain(
                    "rollback",
                    if s.needs_attention == 0 { "passed" } else { "failed" },
                    "install-recovery",
                    if s.needs_attention == 0 { None } else { Some("NEEDS_ATTENTION") },
                    "not-applicable",
                    "adapter",
                ),
            );
        }
        Ok(s)
    }
}

struct Prepared {
    resolved: Resolved,
    verified: crate::approval::VerifiedApproval,
    bytes: Option<Vec<u8>>,
    grant: ConsentGrant,
    computer: String,
}

fn txn_code(e: &TxnError) -> &'static str {
    match e {
        TxnError::Refused(c) => c,
        TxnError::Busy => "busy",
        TxnError::Pending(_) => "needs_attention",
        TxnError::Failed { code, .. } => code,
        TxnError::Interrupted(_) => "needs_attention",
    }
}

/// Audit codes are `'static`; the set of possible codes is closed and small, so interning them is bounded.
fn leak(s: String) -> &'static str {
    static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut g = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = g.iter().find(|x| **x == s) {
        return existing;
    }
    let leaked: &'static str = Box::leak(s.into_boxed_str());
    g.push(leaked);
    leaked
}

#[cfg(any(test, feature = "synthetic-pilot"))]
#[path = "pilot_supply.rs"]
pub mod supply;

#[cfg(test)]
#[path = "install_flow_tests.rs"]
mod tests;
