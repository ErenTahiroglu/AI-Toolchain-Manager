//! Phase 3A approval-contract tests. Synthetic recipes and synthetic actors only; there is no real approval flow.

use super::*;
use crate::catalog::CatalogEntry;
use crate::{
    plan::{self, PlanRequest},
    synthetic::{self, testkit::*},
};
use chrono::Duration;
use serde_json::json;

const COMPUTER: &str = "synthetic-computer-1";

fn base_fx() -> Fx {
    fx("synthetic-tool", "1.0.0", b"payload")
}

fn rebind(a: &mut Approval) {
    a.binding = binding_of(a);
}

fn verify(f: &Fx, a: &Approval, now: DateTime<Utc>) -> Result<VerifiedApproval, ApprovalError> {
    verify_approval(a, &situation(f, Action::Install, None, COMPUTER, now), ApprovalPolicy::synthetic_only())
}

fn err_of<T>(r: Result<T, ApprovalError>) -> ApprovalError {
    match r {
        Err(e) => e,
        Ok(_) => panic!("expected a refusal"),
    }
}

// ---- the happy path and what it is not ---------------------------------------------------------------------------------

#[test]
fn an_explicit_synthetic_confirmation_creates_an_approval_bound_to_everything_and_the_plan_stays_a_draft() {
    let f = base_fx();
    assert_eq!(f.plan.approval_state, "not_given");
    let a = make_approval(&f, Action::Install, None, COMPUTER, t0());
    assert_eq!(a.binding.len(), 64);
    assert_eq!((a.tool_id.as_str(), a.version.as_str(), a.platform_target.as_str()), ("synthetic-tool", "1.0.0", this_platform()));
    assert_eq!(a.plan_id, f.plan.plan_id);
    assert_eq!(a.recipe_digest, f.valid.digest());
    assert_eq!(a.computer_id, COMPUTER);
    assert!(a.expires_at > a.approved_at);
    let v = verify(&f, &a, t0() + Duration::seconds(30)).unwrap();
    assert_eq!(v.approval(), &a);
    // creating an approval changed nothing about the plan, which still says it is a draft
    assert_eq!(f.plan.approval_state, "not_given");
    assert_eq!(f.plan.eligibility.blockers.iter().map(|b| b.code.as_str()).collect::<Vec<_>>(), vec!["user_approval_missing"]);
}

#[test]
fn production_accepts_no_actor_at_all_and_a_plan_hash_is_not_an_approval() {
    let f = base_fx();
    let a = make_approval(&f, Action::Install, None, COMPUTER, t0());
    assert_eq!(
        err_of(verify_approval(&a, &situation(&f, Action::Install, None, COMPUTER, t0()), ApprovalPolicy::production())),
        ApprovalError::ActorNotAllowed
    );
    // approving under the production policy is refused as well
    let refused = approve(
        &situation(&f, Action::Install, None, COMPUTER, t0()),
        None,
        Actor::synthetic("synthetic-user-1"),
        &confirm(Action::Install, &f),
        ApprovalPolicy::production(),
        600,
    );
    assert_eq!(err_of(refused), ApprovalError::ActorNotAllowed);
    // a hand-made object carrying a correct plan id and a self-computed binding is still not accepted by production
    let mut hand_made = a.clone();
    hand_made.approval_id = "made-by-hand".to_string();
    rebind(&mut hand_made);
    assert_eq!(
        err_of(verify_approval(&hand_made, &situation(&f, Action::Install, None, COMPUTER, t0()), ApprovalPolicy::production())),
        ApprovalError::ActorNotAllowed
    );
    // and the plan id by itself is just text
    assert_eq!(f.plan.plan_id.len(), 64);
}

// ---- refusals when creating --------------------------------------------------------------------------------------------

