//! Phase 3D-2: the SYNTHETIC supply for the real installation workflow (tests and the `synthetic-pilot` build only).
//!
//! Three made-up tools stand in for the three situations the interface must tell apart. They have no upstream, no real repository
//! and no real behaviour; the "program" is a few bytes behind a native-looking header and is never executed:
//!
//! * `synthetic-hello` -- everything in order: a reviewed recipe, an authenticated reference, an accepted licence. It can be previewed,
//!   confirmed, installed, verified and removed.
//! * `synthetic-unverified-tool` -- a reviewed recipe whose artifact has no authenticated reference: "verification incomplete".
//! * `synthetic-license-pending` -- the only open point is that its licence was never accepted by a review.
//!
//! Nothing here is reachable in a release build (the module is not compiled), and every recipe is `synthetic`, so the adapter's own
//! synthetic-only gate would refuse them anywhere else.
#![allow(dead_code)]

use super::{ArtifactProvider, NoticeBundle, Sources};
use crate::{
    artifact_provider::{RetainedEntry, RetainedFileProvider},
    managed_fs::{Dir, FsError},
    plan::HostFacts,
};
use crate::{
    catalog,
    error::AppResult,
    recipe::{self, RecipeRegistry},
    synthetic,
};
use rusqlite::Connection;
use serde_json::json;
use std::collections::HashMap;

pub const HELLO: &str = "synthetic-hello";
/// A synthetic tool whose bytes come from the RETAINED-FILE provider (a fixed folder), like the first real pilot will.
pub const RETAINED: &str = "synthetic-retained";
pub const RETAINED_FILE: &str = "synthetic-retained.exe";
pub const RETAINED_FOLDER: &str = "pilot-input";
pub const UNVERIFIED: &str = "synthetic-unverified-tool";
pub const LICENSE_PENDING: &str = "synthetic-license-pending";
pub const VERSION: &str = "1.0.0";
pub const PLATFORMS: [&str; 2] = ["windows-x64", "macos-arm64"];

pub fn payload(tool: &str) -> Vec<u8> {
    format!("SYNTHETIC TEST FILE for {tool} {VERSION}. This is not a program and is never executed.").into_bytes()
}

/// In-memory synthetic bytes, keyed by tool, version and platform.
pub struct Bytes {
    files: HashMap<(String, String, String), Vec<u8>>,
}

impl ArtifactProvider for Bytes {
    fn bytes(&self, tool: &str, version: &str, platform: &str) -> Result<Vec<u8>, &'static str> {
        self.files.get(&(tool.to_string(), version.to_string(), platform.to_string())).cloned().ok_or("artifact_not_available")
    }
}

pub fn notices() -> NoticeBundle {
    NoticeBundle {
        tool_id: HELLO.to_string(),
        version: VERSION.to_string(),
        title: "SENTETİK bildirim paketi (deneme)".to_string(),
        components: vec![
            "synthetic-hello - MIT (sentetik)".to_string(),
            "synthetic-parser - BSD-2-Clause (sentetik)".to_string(),
            "synthetic-runtime - GPL-3.0-or-later WITH GCC-exception (sentetik)".to_string(),
        ],
        unresolved: vec!["Bu bir deneme paketidir; hiçbir gerçek hukuki durumu temsil etmez.".to_string()],
        text:
            "SYNTHETIC NOTICE BUNDLE\n\nThis text stands in for third-party notices in tests. It is not a licence and not legal advice.\n\n\
               synthetic-hello: MIT-style permission notice (synthetic)\nsynthetic-parser: BSD-2-Clause-style notice (synthetic)\n\
               synthetic-runtime: runtime-exception notice (synthetic)\n"
                .to_string(),
    }
}

/// The complete synthetic supply: recipes and references for both platforms, the bytes, and the notice bundle.
pub fn sources() -> Sources {
    build(&[HELLO, UNVERIFIED, LICENSE_PENDING], None)
}

/// The supply of the `synthetic-pilot` application build: the three tools above plus one whose file is read from the fixed input folder.
pub fn sources_for_app(input_folder: std::path::PathBuf) -> Sources {
    build(&[HELLO, UNVERIFIED, LICENSE_PENDING, RETAINED], Some(input_folder))
}

/// Puts the retained-file fixture where a person would put the retained file: the fixed input folder inside the application's own data
/// directory. Idempotent; never overwrites a file that is already there (so tests can swap or remove it and see the refusals).
pub fn place_retained_fixture(data_dir: &std::path::Path) {
    let platform = HostFacts::detect().platform;
    if !PLATFORMS.contains(&platform.as_str()) {
        return;
    }
    let bytes = synthetic::artifact_bytes(&platform, &payload(RETAINED));
    let Ok(top) = Dir::open_anchor(data_dir) else { return };
    let dir = match top.child(RETAINED_FOLDER) {
        Ok(d) => d,
        Err(FsError::NotFound) => match top.create_child(RETAINED_FOLDER) {
            Ok(d) => d,
            Err(_) => return,
        },
        Err(_) => return,
    };
    if matches!(dir.stat(RETAINED_FILE), Ok(Some(_))) {
        return;
    }
    if let Ok(mut f) = dir.create_file(RETAINED_FILE) {
        let _ = f.write_all(&bytes);
        let _ = f.sync_all();
    }
}

struct Both {
    memory: Bytes,
    retained: RetainedFileProvider,
}

impl ArtifactProvider for Both {
    fn bytes(&self, tool: &str, version: &str, platform: &str) -> Result<Vec<u8>, &'static str> {
        if tool == RETAINED {
            self.retained.bytes(tool, version, platform)
        } else {
            self.memory.bytes(tool, version, platform)
        }
    }
}

