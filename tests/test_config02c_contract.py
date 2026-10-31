"""CONFIG-02C (Issue #40) contract, no running app needed. R2: the owner-authorized ONE-FILE READ of a project `.mcp.json` is implemented. This file is the documentation-and-reach
guard that was written for the design phase (R1), UPDATED DELIBERATELY for the authorization -- it was not disabled:

* the three documents exist, carry the three status flags and use ONE closed status vocabulary (also in the Rust enum and the TypeScript table),
* the owner's response block is exactly the transcribed authorization record (provenance link, "not a direct owner comment", and the recorded answers -- nothing else ticked),
* the mapping table covers every FsError code of the handle-relative layer,
* the new reach is EXACTLY one command and one permission, registered in the four places; nothing else about the application's reach changed (no plugin, dependency, migration),
* the production files that name `.mcp.json` are exactly the known set, the request has no free fields and no Debug, the response has exactly the contract's fields,
* the read-only modules contain none of the forbidden calls and nothing in production refers to the 02B prototype,
* the evidence rules are in place (manifest <-> sources, the gate, the CI step on both operating systems).

It checks wording, structure and reach only. It cannot show any behaviour of the reader: the Rust and vitest cases do, through the CONFIG02C_RESULT lines, per operating system.
"""
import json
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
SRC = ROOT / "src-tauri" / "src"
DOCS = ROOT / "docs" / "config-02c"

CONTRACT = "READ_ONLY_CONTRACT.md"
PLAN = "THREAT_AND_PRIVACY_TEST_PLAN.md"
REQUEST = "REAL_READ_AUTHORIZATION_REQUEST.md"

# The closed list the backend may emit. `not_requested` is a UI-only state and is deliberately not in it.
STATUSES = [
    "root_not_found", "absent", "present_summarized", "present_metadata_only", "malformed", "denied",
    "unsafe_path", "unsupported_fs", "too_large", "changed_during_read", "io_error",
]
REASONS = [
    "duplicate_key", "byte_order_mark", "root_not_object", "servers_not_object", "too_deep", "too_many_servers", "invalid_utf8", "invalid_json", "lone_surrogate",
    "empty_file", "access_denied", "locked", "link", "not_a_directory", "not_regular_file", "hard_linked", "case_alias", "invalid_root_text", "root_too_broad",
]
RESPONSE_FIELDS = [
    "schema", "status", "reason", "alias", "file", "scope", "surface", "max_bytes", "size_bytes", "sha256", "server_count", "reviewed_declarations", "not_claims",
]
FLAGS = ("READ_IMPLEMENTATION_AUTHORIZED", "REAL_USER_PROJECT_NOT_READ", "FEATURE_NOT_INDEPENDENTLY_ACCEPTED")
RECORD = "issuecomment-6519548226"
COMMAND = "read_project_mcp_config"
PERMISSION = "allow-read-project-mcp-config"


def read(rel):
    return (ROOT / rel).read_text(encoding="utf-8")


def doc(name):
    return (DOCS / name).read_text(encoding="utf-8")


def snake(camel):
    return re.sub(r"(?<!^)(?=[A-Z])", "_", camel).lower()


def rust_enum(source, name):
    body = re.search(r"pub enum %s \{(.*?)\n\}" % name, source, re.S).group(1)
    return [snake(v) for v in re.findall(r"^\s{4}([A-Z][A-Za-z0-9]*),", body, re.M)]


def production(path):
    """The source before the first `#[cfg(test)]`."""
    return path.read_text(encoding="utf-8").split("#[cfg(test)]")[0]


