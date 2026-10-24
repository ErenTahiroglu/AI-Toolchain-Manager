"""CATALOG-01 (Issue #26) data contract, no running app needed:

* the three new candidates satisfy the Phase 0 catalog schema and carry no executable authority,
* identities, sources and licences are the ones the Issue asked for, pinned to immutable commits,
* compatibility labels are conservative, every one carries its limitation, nothing is "tested",
* the two earlier candidate files are byte-for-byte unchanged (line endings normalised),
* every evidence id is registered in the research register.
"""
import hashlib
import json
import pathlib
import re
import unittest

import jsonschema

ROOT = pathlib.Path(__file__).resolve().parents[1]
FORBIDDEN_TEXT = ["curl ", "wget ", "| bash", "| sh", "iex ", "irm ", "powershell -", "Invoke-Expression", "exaApiKey", "APIFY_TOKEN", "Authorization", "apify_api_", "ghp_", "sk-"]
ALLOWED_URLS = {
    "https://github.com/microsoft/playwright",
    "https://github.com/apify/apify-mcp-server",
    "https://github.com/exa-labs/exa-mcp-server",
    "https://mcp.apify.com",
    "https://mcp.exa.ai/mcp",
}


def load(rel):
    return json.loads((ROOT / rel).read_text(encoding="utf-8"))


def lf_sha256(rel):
    return hashlib.sha256((ROOT / rel).read_bytes().replace(b"\r\n", b"\n")).hexdigest()