fn build(tools: &[&str], input_folder: Option<std::path::PathBuf>) -> Sources {
    let mut retained_entries = Vec::new();
    let mut recipes = Vec::new();
    let mut authenticated = Vec::new();
    let mut files = HashMap::new();
    for platform in PLATFORMS {
        for &tool in tools {
            let bytes = synthetic::artifact_bytes(platform, &payload(tool));
            if tool == RETAINED {
                retained_entries.push(RetainedEntry {
                    tool_id: tool.to_string(),
                    version: VERSION.to_string(),
                    platform_target: platform.to_string(),
                    file_name: RETAINED_FILE.to_string(),
                    size_bytes: bytes.len() as u64,
                });
            }
            let mut rr = synthetic::reviewed_recipe(tool, VERSION, platform, &bytes);
            if tool != UNVERIFIED {
                authenticated.push(synthetic::reference_for(tool, VERSION, platform, &bytes));
            }
            if tool == RETAINED {
                // like the first real pilot: the bytes come from ONE retained local file, and the (reviewed) recipe says so
                rr.recipe.artifact.origin = Some(crate::recipe::ORIGIN_LOCAL_RETAINED_FILE.to_string());
                synthetic::rereview(&mut rr);
            }
            if tool == LICENSE_PENDING {
                rr.recipe.license.state = "unknown".to_string();
                synthetic::rereview(&mut rr);
            }
            if tool != RETAINED {
                files.insert((tool.to_string(), VERSION.to_string(), platform.to_string()), bytes);
            }
            recipes.push(rr);
        }
    }
    Sources {
        registry: RecipeRegistry::synthetic(recipes).expect("the synthetic recipes are synthetic"),
        extra_authenticated: authenticated,
        artifacts: match input_folder {
            Some(folder) => Box::new(Both { memory: Bytes { files }, retained: RetainedFileProvider::new(folder, retained_entries) }),
            None => Box::new(Bytes { files }),
        },
        notices: vec![notice_for_license_pending(), notices()],
        // (the retained tool carries no notice bundle: its preview shows none)
        evidence: super::Evidence::none(),
        pilot_gate: vec![],
    }
}

fn notice_for_license_pending() -> NoticeBundle {
    NoticeBundle {
        tool_id: LICENSE_PENDING.to_string(),
        version: VERSION.to_string(),
        title: "SENTETİK bildirim paketi (lisans bekliyor)".to_string(),
        components: vec!["synthetic-license-pending - lisans bilinmiyor (sentetik)".to_string()],
        unresolved: vec!["Lisans henüz bir incelemeden geçmedi (sentetik).".to_string()],
        text: "SYNTHETIC NOTICE BUNDLE (licence pending)\n".to_string(),
    }
}

/// Puts the three synthetic tools into the catalog of an ISOLATED data directory, so the ordinary catalog page lists them like any
/// other entry. Idempotent. Never used for a real catalog: only the `synthetic-pilot` build calls it.
pub fn seed_catalog(conn: &Connection) -> AppResult<()> {
    for (tool, name) in [
        (HELLO, "Sentetik Merhaba (deneme aracı)"),
        (UNVERIFIED, "Sentetik doğrulanmamış araç (deneme)"),
        (LICENSE_PENDING, "Sentetik lisansı bekleyen araç (deneme)"),
        (RETAINED, "Sentetik saklanan-dosya aracı (deneme)"),
    ] {
        seed_one(conn, tool, name)?;
    }
    Ok(())
}

/// One catalog row for a tool id (test-only helper for the real-recipe transition tests uses it with the id `jq`: a TEST row, never seeded).
pub fn seed_one(conn: &Connection, tool: &str, name: &str) -> AppResult<()> {
    let now = crate::audit::now_utc();
    {
        let record = json!({
            "schemaVersion": 0,
            "synthetic": true,
            "repository": { "url": format!("https://github.com/synthetic-fixtures/{tool}"), "owner": "synthetic-fixtures", "name": tool },
            "targetEvidence": [],
            "installMethods": [],
            "license": { "state": "unknown" },
            "privacy": { "telemetry": "none-declared", "mayTransmitProjectContent": false },
            "note": "SENTETİK deneme kaydı: gerçek bir araç değildir, hiçbir adres kullanılmaz.",
        });
        let fingerprint = catalog::capability_fingerprint(&record, "executable_tool", "manual-only");
        conn.execute(
            "INSERT OR IGNORE INTO catalog_entries
             (entry_id, kind, origin, display_name, purpose, source_key, repo_url, source_path, plugin_identity, review_state, metadata_state,
              execution_policy, license_state, license_spdx, enabled, revision, capability_fingerprint, record_json, added_at, updated_at)
             VALUES (?1, 'executable_tool', 'user', ?2, ?3, ?4, ?5, NULL, NULL, 'unreviewed', 'not-fetched', 'manual-only', 'unknown', NULL, 1,
                     'synthetic-pilot', ?6, ?7, ?8, ?8)",
            rusqlite::params![
                tool,
                name,
                "SENTETİK deneme aracı: gerçek bir program değildir; yalnızca kurulum akışını denemek içindir.",
                format!("github.com/synthetic-fixtures/{tool}"),
                format!("https://github.com/synthetic-fixtures/{tool}"),
                fingerprint,
                record.to_string(),
                now
            ],
        )?;
    }
    Ok(())
}

/// A recipe the supply knows, for tests that want to look at it.
pub fn validated(sources: &Sources, conn: &Connection, tool: &str, platform: &str) -> recipe::ValidatedRecipe {
    let refs = sources.references(conn).expect("references");
    let found = sources.registry.find(tool, platform, super::SURFACE, super::SCOPE);
    recipe::validate(found[0], &refs).expect("valid")
}
