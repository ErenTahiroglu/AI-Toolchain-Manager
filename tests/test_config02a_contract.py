"""CONFIG-02A (Issue #38) contract, no running app needed. One real catalog entry gets one reviewed, preview-only declaration:

* the declaration is exactly the documented stanza and is bound to the catalog record's pinned commit and repository,
* nothing else about the application's reach changed (no IPC / filesystem / network / process access, no new capability, no new migration, no new dependency),
* the sources and the documents keep the boundaries, and the required Turkish notice is present once.
"""
import json
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
NOTICE = "Yalnızca yapılandırma taslağı; uygulanmadı, bağlantı kurulmadı. Hizmetin ücretleri, gizliliği ve kimlik doğrulaması ayrıca değerlendirilmelidir."


def read(rel):
    return (ROOT / rel).read_text(encoding="utf-8")


def catalog_record():
    records = json.loads(read("data/catalog-candidates-c01.json"))["records"]
    return next(r["record"] for r in records if r["record"]["toolId"] == "exa-mcp-server")


class Declaration(unittest.TestCase):
    def test_the_registry_holds_exactly_the_documented_exa_stanza(self):
        src = read("src/configDeclarations.ts")
        self.assertEqual(re.findall(r'entryId: "([^"]+)"', src), ["exa-mcp-server"])
        self.assertEqual(re.findall(r'hostId: "([^"]+)"', src), ["claude-code-project-mcp"])
        self.assertIn('declaration: { type: "http", url: "https://mcp.exa.ai/mcp" },', src)
        self.assertEqual(re.findall(r'serverName: "([^"]+)"', src), ["exa"])

    def test_it_is_bound_to_the_catalogs_pin_and_repository(self):
        src = read("src/configDeclarations.ts")
        record = catalog_record()
        self.assertEqual(re.findall(r'reviewedAgainst: "([0-9a-f]{40})"', src), [record["sourceSnapshot"]["immutableCommit"]])
        self.assertEqual(re.findall(r'repositoryUrl: "([^"]+)"', src), [record["repository"]["url"]])
        self.assertEqual(record["category"], "mcp-server")

    def test_the_stanza_carries_no_credential_header_or_query(self):
        block = read("src/configDeclarations.ts").split("export const REVIEWED_DECLARATIONS")[1]
        declaration = re.search(r"declaration: (\{[^}]*\})", block).group(1)
        for banned in ("headers", "env", "?", "apiKey", "Bearer", "token", "tools="):
            self.assertNotIn(banned, declaration, banned)

    def test_the_required_notice_is_defined_once_and_the_review_is_not_called_verified(self):
        self.assertEqual(read("src/configPlan.ts").count(NOTICE), 1)
        src = read("src/configDeclarations.ts")
        self.assertIn("çalışma zamanı doğrulanmadı", src)
        self.assertIn("bağımsız inceleme bekliyor", src)
        self.assertNotRegex(src, r"(?i)(runtime|çalışma zamanı) (verified|doğrulandı)")


class Reach(unittest.TestCase):
    def test_the_sources_remain_pure(self):
        forbidden = r"invoke\(|@tauri-apps|\bapi\.|fetch\(|XMLHttpRequest|WebSocket|navigator\.clipboard|localStorage|sessionStorage|indexedDB|setTimeout|setInterval|createObjectURL|FileReader|showSaveFilePicker|showOpenFilePicker|window\.open|child_process|require\("
        for rel in ("src/configPlan.ts", "src/configDeclarations.ts", "src/components/ConfigPreview.tsx"):
            self.assertIsNone(re.search(forbidden, read(rel)), rel)

    def test_no_apply_copy_or_export_control_exists_in_the_preview_component(self):
        text = read("src/components/ConfigPreview.tsx")
        self.assertNotRegex(text, r"(?i)uygula\b|>\s*kopyala|dışa aktar|indir|\bexport\b.*\bblob\b")

    def test_no_new_capability_migration_or_dependency(self):
        for p in json.loads(read("src-tauri/capabilities/default.json"))["permissions"]:
            self.assertIsNone(re.search(r"fs|shell|http|opener|dialog|clipboard|process|os:|path", p, re.I), p)
        migrations = sorted(p.name for p in (ROOT / "src-tauri" / "migrations").glob("*.sql"))
        self.assertEqual(len(migrations), 10)
        self.assertEqual(migrations[-1], "0010_codex_cli_surface.sql")
        for rel in ("src-tauri/src/commands.rs", "src-tauri/src/lib.rs"):
            self.assertNotRegex(read(rel), r"(?i)fn\s+\w*(mcp_config|config_preview|config_plan|write_config|apply_config|exa)\w*")


class Documents(unittest.TestCase):
    def test_the_documents_keep_the_boundaries(self):
        docs = "\n".join(read(f"docs/config-02a/{n}.md") for n in ("SOURCES_AND_DECLARATION", "IMPLEMENTATION_AND_TESTS"))
        for needle in ("REAL_INSTALLATION_NO_GO", "REAL_CONFIG_WRITE_NO_GO", "#14", "#25", "#38", "SRC-CONFIG-02A-EXA-1", "not authorized", "R55", "R56"):
            self.assertIn(needle, docs, needle)
        self.assertIn(NOTICE, docs)
        self.assertNotRegex(docs, r"(?i)runtime[- ]verified(?! )|production[- ]ready")

    def test_the_sources_are_registered(self):
        register = read("docs/RESEARCH-SOURCES.md")
        for evidence in ("R55", "R56"):
            self.assertTrue(any(line.startswith(f"| {evidence} |") for line in register.splitlines()), evidence)
        row = next(line for line in register.splitlines() if line.startswith("| R55 |"))
        self.assertIn("0b499f095c4ca145eee07b09137641c037d58449", row)
        self.assertIn("trailing comma", row)


if __name__ == "__main__":
    unittest.main()
