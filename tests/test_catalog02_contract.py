"""CATALOG-02 (Issue #27) data contract, no running app needed:

* the two new Playwright-family candidates satisfy the catalog schema and carry no executable authority,
* identities, sources, licences and pins are the ones the Issue asked for; the three siblings are distinct,
* compatibility labels are conservative (the suggested-default CLI declares nothing), every label carries its limitation,
* the three earlier data files are byte-for-byte unchanged and the purpose refinement can only target a known earlier entry,
* every evidence id is registered in the research register.
"""
import hashlib
import json
import pathlib
import re
import unittest

import jsonschema

ROOT = pathlib.Path(__file__).resolve().parents[1]
FORBIDDEN_TEXT = ["curl ", "wget ", "| bash", "| sh", "iex ", "irm ", "powershell -", "Invoke-Expression", "--secrets", "API_KEY", "Authorization", "ghp_", "sk-", "claude mcp add", "npm install -g", "npx @playwright"]
ALLOWED_URLS = {"https://github.com/microsoft/playwright-cli", "https://github.com/microsoft/playwright-mcp"}


def load(rel):
    return json.loads((ROOT / rel).read_text(encoding="utf-8"))


def lf_sha256(rel):
    return hashlib.sha256((ROOT / rel).read_bytes().replace(b"\r\n", b"\n")).hexdigest()


