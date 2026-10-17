"""Phase 1B data contracts that need no running app:

* the four new catalog candidates satisfy the Phase 0 catalog schema and carry no executable authority,
* their classification and risk policy match ADR-0002 / Issue #2,
* the synthetic recommendation scenarios are well-formed and only refer to known catalog entries.
"""
import json
import pathlib
import unittest

import jsonschema

ROOT = pathlib.Path(__file__).resolve().parents[1]
KINDS = {"executable_tool", "agent_plugin", "skill_collection", "prompt_source"}
FORBIDDEN_TEXT = ["curl ", "wget ", "| bash", "| sh", "iex", "irm ", "powershell -", "Invoke-Expression"]


def load(rel):
    return json.loads((ROOT / rel).read_text(encoding="utf-8"))


class CatalogCandidates(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.schema = load("schemas/catalog-entry.schema.json")
        cls.validator = jsonschema.Draft202012Validator(cls.schema, format_checker=jsonschema.FormatChecker())
        cls.file = load("data/catalog-candidates-1b.json")
        cls.wrappers = cls.file["records"]
        cls.by_id = {w["record"]["toolId"]: w for w in cls.wrappers}
        cls.seed_ids = {r["toolId"] for r in load("data/catalog-seed.json")["records"]}

    def test_exactly_the_four_requested_candidates(self):
        self.assertEqual(set(self.by_id), {"anthropic-frontend-design", "strix", "agent-browser", "agentic-awesome-skills"})
        self.assertEqual(len(self.wrappers), 4)

    def test_every_record_validates_against_the_phase0_catalog_schema(self):
        for w in self.wrappers:
            self.validator.validate(w["record"])

    def test_identities_do_not_collide_with_the_nine_seed_tools(self):
        self.assertFalse(self.seed_ids & set(self.by_id))

    def test_classification_matches_adr_0002(self):
        expected = {
            "anthropic-frontend-design": "agent_plugin",
            "strix": "executable_tool",
            "agent-browser": "executable_tool",
            "agentic-awesome-skills": "skill_collection",
        }
        for tool_id, kind in expected.items():
            self.assertEqual(self.by_id[tool_id]["entryKind"], kind, tool_id)
        for w in self.wrappers:
            self.assertIn(w["entryKind"], KINDS)

    def test_official_sources_and_plugin_identity(self):
        fd = self.by_id["anthropic-frontend-design"]
        self.assertEqual(fd["pluginIdentity"], "frontend-design@claude-plugins-official")
        self.assertEqual(fd["sourcePath"], "plugins/frontend-design")
        self.assertEqual(fd["record"]["repository"]["url"], "https://github.com/anthropics/claude-plugins-official")
        urls = {tid: w["record"]["repository"]["url"] for tid, w in self.by_id.items()}
        self.assertEqual(urls["strix"], "https://github.com/usestrix/strix")
        self.assertEqual(urls["agent-browser"], "https://github.com/vercel-labs/agent-browser")
        self.assertEqual(urls["agentic-awesome-skills"], "https://github.com/sickn33/agentic-awesome-skills")

    def test_no_candidate_claims_a_review_a_verification_or_an_installation(self):
        for tool_id, w in self.by_id.items():
            r = w["record"]
            self.assertIn(r["reviewState"], {"unreviewed", "quarantined"}, tool_id)
            self.assertIsNone(r["artifactEvidence"], tool_id)
            for claim in r["verificationClaims"]:
                self.assertIn(claim["state"], {"publisher-describes", "unknown", "not-run"}, tool_id)
            for t in r["targetEvidence"]:
                self.assertNotEqual(t["compatibilityState"], "independently-tested", f"{tool_id}: nothing was tested")

    def test_strix_is_quarantined_and_blocked_until_scoped_authorization(self):
        strix = self.by_id["strix"]
        self.assertEqual(strix["record"]["reviewState"], "quarantined")
        self.assertEqual(strix["executionPolicy"], "blocked-until-scoped-authorization")
        for tool_id, w in self.by_id.items():
            if tool_id != "strix":
                self.assertEqual(w["executionPolicy"], "manual-only")
        self.assertTrue(strix["record"]["privacy"]["mayTransmitProjectContent"])

    def test_skill_collection_license_is_not_blanket_approved(self):
        aas = self.by_id["agentic-awesome-skills"]["record"]
        self.assertEqual(aas["license"]["state"], "review-required")
        self.assertIsNone(aas["license"]["spdxId"])

    def test_unknown_compatibility_is_never_upgraded(self):
        fd = {(t["platformTarget"], t["agentSurface"]): t["compatibilityState"] for t in self.by_id["anthropic-frontend-design"]["record"]["targetEvidence"]}
        self.assertEqual(fd[("windows-x64", "claude-code-terminal")], "vendor-declared")
        self.assertEqual(fd[("windows-x64", "antigravity-ide")], "unknown")
        self.assertEqual(fd[("macos-arm64", "antigravity-ide")], "unknown")
        for t in self.by_id["strix"]["record"]["targetEvidence"]:
            self.assertEqual(t["compatibilityState"], "unknown", "no OS/architecture matrix was reviewed for Strix")

    def test_no_executable_commands_or_pipe_to_shell_snippets_are_stored(self):
        for tool_id, w in self.by_id.items():
            text = json.dumps(w)
            for needle in FORBIDDEN_TEXT:
                self.assertNotIn(needle, text, f"{tool_id} contains {needle!r}")
        # The unsafe publisher installer is recorded only as a labelled warning, never as a method.
        strix_methods = [m["methodId"] for m in self.by_id["strix"]["record"]["installMethods"]]
        self.assertNotIn("manual-instructions", strix_methods)

    def test_evidence_ids_are_registered_in_the_research_register(self):
        register = (ROOT / "docs" / "RESEARCH-SOURCES.md").read_text(encoding="utf-8")
        for w in self.wrappers:
            for evidence in w["record"]["sourceSnapshot"]["evidenceIds"]:
                self.assertIn(f"| {evidence} |", register, evidence)


class RecommendationScenarios(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.data = load("data/recommendation-scenarios.json")
        cls.entry_ids = {r["toolId"] for r in load("data/catalog-seed.json")["records"]} | {
            w["record"]["toolId"] for w in load("data/catalog-candidates-1b.json")["records"]
        }

    def test_scenarios_are_labelled_synthetic_and_well_formed(self):
        self.assertTrue(self.data["synthetic"])
        ids = [s["id"] for s in self.data["scenarios"]]
        self.assertEqual(len(ids), len(set(ids)))
        self.assertGreaterEqual(len(ids), 10)
        for s in self.data["scenarios"]:
            self.assertTrue(s["title"] and s["taskText"])
            self.assertIsInstance(s["evidenceFiles"], list)
            for name in s["evidenceFiles"]:
                self.assertNotRegex(name, r"[\\/:]", "evidence entries are bare file names, never paths")

    def test_expectations_refer_only_to_known_entries_and_levels(self):
        for s in self.data["scenarios"]:
            for entry, level in s["expect"]["entries"].items():
                self.assertIn(entry, self.entry_ids)
                self.assertIn(level, {"suggest", "informational"})
            for entry in s["expect"].get("suppressed", {}):
                self.assertIn(entry, self.entry_ids)

    def test_negative_scenarios_exist_for_the_required_false_positives(self):
        by_id = {s["id"]: s for s in self.data["scenarios"]}
        for sid in ["backend-with-css-in-repo", "security-keyword-only", "unit-test-no-browser", "api-endpoint-with-ui-files"]:
            self.assertEqual(by_id[sid]["expect"]["entries"], {}, sid)
        self.assertEqual(by_id["react-component"]["evidenceFiles"], ["PricingTable.tsx"], "TSX UI task without any CSS file")


if __name__ == "__main__":
    unittest.main()
