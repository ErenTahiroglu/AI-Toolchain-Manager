#!/usr/bin/env python3
"""Verifies the jq review package (Phase 3D-3B) against the repository. Offline, read-only, standard library only; it never opens the jq executable.

  python tools/verify_review_package.py [docs/phase-3d3b/review-package.json]

Checks (each reported by name; exit 0 only if every one passes):
  * every artifact listed in the package has the recorded SHA-256 -- of the COMMITTED (LF) form -- in the working tree AND in the git object of
    the candidate commit, and every JSON artifact has the recorded canonical-JSON SHA-256 (computed here, independently of the application);
  * the recipe digest equals the SHA-256 of the canonical JSON of the candidate's `recipe` object;
  * every retained-evidence file named by the attestation record is a TRACKED file with the recorded hash;
  * the staging fixture is rebuilt from its format description and has the recorded size and SHA-256;
  * the candidate's review block is still empty (state `draft`, reviewer `none-yet`, no staging evidence): nobody filled it in;
  * the compiled-in lists are still empty (recipes, authenticated references, evidence);
  * every commit after the candidate commit changed only files under docs/phase-3d3b/ (so the package describes the tree it names).
"""
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT = ROOT / "docs" / "phase-3d3b" / "review-package.json"
sys.path.insert(0, str(Path(__file__).resolve().parent))
import staging_fixture  # noqa: E402


