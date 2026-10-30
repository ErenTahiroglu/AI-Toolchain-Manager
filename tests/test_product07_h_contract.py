"""PRODUCT-07 H (Issue #29) contract, no running app needed.

Policy H adds scoped MCP-configuration readings BESIDE the unchanged presence verdict:

* the Rust enums and the TypeScript closed lists describe the same vocabulary (a drift here would make the UI distrust or mis-word the backend),
* the presence selection (`classify_request`) is untouched and never reads configuration, scope, method or declarations,
* no migration, IPC command, Tauri permission, dependency or write was added,
* the frozen wire-contract document exists and states the compatibility claim honestly (`origin` is NOT byte-identical; one additive field).
"""
import json
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


def read(rel):
    return (ROOT / rel).read_text(encoding="utf-8")


def snake(name):
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def kebab(name):
    return snake(name).replace("_", "-")


def rust_enum(source, name, case=snake):
    m = re.search(r"pub enum %s \{(.*?)\n\}" % name, source, re.S)
    assert m, name
    body = re.sub(r"///[^\n]*", "", m.group(1))
    return [case(v) for v in re.findall(r"^\s*([A-Z][A-Za-z]*)\s*,", body, re.M)]


def ts_list(source, name):
    m = re.search(r"export const %s = \[(.*?)\] as const" % name, source, re.S)
    assert m, name
    return re.findall(r'"([^"]+)"', m.group(1))


class VocabularyAgrees(unittest.TestCase):
    def setUp(self):
        self.rust = read("src-tauri/src/discovery/declarations.rs")
        self.store = read("src-tauri/src/discovery/store.rs")
        self.ts = read("src/scopedDeclarations.ts")

    def test_scopes_configurations_freshness_coverage_and_detector_kinds_match_the_frontend(self):
        self.assertEqual(rust_enum(self.rust, "DeclarationScope"), ts_list(self.ts, "DECLARATION_SCOPES"))
        self.assertEqual(rust_enum(self.rust, "DeclarationConfiguration"), ts_list(self.ts, "DECLARATION_CONFIGURATIONS"))
        self.assertEqual(rust_enum(self.rust, "DeclarationFreshness"), ts_list(self.ts, "DECLARATION_FRESHNESS"))
        self.assertEqual(rust_enum(self.rust, "ScopeCoverage"), ts_list(self.ts, "SCOPE_COVERAGE"))
        self.assertEqual(rust_enum(self.rust, "DetectorKind"), ts_list(self.ts, "DETECTOR_KINDS"))
        # the fixed display order is the order of the Rust enum and of SCOPES
        self.assertEqual(ts_list(self.ts, "DECLARATION_SCOPES"), ["project", "local", "global"])
        self.assertRegex(self.rust, r"const SCOPES: \[DeclarationScope; 3\] = \[DeclarationScope::Project, DeclarationScope::Local, DeclarationScope::Global\]")

    def test_origin_methods_are_the_six_detection_techniques_and_match_the_frontend_and_the_discovery_vocabulary(self):
        rust = rust_enum(self.store, "OriginMethod", kebab)
        self.assertEqual(rust, ts_list(read("src/origin.ts"), "ORIGIN_METHODS"))
        self.assertEqual(len(rust), 6)
        types = read("src-tauri/src/discovery/types.rs")
        known = set(re.findall(r'Method::\w+ => "([a-z-]+)"', types))
        self.assertTrue(set(rust) <= known, set(rust) - known)
        # the pseudo-methods are NOT origin methods
        for pseudo in ("no-detector", "not-documented", "unsupported-platform"):
            self.assertNotIn(pseudo, rust)
            self.assertIn(pseudo, known)

    def test_the_wire_names_are_camel_case_and_the_enum_values_snake_or_kebab_case(self):
        self.assertIn('#[serde(rename_all = "snake_case")]\npub enum DetectorKind', self.rust)
        self.assertIn('#[serde(rename_all = "kebab-case")]\npub enum OriginMethod', self.store)
        for struct in ("DeclarationReading", "DeclarationCoverage", "DeclarationDetector"):
            self.assertRegex(self.rust, r'#\[serde\(rename_all = "camelCase"\)\]\npub struct %s' % struct)


class LegacySelectionIsUntouched(unittest.TestCase):
    def test_the_presence_ranking_expression_and_the_function_do_not_read_configuration_scope_method_or_declarations(self):
        store = read("src-tauri/src/discovery/store.rs")
        start = store.index("pub fn classify_request")
        body = store[start : store.index("fn requests(", start)]
        self.assertIn("max_by_key(|o| (rank(&o.presence), o.id))", body)
        for forbidden in ("configuration", ".scope", "declaration", "method"):
            self.assertNotIn(forbidden, body)

    def test_the_legacy_members_of_the_request_and_the_four_origin_members_are_still_there(self):
        store = read("src-tauri/src/discovery/store.rs")
        origin = re.search(r"pub struct EvidenceOrigin \{(.*?)\n\}", store, re.S).group(1)
        for member in ("project_relation", "surface_relation", "observed_at", "age_hours"):
            self.assertIn("pub %s:" % member, origin)
        self.assertIn("pub method: Option<OriginMethod>", origin)
        request = re.search(r"pub struct RequestVsObservation \{(.*?)\n\}", store, re.S).group(1)
        for member in ("status", "note", "basis", "origin", "declaration_detector", "declarations"):
            self.assertIn("pub %s:" % member, request)

    def test_the_declaration_code_is_pure_and_the_selection_does_not_call_it(self):
        decl = read("src-tauri/src/discovery/declarations.rs")
        for forbidden in ("std::fs", "std::process", "std::net", "rusqlite", "Connection", "execute(", "Utc::now", "SystemTime", "reqwest"):
            self.assertNotIn(forbidden, decl)
        store = read("src-tauri/src/discovery/store.rs")
        start = store.index("pub fn classify_request")
        self.assertNotIn("declarations::", store[start : store.index("fn requests(", start)])

    def test_only_the_claude_mcp_detector_may_populate_readings(self):
        decl = read("src-tauri/src/discovery/declarations.rs")
        self.assertIn('const MCP_METHOD: &str = "claude-mcp-config";', decl)
        self.assertEqual(len(re.findall(r"o\.method == MCP_METHOD", decl)), 1)
        detect = read("src-tauri/src/discovery/detect.rs")
        self.assertRegex(detect, r'pub fn has_declaration_detector\(entry_id: &str, surface: &str\) -> bool \{\s*entry_id == MCP_ENTRY && surface == SURFACE_CLAUDE\s*\}')


