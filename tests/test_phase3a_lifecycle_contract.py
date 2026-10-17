"""Phase 3A: reviewed-recipe, approval-contract and SYNTHETIC lifecycle contract, checked independently of the Rust code.

`--self-check` runs the synthetic lifecycle of the built binary (`lifecycleProbe`): a made-up tool with made-up artifacts and made-up
authenticated references inside a throw-away sandbox below the self-check data directory. Set ATM_SELFCHECK_JSON to that file;
without it the tests are skipped. Nothing real is installed, started or downloaded by the probe or by these tests.
"""

import hashlib
import json
import os
import pathlib
import unittest

import jsonschema

ROOT = pathlib.Path(__file__).resolve().parents[1]
SELFCHECK = os.environ.get("ATM_SELFCHECK_JSON")
RECIPE_SCHEMA = json.loads((ROOT / "schemas" / "reviewed-recipe.schema.json").read_text(encoding="utf-8"))
RECEIPT_SCHEMA = json.loads((ROOT / "schemas" / "lifecycle-receipt.schema.json").read_text(encoding="utf-8"))
OPERATION_SCHEMA = json.loads((ROOT / "schemas" / "install-recipe.schema.json").read_text(encoding="utf-8"))["properties"]["operations"]["items"]


def canonical(value):
    if isinstance(value, dict):
        return "{" + ",".join(json.dumps(k, ensure_ascii=False) + ":" + canonical(value[k]) for k in sorted(value)) + "}"
    if isinstance(value, list):
        return "[" + ",".join(canonical(v) for v in value) + "]"
    return json.dumps(value, ensure_ascii=False)


def digest(value):
    return hashlib.sha256(canonical(value).encode("utf-8")).hexdigest()


class StaticSchemas(unittest.TestCase):
    """These run without the self-check output."""

    def test_the_recipe_schema_has_no_approval_field_and_refuses_unverified_rollback_and_archive_digests(self):
        recipe = RECIPE_SCHEMA["properties"]["recipe"]
        self.assertNotIn("approval", RECIPE_SCHEMA["properties"])
        self.assertNotIn("approval", recipe["properties"], "a recipe carries no approval")
        self.assertEqual(recipe["properties"]["rollback"]["properties"]["state"]["enum"], ["designed", "simulated"])
        self.assertEqual(recipe["properties"]["artifact"]["properties"]["digestScope"], {"const": "exact_file"})
        self.assertFalse(RECIPE_SCHEMA["additionalProperties"] or recipe["additionalProperties"])

    def test_the_receipt_schema_only_describes_committed_synthetic_transactions(self):
        props = RECEIPT_SCHEMA["properties"]
        self.assertEqual((props["synthetic"], props["state"]), ({"const": True}, {"const": "committed"}))
        self.assertTrue(props["toolId"]["pattern"].startswith("^synthetic-"))


