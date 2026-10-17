"""Phase 2D: installation-plan contract, checked independently of the Rust code.

`--self-check` builds read-only installation plans with the built binary's planner (`planProbe`). Set ATM_SELFCHECK_JSON to
that file; without it the tests are skipped. The plans are validated against schemas/install-recipe.schema.json (every typed
operation must be a schema-valid recipe operation) and against the preview-only invariants.
"""

import hashlib
import json
import os
import pathlib
import unittest

import jsonschema

ROOT = pathlib.Path(__file__).resolve().parents[1]
SELFCHECK = os.environ.get("ATM_SELFCHECK_JSON")
RECIPE = json.loads((ROOT / "schemas" / "install-recipe.schema.json").read_text(encoding="utf-8"))
OPERATION_SCHEMA = RECIPE["properties"]["operations"]["items"]
KINDS = {o["properties"]["operation"]["const"] for o in OPERATION_SCHEMA["oneOf"]}


@unittest.skipUnless(SELFCHECK and pathlib.Path(SELFCHECK).is_file(), "ATM_SELFCHECK_JSON not provided; Rust self-check output not available")
class PlanContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.data = json.loads(pathlib.Path(SELFCHECK).read_text(encoding="utf-8"))
        cls.plans = cls.data["planProbe"]

    def test_the_probe_produced_plans_for_this_computer(self):
        self.assertGreaterEqual(len(self.plans), 4)

    def test_every_plan_is_a_versioned_blocked_preview_that_claims_no_effect(self):
        for p in self.plans:
            name = p["entry"]["entryId"]
            self.assertEqual(p["schemaVersion"], 1, name)
            self.assertEqual(p["state"], "preview_only", name)
            self.assertEqual((p["executable"], p["executed"], p["modifiesAnything"]), (False, False, False), name)
            self.assertEqual((p["eligibility"]["status"], p["eligibility"]["canExecute"]), ("blocked", False), name)
            self.assertIn("no_approved_recipe", [b["code"] for b in p["eligibility"]["blockers"]], name)
            self.assertFalse(p["rollback"]["available"], name)
            self.assertFalse(p["network"]["usedToBuildPreview"], name)
            self.assertRegex(p["planId"], r"^[0-9a-f]{64}$")
            self.assertIn("ÖNİZLEMEDİR", p["statement"])

    def test_every_typed_operation_is_a_schema_valid_recipe_operation_and_untyped_ones_say_what_is_missing(self):
        validator = jsonschema.Draft202012Validator(OPERATION_SCHEMA)
        seen = 0
        for p in self.plans:
            for step in p["steps"]:
                self.assertIn(step["kind"], KINDS)
                if step["typed"] is not None:
                    seen += 1
                    errors = list(validator.iter_errors(step["typed"]))
                    self.assertEqual(errors, [], f"{p['entry']['entryId']}: {[e.message for e in errors][:2]}")
                    self.assertEqual(step["typed"]["operation"], step["kind"])
                    self.assertEqual(step["missingParameters"], [])
                else:
                    self.assertTrue(step["missingParameters"], "an operation without a typed form must name what is missing")
        self.assertGreater(seen, 0)

    def test_no_plan_contains_a_path_a_command_or_a_secret_shape(self):
        text = json.dumps(self.plans)
        for needle in ("C:\\", "/Users/", "/home/", "curl ", "| sh", "powershell", "ghp_", "Bearer "):
            self.assertNotIn(needle, text)

    def test_a_quarantined_entry_stays_blocked_for_its_own_reason(self):
        strix = [p for p in self.plans if p["entry"]["entryId"] == "strix"]
        self.assertTrue(strix)
        codes = [b["code"] for b in strix[0]["eligibility"]["blockers"]]
        self.assertIn("execution_policy_blocked", codes)

    @staticmethod
    def canonical(value):
        """Independent re-implementation of the application's canonical JSON (sorted keys, compact, UTF-8)."""
        if isinstance(value, dict):
            return "{" + ",".join(json.dumps(k, ensure_ascii=False) + ":" + PlanContract.canonical(value[k]) for k in sorted(value)) + "}"
        if isinstance(value, list):
            return "[" + ",".join(PlanContract.canonical(v) for v in value) + "]"
        return json.dumps(value, ensure_ascii=False)

    def test_the_plan_id_is_an_independently_recomputable_digest_of_the_plan_content(self):
        for p in self.plans:
            blank = dict(p, planId="")
            digest = hashlib.sha256(self.canonical(blank).encode("utf-8")).hexdigest()
            self.assertEqual(digest, p["planId"], p["entry"]["entryId"])
            # any edit changes it
            edited = json.loads(json.dumps(blank))
            edited["entry"]["displayName"] += "x"
            self.assertNotEqual(hashlib.sha256(self.canonical(edited).encode("utf-8")).hexdigest(), p["planId"])

    def test_source_pin_and_prerequisite_basis_use_closed_vocabularies(self):
        for p in self.plans:
            self.assertIn(p["entry"]["sourcePin"], {"none", "malformed", "declared_unverified"})
            self.assertNotIn("immutableCommitPinned", p["entry"])
            if p["entry"]["sourcePin"] == "declared_unverified":
                self.assertIn("source_origin_unverified", [b["code"] for b in p["eligibility"]["blockers"]])
            for pre in p["prerequisites"]:
                self.assertIn(pre["basis"], {"current_scan", "no_current_evidence", "publisher_declared", "not_checked"})
                self.assertIn(pre["state"], {"met", "unmet", "unknown"})
                if pre["basis"] != "current_scan" and pre["source"] == "observed_scan":
                    self.assertEqual(pre["state"], "unknown", "no current evidence can never be met or unmet")
            self.assertNotIn("\"approval\"", json.dumps(p))
            self.assertEqual(p["approvalState"], "not_given")


if __name__ == "__main__":
    unittest.main()
