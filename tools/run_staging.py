#!/usr/bin/env python3
"""Reproduces the staging checks of the jq pilot review package (Phase 3D-3B) and reports each one by its exact identifier.

  python tools/run_staging.py [--report out.json] [--skip-python] [--skip-frontend] [--skip-cargo]

What it does: runs exactly the identifiers listed in docs/phase-3d3b/staging-manifest.json -- the Rust tests with `cargo test --lib -- --exact ...`,
the Python contract modules with `unittest`, and (unless skipped) the two frontend test files with `vitest` -- and fails if an identifier is
missing, renamed, ignored or failing. It uses only SYNTHETIC fixture bytes; it never touches jq, never uses the network (it starts cargo, python
and node, which are the build tools of this repository, and nothing else) and writes only the report file you name.

Exit status: 0 = every named check passed; 1 = a check failed or is missing; 3 = a test executable was BLOCKED by the operating system
(for example Smart App Control, `os error 4551`): those checks are reported as BLOCKED / NOT RUN, never as passed, and nothing is retried or rebuilt
to get around the block.
"""
import argparse
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "docs" / "phase-3d3b" / "staging-manifest.json"
BLOCK = re.compile(r"os error 4551|never executed|Uygulama Denetimi ilkesi", re.I)


def run(cmd, cwd, timeout=3000):
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=timeout)
    return p.returncode, p.stdout + "\n" + p.stderr


def version(cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=30).stdout.strip().splitlines()[0]
    except Exception:  # noqa: BLE001 - a missing tool is reported, not fatal here
        return "unavailable"


def git(*args):
    try:
        return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, timeout=30).stdout.strip()
    except Exception:  # noqa: BLE001
        return ""


def cargo_group(ids):
    results = {i: "missing" for i in ids}
    code, out = run(["cargo", "test", "--lib", "--", "--exact", *ids], ROOT / "src-tauri")
    if BLOCK.search(out) and "test result" not in out:
        return {i: "blocked_not_run" for i in ids}, out
    for m in re.finditer(r"^test (\S+) \.\.\. (ok|FAILED|ignored)", out, re.M):
        if m.group(1) in results:
            results[m.group(1)] = {"ok": "passed", "FAILED": "failed", "ignored": "ignored"}[m.group(2)]
    return results, out


def python_group(modules):
    results = {}
    out_all = ""
    for mod in modules:
        code, out = run([sys.executable, "-m", "unittest", mod], ROOT)
        out_all += out
        results[mod] = "passed" if code == 0 and re.search(r"^OK", out, re.M) else "failed"
    return results, out_all


def frontend_group(spec):
    results = {t: "missing" for t in spec["titles"]}
    out_all = ""
    for f in spec["files"]:
        tmp = ROOT / "tools" / f"_vitest_{Path(f).stem}.json"
        code, out = run([shutil.which("npx") or "npx", "vitest", "run", f, "--reporter=json", f"--outputFile={tmp}"], ROOT)
        out_all += out
        try:
            data = json.loads(tmp.read_text(encoding="utf-8"))
            for tr in data.get("testResults", []):
                for a in tr.get("assertionResults", []):
                    if a.get("title") in results:
                        results[a["title"]] = "passed" if a.get("status") == "passed" else "failed"
        except Exception:  # noqa: BLE001
            pass
        finally:
            if tmp.exists():
                tmp.unlink()
    return results, out_all


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--report")
    ap.add_argument("--manifest", default=str(MANIFEST), help="a staging manifest (default: the Phase 3D-3B jq review package's)")
    ap.add_argument("--skip-python", action="store_true")
    ap.add_argument("--skip-frontend", action="store_true")
    ap.add_argument("--skip-cargo", action="store_true")
    a = ap.parse_args()
    manifest_path = Path(a.manifest).resolve()
    m = json.loads(manifest_path.read_text(encoding="utf-8"))
    report = {
        "kind": "staging-run",
        "manifest": manifest_path.relative_to(ROOT).as_posix() if manifest_path.is_relative_to(ROOT) else manifest_path.name,
        "commit": git("rev-parse", "HEAD"),
        "workingTreeClean": git("status", "--porcelain") == "",
        "tools": {"cargo": version(["cargo", "--version"]), "rustc": version(["rustc", "--version"]), "python": sys.version.split()[0], "node": version(["node", "--version"])},
        "evidenceClass": "developer-generated when run by the developer; CI-generated when run by the CI job; neither is independent evidence unless an independent reviewer runs it themselves",
        "requirements": [],
    }
    failed = blocked = False
    cargo_ids = [t for r in m["requirements"] for t in r["cargo"]]
    cres = {}
    if not a.skip_cargo:
        cres, _ = cargo_group(cargo_ids)
    for r in m["requirements"]:
        rows = [{"id": t, "runner": "cargo", "result": cres.get(t, "not_requested")} for t in r["cargo"]]
        report["requirements"].append({"id": r["id"], "title": r["title"], "checks": rows})
        for row in rows:
            if row["result"] == "blocked_not_run":
                blocked = True
            elif row["result"] not in ("passed", "not_requested"):
                failed = True
    extra = []
    if not a.skip_python:
        pres, _ = python_group(m["python"])
        extra += [{"id": k, "runner": "python-unittest", "result": v} for k, v in pres.items()]
    if not a.skip_frontend:
        fres, _ = frontend_group(m["frontend"])
        extra += [{"id": k, "runner": "vitest", "result": v} for k, v in fres.items()]
    for row in extra:
        if row["result"] != "passed":
            failed = True
    report["otherChecks"] = extra
    report["notReproducibleInCi"] = m["notReproducibleInCi"]
    allrows = [c for r in report["requirements"] for c in r["checks"]] + extra
    report["counts"] = {k: sum(1 for c in allrows if c["result"] == k) for k in ("passed", "failed", "missing", "ignored", "blocked_not_run", "not_requested")}
    report["allPassed"] = not failed and not blocked and report["counts"]["passed"] > 0
    text = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
    if a.report:
        Path(a.report).write_text(text, encoding="utf-8", newline="\n")
    print(json.dumps({"counts": report["counts"], "allPassed": report["allPassed"], "commit": report["commit"]}, indent=2))
    return 3 if blocked and not failed else (0 if report["allPassed"] else 1)


if __name__ == "__main__":
    sys.exit(main())
