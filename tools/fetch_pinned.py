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
  * publication is ATOMIC AND NO-CLOBBER: the verified bytes sit in a uniquely named, exclusively created staging file next to the
    destination and are published with a hard link (`link(2)` / `CreateHardLink`), which fails if ANY entry (file, directory, symlink,
    dangling symlink) already carries the final name, even one created a moment ago by someone else. There is no check-then-replace, no
    rename-over and no copy anywhere. A folder whose filesystem cannot do this (probed with throw-away files BEFORE any network use) is
    refused. Only files this process created (verified by device+inode) are ever removed; anything else is preserved untouched;
  * the hops (scheme, host, path WITHOUT the query string, status) are logged, so the hosts actually contacted can be reported.
It never executes, opens or parses what it downloads.
"""

import argparse
import hashlib
import http.client
import json
import os
import secrets
import stat
import sys
import urllib.parse

MAX_REDIRECTS = 3
TIMEOUT = 30


class Refused(Exception):
    pass


# Test seam only: called with (staging_path, destination_path) immediately before the publication step. Never set in production.
_before_publish = None

_CREATE_FLAGS = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_CLOEXEC", 0)


def _identity(st):
    return (st.st_dev, st.st_ino)


def _create_private(path):
    """Exclusively creates `path` (never follows or reuses an existing entry); returns (fd, identity)."""
    fd = os.open(path, _CREATE_FLAGS, 0o600)
    st = os.fstat(fd)
    if st.st_ino == 0:
        os.close(fd)
        _remove_if_ours(path, None, created_here=True)
        raise Refused("file_identity_unavailable")
    return fd, _identity(st)


def _remove_if_ours(path, ident, created_here=False):
    """Removes `path` only if it is still the regular file this process created. Anything else is left alone. Returns True if gone."""
    try:
        st = os.lstat(path)
    except FileNotFoundError:
        return True
    except OSError:
        return False
    if not created_here and (not stat.S_ISREG(st.st_mode) or _identity(st) != ident):
        return False
    try:
        os.unlink(path)
        return True
    except OSError:
        return False


def _link_no_clobber(src, dst):
    """The one publication primitive: a hard link, which the operating system refuses (FileExistsError) if `dst` exists in any form."""
    if os.link in os.supports_follow_symlinks:
        os.link(src, dst, follow_symlinks=False)
    else:
        os.link(src, dst)


def _probe_no_clobber(folder):
    """Proves, with throw-away files in the destination folder, that exclusive hard-link publication works here. Refuses otherwise."""
    tag = secrets.token_hex(8)
    a, b = os.path.join(folder, f".atm-probe-{tag}-a"), os.path.join(folder, f".atm-probe-{tag}-b")
    made = []
    try:
        fd, ident = _create_private(a)
        made.append((a, ident))
        os.close(fd)
        try:
            _link_no_clobber(a, b)
        except OSError as e:
            raise Refused(f"no_clobber_publication_unsupported:link_failed:{e.errno}") from None
        made.append((b, ident))
        try:
            _link_no_clobber(a, b)
        except FileExistsError:
            pass
        except OSError as e:
            raise Refused(f"no_clobber_publication_unsupported:existing_entry_not_reported:{e.errno}") from None
        else:
            raise Refused("no_clobber_publication_unsupported:existing_entry_overwritten_or_ignored")
        if os.lstat(b).st_nlink < 2:
            raise Refused("no_clobber_publication_unsupported:link_count")
    except OSError as e:
        raise Refused(f"no_clobber_publication_unsupported:probe_failed:{e.errno}") from None
    finally:
        for path, ident in made:
            _remove_if_ours(path, ident)


def _publish_no_clobber(part, dest, ident, size):
    """Publishes the verified staging file under its final name without ever replacing anything. Returns a list of leftovers."""
    if _before_publish is not None:
        _before_publish(part, dest)
    try:
        st = os.lstat(part)
    except OSError:
        raise Refused("staged_file_missing") from None
    if not stat.S_ISREG(st.st_mode) or _identity(st) != ident:
        raise Refused("staged_file_replaced")
    try:
        _link_no_clobber(part, dest)
    except OSError as e:
        if os.path.lexists(dest):
            raise Refused("destination_exists") from None
        raise Refused(f"no_clobber_publication_unsupported:{e.errno}") from None
    try:
        final = os.lstat(dest)
    except OSError:
        raise Refused("published_entry_mismatch") from None
    if not stat.S_ISREG(final.st_mode) or _identity(final) != ident or final.st_size != size:
        raise Refused("published_entry_mismatch")
    if os.name != "nt" and final.st_mode & 0o177:
        raise Refused("permissions_unexpected")
    return [] if _remove_if_ours(part, ident) else [os.path.basename(part)]


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
    folder = os.path.dirname(os.path.abspath(dest))
    try:
        if not stat.S_ISDIR(os.lstat(folder).st_mode):
            raise Refused("destination_folder_not_a_directory")
    except OSError:
        raise Refused("destination_folder_missing") from None
    if os.path.lexists(dest):
        raise Refused("destination_exists")
    _probe_no_clobber(folder)
    part = f"{dest}.part-{secrets.token_hex(8)}"
    staged = None
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
            fd, ident = _create_private(part)
            staged = ident
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
                out.flush()
                os.fsync(out.fileno())
            conn.close()
            if total != expect_size:
                raise Refused(f"size_differs:{total}")
            if digest.hexdigest() != expect_sha256:
                raise Refused("sha256_differs")
            leftovers = _publish_no_clobber(part, dest, ident, total)
            rec = {"file": os.path.basename(dest), "size": total, "sha256": digest.hexdigest(), "hops": hops, "finalHost": hops[-1]["host"]}
            if leftovers:
                rec["leftovers"] = leftovers
            return rec
        raise Refused("too_many_redirects")
    except (OSError, http.client.HTTPException, ValueError) as e:
        raise Refused(f"transport:{type(e).__name__}") from None
    finally:
        # only the staging file THIS call created (same device+inode) is ever removed; a replaced or unknown entry is preserved
        if staged is not None:
            _remove_if_ours(part, staged)


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