class Documents(unittest.TestCase):
    def test_all_three_state_the_three_flags_and_claim_no_broad_grant_or_readiness(self):
        for name in (CONTRACT, PLAN, REQUEST):
            text = doc(name)
            for flag in FLAGS:
                self.assertIn(flag, text, f"{name}: {flag}")
            self.assertNotIn("READ_GRANTED", text, name)  # no blanket grant of reading ("READ_NOT_GRANTED" is a different word)
            self.assertNotIn("CONFIG_02C_IMPLEMENTED", text, name)  # readiness is reported in the Issue after exact-SHA CI, never asserted in these files
            self.assertNotIn("READY_FOR_INDEPENDENT_REVIEW", text, name)
        contract = doc(CONTRACT)
        self.assertIn("CONFIG_02C_DESIGN_ACCEPTED_WITH_REVISIONS", contract)
        self.assertIn("issuecomment-6518093495", contract)
        self.assertIn(RECORD, contract)
        self.assertIn("not** a comment the owner wrote directly", contract)

    def test_the_three_states_are_kept_apart_in_the_request(self):
        text = doc(REQUEST)
        for needle in (
            "The owner approved building it", "No end user has consented to any read yet", "The implementation is not accepted", "not** a comment the owner wrote directly",
            "not** an owner signature", "not** an independent acceptance", "each read asks again", "is **not** permission for the author, for CI or for a reviewer to open any personal or project file",
        ):
            self.assertIn(needle, text, needle)

    def test_the_owner_response_block_is_exactly_the_transcribed_record(self):
        text = doc(REQUEST)
        block = text[text.index("## 8. Owner response"):]
        self.assertIn(RECORD, block)
        self.assertIn("transcribed by the author", block)
        self.assertIn("carries **no owner signature**", block)
        self.assertIn("not** a direct GitHub comment by the owner", block)
        fenced = re.search(r"```\n(.*?)\n```", block, re.S).group(1)
        self.assertEqual(len(re.findall(r"\[x\]", fenced)), 12, "D1, D2, D3, D4a, D4b, D5, D5a, D6, D7, D8, D9 and the provenance link: nothing else is ticked")
        self.assertRegex(fenced, r"\[x\] comment link https://github\.com/ErenTahiroglu/AI-Toolchain-Manager/issues/40#issuecomment-6519548226")
        # the answers NOT given stay unticked: D2 B, D4a NO, D4b YES, D5 NO, D5a YES, D6 fsx, D3 NO, D1 NO, D7 NO
        for line in fenced.splitlines():
            if line.startswith("D1 "):
                self.assertRegex(line, r"D1\s+\[x\] YES\s+\[ \] NO\s+D2\s+\[x\] A\s+\[ \] B\s+\[ \] NO read at all")
            if line.startswith("D3 "):
                self.assertRegex(line, r"D3\s+\[x\] YES\s+\[ \] NO\s+D4a \[x\] YES \[ \] NO\s+D4b \[x\] NO \(default\)\s+\[ \] YES \(separate phase\)")
            if line.startswith("D5 "):
                self.assertRegex(line, r"D5\s+\[x\] YES\s+\[ \] NO\s+D5a \[x\] NO \(default\)\s+\[ \] YES")
            if line.startswith("D6 "):
                self.assertRegex(line, r"D6\s+\[x\] managed_fs read half\s+\[ \] extend fsx\s+D7\s+\[x\] YES\s+\[ \] NO")
            if line.startswith("D8 "):
                self.assertRegex(line, r"D8\s+\[x\] confirmed in full\s+D9\s+\[x\] confirmed")
        self.assertIn("Status: READ_IMPLEMENTATION_AUTHORIZED | REAL_USER_PROJECT_NOT_READ | FEATURE_NOT_INDEPENDENTLY_ACCEPTED", fenced)
        # no checked box anywhere else in the three documents
        for name in (CONTRACT, PLAN):
            self.assertIsNone(re.search(r"\[[xX]\]", doc(name)), name)
        self.assertEqual(len(re.findall(r"\[[xX]\]", text[: text.index("## 8. Owner response")])), 0)

    def test_one_closed_status_vocabulary_everywhere(self):
        contract = doc(CONTRACT)
        start = contract.index("### 3.3")
        end = contract.index("The mapping from the existing `FsError`")
        rows = re.findall(r"^\| `([a-z_]+)` \|", contract[start:end], re.M)
        self.assertEqual(rows, STATUSES)
        self.assertNotIn("`not_requested`", contract[start:end].split("| Status |")[1], "not_requested is UI-only and must not be a backend row")
        request = doc(REQUEST)
        for s in STATUSES:
            self.assertIn(f"`{s}`", request, s)
        self.assertIn("closed list of 11", request)
        plan = doc(PLAN)
        for token in re.findall(r"`(root_not_found|absent|present_\w+|malformed|denied|unsafe_path|unsupported_fs|too_large|changed_during_read|io_error)`", plan):
            self.assertIn(token, STATUSES, token)
        # the Rust enum and the TypeScript table are the same list
        self.assertEqual(rust_enum(read("src-tauri/src/mcp_read.rs"), "Status"), STATUSES)
        ts = read("src/fileSnapshot.ts")
        self.assertEqual(re.findall(r'"([a-z_]+)"', re.search(r"export const STATUSES = \[(.*?)\] as const;", ts, re.S).group(1)), STATUSES)
        # the reasons: Rust enum, TypeScript table and the request's reason table
        self.assertEqual(rust_enum(read("src-tauri/src/mcp_read.rs"), "Reason"), REASONS)
        for r in REASONS:
            self.assertIn(f'  {r}: ', ts.replace("\n", "\n  ") if False else ts, r)
            self.assertIn(f"| `{r}` |", request, r)

    def test_every_fs_error_code_has_a_row_in_the_mapping(self):
        codes = re.findall(r'FsError::\w+ => "([a-z_]+)"', read("src-tauri/src/managed_fs.rs"))
        self.assertGreaterEqual(len(codes), 15)
        contract = doc(CONTRACT)
        for code in codes:
            self.assertTrue(re.search(r"^\| `%s` \|" % re.escape(code), contract, re.M), f"FsError code {code!r} has no row in the contract's mapping table")

    def test_the_result_lines_and_the_gate_are_documented(self):
        plan = doc(PLAN)
        self.assertIn("CONFIG02C_RESULT|<id>|<os>|<status>|<reason>", plan)
        self.assertIn("CONFIG02C_RESULTS", plan)
        for word in ("PASS", "FAIL", "SKIPPED", "BLOCKED", "NOT RUN"):
            self.assertIn(word, plan)
        self.assertIn("a `SKIPPED` line is never counted as PASS", plan)
        self.assertIn("An id with no line for an operating system is `NOT RUN`", plan)
        self.assertIn("jsdom", plan)
        self.assertIn("**no real project file was read**", plan)
        self.assertNotIn("CONFIG02C_SKIP ", plan)

    def test_the_turkish_gate_text_names_the_file_the_limit_and_the_no_button(self):
        request = doc(REQUEST)
        for needle in (".mcp.json", "1.048.576", "Hayır, metni yapıştırarak devam edeceğim", "Temizle", "kaydetmez", "bilerek ağ bağlantısı başlatmaz", "#25", "kanıtlanmamıştır"):
            self.assertIn(needle, request, needle)
        self.assertNotIn("İnternete bağlanılmaz", request)  # the absolute promise exceeds the evidence (Issue #25 is open)


