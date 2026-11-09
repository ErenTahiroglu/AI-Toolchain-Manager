"""CATALOG-04 data contract (Elephant-Goldfish Model, one metadata-only record), no running app needed:

* P1: the record satisfies the catalog schema, is the only record of its file, collides with no earlier id and carries no executable text, secret or unexpected URL,
* P2: its evidence id is registered in the research register with the full pinned commit and the words "Nothing was installed",
* P3: the new file and the five earlier data files are byte-for-byte (line-ending-normalised) the approved versions.

The Rust wiring and the numeric pins of the older tests belong to a later gate; this file only reads data files and the register.
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
ALLOWED_URLS = {"https://github.com/google/elephant-goldfish-model"}
COMMIT = "e9e1f06aa4e26e95cd4ed37210a0632202c429e9"


def load(rel):
    return json.loads((ROOT / rel).read_text(encoding="utf-8"))


def lf_sha256(rel):
    return hashlib.sha256((ROOT / rel).read_bytes().replace(b"\r\n", b"\n")).hexdigest()


class Catalog04Record(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.schema = load("schemas/catalog-entry.schema.json")
        cls.file = load("data/catalog-candidates-c04.json")
        cls.wrappers = cls.file["records"]
        cls.wrapper = cls.wrappers[0]
        cls.record = cls.wrapper["record"]
        earlier = {r["toolId"] for r in load("data/catalog-seed.json")["records"]}
        for f in ("1b", "c01", "c02", "c03"):
            earlier |= {w["record"]["toolId"] for w in load(f"data/catalog-candidates-{f}.json")["records"]}
        cls.earlier = earlier
        cls.register = (ROOT / "docs" / "RESEARCH-SOURCES.md").read_text(encoding="utf-8")

    def test_the_record_is_valid_and_carries_no_executable_text(self):
        jsonschema.Draft202012Validator(self.schema, format_checker=jsonschema.FormatChecker()).validate(self.record)
        self.assertEqual(len(self.wrappers), 1)
        self.assertEqual(len(self.earlier), 22)
        self.assertNotIn(self.record["toolId"], self.earlier, "the id collides with none of the five earlier files")
        text = json.dumps(self.wrapper, ensure_ascii=False)
        for needle in FORBIDDEN_TEXT:
            self.assertNotIn(needle, text, f"the record contains {needle!r}")
        for url in re.findall(r"https?://[^\s\"')]+", text):
            self.assertIn(url.rstrip(".,;"), ALLOWED_URLS, url)

    def test_the_evidence_id_is_registered_with_the_full_commit(self):
        row = next((l for l in self.register.splitlines() if l.startswith("| R57 |")), None)
        self.assertIsNotNone(row, "the register has an R57 row")
        self.assertIn(COMMIT, row, "the row names the pinned commit in full")
        self.assertIn("Nothing was installed", row)
        r = self.record
        ids = set(r["sourceSnapshot"]["evidenceIds"]) | {r["privacy"]["evidenceId"]} | {t["evidenceId"] for t in r["targetEvidence"]}
        ids |= {m["evidenceId"] for m in r["installMethods"]} | {c["evidenceId"] for c in r["verificationClaims"]}
        if r["license"]["evidenceId"]:
            ids.add(r["license"]["evidenceId"])
        for evidence in ids:
            self.assertIn(f"| {evidence} |", self.register, evidence)

    def test_the_c04_file_and_the_five_earlier_data_files_are_unchanged(self):
        self.assertEqual(lf_sha256("data/catalog-seed.json"), "8341414b7671630e92fd7ac1a86916299434fcbc56a8475d9c74cfa129c73ad2")
        self.assertEqual(lf_sha256("data/catalog-candidates-1b.json"), "235b1ef8754f094f77287ef3ba52f793ac29390a61570236a7e8e01e613ba11e")
        self.assertEqual(lf_sha256("data/catalog-candidates-c01.json"), "2d1a104f31946dbb26f3134bfb511cdb55d8f6f2fe64496b08b9268fbf65278e")
        self.assertEqual(lf_sha256("data/catalog-candidates-c02.json"), "fac409c6c98eccfc42dc748ef986c0e4dde77fa547494b8814a00dd68cf4b9da")
        self.assertEqual(lf_sha256("data/catalog-candidates-c03.json"), "ee5bd368ba8c4d523c18ad6df8fc03e51f8c45ce0201a8011cbcb077c9ddd526")
        self.assertEqual(lf_sha256("data/catalog-candidates-c04.json"), "3aba180a37978934a83112a389170cc45ac6181ff7647d5ff7bf2082744c7038")


if __name__ == "__main__":
    unittest.main()
