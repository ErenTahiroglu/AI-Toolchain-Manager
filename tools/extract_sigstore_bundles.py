#!/usr/bin/env python3
"""Extract the Sigstore bundles from a stored GitHub attestations API response as JSON Lines, BYTE-FOR-BYTE.

`gh attestation verify --bundle` reads one bare Sigstore bundle per line. The evidence stored in this repository is the GitHub API
response wrapper (`{"attestations":[{"repository_id":..., "bundle_url":..., "bundle":{...}}, ...]}`) and is rejected by the CLI
("unknown field attestations"). This tool turns the wrapper into the required format WITHOUT re-serialising anything: for every
attestation the exact text of its `bundle` value is copied from the original file (located with the standard JSON decoder's own
value boundaries), then checked to decode to the same value as the parsed original. Nothing is added, removed, reordered or
re-escaped inside a bundle, so no signature, certificate or trusted-root check is affected or weakened.

Offline: no network, nothing executed. Refuses to overwrite an existing output file, a response without a bundle, a bundle that
contains a raw line break (it could not be a single line without altering it), and trailing garbage.
"""

import argparse
import hashlib
import json
import os
import sys

DEC = json.JSONDecoder()
WS = " \t\r\n"


class Refused(Exception):
    pass


def _ws(t, i):
    while i < len(t) and t[i] in WS:
        i += 1
    return i


def _value_span(t, i):
    """(start, end) of the JSON value that begins at t[i] (leading whitespace skipped)."""
    i = _ws(t, i)
    try:
        _, end = DEC.raw_decode(t, i)
    except ValueError:
        raise Refused("not_valid_json") from None
    return i, end


def _members(t, i, kind):
    """Yield (key, start, end) for the members of the object/array that starts at t[i]; also returns the end index via StopIteration value."""
    i = _ws(t, i)
    open_, close = ("{", "}") if kind == "object" else ("[", "]")
    if t[i:i + 1] != open_:
        raise Refused(f"expected_{kind}")
    i = _ws(t, i + 1)
    if t[i:i + 1] == close:
        return i + 1
    while True:
        key = None
        if kind == "object":
            ks, ke = _value_span(t, i)
            key = DEC.raw_decode(t, ks)[0]
            i = _ws(t, ke)
            if t[i:i + 1] != ":":
                raise Refused("malformed_object")
            i += 1
        vs, ve = _value_span(t, i)
        yield key, vs, ve
        i = _ws(t, ve)
        if t[i:i + 1] == ",":
            i = _ws(t, i + 1)
            continue
        if t[i:i + 1] == close:
            return i + 1
        raise Refused("malformed_container")


def extract(raw: bytes):
    """Returns a list of the exact bundle texts (str) in the order of the attestations."""
    try:
        t = raw.decode("utf-8")
    except UnicodeDecodeError:
        raise Refused("not_utf8") from None
    whole = json.loads(t)
    if not isinstance(whole, dict) or not isinstance(whole.get("attestations"), list):
        raise Refused("no_attestations_array")
    if _ws(t, _value_span(t, 0)[1]) != len(t):
        raise Refused("trailing_data")
    spans = None
    for key, vs, ve in _members(t, 0, "object"):
        if key == "attestations":
            spans = (vs, ve)
    if spans is None:
        raise Refused("no_attestations_array")
    out = []
    for n, (_, vs, ve) in enumerate(_members(t, spans[0], "array")):
        found = None
        for key, bs, be in _members(t, vs, "object"):
            if key == "bundle":
                found = (bs, be)
        if found is None or whole["attestations"][n].get("bundle") is None:
            raise Refused(f"attestation_{n}_has_no_bundle")
        text = t[found[0]:found[1]]
        if "\n" in text or "\r" in text:
            raise Refused(f"attestation_{n}_bundle_contains_a_line_break")
        if json.loads(text) != whole["attestations"][n]["bundle"]:
            raise Refused(f"attestation_{n}_bundle_text_differs_from_parsed_value")
        out.append(text)
    if not out:
        raise Refused("no_attestations")
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--in", dest="src", required=True)
    ap.add_argument("--out", required=True, help="new file; must not exist")
    a = ap.parse_args()
    try:
        with open(a.src, "rb") as f:
            bundles = extract(f.read())
        data = "".join(b + "\n" for b in bundles).encode("utf-8")
        fd = os.open(a.out, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_BINARY", 0), 0o600)
    except FileExistsError:
        print(json.dumps({"ok": False, "refused": "output_exists"}))
        return 1
    except (Refused, OSError, ValueError) as e:
        print(json.dumps({"ok": False, "refused": str(e) if isinstance(e, Refused) else type(e).__name__}))
        return 1
    with os.fdopen(fd, "wb") as out:
        out.write(data)
    print(json.dumps({"ok": True, "bundles": len(bundles), "bundleSha256": [hashlib.sha256(b.encode("utf-8")).hexdigest() for b in bundles], "outSha256": hashlib.sha256(data).hexdigest()}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
