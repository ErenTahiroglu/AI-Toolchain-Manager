"""CATALOG-05 data contract (Unslop, one metadata-only record, Issue #56), no running app needed:

* P1: the record satisfies the catalog schema, is the only record of its file, collides with no earlier id or source and carries no executable text, secret or
  unexpected URL,
* P2: its evidence id is registered in the research register with the full pinned commit and the words "Nothing was installed",
* P3: the new file and the six earlier data files are byte-for-byte (line-ending-normalised) the approved versions,
* P4: the safe states (unreviewed, blocked-until-scoped-authorization, unknown compatibility, unknown telemetry, no independent test) hold, and the Rust wiring adds
  exactly one entry and no capability, command, dependency or workflow.

The Rust runtime behaviour belongs to `src-tauri/src/catalog_c05_tests.rs`; this file only reads data files, the register and source text.
"""
import hashlib
import json
import pathlib
import re
import subprocess
import unittest

import jsonschema

ROOT = pathlib.Path(__file__).resolve().parents[1]
FORBIDDEN_TEXT = ["curl ", "wget ", "iwr ", "iex ", "irm ", "| bash", "| sh", "Invoke-Expression", "powershell -", "npm install", "npx ", "pip install", "git clone",
                  "cp -r", "mkdir ", "claude plugin", "/plugin install", "/plugin marketplace", "skill-installer", "API_KEY", "Authorization", "sk-", "ghp_"]
ALLOWED_URLS = {"https://github.com/heydamir/unslop"}
COMMIT = "620163eedb3529d9cd23a3994ed8d287d06f760d"
EARLIER_FILES = ("catalog-seed", "catalog-candidates-1b", "catalog-candidates-c01", "catalog-candidates-c02", "catalog-candidates-c03", "catalog-candidates-c04")


def read(rel):
    return (ROOT / rel).read_text(encoding="utf-8")


def load(rel):
    return json.loads(read(rel))


def lf_sha256(rel):
    return hashlib.sha256((ROOT / rel).read_bytes().replace(b"\r\n", b"\n")).hexdigest()


def tool_id(record):
    return record.get("toolId") or record["record"]["toolId"]


