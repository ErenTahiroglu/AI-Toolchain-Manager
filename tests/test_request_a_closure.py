"""Request A closure (Issue #19): egress guard, six-fact reporting model, GPG error classification, byte-exact bundle extraction.

Everything here uses local mocks or the evidence already stored in the repository. No external host is contacted, nothing is
downloaded, and nothing third-party is executed (the only child process is this Python interpreter).
"""

import contextlib
import importlib.util
import io
import json
import os
import pathlib
import socket
import socketserver
import subprocess
import sys
import tempfile
import threading
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
BLOB_HOST = "tmaproduction.blob.core.windows.net"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules.setdefault(name, mod)
    spec.loader.exec_module(mod)
    return mod


guard_mod = load("egress_guard", ROOT / "tools" / "egress_guard.py")
review_mod = load("review_jq_verification", ROOT / "tools" / "review_jq_verification.py")
extract_mod = load("extract_sigstore_bundles", ROOT / "tools" / "extract_sigstore_bundles.py")
closure_tests = load("test_phase3d0_closure_for_reuse", ROOT / "tests" / "test_phase3d0_closure.py")
synthetic_session = closure_tests.synthetic_session


class Upstream:
    """A local TCP echo server standing in for a remote host; counts accepted connections."""

    def __init__(self):
        self.accepted = 0
        outer = self

        class H(socketserver.BaseRequestHandler):
            def handle(self):
                outer.accepted += 1
                try:
                    while True:
                        data = self.request.recv(4096)
                        if not data:
                            return
                        self.request.sendall(data)
                except OSError:
                    return

        class S(socketserver.ThreadingTCPServer):
            daemon_threads = True
            allow_reuse_address = True

        self.server = S(("127.0.0.1", 0), H)
        self.port = self.server.server_address[1]
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def close(self):
        self.server.shutdown()
        self.server.server_close()


def raw_request(port, payload, read=True):
    s = socket.create_connection(("127.0.0.1", port), timeout=10)
    s.sendall(payload)
    out = b""
    if read:
        try:
            out = s.recv(4096)
        except OSError:
            pass
    return s, out


