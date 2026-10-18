#!/usr/bin/env python3
"""Consensus review of ONE jq verification session (no network, nothing executed, nothing downloaded).

Reviewer tooling for a FUTURE, separately authorised download-and-verify session (docs/phase-3d0/REQUEST-A.md). It does not run
`gh` or `gpg` itself; it reads the files those tools produced in the session folder and refuses unless EVERYTHING agrees:

  per artifact (`<name>` is the exact release file name):
    artifacts/<name>/<name> the downloaded bytes (each file alone in its own new folder)
    <name>.gh.json/.exit    stdout / exit code of `gh attestation verify ... --format json` fetching attestations online
    <name>.gh-offline.json/.exit   the same with the stored bundle and the freshly fetched trusted root (--bundle, --custom-trusted-root)
    <name>.custom.json      stdout of tools/verify_sigstore_bundle.py ... --artifact  (supporting evidence only)
    <name>.gpg.status       stdout of `gpg --status-fd=1 --verify <name>.asc <name>` with an isolated keyring
  per session:
    keyring.colons          `gpg --list-keys --with-colons --with-fingerprint` of that isolated keyring
    fetch.log               one JSON record per download (tools/fetch_pinned.py --log)
    pinned/<file>           the pinned text files exactly as fetched (signatures, key, checksum list)

Rule: the official verifier is the authority for the attestation; the custom verifier is supporting evidence; if they DISAGREE the
session FAILS. A missing or malformed input FAILS. Nothing here can promote a digest to an authenticated reference: that stays a
reviewed code change after the Red Team has accepted the report.
"""

import hashlib
import json
import os
import re
import sys

PINS = {
    "repository": "jqlang/jq",
    "repositoryUri": "https://github.com/jqlang/jq",
    "repositoryId": "5101141",
    "ownerId": "104800540",
    "workflowPath": ".github/workflows/ci.yml",
    "ref": "refs/tags/jq-1.8.2",
    "commit": "34f7186b86743a083a589741b6cea95293524108",
    "issuer": "https://token.actions.githubusercontent.com",
    "signatureCommit": "f0dabad36a5fdb37393829e6bd4c465d70c40a3a",
    "gpgFingerprint": "93079A9511EFC0B7D6CDBFB5B0DA60FB454BAF18",
    "artifacts": {
        "jq-windows-amd64.exe": {"size": 1035264, "sha256": "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627", "sig": "jq-windows-amd64.exe.asc"},
        "jq-macos-arm64": {"size": 841504, "sha256": "2d75340ba57a4b4b4c8708a21c2dc8e958a48aaa8bba13b27f77f6e4c0eca07e", "sig": "jq-macos-arm64.asc"},
    },
    "pinnedText": {
        "jq-windows-amd64.exe.asc": "055c433ac06536be0acf06edc727c1b0db2de97e7d3c85a8f1cbef32bffbf2f0",
        "jq-macos-arm64.asc": "8ce9edc9647d78a4320de42ce6aa4ae82e7eb3c98a661011cd37ce9ebd32a6f1",
        "jq-release-new.key": "710cc5f046706bb728479e57aebe0ade744317cbd27ecc0bf5d4c92cb582283b",
        "sha256sum.txt": "dc86824a41c165ece971ff691aff6e08bbfe6e1d1f531688b47ee78c283a85cd",
    },
    # hosts that may be contacted by the download session itself (the TUF/Sigstore hosts of the GitHub CLI are reported separately)
    "allowedDownloadHosts": ["github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com", "raw.githubusercontent.com"],
}


class Review:
    def __init__(self):
        self.checks = []

    def ok(self, name, cond, detail=""):
        self.checks.append({"check": name, "ok": bool(cond), "detail": str(detail)[:300]})
        return bool(cond)

    @property
    def passed(self):
        return bool(self.checks) and all(c["ok"] for c in self.checks)


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def read_json(path):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def read_text(path):
    try:
        with open(path, encoding="utf-8") as f:
            return f.read()
    except OSError:
        return None


