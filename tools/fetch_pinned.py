#!/usr/bin/env python3
"""Fetch ONE pinned file over HTTPS with a strict host allowlist, explicit redirect control and no credentials.

Reviewer tooling for a FUTURE, separately authorised download session (docs/phase-3d0/REQUEST-A.md). It was written and tested in
Phase 3D-0 closure but has NEVER been run against a real remote host: the tests use a loopback server only.

What it guarantees:
  * every hop (the first URL and every redirect target) must be `https` and its host must be on the allowlist given on the command
    line; any other host, a scheme change, or more than 3 redirects aborts the download and nothing is kept;
  * NO credential of any kind is ever sent: no Authorization header, no cookie, no token from the environment or from the GitHub CLI;
  * the expected size and SHA-256 are pinned: a longer body is cut off and rejected, a shorter or different one is rejected; the file
    appears under its final name only after both match, with owner-only permissions and no execute bit;
  * the hops (scheme, host, path WITHOUT the query string, status) are logged, so the hosts actually contacted can be reported.
It never executes, opens or parses what it downloads.
"""

import argparse
import hashlib
import http.client
import json
import os
import stat
import sys
import urllib.parse

MAX_REDIRECTS = 3
TIMEOUT = 30


class Refused(Exception):
    pass


def _connection(url, loopback_http):
    parts = urllib.parse.urlsplit(url)
    if parts.scheme == "https":
        return http.client.HTTPSConnection(parts.hostname, parts.port or 443, timeout=TIMEOUT), parts
    if parts.scheme == "http" and loopback_http and parts.hostname in ("127.0.0.1", "localhost"):
        return http.client.HTTPConnection(parts.hostname, parts.port or 80, timeout=TIMEOUT), parts
    raise Refused(f"scheme_not_allowed:{parts.scheme}")


def fetch(url, dest, expect_size, expect_sha256, allow_hosts, loopback_http=False):
    """Returns a record; raises Refused on ANY deviation. `dest` is the final path (its folder must exist)."""
    allow = {h.lower() for h in allow_hosts}
    hops = []
    current = url
    part = dest + ".part"
    try:
        for hop in range(MAX_REDIRECTS + 1):
            conn, parts = _connection(current, loopback_http)
            host = (parts.hostname or "").lower()
            if host not in allow:
                raise Refused(f"host_not_allowed:{host}")
            target = parts.path + (("?" + parts.query) if parts.query else "")
            # a fixed minimal request: no cookies, no Authorization, no tokens
            conn.request("GET", target, headers={"User-Agent": "atm-pinned-fetch/1", "Accept": "application/octet-stream", "Accept-Encoding": "identity"})
            resp = conn.getresponse()
            hops.append({"scheme": parts.scheme, "host": host, "path": parts.path, "status": resp.status})
            if resp.status in (301, 302, 303, 307, 308):
                loc = resp.getheader("Location")
                resp.read(0)
                conn.close()
                if not loc:
                    raise Refused("redirect_without_location")
                if hop == MAX_REDIRECTS:
                    raise Refused("too_many_redirects")
                current = urllib.parse.urljoin(current, loc)
                continue
            if resp.status != 200:
                raise Refused(f"unexpected_status:{resp.status}")
            declared = resp.getheader("Content-Length")
            if declared is not None and int(declared) != expect_size:
                raise Refused(f"content_length_differs:{declared}")
            digest = hashlib.sha256()
            total = 0
            fd = os.open(part, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(fd, "wb") as out:
                while True:
                    chunk = resp.read(min(65536, expect_size + 1 - total))
                    if not chunk:
                        break
                    total += len(chunk)
                    if total > expect_size:
                        raise Refused("body_longer_than_pinned_size")
                    digest.update(chunk)
                    out.write(chunk)
            conn.close()
            if total != expect_size:
                raise Refused(f"size_differs:{total}")
            if digest.hexdigest() != expect_sha256:
                raise Refused("sha256_differs")
            if os.path.exists(dest):
                raise Refused("destination_exists")
            os.replace(part, dest)
            os.chmod(dest, stat.S_IRUSR | stat.S_IWUSR)
            return {"file": os.path.basename(dest), "size": total, "sha256": digest.hexdigest(), "hops": hops, "finalHost": hops[-1]["host"]}
        raise Refused("too_many_redirects")
    except (OSError, http.client.HTTPException, ValueError) as e:
        raise Refused(f"transport:{type(e).__name__}") from None
    finally:
        if os.path.exists(part):
            os.remove(part)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--url", required=True)
    ap.add_argument("--dest", required=True, help="final file path; its folder must already exist and the file must not")
    ap.add_argument("--size", required=True, type=int)
    ap.add_argument("--sha256", required=True)
    ap.add_argument("--allow-host", action="append", required=True)
    ap.add_argument("--log", help="append the JSON record of this download to this file")
    a = ap.parse_args()
    try:
        rec = fetch(a.url, a.dest, a.size, a.sha256, a.allow_host)
    except Refused as e:
        print(json.dumps({"ok": False, "refused": str(e)}))
        return 1
    rec["ok"] = True
    print(json.dumps(rec))
    if a.log:
        with open(a.log, "a", encoding="utf-8") as f:
            f.write(json.dumps(rec) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
