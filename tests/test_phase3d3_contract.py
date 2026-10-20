"""Phase 3D-3 contract: audit honesty, the jq evidence records, the candidate recipe, and the gates that must not move.

Offline, repository files only. Nothing here downloads, installs, opens or runs jq.
"""
import hashlib
import json
import re
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src-tauri" / "src"
D3 = ROOT / "docs" / "phase-3d3"
WIN_SHA = "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627"
WIN_SIZE = 1035264


def text(path):
    return Path(path).read_text(encoding="utf-8").replace("\r\n", "\n")


def sha(path):
    # hash of the committed (LF) form of a text file, whatever the working copy's line endings are
    return hashlib.sha256(Path(path).read_bytes().replace(bytes([13, 10]), bytes([10]))).hexdigest()


def production(name):
    return text(SRC / name).split("#[cfg(test)]\n#[path")[0]


def jload(rel):
    return json.loads(text(D3 / rel))


class EvidenceRecords(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.att = jload("evidence/jq-1.8.2-windows-x64.source-attestation.json")
        cls.lic = jload("evidence/jq-1.8.2-windows-x64.local-use-license.json")
        cls.rec = jload("jq-1.8.2-windows-x64.recipe.candidate.json")

    def test_every_retained_evidence_hash_is_the_hash_of_a_retained_file(self):
        self.assertGreaterEqual(len(self.att["retainedEvidence"]), 5)
        for f in self.att["retainedEvidence"]:
            self.assertEqual(sha(ROOT / f["path"]), f["sha256"], f["path"])
        listing = ROOT / "docs/phase-3d0/evidence/request-a-session-20261018/session-sha256.txt"
        self.assertEqual(sha(listing), self.att["retainedSessionListingSha256"])
        self.assertIn(WIN_SHA, text(listing))

    def test_the_attestation_binds_the_exact_file_repository_workflow_and_commit(self):
        a = self.att
        self.assertEqual((a["toolId"], a["version"], a["platformTarget"]), ("jq", "1.8.2", "windows-x64"))
        self.assertEqual(a["artifact"], {"fileName": "jq-windows-amd64.exe", "sizeBytes": WIN_SIZE, "sha256": WIN_SHA, "format": "pe-x64"})
        self.assertEqual(a["repository"]["slug"], "jqlang/jq")
        self.assertEqual(a["source"], {"tag": "jq-1.8.2", "commit": "34f7186b86743a083a589741b6cea95293524108"})
        w = a["workflow"]
        self.assertEqual((w["path"], w["gitRef"], w["issuer"], w["runnerEnvironment"]), (".github/workflows/ci.yml", "refs/tags/jq-1.8.2", "https://token.actions.githubusercontent.com", "github-hosted"))
        v = a["verification"]
        self.assertTrue(v["officialVerifierPassed"] and v["exactBytesVerified"] and v["gpgSignatureValid"])
        self.assertEqual((v["checksTotal"], v["checksFailed"]), (151, 0))
        # the recorded incident is not hidden
        self.assertFalse(v["networkPolicyCompliant"])
        self.assertIn("tmaproduction.blob.core.windows.net", v["networkIncident"])
        self.assertEqual(v["offlineReplayContactedHosts"], 0)
        # the retained session's own review says the same thing about the network and does not claim authorisation to install or run
        review = json.loads(text(ROOT / "docs/phase-3d0/evidence/request-a-session-20261018/review-v2.json"))
        self.assertFalse(review["summary"]["network_policy_compliant"])
        self.assertFalse(review["summary"]["installation_authorized"] or review["summary"]["execution_authorized"])

    def test_the_local_use_licence_is_scoped_to_one_file_one_notice_text_and_one_use(self):
        l = self.lic
        self.assertEqual((l["scope"], l["decision"]), ("local_use_no_redistribution", "LICENSE_PASS_LOCAL_USE"))
        self.assertEqual(l["artifactSha256"], WIN_SHA)
        self.assertEqual(sha(ROOT / "docs/phase-3d2/notices/THIRD-PARTY-NOTICES.jq-1.8.2.windows-x64.txt"), l["noticeBundleSha256"])
        self.assertEqual(sha(ROOT / l["reviewRecordPath"]), l["reviewRecordSha256"])
        self.assertEqual({c["id"] for c in l["conditions"]}, {"C1", "C2", "C3", "C4", "C5"})
        self.assertEqual({q["id"] for q in l["openQuestions"]}, {"Q1", "Q2", "Q3"})
        self.assertTrue(all(not q["blocksLocalUse"] and q["blocksRedistribution"] for q in l["openQuestions"]))
        self.assertEqual(l["evidenceId"], "R34")
        self.assertIn("| R34 |", text(ROOT / "docs" / "RESEARCH-SOURCES.md"))

    def test_the_candidate_is_inert_until_an_independent_review_and_never_says_accepted(self):
        r = self.rec["recipe"]
        self.assertEqual(r["license"]["state"], "accepted_local_use")
        self.assertNotEqual(r["license"]["state"], "accepted")
        self.assertEqual(r["network"], [])
        self.assertNotIn("network_download", r["permissions"])
        self.assertEqual(r["lifecycleActions"], ["install", "uninstall"])
        self.assertEqual(r["artifact"]["sha256"], WIN_SHA)
        self.assertEqual(r["steps"][1]["expectedSha256"], WIN_SHA)
        self.assertEqual(r["revision"], 2)
        self.assertEqual(self.rec["review"]["state"], "draft", "only an independent reviewer may set it")
        self.assertEqual(self.rec["review"]["stagingEvidence"], [])
        self.assertTrue(r["source"]["sourceAttested"])
        self.assertIn({"kind": "source_attestation_evidence", "evidenceId": self.att["evidenceId"]}, r["authentication"])
        # the draft of Phase 3D-1 is untouched and still says the licence is unknown
        draft = json.loads(text(ROOT / "docs/phase-3d1/jq-1.8.2-windows-x64.recipe.draft.json"))
        self.assertEqual(draft["recipe"]["license"]["state"], "unknown")

    def test_the_schema_admits_exactly_the_two_new_values(self):
        schema = json.loads(text(ROOT / "schemas/reviewed-recipe.schema.json"))
        recipe = schema["properties"]["recipe"] if "recipe" in schema.get("properties", {}) else schema
        blob = json.dumps(schema)
        self.assertIn('"accepted", "accepted_local_use"', blob)
        self.assertIn("source_attestation_evidence", blob)

    def test_the_generator_is_offline_and_never_opens_the_retained_executable(self):
        gen = text(ROOT / "tools" / "build_phase3d3_evidence.py")
        for needle in ["urllib", "requests", "socket", "subprocess", "http.client", ".exe\").read_bytes", "jq-windows-amd64.exe\").open"]:
            self.assertNotIn(needle, gen)


class NothingIsActivated(unittest.TestCase):
    def test_the_compiled_in_lists_are_empty_and_the_enablement_list_is_empty(self):
        self.assertEqual(text(SRC / "recipe.rs").count('pub const BUILT_IN_RECIPES_JSON: &str = "[]";'), 1)
        self.assertIn('pub const BUILT_IN_EVIDENCE_JSON: &str = r#"{"attestations":[],"localUseLicenses":[]}"#;', text(SRC / "attestation.rs"))
        self.assertIn("pub const BUILT_IN_AUTHENTICATED: &[BuiltIn] = &[];", text(SRC / "discovery" / "reference.rs"))
        flow = production("install_flow.rs")
        self.assertIn("evidence: Evidence::production(),\n            pilot_gate: Vec::new(),", flow)
        for name in ["install_flow.rs", "commands.rs", "lib.rs", "state.rs", "recipe.rs", "plan.rs", "adapter.rs", "lifecycle.rs"]:
            body = production(name)
            self.assertNotIn("phase-3d3", body, name)
            self.assertNotIn("recipe.candidate.json", body, name)
            self.assertNotIn("RetainedFileProvider", body if name != "install_flow.rs" else "", name)

    def test_no_tracked_file_is_or_contains_the_jq_executable(self):
        out = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True).stdout.decode("utf-8", "replace")
        checked = 0
        for rel in filter(None, out.split("\0")):
            p = ROOT / rel
            if not p.is_file() or p.stat().st_size > 40 * 1024 * 1024:
                continue
            if p.stat().st_size == WIN_SIZE:
                self.assertNotEqual(sha(p), WIN_SHA, f"{rel} IS the jq executable: the project must not host it")
            checked += 1
        self.assertGreater(checked, 100)

    def test_the_jq_digest_is_still_never_a_literal_in_rust(self):
        for p in SRC.glob("*.rs"):
            self.assertNotIn(WIN_SHA, text(p), p.name)

    def test_the_acting_adapter_and_the_enablement_gate_exist_only_where_expected(self):
        adapter = production("adapter.rs")
        self.assertIn('#[cfg(any(test, feature = "synthetic-pilot", feature = "jq-pilot"))]\n    pub fn with_pilot_gate', adapter)
        self.assertIn('#[cfg(any(test, feature = "synthetic-pilot", feature = "jq-pilot"))]\n    pub fn mvp_synthetic', adapter)
        life = production("lifecycle.rs")
        self.assertIn("let synthetic_ok = r.synthetic && r.tool_id.starts_with(\"synthetic-\");", life)
        self.assertIn("!synthetic_ok && !gate.iter().any(|g| g.allows(a.recipe))", life)
        self.assertIn("not_a_synthetic_recipe", life)
        # the path-based synthetic engine can never apply a real recipe
        self.assertIn("check_authority_gated(a, action, computer_id, host_platform, now, &[])", life)
        flow = production("install_flow.rs")
        self.assertIn('#[cfg(not(any(test, feature = "synthetic-pilot", feature = "jq-pilot")))]\n    fn adapter<', flow)
        self.assertIn('Err("production_adapter_not_enabled")', flow)

    def test_ci_never_builds_the_release_with_a_pilot_feature(self):
        ci = text(ROOT / ".github" / "workflows" / "ci.yml")
        for line in ci.splitlines():
            if "tauri build" in line:
                self.assertNotIn("pilot", line)
        self.assertIn("--features synthetic-pilot", ci)

    def test_windows_hello_is_untouched(self):
        for name in ["install_flow.rs", "attestation.rs", "artifact_provider.rs", "consent.rs", "commands.rs"]:
            body = "\n".join(l for l in production(name).splitlines() if not l.strip().startswith("//"))
            for needle in ["KeyCredential", "os_hello", "os_approval", "ApprovalStore", "cng_verify", "device_trust"]:
                self.assertNotIn(needle, body, f"{needle} in {name}")


