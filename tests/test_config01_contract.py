"""CONFIG-01 (Issue #30) contract, no running app needed. The configuration preview is a pure TypeScript planner plus a read-only component:

* the registry of reviewed declarations the application ships holds exactly ONE entry since CONFIG-02A (it was empty in CONFIG-01),
* the two supported host files are exactly `.mcp.json` and `.agents/mcp_config.json`,
* the Tauri capability still grants no filesystem / shell / http / dialog / clipboard permission and no configuration command exists,
* the new sources contain no IPC, file, clipboard, network, storage, timer or process access,
* the four documents state the hard boundaries (REAL_INSTALLATION_NO_GO, REAL_CONFIG_WRITE_NO_GO, open issues) and the future write gate lists the required controls.
"""
import json
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


def read(rel):
    return (ROOT / rel).read_text(encoding="utf-8")


class ConfigPreviewContract(unittest.TestCase):
    def test_the_registry_of_reviewed_declarations_holds_exactly_one_entry(self):
        src = read("src/configDeclarations.ts")
        self.assertRegex(src, r"export const REVIEWED_DECLARATIONS: readonly ReviewedDeclaration\[\] = \[")
        self.assertEqual(re.findall(r'entryId: "([^"]+)"', src), ["exa-mcp-server"])

    def test_the_two_supported_host_files_and_surfaces(self):
        src = read("src/configPlan.ts")
        paths = re.findall(r'relativePath: "([^"]+)"', src)
        self.assertEqual(paths, [".mcp.json", ".agents/mcp_config.json"])
        surfaces = re.findall(r'^\s+surface: "([^"]+)"', src, re.M)
        self.assertEqual(surfaces, ["claude-code-terminal", "antigravity-ide"])
        for p in paths:
            self.assertFalse(p.startswith(("/", "\\")) or ".." in p or "~" in p or ":" in p)

    def test_the_capability_grants_no_filesystem_shell_network_or_configuration_permission(self):
        permissions = json.loads(read("src-tauri/capabilities/default.json"))["permissions"]
        self.assertGreater(len(permissions), 20)
        for p in permissions:
            self.assertIsNone(re.search(r"fs|shell|http|opener|dialog|clipboard|process|os:|path|window", p, re.I), p)
            self.assertIsNone(re.search(r"config|mcp|write-file|apply", p, re.I), p)

    def test_no_rust_command_for_configuration_was_added(self):
        for rel in ("src-tauri/src/commands.rs", "src-tauri/src/lib.rs"):
            text = read(rel)
            self.assertNotRegex(text, r"(?i)fn\s+\w*(mcp_config|config_preview|config_plan|write_config|apply_config)\w*")

    def test_the_new_sources_are_pure(self):
        forbidden = r"invoke\(|@tauri-apps|\bapi\.|fetch\(|XMLHttpRequest|WebSocket|navigator\.clipboard|localStorage|sessionStorage|indexedDB|setTimeout|setInterval|createObjectURL|FileReader|showSaveFilePicker|showOpenFilePicker|window\.open|child_process|require\("
        for rel in ("src/configPlan.ts", "src/configDeclarations.ts", "src/components/ConfigPreview.tsx"):
            self.assertIsNone(re.search(forbidden, read(rel)), rel)

    def test_the_documents_state_the_boundaries(self):
        docs = {name: read(f"docs/config-01/{name}.md") for name in ("BEHAVIOR_AND_SCOPE", "SOURCE_AND_TARGET_MAP", "TESTS_AND_THREAT_MODEL", "FUTURE_WRITE_GATE")}
        for name in ("BEHAVIOR_AND_SCOPE", "FUTURE_WRITE_GATE"):
            self.assertIn("REAL_CONFIG_WRITE_NO_GO", docs[name], name)
        self.assertIn("REAL_INSTALLATION_NO_GO", docs["BEHAVIOR_AND_SCOPE"])
        for needle in ("#14", "#25", "#29", "#30"):
            self.assertIn(needle, docs["BEHAVIOR_AND_SCOPE"] + docs["FUTURE_WRITE_GATE"])
        gate = docs["FUTURE_WRITE_GATE"]
        for needle in ("Project-root identity", "Links, reparse points, hard links", "TOCTOU", "Atomic, no-clobber", "Rollback and crash recovery", "Stanza ownership", "Consent", "No generic"):
            self.assertIn(needle.lower(), gate.lower(), needle)
        self.assertIn("not solved", gate.lower())
        self.assertNotRegex(docs["BEHAVIOR_AND_SCOPE"], r"(?i)issue #14 (is )?(closed|resolved|satisfied)")

    def test_the_publisher_sources_are_registered(self):
        register = read("docs/RESEARCH-SOURCES.md")
        for evidence in ("R45", "R46", "R47"):
            row = next(line for line in register.splitlines() if line.startswith(f"| {evidence} |"))
            self.assertIn("summarising fetch tool", row)
        self.assertIn(".mcp.json", register)
        self.assertIn(".agents/mcp_config.json", register)


if __name__ == "__main__":
    unittest.main()
