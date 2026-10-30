"""LICENSE-01 (Issue #41) contract, no running app needed. Targeted checks only:

* LICENSE.md holds the official PolyForm Noncommercial License 1.0.0 text unmodified (git-blob hash of the upstream file, recomputed offline),
* the Required Notice is the exact owner-supplied line and sits OUTSIDE the official text,
* the project's own package.json / lockfile root entry / Cargo.toml carry exactly the SPDX identifier,
* the README links the license and keeps three statements (source-available, not OSI-approved, no commercial-use grant).

It does not judge legal sufficiency, ownership, dependency compatibility or what "noncommercial" covers, and it says nothing about the default
branch: `main` is licensed only after a separately authorised promotion (docs/license-01/LICENSE_NOTES.md section 6).
"""
import hashlib
import json
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]

SPDX = "PolyForm-Noncommercial-1.0.0"
NOTICE = "Required Notice: Copyright (c) 2026 Eren Tahiroğlu"
# polyformproject/polyform-licenses, ref `1.0.0` (a BRANCH) = commit 76a278c402bc43b8d2b561da140b0f3e17263015, file PolyForm-Noncommercial-1.0.0.md
UPSTREAM_GIT_BLOB_SHA1 = "5ecc88cfc4b1cff608ed640efe913c9dd97935c3"
UPSTREAM_SHA256 = "c0ea4a896d2c8c394b29f9427589996db826cd501c512279ff0ed3ef48fabbe5"
UPSTREAM_SIZE = 4563
FIRST_LINE = "# PolyForm Noncommercial License 1.0.0"
SEPARATOR = "\n\n---\n\n"


def license_bytes():
    # .gitattributes normalises to LF, but a Windows checkout with autocrlf may differ: compare on LF.
    return (ROOT / "LICENSE.md").read_bytes().replace(b"\r\n", b"\n")


def official_block():
    data = license_bytes()
    head, sep, body = data.partition(SEPARATOR.encode("utf-8"))
    assert sep, "LICENSE.md lacks the separator between the Required Notice and the license text"
    return head.decode("utf-8"), body


class LicenseText(unittest.TestCase):
    def test_official_text_is_byte_identical_to_the_upstream_file(self):
        _, body = official_block()
        self.assertEqual(len(body), UPSTREAM_SIZE)
        self.assertEqual(hashlib.sha1(b"blob %d\0" % len(body) + body).hexdigest(), UPSTREAM_GIT_BLOB_SHA1)
        self.assertEqual(hashlib.sha256(body).hexdigest(), UPSTREAM_SHA256)
        self.assertTrue(body.decode("utf-8").startswith(FIRST_LINE + "\n"))

    def test_required_notice_is_exact_and_outside_the_official_text(self):
        head, body = official_block()
        self.assertEqual(head, NOTICE)
        # (the official text mentions `Required Notice:` inside its own Notices section, in a quoted example; no LINE of it may begin with the string)
        self.assertEqual([l for l in body.decode("utf-8").splitlines() if l.startswith("Required Notice:")], [])
        # the license's own Notices section refers to plain-text lines beginning with "Required Notice:"
        self.assertRegex(license_bytes().decode("utf-8").splitlines()[0], r"^Required Notice: ")


class Metadata(unittest.TestCase):
    def test_own_manifests_carry_exactly_the_identifier(self):
        pkg = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))
        self.assertEqual(pkg["license"], SPDX)
        lock = json.loads((ROOT / "package-lock.json").read_text(encoding="utf-8"))
        self.assertEqual(lock["packages"][""]["license"], SPDX)
        cargo = (ROOT / "src-tauri" / "Cargo.toml").read_text(encoding="utf-8")
        package = re.search(r"\[package\]\r?\n(.*?)\r?\n\r?\n", cargo, re.S).group(1)
        self.assertEqual(re.findall(r'^license\s*=\s*"([^"]*)"', package, re.M), [SPDX])
        self.assertNotIn("license-file", package)

    def test_no_third_party_package_metadata_was_given_the_identifier(self):
        lock = json.loads((ROOT / "package-lock.json").read_text(encoding="utf-8"))
        for path, entry in lock["packages"].items():
            if path:
                self.assertNotEqual(entry.get("license"), SPDX, path)


class Readme(unittest.TestCase):
    def test_links_the_license_and_states_its_limits(self):
        text = (ROOT / "README.md").read_text(encoding="utf-8")
        section = text[text.index("## Lisans"):text.index("## Dallar")]
        self.assertIn("](LICENSE.md)", section)
        self.assertIn(SPDX, section)
        self.assertIn("https://polyformproject.org/licenses/noncommercial/1.0.0", section)
        self.assertIn("source-available", section)
        self.assertIn("açık kaynak lisansı değildir", section)
        self.assertIn("ticari kullanım hakkı vermez", section)
        self.assertIn("lisansları **değişmez**", section)

    def test_the_notes_state_the_default_branch_caveat(self):
        notes = (ROOT / "docs" / "license-01" / "LICENSE_NOTES.md").read_text(encoding="utf-8")
        self.assertIn("Branch caveat", notes)
        self.assertIn("68eda3f0407490fabccb517dcf822ab4818fc0bf", notes)
        self.assertIn("is NOT licensed by this change", notes)


if __name__ == "__main__":
    unittest.main()
