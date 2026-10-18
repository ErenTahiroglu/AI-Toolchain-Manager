#!/usr/bin/env python3
"""Offline verification of ONE Sigstore bundle (as GitHub artifact attestations deliver it) against a Sigstore trusted root.

Reviewer tooling for the jq pilot evidence. It is NOT part of the application, makes NO network request, executes NOTHING it
reads, and never needs the artifact itself: it proves what a *published attestation* says (who built which digest from which
source) and that the statement is authentically signed. A later step must still hash the downloaded bytes and compare with the
attested digest (`--artifact`), which is the only link between attestation and file.

What is verified, independently of the `gh` client:
  1. the DSSE envelope signature, with the public key in the leaf certificate;
  2. the leaf certificate chains to a Fulcio CA of the trusted root that was valid at the transparency-log time, and the leaf was
     valid at that time (short-lived certificate);
  3. the transparency-log entry: its signed entry timestamp (SET), the log id, the RFC 6962/9162 inclusion proof, and the signed
     checkpoint, against the log key of the trusted root;
  4. the in-toto statement: subject names/digests, SLSA provenance fields;
  5. the identity policy given on the command line, matched against the certificate extensions (workflow identity, issuer,
     source repository and commit, repository and owner ids, runner environment).

What it does NOT establish: that the build was benign, that the source is free of defects, that the repository or its workflow
were not compromised before the build, or that the file you hold has the attested digest (use --artifact for that).
"""

import argparse
import base64
import hashlib
import json
import sys
from datetime import datetime, timezone

from cryptography import x509
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, ed25519

OID = {
    "1.3.6.1.4.1.57264.1.1": "oidcIssuerV1",
    "1.3.6.1.4.1.57264.1.2": "githubWorkflowTrigger",
    "1.3.6.1.4.1.57264.1.3": "githubWorkflowSha",
    "1.3.6.1.4.1.57264.1.4": "githubWorkflowName",
    "1.3.6.1.4.1.57264.1.5": "githubWorkflowRepository",
    "1.3.6.1.4.1.57264.1.6": "githubWorkflowRef",
    "1.3.6.1.4.1.57264.1.8": "oidcIssuer",
    "1.3.6.1.4.1.57264.1.9": "buildSignerUri",
    "1.3.6.1.4.1.57264.1.10": "buildSignerDigest",
    "1.3.6.1.4.1.57264.1.11": "runnerEnvironment",
    "1.3.6.1.4.1.57264.1.12": "sourceRepositoryUri",
    "1.3.6.1.4.1.57264.1.13": "sourceRepositoryDigest",
    "1.3.6.1.4.1.57264.1.14": "sourceRepositoryRef",
    "1.3.6.1.4.1.57264.1.15": "sourceRepositoryIdentifier",
    "1.3.6.1.4.1.57264.1.16": "sourceRepositoryOwnerUri",
    "1.3.6.1.4.1.57264.1.17": "sourceRepositoryOwnerIdentifier",
    "1.3.6.1.4.1.57264.1.18": "buildConfigUri",
    "1.3.6.1.4.1.57264.1.19": "buildConfigDigest",
    "1.3.6.1.4.1.57264.1.20": "buildTrigger",
    "1.3.6.1.4.1.57264.1.21": "runInvocationUri",
}


class Report:
    def __init__(self):
        self.checks = []

    def add(self, name, ok, detail=""):
        self.checks.append({"check": name, "ok": bool(ok), "detail": detail})
        return bool(ok)

    @property
    def ok(self):
        return all(c["ok"] for c in self.checks)


def b64(s):
    return base64.b64decode(s)


def decode_ext_value(raw):
    # newer extensions are DER UTF8String (tag 0x0c); the oldest are the raw ASCII bytes
    if len(raw) >= 2 and raw[0] == 0x0C and raw[1] == len(raw) - 2:
        return raw[2:].decode("utf-8", "replace")
    return raw.decode("utf-8", "replace")


def verify_sig(public_key, signature, message, hash_alg=None):
    if isinstance(public_key, ec.EllipticCurvePublicKey):
        public_key.verify(signature, message, ec.ECDSA(hash_alg or hashes.SHA256()))
    elif isinstance(public_key, ed25519.Ed25519PublicKey):
        public_key.verify(signature, message)
    else:
        raise InvalidSignature("unsupported key type")


