"""ATM-GOV-01 (Issue #33) contract, no running app needed.

"Checkpoint Merkezi" is a frontend-only, read-only viewer for hand-pasted, untrusted metadata:

* it adds no Rust, no IPC command, no Tauri permission, no migration, no dependency, no network/file/clipboard/storage access,
* every record is labelled KAYNAK BEYANI — DOĞRULANMADI and there is no verdict member,
* the three documents exist, name the (empty) external engine and its issue, and agree with the code on the label, the schema version and the limits,
* nothing in the application renders HTML from data.
"""
import json
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
LABEL = "KAYNAK BEYANI — DOĞRULANMADI"
VERSION = "atm.checkpoint-view.v1"
DOCS = ("CONTRACT_AND_BOUNDARY", "THREAT_MODEL_AND_TESTS", "EXTERNAL_ENGINE_HANDOFF")


def read(rel):
    return (ROOT / rel).read_text(encoding="utf-8")


class FrontendOnly(unittest.TestCase):
    def test_no_rust_command_capability_migration_or_dependency_was_added(self):
        for rel in ("src-tauri/src/commands.rs", "src-tauri/src/lib.rs", "src-tauri/src/db.rs"):
            self.assertNotRegex(read(rel), r"(?i)checkpoint[-_ ]?(view|merkezi|center|centre)")  # WAL 'checkpoint' in db.rs/backups.rs is unrelated
        for path in (ROOT / "src-tauri" / "src").rglob("*.rs"):
            self.assertNotIn("checkpoint-view", path.read_text(encoding="utf-8"), str(path))
        perms = json.loads(read("src-tauri/capabilities/default.json"))["permissions"]
        for p in perms:
            self.assertIsNone(re.search(r"fs|shell|http|opener|dialog|clipboard|process|os:|path|window|checkpoint", p, re.I), p)  # (the pre-existing catalog command permission "allow-preview-github-url" is not new)
        names = sorted(p.name for p in (ROOT / "src-tauri" / "migrations").glob("*.sql"))
        self.assertEqual(len(names), 10)
        self.assertEqual(names[-1], "0010_codex_cli_surface.sql")
        pkg = json.loads(read("package.json"))
        every = {**pkg.get("dependencies", {}), **pkg.get("devDependencies", {})}
        self.assertFalse([n for n in every if re.search(r"octokit|github|zod|ajv|yup|joi|markdown|marked|remark|dompurify|sanitize|html", n, re.I)], every)

    def test_the_new_sources_do_no_io_and_render_no_html(self):
        forbidden = r"invoke\(|@tauri-apps|from \"\.\./api\"|from \"\./api\"|\bapi\.|fetch\(|XMLHttpRequest|WebSocket|navigator\.|localStorage|sessionStorage|indexedDB|setTimeout|setInterval|queueMicrotask|createObjectURL|FileReader|window\.open|child_process|require\(|dangerouslySetInnerHTML|innerHTML|eval\(|new Function|useLoad"
        for rel in ("src/checkpointView.ts", "src/pages/Checkpoints.tsx"):
            self.assertIsNone(re.search(forbidden, read(rel)), rel)
        page = read("src/pages/Checkpoints.tsx")
        self.assertNotRegex(page, r"<a[\s>]|href=|window\.location|target=")
        self.assertEqual(re.findall(r'\sfrom "([^"]+)";', page), ["react", "../checkpointView"])
        self.assertEqual(re.findall(r'^import .*? from "([^"]+)";', read("src/checkpointView.ts"), re.M), ["./review"])

    def test_nothing_in_the_application_renders_html_from_data(self):
        for path in (ROOT / "src").rglob("*"):
            if path.suffix in (".ts", ".tsx") and ".test." not in path.name:
                self.assertNotIn("dangerouslySetInnerHTML", path.read_text(encoding="utf-8"), str(path))

    def test_the_page_is_a_separate_navigation_entry_and_unchanged_pages_remain(self):
        app = read("src/App.tsx")
        self.assertIn('{ id: "checkpoints", label: "Checkpoint Merkezi" }', app)
        self.assertIn('{page === "checkpoints" && <CheckpointsPage />}', app)
        self.assertIn("<ErrorBoundary key={page}>", app)  # a page is re-mounted on every navigation: its state is destroyed on leaving
        for label in ("Ana Sayfa", "Projeler", "Araç Kütüphanesi", "Öneriler", "Bilgisayar Envanteri", "Kurulum Planı", "İstem Kütüphanesi", "Etkinlik ve Denetim", "Ayarlar"):
            self.assertIn('label: "%s"' % label, app)


