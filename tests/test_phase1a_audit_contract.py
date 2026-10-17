"""Cross-language contract: audit events written by the Rust app must satisfy the Phase 0 schema.

The app is run headless with `--self-check <file>` (see src-tauri/src/selfcheck.rs) and the produced
JSON is validated here by independent Python code. Set ATM_SELFCHECK_JSON to that file; without it
the tests are reported as SKIPPED (never as passed). CI always sets it.
"""
import copy
import hashlib
import json
import os
import pathlib
import unittest

import jsonschema

ROOT = pathlib.Path(__file__).resolve().parents[1]
SELFCHECK = os.environ.get("ATM_SELFCHECK_JSON")


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def digest_without(record, omitted):
    payload = {k: v for k, v in record.items() if k not in omitted}
    return hashlib.sha256(canonical(payload).encode("utf-8")).hexdigest()


@unittest.skipUnless(SELFCHECK and pathlib.Path(SELFCHECK).is_file(), "ATM_SELFCHECK_JSON not provided; Rust self-check output not available")
class RustAuditContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.data = json.loads(pathlib.Path(SELFCHECK).read_text(encoding="utf-8"))
        schema = json.loads((ROOT / "schemas" / "audit-event.schema.json").read_text(encoding="utf-8"))
        cls.validator = jsonschema.Draft202012Validator(schema, format_checker=jsonschema.FormatChecker())
        cls.events = cls.data["auditEvents"]

    def test_self_check_ran_without_internal_error(self):
        self.assertNotIn("error", self.data)
        self.assertIn(self.data["report"]["overall"], {"passed", "unknown", "failed"})
        self.assertTrue(self.events, "no audit events were produced")

    def test_every_event_validates_against_the_phase0_schema(self):
        for event in self.events:
            self.validator.validate(event)

    def test_digests_and_hash_chain_recompute_independently(self):
        previous = None
        for index, event in enumerate(self.events, start=1):
            self.assertEqual(event["sequence"], index)
            self.assertEqual(event["segmentStart"], index == 1)
            self.assertEqual(event["previousHash"], previous)
            self.assertEqual(event["recordDigest"], digest_without(event, {"recordDigest"}))
            self.assertEqual(event["redaction"]["payloadDigest"], digest_without(event, {"recordDigest", "redaction"}))
            previous = event["recordDigest"]
        self.assertEqual(len({e["chainId"] for e in self.events}), 1)
        self.assertEqual(self.data["auditChain"]["head"], previous)
        self.assertTrue(self.data["auditChain"]["ok"])

    def test_tampering_is_detected_by_the_same_recomputation(self):
        forged = copy.deepcopy(self.events[0])
        forged["outcome"] = "failed" if forged["outcome"] != "failed" else "passed"
        self.assertNotEqual(forged["recordDigest"], digest_without(forged, {"recordDigest"}))

    def test_app_never_claims_independent_verification(self):
        for event in self.events:
            self.assertNotEqual(event["eventType"], "verify")
            self.assertNotEqual(event["verdictSource"], "independent-harness")
            self.assertEqual(event["verdictSource"], "operational-health")

    def test_headless_run_cannot_report_a_clean_pass(self):
        # single-instance behaviour is unobservable in-process, so a full "passed" would be a false green.
        self.assertNotEqual(self.data["report"]["overall"], "passed")
        statuses = {c["id"]: c["status"] for c in self.data["report"]["checks"]}
        self.assertEqual(statuses["single-instance"], "unknown")
        self.assertEqual(statuses["adapters"], "skipped")

    def test_no_local_paths_or_secret_shaped_values_in_serialized_events(self):
        text = json.dumps(self.events)
        for needle in ("C:\\\\", "/Users/", "/home/", "ghp_", "Bearer ", "password"):
            self.assertNotIn(needle, text)


if __name__ == "__main__":
    unittest.main()