class Catalog02Candidates(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.validator = jsonschema.Draft202012Validator(load("schemas/catalog-entry.schema.json"), format_checker=jsonschema.FormatChecker())
        cls.file = load("data/catalog-candidates-c02.json")
        cls.wrappers = cls.file["records"]
        cls.by_id = {w["record"]["toolId"]: w for w in cls.wrappers}
        cls.earlier = {r["toolId"] for r in load("data/catalog-seed.json")["records"]}
        cls.earlier |= {w["record"]["toolId"] for w in load("data/catalog-candidates-1b.json")["records"]}
        cls.c01 = {w["record"]["toolId"]: w for w in load("data/catalog-candidates-c01.json")["records"]}
        cls.register = (ROOT / "docs" / "RESEARCH-SOURCES.md").read_text(encoding="utf-8")

    def test_exactly_the_two_requested_siblings(self):
        self.assertEqual(set(self.by_id), {"microsoft-playwright-cli", "microsoft-playwright-mcp"})
        self.assertEqual(len(self.wrappers), 2)
        self.assertEqual(len(self.earlier) + len(self.c01), 16)
        self.assertFalse((self.earlier | set(self.c01)) & set(self.by_id), "no id collides with the 16 earlier entries")

    def test_every_record_validates_against_the_catalog_schema(self):
        for w in self.wrappers:
            self.validator.validate(w["record"])

    def test_kinds_sources_licences_and_pins(self):
        expected = {
            "microsoft-playwright-cli": ("executable_tool", "native-cli", "playwright-cli", "b1a5903c5b633b8c9f843ad7b629c2a78437040c", "R38"),
            "microsoft-playwright-mcp": ("agent_plugin", "mcp-server", "playwright-mcp", "0c4107acbecb75472b23c05c0e42471a0011874f", "R39"),
        }
        for tool_id, (kind, category, name, commit, evidence) in expected.items():
            w, r = self.by_id[tool_id], self.by_id[tool_id]["record"]
            self.assertEqual((w["entryKind"], r["category"]), (kind, category), tool_id)
            self.assertEqual((r["repository"]["owner"], r["repository"]["name"], r["repository"]["url"]), ("microsoft", name, f"https://github.com/microsoft/{name}"), tool_id)
            self.assertEqual(r["license"], {"state": "known", "spdxId": "Apache-2.0", "evidenceId": evidence}, tool_id)
            self.assertEqual(r["sourceSnapshot"]["immutableCommit"], commit, tool_id)
            self.assertEqual((r["sourceSnapshot"]["revisionState"], r["sourceSnapshot"]["observedRef"]), ("immutable-commit", "main"), tool_id)
            self.assertIsNone(r["sourceSnapshot"]["releaseVersion"], f"{tool_id}: no release is claimed")
            self.assertEqual(r["sourceSnapshot"]["evidenceIds"], [evidence, "R40"], tool_id)
            self.assertEqual((w["executionPolicy"], w["pluginIdentity"], w["sourcePath"]), ("manual-only", None, None), tool_id)

    def test_the_family_is_three_distinct_sources(self):
        keys = {self.c01["microsoft-playwright"]["record"]["repository"]["url"]} | {w["record"]["repository"]["url"] for w in self.wrappers}
        self.assertEqual(len(keys), 3)
        commits = {self.c01["microsoft-playwright"]["record"]["sourceSnapshot"]["immutableCommit"]} | {w["record"]["sourceSnapshot"]["immutableCommit"] for w in self.wrappers}
        self.assertEqual(len(commits), 3)
        self.assertEqual(self.c01["microsoft-playwright"]["record"]["sourceSnapshot"]["immutableCommit"], "91a765d1681cb3eeb0d0fb89fefec3cf0d21bd9b", "the framework pin is the CATALOG-01 pin, not silently moved")

    def test_nothing_claims_a_review_a_test_or_an_artifact(self):
        for tool_id, w in self.by_id.items():
            r = w["record"]
            self.assertEqual(r["reviewState"], "unreviewed", tool_id)
            self.assertIsNone(r["artifactEvidence"], tool_id)
            self.assertTrue(all(c["state"] == "publisher-describes" for c in r["verificationClaims"]), tool_id)
            for t in r["targetEvidence"]:
                self.assertIn(t["compatibilityState"], {"vendor-declared", "unknown"}, tool_id)
                self.assertTrue(t.get("limitation"), f"{tool_id}: every label carries its limitation")
                for field in ("independentCheckId", "harnessVersion", "testedAt", "evidenceDigest"):
                    self.assertNotIn(field, t)
            self.assertEqual([m["methodId"] for m in r["installMethods"]], ["unknown"], tool_id)

    def test_compatibility_matrix_follows_the_sources(self):
        every = {(p, s) for p in ("windows-x64", "macos-arm64") for s in ("local-cli", "claude-code-terminal", "antigravity-ide")}
        declared = {
            "microsoft-playwright-cli": set(),  # the suggested default for coding agents declares NOTHING: no source names an operating system
            "microsoft-playwright-mcp": {("windows-x64", "claude-code-terminal"), ("macos-arm64", "claude-code-terminal")},
        }
        for tool_id, yes in declared.items():
            m = {(t["platformTarget"], t["agentSurface"]): t["compatibilityState"] for t in self.by_id[tool_id]["record"]["targetEvidence"]}
            self.assertEqual(set(m), every, tool_id)
            self.assertEqual({k for k, v in m.items() if v == "vendor-declared"}, yes, tool_id)

    def test_runtime_requirements_and_cautions_are_recorded_not_guessed(self):
        cli = self.by_id["microsoft-playwright-cli"]["record"]
        mcp = self.by_id["microsoft-playwright-mcp"]["record"]
        self.assertIn("Node.js 18 or newer", " ".join(cli["dependencies"]))
        deps = " ".join(mcp["dependencies"])
        for needle in ("Node.js 18 or newer", "says 20", "not reconciled", "no install requirement is approved"):
            self.assertIn(needle, deps)
        notes = " ".join(cli["privacy"]["notes"])
        for needle in ("not benchmarked", "persistent option", "does not by itself mean sharing or isolating", "not authorised"):
            self.assertIn(needle, notes)
        notes = " ".join(mcp["privacy"]["notes"])
        for needle in ("higher token cost", "reach its model provider", "persistent on disk", "not a security boundary", "never edits any agent configuration"):
            self.assertIn(needle, notes)
        self.assertIsNone(cli["privacy"]["mayTransmitProjectContent"])
        self.assertTrue(mcp["privacy"]["mayTransmitProjectContent"])
        self.assertEqual((cli["privacy"]["telemetry"], mcp["privacy"]["telemetry"]), ("unknown", "unknown"))

    def test_no_executable_text_secret_or_unexpected_url(self):
        for tool_id, w in self.by_id.items():
            text = json.dumps(w["record"], ensure_ascii=False)
            for needle in FORBIDDEN_TEXT:
                self.assertNotIn(needle, text, f"{tool_id} contains {needle!r}")
            for url in re.findall(r"https?://[^\s\"')]+", text):
                self.assertIn(url.rstrip(".,;"), ALLOWED_URLS, f"{tool_id}: {url}")

    def test_summaries_are_short_turkish_prose_not_a_copied_readme(self):
        texts = {w["record"]["toolId"]: w["purpose"] for w in self.wrappers}
        texts.update({r["toolId"]: r["purpose"] for r in self.file["purposeRefinements"]})
        self.assertEqual(set(texts), {"microsoft-playwright", "microsoft-playwright-cli", "microsoft-playwright-mcp"})
        for tool_id, p in texts.items():
            self.assertLessEqual(len(p), 300, tool_id)
            self.assertGreaterEqual(len(p), 100, tool_id)
            for bad in ("http", "npm ", "npx", "```", "README"):
                self.assertNotIn(bad, p, tool_id)
            self.assertIn("Bu uygulama", p.replace("bu uygulama", "Bu uygulama"), tool_id)

    def test_refinements_only_target_a_known_earlier_entry_and_touch_nothing_else(self):
        for r in self.file["purposeRefinements"]:
            self.assertEqual(set(r), {"toolId", "purpose"})
            self.assertIn(r["toolId"], set(self.c01), "a refinement may only reword an entry of an earlier file")
        self.assertEqual(lf_sha256("data/catalog-candidates-c01.json"), "2d1a104f31946dbb26f3134bfb511cdb55d8f6f2fe64496b08b9268fbf65278e")

    def test_evidence_ids_are_registered_in_the_research_register(self):
        for tool_id, w in self.by_id.items():
            r = w["record"]
            ids = set(r["sourceSnapshot"]["evidenceIds"]) | {r["license"]["evidenceId"], r["privacy"]["evidenceId"]}
            ids |= {t["evidenceId"] for t in r["targetEvidence"]} | {m["evidenceId"] for m in r["installMethods"]} | {c["evidenceId"] for c in r["verificationClaims"]}
            for evidence in ids:
                self.assertIn(f"| {evidence} |", self.register, f"{tool_id}: {evidence}")
        for evidence, needle in (("R38", "b1a5903c5b633b8c9f843ad7b629c2a78437040c"), ("R39", "0c4107acbecb75472b23c05c0e42471a0011874f"), ("R40", "summarising fetch tool")):
            row = next(l for l in self.register.splitlines() if l.startswith(f"| {evidence} |"))
            self.assertIn(needle, row)

    def test_the_earlier_files_are_unchanged(self):
        self.assertEqual(lf_sha256("data/catalog-seed.json"), "8341414b7671630e92fd7ac1a86916299434fcbc56a8475d9c74cfa129c73ad2")
        self.assertEqual(lf_sha256("data/catalog-candidates-1b.json"), "235b1ef8754f094f77287ef3ba52f793ac29390a61570236a7e8e01e613ba11e")
        self.assertEqual(lf_sha256("data/catalog-candidates-c01.json"), "2d1a104f31946dbb26f3134bfb511cdb55d8f6f2fe64496b08b9268fbf65278e")

    def test_the_rust_loader_expects_exactly_these_files_and_counts(self):
        src = (ROOT / "src-tauri" / "src" / "catalog.rs").read_text(encoding="utf-8")
        self.assertIn('include_str!("../../data/catalog-candidates-c02.json")', src)
        for const in ("PHASE0_TOOL_COUNT: usize = 9", "CANDIDATE_COUNT: usize = 4", "CATALOG01_COUNT: usize = 3", "CATALOG02_COUNT: usize = 2"):
            self.assertIn(f"pub const {const};", src)


if __name__ == "__main__":
    unittest.main()