class Catalog01Candidates(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.schema = load("schemas/catalog-entry.schema.json")
        cls.validator = jsonschema.Draft202012Validator(cls.schema, format_checker=jsonschema.FormatChecker())
        cls.file = load("data/catalog-candidates-c01.json")
        cls.wrappers = cls.file["records"]
        cls.by_id = {w["record"]["toolId"]: w for w in cls.wrappers}
        cls.earlier = {r["toolId"] for r in load("data/catalog-seed.json")["records"]} | {
            w["record"]["toolId"] for w in load("data/catalog-candidates-1b.json")["records"]
        }
        cls.register = (ROOT / "docs" / "RESEARCH-SOURCES.md").read_text(encoding="utf-8")

    def test_exactly_the_three_requested_candidates(self):
        self.assertEqual(set(self.by_id), {"microsoft-playwright", "apify-mcp-server", "exa-mcp-server"})
        self.assertEqual(len(self.wrappers), 3)
        self.assertEqual(len(self.earlier), 13)
        self.assertFalse(self.earlier & set(self.by_id), "no id collides with the 13 earlier entries")

    def test_every_record_validates_against_the_catalog_schema(self):
        for w in self.wrappers:
            self.validator.validate(w["record"])

    def test_kinds_sources_licences_and_pins(self):
        expected = {
            "microsoft-playwright": ("executable_tool", "microsoft", "playwright", "Apache-2.0", "91a765d1681cb3eeb0d0fb89fefec3cf0d21bd9b", "main"),
            "apify-mcp-server": ("agent_plugin", "apify", "apify-mcp-server", "MIT", "cdf5b23230b26cd42e5d0db81528365f6b9cdc9c", "master"),
            "exa-mcp-server": ("agent_plugin", "exa-labs", "exa-mcp-server", "MIT", "0b499f095c4ca145eee07b09137641c037d58449", "main"),
        }
        for tool_id, (kind, owner, name, spdx, commit, ref) in expected.items():
            w = self.by_id[tool_id]
            r = w["record"]
            self.assertEqual(w["entryKind"], kind, tool_id)
            self.assertEqual((r["repository"]["owner"], r["repository"]["name"]), (owner, name), tool_id)
            self.assertEqual(r["repository"]["url"], f"https://github.com/{owner}/{name}", tool_id)
            self.assertEqual(r["license"], {"state": "known", "spdxId": spdx, "evidenceId": r["license"]["evidenceId"]}, tool_id)
            self.assertEqual(r["sourceSnapshot"]["immutableCommit"], commit, tool_id)
            self.assertEqual(r["sourceSnapshot"]["revisionState"], "immutable-commit", tool_id)
            self.assertEqual(r["sourceSnapshot"]["observedRef"], ref, tool_id)
            self.assertIsNone(r["sourceSnapshot"]["releaseVersion"], f"{tool_id}: no release is claimed")
            self.assertEqual(w["executionPolicy"], "manual-only", tool_id)
            self.assertIsNone(w["pluginIdentity"], f"{tool_id}: no unverified plugin identity")
            self.assertIsNone(w["sourcePath"], tool_id)

    def test_nothing_claims_a_review_a_test_or_an_artifact(self):
        for tool_id, w in self.by_id.items():
            r = w["record"]
            self.assertEqual(r["reviewState"], "unreviewed", tool_id)
            self.assertIsNone(r["artifactEvidence"], tool_id)
            for claim in r["verificationClaims"]:
                self.assertEqual(claim["state"], "publisher-describes", tool_id)
            for t in r["targetEvidence"]:
                self.assertIn(t["compatibilityState"], {"vendor-declared", "unknown"}, f"{tool_id} {t['platformTarget']} {t['agentSurface']}")
                self.assertTrue(t.get("limitation"), f"{tool_id}: every label carries its limitation")
                for field in ("independentCheckId", "harnessVersion", "testedAt", "evidenceDigest"):
                    self.assertNotIn(field, t)
            for m in r["installMethods"]:
                self.assertIn(m["methodId"], {"unknown", "native-plugin-manager"}, tool_id)
                self.assertNotIn("command", m)

    def test_compatibility_matrix_is_exactly_what_the_snapshots_document(self):
        def matrix(tool_id):
            return {(t["platformTarget"], t["agentSurface"]): t["compatibilityState"] for t in self.by_id[tool_id]["record"]["targetEvidence"]}

        every = {(p, s) for p in ("windows-x64", "macos-arm64") for s in ("local-cli", "claude-code-terminal", "antigravity-ide")}
        declared = {
            "microsoft-playwright": {("windows-x64", "local-cli"), ("macos-arm64", "local-cli")},
            "apify-mcp-server": {("windows-x64", "local-cli"), ("macos-arm64", "local-cli"), ("windows-x64", "claude-code-terminal"), ("macos-arm64", "claude-code-terminal")},
            "exa-mcp-server": set(),
        }
        for tool_id, yes in declared.items():
            m = matrix(tool_id)
            self.assertEqual(set(m), every, f"{tool_id}: all six combinations are stated, none omitted")
            self.assertEqual({k for k, v in m.items() if v == "vendor-declared"}, yes, tool_id)
            self.assertTrue(all(v == "unknown" for k, v in m.items() if k not in yes), tool_id)

    def test_privacy_cost_and_scope_warnings_are_present(self):
        pw = self.by_id["microsoft-playwright"]["record"]
        self.assertFalse(pw["category"] == "mcp-server")
        notes = " ".join(pw["privacy"]["notes"])
        for needle in ("not a ready-to-use MCP server", "microsoft/playwright-mcp", "microsoft/playwright-cli", "authentication state"):
            self.assertIn(needle, notes)
        apify = self.by_id["apify-mcp-server"]["record"]
        self.assertTrue(apify["privacy"]["mayTransmitProjectContent"])
        text = " ".join(apify["privacy"]["notes"])
        for needle in ("paid", "variable charges", "free", "No sign-in, token, payment method or billing was authorised", "hosted server can differ"):
            self.assertIn(needle, text)
        exa = self.by_id["exa-mcp-server"]["record"]
        self.assertTrue(exa["privacy"]["mayTransmitProjectContent"])
        text = " ".join(exa["privacy"]["notes"])
        for needle in ("Paid features or credits", "never be done", "not the same thing as an installed MCP server", "unknown"):
            self.assertIn(needle, text)
        self.assertEqual(exa["privacy"]["telemetry"], "unknown")
        self.assertEqual(pw["privacy"]["telemetry"], "unknown")
        self.assertEqual(apify["privacy"]["telemetry"], "opt-out")

    def test_no_executable_text_secret_or_unexpected_url(self):
        for tool_id, w in self.by_id.items():
            text = json.dumps(w, ensure_ascii=False)
            for needle in FORBIDDEN_TEXT:
                self.assertNotIn(needle, text, f"{tool_id} contains {needle!r}")
            for url in re.findall(r"https?://[^\s\"')]+", text):
                self.assertIn(url.rstrip(".,;"), ALLOWED_URLS, f"{tool_id}: {url}")

    def test_evidence_ids_are_registered_in_the_research_register(self):
        for tool_id, w in self.by_id.items():
            r = w["record"]
            ids = set(r["sourceSnapshot"]["evidenceIds"]) | {r["license"]["evidenceId"], r["privacy"]["evidenceId"]}
            ids |= {t["evidenceId"] for t in r["targetEvidence"]} | {m["evidenceId"] for m in r["installMethods"]}
            ids |= {c["evidenceId"] for c in r["verificationClaims"]}
            for evidence in ids:
                self.assertIn(f"| {evidence} |", self.register, f"{tool_id}: {evidence}")
        for evidence, commit in (("R35", "91a765d1681cb3eeb0d0fb89fefec3cf0d21bd9b"), ("R36", "cdf5b23230b26cd42e5d0db81528365f6b9cdc9c"), ("R37", "0b499f095c4ca145eee07b09137641c037d58449")):
            row = next(l for l in self.register.splitlines() if l.startswith(f"| {evidence} |"))
            self.assertIn(commit, row)
            self.assertTrue("NOT contacted" in row or "Nothing was installed" in row, f"{evidence}: the register states that nothing was contacted or installed")

    def test_the_earlier_candidate_files_are_unchanged(self):
        self.assertEqual(lf_sha256("data/catalog-seed.json"), "8341414b7671630e92fd7ac1a86916299434fcbc56a8475d9c74cfa129c73ad2")
        self.assertEqual(lf_sha256("data/catalog-candidates-1b.json"), "235b1ef8754f094f77287ef3ba52f793ac29390a61570236a7e8e01e613ba11e")

    def test_the_rust_loader_expects_exactly_these_files_and_counts(self):
        src = (ROOT / "src-tauri" / "src" / "catalog.rs").read_text(encoding="utf-8")
        self.assertIn('include_str!("../../data/catalog-candidates-c01.json")', src)
        self.assertIn("pub const CATALOG01_COUNT: usize = 3;", src)
        self.assertIn("pub const CANDIDATE_COUNT: usize = 4;", src)
        self.assertIn("pub const PHASE0_TOOL_COUNT: usize = 9;", src)


if __name__ == "__main__":
    unittest.main()
