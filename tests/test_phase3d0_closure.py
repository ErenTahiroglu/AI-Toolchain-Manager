"""Phase 3D-0 closure (Issue #17): tests of the pinned-fetch tool and of the consensus reviewer.

Nothing here contacts a remote host: the fetch tests use a loopback server on 127.0.0.1, the reviewer tests use synthetic files.
Nothing downloads, opens or runs jq or any other third-party program.
"""

import copy
import hashlib
import http.server
import importlib.util
import json
import os
import pathlib
import socketserver
import stat
import tempfile
import threading
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "tools" / f"{name}.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


fetch_pinned = load("fetch_pinned")
review_mod = load("review_jq_verification")

BODY = bytes(range(256)) * 40  # 10 240 bytes
SHA = hashlib.sha256(BODY).hexdigest()


class Handler(http.server.BaseHTTPRequestHandler):
    seen_headers = []
    port = 0

    def log_message(self, *a):
        pass

    def do_GET(self):
        Handler.seen_headers.append({k.lower(): v for k, v in self.headers.items()})
        path = self.path.split("?")[0]
        if path == "/ok":
            self.reply(BODY)
        elif path == "/long":
            self.reply(BODY + b"extra")
        elif path == "/long-nolength":
            # no Content-Length: the body just ends when the connection closes (HTTP/1.0 style)
            self.send_response(200)
            self.end_headers()
            self.wfile.write(BODY + b"extra")
            self.wfile.flush()
            self.close_connection = True
        elif path == "/short-nolength":
            self.send_response(200)
            self.end_headers()
            self.wfile.write(BODY[:-1])
            self.wfile.flush()
            self.close_connection = True
        elif path == "/short":
            self.reply(BODY[:-1])
        elif path == "/wrong":
            self.reply(bytes([BODY[0] ^ 1]) + BODY[1:])
        elif path == "/lying-length":
            self.send_response(200)
            self.send_header("Content-Length", str(len(BODY) + 5))
            self.end_headers()
            self.wfile.write(BODY)
        elif path == "/to-ok":
            self.redirect(f"http://127.0.0.1:{Handler.port}/ok?token=SECRET")
        elif path == "/to-localhost":
            self.redirect(f"http://localhost:{Handler.port}/ok")
        elif path == "/to-evil":
            self.redirect("https://evil.example/ok")
        elif path == "/to-ftp":
            self.redirect("ftp://127.0.0.1/ok")
        elif path == "/to-https-downgrade":
            self.redirect("http://127.0.0.1:1/x")
        elif path.startswith("/chain/"):
            n = int(path.split("/")[2])
            self.redirect(f"http://127.0.0.1:{Handler.port}/chain/{n + 1}" if n < 9 else f"http://127.0.0.1:{Handler.port}/ok")
        else:
            self.send_response(404)
            self.end_headers()

    def reply(self, body):
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def redirect(self, to):
        self.send_response(302)
        self.send_header("Location", to)
        self.send_header("Content-Length", "0")
        self.end_headers()