class ReviewerDecisions(unittest.TestCase):
    """Independent decision CONFIG_02C_DESIGN_ACCEPTED_WITH_REVISIONS (Issue #40, O1-O5) must stay visible in the wording, not only in the Issue."""

    def test_o1_two_readers_two_labels_never_merged(self):
        for name in (CONTRACT, REQUEST):
            text = doc(name)
            self.assertIn("önceki envanter gözlemi (farklı kapsam/yöntem)", text, name)
            self.assertIn("dosya anlık görüntüsü (katı JSON sözleşmesi)", text, name)
        contract = doc(CONTRACT)
        self.assertIn("never merged, averaged, ranked or used to override each other", contract)
        self.assertIn("biri diğerini doğrulamaz veya çürütmez", contract)
        self.assertIn("is persisted", contract)
        self.assertIn("is **not changed by this work**", contract)
        ts = read("src/fileSnapshot.ts")
        self.assertIn("önceki envanter gözlemi (farklı kapsam/yöntem)", ts)
        self.assertIn("dosya anlık görüntüsü (katı JSON sözleşmesi)", ts)

    def test_o2_read_subset_only_and_14_stays_open(self):
        contract = doc(CONTRACT)
        self.assertIn("read-only subset of the existing handle-relative `managed_fs`", contract)
        self.assertIn("No production import of the `#[cfg(test)]` 02B writer", contract)
        self.assertIn("**stays open**", contract)
        self.assertIn("does **not** resolve it", contract)
        self.assertIn("independently scoped", contract)
        self.assertIn("**Independence (R1, decision O2).**", doc(PLAN))

    def test_o3_typed_path_is_not_claimed_hidden(self):
        contract, request = doc(CONTRACT), doc(REQUEST)
        self.assertIn("not a secure erase", contract.lower())
        self.assertIn("#25", contract)
        self.assertIn("Not a secure erase and not hidden from the renderer", request)
        self.assertIn("garanti edilmez", request)
        self.assertIn("any **write** needs the stronger chooser", contract)

    def test_o4_names_stay_closed_and_declaration_is_syntax_only(self):
        contract, request = doc(CONTRACT), doc(REQUEST)
        self.assertIn("D4b is NO / deferred", contract)
        self.assertIn("**NO / deferred**", request)
        self.assertIn("source-backed syntax reference", contract)
        self.assertIn("not a statement that the declaration is a verified working Exa server", contract)
        self.assertIn("çalıştığı doğrulanmadı", request)

    def test_disclosure_names_what_is_inspected_beyond_the_one_file(self):
        contract, request = doc(CONTRACT), doc(REQUEST)
        self.assertIn("ancestor *metadata* is touched", contract)
        self.assertIn("Directory entry names", contract)
        for needle in ("üst klasörlerin yalnızca türü ve bağlantı olup olmadığı denetlenir", "harf büyüklüğü farkıyla ikinci bir kopyası", "gösterilmez ve saklanmaz"):
            self.assertIn(needle, request, needle)
            self.assertIn(needle, read("src/fileSnapshot.ts"), needle)

    def test_withdrawn_overclaims_do_not_come_back(self):
        # The plan may QUOTE these (to say they are wrong); the two documents that make promises, and the user-visible text, may not say them.
        for name in (CONTRACT, REQUEST):
            text = doc(name)
            for phrase in ("nothing outside the file", "okuma biter bitmez silinir", "Başka dosya, alt klasör veya üst klasör okunmaz", "erased when the call"):
                self.assertNotIn(phrase, text, f"{name}: {phrase}")
        ts = read("src/fileSnapshot.ts")
        for phrase in ("okuma biter bitmez silinir", "Başka dosya, alt klasör veya üst klasör okunmaz", "güvenle silinir", "doğrulandı."):
            self.assertNotIn(phrase, ts, phrase)

    def test_digest_absent_and_strictness_are_not_overread(self):
        contract, request = doc(CONTRACT), doc(REQUEST)
        self.assertIn("the bytes observed at that moment", contract)
        self.assertIn("not who wrote them, not that they are trustworthy", contract)
        self.assertIn("kaynağını, güvenilirliğini veya dosyanın şimdiki durumunu göstermez", request)
        self.assertIn("no entry named exactly `.mcp.json` was observed in that directory", contract)
        self.assertIn("local parsing policy", contract)
        self.assertIn("yerel bir politikadır", request)

    def test_reviewer_recommendations_are_not_owner_approval(self):
        request = doc(REQUEST)
        self.assertIn("NOT owner approval", request)
        self.assertIn("This table is the reviewer's earlier technical recommendation and is not the owner's approval", request)