CERT_EXPECT = {
    # name in the GitHub CLI's parsed certificate -> expected value. A field that is PRESENT must match; the same facts are enforced
    # from the raw certificate by the custom verifier and by the CLI's own flags, so a missing name is reported, not trusted.
    "issuer": lambda p: p["issuer"],
    "subjectAlternativeName": lambda p: f"{p['repositoryUri']}/{p['workflowPath']}@{p['ref']}",
    "githubWorkflowRepository": lambda p: p["repository"],
    "githubWorkflowRef": lambda p: p["ref"],
    "githubWorkflowSHA": lambda p: p["commit"],
    "sourceRepositoryURI": lambda p: p["repositoryUri"],
    "sourceRepositoryDigest": lambda p: p["commit"],
    "sourceRepositoryRef": lambda p: p["ref"],
    "sourceRepositoryIdentifier": lambda p: p["repositoryId"],
    "sourceRepositoryOwnerIdentifier": lambda p: p["ownerId"],
    "buildSignerDigest": lambda p: p["commit"],
    "runnerEnvironment": lambda p: "github-hosted",
}


def review_gh(r, name, pins, session, tag="gh"):
    label = "online" if tag == "gh" else "offline replay"
    exitcode = read_text(os.path.join(session, f"{name}.{tag}.exit"))
    exit_ok = r.ok(f"{name}: the GitHub CLI verification ({label}) exited 0", exitcode is not None and exitcode.strip() == "0", exitcode)
    gh = read_json(os.path.join(session, f"{name}.{tag}.json"))
    good = isinstance(gh, list) and len(gh) >= 1 and all(isinstance(e, dict) and "verificationResult" in e and "attestation" in e for e in gh)
    r.ok(f"{name}: the CLI ({label}) returned a non-empty structured result", good)
    if not good:
        return False
    if not exit_ok:
        return False
    art = pins["artifacts"][name]
    all_fine = True
    for i, e in enumerate(gh):
        vr = e["verificationResult"]
        stmt = vr.get("statement", {})
        subj = {s.get("name"): (s.get("digest") or {}).get("sha256") for s in stmt.get("subject", [])}
        all_fine &= r.ok(f"{name}: {label} result {i}: subject name and SHA-256 are the pinned ones", subj.get(name) == art["sha256"], subj.get(name))
        all_fine &= r.ok(f"{name}: {label} result {i}: predicate type is SLSA provenance v1", stmt.get("predicateType") == "https://slsa.dev/provenance/v1", stmt.get("predicateType"))
        all_fine &= r.ok(f"{name}: {label} result {i}: a transparency-log timestamp was verified", bool(vr.get("verifiedTimestamps")))
        cert = (vr.get("signature") or {}).get("certificate") or {}
        missing = []
        for field, expect in CERT_EXPECT.items():
            if field not in cert:
                missing.append(field)
                continue
            all_fine &= r.ok(f"{name}: {label} result {i}: certificate {field} equals the pinned value", cert[field] == expect(pins), cert[field])
        r.ok(f"{name}: {label} result {i}: certificate fields not present in the CLI output (informational)", True, ",".join(missing) or "none")
        r.ok(f"{name}: {label} result {i}: the certificate section is present at all", bool(cert))
        all_fine &= bool(cert)
    return all_fine


def review_custom(r, name, pins, session):
    c = read_json(os.path.join(session, name + ".custom.json"))
    art = pins["artifacts"][name]
    if not isinstance(c, dict):
        return r.ok(f"{name}: the supporting verifier produced a result", False)
    checks = {x.get("check"): x.get("ok") for x in c.get("checks", [])}
    byte_check = [ok for k, ok in checks.items() if k == "local file SHA-256 equals the attested digest"]
    results = [
        r.ok(f"{name}: the supporting verifier passed", c.get("ok") is True and all(checks.values()) and len(checks) >= 28, len(checks)),
        r.ok(f"{name}: the supporting verifier compared the DOWNLOADED bytes", byte_check == [True]),
    ]
    ext = c.get("certificateExtensions", {})
    results += [
        r.ok(f"{name}: supporting verifier saw the pinned commit", ext.get("sourceRepositoryDigest") == pins["commit"]),
        r.ok(f"{name}: supporting verifier saw the pinned ref", ext.get("sourceRepositoryRef") == pins["ref"]),
        r.ok(f"{name}: supporting verifier saw the pinned repository id", ext.get("sourceRepositoryIdentifier") == pins["repositoryId"]),
        r.ok(f"{name}: supporting verifier saw the pinned owner id", ext.get("sourceRepositoryOwnerIdentifier") == pins["ownerId"]),
        r.ok(
            f"{name}: the supporting verifier matched the pinned name and digest as an attestation subject",
            any(x.get("check") == "the expected file name is a subject with the expected SHA-256" and x.get("ok") is True and x.get("detail", "").endswith(art["sha256"]) for x in c.get("checks", [])),
        ),
    ]
    return all(results)