def canonical(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def lf(b):
    return b.replace(bytes([13, 10]), bytes([10]))


def sha(b):
    return hashlib.sha256(b).hexdigest()


def git(*args, binary=False):
    p = subprocess.run(["git", *args], cwd=ROOT, capture_output=True)
    if p.returncode != 0:
        raise RuntimeError(p.stderr.decode("utf-8", "replace"))
    return p.stdout if binary else p.stdout.decode("utf-8", "replace")


def main(argv):
    path = Path(argv[0]) if argv else DEFAULT
    pkg = json.loads(path.read_text(encoding="utf-8"))
    results = []

    def check(name, ok, detail=""):
        results.append({"check": name, "ok": bool(ok), "detail": detail})

    cand = pkg["candidateCommit"]
    # a shallow clone (as CI makes) does not contain the candidate commit: those checks are then reported as SKIPPED, never as passed
    have = subprocess.run(["git", "cat-file", "-e", f"{cand}^{{commit}}"], cwd=ROOT, capture_output=True).returncode == 0
    skipped = []
    tracked = set(git("ls-files", "-z").split("\0"))
    for a in pkg["artifacts"]:
        rel = a["path"]
        wt = lf((ROOT / rel).read_bytes())
        check(f"{rel}: tracked", rel in tracked)
        check(f"{rel}: working-tree SHA-256 (LF form)", sha(wt) == a["sha256"], a["sha256"])
        if have:
            try:
                blob = lf(git("show", f"{cand}:{rel}", binary=True))
                check(f"{rel}: SHA-256 in the candidate commit", sha(blob) == a["sha256"])
            except RuntimeError as e:
                check(f"{rel}: exists in the candidate commit", False, str(e)[:80])
        else:
            skipped.append(f"{rel}: SHA-256 in the candidate commit")
        if "canonicalSha256" in a:
            check(f"{rel}: canonical-JSON SHA-256", sha(canonical(json.loads(wt)).encode("utf-8")) == a["canonicalSha256"])

    dev = pkg.get("devRun")
    if dev:
        rp = ROOT / dev["report"]
        check("developer staging report: tracked", dev["report"] in tracked)
        check("developer staging report: SHA-256 (LF form)", sha(lf(rp.read_bytes())) == dev["sha256"])
        check("developer staging report: made on the candidate commit with a clean tree and all checks passed", dev["commit"] == cand and dev["workingTreeClean"] and dev["allPassed"])

    cand_rec = json.loads((ROOT / "docs/phase-3d3/jq-1.8.2-windows-x64.recipe.candidate.json").read_text(encoding="utf-8"))
    check("recipe digest = SHA-256 of the canonical JSON of the recipe object", sha(canonical(cand_rec["recipe"]).encode("utf-8")) == pkg["recipe"]["recipeDigest"], pkg["recipe"]["recipeDigest"])
    rv = cand_rec["review"]
    check("review block untouched (draft, none-yet, no staging evidence)", rv["state"] == "draft" and rv["reviewerId"] == "none-yet" and rv["stagingEvidence"] == [] and rv["reviewedRecipeDigest"] == "")

    att = json.loads((ROOT / "docs/phase-3d3/evidence/jq-1.8.2-windows-x64.source-attestation.json").read_text(encoding="utf-8"))
    for f in att["retainedEvidence"]:
        rel = f["path"]
        check(f"retained evidence tracked: {rel}", rel in tracked)
        check(f"retained evidence hash: {rel}", sha(lf((ROOT / rel).read_bytes())) == f["sha256"])
    listing = ROOT / "docs/phase-3d0/evidence/request-a-session-20261018/session-sha256.txt"
    check("retained session listing hash", sha(lf(listing.read_bytes())) == att["retainedSessionListingSha256"])
    check("the listing names the target file digest", pkg["target"]["sha256"] in listing.read_text(encoding="utf-8"))

    fx = json.loads((ROOT / "docs/phase-3d3b/staging-fixture.json").read_text(encoding="utf-8"))
    for p, want in fx["platforms"].items():
        b = staging_fixture.fixture(p)
        check(f"staging fixture {p}: rebuilt size and SHA-256", len(b) == want["sizeBytes"] and sha(b) == want["sha256"], want["sha256"])

    src = (ROOT / "src-tauri/src/recipe.rs").read_text(encoding="utf-8")
    check("compiled-in recipe list empty", 'pub const BUILT_IN_RECIPES_JSON: &str = "[]";' in src)
    check("compiled-in evidence list empty", '{"attestations":[],"localUseLicenses":[]}' in (ROOT / "src-tauri/src/attestation.rs").read_text(encoding="utf-8"))
    check("compiled-in authenticated references empty", "pub const BUILT_IN_AUTHENTICATED: &[BuiltIn] = &[];" in (ROOT / "src-tauri/src/discovery/reference.rs").read_text(encoding="utf-8"))
    flow = (ROOT / "src-tauri/src/install_flow.rs").read_text(encoding="utf-8")
    check("enablement list empty in the release supply", "evidence: Evidence::production(),\n            pilot_gate: Vec::new()," in flow.replace("\r\n", "\n"))

    if have:
        # The package was frozen at the commit the independent reviewer examined and accepted (recorded by the Phase 3D-4 binding, when it exists);
        # later commits belong to later phases and are checked by their own rules. Without that record the range still ends at HEAD, as before.
        head = "HEAD"
        binding = ROOT / "docs/phase-3d4/b1-review-binding.json"
        if binding.is_file():
            head = json.loads(binding.read_text(encoding="utf-8"))["accepted"]["finalSha"]
        changed = [l for l in git("diff", "--name-only", f"{cand}..{head}").splitlines() if l]
        outside = [c for c in changed if not c.startswith("docs/phase-3d3b/")]
        check("commits after the candidate commit touch only docs/phase-3d3b/", not outside, ", ".join(outside[:5]))
    else:
        skipped.append("commits after the candidate commit touch only docs/phase-3d3b/")

    ok = all(r["ok"] for r in results)
    print(json.dumps({"package": str(path.relative_to(ROOT)) if path.is_absolute() and ROOT in path.parents else str(path), "candidateCommit": cand, "passed": sum(r["ok"] for r in results), "total": len(results), "allPassed": ok, "skippedBecauseTheCandidateCommitIsNotAvailableLocally": skipped, "failed": [r for r in results if not r["ok"]]}, indent=2))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