class GuardTests(unittest.TestCase):
    def setUp(self):
        self.up = Upstream()
        self.addCleanup(self.up.close)

    def make(self, allow, override=None):
        g = guard_mod.Guard(allow, override if override is not None else {"allowed.example": ("127.0.0.1", self.up.port), "blocked.example": ("127.0.0.1", self.up.port)})
        port = g.start()
        self.addCleanup(g.stop)
        return g, port

    def test_an_allowed_host_is_tunnelled_and_the_bytes_are_counted(self):
        g, port = self.make(["allowed.example"])
        s, resp = raw_request(port, b"CONNECT Allowed.Example:443 HTTP/1.1\r\nHost: allowed.example:443\r\n\r\n")
        self.assertTrue(resp.startswith(b"HTTP/1.1 200"), resp)
        s.sendall(b"ping")
        self.assertEqual(s.recv(16), b"ping")
        s.close()
        for _ in range(100):
            if any(e["event"] == "tunnel" for e in g.events):
                break
            threading.Event().wait(0.05)
        rep = g.report()
        self.assertEqual([h["host"] for h in rep["contactedHosts"]], ["allowed.example"])
        self.assertEqual((rep["contactedHosts"][0]["bytesUp"], rep["contactedHosts"][0]["bytesDown"]), (4, 4))
        self.assertEqual(rep["incidents"], [])
        self.assertEqual(self.up.accepted, 1)

    def test_every_unlisted_or_malformed_destination_is_refused_before_any_upstream_connection(self):
        g, port = self.make(["allowed.example"])
        cases = [
            (b"CONNECT blocked.example:443 HTTP/1.1\r\n\r\n", b"HTTP/1.1 403", "host_not_allowed"),
            (b"CONNECT allowed.example:8443 HTTP/1.1\r\n\r\n", b"HTTP/1.1 400", "port_not_443"),
            (b"CONNECT 127.0.0.1:443 HTTP/1.1\r\n\r\n", b"HTTP/1.1 400", "ip_literal"),
            (b"CONNECT 140.82.112.3:443 HTTP/1.1\r\n\r\n", b"HTTP/1.1 400", "ip_literal"),
            (b"CONNECT allowed.example.:443 HTTP/1.1\r\n\r\n", b"HTTP/1.1 400", "malformed_host"),
            (b"CONNECT user@allowed.example:443 HTTP/1.1\r\n\r\n", b"HTTP/1.1 400", "malformed_host"),
            (b"CONNECT allowed.example.evil.test:443 HTTP/1.1\r\n\r\n", b"HTTP/1.1 403", "host_not_allowed"),
            (b"CONNECT xallowed.example:443 HTTP/1.1\r\n\r\n", b"HTTP/1.1 403", "host_not_allowed"),
            (b"CONNECT sub.allowed.example:443 HTTP/1.1\r\n\r\n", b"HTTP/1.1 403", "host_not_allowed"),
            (b"CONNECT [::1]:443 HTTP/1.1\r\n\r\n", b"HTTP/1.1 400", "malformed_target"),
            (b"CONNECT allowed.example HTTP/1.1\r\n\r\n", b"HTTP/1.1 400", "malformed_target"),
            (b"CONNECT allowed.example:443 HTTP/2\r\n\r\n", b"HTTP/1.1 405", "not_a_connect_request"),
            (b"GET http://allowed.example/ HTTP/1.1\r\n\r\n", b"HTTP/1.1 405", "not_a_connect_request"),
            (b"\x16\x03\x01garbage\r\n\r\n", b"HTTP/1.1 405", "not_a_connect_request"),
            (b"CONNECT " + b"a" * 300 + b".example:443 HTTP/1.1\r\n\r\n", b"HTTP/1.1 400", "malformed_host"),
        ]
        for payload, status, reason in cases:
            s, resp = raw_request(port, payload)
            s.close()
            self.assertTrue(resp.startswith(status), (payload[:40], resp))
        self.assertEqual(self.up.accepted, 0, "no upstream connection was ever made, although a mapping to a reachable server existed")
        self.assertEqual(g.upstream_connections, 0)
        reasons = [e["reason"] for e in g.events if e["event"] == "refused"]
        for _, _, reason in cases:
            self.assertIn(reason, reasons)
        self.assertEqual(g.report()["contactedHosts"], [])
        self.assertGreaterEqual(len(g.report()["incidents"]), 5)

    def test_an_empty_allowlist_is_the_offline_mode_and_refuses_everything(self):
        g, port = self.make([])
        for host in ("allowed.example", "api.github.com", "github.com"):
            s, resp = raw_request(port, f"CONNECT {host}:443 HTTP/1.1\r\n\r\n".encode())
            s.close()
            self.assertTrue(resp.startswith(b"HTTP/1.1 403"), (host, resp))
        self.assertEqual((self.up.accepted, g.upstream_connections), (0, 0))

    def test_too_many_header_lines_are_refused(self):
        g, port = self.make(["allowed.example"])
        payload = b"CONNECT allowed.example:443 HTTP/1.1\r\n" + b"X: y\r\n" * 200 + b"\r\n"
        s, resp = raw_request(port, payload)
        s.close()
        self.assertTrue(resp.startswith(b"HTTP/1.1 431"), resp)
        self.assertEqual(self.up.accepted, 0)

    def test_parse_connect_target_is_strict(self):
        p = guard_mod.parse_connect_target
        self.assertEqual(p("CONNECT API.GitHub.com:443 HTTP/1.1"), ("api.github.com", 443))
        for bad in ["CONNECT a.b:80 HTTP/1.1", "CONNECT 1.2.3.4:443 HTTP/1.1", "CONNECT a..b:443 HTTP/1.1", "CONNECT -a.b:443 HTTP/1.1", "CONNECT a.b: HTTP/1.1",
                    "CONNECT localhost:443 HTTP/1.1", "POST a.b:443 HTTP/1.1", "CONNECT a.b:443", "CONNECT a.b:443 HTTP/1.1 extra"]:
            with self.assertRaises(ValueError, msg=bad):
                p(bad)