class NothingNewIsGranted(unittest.TestCase):
    def test_no_migration_was_added(self):
        db = read("src-tauri/src/db.rs")
        block = db[db.index("const MIGRATIONS: &[Migration] = &[") :]
        block = block[: block.index("];")]
        self.assertEqual(block.count("Migration::"), 10)
        self.assertIn("0010_codex_cli_surface.sql", block)
        names = sorted(p.name for p in (ROOT / "src-tauri" / "migrations").glob("*.sql"))
        self.assertEqual(names[-1], "0010_codex_cli_surface.sql")
        self.assertEqual(len(names), 10)

    def test_no_ipc_command_or_permission_or_dependency_was_added(self):
        for rel in ("src-tauri/src/commands.rs", "src-tauri/src/lib.rs"):
            self.assertNotRegex(read(rel), r"(?i)fn\s+\w*(declar|scoped|origin_method)\w*")
        perms = json.loads(read("src-tauri/capabilities/default.json"))["permissions"]
        # CONFIG-02C (Issue #40, owner-authorized): exactly one command, `read_project_mcp_config`, and its single permission are allowed; tests/test_config02c_contract.py pins that they are the only ones.
        for p in [x for x in perms if x != "allow-read-project-mcp-config"]:
            self.assertIsNone(re.search(r"fs|shell|http|opener|dialog|clipboard|process|os:|path|window|declar|mcp", p, re.I), p)
        pkg = json.loads(read("package.json"))
        self.assertFalse([n for n in {**pkg.get("dependencies", {}), **pkg.get("devDependencies", {})} if re.search(r"toml|yaml|jsonc|json5", n)])

    def test_the_new_typescript_modules_do_no_io(self):
        forbidden = r"invoke\(|@tauri-apps|\bapi\.|fetch\(|XMLHttpRequest|WebSocket|navigator\.clipboard|localStorage|sessionStorage|indexedDB|setTimeout|setInterval|createObjectURL|FileReader|window\.open|child_process|require\("
        for rel in ("src/scopedDeclarations.ts", "src/components/ScopedConfigNote.tsx", "src/origin.ts"):
            self.assertIsNone(re.search(forbidden, read(rel)), rel)

    def test_the_review_queue_reasons_and_statuses_are_unchanged(self):
        review = read("src/review.ts")
        statuses = re.search(r"export const EVIDENCE_REVIEW_STATUSES: readonly RequestStatus\[\] = \[(.*?)\];", review, re.S).group(1)
        self.assertEqual(re.findall(r'"([a-z_]+)"', statuses), ["absent_current", "unknown_current", "previously_present", "previously_absent", "previously_unknown", "platform_mismatch", "not_scanned"])
        # the derivation creates no reason from the new members: the only place they are mentioned is the display-only carry-through
        body = review.split("export function deriveReview")[1]
        self.assertEqual(body.count("item.reasons.push("), 4)  # material_change, entry_removed, reminder_due, evidence - exactly as before
        types = read("src/types.ts")
        status = re.search(r"export type RequestStatus =(.*?);", types, re.S).group(1)
        self.assertEqual(len(re.findall(r'"([a-z_]+)"', status)), 8)  # no new (e.g. "mixed") verdict


class ContractDocument(unittest.TestCase):
    def test_the_frozen_contract_states_the_compatibility_claim_honestly_and_the_deferred_items(self):
        doc = read("docs/product-07/H_WIRE_CONTRACT.md")
        self.assertRegex(doc, r"(?i)\*\*not\*\* byte-identical")
        self.assertIn("additive field only", doc)
        self.assertIn("An empty `declarations` array never means \"not configured\"", doc)
        self.assertIn("#36", doc)
        for needle in ("not_implemented", "project_not_in_latest_scan", "previous_area_not_checked", "REAL_INSTALLATION_NO_GO", "REAL_CONFIG_WRITE_NO_GO"):
            self.assertIn(needle, doc, needle)
        self.assertNotRegex(doc, r"(?i)byte-identical `origin`")

    def test_the_documents_do_not_claim_acceptance(self):
        for name in ("H_WIRE_CONTRACT", "H_IMPLEMENTATION_AND_TESTS"):
            text = read("docs/product-07/%s.md" % name)
            self.assertNotRegex(text, r"(?i)independently accepted|production[- ]ready(?! değil| \(not)")


if __name__ == "__main__":
    unittest.main()