class Catalog05Record(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.schema = load("schemas/catalog-entry.schema.json")
        cls.file = load("data/catalog-candidates-c05.json")
        cls.wrappers = cls.file["records"]
        cls.wrapper = cls.wrappers[0]
        cls.record = cls.wrapper["record"]
        cls.earlier = []
        for f in EARLIER_FILES:
            for r in load(f"data/{f}.json")["records"]:
                rec = r.get("record", r)
                cls.earlier.append((rec["toolId"], f'{rec["repository"]["owner"]}/{rec["repository"]["name"]}'.lower()))
        cls.register = read("docs/RESEARCH-SOURCES.md")

    def test_the_record_is_valid_unique_and_carries_no_executable_text(self):
        jsonschema.Draft202012Validator(self.schema, format_checker=jsonschema.FormatChecker()).validate(self.record)
        self.assertEqual(len(self.wrappers), 1)
        self.assertEqual(len(self.earlier), 23)
        self.assertEqual(len({i for i, _ in self.earlier}), 23)
        r = self.record
        self.assertEqual(r["toolId"], "heydamir-unslop")
        self.assertNotIn(r["toolId"], {i for i, _ in self.earlier})
        key = f'{r["repository"]["owner"]}/{r["repository"]["name"]}'.lower()
        self.assertEqual(key, "heydamir/unslop")
        self.assertNotIn(key, {k for _, k in self.earlier}, "no second plugin-vs-skill record for the same source")
        self.assertIsNone(self.wrapper["sourcePath"])
        self.assertIsNone(self.wrapper["pluginIdentity"])
        text = json.dumps(self.wrapper, ensure_ascii=False)
        for needle in FORBIDDEN_TEXT:
            self.assertNotIn(needle, text, f"the record contains {needle!r}")
        for url in re.findall(r"https?://[^\s\"')]+", text):
            self.assertIn(url.rstrip(".,;"), ALLOWED_URLS, url)

    def test_the_states_are_the_safe_initial_ones_and_nothing_is_promoted(self):
        r, w = self.record, self.wrapper
        self.assertEqual((w["entryKind"], r["category"]), ("skill_collection", "agent-skill"))
        self.assertEqual((r["reviewState"], w["executionPolicy"]), ("unreviewed", "blocked-until-scoped-authorization"))
        self.assertEqual(r["privacy"]["telemetry"], "unknown")
        self.assertIs(r["privacy"]["mayTransmitProjectContent"], True)
        self.assertIsNone(r["artifactEvidence"])
        self.assertEqual(r["license"], {"state": "known", "spdxId": "CC-BY-4.0", "evidenceId": "R61"})
        snap = r["sourceSnapshot"]
        self.assertEqual((snap["immutableCommit"], snap["revisionState"], snap["evidenceIds"]), (COMMIT, "immutable-commit", ["R61"]))
        self.assertNotEqual(snap["observedRef"], "main", "no moving branch name is the observed ref")
        self.assertEqual({(c["kind"], c["state"]) for c in r["verificationClaims"]}, {("function", "publisher-describes"), ("independent-test", "not-run")})
        text = json.dumps(r)
        self.assertNotIn("independently-tested", text)
        self.assertNotIn("vendor-declared", text, "the publisher names hosts but no platform")
        self.assertNotIn("verified-by-manager", text)
        self.assertEqual([m["methodId"] for m in r["installMethods"]], ["native-plugin-manager", "manual-instructions"])
        for m in r["installMethods"]:
            self.assertEqual(set(m), {"methodId", "evidenceId", "publisherPin"}, "labels only")
        # the attribution the licence asks for
        notes = " ".join(r["privacy"]["notes"])
        for needle in ("Damir Sokolovsky", "CC BY 4.0", "https://github.com/heydamir/unslop"):
            self.assertIn(needle, notes)

    def test_compatibility_rows_are_exactly_the_three_documented_hosts_on_two_platforms_all_unknown(self):
        rows = self.record["targetEvidence"]
        pairs = sorted((t["platformTarget"], t["agentSurface"]) for t in rows)
        want = sorted((p, s) for p in ("windows-x64", "macos-arm64") for s in ("claude-code-terminal", "codex-cli", "antigravity-ide"))
        self.assertEqual(pairs, want)
        for t in rows:
            self.assertEqual(t["compatibilityState"], "unknown")
            self.assertTrue(t["limitation"])
            self.assertEqual(t["evidenceId"], "R61")
            for extra in ("independentCheckId", "harnessVersion", "testedAt"):
                self.assertNotIn(extra, t)
        surfaces = {t["agentSurface"] for t in rows}
        for inferred in ("codex-vscode", "cline-cli", "cline-vscode", "cline-jetbrains", "kiro-cli", "kiro-ide", "claude-desktop-code", "local-cli", "antigravity-cli"):
            self.assertNotIn(inferred, surfaces)

    def test_the_evidence_id_is_registered_with_the_full_commit_and_every_id_resolves(self):
        ids = [l.split("|")[1].strip() for l in self.register.splitlines() if re.match(r"\| R[0-9]{2,3} \|", l)]
        self.assertEqual(ids.count("R61"), 1, "R61 is registered exactly once")
        row = next(l for l in self.register.splitlines() if l.startswith("| R61 |"))
        self.assertIn(COMMIT, row, "the row names the pinned commit in full")
        self.assertIn("Nothing was installed", row)
        self.assertIn("CC BY 4.0", row)
        r = self.record
        used = set(r["sourceSnapshot"]["evidenceIds"]) | {r["privacy"]["evidenceId"], r["license"]["evidenceId"]} | {t["evidenceId"] for t in r["targetEvidence"]}
        used |= {m["evidenceId"] for m in r["installMethods"]} | {c["evidenceId"] for c in r["verificationClaims"]}
        self.assertEqual(used, {"R61"})
        for earlier in ("R57", "R58", "R59", "R60"):
            self.assertEqual(ids.count(earlier), 1, f"{earlier} was not touched")

    def test_the_c05_file_and_the_six_earlier_data_files_are_the_approved_versions(self):
        self.assertEqual(lf_sha256("data/catalog-seed.json"), "8341414b7671630e92fd7ac1a86916299434fcbc56a8475d9c74cfa129c73ad2")
        self.assertEqual(lf_sha256("data/catalog-candidates-1b.json"), "235b1ef8754f094f77287ef3ba52f793ac29390a61570236a7e8e01e613ba11e")
        self.assertEqual(lf_sha256("data/catalog-candidates-c01.json"), "2d1a104f31946dbb26f3134bfb511cdb55d8f6f2fe64496b08b9268fbf65278e")
        self.assertEqual(lf_sha256("data/catalog-candidates-c02.json"), "fac409c6c98eccfc42dc748ef986c0e4dde77fa547494b8814a00dd68cf4b9da")
        self.assertEqual(lf_sha256("data/catalog-candidates-c03.json"), "ee5bd368ba8c4d523c18ad6df8fc03e51f8c45ce0201a8011cbcb077c9ddd526")
        self.assertEqual(lf_sha256("data/catalog-candidates-c04.json"), "3aba180a37978934a83112a389170cc45ac6181ff7647d5ff7bf2082744c7038")
        self.assertEqual(lf_sha256("data/catalog-candidates-c05.json"), "9b6f8f65dea4446064960714340ea47aaed65ade9fa1abb87840e862a35fd764")

    def test_the_serialization_pin_in_the_rust_test_equals_the_record(self):
        canonical = json.dumps(self.record, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
        digest = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
        self.assertIn(f'"heydamir-unslop", "{digest}"', read("src-tauri/src/ece_ci_serialization_tests.rs"))
        # the method reproduces the pin of an earlier record
        egm = load("data/catalog-candidates-c04.json")["records"][0]["record"]
        egm_digest = hashlib.sha256(json.dumps(egm, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")).hexdigest()
        self.assertIn(egm_digest, read("src-tauri/src/ece_ci_serialization_tests.rs"))


class Catalog05Wiring(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.catalog = read("src-tauri/src/catalog.rs")
        cls.production = cls.catalog.split("#[cfg(test)]")[0]

    def test_exactly_one_entry_is_added_through_the_existing_pipeline(self):
        self.assertIn('const CATALOG05_JSON: &str = include_str!("../../data/catalog-candidates-c05.json");', self.production)
        self.assertIn("pub const CATALOG05_COUNT: usize = 1;", self.production)
        self.assertRegex(self.production, r"CATALOG04_COUNT \+ CATALOG05_COUNT;")
        self.assertIn("all.extend(catalog05_seed_entries()?);", self.production)
        self.assertEqual(self.production.count("wrapped_seed_entries(CATALOG05_JSON, CATALOG05_COUNT"), 1)
        for forbidden in ("std::process", "Command::", "std::net", "reqwest", "TcpStream", "std::fs", "File::open", "std::env"):
            self.assertNotIn(forbidden, self.production)

    def test_the_earlier_seed_wiring_and_the_test_module_registration(self):
        for n in (1, 2, 3, 4):
            self.assertIn(f"CATALOG0{n}_JSON", self.production)
        lib = read("src-tauri/src/lib.rs")
        self.assertIn("#[cfg(test)]\nmod catalog_c05_tests;", lib)
        self.assertNotIn("unslop", lib.lower())

    def test_no_privileged_layer_names_the_entry(self):
        for rel in ("plan.rs", "recipe.rs", "gate.rs", "config_txn.rs", "managed_fs_mcp.rs", "install_flow.rs", "projects.rs", "recommend.rs"):
            text = read(f"src-tauri/src/{rel}").split("#[cfg(test)]")[0].lower()
            self.assertNotIn("unslop", text, rel)
            self.assertNotIn("heydamir", text, rel)
        for rel in ("src/workspace.ts", "src/agentHosts.ts"):
            self.assertNotIn("unslop", read(rel).lower(), rel)

    def test_no_capability_dependency_workflow_migration_or_surface_list_changed(self):
        diff = subprocess.run(["git", "diff", "HEAD", "--name-only", "--", "src-tauri/capabilities", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml",
                               "src-tauri/Cargo.lock", "package.json", "package-lock.json", ".github", "tools", "schemas", "src-tauri/migrations", "src-tauri/src/plan.rs",
                               "src-tauri/src/projects.rs", "src-tauri/src/recipe.rs", "src/workspace.ts", "src/agentHosts.ts"],
                              cwd=ROOT, capture_output=True, text=True, check=True)
        self.assertEqual(diff.stdout.strip(), "", diff.stdout)


if __name__ == "__main__":
    unittest.main()
