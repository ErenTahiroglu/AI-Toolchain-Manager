"""CONFIG-02C: the result gate (tools/config02c_results_check.py) itself. It runs on SYNTHETIC result lines only; it proves that a missing line is NOT RUN, that a skip
is never a pass, and that nothing is inferred. It says nothing about any reader's behaviour (that is the job of the Rust and vitest cases the lines come from)."""
import contextlib
import io
import json
import pathlib
import re
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import config02c_results_check as gate  # noqa: E402

MANIFEST = json.loads((ROOT / "tests" / "config02c_expected_results.json").read_text(encoding="utf-8"))


def line(ident, os_name="windows", status="PASS", reason=""):
    return f"CONFIG02C_RESULT|{ident}|{os_name}|{status}|{reason}"


def everything_passing(os_name="windows"):
    return [line(i, os_name) for i in MANIFEST["ids"]]


def run(lines, os_name="windows"):
    with tempfile.TemporaryDirectory() as tmp:
        path = pathlib.Path(tmp) / "results.txt"
        path.write_text("some log text\n" + "\n".join(lines) + "\nmore text\n", encoding="utf-8")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = gate.main(["--os", os_name, "--manifest", str(ROOT / "tests" / "config02c_expected_results.json"), "--results", str(path)])
        return code, out.getvalue()


class Gate(unittest.TestCase):
    def test_a_complete_set_passes_and_the_summary_line_counts_it(self):
        code, out = run(everything_passing())
        self.assertEqual(code, 0, out)
        self.assertIn("CONFIG02C_RESULTS|windows|PASS=%d|SKIPPED=0|FAIL=0|BLOCKED=0|NOT_RUN=0" % len(MANIFEST["ids"]), out)
        self.assertIn("GATE OK", out)

    def test_a_missing_id_is_not_run_and_fails_the_gate(self):
        lines = [x for x in everything_passing() if "|B5|" not in x]
        code, out = run(lines)
        self.assertEqual(code, 1)
        self.assertIn("B5 | rust | NOT RUN", out)
        self.assertIn("NOT_RUN=1", out)

    def test_an_empty_log_is_all_not_run(self):
        code, out = run([])
        self.assertEqual(code, 1)
        self.assertIn("NOT_RUN=%d" % len(MANIFEST["ids"]), out)

    def test_a_skipped_line_is_never_counted_as_pass(self):
        lines = [x for x in everything_passing() if "|B3|" not in x] + [line("B3", status="SKIPPED", reason="cannot_create_file_symlink")]
        code, out = run(lines)
        self.assertEqual(code, 0, out)  # an allowed, visible skip does not fail the gate ...
        self.assertIn("SKIPPED=1", out)
        self.assertIn("PASS=%d" % (len(MANIFEST["ids"]) - 1), out)  # ... but it is not a pass

    def test_a_skip_with_an_unlisted_reason_fails(self):
        lines = [x for x in everything_passing() if "|A1|" not in x] + [line("A1", status="SKIPPED", reason="because")]
        code, out = run(lines)
        self.assertEqual(code, 1)
        self.assertIn("does not allow", out)

    def test_fail_and_blocked_fail_the_gate(self):
        for status in ("FAIL", "BLOCKED"):
            lines = [x for x in everything_passing() if "|C5|" not in x] + [line("C5", status=status)]
            code, out = run(lines)
            self.assertEqual(code, 1, status)
            self.assertIn(f"C5: {status}", out)

    def test_conflicting_duplicates_and_foreign_or_unknown_lines_fail(self):
        code, _ = run(everything_passing() + [line("D1", status="FAIL")])
        self.assertEqual(code, 1)
        code, _ = run(everything_passing() + [line("D1")])  # the same line twice is harmless
        self.assertEqual(code, 0)
        code, out = run(everything_passing() + [line("Z99")])
        self.assertEqual(code, 1)
        self.assertIn("unknown id Z99", out)
        code, out = run(everything_passing() + [line("A1", os_name="macos")])
        self.assertEqual(code, 1)
        self.assertIn("another operating system", out)

    def test_each_operating_system_is_judged_on_its_own_lines(self):
        code, out = run(everything_passing("windows"), os_name="macos")
        self.assertEqual(code, 1)
        self.assertIn("NOT_RUN=%d" % len(MANIFEST["ids"]), out)

    def test_the_manifest_allows_only_fixed_reason_codes_and_no_path(self):
        for ident, spec in MANIFEST["ids"].items():
            for os_name, reasons in spec["skip_allowed"].items():
                self.assertIn(os_name, ("windows", "macos"), ident)
                for r in reasons:
                    self.assertRegex(r, r"^[a-z0-9_]+$", ident)
        self.assertEqual(sorted({s["suite"] for s in MANIFEST["ids"].values()}), ["rust", "vitest"])

    def test_the_line_format_is_the_documented_pipe_form(self):
        self.assertIsNotNone(gate.LINE.search(line("A1")))
        self.assertIsNone(gate.LINE.search("CONFIG02C_RESULT A1 windows PASS"))  # the space-delimited sketch of R1 is not accepted
        self.assertIsNotNone(re.search(r"CONFIG02C_RESULT\|<id>\|<os>\|<status>\|<reason>", (ROOT / "docs/config-02c/THREAT_AND_PRIVACY_TEST_PLAN.md").read_text(encoding="utf-8")))


if __name__ == "__main__":
    unittest.main()