#[test]
fn nothing_is_approved_without_the_exact_confirmation_a_valid_lifetime_a_synthetic_actor_and_a_supported_action() {
    let f = base_fx();
    let sit = |action, from| situation(&f, action, from, COMPUTER, t0());
    let make = |action, from, text: &str, actor: Actor, ttl| {
        approve(&sit(action, from), None, actor, &UserAction { confirmed_text: text.to_string() }, ApprovalPolicy::synthetic_only(), ttl)
    };
    let good = expected_confirmation(Action::Install, "synthetic-tool", "1.0.0", this_platform());
    assert!(make(Action::Install, None, &good, Actor::synthetic("synthetic-user-1"), 600).is_ok());
    // a confirmation for something else
    for text in [
        "",
        "evet",
        &expected_confirmation(Action::Uninstall, "synthetic-tool", "1.0.0", this_platform()),
        &expected_confirmation(Action::Install, "synthetic-tool", "2.0.0", this_platform()),
        &expected_confirmation(Action::Install, "synthetic-other", "1.0.0", this_platform()),
        &expected_confirmation(Action::Install, "synthetic-tool", "1.0.0", other_platform()),
        &format!("{good} "),
    ] {
        assert_eq!(
            err_of(make(Action::Install, None, text, Actor::synthetic("synthetic-user-1"), 600)),
            ApprovalError::ConfirmationMismatch,
            "{text:?}"
        );
    }
    assert_eq!(
        err_of(make(Action::Install, None, &good, Actor { kind: "user".to_string(), id: "someone".to_string() }, 600)),
        ApprovalError::ActorNotAllowed
    );
    assert_eq!(err_of(make(Action::Install, None, &good, Actor::synthetic("x"), 600)), ApprovalError::ActorNotAllowed);
    for ttl in [0, -5, MAX_TTL_SECONDS + 1] {
        assert_eq!(
            err_of(make(Action::Install, None, &good, Actor::synthetic("synthetic-user-1"), ttl)),
            ApprovalError::BadLifetime,
            "{ttl}"
        );
    }
    assert!(make(Action::Install, None, &good, Actor::synthetic("synthetic-user-1"), MAX_TTL_SECONDS).is_ok());
    // actions: update needs a source version that differs; install and uninstall must not name one
    let upd = expected_confirmation(Action::Update, "synthetic-tool", "1.0.0", this_platform());
    assert_eq!(err_of(make(Action::Update, None, &upd, Actor::synthetic("synthetic-user-1"), 600)), ApprovalError::ActionChanged);
    assert_eq!(err_of(make(Action::Update, Some("1.0.0"), &upd, Actor::synthetic("synthetic-user-1"), 600)), ApprovalError::ActionChanged);
    assert!(make(Action::Update, Some("0.9.0"), &upd, Actor::synthetic("synthetic-user-1"), 600).is_ok());
    assert_eq!(
        err_of(make(Action::Install, Some("0.9.0"), &good, Actor::synthetic("synthetic-user-1"), 600)),
        ApprovalError::ActionChanged
    );
}