class AuditHonesty(unittest.TestCase):
    def test_no_audit_result_is_discarded_in_the_workflow(self):
        flow = production("install_flow.rs")
        self.assertNotRegex(flow, r"let _ = audit::append", "audit::append's result must be read")
        self.assertIn("fn audit(conn: &Connection, event: NewEvent) -> bool", flow)
        self.assertIn('return Err("audit_unavailable");', flow)
        self.assertIn("audit_outcome_not_persisted", flow)
        self.assertIn("install-audit-reconciled", flow)
        # the compiler enforces it: ignoring the answer is an unused-result error under `clippy -D warnings`
        self.assertIn('#[must_use = "an audit row that was not written must be handled"]\n    fn audit(conn: &Connection, event: NewEvent) -> bool', flow)

    def test_the_result_type_says_whether_the_row_was_written(self):
        flow = production("install_flow.rs")
        self.assertIn("pub audit_recorded: bool,", flow)
        self.assertIn('"needs_attention"', flow)
        ts = text(ROOT / "src" / "types.ts")
        self.assertIn("auditRecorded: boolean;", ts)
        self.assertIn('"needs_attention"', ts)


class Documents(unittest.TestCase):
    NAMES = [
        "AUDIT-INTEGRITY.md", "LICENSE-LOCAL-USE-REVIEW.md", "READINESS-DECISION.md", "PILOT-PROCEDURE.md", "ISSUE-14-PILOT-DECISION.md",
        "SMART-APP-CONTROL-RECORD.md", "GUI-EVIDENCE.md", "AUTHORIZATION-REQUEST.tr.md",
    ]

    def test_documents_exist_and_their_links_resolve(self):
        link = re.compile(r"\]\(([^)#]+)(?:#[^)]*)?\)")
        for f in [ROOT / "docs" / "PHASE-3D3.md"] + [D3 / n for n in self.NAMES]:
            self.assertTrue(f.exists(), f)
            for target in link.findall(text(f)):
                if target.startswith(("http://", "https://", "mailto:")):
                    continue
                self.assertTrue((f.parent / target).resolve().exists(), f"{f.name}: {target}")

    def test_the_authorization_request_is_a_proposal_that_excludes_execution(self):
        body = text(D3 / "AUTHORIZATION-REQUEST.tr.md")
        self.assertIn("VERİLMEDİ", body)
        self.assertIn("yürürlükte DEĞİL", body)
        self.assertIn("ÇALIŞTIRILMAYACAK", body)
        self.assertIn("Onay düğmesine yalnızca siz basarsınız", body)
        self.assertIn(WIN_SHA[:8], body)
        self.assertIn("Windows Hello", body)

    def test_the_readiness_decision_lists_the_blockers_and_is_a_no_go_today(self):
        body = text(D3 / "READINESS-DECISION.md")
        for needle in ["NO-GO", "B1", "B2", "B3", "independent", "enablement", "jq-pilot"]:
            self.assertIn(needle, body)
        self.assertNotRegex(body, r"(?i)\bGO\b for (the )?(real )?installation (now|today)")

    def test_the_license_review_names_its_single_assumption_and_keeps_redistribution_conditional(self):
        body = text(D3 / "LICENSE-LOCAL-USE-REVIEW.md")
        self.assertIn("LICENSE_PASS_LOCAL_USE", body)
        self.assertIn("LICENSE_CONDITIONAL", body)
        self.assertIn("not legal advice", body)
        self.assertIn("The one legal assumption", body)

    def test_the_sac_record_forbids_settings_changes_and_classifies_blocks(self):
        body = text(D3 / "SMART-APP-CONTROL-RECORD.md")
        self.assertIn("BLOCKED / NOT RUN", body)
        self.assertIn("No SAC or Defender setting is changed", body)

    def test_the_gui_evidence_run_is_all_true(self):
        res = json.loads(text(D3 / "evidence" / "gui" / "first-result.json"))
        self.assertEqual(res["passed"], res["total"])
        self.assertGreaterEqual(res["total"], 70)
        after = json.loads(text(D3 / "evidence" / "gui" / "after-restart-result.json"))
        self.assertEqual(after["passed"], after["total"])
        diff = json.loads(text(D3 / "evidence" / "gui" / "inventory-diff-start-to-final.json"))
        # outside the manager folder nothing was added, removed or changed except the application's own database files
        for key in diff["added"] + diff["removed"]:
            self.assertTrue(key.startswith("atm-managed"), key)
        self.assertEqual(diff["removed"], [])
        self.assertTrue(all(k.startswith("atm.sqlite") for k in diff["changed"]), diff["changed"])

    def test_the_gui_tools_change_no_security_setting(self):
        for name in ["gui_install_flow.ps1", "gui_install_flow.mjs", "fs_inventory.mjs"]:
            body = text(ROOT / "tools" / name)
            for needle in ["Set-MpPreference", "Add-MpPreference", "Set-ItemProperty", "reg add", "icacls", "takeown"]:
                self.assertNotIn(needle, body, f"{needle} in {name}")


if __name__ == "__main__":
    unittest.main()
