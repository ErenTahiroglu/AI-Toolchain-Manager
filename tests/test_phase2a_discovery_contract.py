"""Phase 2A: the BUILT binary's read-only discovery engine, checked by independent Python code.

`--self-check` builds a throw-away fake home, PATH folder and project folder (with dummy secrets) inside its
isolated directory, scans them with the real engine and reports what it observed. Set ATM_SELFCHECK_JSON to
that file; without it these tests are reported as SKIPPED (never as passed). CI always sets it.
"""
import json
import os
import pathlib
import unittest

SELFCHECK = os.environ.get("ATM_SELFCHECK_JSON")
FORBIDDEN_WORDS = ("install", "update", "approve", "verify", "rollback", "uninstall")


@unittest.skipUnless(SELFCHECK and pathlib.Path(SELFCHECK).is_file(), "ATM_SELFCHECK_JSON not provided; Rust self-check output not available")
class DiscoveryContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path = pathlib.Path(SELFCHECK)
        cls.raw = path.read_text(encoding="utf-8")
        cls.data = json.loads(cls.raw)
        cls.probe = cls.data["discoveryProbe"]
        cls.obs = cls.probe["observations"]

    def one(self, subject, method, scope):
        found = [o for o in self.obs if o["subject"] == subject and o["method"] == method and o["scope"] == scope]
        self.assertEqual(len(found), 1, f"{subject}/{method}/{scope}: {found}")
        return found[0]

    def test_scan_completed_twice_and_the_schema_version_includes_discovery(self):
        self.assertEqual(self.probe["scan"]["status"], "completed")
        self.assertNotEqual(self.probe["scan"]["scanId"], self.probe["secondScanId"])
        self.assertGreaterEqual(self.data["dbVersion"], 4)

    def test_nothing_in_the_scanned_tree_changed(self):
        self.assertTrue(self.probe["treeUnchanged"], "a read-only scan must leave sizes, contents and timestamps untouched")

    def test_no_dummy_secret_reached_observations_or_the_output_file(self):
        self.assertFalse(self.probe["dummySecretInObservations"])
        for needle in ("SELFCHECKDUMMY", "selfcheck-dummy", "Bearer", "oauthAccount", "accessToken", "apiKeyHelper"):
            self.assertNotIn(needle, self.raw, f"{needle} leaked into the self-check output")

    def test_a_documented_plugin_install_is_present_and_configured_but_not_tested(self):
        o = self.one("anthropic-frontend-design", "claude-plugin-declaration", "global")
        self.assertEqual((o["presence"], o["configuration"], o["confidence"]), ("present", "configured", "high"))
        self.assertEqual(o["version"], "1.0.0")
        self.assertTrue(o["identityVerified"])
        self.assertFalse(o["functionalityTested"])
        self.assertTrue(any("oturumunda yüklü olup olmadığı bilinmiyor" in x for x in o["limitations"]))

    def test_project_scope_is_reported_separately_from_global_scope(self):
        p = self.one("anthropic-frontend-design", "claude-plugin-declaration", "project")
        self.assertEqual((p["configuration"], p["projectAlias"]), ("disabled", "probe-project"))
        g = self.one("anthropic-frontend-design", "claude-plugin-declaration", "global")
        self.assertNotEqual(g["configuration"], p["configuration"])

    def test_executables_are_found_by_name_only_without_versions_or_verification(self):
        g = self.one("gitleaks", "path-executable", "global")
        self.assertEqual((g["presence"], g["confidence"], g["identityVerified"]), ("present", "low", False))
        self.assertIsNone(g["version"])
        s = self.one("strix", "path-executable", "global")
        self.assertEqual(s["presence"], "absent")
        c = self.one("agent:claude-code", "native-install-location", "global")
        self.assertEqual(c["presence"], "present")
        self.assertTrue(c["location"].startswith("~/.local/bin/"), c["location"])

    def test_mcp_references_are_configuration_only(self):
        for scope in ("global", "project"):
            m = self.one("codebase-memory-mcp", "claude-mcp-config", scope)
            self.assertEqual((m["presence"], m["configuration"]), ("unknown", "configured"), scope)
            self.assertFalse(m["identityVerified"])

    def test_antigravity_ide_is_never_guessed(self):
        a = self.one("agent:antigravity-ide", "not-documented", "unknown")
        self.assertEqual((a["presence"], a["agentSurface"]), ("unknown", "antigravity-ide"))

    def test_no_observation_claims_a_test_or_carries_an_absolute_home_path(self):
        for o in self.obs:
            self.assertFalse(o["functionalityTested"], o["subject"])
            if o["version"] is not None:
                self.assertEqual(o["versionSource"], "Claude Code kurulum kaydı", "only the plugin registry may supply a version")
            text = json.dumps(o, ensure_ascii=False)
            self.assertNotIn("discovery-probe/home", text.replace("\\", "/"), "the probe's home folder must be collapsed to ~")

    def test_the_scan_is_audited_with_a_schema_valid_inspect_event_and_never_with_install_like_events(self):
        types = [e["eventType"] for e in self.data["auditEvents"]]
        self.assertIn("inspect", types)
        self.assertFalse(set(FORBIDDEN_WORDS) & set(types))
        for e in self.data["auditEvents"]:
            if e["eventType"] == "inspect":
                self.assertEqual((e["outcome"], e["actorType"], e["checkId"]), ("passed", "user", "discovery-scan"))

    def test_the_self_audit_has_a_discovery_check_and_it_never_claims_more_than_observation(self):
        checks = {c["id"]: c for c in self.data["report"]["checks"]}
        self.assertIn("discovery", checks)
        self.assertIn(checks["discovery"]["status"], {"passed", "unknown"})
        self.assertNotEqual(self.data["report"]["overall"], "passed")


if __name__ == "__main__":
    unittest.main()
