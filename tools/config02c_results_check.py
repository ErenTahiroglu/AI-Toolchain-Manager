#!/usr/bin/env python3
"""CONFIG-02C result gate (standard library only; reads text files, writes only the summary file you name).

Reads the `CONFIG02C_RESULT|<id>|<os>|<PASS|FAIL|SKIPPED|BLOCKED>|<reason>` lines printed by the Rust and vitest cases, compares them with the
manifest `tests/config02c_expected_results.json`, prints an id x status table for one operating system and ONE summary line
`CONFIG02C_RESULTS|<os>|PASS=n|SKIPPED=n|FAIL=n|BLOCKED=n|NOT_RUN=n`, and exits non-zero unless every id has exactly one acceptable line:

* no line for an id            -> NOT RUN (fails the gate; nothing is ever inferred from a passing test function)
* FAIL or BLOCKED              -> fails the gate
* SKIPPED                      -> passes the gate only with a reason the manifest lists for that id and operating system; it is counted as SKIPPED, never as PASS
* two different lines for an id -> fails the gate
* a line for an id the manifest does not know, or for another operating system -> fails the gate
Reasons are fixed short codes; the lines contain no path.
"""
import argparse
import json
import pathlib
import re
import sys

LINE = re.compile(r"CONFIG02C_RESULT\|([A-Za-z0-9]+)\|([a-z]+)\|(PASS|FAIL|SKIPPED|BLOCKED)\|([A-Za-z0-9_]*)")


def parse(texts):
    found = {}
    problems = []
    for text in texts:
        for m in LINE.finditer(text):
            ident, os_name, status, reason = m.groups()
            found.setdefault(ident, []).append((os_name, status, reason))
    return found, problems


def judge(manifest, found, os_name):
    rows = []
    problems = []
    counts = {"PASS": 0, "SKIPPED": 0, "FAIL": 0, "BLOCKED": 0, "NOT_RUN": 0}
    known = manifest["ids"]
    for ident in sorted(found):
        if ident not in known:
            problems.append(f"unknown id {ident}")
    for ident, spec in known.items():
        lines = found.get(ident, [])
        other = [x for x in lines if x[0] != os_name]
        mine = sorted(set(x for x in lines if x[0] == os_name))
        if other:
            problems.append(f"{ident}: a line for another operating system in the {os_name} results")
        if not mine:
            rows.append((ident, spec["suite"], "NOT RUN", ""))
            counts["NOT_RUN"] += 1
            problems.append(f"{ident}: NOT RUN (no result line for {os_name})")
            continue
        if len(mine) > 1:
            rows.append((ident, spec["suite"], "CONFLICT", ";".join(f"{s}:{r}" for _, s, r in mine)))
            counts["FAIL"] += 1
            problems.append(f"{ident}: conflicting lines {mine}")
            continue
        _, status, reason = mine[0]
        if status == "PASS":
            counts["PASS"] += 1
        elif status == "SKIPPED":
            counts["SKIPPED"] += 1
            if reason not in spec.get("skip_allowed", {}).get(os_name, []):
                problems.append(f"{ident}: SKIPPED with a reason the manifest does not allow on {os_name}: {reason!r}")
        elif status == "FAIL":
            counts["FAIL"] += 1
            problems.append(f"{ident}: FAIL")
        else:
            counts["BLOCKED"] += 1
            problems.append(f"{ident}: BLOCKED")
        rows.append((ident, spec["suite"], status, reason))
    return rows, counts, problems


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--os", required=True, choices=["windows", "macos"])
    ap.add_argument("--manifest", required=True)
    ap.add_argument("--results", required=True, nargs="+", help="files holding result lines (a log is fine; other text is ignored)")
    ap.add_argument("--summary", help="write the table and the summary line to this file")
    args = ap.parse_args(argv)
    manifest = json.loads(pathlib.Path(args.manifest).read_text(encoding="utf-8"))
    texts = []
    for name in args.results:
        path = pathlib.Path(name)
        texts.append(path.read_text(encoding="utf-8", errors="replace") if path.exists() else "")
    found, _ = parse(texts)
    rows, counts, problems = judge(manifest, found, args.os)
    out = [f"CONFIG-02C result gate - {args.os}", "id | suite | status | reason"]
    out += [f"{i} | {s} | {st} | {r}" for i, s, st, r in rows]
    out.append("Not collected here: " + "; ".join(f"{k} ({v})" for k, v in manifest.get("not_collected", {}).items()))
    out.append("CONFIG02C_RESULTS|%s|PASS=%d|SKIPPED=%d|FAIL=%d|BLOCKED=%d|NOT_RUN=%d" % (args.os, counts["PASS"], counts["SKIPPED"], counts["FAIL"], counts["BLOCKED"], counts["NOT_RUN"]))
    out += [f"GATE PROBLEM: {p}" for p in problems]
    out.append("GATE " + ("FAILED" if problems else "OK"))
    text = "\n".join(out)
    print(text)
    if args.summary:
        pathlib.Path(args.summary).write_text(text + "\n", encoding="utf-8", newline="\n")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
