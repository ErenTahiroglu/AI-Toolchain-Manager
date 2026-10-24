"""CATALOG-03 (Issue #28) data contract, no running app needed:

* the four new candidates satisfy the catalog schema and carry no executable authority, secret or approved install command,
* identities, pins, licences and the quarantine posture are the ones the Issue asked for; the Karpathy-inspired repository is licence `review-required`
  with NO invented SPDX identifier and never names the former owner as an identity or a source,
* compatibility is conservative: the one declared cell is Claude-Mem x Claude Code x Windows, everything else is `unknown`,
* the four earlier data files are byte-for-byte unchanged and every evidence id is registered in the research register.
"""
import hashlib
import json
import pathlib
import re
import unittest

import jsonschema

ROOT = pathlib.Path(__file__).resolve().parents[1]
FORBIDDEN_TEXT = ["curl ", "wget ", "iwr ", "iex ", "irm ", "| bash", "| sh", "Invoke-Expression", "powershell -", "npm install -g", "npx ruflo", "npx claude-mem",
                  "/plugin install", "/plugin marketplace", "API_KEY", "Authorization", "sk-", "ghp_"]
ALLOWED_URLS = {
    "https://github.com/Egonex-AI/Understand-Anything",
    "https://github.com/thedotmack/claude-mem",
    "https://github.com/multica-ai/andrej-karpathy-skills",
    "https://github.com/ruvnet/ruflo",
}
EXPECTED = {
    "egonex-ai-understand-anything": ("agent_plugin", "agent-plugin", "known", "MIT", "Egonex-AI", "Understand-Anything", "e24fcf6d371bcfa529bc110bacb1bb43de1b8d33", "unreviewed", "manual-only", "R41"),
    "thedotmack-claude-mem": ("agent_plugin", "agent-plugin", "known", "Apache-2.0", "thedotmack", "claude-mem", "75fc021a40655cbdd258f0e070f55a03ea9c8642", "quarantined", "blocked-until-scoped-authorization", "R42"),
    "multica-ai-andrej-karpathy-skills": ("skill_collection", "agent-skill", "review-required", None, "multica-ai", "andrej-karpathy-skills", "2c606141936f1eeef17fa3043a72095b4765b9c2", "unreviewed", "manual-only", "R43"),
    "ruvnet-ruflo": ("agent_plugin", "workflow", "known", "MIT", "ruvnet", "ruflo", "fe99f748c577158ec3e517c7b421670a4601f54f", "quarantined", "blocked-until-scoped-authorization", "R44"),
}


def load(rel):
    return json.loads((ROOT / rel).read_text(encoding="utf-8"))


def lf_sha256(rel):
    return hashlib.sha256((ROOT / rel).read_bytes().replace(b"\r\n", b"\n")).hexdigest()


