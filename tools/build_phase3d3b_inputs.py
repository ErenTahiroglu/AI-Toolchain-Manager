#!/usr/bin/env python3
"""Phase 3D-3B: writes the two small data files the review package pins. Offline, read-only apart from the two output files.

  docs/phase-3d3b/staging-fixture.json        the identity of the synthetic staging fixture (rebuilt by tools/staging_fixture.py)
  docs/phase-3d3b/evidence/reference-entry.json   the EXACT authenticated-reference entry that the activation change would add (nothing is
                                                  compiled in by this phase; a Rust test validates the candidate against it)
"""
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import staging_fixture  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "phase-3d3b"
WIN_SHA = "a6fc67fedaf9128a3309a1e2ebb8b986aeccf70122ee46d2cb4849e423f0c627"


def canonical(value):
    """The canonical JSON of src-tauri/src/audit.rs::canonical_json, written independently: sorted keys, no spaces, UTF-8 unescaped."""
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def write(path, obj):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")


def main():
    fx = staging_fixture.describe()
    fx["note"] = "Made-up stand-in for the jq executable. Not jq, not a program, never executed. Rebuilt independently by tools/staging_fixture.py."
    fx["reproduce"] = "python tools/staging_fixture.py print"
    write(OUT / "staging-fixture.json", fx)
    write(
        OUT / "evidence" / "reference-entry.json",
        {
            "entryId": "jq",
            "kind": "executable",
            "format": "pe-x64",
            "platformTarget": "windows-x64",
            "version": "1.8.2",
            "sha256": WIN_SHA,
            "authentication": "jq-1-8-2-windows-x64-request-a: the exact-file digest equals the attestation subject, the release digest, the publisher checksum list and the digest of the retained bytes (see docs/phase-3d3/evidence/jq-1.8.2-windows-x64.source-attestation.json)",
        },
    )
    cand = json.loads((ROOT / "docs" / "phase-3d3" / "jq-1.8.2-windows-x64.recipe.candidate.json").read_text(encoding="utf-8"))
    write(
        OUT / "evidence" / "candidate-recipe-digest.json",
        {
            "note": "SHA-256 of the canonical JSON of the candidate's `recipe` object, computed by Python (tools/build_phase3d3b_inputs.py). The Rust test `the_candidate_recipe_digest_is_the_one_an_independent_python_computation_gives` recomputes it with the application's own code; any edit of the recipe changes it and invalidates every earlier review claim.",
            "recipeId": cand["recipe"]["recipeId"],
            "revision": cand["recipe"]["revision"],
            "recipeDigest": hashlib.sha256(canonical(cand["recipe"]).encode("utf-8")).hexdigest(),
        },
    )
    print("ok")


if __name__ == "__main__":
    main()
