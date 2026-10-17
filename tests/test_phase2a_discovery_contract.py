"""Phase 2A: the BUILT binary's read-only discovery engine, checked by independent Python code.

`--self-check` builds a throw-away fake home, PATH folder and project folder (with dummy secrets) inside its
isolated directory, scans them with the real engine and reports what it observed. Set ATM_SELFCHECK_JSON to
that file; without it these tests are reported as SKIPPED (never as passed). CI always sets it.
"""
import copy
import hashlib
import json
import os
import pathlib
import re
import struct
import sys
import unittest

import jsonschema

SELFCHECK = os.environ.get("ATM_SELFCHECK_JSON")
FORBIDDEN_WORDS = ("install", "update", "approve", "verify", "rollback", "uninstall")

# An absolute path in any common shape: drive letter, UNC share, or a well-known root such as /Users/.
ABSOLUTE_PATH = re.compile(
    r"(?i)(?:^|[\s(\[\"'=:,;])(?:[a-z]:[\\/]|\\\\[^\s\\]|//[^\s/]|/(?:users|home|var|tmp|etc|opt|usr|private|volumes|mnt|root|library|applications|system|windows|programdata)(?:/|$))"
)
# The closed grammar of `location` (documented in docs/PHASE-2A.md).
LOCATION = re.compile(
    r"^(?:~/[A-Za-z0-9._][A-Za-z0-9._/-]*|PATH girdisi #(?:[1-9]|[1-5][0-9]|6[0-4]) \u00b7 "
    r"(?:kullan\u0131c\u0131 klas\u00f6r\u00fc alt\u0131nda|sistem veya paket y\u00f6neticisi klas\u00f6r\u00fc|ba\u015fka bir klas\u00f6r) \u00b7 [a-z0-9][a-z0-9._-]{0,40})$"
)


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
        self.assertGreaterEqual(self.data["dbVersion"], 8)

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
        self.assertFalse(o["identityVerified"], "a matching local record is corroborated metadata, not authenticated identity")
        self.assertFalse(o["functionalityTested"])
        self.assertTrue(any("oturumunda yüklü olup olmadığı bilinmiyor" in x for x in o["limitations"]))

    def test_project_scope_is_reported_separately_from_global_scope(self):
        p = self.one("anthropic-frontend-design", "claude-plugin-declaration", "project")
        self.assertEqual((p["configuration"], p["projectAlias"]), ("disabled", "probe-project"))
        g = self.one("anthropic-frontend-design", "claude-plugin-declaration", "global")
        self.assertNotEqual(g["configuration"], p["configuration"])

    def test_executables_are_found_by_name_only_without_versions_or_verification(self):
        g = self.one("graphify", "path-executable", "global")
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
                self.assertIn(o["versionSource"], ("Claude Code kurulum kaydı", "yetkili referans özeti"), "only the plugin registry or an authenticated reference may supply a version")
            text = json.dumps(o, ensure_ascii=False)
            self.assertNotIn("discovery-probe/home", text.replace("\\", "/"), "the probe's home folder must be collapsed to ~")

    def test_headless_observations_contain_no_absolute_path_and_only_grammar_conforming_locations(self):
        for o in self.obs:
            for field in ("evidence", "location", "versionSource", "projectAlias"):
                value = o.get(field)
                if value is not None:
                    self.assertIsNone(ABSOLUTE_PATH.search(value), f"{o['subject']}.{field}: {value}")
            for line in o["limitations"]:
                self.assertIsNone(ABSOLUTE_PATH.search(line), line)
            if o["location"] is not None:
                self.assertRegex(o["location"], LOCATION)
        self.assertIsNone(ABSOLUTE_PATH.search(json.dumps(self.probe, ensure_ascii=False)))
        self.assertNotIn("discovery-probe", json.dumps(self.probe, ensure_ascii=False), "the probe folder name must not appear anywhere")

    def test_a_path_hit_is_located_by_its_position_in_path_not_by_its_folder(self):
        g = self.one("gitleaks", "path-executable", "global")
        self.assertRegex(g["location"], r"^PATH girdisi #1 \u00b7 .+ \u00b7 gitleaks(\.exe)?$")

    def test_mcp_evidence_counts_matches_and_never_names_a_server(self):
        for scope in ("global", "project"):
            m = self.one("codebase-memory-mcp", "claude-mcp-config", scope)
            self.assertIn("1 MCP sunucu tan\u0131m\u0131 var", m["evidence"])
            self.assertNotIn("(ad:", m["evidence"])
            self.assertNotIn("codebase-memory", m["evidence"])

    def test_every_observation_names_its_scan_and_is_current_after_a_fresh_scan(self):
        for o in self.obs:
            self.assertEqual(o["scanId"], self.probe["secondScanId"], "the shown row of every key comes from the latest scan")
            self.assertEqual((o["provenance"], o["current"], o["legacy"]), ("latest_scan", True, False), o["subject"])
            self.assertIsNone(o["notCurrentReason"])
            self.assertTrue(o["scanAuthorizedPath"] and o["scanAuthorizedUserConfig"] and o["scanAuthorizedProject"])
            self.assertIn("scanFinishedAt", o)

    # ---- Phase 2B: passive provenance ladder, checked independently of the Rust code ----------------------------
    VOCAB = {
        "publisher": {"none", "self_declared", "marketplace"},
        "identity": {"not_assessed", "unknown", "evidence_available", "record_corroborated", "validated", "conflicted"},
        "version": {"unknown", "known", "conflicted"},
        "versionTrust": {"none", "declared", "installer_recorded", "validated"},
        "signatureBlock": {"not_assessed", "absent", "present_unvalidated", "unreadable"},
    }
    NOT_PERFORMED = {
        "active_version_probe", "signature_chain_validation", "signature_parsing_unsupported", "installed_file_digest",
        "package_metadata_not_found", "no_package_detector_for_tool", "record_to_file_linkage", "artifact_authentication",
        "artifact_fingerprint", "plugin_directory_digest", "authenticated_reference",
    }
    SCHEMA = json.loads((pathlib.Path(__file__).resolve().parents[1] / "schemas" / "artifact-provenance.schema.json").read_text(encoding="utf-8"))
    # Only the synthetic probe has authenticated references (compiled-in production list is empty). Tool -> bytes it was given.
    @staticmethod
    def synthetic_executable(payload):
        """Independent re-implementation of the probe's synthetic native header (PE on Windows, Mach-O arm64 elsewhere)."""
        if sys.platform.startswith("win"):
            head = bytearray(0x80 + 24)
            head[0:2] = b"MZ"
            head[0x3C:0x40] = struct.pack("<I", 0x80)
            head[0x80:0x84] = b"PE\0\0"
            head[0x84:0x86] = struct.pack("<H", 0x8664)
            return bytes(head) + payload
        return b"\xcf\xfa\xed\xfe" + struct.pack("<I", 0x0100000C) + bytes(24) + payload

    EXPECTED_FORMAT = "pe-x64" if sys.platform.startswith("win") else "macho-arm64"
    SYNTHETIC_MATCH = {"gitleaks": b"stub"}
    SYNTHETIC_MISMATCH = {"graphify": b"different bytes under a catalogued name"}

    def test_every_ladder_uses_closed_vocabulary_and_obeys_the_invariants(self):
        for o in self.obs:
            l = o["ladder"]
            self.assertTrue(l["assessed"], o["subject"])
            for key, allowed in self.VOCAB.items():
                self.assertIn(l[key], allowed, f"{o['subject']}.{key}")
            self.assertTrue(set(l["notPerformed"]) <= self.NOT_PERFORMED, l["notPerformed"])
            # "validated" requires an AUTHENTICATED exact-file reference matched by the fingerprint of the installed file.
            a = l["artifact"]
            if l["identity"] == "validated":
                self.assertEqual((l["artifactAuth"], a["fingerprint"], a["reference"], a["matchResult"], a["invalidated"]), ("digest_match", "computed", "authenticated", "matches_authenticated", False), o["subject"])
                self.assertIn(a["referenceKind"], ("executable", "script", "plugin_file"))
                self.assertEqual(l["versionTrust"], "validated")
            else:
                self.assertEqual(l["artifactAuth"], "none", o["subject"])
            self.assertEqual(l["identity"] == "validated", l["artifactAuth"] == "digest_match", o["subject"])
            if l["identity"] == "record_corroborated":
                self.assertTrue(l["installRecord"] and l["installedFilesSeen"] and l["publisher"] == "marketplace", "corroboration needs record + marketplace + files")
                self.assertIn("artifact_authentication", l["notPerformed"])
            self.assertEqual(o["identityVerified"], l["identity"] == "validated", o["subject"])
            if o["version"] is not None:
                self.assertEqual(l["version"], "known")
                self.assertNotEqual(l["versionTrust"], "none")
            else:
                self.assertNotEqual(l["version"], "known")
            self.assertFalse(o["functionalityTested"])
            if l["signatureBlock"] == "present_unvalidated":
                self.assertIn("signature_chain_validation", l["notPerformed"])

    def test_a_corroborated_plugin_record_and_a_name_only_program_sit_on_different_rungs(self):
        fd = self.one("anthropic-frontend-design", "claude-plugin-declaration", "global")["ladder"]
        self.assertEqual((fd["identity"], fd["publisher"], fd["installRecord"], fd["versionTrust"]), ("record_corroborated", "marketplace", True, "installer_recorded"))
        g = self.one("agent:claude-code", "native-install-location", "global")["ladder"]
        self.assertEqual((g["filenameDetected"], g["installRecord"], g["identity"], g["version"]), (True, False, "unknown", "unknown"))
        self.assertIn("active_version_probe", g["notPerformed"])
        self.assertIn("installed_file_digest", g["notPerformed"])
        self.assertEqual(g["artifact"]["fingerprint"], "computed")
        self.assertEqual(g["artifact"]["matchResult"], "not_applicable", "a fingerprint with no reference concludes nothing")

    # ---- Phase 2C: artifact fingerprints and the reference-digest trust contract -----------------------------------

    def test_every_observation_validates_against_the_artifact_provenance_schema(self):
        validator = jsonschema.Draft202012Validator(self.SCHEMA)
        for o in self.obs:
            errors = list(validator.iter_errors(o))
            self.assertEqual(errors, [], f"{o['subject']}: {[e.message for e in errors][:3]}")

    def test_the_schema_rejects_forged_verification(self):
        validator = jsonschema.Draft202012Validator(self.SCHEMA)
        good = copy.deepcopy(self.one("gitleaks", "path-executable", "global"))
        self.assertEqual(list(validator.iter_errors(good)), [])
        def broken(mutate):
            o = copy.deepcopy(good)
            mutate(o)
            return list(validator.iter_errors(o))
        # a verified flag without a validated ladder
        self.assertTrue(broken(lambda o: o["ladder"].update(identity="record_corroborated")))
        # validated, but the reference is only self-declared
        self.assertTrue(broken(lambda o: o["ladder"]["artifact"].update(reference="unauthenticated")))
        # validated, but the "reference" is an archive digest
        self.assertTrue(broken(lambda o: o["ladder"]["artifact"].update(referenceKind="archive")))
        # validated without a fingerprint
        self.assertTrue(broken(lambda o: o["ladder"]["artifact"].update(fingerprint="unknown", sha256=None, matchResult="not_applicable")))
        # validated but the verdict was invalidated
        self.assertTrue(broken(lambda o: o["ladder"]["artifact"].update(invalidated=True)))
        # a path instead of a digest
        self.assertTrue(broken(lambda o: o["ladder"]["artifact"].update(sha256="C:\\Users\\x\\tool.exe")))
        # functionality claimed
        self.assertTrue(broken(lambda o: o.update(functionalityTested=True)))
        # a computed fingerprint with no digest
        self.assertTrue(broken(lambda o: o["ladder"]["artifact"].update(sha256=None)))
        # validated but the verified format is not an exact-file executable/script format (or is missing / unrecognised)
        for fmt in ("zip", "gzip", "unrecognised", None):
            self.assertTrue(broken(lambda o, fmt=fmt: o["ladder"]["artifact"].update(verifiedFormat=fmt)), fmt)
        # validated against a plugin-file reference: no hashed file is a plugin file in this policy
        self.assertTrue(broken(lambda o: o["ladder"]["artifact"].update(referenceKind="plugin_file")))
        # a free-text format is not a closed-vocabulary word
        self.assertTrue(broken(lambda o: o["ladder"]["artifact"].update(verifiedFormat="pe-exe")))

    def test_the_fingerprints_are_independently_recomputable_and_only_the_synthetic_exact_reference_verifies(self):
        for tool, payload in {**self.SYNTHETIC_MATCH, **self.SYNTHETIC_MISMATCH}.items():
            content = self.synthetic_executable(payload)
            o = self.one(tool, "path-executable", "global")
            a = o["ladder"]["artifact"]
            self.assertEqual(a["verifiedFormat"], self.EXPECTED_FORMAT, f"{tool}: the format is established from the bytes")
            self.assertEqual(a["fingerprint"], "computed")
            self.assertEqual(a["sha256"], hashlib.sha256(content).hexdigest(), f"{tool}: the digest must equal an independent SHA-256 of the bytes")
            self.assertEqual(a["bytes"], len(content))
        ok = self.one("gitleaks", "path-executable", "global")
        self.assertEqual((ok["identityVerified"], ok["ladder"]["identity"], ok["version"], ok["versionSource"]), (True, "validated", "0.0.1", "yetkili referans özeti"))
        bad = self.one("graphify", "path-executable", "global")
        self.assertEqual((bad["identityVerified"], bad["ladder"]["identity"], bad["ladder"]["artifact"]["matchResult"]), (False, "conflicted", "mismatches_authenticated"))
        verified = [o["subject"] for o in self.obs if o["identityVerified"]]
        self.assertEqual(verified, ["gitleaks"], "nothing else may be verified: no reference, no archive digest, no record alone")

    def test_a_computed_fingerprint_without_a_reference_and_a_plugin_record_stay_unverified(self):
        c = self.one("agent:claude-code", "native-install-location", "global")
        self.assertEqual((c["identityVerified"], c["ladder"]["artifact"]["reference"]), (False, "none"))
        fd = self.one("anthropic-frontend-design", "claude-plugin-declaration", "global")
        self.assertEqual((fd["identityVerified"], fd["ladder"]["identity"], fd["ladder"]["artifact"]["recordLink"]), (False, "record_corroborated", "folder_by_name"))
        self.assertIn("plugin_directory_digest", fd["ladder"]["notPerformed"])
        self.assertEqual(fd["ladder"]["artifact"]["fingerprint"], "not_requested")

    def test_the_probe_reports_that_its_references_are_synthetic_and_the_scan_recorded_the_fingerprint_permission(self):
        self.assertTrue(self.probe["syntheticReferences"])
        self.assertTrue(self.probe["scan"]["authorizedFingerprint"])
        self.assertTrue(all(o["scanAuthorizedFingerprint"] for o in self.obs))

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
