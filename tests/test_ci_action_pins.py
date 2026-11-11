"""CI-HARDEN-02 (Issue #48): regression tests for tools/check_action_pins.py, the static GitHub Actions `uses:` pin guard.

All negative cases use in-memory workflow text or temporary directories; no tracked workflow file is modified. The guard is a regression and CI quality
gate, not tamper-proof protection (see the module docstring and docs/CI-ACTION-PINS.md). A green run proves identity+SHA text, not upstream code safety.
"""
import importlib.util
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
TOOL = ROOT / "tools" / "check_action_pins.py"
_spec = importlib.util.spec_from_file_location("check_action_pins_under_test", TOOL)
cap = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = cap
_spec.loader.exec_module(cap)

APPROVED, REQUIRED = cap.load_allowlist(cap.DEFAULT_ALLOWLIST)
CHECKOUT = APPROVED["actions/checkout"]
OTHER_VALID_SHA = "0123456789abcdef0123456789abcdef01234567"  # well-formed, approved for nothing


def wf(steps: str, job_extra: str = "") -> str:
    """A minimal valid workflow; `steps` is already indented for a step list (6 spaces)."""
    return "name: t\non:\n  push:\n    branches: [main]\njobs:\n  j:\n    runs-on: ubuntu-24.04\n" + job_extra + "    steps:\n" + steps


def evaluate(text: str, name: str = "w.yml"):
    """What the guard would report for this workflow text: structural violations plus reference violations, as strings."""
    violations, refs = cap.scan_workflow_text(text, name)
    msgs = [v.message for v in violations]
    for _line, value in refs:
        reason = cap.validate_reference(value, APPROVED)
        if reason:
            msgs.append(reason + ": " + value)
    return msgs


def has(msgs, needle):
    return any(needle in m for m in msgs)


