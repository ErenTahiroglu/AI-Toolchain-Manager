#!/usr/bin/env python3
"""Fail-closed egress guard for network-capable verification steps (Request A closure, Issue #19).

WHY: `tools/fetch_pinned.py` enforces its host allowlist itself, but the GitHub CLI (`gh`) is a separate program: in Request A it
followed an API-supplied signed URL to an Azure blob-storage host that was not on the approved list (see
docs/phase-3d0/REQUEST-A-NETWORK-INCIDENT.md) and nothing could stop or even see it. An allowlist inside OUR code cannot monitor another program's traffic.

WHAT THIS DOES: a deny-by-default HTTPS CONNECT proxy on 127.0.0.1 plus a `run` mode that starts it, runs ONE command with every
proxy variable (HTTPS_PROXY/HTTP_PROXY/ALL_PROXY, upper and lower case) pointed at it and NO_PROXY emptied, then writes a report.
  * the destination is decided from the CONNECT line BEFORE any upstream socket is opened: an unlisted, malformed, IP-literal,
    non-443 or non-CONNECT request gets 4xx and no connection to anything is ever made;
  * with an EMPTY allowlist (`--allow-host` absent) every attempt is refused: that is the mode for offline verification;
  * any refused attempt is an INCIDENT: the exit status is 97 whatever the child returned, and the report says so. The child cannot
    quietly turn a policy violation into a pass, and a host that is not listed is never added automatically: the user must
    authorise it in a separate request.

LIMITS (stated, tested, not hidden): this is NOT an operating-system firewall. A program that ignores proxy variables and connects
directly is neither stopped nor seen. Real enforcement of that needs a per-program firewall rule (administrator rights, which this
project forbids) or a network-less sandbox. So: (1) prefer OFFLINE verification, which needs no host at all; (2) use `run` only for
programs known to honour proxy variables (the GitHub CLI does: observed in Request A); (3) a report that cannot show the guard was
used is treated as NOT compliant by tools/review_jq_verification.py.
"""

import argparse
import json
import os
import re
import select
import socket
import socketserver
import subprocess
import sys
import threading
import time

INCIDENT_EXIT = 97
MAX_HEADER_LINES = 64
HOST_RE = re.compile(r"^(?=.{1,253}$)([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)+$")
PROXY_VARS = ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"]
RESIDUAL_LIMITS = ("not an operating-system firewall: a program that ignores proxy variables and connects directly is neither stopped nor seen; "
                   "prefer offline verification")


def parse_connect_target(line):
    """'CONNECT host:443 HTTP/1.1' -> (host, port) or raises ValueError(reason). Strict: no IP literals, no userinfo, no trailing dot."""
    parts = line.split(" ")
    if len(parts) != 3 or parts[0] != "CONNECT" or not parts[2].startswith("HTTP/1."):
        raise ValueError("not_a_connect_request")
    target = parts[1]
    if target.count(":") != 1:
        raise ValueError("malformed_target")
    host, port = target.split(":")
    host = host.lower()
    if port != "443":
        raise ValueError("port_not_443")
    if not HOST_RE.match(host):
        raise ValueError("malformed_host")
    if all(label.isdigit() for label in host.split(".")):
        raise ValueError("ip_literal")
    return host, 443


