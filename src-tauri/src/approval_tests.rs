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

fn project_fx() -> (Fx, PlanRequest) {
    let mut f = base_fx();
    f.sc.reviewed.recipe.target.scope = "project".to_string();
    synthetic::rereview(&mut f.sc.reviewed);
    f.valid = f.sc.validated();
    let req = PlanRequest {
        scope: "project".to_string(),
        project_alias: Some("proje-a".to_string()),
        ..synthetic::request("synthetic-tool", this_platform())
    };
    f.plan = plan::build_plan_with(&f.sc.entry, &req, &synthetic::host(this_platform()), &[], &[&f.sc.reviewed], &f.sc.refs).unwrap();
    assert_eq!(f.plan.recipe.state, "validated");
    (f, req)
}

#[test]
fn a_project_approval_needs_a_registered_project_and_stops_being_valid_when_it_disappears() {
    let (f, _) = project_fx();
    let make = |alias: Option<&str>| {
        approve(
            &situation(&f, Action::Install, None, COMPUTER, t0()),
            alias,
            Actor::synthetic("synthetic-user-1"),
            &confirm(Action::Install, &f),
            ApprovalPolicy::synthetic_only(),
            600,
        )
    };
    // well-formed but nonexistent, a path-like string, and no alias at all
    assert_eq!(err_of(make(Some("proje-yok"))), ApprovalError::ProjectUnknown);
    assert_eq!(err_of(make(Some("C:\\Users\\x"))), ApprovalError::ProjectUnknown);
    assert_eq!(err_of(make(None)), ApprovalError::ProjectUnknown);
    let a = make(Some("proje-a")).unwrap();
    assert!(verify(&f, &a, t0()).is_ok());
    // the project is removed after the approval: it no longer binds
    crate::projects::remove_reference(&f.conn, "proje-a").unwrap();
    assert_eq!(err_of(verify(&f, &a, t0())), ApprovalError::ProjectUnknown);
    // a user-scope recipe cannot be approved for a project
    let user = base_fx();
    let r = approve(
        &situation(&user, Action::Install, None, COMPUTER, t0()),
        Some("proje-a"),
        Actor::synthetic("synthetic-user-1"),
        &confirm(Action::Install, &user),
        ApprovalPolicy::synthetic_only(),
        600,
    );
    assert_eq!(err_of(r), ApprovalError::TargetChanged);
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