@unittest.skipUnless(SELFCHECK and pathlib.Path(SELFCHECK).is_file(), "ATM_SELFCHECK_JSON not provided; Rust self-check output not available")
class LifecycleContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        data = json.loads(pathlib.Path(SELFCHECK).read_text(encoding="utf-8"))
        cls.probe = data["lifecycleProbe"]
        cls.steps = {s["step"]: s for s in cls.probe.get("steps", [])}

    def setUp(self):
        if "skipped" in self.probe:
            self.skipTest("unsupported platform for the synthetic lifecycle")

    def test_the_probe_is_marked_synthetic_and_stays_inside_one_sandbox(self):
        self.assertTrue(self.probe["synthetic"])
        self.assertEqual(self.probe["baseEntries"], ["<sandbox>"], "nothing was created next to the sandbox")
        text = json.dumps(self.probe)
        for needle in ("C:\\", "/Users/", "/home/", "/tmp", "lifecycle-probe", "ghp_", "Bearer "):
            self.assertNotIn(needle, text)

    def test_the_recipe_validates_and_its_review_is_bound_to_the_recipe_digest(self):
        reviewed = self.probe["recipe"]
        self.assertEqual(list(jsonschema.Draft202012Validator(RECIPE_SCHEMA).iter_errors(reviewed)), [])
        self.assertEqual(digest(reviewed["recipe"]), self.probe["recipeDigest"])
        self.assertEqual(reviewed["review"]["reviewedRecipeDigest"], self.probe["recipeDigest"])
        self.assertTrue(reviewed["recipe"]["synthetic"])
        self.assertTrue(reviewed["recipe"]["toolId"].startswith("synthetic-"))
        validator = jsonschema.Draft202012Validator(OPERATION_SCHEMA)
        for step in reviewed["recipe"]["steps"]:
            self.assertEqual(list(validator.iter_errors(step)), [], step)
        self.assertEqual(reviewed["recipe"]["steps"][1]["expectedSha256"], reviewed["recipe"]["artifact"]["sha256"])

    def test_the_plans_are_drafts_blocked_only_by_the_missing_user_approval(self):
        for plan in self.probe["plans"]:
            self.assertEqual([b["code"] for b in plan["eligibility"]["blockers"]], ["user_approval_missing"])
            self.assertEqual((plan["approvalState"], plan["state"], plan["executable"], plan["eligibility"]["canExecute"]), ("not_given", "preview_only", False, False))
            self.assertEqual(plan["recipe"]["state"], "validated")
            self.assertEqual(plan["rollback"]["reliability"], "simulated")
            self.assertFalse(plan["rollback"]["available"], "a plan never claims a working rollback")
            self.assertEqual(digest(dict(plan, planId="")), plan["planId"])
            self.assertNotIn('"approval"', json.dumps(plan))

    def test_the_lifecycle_followed_the_expected_story(self):
        s = self.steps
        self.assertEqual(s["install"]["activeVersion"], "1.0.0")
        self.assertTrue(s["install"]["detail"]["ok"] and not s["install"]["detail"]["alreadyApplied"])
        self.assertTrue(s["install-again"]["detail"]["alreadyApplied"], "repeating an install is a no-op")
        self.assertEqual(s["install-again"]["detail"]["receipt"], s["install"]["detail"]["receipt"])
        self.assertEqual((s["update"]["activeVersion"], s["update"]["detail"]["receipt"]["fromVersion"]), ("2.0.0", "1.0.0"))
        failed = s["failed-update"]
        self.assertFalse(failed["detail"]["ok"])
        self.assertEqual(failed["activeVersion"], "2.0.0", "the failed update was rolled back")
        self.assertEqual(failed["detail"]["rollback"]["status"], "restored")
        self.assertEqual(failed["detail"]["rollback"]["notUndone"], [])
        self.assertTrue(any("pointer" in u for u in failed["detail"]["rollback"]["undone"]))
        self.assertIn("interrupted", s["crashed-update"]["detail"])
        self.assertEqual(s["recovery"]["detail"]["recovered"], ["rolled_back"])
        self.assertEqual(s["recovery"]["activeVersion"], "2.0.0")
        self.assertIsNone(s["uninstall"]["activeVersion"])
        self.assertTrue(s["uninstall"]["detail"]["ok"])

    def test_every_receipt_validates_and_its_digest_is_independently_recomputable(self):
        validator = jsonschema.Draft202012Validator(RECEIPT_SCHEMA)
        receipts = [s["detail"]["receipt"] for s in self.steps.values() if s["detail"].get("receipt")]
        self.assertGreaterEqual(len(receipts), 3)
        for r in receipts:
            self.assertEqual(list(validator.iter_errors(r)), [], r)
            self.assertEqual(digest(dict(r, receiptDigest="")), r["receiptDigest"])
            self.assertTrue(r["synthetic"])
        # a receipt exists only for committed transactions: neither the failed nor the crashed update has one
        self.assertNotIn("receipt", self.steps["failed-update"]["detail"])
        self.assertNotIn("receipt", self.steps["crashed-update"]["detail"])
        ids = [r["txnId"] for r in receipts]
        self.assertEqual(len(ids), len(set(ids)) + 1, "only the repeated install reuses a receipt")


if __name__ == "__main__":
    unittest.main()