CHILD_TEMPLATE = r'''
import os, socket, sys
assert all(os.environ[v].startswith("http://127.0.0.1:") for v in ("HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy")), "proxy variables"
assert os.environ["NO_PROXY"] == "" and os.environ["no_proxy"] == "", "NO_PROXY must be emptied"
host, port = os.environ["HTTPS_PROXY"][len("http://"):].split(":")
mode = sys.argv[1]
if mode == "direct":
    s = socket.create_connection(("127.0.0.1", int(sys.argv[2])))
    s.sendall(b"hi"); s.recv(8); sys.exit(0)
s = socket.create_connection((host, int(port)))
s.sendall(("CONNECT %s:443 HTTP/1.1\r\n\r\n" % sys.argv[2]).encode())
resp = s.recv(256)
if mode == "try-and-ignore":
    sys.exit(0)  # the child swallows a refusal and "succeeds"
if mode == "use":
    assert resp.startswith(b"HTTP/1.1 200"), resp
    s.sendall(b"data"); assert s.recv(8) == b"data"
    sys.exit(int(sys.argv[3]))
'''


class RunModeTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.script = os.path.join(self.tmp, "child.py")
        with open(self.script, "w", encoding="utf-8") as fh:
            fh.write(CHILD_TEMPLATE)
        self.up = Upstream()
        self.addCleanup(self.up.close)

    def run_child(self, allow, *args, override=True):
        report = os.path.join(self.tmp, f"report{len(os.listdir(self.tmp))}.json")
        ov = {"allowed.example": ("127.0.0.1", self.up.port)} if override else None
        status, rep = guard_mod.run(allow, [sys.executable, "-I", self.script, *args], report, connect_override=ov)
        with open(report, encoding="utf-8") as fh:
            self.assertEqual(json.load(fh), rep)
        return status, rep

    def test_a_refused_attempt_makes_the_run_fail_even_when_the_child_ignores_it_and_exits_zero(self):
        status, rep = self.run_child(["allowed.example"], "try-and-ignore", "blocked.example")
        self.assertEqual(rep["childExit"], 0)
        self.assertEqual(status, guard_mod.INCIDENT_EXIT)
        self.assertEqual([i["host"] for i in rep["incidents"]], ["blocked.example"])
        self.assertEqual(rep["contactedHosts"], [])
        self.assertEqual(self.up.accepted, 0)

    def test_offline_mode_turns_any_attempt_into_an_incident(self):
        status, rep = self.run_child([], "try-and-ignore", "allowed.example")
        self.assertEqual(status, guard_mod.INCIDENT_EXIT)
        self.assertEqual(self.up.accepted, 0)

    def test_an_allowed_run_passes_the_childs_exit_status_through_and_records_the_host(self):
        status, rep = self.run_child(["allowed.example"], "use", "allowed.example", "0")
        self.assertEqual((status, rep["incidents"]), (0, []))
        self.assertEqual([h["host"] for h in rep["contactedHosts"]], ["allowed.example"])
        self.assertTrue(rep["guard"]["used"])
        status, rep = self.run_child(["allowed.example"], "use", "allowed.example", "7")
        self.assertEqual(status, 7)

    def test_child_output_can_be_captured_into_new_files_only(self):
        out, err = os.path.join(self.tmp, "child.out"), os.path.join(self.tmp, "child.err")
        status, _ = guard_mod.run([], [sys.executable, "-I", "-c", "import sys; print('out'); print('err', file=sys.stderr)"], stdout_path=out, stderr_path=err)
        self.assertEqual(status, 0)
        with open(out) as fh1, open(err) as fh2:
            self.assertEqual((fh1.read().strip(), fh2.read().strip()), ("out", "err"))
        with self.assertRaises(FileExistsError):
            guard_mod.run([], [sys.executable, "-I", "-c", "pass"], stdout_path=out)

    def test_the_report_file_is_created_exclusively(self):
        path = os.path.join(self.tmp, "exists.json")
        open(path, "w").close()
        with self.assertRaises(FileExistsError):
            guard_mod.run([], [sys.executable, "-I", "-c", "pass"], path)

    def test_documented_limit_a_child_that_ignores_the_proxy_is_neither_stopped_nor_seen(self):
        status, rep = self.run_child([], "direct", str(self.up.port))
        self.assertEqual(self.up.accepted, 1, "the direct connection succeeded")
        self.assertEqual((status, rep["incidents"], rep["contactedHosts"]), (0, [], []), "and the guard saw nothing")
        self.assertIn("not an operating-system firewall", rep["guard"]["residualLimits"])
        text = (ROOT / "tools" / "egress_guard.py").read_text(encoding="utf-8")
        self.assertIn("LIMITS (stated, tested, not hidden)", text)

    def test_the_guard_source_has_no_default_host_list_and_no_http_client(self):
        text = (ROOT / "tools" / "egress_guard.py").read_text(encoding="utf-8")
        for needle in [BLOB_HOST, "github.com", "urllib", "http.client", "requests"]:
            self.assertNotIn(needle, text, needle)