class Reach(unittest.TestCase):
    def test_only_the_known_production_files_name_the_target(self):
        found = set()
        for path in SRC.rglob("*.rs"):
            if "tests" in path.name:
                continue
            if ".mcp.json" in path.read_text(encoding="utf-8"):
                found.add(path.relative_to(SRC).as_posix())
        # config_txn.rs and managed_fs_mcp.rs are compiled for tests only (checked by test_config02b_contract); config02c_support.rs too (below);
        # discovery/* is the existing scan (discovery/store.rs only names the file inside a Turkish sentence); managed_fs_read.rs and mcp_read.rs are the new reader.
        self.assertEqual(
            found,
            {
                "config_txn.rs", "managed_fs_mcp.rs", "discovery/detect.rs", "discovery/store.rs", "discovery_probe.rs", "managed_fs_read.rs", "mcp_read.rs", "config02c_support.rs",
                "commands.rs", "managed_fs.rs",  # a doc comment of the new command, and a comment on the new child module (neither contains code that opens the file)
            },
            "a new production file now names .mcp.json: that is a new reader and needs the owner decision of docs/config-02c/REAL_READ_AUTHORIZATION_REQUEST.md",
        )
        lib = read("src-tauri/src/lib.rs")
        self.assertIn("#[cfg(test)]\nmod config02c_support;", lib)
        self.assertIn("#[cfg(test)]\nmod config_txn;", lib)
        self.assertIn("\nmod mcp_read;", lib)

    def test_exactly_one_new_command_and_one_new_permission_registered_in_every_place(self):
        commands = read("src-tauri/src/commands.rs")
        self.assertEqual(commands.count("#[tauri::command]"), 37)
        self.assertEqual(len(re.findall(r"\bfn read_project_mcp_config\b", commands)), 1)
        for rel in ("src-tauri/src/lib.rs", "src-tauri/build.rs", "src-tauri/src/health.rs", "src-tauri/capabilities/default.json"):
            self.assertEqual(len(re.findall(r"read[_-]project[_-]mcp[_-]config", read(rel))), 1, rel)
        permissions = json.loads(read("src-tauri/capabilities/default.json"))["permissions"]
        self.assertEqual(len(permissions), 37)
        self.assertEqual([p for p in permissions if "mcp" in p or "config" in p], [PERMISSION])
        for p in permissions:
            self.assertIsNone(re.search(r"fs|shell|http|opener|dialog|clipboard|process|os:|path", p, re.I), p)
        # every other mention of the command name in production Rust: only the handler list, the allow-lists and the doc comments of the command
        for path in SRC.rglob("*.rs"):
            if "tests" in path.name:
                continue
            if path.name in ("commands.rs", "lib.rs", "health.rs", "mcp_read.rs"):  # registration points, and the typed module whose header names its own command
                continue
            self.assertNotIn("read_project_mcp", path.read_text(encoding="utf-8"), str(path))
        # the command body is thin: authorization, then one read through the typed module; it opens no file itself
        body = commands[commands.index("pub fn read_project_mcp_config"):].split("\n}\n")[0]
        self.assertIn("crate::mcp_read::handle(", body)
        self.assertNotRegex(body, r"std::fs|File::|OpenOptions|read_dir|canonicalize|Command::new|std::net|diagnostics|audit|execute\(")
        # it does not call or touch the existing scan
        self.assertNotIn("discovery::", body)

    def test_no_dialog_or_fs_plugin_dependency_and_no_new_migration(self):
        forbidden = re.compile(r"plugin-dialog|plugin-fs|plugin_dialog|plugin_fs|\brfd\b", re.I)
        for rel in ("src-tauri/Cargo.toml", "package.json"):
            self.assertIsNone(forbidden.search(read(rel)), rel)
        for path in (ROOT / "src").rglob("*.ts*"):
            self.assertIsNone(forbidden.search(path.read_text(encoding="utf-8")), str(path))
        migrations = sorted(p.name for p in (ROOT / "src-tauri" / "migrations").glob("*.sql"))
        self.assertEqual((len(migrations), migrations[-1]), (10, "0010_codex_cli_surface.sql"))
        cargo = read("src-tauri/Cargo.toml")
        deps = re.search(r"\[dependencies\]\r?\n(.*?)\r?\n\r?\n", cargo, re.S).group(1)
        self.assertEqual(
            sorted(re.findall(r"^([a-z0-9_-]+) =", deps, re.M)),
            sorted(["tauri", "tauri-plugin-single-instance", "tauri-plugin-autostart", "rusqlite", "serde", "serde_json", "sha2", "uuid", "chrono", "regex"]),
        )

    def test_only_the_old_scan_and_the_new_request_take_a_directory(self):
        taking = sorted(p.relative_to(SRC).as_posix() for p in SRC.rglob("*.rs") if "tests" not in p.name and re.search(r"pub directory: String", p.read_text(encoding="utf-8")))
        self.assertEqual(taking, ["discovery/mod.rs", "mcp_read.rs"])

    def test_the_request_has_three_fields_no_unknown_fields_and_no_debug(self):
        source = read("src-tauri/src/mcp_read.rs")
        at = source.index("pub struct ReadMcpConfigRequest")
        attrs = source[:at].rsplit("\n\n", 1)[-1]
        self.assertIn("deny_unknown_fields", attrs)
        derive = [l for l in attrs.splitlines() if l.strip().startswith("#[derive")][0]
        self.assertNotIn("Debug", derive)
        fields = re.findall(r"pub (\w+): ", source[at: source.index("}", at)])
        self.assertEqual(fields, ["alias", "directory", "consent"])

    def test_the_response_has_exactly_the_contracts_fields(self):
        source = read("src-tauri/src/mcp_read.rs")
        at = source.index("pub struct ReadMcpConfigResult")
        self.assertEqual(re.findall(r"pub (\w+): ", source[at: source.index("}", at)]), RESPONSE_FIELDS)
        contract = doc(CONTRACT)
        for camel in ("schema", "status", "reason", "alias", "file", "scope", "surface", "maxBytes", "sizeBytes", "sha256", "serverCount", "reviewedDeclarations", "notClaims"):
            self.assertIn(camel, contract, camel)
        self.assertIn('NOT_CLAIMS: [&str; 6]', source)

    def test_the_read_only_modules_contain_none_of_the_forbidden_calls(self):
        tokens = [
            "std::process", "Command::new", "std::net", "reqwest", "tauri::", "invoke", "SystemTime", "chrono", "std::env", "std::fs", "canonicalize", "read_dir", "OpenOptions", "create_file",
            "create_child", "create_owner_only_child", "write_all", "remove_", "move_file", "replace_owned_file", "rename", "File::create", "set_permissions", "set_len",
            "diagnostics", "audit", "println!", "eprintln!", "dbg!", "log::",
        ]
        text = production(SRC / "managed_fs_read.rs")
        for token in tokens:
            self.assertNotIn(token, text, f"managed_fs_read.rs: {token}")
        typed = production(SRC / "mcp_read.rs")
        for token in ["std::process", "Command::new", "std::net", "reqwest", "std::fs", "OpenOptions", "write_all", "println!", "eprintln!", "dbg!", "diagnostics", "audit::", "rusqlite", "Connection", "tauri::"]:
            self.assertNotIn(token, typed, f"mcp_read.rs: {token}")
        # the only addition to the shared layer is a read of the handle's own last-write time
        fs_layer = read("src-tauri/src/managed_fs.rs")
        self.assertIn("pub fn modified(&self)", fs_layer)
        self.assertIn('#[path = "managed_fs_read.rs"]\npub(crate) mod read_mcp;', fs_layer)
        # ... and the read-only root open (R1-F1): the reader never uses the writer-capable open, the Windows mask has no write-like right
        self.assertIn("pub fn open_anchor_readonly(path: &Path)", fs_layer)
        self.assertIn("Dir::open_anchor_readonly(", text)
        self.assertNotIn("Dir::open_anchor(", text)
        windows = read("src-tauri/src/managed_fs_windows.rs")
        mask = re.search(r"const READ_ANCHOR_ACCESS: u32 = ([^;]+);", windows).group(1)
        self.assertEqual(sorted(x.strip() for x in mask.split("|")), ["FILE_READ_ATTRIBUTES", "FILE_READ_DATA", "FILE_TRAVERSE", "SYNCHRONIZE"])
        self.assertIn("open_anchor_with(path, DIR_ACCESS)", windows)  # the writer paths keep their rights

    def test_nothing_in_production_refers_to_the_02b_prototype(self):
        for path in SRC.rglob("*.rs"):
            if "tests" in path.name or path.name in ("config_txn.rs", "managed_fs_mcp.rs", "lib.rs", "managed_fs.rs"):
                continue
            self.assertIsNone(re.search(r"\bconfig_txn\b|\bmcp_file\b|managed_fs_mcp|ProjectRoot", production(path)), str(path))
        for name in ("managed_fs_read.rs", "mcp_read.rs"):
            self.assertIsNone(re.search(r"config_txn|mcp_file|managed_fs_mcp|ProjectRoot|Ledger|Approval|rollback|backup", production(SRC / name)), name)

    def test_the_panel_is_mounted_only_by_the_preview_and_only_the_api_names_the_command(self):
        mounts = [p.relative_to(ROOT).as_posix() for p in (ROOT / "src").rglob("*.tsx") if ".test." not in p.name and "<FileSnapshot" in p.read_text(encoding="utf-8")]
        self.assertEqual(mounts, ["src/components/ConfigPreview.tsx"])
        named = sorted(p.relative_to(ROOT).as_posix() for p in (ROOT / "src").rglob("*.ts*") if ".test." not in p.name and "read_project_mcp_config" in p.read_text(encoding="utf-8"))
        self.assertEqual(named, ["src/api.ts"])
        callers = sorted(p.relative_to(ROOT).as_posix() for p in (ROOT / "src").rglob("*.ts*") if ".test." not in p.name and "readProjectMcpConfig" in p.read_text(encoding="utf-8"))
        self.assertEqual(callers, ["src/api.ts", "src/components/FileSnapshot.tsx"])
        self.assertIn('surface === "claude-code-terminal"', read("src/components/ConfigPreview.tsx"))
        # the preview component itself stays free of IPC, timers and storage (the older guards pin the same)
        self.assertIsNone(re.search(r"invoke\(|@tauri-apps|\bapi\.|setTimeout|localStorage", read("src/components/ConfigPreview.tsx")))