class NoVerdict(unittest.TestCase):
    def test_the_label_and_the_schema_version_are_the_documented_ones(self):
        ts = read("src/checkpointView.ts")
        self.assertIn('export const TRUST_LABEL = "%s";' % LABEL, ts)
        self.assertIn('export const SCHEMA_VERSION = "%s";' % VERSION, ts)

    def test_the_view_model_has_no_member_that_could_carry_a_verdict(self):
        ts = read("src/checkpointView.ts")
        body = ts[ts.index("export interface JobView") : ts.index("// ---- 1. the raw-text scanner")]
        members = set(re.findall(r"^\s+(\w+)\??:", body, re.M))
        self.assertTrue(members)
        self.assertFalse(members & {"verified", "isVerified", "accepted", "passed", "pass", "trusted", "verdict", "status", "result", "valid", "approved", "score", "total", "progress"}, members)
        self.assertIn("trust", members)

    def test_the_page_never_speaks_in_its_own_voice_about_success_or_verification(self):
        for rel in ("src/pages/Checkpoints.tsx", "src/checkpointView.ts"):
            text = re.sub(r"//[^\n]*", "", read(rel))
            self.assertNotRegex(text, r"(?i)doğrulandı|onaylandı|başarıyla|✓|✔|\bPASSED\b|\bVERIFIED\b|badge")


class Documents(unittest.TestCase):
    def test_the_three_documents_exist_and_carry_the_boundaries(self):
        docs = {n: read("docs/atm-gov-01/%s.md" % n) for n in DOCS}
        joined = "\n".join(docs.values())
        for needle in (LABEL, VERSION, "REAL_INSTALLATION_NO_GO", "REAL_CONFIG_WRITE_NO_GO", "#14", "#25", "#32", "#33", "#34", "#35"):
            self.assertIn(needle, joined, needle)
        handoff = docs["EXTERNAL_ENGINE_HANDOFF"]
        self.assertIn("ErenTahiroglu/Evidence-Checkpoint-Engine", handoff)
        self.assertRegex(handoff, r"(?i)empty")
        self.assertRegex(handoff, r"issue #1|ECE #1")
        self.assertRegex(handoff, r"(?i)no verifier logic in React")
        for needle in ("source assertion", "machine evidence", "independent human approval"):
            self.assertIn(needle, handoff.lower().replace("**", ""), needle)
        # no document may CLAIM acceptance or readiness (the phrase may appear in a negation or as a future condition: "an independently accepted #33")
        self.assertNotRegex(joined, r"(?i)(?:is|was|has been|been) independently accepted")
        self.assertNotRegex(joined, r"(?i)(?<!not )(?<!not\W)production[- ]ready")

    def test_the_documented_limits_are_the_enforced_limits(self):
        ts = read("src/checkpointView.ts")
        limits = dict((k, int(v.replace("_", ""))) for k, v in re.findall(r"^\s+(\w+): ([0-9_]+),", ts[ts.index("export const LIMITS") : ts.index("} as const")], re.M))
        doc = read("docs/atm-gov-01/CONTRACT_AND_BOUNDARY.md")
        self.assertEqual(limits["chars"], 40000)
        self.assertIn("40 000", doc)
        for key, label in (("depth", "nesting ≤ **8**"), ("checkpoints", "≤ 10 checkpoints"), ("workflows", "≤ 5 workflows"), ("jobs", "≤ 30 jobs"), ("artifacts", "≤ 10 artifacts"), ("issues", "≤ 20 issues"), ("pullRequests", "≤ 20 pull requests"), ("limitations", "≤ 10 limitations")):
            self.assertIn(label, doc, key)
        self.assertEqual((limits["depth"], limits["checkpoints"], limits["workflows"], limits["jobs"], limits["artifacts"], limits["issues"], limits["pullRequests"], limits["limitations"]), (8, 10, 5, 30, 10, 20, 20, 10))
        self.assertEqual((limits["phase"], limits["name"], limits["ref"], limits["limitation"], limits["values"]), (80, 80, 100, 160, 3000))


if __name__ == "__main__":
    unittest.main()