class ReportingModel(unittest.TestCase):
    CLEAN = {"schema": 1, "guard": {"used": True}, "contactedHosts": [{"host": "api.github.com", "how": "observed"}, {"host": "tuf-repo.github.com", "how": "observed"}], "incidents": []}

    def summarize(self, report, mutate=None):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            pins = synthetic_session(root, mutate)
            r = review_mod.review(str(root), pins)
            return r, review_mod.summarize(r, report, str(root), pins)

    def test_crypto_pass_without_a_network_report_is_not_compliant(self):
        r, s = self.summarize(None)
        self.assertTrue(r.passed)
        self.assertEqual([s[k] for k in ("artifact_crypto_verified", "exact_bytes_verified", "official_verifier_passed", "network_policy_compliant", "installation_authorized", "execution_authorized")],
                         [True, True, True, False, False, False])
        self.assertEqual(s["overall"], "CRYPTO_CONSISTENT_BUT_NETWORK_POLICY_NOT_COMPLIANT")
        self.assertTrue(any("missing" in f for f in s["network_policy_findings"]))

    def test_a_clean_guard_report_makes_the_session_compliant_but_never_authorizes_anything(self):
        r, s = self.summarize(self.CLEAN)
        self.assertTrue(s["network_policy_compliant"])
        self.assertEqual(s["overall"], "VERIFIED_AND_COMPLIANT_NOT_AUTHORIZED_FOR_INSTALL_OR_EXECUTION")
        self.assertIs(s["installation_authorized"], False)
        self.assertIs(s["execution_authorized"], False)

    def test_the_recorded_azure_incident_is_never_compliant_however_good_the_crypto_is(self):
        incident = {"schema": 1, "guard": {"used": False}, "contactedHosts": [{"host": BLOB_HOST, "how": "observed"}],
                    "incidents": [{"type": "unlisted_host_contacted", "host": BLOB_HOST}]}
        r, s = self.summarize(incident)
        self.assertTrue(r.passed and s["artifact_crypto_verified"] and s["exact_bytes_verified"] and s["official_verifier_passed"])
        self.assertFalse(s["network_policy_compliant"])
        self.assertTrue(any(BLOB_HOST in f for f in s["network_policy_findings"]))
        self.assertEqual(s["overall"], "CRYPTO_CONSISTENT_BUT_NETWORK_POLICY_NOT_COMPLIANT")

    def test_every_other_defect_in_a_network_report_is_non_compliant(self):
        bad = [
            "not a dict", [], {}, {"schema": 2}, {"schema": 1, "contactedHosts": [], "guard": {"used": True}},
            {"schema": 1, "incidents": [], "guard": {"used": True}},
            {**self.CLEAN, "guard": {"used": False}},
            {**self.CLEAN, "incidents": [{"x": 1}]},
            {**self.CLEAN, "contactedHosts": [{"host": "evil.example"}]},
            {**self.CLEAN, "contactedHosts": ["api.github.com"]},
        ]
        for report in bad:
            ok, reasons = review_mod.evaluate_network_report(report)
            self.assertFalse(ok, report)
            self.assertTrue(reasons)

    def test_a_cryptographic_failure_is_a_failure_whatever_the_network_report_says(self):
        W = "jq-windows-amd64.exe"
        r, s = self.summarize(self.CLEAN, lambda root, p: (root / "artifacts" / W / W).write_bytes(b"tampered"))
        self.assertFalse(r.passed)
        self.assertFalse(s["exact_bytes_verified"])
        self.assertEqual(s["overall"], "FAIL_CHECKS")
        r, s = self.summarize(self.CLEAN, lambda root, p: (root / f"{W}.gh.exit").write_text("1"))
        self.assertFalse(s["official_verifier_passed"])
        self.assertFalse(s["artifact_crypto_verified"])
        self.assertEqual(s["overall"], "FAIL_CHECKS")

    def test_a_non_https_or_unlisted_download_hop_is_a_network_finding(self):
        def bad_hop(root, p):
            (root / "fetch.log").write_text(json.dumps({"hops": [{"scheme": "https", "host": BLOB_HOST, "path": "/x", "status": 200}]}) + "\n")

        r, s = self.summarize(self.CLEAN, bad_hop)
        self.assertFalse(s["network_policy_compliant"])

    def test_kept_failed_attempts_are_listed_as_tool_invocation_errors_and_change_no_verdict(self):
        def add(root, p):
            (root / "jq-macos-arm64.gh-offline.attempt1-wrapper-format.err").write_text("Error: unknown field attestations")
            (root / "attempt1-winpath-keyring.colons").write_text("")

        r, s = self.summarize(self.CLEAN, add)
        self.assertTrue(r.passed and s["network_policy_compliant"])
        self.assertEqual(sorted(e["file"] for e in s["toolInvocationErrors"]), ["attempt1-winpath-keyring.colons", "jq-macos-arm64.gh-offline.attempt1-wrapper-format.err"])

    def test_an_offline_only_session_passes_but_a_missing_offline_or_half_an_online_record_fails(self):
        W = "jq-windows-amd64.exe"

        def drop_online(root, p):
            for n in (W, "jq-macos-arm64"):
                (root / f"{n}.gh.json").unlink()
                (root / f"{n}.gh.exit").unlink()

        r, s = self.summarize(self.CLEAN, drop_online)
        self.assertTrue(r.passed and s["official_verifier_passed"] and s["network_policy_compliant"])
        self.assertIs(s["online_check_performed"], False)
        r, s = self.summarize(self.CLEAN)
        self.assertIs(s["online_check_performed"], True)
        r, s = self.summarize(self.CLEAN, lambda root, p: (drop_online(root, p), (root / f"{W}.gh-offline.exit").unlink()))
        self.assertFalse(r.passed, "the offline replay is mandatory")
        self.assertFalse(s["official_verifier_passed"])
        r, s = self.summarize(self.CLEAN, lambda root, p: (drop_online(root, p), (root / f"{W}.gh.json").write_text("[]")))
        self.assertFalse(r.passed, "an online record that is present must be complete and pass")

    def test_exit_status_separates_failure_from_non_compliance(self):
        self.assertEqual(review_mod.exit_status(True, {"network_policy_compliant": True}), 0)
        self.assertEqual(review_mod.exit_status(True, {"network_policy_compliant": False}), 3)
        self.assertEqual(review_mod.exit_status(False, {"network_policy_compliant": True}), 1)
        self.assertEqual(review_mod.exit_status(False, {"network_policy_compliant": False}), 1)

    def test_the_azure_host_is_in_no_approved_list_and_no_default(self):
        self.assertNotIn(BLOB_HOST, review_mod.PINS["approvedNetworkHosts"])
        self.assertNotIn(BLOB_HOST, review_mod.PINS["allowedDownloadHosts"])
        self.assertFalse(any("blob.core" in h for h in review_mod.PINS["approvedNetworkHosts"]))
        self.assertNotIn(BLOB_HOST, (ROOT / "tools" / "fetch_pinned.py").read_text(encoding="utf-8"))