def review_gpg(r, name, pins, session):
    status = read_text(os.path.join(session, name + ".gpg.status")) or ""
    fpr = pins["gpgFingerprint"]
    lines = [l for l in status.splitlines() if l.startswith("[GNUPG:] ")]
    validsig = [l.split() for l in lines if l.startswith("[GNUPG:] VALIDSIG ")]
    r.ok(f"{name}: exactly one valid GPG signature", len(validsig) == 1, len(validsig))
    r.ok(f"{name}: no bad, expired, revoked or erroneous signature", not any(re.match(r"\[GNUPG:\] (BADSIG|ERRSIG|EXPKEYSIG|REVKEYSIG|EXPSIG|NO_PUBKEY)", l) for l in lines))
    r.ok(f"{name}: GOODSIG was reported", any(l.startswith("[GNUPG:] GOODSIG ") for l in lines))
    ok_fpr = len(validsig) == 1 and validsig[0][2] == fpr and validsig[0][-1] == fpr
    return r.ok(f"{name}: the signing key is the pinned fingerprint (signing key and primary key)", ok_fpr, validsig[0][2] if validsig else "")


def review(session, pins=PINS):
    r = Review()
    # -- the pinned text inputs are exactly the pinned bytes
    for fname, want in pins["pinnedText"].items():
        p = os.path.join(session, "pinned", fname)
        r.ok(f"pinned file {fname} has the pinned SHA-256", os.path.isfile(p) and sha256_file(p) == want)
    listing = read_text(os.path.join(session, "pinned", "sha256sum.txt")) or ""
    # -- the isolated keyring holds exactly the pinned primary key
    colons = read_text(os.path.join(session, "keyring.colons")) or ""
    rows = colons.splitlines()
    primaries = [rows[i + 1].split(":")[9] for i, l in enumerate(rows[:-1]) if l.startswith("pub:") and rows[i + 1].startswith("fpr:")]
    r.ok("the isolated keyring contains exactly one primary key and it is the pinned fingerprint", primaries == [pins["gpgFingerprint"]], primaries)
    r.ok("the keyring has no second certificate", sum(1 for l in rows if l.startswith("pub:")) == 1)
    # -- download log: hosts
    hosts, sessions_ok = set(), True
    log_text = read_text(os.path.join(session, "fetch.log")) or ""
    for line in log_text.splitlines():
        try:
            rec = json.loads(line)
        except ValueError:
            sessions_ok = False
            continue
        for h in rec.get("hops", []):
            hosts.add(h.get("host"))
            sessions_ok &= h.get("scheme") == "https" and h.get("host") in pins["allowedDownloadHosts"]
    r.ok("every download hop used https and an allowed host", sessions_ok and bool(log_text), sorted(hosts))
    # -- per artifact
    for name, art in pins["artifacts"].items():
        path = os.path.join(session, "artifacts", name, name)  # each downloaded file lives alone in its own new folder
        present = os.path.isfile(path)
        r.ok(f"{name}: the downloaded file exists", present)
        if not present:
            continue
        r.ok(f"{name}: size is the pinned size", os.path.getsize(path) == art["size"], os.path.getsize(path))
        digest = sha256_file(path)
        r.ok(f"{name}: SHA-256 equals the pinned value", digest == art["sha256"], digest)
        r.ok(f"{name}: SHA-256 appears in the pinned publisher checksum list", re.search(rf"^{art['sha256']}\s+\*?{re.escape(name)}\s*$", listing, re.M) is not None)
        r.ok(f"{name}: the file has no execute permission", os.name == "nt" or (os.stat(path).st_mode & 0o111) == 0)
        gh_ok = review_gh(r, name, pins, session, "gh")
        gh_ok = review_gh(r, name, pins, session, "gh-offline") and gh_ok
        custom_ok = review_custom(r, name, pins, session)
        r.ok(f"{name}: the maintained verifier and the supporting verifier AGREE (both passed)", gh_ok and custom_ok, f"gh={gh_ok} custom={custom_ok}")
        review_gpg(r, name, pins, session)
    return r


def main():
    if len(sys.argv) != 2:
        print("usage: review_jq_verification.py <session-folder>", file=sys.stderr)
        return 2
    r = review(sys.argv[1])
    print(json.dumps({"ok": r.passed, "failed": [c for c in r.checks if not c["ok"]], "checks": r.checks}, indent=2))
    return 0 if r.passed else 1


if __name__ == "__main__":
    sys.exit(main())