class CurrentRepository(unittest.TestCase):
    def test_every_current_workflow_passes_and_all_three_required_files_are_scanned(self):
        violations, summary = cap.check_repository(ROOT)
        self.assertEqual([v.render() for v in violations], [])
        self.assertGreaterEqual(summary["files"], 3)
        self.assertGreaterEqual(summary["uses"], 13)
        names = {p.name for p in (ROOT / ".github" / "workflows").iterdir()}
        for required in REQUIRED:
            self.assertIn(required, names)

    def test_every_approved_reference_is_accepted(self):
        for identity, sha in APPROVED.items():
            self.assertEqual(evaluate(wf(f"      - uses: {identity}@{sha}\n")), [], identity)

    def test_allowlist_is_exactly_the_six_reviewed_identities(self):
        # changing this mapping is a policy change: it needs a separately reviewed change with upstream provenance, and must edit this test.
        self.assertEqual(
            APPROVED,
            {
                "actions/cache": "0057852bfaa89a56745cba8c7296529d2fc39830",
                "actions/checkout": "11d5960a326750d5838078e36cf38b85af677262",
                "actions/setup-node": "49933ea5288caeca8642d1e84afbd3f7d6820020",
                "actions/setup-python": "a26af69be951a213d495a4c3e4e4022e16d87065",
                "actions/upload-artifact": "ea165f8d65b6e75b540449e92b4886f43607fa02",
                "dtolnay/rust-toolchain": "b725eda525362e7cf34af9a7243cac1e3f01a7ec",
            },
        )
        self.assertEqual(sorted(REQUIRED), ["atm-gov-02-ci-pilot.yml", "ci.yml", "license-check.yml"])

    def test_the_checker_runs_from_the_command_line_and_reports_pass(self):
        r = subprocess.run([sys.executable, str(TOOL)], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("ACTION_PINS_RESULT PASS", r.stdout)


class LegitimateSyntaxIsNotAFalsePositive(unittest.TestCase):
    def test_trailing_and_full_line_comments(self):
        text = wf(f"      # uses: actions/checkout@v4 (a comment is not a dependency)\n      - uses: actions/checkout@{CHECKOUT} # v4.4.0\n")
        violations, refs = cap.scan_workflow_text(text, "w.yml")
        self.assertEqual(violations, [])
        self.assertEqual(len(refs), 1)
        self.assertEqual(evaluate(text), [])

    def test_uses_text_inside_block_scalars_and_quoted_values_is_not_a_dependency(self):
        text = wf(
            "      - name: 'uses: actions/checkout@v4'\n"
            "      - name: \"uses: actions/checkout@main\"\n"
            "      - name: harmless\n"
            "        run: |\n"
            "          echo \"uses: actions/checkout@v4\"\n"
            "          # uses: actions/checkout@main\n"
            "          uses: actions/checkout@v3\n"
            "\n"
            "          - uses: not-a-step@v1\n"
            "      - name: folded\n"
            "        run: >-\n"
            "          uses: actions/checkout@v2\n"
            "      - name: keep\n"
            "        run: |+\n"
            "          uses: x/y@z\n"
            f"      - uses: actions/checkout@{CHECKOUT}\n"
        )
        violations, refs = cap.scan_workflow_text(text, "w.yml")
        self.assertEqual(violations, [])
        self.assertEqual([v for _l, v in refs], [f"actions/checkout@{CHECKOUT}"])
        self.assertEqual(evaluate(text), [])

    def test_a_uses_key_right_after_a_block_scalar_is_still_scanned(self):
        text = wf("      - run: |\n          echo hi\n      - uses: actions/checkout@v4\n")
        self.assertTrue(has(evaluate(text), "mutable ref"))

    def test_simple_flow_lists_and_crlf_line_endings(self):
        text = wf(f"      - uses: actions/checkout@{CHECKOUT}\n        with:\n          path: [a, b]\n").replace("\n", "\r\n")
        self.assertEqual(evaluate(text), [])

    def test_yml_yaml_and_uppercase_extensions_are_all_scanned(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = make_repo(pathlib.Path(tmp))
            good = wf(f"      - uses: actions/checkout@{CHECKOUT}\n")
            (root / ".github" / "workflows" / "extra.yaml").write_text(good, encoding="utf-8")
            (root / ".github" / "workflows" / "EXTRA2.YML").write_text(good, encoding="utf-8")
            violations, summary = cap.check_repository(root)
            self.assertEqual([v.render() for v in violations], [])
            self.assertEqual(summary["files"], 5)
            for name in ("extra.yaml", "EXTRA2.YML"):
                (root / ".github" / "workflows" / name).write_text(wf("      - uses: actions/checkout@v4\n"), encoding="utf-8")
            violations, _ = cap.check_repository(root)
            self.assertEqual(sorted({v.file.rsplit("/", 1)[1] for v in violations}), ["EXTRA2.YML", "extra.yaml"])


class UnsafeReferencesAreRejected(unittest.TestCase):
    def ref(self, value: str):
        return evaluate(wf(f"      - uses: {value}\n"))

    def test_moving_tags_branches_and_version_tags(self):
        for value in ("actions/checkout@v4", "actions/checkout@v4.4.0", "actions/checkout@main", "dtolnay/rust-toolchain@stable", "actions/checkout@releases/v4"):
            self.assertTrue(has(self.ref(value), "mutable ref"), value)

    def test_wrong_length_and_malformed_shas(self):
        self.assertTrue(has(self.ref("actions/checkout@" + CHECKOUT[:7]), "abbreviated"))
        self.assertTrue(has(self.ref("actions/checkout@" + CHECKOUT[:39]), "abbreviated"))
        self.assertTrue(has(self.ref("actions/checkout@" + CHECKOUT + "0"), "longer than 40"))
        self.assertTrue(has(self.ref("actions/checkout@" + "g" * 40), "mutable ref"))  # 40 characters but not hexadecimal
        self.assertTrue(has(self.ref("actions/checkout@" + CHECKOUT.upper()), "lowercase"))
        self.assertTrue(has(self.ref("actions/checkout@" + CHECKOUT[:20].upper() + CHECKOUT[20:]), "lowercase"))
        self.assertTrue(has(self.ref("actions/checkout"), "no @<sha>"))

    def test_a_well_formed_sha_must_still_be_the_approved_one(self):
        self.assertTrue(has(self.ref("actions/checkout@" + OTHER_VALID_SHA), "not the approved SHA for actions/checkout"))

    def test_unknown_action_with_a_valid_sha(self):
        self.assertTrue(has(self.ref("evil/action@" + OTHER_VALID_SHA), "not in the approved list"))
        self.assertTrue(has(self.ref("Actions/Checkout@" + CHECKOUT), "not in the approved list"))  # identity is matched exactly

    def test_an_approved_sha_assigned_to_a_different_action(self):
        msgs = self.ref("actions/setup-node@" + CHECKOUT)
        self.assertTrue(has(msgs, "not the approved SHA for actions/setup-node"))
        msgs = self.ref("someone/else@" + CHECKOUT)
        self.assertTrue(has(msgs, "approved only for actions/checkout"))

    def test_job_level_reusable_workflow_and_path_references(self):
        text = "name: t\non:\n  push:\njobs:\n  call:\n    uses: octo/repo/.github/workflows/w.yml@" + OTHER_VALID_SHA + "\n"
        self.assertTrue(has(evaluate(text), "reusable-workflow"))
        self.assertTrue(has(self.ref("actions/checkout/sub@" + CHECKOUT), "reusable-workflow, path or malformed"))

    def test_dynamic_expressions_local_and_docker_references(self):
        for value in ("${{ matrix.action }}", "actions/checkout@${{ env.SHA }}", "${{ env.OWNER }}/checkout@" + CHECKOUT):
            self.assertTrue(has(self.ref(value), "dynamic expression"), value)
        for value in ("./.github/actions/local", "./x", "../x", "."):
            self.assertTrue(has(self.ref(value), "local action"), value)
        for value in ("docker://alpine:3.20", "docker://alpine@sha256:" + "a" * 64, "DOCKER://alpine"):
            self.assertTrue(has(self.ref(value), "docker://"), value)

    def test_trailing_junk_that_is_not_a_comment(self):
        self.assertTrue(has(self.ref("actions/checkout@" + CHECKOUT + "#frag"), "mutable ref"))
        self.assertTrue(has(self.ref("actions/checkout@" + CHECKOUT + "x"), "mutable ref"))  # 41 characters, not all hexadecimal


class SyntaxThatCouldHideAReferenceIsRejected(unittest.TestCase):
    """Fail closed: each of these must produce a violation rather than be silently skipped or accepted."""

    def bad(self, text, needle=None):
        msgs = evaluate(text)
        self.assertTrue(msgs, "expected a violation for:\n" + text)
        if needle:
            self.assertTrue(has(msgs, needle), (needle, msgs))

    def test_flow_mappings_hiding_uses(self):
        self.bad(wf("      - {uses: actions/checkout@v4}\n"), "flow mapping")
        self.bad(wf("      - { name: x, uses: actions/checkout@v4 }\n"), "flow mapping")
        self.bad(wf("      - name: x\n        with: {uses: actions/checkout@v4}\n"), "unsupported value syntax")
        self.bad(wf("      - name: x\n        with: [uses: actions/checkout@v4]\n"), "flow list")

    def test_quoted_keys(self):
        self.bad(wf("      - \"uses\": actions/checkout@v4\n"), "quoted key")
        self.bad(wf("      - 'uses': actions/checkout@v4\n"), "quoted key")
        self.bad(wf("      - name: x\n        \"uses\": actions/checkout@v4\n"), "quoted key")

    def test_quoted_uses_value_is_unsupported(self):
        self.bad(wf(f"      - uses: \"actions/checkout@{CHECKOUT}\"\n"), "quoted `uses` value")

    def test_anchors_aliases_merge_keys_and_tags(self):
        self.bad(wf(f"      - &pin uses: actions/checkout@{CHECKOUT}\n"), "anchor")
        self.bad(wf("      - uses: *pin\n"), "alias")
        self.bad(wf("      - <<: *pin\n"), "anchor, alias, tag, merge")
        self.bad(wf("      - !!str uses: actions/checkout@v4\n"), "anchor, alias, tag")
        self.bad(wf("      - name: !!str x\n"), "unsupported value syntax")
        self.bad(wf("      - ? uses\n        : actions/checkout@v4\n"), "anchor, alias, tag, merge or complex key")

    def test_multi_line_values_and_continuations(self):
        self.bad(wf("      - uses: actions/checkout@\n          " + CHECKOUT + "\n"), "continuation")
        self.bad(wf("      - uses: \"actions/checkout@\n          " + CHECKOUT + "\"\n"), "not closed")
        self.bad(wf("      - name: foo\n          uses: actions/checkout@v4\n"), "continuation")
        self.bad(wf("      - uses:\n          actions/checkout@v4\n"), "no value on its line")

    def test_block_scalar_forms_for_uses_or_with_indicators(self):
        self.bad(wf("      - uses: |\n          actions/checkout@v4\n"), "block scalar")
        self.bad(wf("      - run: |2\n          x\n"), "indentation indicator")

    def test_uses_spelled_in_another_case(self):
        self.bad(wf("      - USES: actions/checkout@v4\n"), "spelled")
        self.bad(wf("      - Uses: actions/checkout@v4\n"), "spelled")

    def test_document_markers_tabs_bom_and_control_characters(self):
        self.bad("---\n" + wf(""), "document marker")
        self.bad(wf("") + "---\nuses: x\n", "document marker")
        self.bad(wf("      - name: x\n\t  uses: actions/checkout@v4\n"), "tab indentation")
        self.bad("\ufeff" + wf(f"      - uses: actions/checkout@{CHECKOUT}\n"), "byte order mark")
        self.bad(wf(f"      - uses: actions/checkout@{CHECKOUT}\n") + "\x00", "NUL")
        self.bad(wf(f"      - uses: actions/checkout@{CHECKOUT}\n").replace("\n", "\r", 1), "lone carriage return")

    def test_a_file_without_a_jobs_mapping_is_not_silently_accepted(self):
        self.bad("", "no top-level `jobs:`")
        self.bad("# only a comment\n", "no top-level `jobs:`")
        self.bad("name: t\non:\n  push:\n", "no top-level `jobs:`")

    def test_json_style_workflows_and_flow_jobs_are_rejected(self):
        self.bad('{"jobs": {"j": {"steps": [{"uses": "actions/checkout@v4"}]}}}\n', "flow mapping")
        self.bad("name: t\non: push\njobs: {j: {runs-on: x, steps: [{uses: actions/checkout@v4}]}}\n", "unsupported value syntax")

    def test_space_before_the_colon_is_still_read_as_uses(self):
        self.assertTrue(has(evaluate(wf("      - uses : actions/checkout@v4\n")), "mutable ref"))
        self.assertTrue(has(evaluate(wf("      -   uses: actions/checkout@v4\n")), "mutable ref"))

    def test_a_bare_scalar_line_is_unsupported(self):
        self.bad(wf("      just a stray line\n"), "neither a `key: value` pair")

    def test_two_uses_in_one_step_are_both_checked(self):
        msgs = evaluate(wf(f"      - uses: actions/checkout@{CHECKOUT}\n        uses: actions/checkout@v4\n"))
        self.assertTrue(has(msgs, "mutable ref"))


def make_repo(tmp: pathlib.Path) -> pathlib.Path:
    """A temp repository root holding copies of the three real workflows."""
    wf_dir = tmp / ".github" / "workflows"
    wf_dir.mkdir(parents=True)
    for name in REQUIRED:
        shutil.copyfile(ROOT / ".github" / "workflows" / name, wf_dir / name)
    return tmp


class InventoryAndConfiguration(unittest.TestCase):
    def test_a_new_yaml_workflow_with_an_unapproved_dependency_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = make_repo(pathlib.Path(tmp))
            self.assertEqual(cap.check_repository(root)[0], [])
            (root / ".github" / "workflows" / "new.yaml").write_text(wf("      - uses: evil/action@" + OTHER_VALID_SHA + "\n"), encoding="utf-8")
            violations, _ = cap.check_repository(root)
            self.assertEqual(len(violations), 1)
            self.assertEqual(violations[0].file, ".github/workflows/new.yaml")
            self.assertIn("not in the approved list", violations[0].message)
            self.assertGreater(violations[0].line, 0)

    def test_missing_required_workflow(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = make_repo(pathlib.Path(tmp))
            (root / ".github" / "workflows" / "license-check.yml").unlink()
            violations, _ = cap.check_repository(root)
            self.assertTrue(any("required workflow file is missing" in v.message and v.file.endswith("license-check.yml") for v in violations))

    def test_empty_inventory_and_missing_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            violations, _ = cap.check_repository(root)
            self.assertTrue(any("missing" in v.message for v in violations))
            (root / ".github" / "workflows").mkdir(parents=True)
            violations, _ = cap.check_repository(root)
            self.assertTrue(any("empty workflow inventory" in v.message for v in violations))
            self.assertTrue(any("required workflow file is missing" in v.message for v in violations))

    def test_non_yaml_files_and_directories_are_not_workflows(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = make_repo(pathlib.Path(tmp))
            (root / ".github" / "workflows" / "notes.md").write_text("uses: actions/checkout@v4\n", encoding="utf-8")
            (root / ".github" / "workflows" / "sub.yml").mkdir()
            self.assertEqual(cap.check_repository(root)[0], [])

    def test_undecodable_workflow_bytes_fail(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = make_repo(pathlib.Path(tmp))
            (root / ".github" / "workflows" / "bad.yml").write_bytes(b"jobs:\n  j:\n    steps:\n      - uses: a/b@" + b"\xff" * 40 + b"\n")
            violations, _ = cap.check_repository(root)
            self.assertTrue(any("UTF-8" in v.message for v in violations))

    def test_allowlist_itself_must_be_well_formed(self):
        base = {"schema_version": 1, "approved_actions": {"a/b": "0" * 40}, "required_workflows": ["ci.yml"]}
        variants = {
            "short sha": {**base, "approved_actions": {"a/b": "0" * 39}},
            "uppercase sha": {**base, "approved_actions": {"a/b": "A" * 40}},
            "tag instead of sha": {**base, "approved_actions": {"a/b": "v4"}},
            "identity with path": {**base, "approved_actions": {"a/b/c": "0" * 40}},
            "empty actions": {**base, "approved_actions": {}},
            "extra key": {**base, "extra": 1},
            "missing key": {"schema_version": 1, "approved_actions": {"a/b": "0" * 40}},
            "wrong version": {**base, "schema_version": 2},
            "no required workflows": {**base, "required_workflows": []},
            "duplicate required": {**base, "required_workflows": ["ci.yml", "ci.yml"]},
            "required with directory": {**base, "required_workflows": ["x/ci.yml"]},
        }
        with tempfile.TemporaryDirectory() as tmp:
            for label, data in variants.items():
                p = pathlib.Path(tmp) / "a.json"
                p.write_text(json.dumps(data), encoding="utf-8")
                with self.assertRaises(cap.AllowlistError, msg=label):
                    cap.load_allowlist(p)
            p = pathlib.Path(tmp) / "dup.json"
            p.write_text('{"schema_version": 1, "schema_version": 1, "approved_actions": {"a/b": "' + "0" * 40 + '"}, "required_workflows": ["ci.yml"]}', encoding="utf-8")
            with self.assertRaises(cap.AllowlistError):
                cap.load_allowlist(p)
            with self.assertRaises(cap.AllowlistError):
                cap.load_allowlist(pathlib.Path(tmp) / "missing.json")

    def test_exit_statuses_are_nonzero_for_violations_and_configuration_errors(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = make_repo(pathlib.Path(tmp))
            self.assertEqual(cap.main(["--root", str(root)]), 0)
            (root / ".github" / "workflows" / "bad.yml").write_text(wf("      - uses: actions/checkout@v4\n"), encoding="utf-8")
            self.assertEqual(cap.main(["--root", str(root)]), 1)
            r = subprocess.run([sys.executable, str(TOOL), "--root", str(root)], capture_output=True, text=True)
            self.assertEqual(r.returncode, 1)
            self.assertRegex(r.stdout, r"\.github/workflows/bad\.yml:\d+: mutable ref")
            self.assertIn("ACTION_PINS_RESULT FAIL", r.stdout)
            self.assertEqual(cap.main(["--root", str(root), "--allowlist", str(pathlib.Path(tmp) / "nope.json")]), 2)

    def test_diagnostics_name_file_line_and_reason(self):
        text = wf("      - uses: actions/checkout@v4\n")
        violations, refs = cap.scan_workflow_text(text, "w.yml")
        self.assertEqual(violations, [])
        self.assertEqual(refs, [(text.split("\n").index("      - uses: actions/checkout@v4") + 1, "actions/checkout@v4")])


class CiWorkflowContract(unittest.TestCase):
    def setUp(self):
        self.ci = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8").replace("\r\n", "\n")

    def test_ci_runs_the_checker_early_in_the_desktop_job_after_setup_python(self):
        step = "      - name: Verify immutable GitHub Actions pins\n        run: python tools/check_action_pins.py\n"
        self.assertEqual(self.ci.count(step), 1)
        setup_python = self.ci.index("actions/setup-python@")
        rust = self.ci.index("- name: Rust toolchain")
        self.assertLess(setup_python, self.ci.index(step))
        self.assertLess(self.ci.index(step), rust)
        # nothing but the setup-python step's own `with:` block sits between them
        between = self.ci[setup_python : self.ci.index(step)]
        self.assertNotIn("run:", between)

    def test_the_two_required_check_names_are_unchanged(self):
        # the main-branch required-check ruleset binds exactly these contexts; renaming them is outside this checkpoint's authority
        self.assertRegex(self.ci, r"(?m)^  desktop:\n    name: \$\{\{ matrix\.name \}\}$")
        matrix = self.ci.split("matrix:", 1)[1].split("defaults:", 1)[0]
        self.assertEqual(re.findall(r"(?m)^\s+- name: (\S+)\s*$", matrix), ["windows-x64", "macos-arm64"])

    def test_python_unit_tests_still_run_in_ci(self):
        self.assertRegex(self.ci, r"unittest discover")


if __name__ == "__main__":
    unittest.main()
