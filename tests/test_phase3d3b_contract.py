"""Phase 3D-3B contract: the offline source decision, the review package, the staging manifest and the fixture.

Offline, repository files only. Nothing here downloads, installs, opens or runs jq, and nothing here marks anything reviewed.
"""
import hashlib
import json
import re
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src-tauri" / "src"
D = ROOT / "docs" / "phase-3d3b"
sys.path.insert(0, str(ROOT / "tools"))
import staging_fixture  # noqa: E402

WIN_SHA = "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627"


def text(p):
    return Path(p).read_text(encoding="utf-8").replace("\r\n", "\n")


def jl(p):
    return json.loads(text(p))


def canonical(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def sha(b):
    return hashlib.sha256(b).hexdigest()


class OfflineSource(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cand = jl(ROOT / "docs/phase-3d3/jq-1.8.2-windows-x64.recipe.candidate.json")

    def test_the_candidate_declares_the_offline_origin_and_has_no_network_at_all(self):
        r = self.cand["recipe"]
        self.assertEqual(r["artifact"]["origin"], "local_retained_file")
        self.assertEqual(r["network"], [])
        self.assertNotIn("network_download", r["permissions"])
        # the first step keeps the existing operation kind: a logical acquisition step, not a downloader
        self.assertEqual(r["steps"][0], {"operation": "fetch-release-artifact", "artifactId": "primary"})
        self.assertEqual(r["revision"], 2)

    def test_the_recipe_digest_is_the_independent_python_computation_and_the_review_is_untouched(self):
        pinned = jl(D / "evidence" / "candidate-recipe-digest.json")
        self.assertEqual(sha(canonical(self.cand["recipe"]).encode("utf-8")), pinned["recipeDigest"])
        rv = self.cand["review"]
        self.assertEqual((rv["state"], rv["reviewerId"], rv["reviewedRecipeDigest"], rv["stagingEvidence"]), ("draft", "none-yet", "", []))

    def test_no_data_file_of_the_package_claims_an_independent_check(self):
        for p in D.rglob("*.json"):
            body = text(p)
            self.assertNotRegex(body, r'"independent"\s*:\s*true', p.name)
            self.assertNotRegex(body, r'"state"\s*:\s*"reviewed"', p.name)

    def test_the_schema_knows_the_origin_and_nothing_else_new(self):
        schema = text(ROOT / "schemas" / "reviewed-recipe.schema.json")
        self.assertIn('"origin": { "enum": ["local_retained_file"]', schema)
        json.loads(schema)

    def test_the_plan_vocabulary_has_the_local_texts_and_the_old_texts_are_untouched(self):
        plan = text(SRC / "plan.rs")
        self.assertIn("YEREL bir dosyadan okunacaktı (indirme yok; ağ kullanılmaz)", plan)
        self.assertIn('OperationKind::FetchReleaseArtifact => "Yayıncının sürüm dosyası, onaylı tarifte yazılı adresten indirilecekti."', plan)
        self.assertIn('skip_serializing_if = "Option::is_none"', plan)
        recipe = text(SRC / "recipe.rs")
        self.assertIn("artifact_source_undeclared", recipe)
        self.assertIn("artifact_origin_conflict", recipe)
        self.assertIn('#[serde(default, skip_serializing_if = "Option::is_none")]\n    pub origin: Option<String>,', recipe)

    def test_the_workflow_never_reads_recipe_steps(self):
        for name in ["adapter.rs", "install_flow.rs", "artifact_provider.rs", "consent.rs"]:
            body = "\n".join(l for l in text(SRC / name).split("#[cfg(test)]\n#[path")[0].splitlines() if not l.strip().startswith("//"))
            for needle in ["FetchReleaseArtifact", "fetch-release-artifact", ".steps", "OperationKind"]:
                self.assertNotIn(needle, body, f"{needle} in {name}")


class Fixture(unittest.TestCase):
    def test_the_fixture_is_rebuilt_without_the_application_and_matches_its_pinned_identity(self):
        fx = jl(D / "staging-fixture.json")
        self.assertEqual(sha(staging_fixture.PAYLOAD), fx["payloadSha256"])
        for p, want in fx["platforms"].items():
            b = staging_fixture.fixture(p)
            self.assertEqual((len(b), sha(b)), (want["sizeBytes"], want["sha256"]), p)
        # it is not jq and says so
        self.assertNotEqual(fx["platforms"]["windows-x64"]["sha256"], WIN_SHA)
        self.assertIn(b"not jq", staging_fixture.fixture("windows-x64"))

    def test_the_reference_entry_is_exactly_the_target_and_has_no_extra_field(self):
        e = jl(D / "evidence" / "reference-entry.json")
        self.assertEqual(set(e), {"entryId", "kind", "format", "platformTarget", "version", "sha256", "authentication"})
        self.assertEqual((e["entryId"], e["version"], e["platformTarget"], e["format"], e["sha256"]), ("jq", "1.8.2", "windows-x64", "pe-x64", WIN_SHA))
        # nothing is compiled in
        self.assertIn("pub const BUILT_IN_AUTHENTICATED: &[BuiltIn] = &[];", text(SRC / "discovery" / "reference.rs"))


class StagingManifest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.m = jl(D / "staging-manifest.json")
        cls.sources = {p.name: text(p) for p in SRC.rglob("*.rs")}

    def test_every_requirement_of_the_issue_is_covered_and_every_identifier_is_a_real_non_ignored_test(self):
        self.assertEqual([r["id"] for r in self.m["requirements"]], [f"S{i}" for i in range(1, 13)])
        ids = [t for r in self.m["requirements"] for t in r["cargo"]]
        self.assertEqual(len(ids), len(set(ids)), "an identifier is listed twice")
        self.assertGreaterEqual(len(ids), 40)
        for ident in ids:
            name = ident.split("::")[-1]
            hits = [f for f, body in self.sources.items() if re.search(rf"(?m)^\s*(pub\s+)?fn {re.escape(name)}\(", body)]
            self.assertEqual(len(hits), 1, f"{ident}: found in {hits}")
            body = self.sources[hits[0]]
            at = re.search(rf"(?m)^\s*(pub\s+)?fn {re.escape(name)}\(", body).start()
            self.assertNotIn("#[ignore", body[max(0, at - 160): at], f"{ident} is ignored")
            module = ident.split("::")[0]
            self.assertIn(module, {"recipe", "attestation", "install_flow", "artifact_provider", "consent"})

    def test_the_frontend_titles_exist(self):
        bodies = "\n".join(text(ROOT / f) for f in self.m["frontend"]["files"])
        for t in self.m["frontend"]["titles"]:
            self.assertIn(t, bodies)

    def test_what_cannot_be_reproduced_in_ci_is_stated(self):
        n = " ".join(self.m["notReproducibleInCi"])
        for needle in ["real application window", "Smart App Control", "network isolation"]:
            self.assertIn(needle, n)

    def test_the_runner_reports_blocked_runs_as_not_run_and_never_retries_or_rebuilds(self):
        r = text(ROOT / "tools" / "run_staging.py")
        self.assertIn("blocked_not_run", r)
        self.assertIn("os error 4551", r)
        self.assertNotIn("--force", r)
        self.assertEqual(r.count('"cargo", "test"'), 1, "cargo is started exactly once, never in a retry loop")
        for needle in ["urllib", "requests", "socket", "Set-MpPreference"]:
            self.assertNotIn(needle, r)


class ReviewPackage(unittest.TestCase):
    PKG = D / "review-package.json"

    @unittest.skipUnless((D / "review-package.json").exists(), "the package is added in the commit after the candidate commit")
    def test_the_package_verifies_and_marks_nothing_reviewed(self):
        pkg = jl(self.PKG)
        self.assertEqual(pkg["reviewState"]["recipeReviewState"], "draft")
        self.assertEqual(pkg["reviewState"]["reviewerId"], "none-yet")
        self.assertEqual(pkg["reviewState"]["stagingEvidence"], [])
        cls = pkg["evidenceClasses"]
        self.assertTrue(any("NOTHING ELSE" in x for x in cls["3_independentlyReviewedByChatGPT"]))
        self.assertTrue(cls["4_awaitingIndependentReview"])
        self.assertEqual(pkg["ci"]["headSha"], pkg["candidateCommit"])
        out = subprocess.run([sys.executable, str(ROOT / "tools" / "verify_review_package.py")], capture_output=True, text=True, cwd=ROOT)
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.assertTrue(json.loads(out.stdout)["allPassed"])

    def test_the_real_window_run_of_this_phase_is_all_true_and_states_the_local_origin(self):
        res = jl(D / "evidence" / "gui" / "first-result.json")
        self.assertEqual(res["passed"], res["total"])
        self.assertTrue(any("LOCAL retained file" in r["label"] and r["value"] is True for r in res["results"]))
        after = jl(D / "evidence" / "gui" / "after-restart-result.json")
        self.assertEqual(after["passed"], after["total"])
        diff = jl(D / "evidence" / "gui" / "inventory-diff-start-to-final.json")
        self.assertEqual(diff["removed"], [])
        self.assertTrue(all(k.startswith("atm-managed") for k in diff["added"]))
        guard = jl(D / "evidence" / "egress-guard-staging-run.json")
        self.assertEqual((guard["childExit"], guard["contactedHosts"], guard["incidents"], guard["guard"]["allowedHosts"]), (0, [], [], []))

    def test_the_review_document_separates_the_four_classes_and_pre_fills_nothing(self):
        body = text(D / "REVIEW-PACKAGE.md")
        for needle in ["Developer-generated", "CI-generated", "Independently reviewed by ChatGPT", "Awaiting independent review", "nothing here is pre-filled", "draft"]:
            self.assertIn(needle, body.replace("**", ""), needle)
        self.assertNotRegex(body, r"(?i)\breviewed by [A-Z][a-z]+ [A-Z][a-z]+")
        self.assertIn("python tools/verify_review_package.py", body)

    def test_documents_link_to_existing_files(self):
        link = re.compile(r"\]\(([^)#]+)(?:#[^)]*)?\)")
        for f in [D / "REVIEW-PACKAGE.md", ROOT / "docs" / "PHASE-3D3B.md"]:
            self.assertTrue(f.exists(), f)
            for target in link.findall(text(f)):
                if target.startswith(("http://", "https://")):
                    continue
                if target == "review-package.json" and not self.PKG.exists():
                    continue
                self.assertTrue((f.parent / target).resolve().exists(), f"{f.name}: {target}")


class ProductionStaysClosed(unittest.TestCase):
    def test_every_list_that_must_stay_empty_is_empty(self):
        self.assertEqual(text(SRC / "recipe.rs").count('pub const BUILT_IN_RECIPES_JSON: &str = "[]";'), 1)
        self.assertIn('{"attestations":[],"localUseLicenses":[]}', text(SRC / "attestation.rs"))
        flow = text(SRC / "install_flow.rs")
        self.assertIn("evidence: Evidence::production(),\n            pilot_gate: Vec::new(),", flow)
        for p in SRC.glob("*.rs"):
            self.assertNotIn(WIN_SHA, text(p), p.name)

    def test_ci_runs_the_staging_runner_and_never_enables_a_pilot_feature_in_the_release_build(self):
        ci = text(ROOT / ".github" / "workflows" / "ci.yml")
        self.assertIn("tools/run_staging.py", ci)
        for line in ci.splitlines():
            if "tauri build" in line:
                self.assertNotIn("pilot", line)


if __name__ == "__main__":
    unittest.main()