class GpgClassification(unittest.TestCase):
    OK = "[GNUPG:] NEWSIG\n[GNUPG:] GOODSIG B0DA60FB454BAF18 jq\n[GNUPG:] VALIDSIG 93079A9511EFC0B7D6CDBFB5B0DA60FB454BAF18 2026-06-20 1 0 4 0 1 10 00 93079A9511EFC0B7D6CDBFB5B0DA60FB454BAF18\n[GNUPG:] TRUST_UNDEFINED 0 pgp\n"
    MISSING_HOME = "gpg: keyblock resource '/x/C:\\\\y/pubring.kbx': No such file or directory\ngpg: Fatal: /x/C:\\\\y: directory does not exist!\n"

    def test_a_clean_signature_is_ok(self):
        self.assertEqual(review_mod.classify_gpg_status(self.OK), "ok")

    def test_the_observed_windows_path_failure_is_an_invocation_error_not_a_crypto_failure(self):
        status = "[GNUPG:] ERRSIG B0DA60FB454BAF18 1 10 00 1781964636 9 93079A9511EFC0B7D6CDBFB5B0DA60FB454BAF18\n[GNUPG:] NO_PUBKEY B0DA60FB454BAF18\n"
        self.assertEqual(review_mod.classify_gpg_status(status, self.MISSING_HOME), "invocation_error")

    def test_everything_else_stays_fail_closed(self):
        err_status = "[GNUPG:] ERRSIG B0DA60FB454BAF18 1 10 00 1 9 93079A9511EFC0B7D6CDBFB5B0DA60FB454BAF18\n[GNUPG:] NO_PUBKEY B0DA60FB454BAF18\n"
        cases = [
            ("[GNUPG:] BADSIG B0DA60FB454BAF18 jq\n", ""),
            ("[GNUPG:] BADSIG B0DA60FB454BAF18 jq\n", self.MISSING_HOME),
            ("[GNUPG:] EXPKEYSIG B0DA60FB454BAF18 jq\n", ""),
            ("[GNUPG:] REVKEYSIG B0DA60FB454BAF18 jq\n", ""),
            (err_status, ""),  # a missing key with NO environmental cause is unexpected
            (err_status + "[GNUPG:] BADSIG B0DA60FB454BAF18 jq\n", self.MISSING_HOME),
            (self.OK + "[GNUPG:] BADSIG B0DA60FB454BAF18 jq\n", ""),
            ("", ""),
            ("[GNUPG:] GOODSIG B0DA60FB454BAF18 jq\n", ""),
        ]
        for status, err in cases:
            self.assertEqual(review_mod.classify_gpg_status(status, err), "cryptographic_failure", (status, err))