class Catalog03Candidates(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.validator = jsonschema.Draft202012Validator(load("schemas/catalog-entry.schema.json"), format_checker=jsonschema.FormatChecker())
        cls.file = load("data/catalog-candidates-c03.json")
        cls.wrappers = cls.file["records"]
        cls.by_id = {w["record"]["toolId"]: w for w in cls.wrappers}
        earlier = {r["toolId"] for r in load("data/catalog-seed.json")["records"]}
        for f in ("1b", "c01", "c02"):
            earlier |= {w["record"]["toolId"] for w in load(f"data/catalog-candidates-{f}.json")["records"]}
        cls.earlier = earlier
        cls.register = (ROOT / "docs" / "RESEARCH-SOURCES.md").read_text(encoding="utf-8")

    def test_exactly_the_four_requested_candidates(self):
        self.assertEqual(set(self.by_id), set(EXPECTED))
        self.assertEqual(len(self.wrappers), 4)
        self.assertEqual(len(self.earlier), 18)
        self.assertFalse(self.earlier & set(self.by_id), "no id collides with the 18 earlier entries")
        self.assertNotIn("purposeRefinements", self.file, "this file does not reword any earlier entry")

    def test_every_record_validates_against_the_catalog_schema(self):
        for w in self.wrappers:
            self.validator.validate(w["record"])

    def test_kinds_sources_licences_pins_and_posture(self):
        for tool_id, (kind, category, lic, spdx, owner, name, commit, review, policy, evidence) in EXPECTED.items():
            w, r = self.by_id[tool_id], self.by_id[tool_id]["record"]
            self.assertEqual((w["entryKind"], r["category"]), (kind, category), tool_id)
            self.assertEqual((r["repository"]["owner"], r["repository"]["name"], r["repository"]["url"]), (owner, name, f"https://github.com/{owner}/{name}"), tool_id)
            self.assertEqual((r["license"]["state"], r["license"]["spdxId"]), (lic, spdx), tool_id)
            self.assertEqual(r["sourceSnapshot"]["immutableCommit"], commit, tool_id)
            self.assertEqual((r["sourceSnapshot"]["revisionState"], r["sourceSnapshot"]["observedRef"], r["sourceSnapshot"]["observedAt"]), ("immutable-commit", "main", "2026-10-25T00:00:00Z"), tool_id)
            self.assertIsNone(r["sourceSnapshot"]["releaseVersion"], f"{tool_id}: no release is invented")
            self.assertEqual(r["sourceSnapshot"]["evidenceIds"], [evidence], tool_id)
            self.assertEqual((r["reviewState"], w["executionPolicy"]), (review, policy), tool_id)
            self.assertEqual((w["pluginIdentity"], w["sourcePath"]), (None, None), tool_id)
            self.assertIsNone(r["artifactEvidence"], f"{tool_id}: no checksum or signature is invented")

    def test_the_karpathy_licence_is_review_required_without_invented_proof(self):
        lic = self.by_id["multica-ai-andrej-karpathy-skills"]["record"]["license"]
        self.assertEqual(lic, {"state": "review-required", "spdxId": None, "evidenceId": None})
        notes = " ".join(self.by_id["multica-ai-andrej-karpathy-skills"]["record"]["privacy"]["notes"])
        for needle in ("no LICENSE file", "GitHub detects none", "no independently reviewable licence grant", "no SPDX identifier is asserted"):
            self.assertIn(needle, notes)

    def test_identity_fields_never_name_the_former_owner_and_the_text_never_claims_karpathy_authorship(self):
        w = self.by_id["multica-ai-andrej-karpathy-skills"]
        r = w["record"]
        identity = json.dumps([r["toolId"], r["repository"], w["pluginIdentity"], w["sourcePath"]])
        self.assertNotIn("forrestchang", identity)
        text = json.dumps(r, ensure_ascii=False)
        for needle in ("former owner", "must not be used", "stale", "not authored or endorsed by him"):
            self.assertIn(needle, text)
        self.assertIn("Karpathy’den esinlenmiş, üçüncü taraf", w["purpose"])
        for bad in ("Karpathy tarafından", "onaylı", "resmî", "resmi"):
            self.assertNotIn(bad, w["purpose"])

    def test_quarantine_privacy_and_packaging_statements(self):
        cm = self.by_id["thedotmack-claude-mem"]["record"]
        notes = " ".join(cm["privacy"]["notes"])
        for needle in ("High privacy impact", "Local storage", "hosted options are different", "none of that may be assumed for other clients", "Quarantined"):
            self.assertIn(needle, notes)
        self.assertTrue(cm["privacy"]["mayTransmitProjectContent"])
        self.assertIn("not reconciled", " ".join(cm["dependencies"]))
        rf = self.by_id["ruvnet-ruflo"]["record"]
        notes = " ".join(rf["privacy"]["notes"])
        for needle in ("not 'all modules installed'", "47 separate plugin modules", "High runtime privilege", "Packaging identity is ambiguous"):
            self.assertIn(needle, notes)
        self.assertEqual([m["methodId"] for m in rf["installMethods"]], ["unknown"])
        self.assertIn("claude-flow", " ".join(rf["dependencies"]))
        ua = self.by_id["egonex-ai-understand-anything"]["record"]
        self.assertIn("pipe-to-shell", json.dumps(ua))
        self.assertIn("must never be run or reproduced", json.dumps(ua))
        for tool_id, w in self.by_id.items():
            self.assertEqual(w["record"]["privacy"]["telemetry"], "unknown", tool_id)

    def test_compatibility_only_one_declared_cell_and_all_six_stated(self):
        every = {(p, s) for p in ("windows-x64", "macos-arm64") for s in ("local-cli", "claude-code-terminal", "antigravity-ide")}
        declared = []
        for tool_id, w in self.by_id.items():
            cells = {(t["platformTarget"], t["agentSurface"]): t for t in w["record"]["targetEvidence"]}
            self.assertEqual(set(cells), every, tool_id)
            for key, t in cells.items():
                self.assertIn(t["compatibilityState"], {"vendor-declared", "unknown"}, tool_id)
                self.assertGreater(len(t.get("limitation", "")), 20, f"{tool_id} {key}: every label carries its limitation")
                for field in ("independentCheckId", "harnessVersion", "testedAt", "evidenceDigest"):
                    self.assertNotIn(field, t)
                if t["compatibilityState"] == "vendor-declared":
                    declared.append((tool_id, key))
        self.assertEqual(declared, [("thedotmack-claude-mem", ("windows-x64", "claude-code-terminal"))])

    def test_summaries_are_standalone_turkish_prose_within_the_gate(self):
        for tool_id, w in self.by_id.items():
            p = w["purpose"]
            self.assertTrue(100 <= len(p) <= 300, f"{tool_id}: {len(p)}")
            sentences = p.count(". ") + int(p.endswith("."))
            self.assertIn(sentences, (2, 3), tool_id)
            self.assertIn("bu uygulama", p.lower(), tool_id)
            for bad in ("http", "npm ", "npx", "```", "curl", "README"):
                self.assertNotIn(bad, p, tool_id)
        for tool_id in ("thedotmack-claude-mem", "ruvnet-ruflo"):
            self.assertIn("karantinada", self.by_id[tool_id]["purpose"])

    def test_every_record_meets_the_machine_checked_quality_criteria_except_the_documented_licence_exception(self):
        for tool_id, w in self.by_id.items():
            r = w["record"]
            gaps = []
            if not (r["license"]["state"] == "known" and r["license"]["spdxId"] and r["license"]["evidenceId"]):
                gaps.append("Q2")
            if not (r["sourceSnapshot"]["immutableCommit"] and r["sourceSnapshot"]["observedAt"]):
                gaps.append("Q3")
            if not r.get("dependencies"):
                gaps.append("Q4")
            if not (r.get("networkClaims") and r["privacy"].get("notes")):
                gaps.append("Q5")
            if not all(t.get("limitation") for t in r["targetEvidence"]):
                gaps.append("Q6")
            self.assertEqual(gaps, ["Q2"] if tool_id == "multica-ai-andrej-karpathy-skills" else [], tool_id)

    def test_no_executable_text_secret_or_unexpected_url(self):
        for tool_id, w in self.by_id.items():
            text = json.dumps(w, ensure_ascii=False)
            for needle in FORBIDDEN_TEXT:
                self.assertNotIn(needle, text, f"{tool_id} contains {needle!r}")
            for m in w["record"]["installMethods"]:
                self.assertIn(m["methodId"], {"unknown", "native-plugin-manager"}, tool_id)
                self.assertNotIn("command", m)
            for url in re.findall(r"https?://[^\s\"')]+", text):
                self.assertIn(url.rstrip(".,;"), ALLOWED_URLS, f"{tool_id}: {url}")

    def test_evidence_ids_are_registered_in_the_research_register(self):
        for tool_id, w in self.by_id.items():
            r = w["record"]
            ids = set(r["sourceSnapshot"]["evidenceIds"]) | {r["privacy"]["evidenceId"]} | {t["evidenceId"] for t in r["targetEvidence"]}
            ids |= {m["evidenceId"] for m in r["installMethods"]} | {c["evidenceId"] for c in r["verificationClaims"]}
            if r["license"]["evidenceId"]:
                ids.add(r["license"]["evidenceId"])
            for evidence in ids:
                self.assertIn(f"| {evidence} |", self.register, f"{tool_id}: {evidence}")
        for evidence, tool_id in (("R41", "egonex-ai-understand-anything"), ("R42", "thedotmack-claude-mem"), ("R43", "multica-ai-andrej-karpathy-skills"), ("R44", "ruvnet-ruflo")):
            row = next(l for l in self.register.splitlines() if l.startswith(f"| {evidence} |"))
            self.assertIn(EXPECTED[tool_id][6], row)
            self.assertIn("Nothing was installed", row)

    def test_the_earlier_files_are_unchanged(self):
        self.assertEqual(lf_sha256("data/catalog-seed.json"), "8341414b7671630e92fd7ac1a86916299434fcbc56a8475d9c74cfa129c73ad2")
        self.assertEqual(lf_sha256("data/catalog-candidates-1b.json"), "235b1ef8754f094f77287ef3ba52f793ac29390a61570236a7e8e01e613ba11e")
        self.assertEqual(lf_sha256("data/catalog-candidates-c01.json"), "2d1a104f31946dbb26f3134bfb511cdb55d8f6f2fe64496b08b9268fbf65278e")
        self.assertEqual(lf_sha256("data/catalog-candidates-c02.json"), "fac409c6c98eccfc42dc748ef986c0e4dde77fa547494b8814a00dd68cf4b9da")

    def test_the_rust_loader_expects_exactly_these_files_and_counts(self):
        src = (ROOT / "src-tauri" / "src" / "catalog.rs").read_text(encoding="utf-8")
        self.assertIn('include_str!("../../data/catalog-candidates-c03.json")', src)
        for const in ("PHASE0_TOOL_COUNT: usize = 9", "CANDIDATE_COUNT: usize = 4", "CATALOG01_COUNT: usize = 3", "CATALOG02_COUNT: usize = 2", "CATALOG03_COUNT: usize = 4"):
            self.assertIn(f"pub const {const};", src)


if __name__ == "__main__":
    unittest.main()