class Evidence(unittest.TestCase):
    def manifest(self):
        return json.loads(read("tests/config02c_expected_results.json"))

    def test_every_manifest_id_has_a_case_and_every_case_is_in_the_manifest(self):
        manifest = self.manifest()["ids"]
        rust = set(re.findall(r'\bcase\("([A-Za-z0-9]+)"', read("src-tauri/src/mcp_read_tests.rs") + read("src-tauri/src/managed_fs_read_tests.rs")))
        ui = set(re.findall(r'caseIt\("([A-Za-z0-9]+)"', read("src/FileSnapshot.test.tsx")))
        self.assertEqual(rust, {k for k, v in manifest.items() if v["suite"] == "rust"})
        self.assertEqual(ui, {k for k, v in manifest.items() if v["suite"] == "vitest"})
        self.assertFalse(rust & ui)
        # every id of the plan's threat tables is either in the manifest or explicitly listed as not collected
        plan_ids = set(re.findall(r"^\| ([A-G]\d+) \|", doc(PLAN), re.M))
        self.assertTrue(plan_ids)
        for ident in plan_ids:
            covered = ident in manifest or any(k.startswith(ident) for k in manifest) or ident in self.manifest()["not_collected"] or ident in ("F7", "G1", "G3", "G5")
            self.assertTrue(covered, f"{ident} of the test plan has neither a result line nor a stated reason")

    def test_the_ci_step_runs_the_gate_for_both_operating_systems_and_uploads_the_lines(self):
        ci = read(".github/workflows/ci.yml")
        for needle in (
            "CONFIG02C_RESULTS=", "cargo test --target ${{ matrix.target }} config02c -- --nocapture --test-threads=1", "npx vitest run src/FileSnapshot.test.tsx --reporter=verbose",
            "tools/config02c_results_check.py", "tests/config02c_expected_results.json", "config02c-results-${{ matrix.name }}", "OSNAME=windows", "OSNAME=macos",
        ):
            self.assertIn(needle, ci, needle)
        self.assertIn("exit $RC", ci)
        self.assertIn("if: always()", ci[ci.index("config02c-results-${{ matrix.name }}") - 120: ci.index("config02c-results-${{ matrix.name }}")])

    def test_blocked_and_skipped_are_never_counted_as_pass_by_the_gate_documentation(self):
        checker = read("tools/config02c_results_check.py")
        self.assertIn("never as PASS", checker)
        self.assertIn("NOT RUN", checker)


if __name__ == "__main__":
    unittest.main()