class FetchPinned(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = socketserver.TCPServer(("127.0.0.1", 0), Handler)
        Handler.port = cls.server.server_address[1]
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()

    def get(self, path, allow=("127.0.0.1",), size=len(BODY), sha=SHA):
        Handler.seen_headers.clear()
        tmp = tempfile.mkdtemp()
        dest = os.path.join(tmp, "artifact.bin")
        try:
            rec = fetch_pinned.fetch(f"http://127.0.0.1:{Handler.port}{path}", dest, size, sha, allow, loopback_http=True)
            return rec, dest, tmp
        except fetch_pinned.Refused as e:
            return str(e), dest, tmp

    def test_a_pinned_download_succeeds_and_leaves_one_non_executable_file(self):
        rec, dest, tmp = self.get("/ok")
        self.assertIsInstance(rec, dict, rec)
        self.assertEqual((rec["size"], rec["sha256"], rec["finalHost"]), (len(BODY), SHA, "127.0.0.1"))
        self.assertEqual(os.listdir(tmp), ["artifact.bin"])
        self.assertEqual(open(dest, "rb").read(), BODY)
        if os.name != "nt":
            self.assertEqual(stat.S_IMODE(os.stat(dest).st_mode) & 0o177, 0, "owner-only, no execute bit")

    def test_no_credential_of_any_kind_is_ever_sent(self):
        os.environ["GH_TOKEN"] = "ghp_SECRET_TOKEN_VALUE"
        os.environ["GITHUB_TOKEN"] = "ghp_SECRET_TOKEN_VALUE"
        try:
            self.get("/ok")
        finally:
            del os.environ["GH_TOKEN"], os.environ["GITHUB_TOKEN"]
        self.assertTrue(Handler.seen_headers)
        for h in Handler.seen_headers:
            self.assertNotIn("authorization", h)
            self.assertNotIn("cookie", h)
            self.assertFalse(any("SECRET" in v for v in h.values()), h)

    def test_an_allowed_redirect_is_followed_and_logged_without_its_query_string(self):
        rec, _, _ = self.get("/to-ok")
        self.assertIsInstance(rec, dict, rec)
        self.assertEqual([h["path"] for h in rec["hops"]], ["/to-ok", "/ok"])
        self.assertNotIn("SECRET", json.dumps(rec), "the (possibly signed) query string is never logged")

    def test_an_unexpected_redirect_host_scheme_or_chain_is_refused_and_nothing_is_kept(self):
        for path, why in [
            ("/to-localhost", "host_not_allowed:localhost"),
            ("/to-evil", "host_not_allowed:evil.example"),
            ("/to-ftp", "scheme_not_allowed:ftp"),
            ("/chain/0", "too_many_redirects"),
        ]:
            rec, dest, tmp = self.get(path)
            self.assertEqual(rec, why, path)
            self.assertEqual(os.listdir(tmp), [], f"{path}: nothing is kept")
        # the same redirect IS accepted when the second name is explicitly allowed
        rec, _, _ = self.get("/to-localhost", allow=("127.0.0.1", "localhost"))
        self.assertIsInstance(rec, dict, rec)

    def test_a_wrong_size_content_or_digest_is_refused_and_nothing_is_kept(self):
        for path, expect in [("/long", "content_length_differs"), ("/long-nolength", "body_longer_than_pinned_size"), ("/short", "content_length_differs"), ("/short-nolength", "size_differs"), ("/wrong", "sha256_differs"), ("/lying-length", "content_length_differs")]:
            rec, dest, tmp = self.get(path)
            self.assertTrue(str(rec).startswith(expect), (path, rec))
            self.assertEqual(os.listdir(tmp), [], path)
        rec, _, tmp = self.get("/ok", sha="0" * 64)
        self.assertEqual(rec, "sha256_differs")
        self.assertEqual(os.listdir(tmp), [])
        rec, _, tmp = self.get("/missing")
        self.assertTrue(str(rec).startswith("unexpected_status:404"))

    def test_plain_http_to_a_real_host_and_an_existing_destination_are_refused(self):
        tmp = tempfile.mkdtemp()
        dest = os.path.join(tmp, "a.bin")
        with self.assertRaises(fetch_pinned.Refused) as cm:
            fetch_pinned.fetch("http://github.com/x", dest, 1, "0" * 64, ["github.com"])
        self.assertTrue(str(cm.exception).startswith("scheme_not_allowed"))
        with open(dest, "wb") as fh:
            fh.write(b"mine")
        rec, _, _ = self.get("/ok")
        with self.assertRaises(fetch_pinned.Refused) as cm:
            fetch_pinned.fetch(f"http://127.0.0.1:{Handler.port}/ok", dest, len(BODY), SHA, ["127.0.0.1"], loopback_http=True)
        self.assertEqual(str(cm.exception), "destination_exists")
        with open(dest, "rb") as fh:
            self.assertEqual(fh.read(), b"mine")

    def test_the_tool_cannot_execute_anything(self):
        text = (ROOT / "tools" / "fetch_pinned.py").read_text(encoding="utf-8")
        for needle in ["subprocess", "os.system", "os.exec", "os.spawn", "Popen", "eval(", "exec(", "pickle", "marshal"]:
            self.assertNotIn(needle, text)


# ---- the consensus reviewer ----------------------------------------------------------------------------------------------------------


def synthetic_session(root, mutate=None):
    """A complete, consistent session for two made-up artifacts, with made-up pins."""
    pins = copy.deepcopy(review_mod.PINS)
    arts = {"jq-windows-amd64.exe": b"windows bytes" * 50, "jq-macos-arm64": b"mac bytes" * 70}
    pins["artifacts"] = {n: {"size": len(b), "sha256": hashlib.sha256(b).hexdigest(), "sig": n + ".asc"} for n, b in arts.items()}
    pinned = {"jq-windows-amd64.exe.asc": b"sig-win", "jq-macos-arm64.asc": b"sig-mac", "jq-release-new.key": b"key"}
    listing = "".join(f"{a['sha256']}  {n}\n" for n, a in pins["artifacts"].items()).encode()
    pinned["sha256sum.txt"] = listing
    pins["pinnedText"] = {n: hashlib.sha256(b).hexdigest() for n, b in pinned.items()}
    (root / "pinned").mkdir()
    for n, b in pinned.items():
        (root / "pinned" / n).write_bytes(b)
    fpr = pins["gpgFingerprint"]
    (root / "keyring.colons").write_text(f"pub:-:4096:1:B0DA60FB454BAF18:1753967253:::-:::scESC::::::23::0:\nfpr:::::::::{fpr}:\nuid:-::::1753967253::AAAA::jq Release Signing Key::::::::::0:\nsub:-:4096:1:104F445439A0A518:1753967253::::::e::::::23:\nfpr:::::::::0343D5D7CCA45BCC2596B849104F445439A0A518:\n", encoding="utf-8")
    (root / "fetch.log").write_text(json.dumps({"file": "x", "hops": [{"scheme": "https", "host": "github.com", "path": "/p", "status": 302}, {"scheme": "https", "host": "release-assets.githubusercontent.com", "path": "/q", "status": 200}]}) + "\n", encoding="utf-8")
    san = f"{pins['repositoryUri']}/{pins['workflowPath']}@{pins['ref']}"
    for name, data in arts.items():
        (root / "artifacts" / name).mkdir(parents=True)
        (root / "artifacts" / name / name).write_bytes(data)
        sha = pins["artifacts"][name]["sha256"]
        cert = {
            "issuer": pins["issuer"], "subjectAlternativeName": san, "githubWorkflowRepository": pins["repository"], "githubWorkflowRef": pins["ref"],
            "githubWorkflowSHA": pins["commit"], "sourceRepositoryURI": pins["repositoryUri"], "sourceRepositoryDigest": pins["commit"],
            "sourceRepositoryRef": pins["ref"], "sourceRepositoryIdentifier": pins["repositoryId"], "sourceRepositoryOwnerIdentifier": pins["ownerId"],
            "buildSignerDigest": pins["commit"], "runnerEnvironment": "github-hosted",
        }
        gh = [{"attestation": {"bundle": {}}, "verificationResult": {"signature": {"certificate": cert}, "verifiedTimestamps": [{"type": "Tlog"}],
               "statement": {"predicateType": "https://slsa.dev/provenance/v1", "subject": [{"name": name, "digest": {"sha256": sha}}]}}}]
        for tag in ("gh", "gh-offline"):
            (root / f"{name}.{tag}.json").write_text(json.dumps(gh), encoding="utf-8")
            (root / f"{name}.{tag}.exit").write_text("0", encoding="utf-8")
        checks = [{"check": f"c{i}", "ok": True, "detail": ""} for i in range(27)]
        checks.append({"check": "the expected file name is a subject with the expected SHA-256", "ok": True, "detail": f"{name}: {sha}"})
        checks.append({"check": "local file SHA-256 equals the attested digest", "ok": True, "detail": sha})
        custom = {"ok": True, "checks": checks, "certificateExtensions": {"sourceRepositoryDigest": pins["commit"], "sourceRepositoryRef": pins["ref"],
                  "sourceRepositoryIdentifier": pins["repositoryId"], "sourceRepositoryOwnerIdentifier": pins["ownerId"]}}
        (root / f"{name}.custom.json").write_text(json.dumps(custom), encoding="utf-8")
        (root / f"{name}.gpg.status").write_text(f"[GNUPG:] NEWSIG\n[GNUPG:] GOODSIG B0DA60FB454BAF18 jq Release Signing Key\n[GNUPG:] VALIDSIG {fpr} 2026-06-20 1781964636 0 4 0 1 10 00 {fpr}\n[GNUPG:] TRUST_UNDEFINED 0 pgp\n", encoding="utf-8")
    if mutate:
        mutate(root, pins)
    return pins


class Reviewer(unittest.TestCase):
    def run_session(self, mutate=None):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            pins = synthetic_session(root, mutate)
            r = review_mod.review(str(root), pins)
            return r, [c["check"] for c in r.checks if not c["ok"]]

    def test_a_complete_consistent_session_passes(self):
        r, failed = self.run_session()
        self.assertTrue(r.passed, failed)
        self.assertGreater(len(r.checks), 50)

    def test_the_real_pins_with_an_empty_session_fail(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertFalse(review_mod.review(tmp).passed)

    def expect_failure(self, mutate, fragment):
        r, failed = self.run_session(mutate)
        self.assertFalse(r.passed, "the session must fail")
        self.assertTrue(any(fragment in f for f in failed), (fragment, failed))

    def test_wrong_bytes_size_or_pinned_text_fail(self):
        W, M = "jq-windows-amd64.exe", "jq-macos-arm64"
        self.expect_failure(lambda r, p: (r / "artifacts" / W / W).write_bytes(b"x" * len((r / "artifacts" / W / W).read_bytes())), "SHA-256 equals the pinned value")
        self.expect_failure(lambda r, p: (r / "artifacts" / W / W).write_bytes((r / "artifacts" / W / W).read_bytes() + b"!"), "size is the pinned size")
        self.expect_failure(lambda r, p: (r / "pinned" / "jq-release-new.key").write_bytes(b"other key"), "jq-release-new.key has the pinned SHA-256")
        self.expect_failure(lambda r, p: (r / "pinned" / f"{M}.asc").write_bytes(b"other sig"), "has the pinned SHA-256")
        self.expect_failure(lambda r, p: (r / "artifacts" / M / M).unlink(), "the downloaded file exists")
        self.expect_failure(lambda r, p: (r / "pinned" / "sha256sum.txt").write_bytes(b"nothing\n"), "pinned SHA-256")

    def test_the_official_verifier_failing_or_returning_something_else_fails(self):
        W = "jq-windows-amd64.exe"
        self.expect_failure(lambda r, p: (r / f"{W}.gh.exit").write_text("1"), "exited 0")
        self.expect_failure(lambda r, p: (r / f"{W}.gh.json").write_text("[]"), "non-empty structured result")
        self.expect_failure(lambda r, p: (r / f"{W}.gh.json").write_text("not json"), "non-empty structured result")

        def other_digest(r, p):
            d = json.loads((r / f"{W}.gh.json").read_text())
            d[0]["verificationResult"]["statement"]["subject"][0]["digest"]["sha256"] = "f" * 64
            (r / f"{W}.gh.json").write_text(json.dumps(d))

        self.expect_failure(other_digest, "subject name and SHA-256 are the pinned ones")

        def other_commit(r, p):
            d = json.loads((r / f"{W}.gh.json").read_text())
            d[0]["verificationResult"]["signature"]["certificate"]["sourceRepositoryDigest"] = "0" * 40
            (r / f"{W}.gh.json").write_text(json.dumps(d))

        self.expect_failure(other_commit, "sourceRepositoryDigest equals the pinned value")

        def other_runner(r, p):
            d = json.loads((r / f"{W}.gh.json").read_text())
            d[0]["verificationResult"]["signature"]["certificate"]["runnerEnvironment"] = "self-hosted"
            (r / f"{W}.gh.json").write_text(json.dumps(d))

        self.expect_failure(other_runner, "runnerEnvironment equals the pinned value")

        def no_timestamp(r, p):
            d = json.loads((r / f"{W}.gh.json").read_text())
            d[0]["verificationResult"]["verifiedTimestamps"] = []
            (r / f"{W}.gh.json").write_text(json.dumps(d))

        self.expect_failure(no_timestamp, "transparency-log timestamp")

    def test_the_offline_replay_must_agree_with_the_online_result(self):
        W = "jq-windows-amd64.exe"
        self.expect_failure(lambda r, p: (r / f"{W}.gh-offline.exit").write_text("1"), "offline replay) exited 0")

        def offline_other_digest(r, p):
            d = json.loads((r / f"{W}.gh-offline.json").read_text())
            d[0]["verificationResult"]["statement"]["subject"][0]["digest"]["sha256"] = "e" * 64
            (r / f"{W}.gh-offline.json").write_text(json.dumps(d))

        self.expect_failure(offline_other_digest, "offline replay result 0: subject")

    def test_disagreement_between_the_two_verifiers_fails_closed_in_both_directions(self):
        W = "jq-windows-amd64.exe"

        def custom_fails(r, p):
            c = json.loads((r / f"{W}.custom.json").read_text())
            c["ok"] = False
            c["checks"][3]["ok"] = False
            (r / f"{W}.custom.json").write_text(json.dumps(c))

        self.expect_failure(custom_fails, "AGREE")

        def gh_fails_custom_passes(r, p):
            (r / f"{W}.gh.exit").write_text("1")

        self.expect_failure(gh_fails_custom_passes, "AGREE")

        def custom_did_not_hash_the_download(r, p):
            c = json.loads((r / f"{W}.custom.json").read_text())
            c["checks"] = [x for x in c["checks"] if x["check"] != "local file SHA-256 equals the attested digest"] + [{"check": "pad", "ok": True, "detail": ""}]
            (r / f"{W}.custom.json").write_text(json.dumps(c))

        self.expect_failure(custom_did_not_hash_the_download, "compared the DOWNLOADED bytes")

    def test_gpg_problems_fail(self):
        M = "jq-macos-arm64"
        fpr = review_mod.PINS["gpgFingerprint"]
        self.expect_failure(lambda r, p: (r / f"{M}.gpg.status").write_text("[GNUPG:] ERRSIG X\n[GNUPG:] NO_PUBKEY X\n"), "exactly one valid GPG signature")
        self.expect_failure(lambda r, p: (r / f"{M}.gpg.status").write_text(f"[GNUPG:] BADSIG X y\n[GNUPG:] VALIDSIG {fpr} a b c d e f g h {fpr}\n"), "no bad, expired")
        other = "A" * 40
        self.expect_failure(lambda r, p: (r / f"{M}.gpg.status").write_text(f"[GNUPG:] GOODSIG X y\n[GNUPG:] VALIDSIG {other} a b c d e f g h {other}\n"), "pinned fingerprint")
        self.expect_failure(lambda r, p: (r / f"{M}.gpg.status").write_text(f"[GNUPG:] GOODSIG X y\n[GNUPG:] VALIDSIG {fpr} a b c d e f g h {other}\n"), "pinned fingerprint")
        self.expect_failure(lambda r, p: (r / f"{M}.gpg.status").write_text(f"[GNUPG:] GOODSIG X y\n[GNUPG:] VALIDSIG {fpr} a b c d e f g h {fpr}\n[GNUPG:] VALIDSIG {fpr} a b c d e f g h {fpr}\n"), "exactly one valid")

    def test_a_second_key_in_the_keyring_or_an_unexpected_host_fails(self):
        def second_key(r, p):
            with open(r / "keyring.colons", "a", encoding="utf-8") as f:
                f.write("pub:-:4096:1:1111111111111111:1:::-:::scESC::::::23::0:\nfpr:::::::::" + "1" * 40 + ":\n")

        self.expect_failure(second_key, "exactly one primary key")

        def other_host(r, p):
            (r / "fetch.log").write_text(json.dumps({"hops": [{"scheme": "https", "host": "evil.example", "path": "/", "status": 200}]}) + "\n")

        self.expect_failure(other_host, "allowed host")

        def http_hop(r, p):
            (r / "fetch.log").write_text(json.dumps({"hops": [{"scheme": "http", "host": "github.com", "path": "/", "status": 200}]}) + "\n")

        self.expect_failure(http_hop, "https and an allowed host")
        self.expect_failure(lambda r, p: (r / "fetch.log").unlink(), "https and an allowed host")

    @unittest.skipIf(os.name == "nt", "execute permission bits exist on POSIX only")
    def test_an_executable_file_fails(self):
        self.expect_failure(lambda r, p: os.chmod(r / "artifacts" / "jq-macos-arm64" / "jq-macos-arm64", 0o755), "no execute permission")

    def test_the_reviewer_runs_nothing_and_contacts_nothing(self):
        text = (ROOT / "tools" / "review_jq_verification.py").read_text(encoding="utf-8")
        for needle in ["subprocess", "os.system", "socket", "urllib", "http.client", "requests"]:
            self.assertNotIn(needle, text)

    def test_the_pins_are_exactly_the_ones_in_the_documents(self):
        p = review_mod.PINS
        self.assertEqual(p["artifacts"]["jq-windows-amd64.exe"], {"size": 1035264, "sha256": "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627", "sig": "jq-windows-amd64.exe.asc"})
        self.assertEqual(p["artifacts"]["jq-macos-arm64"]["size"], 841504)
        self.assertEqual(p["artifacts"]["jq-macos-arm64"]["sha256"], "2d75340ba57a4b4b4c8708a21c2dc8e958a48aaa8bba13b27f77f6e4c0eca07e")
        self.assertEqual((p["signatureCommit"], p["gpgFingerprint"]), ("f0dabad36a5fdb37393829e6bd4c465d70c40a3a", "93079A9511EFC0B7D6CDBFB5B0DA60FB454BAF18"))
        ev = ROOT / "docs" / "phase-3d0" / "evidence" / "pinned-f0dabad36a5fdb37393829e6bd4c465d70c40a3a"
        for name, want in p["pinnedText"].items():
            self.assertEqual(hashlib.sha256((ev / name).read_bytes()).hexdigest(), want, name)


class Documents(unittest.TestCase):
    def test_the_corrected_request_names_every_pin_host_and_fail_closed_rule(self):
        en = (ROOT / "docs" / "phase-3d0" / "REQUEST-A.md").read_text(encoding="utf-8")
        tr = (ROOT / "docs" / "phase-3d0" / "REQUEST-A.tr.md").read_text(encoding="utf-8")
        needles = ["f0dabad36a5fdb37393829e6bd4c465d70c40a3a", "93079A9511EFC0B7D6CDBFB5B0DA60FB454BAF18", "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627",
                   "2d75340ba57a4b4b4c8708a21c2dc8e958a48aaa8bba13b27f77f6e4c0eca07e", "github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com",
                   "raw.githubusercontent.com", "api.github.com", "tuf-repo-cdn.sigstore.dev", "tuf-repo.github.com"]
        for needle in needles:
            self.assertIn(needle, en, needle)
            self.assertIn(needle, tr, needle)
        for needle in ["Fail-closed rules", "--deny-self-hosted-runners", "--source-digest", "--signer-digest", "--source-ref", "--cert-oidc-issuer"]:
            self.assertIn(needle, en, needle)
        self.assertIn("TEKLİFTİR", tr)
        self.assertIn("İndirme izni verilmemiştir", tr)

    def test_the_old_github_com_only_wording_is_gone_from_the_gate_document(self):
        text = (ROOT / "docs" / "PHASE-3D0.md").read_text(encoding="utf-8")
        self.assertIn("superseded", text)
        self.assertNotIn("to `github.com` only**", text)
        self.assertIn("REQUEST-A.md", text)

    def test_the_stored_trusted_root_is_declared_not_authenticated(self):
        text = (ROOT / "docs" / "phase-3d0" / "evidence" / "README.md").read_text(encoding="utf-8")
        self.assertIn("NOT an authenticated trust anchor", text)


if __name__ == "__main__":
    unittest.main()
