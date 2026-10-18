//! Phase 3A: the explicit user-approval contract.
//!
//! An approval is a *separate* object that binds one explicit user action to everything that makes an installation what it is:
//! the reviewed recipe (identity, revision and content digest), the complete canonical plan, the exact tool and version, the
//! intended computer and platform, the scope and project, the permissions and affected locations, the lifecycle action, and a
//! validity window. If any of these changes after the approval was given, [`verify_approval`] refuses it with the reason.
//!
//! What an approval is **not**:
//! * a plan id is a hash, not an approval, and a plan never carries one (`approvalState` is always `not_given`);
//! * a recipe being `reviewed` is not an approval;
//! * in this phase only *synthetic test actors* exist. There is no user-interface control that creates an approval, the
//!   production policy accepts no actor at all, and the only consumer is the synthetic transaction engine, which can touch
//!   nothing but its own isolated test directories.

#![allow(dead_code)] // the synthetic lifecycle is exercised by tests and the self-check only; no command reaches it

use crate::{
    audit::{canonical_json, sha256_hex},
    plan::InstallPlan,
    recipe::ValidatedRecipe,
};
use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

pub const APPROVAL_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_TTL_SECONDS: i64 = 900;
pub const MAX_TTL_SECONDS: i64 = 24 * 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Install,
    Update,
    Uninstall,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Install => "install",
            Action::Update => "update",
            Action::Uninstall => "uninstall",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Actor {
    /// Only `synthetic_test` exists in this phase.
    pub kind: String,
    pub id: String,
}

impl Actor {
    pub fn synthetic(id: &str) -> Actor {
        Actor { kind: "synthetic_test".to_string(), id: id.to_string() }
    }
}

/// Which actors a verifier accepts. Production accepts none: there is no real approval flow yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApprovalPolicy {
    allow_synthetic: bool,
}

impl ApprovalPolicy {
    pub const fn production() -> ApprovalPolicy {
        ApprovalPolicy { allow_synthetic: false }
    }

    /// For tests and the self-check probe, which exercise the synthetic engine only.
    pub const fn synthetic_only() -> ApprovalPolicy {
        ApprovalPolicy { allow_synthetic: true }
    }

    fn accepts(self, actor: &Actor) -> bool {
        self.allow_synthetic && actor.kind == "synthetic_test" && actor.id.len() >= 3 && actor.id.len() <= 64
    }
}

/// The text a person must have confirmed. Bound to tool, version, action and platform, so a confirmation for one thing is
/// not a confirmation for another.
pub fn expected_confirmation(action: Action, tool_id: &str, version: &str, platform: &str) -> String {
    format!("ONAYLIYORUM: {} {} {} {}", action.as_str(), tool_id, version, platform)
}

