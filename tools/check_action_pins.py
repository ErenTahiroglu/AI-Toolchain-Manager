#!/usr/bin/env python3
"""Static guard: every GitHub Actions `uses:` reference must be an approved, exact, immutable pin (CI-HARDEN-02, Issue #48).

What it does (standard library only, offline, read-only):
  * discovers every `*.yml` / `*.yaml` file (case-insensitive) in `.github/workflows`, including future additions;
  * finds every `uses:` key, step level or job level, and requires the value to be exactly `owner/repo@<40 lowercase hex>` where the owner/repo identity and
    that SHA are listed together in `tools/action_pins_allowlist.json`;
  * fails (exit status 1) on tags, branches, short/long/uppercase/non-hex SHAs, unknown actions, a SHA approved for a different action, dynamic expressions,
    local (`./`) actions, `docker://` images, reusable-workflow paths and anything else that is not an approved identity+SHA;
  * fails (exit status 1) on workflow syntax it cannot classify safely instead of guessing (see below);
  * exits with status 2 when its own configuration (the allowlist) is unusable.

It is NOT a YAML parser. Python has no standard-library YAML parser, so this is a deliberately narrow, line-based reader of the syntax the repository's
workflows use today: block mappings and sequences indented with spaces, plain or single-line quoted scalars, single-line flow lists of plain words and
block scalars (`|`, `>` with an optional `+`/`-`). Text inside block scalars (shell scripts) and comments is skipped, so ordinary text containing "uses:" is not a
dependency. These are rejected as unsupported, because they could hide or alter a `uses` value: multi-document markers and directives, tab indentation,
anchors, aliases, merge keys, tags, complex keys, quoted keys, flow mappings, flow lists containing `:` or quotes, multi-line plain/quoted scalars,
block-scalar indentation indicators, a `uses` key in any spelling other than the lowercase one, a file without a top-level `jobs:`, a BOM, NUL or a lone CR.

Limits (read before trusting a green result):
  * This is a regression and CI quality gate, not tamper-proof protection. Anyone who can change this file, the allowlist and the workflow in the same commit
    can defeat it. Changes to those three need independent (non-author) review.
  * It runs inside a job after the workflow's own Actions were already resolved by GitHub; it cannot prevent the current run from using a bad reference.
  * It checks identity and SHA text only. It does not prove the pinned upstream commit is safe, correctly attributed or still maintained; approving or
    updating a pin needs separate review with upstream provenance.
  * Only `.github/workflows` is scanned. Composite actions elsewhere are not read (local `./` references are rejected, so none is approved).
  * GitHub's own repository-level "require full-length SHA pinning" setting is a separate owner/administrator decision and is not changed by this tool.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Dict, List, NamedTuple, Optional, Tuple

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ALLOWLIST = Path(__file__).resolve().with_name("action_pins_allowlist.json")
WORKFLOW_DIR = ".github/workflows"

IDENTITY = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
SHA40 = re.compile(r"^[0-9a-f]{40}$")
BLOCK_SCALAR = re.compile(r"^[|>][+-]?$")
KEY_SPLIT = re.compile(r"^(.*?):(?: +|$)(.*)$")
FLOW_LIST_BODY = re.compile(r"^\[[^\[\]{}:'\"#&*!|>%@`]*\]$")
SPACE_HASH = re.compile(r"\s#")


class Violation(NamedTuple):
    file: str
    line: int
    message: str

    def render(self) -> str:
        return f"{self.file}:{self.line}: {self.message}"


class AllowlistError(Exception):
    pass


# ---- allowlist ------------------------------------------------------------------------------------------------------------------------------


def _no_duplicate_keys(pairs):
    keys = [k for k, _ in pairs]
    if len(keys) != len(set(keys)):
        raise AllowlistError("duplicate key in allowlist JSON")
    return dict(pairs)


def load_allowlist(path: Path) -> Tuple[Dict[str, str], List[str]]:
    """Returns (approved identity -> sha, required workflow file names). Raises AllowlistError for anything unexpected."""
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"), object_pairs_hook=_no_duplicate_keys)
    except (OSError, ValueError) as exc:
        raise AllowlistError(f"cannot read allowlist {path}: {exc}") from exc
    if not isinstance(data, dict) or set(data) != {"schema_version", "approved_actions", "required_workflows"}:
        raise AllowlistError("allowlist must be an object with exactly: schema_version, approved_actions, required_workflows")
    if data["schema_version"] != 1:
        raise AllowlistError("unsupported allowlist schema_version")
    approved = data["approved_actions"]
    if not isinstance(approved, dict) or not approved:
        raise AllowlistError("approved_actions must be a non-empty object")
    for identity, sha in approved.items():
        if not isinstance(identity, str) or not IDENTITY.match(identity):
            raise AllowlistError(f"approved identity is not owner/repo: {identity!r}")
        if not isinstance(sha, str) or not SHA40.match(sha):
            raise AllowlistError(f"approved SHA for {identity} is not 40 lowercase hex characters")
    required = data["required_workflows"]
    if not isinstance(required, list) or not required or not all(isinstance(r, str) and "/" not in r and r.lower().endswith((".yml", ".yaml")) for r in required):
        raise AllowlistError("required_workflows must be a non-empty list of workflow file names")
    if len(set(required)) != len(required):
        raise AllowlistError("duplicate entry in required_workflows")
    return dict(approved), list(required)


# ---- reference validation ---------------------------------------------------------------------------------------------------------------------


def validate_reference(value: str, approved: Dict[str, str]) -> Optional[str]:
    """None when `value` is an approved immutable pin, otherwise the reason it is not."""
    if "${{" in value or "}}" in value or "$" in value:
        return "dynamic expression in `uses` is not allowed"
    if value == "." or value.startswith(("./", "../")):
        return "local action reference is not approved"
    if value.lower().startswith("docker://"):
        return "docker:// reference is not approved"
    if "@" not in value:
        return "reference has no @<sha> part"
    identity, _, ref = value.partition("@")
    if identity.count("/") != 1 or not IDENTITY.match(identity):
        return "reusable-workflow, path or malformed action identity is not approved (expected owner/repo)"
    if not SHA40.match(ref):
        if re.fullmatch(r"[0-9A-F]{40}", ref) or (re.fullmatch(r"[0-9a-fA-F]{40}", ref) and ref != ref.lower()):
            return "SHA must be lowercase (uppercase or mixed-case hex is rejected)"
        if re.fullmatch(r"[0-9a-fA-F]{7,39}", ref):
            return "abbreviated SHA is not allowed (need the full 40 hex characters)"
        if re.fullmatch(r"[0-9a-fA-F]{41,}", ref):
            return "SHA is longer than 40 hex characters"
        return "mutable ref (tag, branch or malformed SHA) is not allowed; pin a full commit SHA"
    if identity not in approved:
        other = [a for a, s in approved.items() if s == ref]
        hint = f" (that SHA is approved only for {other[0]})" if other else ""
        return f"action {identity} is not in the approved list{hint}"
    if approved[identity] != ref:
        return f"SHA is well-formed but is not the approved SHA for {identity}"
    return None


# ---- narrow workflow reader -------------------------------------------------------------------------------------------------------------------


def _indent(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def _is_skippable(line: str) -> bool:
    s = line.strip()
    return s == "" or line.lstrip(" ").startswith("#")


def _quoted_end(v: str) -> int:
    """Index of the closing quote of the quoted scalar that starts at v[0], or -1 when it is not closed on this line."""
    quote = v[0]
    i = 1
    while i < len(v):
        if quote == '"' and v[i] == "\\":
            i += 2
            continue
        if v[i] == quote:
            if quote == "'" and v[i + 1 : i + 2] == "'":
                i += 2
                continue
            return i
        i += 1
    return -1


def _split_value(raw: str) -> Tuple[str, Optional[str]]:
    """Value text without a trailing comment, or (text, error)."""
    v = raw.strip()
    if v and v[0] in "\"'":
        end = _quoted_end(v)
        if end < 0:
            return v, "quoted scalar is not closed on the same line (multi-line scalars are unsupported)"
        tail = v[end + 1 :]
        if tail.strip() and not tail.lstrip().startswith("#"):
            return v, "unexpected text after a quoted scalar"
        return v[: end + 1], None
    m = SPACE_HASH.search(v)
    return (v[: m.start()] if m else v).rstrip(), None


def scan_workflow_text(text: str, name: str) -> Tuple[List[Violation], List[Tuple[int, str]]]:
    """Returns (violations, [(line, uses value)]). Unsupported syntax is a violation, never silently accepted."""
    violations: List[Violation] = []
    refs: List[Tuple[int, str]] = []

    def bad(line: int, msg: str) -> None:
        violations.append(Violation(name, line, msg))

    if text.startswith("﻿"):
        bad(1, "byte order mark is unsupported")
    if "\x00" in text or re.search(r"\r(?!\n)", text):
        bad(1, "NUL or a lone carriage return is unsupported")
    lines = text.replace("\r\n", "\n").split("\n")
    n = len(lines)
    saw_jobs = False

    def next_significant(j: int) -> Optional[int]:
        while j < n and _is_skippable(lines[j]):
            j += 1
        return j if j < n else None

    i = 0
    while i < n:
        raw = lines[i]
        lineno = i + 1
        if _is_skippable(raw):
            i += 1
            continue
        stripped = raw.lstrip(" ")
        if stripped[0] == "\t":
            bad(lineno, "tab indentation is unsupported")
            i += 1
            continue
        indent = len(raw) - len(stripped)
        if indent == 0 and (stripped.startswith(("---", "...", "%"))):
            bad(lineno, "document marker or directive is unsupported (single-document workflows only)")
            i += 1
            continue

        body = stripped
        col = indent
        dash = False
        if body == "-" or body.startswith("- "):
            dash = True
            rest = body[1:]
            body = rest.lstrip(" ")
            col = indent + 1 + (len(rest) - len(body))
            if body == "" or body.startswith("#"):
                i += 1
                continue
            if body == "-" or body.startswith("- "):
                bad(lineno, "nested sequence on one line is unsupported")
                i += 1
                continue

        first = body[0]
        if first in "&*!?%@`" or body.startswith("<<"):
            bad(lineno, f"unsupported YAML construct starting with {body[:2]!r} (anchor, alias, tag, merge or complex key)")
            i += 1
            continue
        if first == "{":
            bad(lineno, "flow mapping is unsupported")
            i += 1
            continue
        if first in "\"'":
            end = _quoted_end(body)
            if end >= 0 and re.match(r"^\s*:(?: |$)", body[end + 1 :]):
                bad(lineno, "quoted key is unsupported")
                i += 1
                continue
            value, err = _split_value(body)
            if err:
                bad(lineno, err)
            elif not dash:
                bad(lineno, "bare quoted scalar outside a sequence item is unsupported")
            else:
                j = next_significant(i + 1)
                if j is not None and _indent(lines[j]) > col:
                    bad(j + 1, "continuation of a multi-line scalar is unsupported")
            i += 1
            continue

        m = KEY_SPLIT.match(body)
        if not m:
            if not dash:
                bad(lineno, "line is neither a `key: value` pair nor a sequence item (multi-line scalar?) and is unsupported")
            else:
                value, err = _split_value(body)
                if err:
                    bad(lineno, err)
                j = next_significant(i + 1)
                if j is not None and _indent(lines[j]) > col:
                    bad(j + 1, "continuation of a multi-line scalar is unsupported")
            i += 1
            continue

        key = m.group(1).rstrip()
        value_raw = m.group(2)
        if key == "":
            bad(lineno, "empty key is unsupported")
            i += 1
            continue
        if key[0] in "&*!?%@`\"'{[" or key.startswith("<<"):
            bad(lineno, f"unsupported key syntax {key[:2]!r}")
            i += 1
            continue
        is_uses = key == "uses"
        if key.lower() == "uses" and not is_uses:
            bad(lineno, f"`uses` spelled {key!r} is unsupported (only the lowercase key is read)")
            i += 1
            continue
        if indent == 0 and not dash and key == "jobs":
            saw_jobs = True

        value, err = _split_value(value_raw)
        if err:
            bad(lineno, err)
            i += 1
            continue

        if value == "":
            if is_uses:
                bad(lineno, "`uses` has no value on its line (nested or multi-line values are unsupported)")
            i += 1
            continue

        if BLOCK_SCALAR.match(value):
            if is_uses:
                bad(lineno, "`uses` as a block scalar is unsupported")
            j = i + 1
            while j < n and (lines[j].strip() == "" or _indent(lines[j]) > col):
                j += 1
            i = j
            continue
        if value[0] in "|>":
            bad(lineno, "block scalar with an indentation indicator or unknown header is unsupported")
            i += 1
            continue
        if value[0] in "&*!%@`" or value[0] == "{":
            bad(lineno, f"unsupported value syntax {value[:2]!r} (anchor, alias, tag, flow mapping or reserved indicator)")
            i += 1
            continue
        if value[0] == "[":
            if is_uses:
                bad(lineno, "`uses` as a flow list is unsupported")
            elif not FLOW_LIST_BODY.match(value):
                bad(lineno, "flow list must be a single line of plain words (no ':', quotes, braces or nesting)")
        elif value[0] in "\"'":
            if is_uses:
                bad(lineno, "quoted `uses` value is unsupported (write the reference unquoted)")
        elif is_uses:
            refs.append((lineno, value))

        j = next_significant(i + 1)
        if j is not None and _indent(lines[j]) > col:
            bad(j + 1, "continuation of a multi-line scalar is unsupported")
        i += 1

    if not saw_jobs:
        bad(1, "no top-level `jobs:` mapping was found (empty, comment-only or unsupported workflow)")
    return violations, refs


# ---- repository scan --------------------------------------------------------------------------------------------------------------------------


def check_repository(root: Path, allowlist_path: Optional[Path] = None) -> Tuple[List[Violation], Dict[str, int]]:
    root = Path(root)
    approved, required = load_allowlist(Path(allowlist_path) if allowlist_path else DEFAULT_ALLOWLIST)
    violations: List[Violation] = []
    summary = {"files": 0, "uses": 0}
    wf_dir = root / WORKFLOW_DIR
    if wf_dir.is_symlink() or not wf_dir.is_dir():
        return [Violation(WORKFLOW_DIR, 0, "workflow directory is missing or is a symlink")], summary
    files = sorted(p for p in wf_dir.iterdir() if p.suffix.lower() in (".yml", ".yaml") and not p.is_dir())
    if not files:
        violations.append(Violation(WORKFLOW_DIR, 0, "empty workflow inventory (no .yml/.yaml files)"))
    present = {p.name for p in files}
    for req in required:
        if req not in present:
            violations.append(Violation(f"{WORKFLOW_DIR}/{req}", 0, "required workflow file is missing"))
    for path in files:
        rel = f"{WORKFLOW_DIR}/{path.name}"
        if path.is_symlink():
            violations.append(Violation(rel, 0, "workflow file is a symlink"))
            continue
        try:
            text = path.read_bytes().decode("utf-8")
        except (OSError, UnicodeDecodeError) as exc:
            violations.append(Violation(rel, 0, f"cannot read as UTF-8: {type(exc).__name__}"))
            continue
        summary["files"] += 1
        found, refs = scan_workflow_text(text, rel)
        violations.extend(found)
        for line, value in refs:
            summary["uses"] += 1
            reason = validate_reference(value, approved)
            if reason:
                shown = value if len(value) <= 100 else value[:97] + "..."
                violations.append(Violation(rel, line, f"{reason}: uses: {shown}"))
    return violations, summary


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Verify that every GitHub Actions `uses:` reference is an approved immutable pin.")
    parser.add_argument("--root", default=str(REPO_ROOT), help="repository root (default: this checkout)")
    parser.add_argument("--allowlist", default=str(DEFAULT_ALLOWLIST), help="approved identity -> SHA JSON file")
    args = parser.parse_args(argv)
    try:
        violations, summary = check_repository(Path(args.root), Path(args.allowlist))
    except AllowlistError as exc:
        print(f"ACTION_PINS_CONFIG_ERROR {exc}", file=sys.stderr)
        return 2
    for v in violations:
        print(v.render())
    if violations:
        print(f"ACTION_PINS_RESULT FAIL violations={len(violations)} files={summary['files']} uses={summary['uses']}")
        return 1
    print(f"ACTION_PINS_RESULT PASS files={summary['files']} uses={summary['uses']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