class Guard:
    def __init__(self, allow_hosts=(), connect_override=None, log_path=None):
        self.allow = {h.lower() for h in allow_hosts}
        self.override = dict(connect_override or {})  # TESTS ONLY: host -> (address, port) so no real host is ever contacted
        self.log_path = log_path
        self.events = []
        self.upstream_connections = 0
        self._lock = threading.Lock()
        guard = self

        class Handler(socketserver.StreamRequestHandler):
            timeout = 30

            def handle(self):
                guard._handle(self)

        class Server(socketserver.ThreadingTCPServer):
            daemon_threads = True
            allow_reuse_address = True

        self.server = Server(("127.0.0.1", 0), Handler)
        self.port = self.server.server_address[1]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    def start(self):
        self.thread.start()
        self._event({"event": "guard-start", "allow": sorted(self.allow)})
        return self.port

    def stop(self):
        self.server.shutdown()
        self.server.server_close()

    def _event(self, rec):
        rec["t"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        with self._lock:
            self.events.append(rec)
            if self.log_path:
                with open(self.log_path, "a", encoding="utf-8") as f:
                    f.write(json.dumps(rec) + "\n")

    def _handle(self, h):
        try:
            line = h.rfile.readline(8192).decode("latin-1").rstrip("\r\n")
            for _ in range(MAX_HEADER_LINES):
                header = h.rfile.readline(8192)
                if header in (b"\r\n", b"\n", b""):
                    break
            else:
                self._event({"event": "refused", "reason": "too_many_header_lines"})
                h.wfile.write(b"HTTP/1.1 431 Request Header Fields Too Large\r\n\r\n")
                return
        except (OSError, ValueError):
            return
        try:
            host, port = parse_connect_target(line)
        except ValueError as e:
            self._event({"event": "refused", "reason": str(e), "line": line[:80]})
            h.wfile.write(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n" if str(e) == "not_a_connect_request" else b"HTTP/1.1 400 Bad Request\r\n\r\n")
            return
        if host not in self.allow:
            # decided BEFORE any upstream socket exists
            self._event({"event": "refused", "reason": "host_not_allowed", "host": host, "port": port})
            h.wfile.write(b"HTTP/1.1 403 Forbidden\r\n\r\n")
            return
        address = self.override.get(host, (host, port))
        try:
            with self._lock:
                self.upstream_connections += 1
            up = socket.create_connection(address, timeout=30)
        except OSError as e:
            self._event({"event": "upstream-failed", "host": host, "err": type(e).__name__})
            h.wfile.write(b"HTTP/1.1 502 Bad Gateway\r\n\r\n")
            return
        sent = received = 0
        try:
            h.wfile.write(b"HTTP/1.1 200 Connection established\r\n\r\n")
            h.wfile.flush()
            socks = [h.connection, up]
            done = False
            while not done:
                ready, _, _ = select.select(socks, [], [], 60)
                if not ready:
                    break
                for s in ready:
                    data = s.recv(65536)
                    if not data:
                        done = True
                        break
                    if s is h.connection:
                        up.sendall(data)
                        sent += len(data)
                    else:
                        h.connection.sendall(data)
                        received += len(data)
        except OSError:
            pass
        finally:
            up.close()
            self._event({"event": "tunnel", "host": host, "port": port, "bytesUp": sent, "bytesDown": received})

    def report(self, child_exit=None, command_name=None):
        tunnels, refused = {}, {}
        for e in self.events:
            if e["event"] == "tunnel":
                t = tunnels.setdefault(e["host"], {"host": e["host"], "how": "observed", "by": "egress_guard", "tunnels": 0, "bytesUp": 0, "bytesDown": 0})
                t["tunnels"] += 1
                t["bytesUp"] += e["bytesUp"]
                t["bytesDown"] += e["bytesDown"]
            elif e["event"] == "refused":
                key = e.get("host") or ("<" + e["reason"] + ">")
                r = refused.setdefault(key, {"type": "refused_attempt", "host": e.get("host"), "reason": e["reason"], "count": 0})
                r["count"] += 1
        return {
            "schema": 1,
            "guard": {"used": True, "allowedHosts": sorted(self.allow), "residualLimits": RESIDUAL_LIMITS},
            "command": command_name,
            "childExit": child_exit,
            "contactedHosts": sorted(tunnels.values(), key=lambda x: x["host"]),
            "incidents": sorted(refused.values(), key=lambda x: str(x["host"])),
            "exitStatusRule": f"{INCIDENT_EXIT} if any incident, otherwise the child's exit status",
        }


def _new_file(path):
    return os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_BINARY", 0), 0o600), "wb")


def run(allow_hosts, command, report_path=None, log_path=None, extra_env=None, connect_override=None, stdout_path=None, stderr_path=None):
    """Runs `command` (a list) behind the guard. Returns (exit_status, report). Optional new files receive the child's stdout/stderr."""
    guard = Guard(allow_hosts, connect_override, log_path)
    port = guard.start()
    env = dict(os.environ)
    env.update(extra_env or {})
    for v in PROXY_VARS:
        env[v] = f"http://127.0.0.1:{port}"
    env["NO_PROXY"] = env["no_proxy"] = ""
    out = _new_file(stdout_path) if stdout_path else None
    err = _new_file(stderr_path) if stderr_path else None
    try:
        child = subprocess.run(command, env=env, check=False, stdout=out, stderr=err)
        code = child.returncode
    finally:
        guard.stop()
        for f in (out, err):
            if f:
                f.close()
    rep = guard.report(code, os.path.basename(command[0]))
    if report_path:
        fd = os.open(report_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as f:
            json.dump(rep, f, indent=2)
            f.write("\n")
    return (INCIDENT_EXIT if rep["incidents"] else code), rep


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--allow-host", action="append", default=[], help="repeat; omit entirely for OFFLINE (every attempt is an incident)")
    ap.add_argument("--report", help="new file for the JSON report (must not exist)")
    ap.add_argument("--log", help="append one JSON line per guard event")
    ap.add_argument("--child-stdout", help="new file for the command's stdout (otherwise it shares ours)")
    ap.add_argument("--child-stderr", help="new file for the command's stderr")
    ap.add_argument("command", nargs=argparse.REMAINDER, help="after `--`: the one command to run")
    a = ap.parse_args(argv)
    cmd = a.command[1:] if a.command[:1] == ["--"] else a.command
    if not cmd:
        ap.error("a command is required after --")
    status, rep = run(a.allow_host, cmd, a.report, a.log, stdout_path=a.child_stdout, stderr_path=a.child_stderr)
    print(json.dumps({"exit": status, "incidents": rep["incidents"], "contactedHosts": [h["host"] for h in rep["contactedHosts"]]}))
    return status


if __name__ == "__main__":
    sys.exit(main())
