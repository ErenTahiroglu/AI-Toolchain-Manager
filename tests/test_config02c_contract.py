"""CONFIG-02C (Issue #40) contract, no running app needed. A DESIGN phase for one user-authorised read of a project `.mcp.json`:

* the three documents exist, keep READ_NOT_GRANTED, and use ONE closed status vocabulary,
* the mapping table covers every FsError code of the handle-relative layer,
* the only production source files that name `.mcp.json` are the ones found by the design inspection (so a new reader cannot be added silently),
* nothing about the application's reach changed (no new command, permission, dependency, plugin, migration or frontend import),
* the owner's response block is still blank.

This is a text/structure guard. It cannot show any behaviour of a reader: none exists. Whoever implements the read (after an explicit owner decision) must
update this file deliberately, which is the point.
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


def read(rel):
    return (ROOT / rel).read_text(encoding="utf-8")


def doc(name):
    return (DOCS / name).read_text(encoding="utf-8")


class Documents(unittest.TestCase):
    def test_all_three_exist_and_are_not_a_grant(self):
        for name in (CONTRACT, PLAN, REQUEST):
            text = doc(name)
            self.assertIn("READ_NOT_GRANTED", text, name)
            self.assertNotIn("READ_GRANTED", text, name)
            self.assertNotIn("CONFIG_02C_IMPLEMENTED", text, name)
        # R1: the readiness state is reported in the Issue after exact-SHA CI, never asserted inside the design files
        contract = doc(CONTRACT)
        self.assertIn("CONFIG_02C_DESIGN_R1", contract)
        self.assertIn("CONFIG_02C_DESIGN_ACCEPTED_WITH_REVISIONS", contract)
        self.assertIn("issuecomment-6518093495", contract)
        self.assertNotIn("READY_FOR_INDEPENDENT_REVIEW", contract)

    def test_the_owner_response_block_is_blank(self):
        text = doc(REQUEST)
        self.assertIsNone(re.search(r"\[[xX]\]", text), "a box in the owner response is ticked: that is the owner's act, in the Issue and in this file")
        self.assertIn("Status: READ_NOT_GRANTED", text)

    def test_one_closed_status_vocabulary(self):
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
        # no invented status in the test plan: every backticked status-looking token it uses is in the list
        plan = doc(PLAN)
        for token in re.findall(r"`(root_not_found|absent|present_\w+|malformed|denied|unsafe_path|unsupported_fs|too_large|changed_during_read|io_error)`", plan):
            self.assertIn(token, STATUSES, token)

    def test_every_fs_error_code_has_a_row_in_the_mapping(self):
        codes = re.findall(r'FsError::\w+ => "([a-z_]+)"', read("src-tauri/src/managed_fs.rs"))
        self.assertGreaterEqual(len(codes), 15)
        contract = doc(CONTRACT)
        for code in codes:
            self.assertTrue(
                re.search(r"^\| `%s` \|" % re.escape(code), contract, re.M), f"FsError code {code!r} has no row in the contract's mapping table"
            )

    def test_the_skip_visibility_condition_is_stated(self):
        plan = doc(PLAN)
        self.assertIn("CONFIG02C_RESULT <test-id> <os>", plan)
        self.assertIn("CONFIG02C_RESULTS", plan)
        for word in ("PASS", "FAIL", "SKIPPED", "BLOCKED", "NOT RUN"):
            self.assertIn(word, plan)
        self.assertIn("a `SKIPPED` line is never counted as PASS", plan)
        self.assertIn("no result line for that OS", plan)
        self.assertIn("not evidence of a reader", plan)
        self.assertIn("jsdom", plan)
        self.assertNotIn("CONFIG02C_SKIP ", plan)

    def test_the_turkish_gate_text_names_the_file_the_limit_and_the_no_button(self):
        request = doc(REQUEST)
        for needle in (".mcp.json", "1.048.576", "Hayır, metni yapıştırarak devam edeceğim", "Temizle", "kaydetmez", "İnternete bağlanılmaz"):
            self.assertIn(needle, request, needle)


class ReviewerDecisions(unittest.TestCase):
    """Independent decision CONFIG_02C_DESIGN_ACCEPTED_WITH_REVISIONS (Issue #40, O1-O5) must be visible in the wording, not only in the Issue."""

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

    def test_withdrawn_overclaims_do_not_come_back(self):
        # The plan may QUOTE these (to say they are wrong); the two documents that make promises may not say them.
        for name in (CONTRACT, REQUEST):
            text = doc(name)
            for phrase in ("nothing outside the file", "okuma biter bitmez silinir", "Başka dosya, alt klasör veya üst klasör okunmaz", "erased when the call"):
                self.assertNotIn(phrase, text, f"{name}: {phrase}")

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
        self.assertIn("The owner's decisions are made only in §8", request)
        self.assertIsNone(re.search(r"\[[xX]\]", request))


class NothingNewIsReachable(unittest.TestCase):
    def test_only_the_known_production_files_name_the_target(self):
        found = set()
        for path in SRC.rglob("*.rs"):
            if "tests" in path.name:
                continue
            if ".mcp.json" in path.read_text(encoding="utf-8"):
                found.add(path.relative_to(SRC).as_posix())
        # config_txn.rs and managed_fs_mcp.rs are compiled for tests only (checked by test_config02b_contract); discovery/* is the existing scan;
        # discovery/store.rs only names the file inside a Turkish sentence.
        self.assertEqual(
            found,
            {"config_txn.rs", "managed_fs_mcp.rs", "discovery/detect.rs", "discovery/store.rs", "discovery_probe.rs"},
            "a new production file now names .mcp.json: that is a new reader and needs the owner decision of docs/config-02c/REAL_READ_AUTHORIZATION_REQUEST.md",
        )

    def test_no_read_command_exists(self):
        for path in SRC.rglob("*.rs"):
            self.assertNotIn("read_project_mcp", path.read_text(encoding="utf-8"), str(path))
        self.assertEqual(read("src-tauri/src/commands.rs").count("#[tauri::command]"), 36)
        for p in json.loads(read("src-tauri/capabilities/default.json"))["permissions"]:
            self.assertNotIn("mcp", p, p)
            self.assertIsNone(re.search(r"fs|shell|http|opener|dialog|clipboard|process|os:|path", p, re.I), p)

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

    def test_the_existing_path_accepting_command_is_still_the_only_one(self):
        # Finding F2: exactly one command takes a project directory, and it is the existing discovery scan.
        taking_dirs = [p.relative_to(SRC).as_posix() for p in SRC.rglob("*.rs") if "tests" not in p.name and re.search(r"pub directory: String", p.read_text(encoding="utf-8"))]
        self.assertEqual(taking_dirs, ["discovery/mod.rs"])


if __name__ == "__main__":
    unittest.main()