class BundleExtraction(unittest.TestCase):
    WRAPPER = ('{ "attestations" : [ {"repository_id": 1, "bundle_url": "https://x.invalid/b?sig=1", "bundle": {"a":"\\u0026 \\/ \u00e9","b":[1, 2],"c":{ "d" : null }}, "initiator": "github"},'
               ' {"initiator":"x","bundle" : {"k":"v\\n"}} ] }')

    def test_each_bundle_is_copied_byte_for_byte_not_reserialised(self):
        out = extract_mod.extract(self.WRAPPER.encode("utf-8"))
        self.assertEqual(out, ['{"a":"\\u0026 \\/ \u00e9","b":[1, 2],"c":{ "d" : null }}', '{"k":"v\\n"}'])
        for text in out:
            self.assertIn(text, self.WRAPPER)
        self.assertNotEqual(out[0], json.dumps(json.loads(out[0]), separators=(",", ":")), "a re-serialisation would have changed the bytes")

    def test_refusals(self):
        bad = {
            "not json": b"nope", "no array": b'{"x":1}', "not utf8": b'{"attestations":[]}\xff',
            "empty": b'{"attestations":[]}', "no bundle": b'{"attestations":[{"a":1}]}', "null bundle": b'{"attestations":[{"bundle":null}]}',
            "line break": b'{"attestations":[{"bundle":{"a":\n1}}]}', "trailing": b'{"attestations":[{"bundle":{}}]} x',
        }
        for label, raw in bad.items():
            with self.assertRaises((extract_mod.Refused, ValueError), msg=label):
                extract_mod.extract(raw)

    def test_the_stored_repository_evidence_extracts_byte_exactly(self):
        ev = ROOT / "docs" / "phase-3d0" / "evidence"
        for name, count in [("jq-1.8.2-windows-amd64.attestations.json", 2), ("jq-1.8.2-macos-arm64.attestations.json", 1)]:
            raw = (ev / name).read_bytes()
            lines = extract_mod.extract(raw)
            self.assertEqual(len(lines), count)
            parsed = json.loads(raw)
            for line, att in zip(lines, parsed["attestations"]):
                self.assertIn(line.encode("utf-8"), raw)
                self.assertEqual(json.loads(line), att["bundle"])
                self.assertNotIn("attestations", json.loads(line))

    def test_the_cli_writes_a_new_private_file_and_refuses_to_overwrite(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = os.path.join(tmp, "in.json")
            with open(src, "wb") as fh:
                fh.write(self.WRAPPER.encode("utf-8"))
            dst = os.path.join(tmp, "out.jsonl")
            argv = sys.argv
            try:
                sys.argv = ["x", "--in", src, "--out", dst]
                buf = io.StringIO()
                with contextlib.redirect_stdout(buf):
                    self.assertEqual(extract_mod.main(), 0)
                self.assertTrue(json.loads(buf.getvalue())["ok"])
                with open(dst, "rb") as fh:
                    data = fh.read()
                self.assertEqual(data, b'{"a":"\\u0026 \\/ \xc3\xa9","b":[1, 2],"c":{ "d" : null }}\n{"k":"v\\n"}\n')
                with contextlib.redirect_stdout(io.StringIO()) as buf2:
                    self.assertEqual(extract_mod.main(), 1)
                self.assertIn("output_exists", buf2.getvalue())
                with open(dst, "rb") as fh:
                    self.assertEqual(fh.read(), data)
            finally:
                sys.argv = argv

    def test_the_tool_has_no_network_or_process_capability(self):
        text = (ROOT / "tools" / "extract_sigstore_bundles.py").read_text(encoding="utf-8")
        for needle in ["socket", "urllib", "http.client", "subprocess", "os.system", "requests"]:
            self.assertNotIn(needle, text, needle)


class Documents(unittest.TestCase):
    def text(self, *parts):
        return (ROOT.joinpath(*parts)).read_text(encoding="utf-8")

    def test_the_incident_is_documented_permanently_and_the_host_is_not_authorised(self):
        inc = self.text("docs", "phase-3d0", "REQUEST-A-NETWORK-INCIDENT.md")
        for needle in [BLOB_HOST, "NOT authorised", "policy violation", "observed", "inferred", "not independently proven", "bundle_url", "egress_guard"]:
            self.assertIn(needle, inc, needle)
        ev = self.text("docs", "phase-3d0", "evidence", "request-a-session-20261018", "network-incident.json")
        data = json.loads(ev)
        self.assertEqual(data["schema"], 1)
        self.assertTrue(data["incidents"])
        self.assertIn(BLOB_HOST, [h["host"] for h in data["contactedHosts"]])

    def test_the_request_documents_use_offline_replay_by_default_and_the_guard(self):
        en = self.text("docs", "phase-3d0", "REQUEST-A.md")
        tr = self.text("docs", "phase-3d0", "REQUEST-A.tr.md")
        for needle in ["extract_sigstore_bundles.py", "egress_guard.py", "separate explicit authorisation", "GNUPGHOME", "IMPORT_OK", BLOB_HOST]:
            self.assertIn(needle, en, needle)
        for needle in ["extract_sigstore_bundles.py", "egress_guard.py", BLOB_HOST]:
            self.assertIn(needle, tr, needle)

    def test_the_sanitized_review_report_keeps_the_original_pass_and_adds_the_six_facts(self):
        rep = json.loads(self.text("docs", "phase-3d0", "evidence", "request-a-session-20261018", "review-v2.json"))
        self.assertTrue(rep["ok"])
        s = rep["summary"]
        self.assertEqual(len(rep["checks"]), 151)
        self.assertIs(s["artifact_crypto_verified"], True)
        self.assertIs(s["exact_bytes_verified"], True)
        self.assertIs(s["official_verifier_passed"], True)
        self.assertIs(s["network_policy_compliant"], False)
        self.assertIs(s["installation_authorized"], False)
        self.assertIs(s["execution_authorized"], False)
        self.assertEqual(s["overall"], "CRYPTO_CONSISTENT_BUT_NETWORK_POLICY_NOT_COMPLIANT")
        self.assertTrue(self.text("docs", "phase-3d0", "evidence", "request-a-session-20261018", "review.json"), "the original report is preserved")


if __name__ == "__main__":
    unittest.main()