#[test]
fn a_plan_that_is_blocked_for_any_other_reason_cannot_be_approved() {
    let c = conn();
    let build = |edit: &dyn Fn(&mut CatalogEntry)| {
        let mut sc = synthetic::Scenario::new(&base(&c), "synthetic-tool", "1.0.0", this_platform(), b"payload");
        edit(&mut sc.entry);
        let plan = sc.plan();
        let valid = sc.validated();
        Fx { conn: conn(), sc, plan, valid }
    };
    let try_approve = |f: &Fx| {
        approve(
            &situation(f, Action::Install, None, COMPUTER, t0()),
            None,
            Actor::synthetic("synthetic-user-1"),
            &confirm(Action::Install, f),
            ApprovalPolicy::synthetic_only(),
            600,
        )
    };
    assert!(try_approve(&build(&|_| {})).is_ok(), "control");
    match err_of(try_approve(&build(&|e| e.review_state = "quarantined".to_string()))) {
        ApprovalError::PlanBlocked(codes) => assert!(codes.contains(&"entry_quarantined".to_string())),
        other => panic!("{other:?}"),
    }
    match err_of(try_approve(&build(&|e| e.record["dependencies"] = json!(["Node.js >=22"])))) {
        ApprovalError::PlanBlocked(codes) => assert!(codes.contains(&"prerequisites_unverified".to_string()), "{codes:?}"),
        other => panic!("{other:?}"),
    }
    match err_of(try_approve(&build(&|e| e.execution_policy = "blocked-until-scoped-authorization".to_string()))) {
        ApprovalError::PlanBlocked(codes) => assert!(codes.contains(&"execution_policy_blocked".to_string())),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_plan_without_a_validated_recipe_or_for_another_recipe_or_platform_cannot_be_approved() {
    let f = base_fx();
    // the draft plan of a real catalog entry (no recipe)
    let real =
        plan::build_plan(&base(&f.conn), &synthetic::request("gitleaks", this_platform()), &synthetic::host(this_platform()), &[]).unwrap();
    let with_real_plan = Fx {
        conn: conn(),
        sc: synthetic::Scenario::new(&base(&f.conn), "synthetic-tool", "1.0.0", this_platform(), b"payload"),
        plan: real,
        valid: f.valid.clone(),
    };
    let r = approve(
        &situation(&with_real_plan, Action::Install, None, COMPUTER, t0()),
        None,
        Actor::synthetic("synthetic-user-1"),
        &confirm(Action::Install, &f),
        ApprovalPolicy::synthetic_only(),
        600,
    );
    assert!(matches!(err_of(r), ApprovalError::RecipeMismatch | ApprovalError::PlanBlocked(_)));
    // a recipe/plan pair for different versions
    let other = fx("synthetic-tool", "2.0.0", b"payload two");
    let mixed = Fx {
        conn: conn(),
        sc: synthetic::Scenario::new(&base(&f.conn), "synthetic-tool", "1.0.0", this_platform(), b"payload"),
        plan: f.plan.clone(),
        valid: other.valid.clone(),
    };
    let r = approve(
        &situation(&mixed, Action::Install, None, COMPUTER, t0()),
        None,
        Actor::synthetic("synthetic-user-1"),
        &confirm(Action::Install, &other),
        ApprovalPolicy::synthetic_only(),
        600,
    );
    assert_eq!(err_of(r), ApprovalError::RecipeMismatch);
    // a plan for the other platform is not for this computer
    let foreign = fx_on(other_platform(), "synthetic-tool", "1.0.0", b"payload");
    let r = approve(
        &Situation { host_platform: this_platform(), ..situation(&foreign, Action::Install, None, COMPUTER, t0()) },
        None,
        Actor::synthetic("synthetic-user-1"),
        &confirm(Action::Install, &foreign),
        ApprovalPolicy::synthetic_only(),
        600,
    );
    assert!(matches!(err_of(r), ApprovalError::TargetChanged | ApprovalError::PlanBlocked(_)));
}

// ---- projects ------------------------------------------------------------------------------------------------------------

/// A project-scope scenario for `alias` (both "proje-a" and "proje-b" are registered projects in its database).
fn project_fx_for(alias: &str) -> Fx {
    let mut f = base_fx();
    crate::projects::add_reference(&f.conn, "proje-b", "Proje B", "").unwrap();
    f.sc.reviewed.recipe.target.scope = "project".to_string();
    synthetic::rereview(&mut f.sc.reviewed);
    f.valid = f.sc.validated();
    f.plan = project_plan(&f, alias);
    assert_eq!(f.plan.recipe.state, "validated");
    f
}

fn project_plan(f: &Fx, alias: &str) -> plan::InstallPlan {
    let req = PlanRequest {
        scope: "project".to_string(),
        project_alias: Some(alias.to_string()),
        ..synthetic::request("synthetic-tool", this_platform())
    };
    plan::build_plan_with(&f.sc.entry, &req, &synthetic::host(this_platform()), &[], &[&f.sc.reviewed], &f.sc.refs).unwrap()
}

fn project_approval(f: &Fx, alias: Option<&str>) -> Result<Approval, ApprovalError> {
    approve(
        &situation(f, Action::Install, None, COMPUTER, t0()),
        alias,
        Actor::synthetic("synthetic-user-1"),
        &confirm(Action::Install, f),
        ApprovalPolicy::synthetic_only(),
        600,
    )
}

#[test]
fn two_projects_with_identical_facts_get_different_plan_ids_and_an_approval_for_one_never_verifies_for_the_other() {
    let a = project_fx_for("proje-a");
    let b = project_fx_for("proje-b");
    // identical tool, platform, agent, scope and observations: only the project identity differs
    assert_eq!((a.plan.target.project_alias.as_deref(), b.plan.target.project_alias.as_deref()), (Some("proje-a"), Some("proje-b")));
    assert_ne!(a.plan.plan_id, b.plan.plan_id, "the project is part of the canonical plan");
    let strip = |p: &plan::InstallPlan| {
        let mut v = serde_json::to_value(p).unwrap();
        v["target"]["projectAlias"] = serde_json::Value::Null;
        v["planId"] = serde_json::Value::Null;
        v
    };
    assert_eq!(strip(&a.plan), strip(&b.plan), "nothing else differs");
    // the plan id is stable per project
    assert_eq!(a.plan.plan_id, project_plan(&a, "proje-a").plan_id);
    // approve A with the synthetic actor
    let approval_a = project_approval(&a, Some("proje-a")).unwrap();
    assert_eq!(approval_a.project_alias.as_deref(), Some("proje-a"));
    assert!(verify(&a, &approval_a, t0()).is_ok());
    // A's approval against B's plan (both projects exist, everything else is identical): explicitly refused
    assert_eq!(err_of(verify(&b, &approval_a, t0())), ApprovalError::ProjectMismatch);
    // and approving B's plan with A's alias is refused as well
    assert_eq!(err_of(project_approval(&b, Some("proje-a"))), ApprovalError::ProjectMismatch);
    assert_eq!(err_of(project_approval(&a, Some("proje-b"))), ApprovalError::ProjectMismatch);
    // a hand-made approval naming B, bound to A's plan id, with a recomputed binding: still refused
    let mut forged = approval_a.clone();
    forged.project_alias = Some("proje-b".to_string());
    rebind(&mut forged);
    assert_eq!(err_of(verify(&a, &forged, t0())), ApprovalError::ProjectMismatch);
    // naming B AND carrying B's plan id passes the plan-id comparison but not the situation of A
    let mut forged_b = forged.clone();
    forged_b.plan_id = b.plan.plan_id.clone();
    rebind(&mut forged_b);
    assert_eq!(err_of(verify(&a, &forged_b, t0())), ApprovalError::ProjectMismatch);
    // without an alias at all
    let mut none = approval_a.clone();
    none.project_alias = None;
    rebind(&mut none);
    assert_eq!(err_of(verify(&a, &none, t0())), ApprovalError::ProjectMismatch);
}

#[test]
fn a_project_approval_needs_a_registered_project_and_stops_being_valid_when_it_changes_or_disappears() {
    let f = project_fx_for("proje-a");
    assert_eq!(err_of(project_approval(&f, None)), ApprovalError::ProjectMismatch, "a project plan needs its project");
    let a = project_approval(&f, Some("proje-a")).unwrap();
    assert!(verify(&f, &a, t0()).is_ok());
    // approval expiry still applies
    assert_eq!(err_of(verify(&f, &a, t0() + Duration::seconds(600))), ApprovalError::Expired);
    // the project identity changes after approval: the situation now is the other project's plan
    let other_plan = project_plan(&f, "proje-b");
    let moved = Fx { plan: other_plan, ..project_fx_for("proje-b") };
    assert_eq!(err_of(verify(&moved, &a, t0())), ApprovalError::ProjectMismatch);
    // the project is removed after the approval: it no longer binds
    crate::projects::remove_reference(&f.conn, "proje-a").unwrap();
    assert_eq!(err_of(verify(&f, &a, t0())), ApprovalError::ProjectUnknown);
    // a plan for a well-formed but unregistered project can be built as data, but cannot be approved
    let unknown = Fx { plan: project_plan(&f, "proje-yok"), ..project_fx_for("proje-a") };
    assert_eq!(err_of(project_approval(&unknown, Some("proje-yok"))), ApprovalError::ProjectUnknown);
}

#[test]
fn malformed_aliases_and_user_scope_plans_with_an_alias_never_become_plans_or_approvals() {
    let f = base_fx();
    let host = synthetic::host(this_platform());
    for bad in ["C:\\Users\\x", "../x", "A", "x", "proje a", "-bad", "PROJE", &"a".repeat(60)] {
        let r = PlanRequest {
            scope: "project".to_string(),
            project_alias: Some(bad.to_string()),
            ..synthetic::request("synthetic-tool", this_platform())
        };
        assert_eq!(
            plan::build_plan_with(&f.sc.entry, &r, &host, &[], &[&f.sc.reviewed], &f.sc.refs).unwrap_err().code,
            "PLAN_REQUEST_INVALID",
            "{bad:?}"
        );
    }
    // a user-scope request with an alias, and a project-scope request without one
    let r = PlanRequest { project_alias: Some("proje-a".to_string()), ..synthetic::request("synthetic-tool", this_platform()) };
    assert_eq!(plan::build_plan_with(&f.sc.entry, &r, &host, &[], &[&f.sc.reviewed], &f.sc.refs).unwrap_err().code, "PLAN_REQUEST_INVALID");
    let r = PlanRequest { scope: "project".to_string(), ..synthetic::request("synthetic-tool", this_platform()) };
    assert_eq!(plan::build_plan_with(&f.sc.entry, &r, &host, &[], &[&f.sc.reviewed], &f.sc.refs).unwrap_err().code, "PLAN_REQUEST_INVALID");
    // a user-scope plan cannot be approved for a project …
    assert_eq!(err_of(project_approval(&f, Some("proje-a"))), ApprovalError::ProjectMismatch);
    // … and a plan edited to carry an alias (or to lose it) is not a valid plan at all
    for edit in [(|v: &mut serde_json::Value| v["target"]["projectAlias"] = json!("proje-a")) as fn(&mut serde_json::Value), |v| {
        v["target"]["scope"] = json!("project")
    }] {
        let mut v = serde_json::to_value(&f.plan).unwrap();
        edit(&mut v);
        let forged: plan::InstallPlan = serde_json::from_value(v).unwrap();
        assert_eq!(forged.check_semantics().unwrap_err(), "project_identity_inconsistent");
        let g = Fx { plan: forged, ..base_fx() };
        assert!(matches!(err_of(project_approval(&g, None)), ApprovalError::PlanNotValid("project_identity_inconsistent")));
    }
    let mut v = serde_json::to_value(&project_fx_for("proje-a").plan).unwrap();
    v["target"]["projectAlias"] = json!("Not A Safe Alias");
    assert_eq!(serde_json::from_value::<plan::InstallPlan>(v).unwrap().check_semantics().unwrap_err(), "project_identity_inconsistent");
}

#[test]
fn a_version_one_plan_is_not_readable_and_is_never_reinterpreted() {
    let f = base_fx();
    let mut v = serde_json::to_value(&f.plan).unwrap();
    v["schemaVersion"] = json!(1);
    v["plannerVersion"] = json!("1.0.0");
    v["target"].as_object_mut().unwrap().remove("projectAlias");
    assert_eq!(plan::parse_plan(&v.to_string()).unwrap_err(), "unsupported_schema_version");
    assert_eq!(f.plan.schema_version, 2);
}

#[test]
fn a_plan_request_for_an_unregistered_project_never_even_reaches_a_plan() {
    let c = conn();
    let host = plan::HostFacts { platform: this_platform().to_string(), elevated: Some(false) };
    let r = PlanRequest {
        scope: "project".to_string(),
        project_alias: Some("proje-yok".to_string()),
        ..synthetic::request("gitleaks", this_platform())
    };
    assert_eq!(plan::plan_for(&c, &r, &host).unwrap_err().code, "PLAN_REQUEST_INVALID");
}

// ---- an approval stops being valid when anything it was bound to changes -----------------------------------------------

#[test]
fn every_edit_of_an_approval_breaks_its_binding_and_a_recomputed_binding_still_meets_the_current_facts() {
    let f = base_fx();
    let a = make_approval(&f, Action::Install, None, COMPUTER, t0());
    type Edit = fn(&mut Approval);
    let edits: Vec<(&str, Edit, ApprovalError)> = vec![
        ("recipe digest", |a| a.recipe_digest = "0".repeat(64), ApprovalError::RecipeChanged),
        ("recipe revision", |a| a.recipe_revision = 9, ApprovalError::RecipeChanged),
        ("recipe id", |a| a.recipe_id = "other".to_string(), ApprovalError::RecipeChanged),
        ("tool", |a| a.tool_id = "synthetic-other".to_string(), ApprovalError::RecipeChanged),
        ("version", |a| a.version = "9.9.9".to_string(), ApprovalError::RecipeChanged),
        ("plan id", |a| a.plan_id = "1".repeat(64), ApprovalError::PlanChanged),
        ("platform", |a| a.platform_target = other_platform().to_string(), ApprovalError::TargetChanged),
        ("surface", |a| a.agent_surface = "claude-code-terminal".to_string(), ApprovalError::TargetChanged),
        ("scope", |a| a.scope = "project".to_string(), ApprovalError::TargetChanged),
        ("computer", |a| a.computer_id = "synthetic-computer-2".to_string(), ApprovalError::WrongComputer),
        ("permissions", |a| a.permissions_digest = "2".repeat(64), ApprovalError::PermissionsChanged),
        ("prerequisites", |a| a.prerequisites_digest = "3".repeat(64), ApprovalError::PrerequisitesChanged),
        ("action", |a| a.action = Action::Uninstall, ApprovalError::ActionChanged),
        ("from version", |a| a.from_version = Some("0.1.0".to_string()), ApprovalError::ActionChanged),
        ("actor", |a| a.actor = Actor { kind: "user".to_string(), id: "someone".to_string() }, ApprovalError::ActorNotAllowed),
    ];
    for (name, edit, expected) in edits {
        let mut tampered = a.clone();
        edit(&mut tampered);
        assert_eq!(err_of(verify(&f, &tampered, t0())), ApprovalError::BindingMismatch, "{name}: an edit without recomputing the binding");
        rebind(&mut tampered);
        assert_eq!(err_of(verify(&f, &tampered, t0())), expected, "{name}: edit with a recomputed binding");
    }
    let mut wrong_version = a.clone();
    wrong_version.schema_version = 2;
    assert_eq!(err_of(verify(&f, &wrong_version, t0())), ApprovalError::UnsupportedVersion);
    let mut short = a.clone();
    short.binding = "abc".to_string();
    assert_eq!(err_of(verify(&f, &short, t0())), ApprovalError::BindingMismatch);
}

#[test]
fn an_approval_expires_cannot_be_dated_in_the_future_and_has_a_bounded_lifetime() {
    let f = base_fx();
    let a = make_approval(&f, Action::Install, None, COMPUTER, t0());
    assert!(verify(&f, &a, t0()).is_ok());
    assert!(verify(&f, &a, t0() + Duration::seconds(599)).is_ok());
    assert_eq!(err_of(verify(&f, &a, t0() + Duration::seconds(600))), ApprovalError::Expired, "the instant of expiry is already too late");
    assert_eq!(err_of(verify(&f, &a, t0() + Duration::days(2))), ApprovalError::Expired);
    assert_eq!(err_of(verify(&f, &a, t0() - Duration::seconds(1))), ApprovalError::NotYetValid);
    let mut long = a.clone();
    long.expires_at = (t0() + Duration::days(30)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    rebind(&mut long);
    assert_eq!(err_of(verify(&f, &long, t0())), ApprovalError::BadLifetime);
    let mut backwards = a.clone();
    backwards.expires_at = backwards.approved_at.clone();
    rebind(&mut backwards);
    assert_eq!(err_of(verify(&f, &backwards, t0())), ApprovalError::BadLifetime);
    let mut garbage = a.clone();
    garbage.approved_at = "yesterday".to_string();
    rebind(&mut garbage);
    assert_eq!(err_of(verify(&f, &garbage, t0())), ApprovalError::BadLifetime);
}

#[test]
fn a_changed_recipe_a_changed_plan_changed_prerequisites_or_a_new_blocker_invalidate_an_existing_approval() {
    let f = base_fx();
    let a = make_approval(&f, Action::Install, None, COMPUTER, t0());
    // the recipe is edited and re-reviewed (a new revision with a new digest)
    let mut revised = f.sc.reviewed.clone();
    revised.recipe.revision = 2;
    synthetic::rereview(&mut revised);
    let g = Fx {
        conn: conn(),
        plan: plan::build_plan_with(
            &f.sc.entry,
            &synthetic::request("synthetic-tool", this_platform()),
            &synthetic::host(this_platform()),
            &[],
            &[&revised],
            &f.sc.refs,
        )
        .unwrap(),
        valid: crate::recipe::validate(&revised, &f.sc.refs).unwrap(),
        sc: synthetic::Scenario::new(&base(&f.conn), "synthetic-tool", "1.0.0", this_platform(), b"payload"),
    };
    assert_eq!(err_of(verify(&g, &a, t0())), ApprovalError::RecipeChanged);
    // the permissions of the recipe grow (even when re-reviewed, the digest differs)
    let mut wider = f.sc.reviewed.clone();
    wider.recipe.permissions.push("network_download".to_string());
    wider.recipe.network.push("github_release_download".to_string());
    synthetic::rereview(&mut wider);
    let h = Fx {
        conn: conn(),
        plan: plan::build_plan_with(
            &f.sc.entry,
            &synthetic::request("synthetic-tool", this_platform()),
            &synthetic::host(this_platform()),
            &[],
            &[&wider],
            &f.sc.refs,
        )
        .unwrap(),
        valid: crate::recipe::validate(&wider, &f.sc.refs).unwrap(),
        sc: synthetic::Scenario::new(&base(&f.conn), "synthetic-tool", "1.0.0", this_platform(), b"payload"),
    };
    assert_eq!(err_of(verify(&h, &a, t0())), ApprovalError::RecipeChanged);
    // the plan's content is edited (its id no longer matches the approval)
    let mut edited = f.plan.clone();
    edited.unknowns.push("a new unknown".to_string());
    edited.plan_id = "4".repeat(64);
    let e = Fx {
        conn: conn(),
        plan: edited,
        valid: f.valid.clone(),
        sc: synthetic::Scenario::new(&base(&f.conn), "synthetic-tool", "1.0.0", this_platform(), b"payload"),
    };
    assert_eq!(err_of(verify(&e, &a, t0())), ApprovalError::PlanChanged);
    // a material prerequisite changes: an existing installation now appears in the observations
    let c = conn();
    let mut sc = synthetic::Scenario::new(&base(&c), "synthetic-tool", "1.0.0", this_platform(), b"payload");
    let obs = existing_install_observation(&c, this_platform());
    let plan_with_conflict = plan::build_plan_with(
        &sc.entry,
        &synthetic::request("synthetic-tool", this_platform()),
        &synthetic::host(this_platform()),
        std::slice::from_ref(&obs),
        &[&sc.reviewed],
        &sc.refs,
    )
    .unwrap();
    assert!(plan_with_conflict.conflicts.iter().any(|c| c.code == "existing_installation_found"));
    sc.entry.record["dependencies"] = json!([]);
    let k = Fx { conn: c, plan: plan_with_conflict, valid: f.valid.clone(), sc };
    assert_eq!(err_of(verify(&k, &a, t0())), ApprovalError::PrerequisitesChanged);
    // a new blocker (the entry is quarantined later) refuses it
    let mut q = f.sc.entry.clone();
    q.review_state = "quarantined".to_string();
    let quarantined = plan::build_plan_with(
        &q,
        &synthetic::request("synthetic-tool", this_platform()),
        &synthetic::host(this_platform()),
        &[],
        &[&f.sc.reviewed],
        &f.sc.refs,
    )
    .unwrap();
    let qf = Fx {
        conn: conn(),
        plan: quarantined,
        valid: f.valid.clone(),
        sc: synthetic::Scenario::new(&base(&f.conn), "synthetic-tool", "1.0.0", this_platform(), b"payload"),
    };
    assert!(matches!(err_of(verify(&qf, &a, t0())), ApprovalError::PlanBlocked(_)));
}

/// A current, present observation of the synthetic tool, made by a real scan of a synthetic machine.
fn existing_install_observation(c: &rusqlite::Connection, platform: &str) -> crate::discovery::store::StoredObservation {
    use crate::discovery::{
        detect::{Authorization, Roots},
        run_scan,
    };
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    // the catalog knows "gitleaks" by name, so the scan can look for it; the observation is then relabelled for the synthetic tool
    let name = if platform == WIN { "gitleaks.exe" } else { "gitleaks" };
    std::fs::write(bin.join(name), b"synthetic placeholder").unwrap();
    let roots = Roots {
        platform: platform.to_string(),
        home: Some(root.join("home")),
        path_entries: vec![bin],
        system_dirs: vec![],
        pins: vec![],
        references: Default::default(),
    };
    run_scan(c, &roots, &Authorization { path_executables: true, user_config: false, project: None, fingerprint_artifacts: false })
        .unwrap();
    let mut o = crate::discovery::store::latest_observations(c)
        .unwrap()
        .into_iter()
        .find(|o| o.subject == "gitleaks" && o.presence == "present")
        .unwrap();
    o.subject = "synthetic-tool".to_string();
    o
}

#[test]
fn approvals_do_not_cross_actions_computers_or_platforms() {
    let f = base_fx();
    let install = make_approval(&f, Action::Install, None, COMPUTER, t0());
    // the same approval cannot authorize an uninstall or an update
    assert_eq!(
        err_of(verify_approval(&install, &situation(&f, Action::Uninstall, None, COMPUTER, t0()), ApprovalPolicy::synthetic_only())),
        ApprovalError::ActionChanged
    );
    assert_eq!(
        err_of(verify_approval(&install, &situation(&f, Action::Update, Some("0.9.0"), COMPUTER, t0()), ApprovalPolicy::synthetic_only())),
        ApprovalError::ActionChanged
    );
    // nor can it be used on another computer
    assert_eq!(
        err_of(verify_approval(
            &install,
            &situation(&f, Action::Install, None, "synthetic-computer-2", t0()),
            ApprovalPolicy::synthetic_only()
        )),
        ApprovalError::WrongComputer
    );
    // nor on another platform
    let s = Situation { host_platform: other_platform(), ..situation(&f, Action::Install, None, COMPUTER, t0()) };
    assert!(matches!(err_of(verify_approval(&install, &s, ApprovalPolicy::synthetic_only())), ApprovalError::TargetChanged));
    // an update approval remembers what it replaces
    let upd = make_approval(&f, Action::Update, Some("0.9.0"), COMPUTER, t0());
    assert_eq!(
        err_of(verify_approval(&upd, &situation(&f, Action::Update, Some("0.8.0"), COMPUTER, t0()), ApprovalPolicy::synthetic_only())),
        ApprovalError::ActionChanged
    );
    assert!(verify_approval(&upd, &situation(&f, Action::Update, Some("0.9.0"), COMPUTER, t0()), ApprovalPolicy::synthetic_only()).is_ok());
}

#[test]
fn a_recipe_that_does_not_support_the_action_cannot_be_approved_for_it() {
    let mut f = base_fx();
    f.sc.reviewed.recipe.lifecycle_actions = vec!["install".to_string()];
    synthetic::rereview(&mut f.sc.reviewed);
    f.valid = f.sc.validated();
    f.plan = f.sc.plan();
    let r = approve(
        &situation(&f, Action::Uninstall, None, COMPUTER, t0()),
        None,
        Actor::synthetic("synthetic-user-1"),
        &confirm(Action::Uninstall, &f),
        ApprovalPolicy::synthetic_only(),
        600,
    );
    assert_eq!(err_of(r), ApprovalError::ActionNotSupportedByRecipe);
}

#[test]
fn every_error_has_a_distinct_stable_code() {
    let all = [
        ApprovalError::PlanNotValid("x"),
        ApprovalError::PlanBlocked(vec![]),
        ApprovalError::RecipeMismatch,
        ApprovalError::RecipeChanged,
        ApprovalError::PlanChanged,
        ApprovalError::PrerequisitesChanged,
        ApprovalError::PermissionsChanged,
        ApprovalError::TargetChanged,
        ApprovalError::WrongComputer,
        ApprovalError::ActionChanged,
        ApprovalError::ProjectUnknown,
        ApprovalError::ProjectMismatch,
        ApprovalError::ActorNotAllowed,
        ApprovalError::ConfirmationMismatch,
        ApprovalError::BadLifetime,
        ApprovalError::NotYetValid,
        ApprovalError::Expired,
        ApprovalError::BindingMismatch,
        ApprovalError::UnsupportedVersion,
        ApprovalError::ActionNotSupportedByRecipe,
    ];
    let codes: std::collections::BTreeSet<&str> = all.iter().map(|e| e.code()).collect();
    assert_eq!(codes.len(), all.len());
}