/// The explicit user action. A later phase builds this from a real confirmation control; today only tests do.
#[derive(Debug, Clone)]
pub struct UserAction {
    pub confirmed_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Approval {
    pub schema_version: u32,
    pub approval_id: String,
    pub action: Action,
    /// For `update`: the installed version being replaced.
    pub from_version: Option<String>,
    pub recipe_id: String,
    pub recipe_revision: u32,
    pub recipe_digest: String,
    pub plan_id: String,
    pub tool_id: String,
    pub version: String,
    pub platform_target: String,
    pub agent_surface: String,
    pub scope: String,
    pub project_alias: Option<String>,
    /// The computer the approval is for (an identifier of this installation of the manager, not a hardware identifier).
    pub computer_id: String,
    pub permissions_digest: String,
    pub prerequisites_digest: String,
    pub actor: Actor,
    pub approved_at: String,
    pub expires_at: String,
    /// SHA-256 over every other field. Evidence against tampering, not a signature.
    pub binding: String,
}

fn binding_of(a: &Approval) -> String {
    let mut blank = a.clone();
    blank.binding = String::new();
    match serde_json::to_value(&blank) {
        Ok(v) => sha256_hex(&canonical_json(&v)),
        Err(_) => String::new(),
    }
}

/// Digest of the material prerequisites of a plan: their states and evidence basis, the conflicts, and the blocker codes.
pub fn prerequisites_digest(plan: &InstallPlan) -> String {
    let v = serde_json::json!({
        "prerequisites": plan.prerequisites,
        "conflicts": plan.conflicts.iter().map(|c| c.code.clone()).collect::<Vec<_>>(),
        "blockers": plan.eligibility.blockers.iter().map(|b| b.code.clone()).collect::<Vec<_>>(),
    });
    sha256_hex(&canonical_json(&v))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalError {
    PlanNotValid(&'static str),
    PlanBlocked(Vec<String>),
    RecipeMismatch,
    RecipeChanged,
    PlanChanged,
    PrerequisitesChanged,
    PermissionsChanged,
    TargetChanged,
    WrongComputer,
    ActionChanged,
    ProjectUnknown,
    /// The approval names another project than the plan was made for (or names one for a user-scope plan).
    ProjectMismatch,
    ActorNotAllowed,
    ConfirmationMismatch,
    BadLifetime,
    NotYetValid,
    Expired,
    BindingMismatch,
    UnsupportedVersion,
    ActionNotSupportedByRecipe,
}

impl ApprovalError {
    pub fn code(&self) -> &'static str {
        match self {
            ApprovalError::PlanNotValid(_) => "plan_not_valid",
            ApprovalError::PlanBlocked(_) => "plan_blocked",
            ApprovalError::RecipeMismatch => "recipe_mismatch",
            ApprovalError::RecipeChanged => "recipe_changed",
            ApprovalError::PlanChanged => "plan_changed",
            ApprovalError::PrerequisitesChanged => "prerequisites_changed",
            ApprovalError::PermissionsChanged => "permissions_changed",
            ApprovalError::TargetChanged => "target_changed",
            ApprovalError::WrongComputer => "wrong_computer",
            ApprovalError::ActionChanged => "action_changed",
            ApprovalError::ProjectUnknown => "project_unknown",
            ApprovalError::ProjectMismatch => "project_mismatch",
            ApprovalError::ActorNotAllowed => "actor_not_allowed",
            ApprovalError::ConfirmationMismatch => "confirmation_mismatch",
            ApprovalError::BadLifetime => "bad_lifetime",
            ApprovalError::NotYetValid => "not_yet_valid",
            ApprovalError::Expired => "expired",
            ApprovalError::BindingMismatch => "binding_mismatch",
            ApprovalError::UnsupportedVersion => "unsupported_version",
            ApprovalError::ActionNotSupportedByRecipe => "action_not_supported_by_recipe",
        }
    }
}

/// Everything that is true NOW, against which a given approval is judged.
pub struct Situation<'a> {
    pub plan: &'a InstallPlan,
    pub recipe: &'a ValidatedRecipe,
    pub action: Action,
    pub from_version: Option<&'a str>,
    pub computer_id: &'a str,
    pub host_platform: &'a str,
    pub now: DateTime<Utc>,
    pub conn: &'a Connection,
}

fn check_plan_and_target(s: &Situation) -> Result<(), ApprovalError> {
    let plan = s.plan;
    plan.check_invariants().map_err(ApprovalError::PlanNotValid)?;
    plan.check_semantics().map_err(ApprovalError::PlanNotValid)?;
    let r = s.recipe.recipe();
    if plan.recipe.state != "validated"
        || plan.recipe.recipe_digest.as_deref() != Some(s.recipe.digest())
        || plan.recipe.recipe_id.as_deref() != Some(r.recipe_id.as_str())
        || plan.entry.entry_id != r.tool_id
    {
        return Err(ApprovalError::RecipeMismatch);
    }
    // The only thing still missing from the plan may be this very approval.
    let others: Vec<String> =
        plan.eligibility.blockers.iter().filter(|b| b.code != "user_approval_missing").map(|b| b.code.clone()).collect();
    if !others.is_empty() {
        return Err(ApprovalError::PlanBlocked(others));
    }
    if plan.target.platform_target != r.target.platform_target
        || plan.target.agent_surface != r.target.agent_surface
        || plan.target.scope != r.target.scope
        || plan.target.platform_target != s.host_platform
        || !plan.target.this_computer_matches
    {
        return Err(ApprovalError::TargetChanged);
    }
    if !r.lifecycle_actions.iter().any(|a| a == s.action.as_str()) {
        return Err(ApprovalError::ActionNotSupportedByRecipe);
    }
    match (s.action, s.from_version) {
        (Action::Update, Some(from)) if from != r.version && !from.is_empty() && from.len() <= 64 => {}
        (Action::Install | Action::Uninstall, None) => {}
        _ => return Err(ApprovalError::ActionChanged),
    }
    Ok(())
}

/// Creates an approval from an explicit user action. Fails closed on every mismatch.
#[allow(clippy::too_many_arguments)]
pub fn approve(
    s: &Situation,
    project_alias: Option<&str>,
    actor: Actor,
    user_action: &UserAction,
    policy: ApprovalPolicy,
    ttl_seconds: i64,
) -> Result<Approval, ApprovalError> {
    check_plan_and_target(s)?;
    // the project is part of the plan itself; an approval can only name that very project
    if project_alias != s.plan.target.project_alias.as_deref() {
        return Err(ApprovalError::ProjectMismatch);
    }
    if !policy.accepts(&actor) {
        return Err(ApprovalError::ActorNotAllowed);
    }
    let r = s.recipe.recipe();
    if user_action.confirmed_text != expected_confirmation(s.action, &r.tool_id, &r.version, &r.target.platform_target) {
        return Err(ApprovalError::ConfirmationMismatch);
    }
    if !(1..=MAX_TTL_SECONDS).contains(&ttl_seconds) {
        return Err(ApprovalError::BadLifetime);
    }
    check_project(s, project_alias)?;
    let mut a = Approval {
        schema_version: APPROVAL_SCHEMA_VERSION,
        approval_id: uuid::Uuid::new_v4().to_string(),
        action: s.action,
        from_version: s.from_version.map(str::to_string),
        recipe_id: r.recipe_id.clone(),
        recipe_revision: r.revision,
        recipe_digest: s.recipe.digest().to_string(),
        plan_id: s.plan.plan_id.clone(),
        tool_id: r.tool_id.clone(),
        version: r.version.clone(),
        platform_target: r.target.platform_target.clone(),
        agent_surface: r.target.agent_surface.clone(),
        scope: r.target.scope.clone(),
        project_alias: project_alias.map(str::to_string),
        computer_id: s.computer_id.to_string(),
        permissions_digest: s.recipe.permissions_digest(),
        prerequisites_digest: prerequisites_digest(s.plan),
        actor,
        approved_at: s.now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        expires_at: (s.now + Duration::seconds(ttl_seconds)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        binding: String::new(),
    };
    a.binding = binding_of(&a);
    Ok(a)
}

fn check_project(s: &Situation, alias: Option<&str>) -> Result<(), ApprovalError> {
    match (s.recipe.recipe().target.scope.as_str(), alias) {
        ("project", Some(a)) if crate::projects::valid_alias(a) && crate::projects::is_registered(s.conn, a) => Ok(()),
        ("project", _) => Err(ApprovalError::ProjectUnknown),
        (_, None) => Ok(()),
        (_, Some(_)) => Err(ApprovalError::TargetChanged),
    }
}

/// An approval that was checked against the situation at one moment. Only [`verify_approval`] makes one; the transaction engine
/// accepts nothing else.
#[derive(Debug, Clone)]
pub struct VerifiedApproval {
    approval: Approval,
    expires_at: DateTime<Utc>,
}

impl VerifiedApproval {
    pub fn approval(&self) -> &Approval {
        &self.approval
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}

fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|t| t.with_timezone(&Utc))
}

/// Judges an approval against what is true now. Every change since it was given is a distinct refusal.
pub fn verify_approval(a: &Approval, s: &Situation, policy: ApprovalPolicy) -> Result<VerifiedApproval, ApprovalError> {
    if a.schema_version != APPROVAL_SCHEMA_VERSION {
        return Err(ApprovalError::UnsupportedVersion);
    }
    // 1. the object itself must be what was created
    if a.binding.len() != 64 || binding_of(a) != a.binding {
        return Err(ApprovalError::BindingMismatch);
    }
    if !policy.accepts(&a.actor) {
        return Err(ApprovalError::ActorNotAllowed);
    }
    // 2. its lifetime
    let (Some(approved), Some(expires)) = (parse_time(&a.approved_at), parse_time(&a.expires_at)) else {
        return Err(ApprovalError::BadLifetime);
    };
    if expires <= approved || (expires - approved).num_seconds() > MAX_TTL_SECONDS {
        return Err(ApprovalError::BadLifetime);
    }
    if s.now < approved {
        return Err(ApprovalError::NotYetValid);
    }
    if s.now >= expires {
        return Err(ApprovalError::Expired);
    }
    // 3. everything it was bound to, as it is now
    let r = s.recipe.recipe();
    if a.recipe_digest != s.recipe.digest()
        || a.recipe_id != r.recipe_id
        || a.recipe_revision != r.revision
        || a.tool_id != r.tool_id
        || a.version != r.version
    {
        return Err(ApprovalError::RecipeChanged);
    }
    if a.action != s.action || a.from_version.as_deref() != s.from_version {
        return Err(ApprovalError::ActionChanged);
    }
    if a.computer_id != s.computer_id {
        return Err(ApprovalError::WrongComputer);
    }
    if a.platform_target != s.plan.target.platform_target
        || a.agent_surface != s.plan.target.agent_surface
        || a.scope != s.plan.target.scope
        || a.platform_target != s.host_platform
    {
        return Err(ApprovalError::TargetChanged);
    }
    check_plan_and_target(s)?;
    if a.project_alias != s.plan.target.project_alias {
        return Err(ApprovalError::ProjectMismatch);
    }
    check_project(s, a.project_alias.as_deref())?;
    if a.permissions_digest != s.recipe.permissions_digest() {
        return Err(ApprovalError::PermissionsChanged);
    }
    if a.prerequisites_digest != prerequisites_digest(s.plan) {
        return Err(ApprovalError::PrerequisitesChanged);
    }
    if a.plan_id != s.plan.plan_id {
        return Err(ApprovalError::PlanChanged);
    }
    Ok(VerifiedApproval { approval: a.clone(), expires_at: expires })
}

#[cfg(test)]
#[path = "approval_tests.rs"]
mod tests;