def parse_time(value):
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def verify_chain(leaf, chain_certs, at):
    """leaf -> intermediate(s) -> root, signature by signature; every certificate valid at `at`."""
    certs = [leaf] + chain_certs
    for c in certs:
        if not (c.not_valid_before_utc <= at <= c.not_valid_after_utc):
            return False, "a certificate in the chain was not valid at the log time"
    for child, parent in zip(certs, certs[1:]):
        if child.issuer != parent.subject:
            return False, "issuer/subject mismatch in the chain"
        pk = parent.public_key()
        try:
            if isinstance(pk, ec.EllipticCurvePublicKey):
                pk.verify(child.signature, child.tbs_certificate_bytes, ec.ECDSA(child.signature_hash_algorithm))
            else:
                return False, "unsupported CA key type"
        except InvalidSignature:
            return False, "certificate signature invalid"
    return True, f"{len(certs)} certificates, anchored at the trusted root"


def merkle_inclusion(leaf_hash, index, size, proof, root):
    """RFC 9162 section 2.1.3.2."""
    fn, sn, r = index, size - 1, leaf_hash
    if index >= size:
        return False
    for p in proof:
        if sn == 0:
            return False
        if fn & 1 or fn == sn:
            r = hashlib.sha256(b"\x01" + p + r).digest()
            if not fn & 1:
                while not (fn & 1) and fn != 0:
                    fn >>= 1
                    sn >>= 1
        else:
            r = hashlib.sha256(b"\x01" + r + p).digest()
        fn >>= 1
        sn >>= 1
    return sn == 0 and r == root


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--bundle", required=True, help="JSON file: a bare Sigstore bundle, or the GitHub attestations API response")
    ap.add_argument("--index", type=int, default=0, help="which attestation of an API response to check")
    ap.add_argument("--trusted-root", required=True, help="`gh attestation trusted-root` output (JSON lines) or one trusted-root JSON")
    ap.add_argument("--subject-name", required=True)
    ap.add_argument("--subject-sha256", required=True)
    ap.add_argument("--repo-uri", required=True, help="e.g. https://github.com/jqlang/jq")
    ap.add_argument("--workflow-path", required=True, help="e.g. .github/workflows/ci.yml")
    ap.add_argument("--ref", required=True, help="e.g. refs/tags/jq-1.8.2")
    ap.add_argument("--commit", required=True)
    ap.add_argument("--issuer", default="https://token.actions.githubusercontent.com")
    ap.add_argument("--repo-id")
    ap.add_argument("--owner-id")
    ap.add_argument("--artifact", help="optional: a LOCAL file whose SHA-256 must equal the attested digest (never executed)")
    args = ap.parse_args()
    rep = Report()

    data = json.load(open(args.bundle, encoding="utf-8"))
    bundle = data["attestations"][args.index]["bundle"] if "attestations" in data else data
    roots = [json.loads(l) for l in open(args.trusted_root, encoding="utf-8").read().splitlines() if l.strip()]
    rep.add("bundle media type is a Sigstore bundle v0.3", bundle["mediaType"].startswith("application/vnd.dev.sigstore.bundle") and "v0.3" in bundle["mediaType"], bundle["mediaType"])

    vm = bundle["verificationMaterial"]
    leaf = x509.load_der_x509_certificate(b64(vm["certificate"]["rawBytes"]))
    tlog = vm["tlogEntries"][0]
    integrated = datetime.fromtimestamp(int(tlog["integratedTime"]), tz=timezone.utc)

    # -- 1. DSSE signature with the leaf key
    env = bundle["dsseEnvelope"]
    payload = b64(env["payload"])
    pae = b"DSSEv1 %d %s %d " % (len(env["payloadType"]), env["payloadType"].encode(), len(payload)) + payload
    try:
        verify_sig(leaf.public_key(), b64(env["signatures"][0]["sig"]), pae)
        rep.add("DSSE envelope signature verifies with the leaf certificate key", True)
    except InvalidSignature:
        rep.add("DSSE envelope signature verifies with the leaf certificate key", False)

    # -- 2. certificate chain against the trusted root, at the log time
    cas = [ca for r in roots for ca in r.get("certificateAuthorities", []) if "fulcio.sigstore.dev" in ca.get("uri", "")]
    chain_ok, detail = False, "no Fulcio CA of the trusted root was valid at the log time"
    for ca in cas:
        v = ca["validFor"]
        if parse_time(v["start"]) <= integrated and ("end" not in v or integrated <= parse_time(v["end"])):
            certs = [x509.load_der_x509_certificate(b64(c["rawBytes"])) for c in ca["certChain"]["certificates"]]
            chain_ok, detail = verify_chain(leaf, certs, integrated)
            if chain_ok:
                break
    rep.add("leaf certificate chains to a Sigstore public-good Fulcio CA valid at the log time", chain_ok, detail)
    rep.add("leaf certificate was valid at the log time (short-lived)", leaf.not_valid_before_utc <= integrated <= leaf.not_valid_after_utc,
            f"{leaf.not_valid_before_utc.isoformat()} .. {leaf.not_valid_after_utc.isoformat()} ; logged {integrated.isoformat()}")

    # -- certificate identity
    ext = {}
    for e in leaf.extensions:
        name = OID.get(e.oid.dotted_string)
        if name:
            ext[name] = decode_ext_value(e.value.value)
    san = leaf.extensions.get_extension_for_class(x509.SubjectAlternativeName).value.get_values_for_type(x509.UniformResourceIdentifier)
    want_san = f"{args.repo_uri}/{args.workflow_path}@{args.ref}"
    rep.add("certificate subject alternative name is the expected workflow identity", san == [want_san], f"{san} vs {want_san}")
    rep.add("OIDC issuer is the expected one", ext.get("oidcIssuer", ext.get("oidcIssuerV1")) == args.issuer, str(ext.get("oidcIssuer", ext.get("oidcIssuerV1"))))
    rep.add("source repository is the expected one", ext.get("sourceRepositoryUri") == args.repo_uri, str(ext.get("sourceRepositoryUri")))
    rep.add("source repository digest is the expected commit", ext.get("sourceRepositoryDigest") == args.commit, str(ext.get("sourceRepositoryDigest")))
    rep.add("source repository ref is the expected ref", ext.get("sourceRepositoryRef") == args.ref, str(ext.get("sourceRepositoryRef")))
    rep.add("build signer is the expected workflow at the expected ref", ext.get("buildSignerUri") == want_san, str(ext.get("buildSignerUri")))
    rep.add("build signer digest is the expected commit", ext.get("buildSignerDigest") == args.commit, str(ext.get("buildSignerDigest")))
    rep.add("runner environment is github-hosted", ext.get("runnerEnvironment") == "github-hosted", str(ext.get("runnerEnvironment")))
    if args.repo_id:
        rep.add("repository id matches", ext.get("sourceRepositoryIdentifier") == args.repo_id, str(ext.get("sourceRepositoryIdentifier")))
    if args.owner_id:
        rep.add("repository owner id matches", ext.get("sourceRepositoryOwnerIdentifier") == args.owner_id, str(ext.get("sourceRepositoryOwnerIdentifier")))

    # -- 3. transparency log entry
    tlogs = [t for r in roots for t in r.get("tlogs", []) if "rekor.sigstore.dev" in t.get("baseUrl", "")]
    key_id = b64(tlog["logId"]["keyId"])
    log = next((t for t in tlogs if b64(t["logId"]["keyId"]) == key_id), None)
    rep.add("transparency log id is a log of the trusted root", log is not None, key_id.hex())
    if log is not None:
        logkey = serialization.load_der_public_key(b64(log["publicKey"]["rawBytes"]))
        rep.add("log key id is the SHA-256 of the log public key", hashlib.sha256(b64(log["publicKey"]["rawBytes"])).digest() == key_id)
        body = b64(tlog["canonicalizedBody"])
        entry = json.loads(body)
        set_payload = json.dumps(
            {"body": tlog["canonicalizedBody"], "integratedTime": int(tlog["integratedTime"]), "logID": key_id.hex(), "logIndex": int(tlog["logIndex"])},
            separators=(",", ":"), sort_keys=True,
        ).encode()
        try:
            verify_sig(logkey, b64(tlog["inclusionPromise"]["signedEntryTimestamp"]), set_payload)
            rep.add("signed entry timestamp (SET) verifies with the log key", True)
        except InvalidSignature:
            rep.add("signed entry timestamp (SET) verifies with the log key", False)
        proof = tlog["inclusionProof"]
        leaf_hash = hashlib.sha256(b"\x00" + body).digest()
        okp = merkle_inclusion(leaf_hash, int(proof["logIndex"]), int(proof["treeSize"]), [b64(h) for h in proof["hashes"]], b64(proof["rootHash"]))
        rep.add("Merkle inclusion proof reproduces the signed root hash", okp, f"index {proof['logIndex']} of {proof['treeSize']}")
        # signed checkpoint (a signed note: text, blank line, signature lines)
        cp = proof["checkpoint"]["envelope"]
        text, _, sigs = cp.partition("\n\n")
        lines = text.split("\n")
        cp_ok = False
        for line in sigs.strip().split("\n"):
            if line.startswith("— "):
                raw = b64(line.split(" ")[2])
                try:
                    verify_sig(logkey, raw[4:], (text + "\n").encode())
                    cp_ok = True
                except InvalidSignature:
                    pass
        rep.add("checkpoint signature verifies with the log key", cp_ok)
        rep.add("checkpoint size and root hash equal the inclusion proof", lines[1] == proof["treeSize"] and b64(lines[2]) == b64(proof["rootHash"]))
        # the logged entry must be this very envelope
        spec = entry.get("spec", {})
        logged_sig = spec.get("signatures", [{}])[0].get("signature")
        rep.add("the logged DSSE entry carries this envelope's signature", logged_sig == env["signatures"][0]["sig"])
        logged_hash = spec.get("payloadHash", {}).get("value")
        rep.add("the logged payload hash equals the envelope payload", logged_hash == hashlib.sha256(payload).hexdigest())

    # -- 4. in-toto statement and SLSA provenance
    st = json.loads(payload)
    rep.add("statement is in-toto v1 with SLSA provenance v1", st["_type"] == "https://in-toto.io/Statement/v1" and st["predicateType"] == "https://slsa.dev/provenance/v1", st["predicateType"])
    subj = {s["name"]: s["digest"].get("sha256") for s in st["subject"]}
    rep.add("the expected file name is a subject with the expected SHA-256", subj.get(args.subject_name) == args.subject_sha256, f"{args.subject_name}: {subj.get(args.subject_name)}")
    pred = st["predicate"]
    ext_params = pred["buildDefinition"]["externalParameters"]["workflow"]
    rep.add("provenance workflow repository/path/ref match", ext_params == {"ref": args.ref, "repository": args.repo_uri, "path": args.workflow_path}, json.dumps(ext_params, sort_keys=True))
    deps = pred["buildDefinition"]["resolvedDependencies"]
    rep.add("provenance source commit matches", any(d["digest"].get("gitCommit") == args.commit for d in deps), json.dumps(deps))
    rep.add("builder id is the expected workflow identity", pred["runDetails"]["builder"]["id"] == want_san, pred["runDetails"]["builder"]["id"])
    gh = pred["buildDefinition"]["internalParameters"]["github"]
    rep.add("provenance runner is github-hosted", gh.get("runner_environment") == "github-hosted", str(gh))

    # -- 6. optionally the local file
    if args.artifact:
        h = hashlib.sha256()
        with open(args.artifact, "rb") as f:
            for chunk in iter(lambda: f.read(1 << 20), b""):
                h.update(chunk)
        rep.add("local file SHA-256 equals the attested digest", h.hexdigest() == args.subject_sha256, h.hexdigest())

    out = {
        "ok": rep.ok,
        "checks": rep.checks,
        "certificateExtensions": ext,
        "subjectCount": len(st["subject"]),
        "logIndex": tlog["logIndex"],
        "integratedTime": integrated.isoformat(),
        "invocation": pred["runDetails"].get("metadata", {}).get("invocationId"),
    }
    print(json.dumps(out, indent=2))
    return 0 if rep.ok else 1


if __name__ == "__main__":
    sys.exit(main())
