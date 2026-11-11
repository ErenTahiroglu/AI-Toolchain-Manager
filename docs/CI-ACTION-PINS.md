# GitHub Actions pin guard (CI-HARDEN-02, Issue #48)

`tools/check_action_pins.py` reads every `.yml` / `.yaml` file in `.github/workflows` and fails unless each `uses:` reference is exactly
`owner/repo@<40 lowercase hex>` and that owner/repo and SHA are listed together in `tools/action_pins_allowlist.json`. The CI `desktop` job (check
names `windows-x64` and `macos-arm64`) runs it as the step "Verify immutable GitHub Actions pins", right after `actions/setup-python`; regression tests are in
`tests/test_ci_action_pins.py` and run with the normal `unittest discover`.

Run locally: `python tools/check_action_pins.py` (exit 0 = pass, 1 = violation, 2 = unusable allowlist). No network, no installs, read-only.

## What it rejects
Version tags, branches, short/long/uppercase/non-hex SHAs, unknown actions or owners, a SHA approved for a different action, dynamic `${{ }}` references,
local `./` actions, `docker://` images, reusable-workflow paths, a missing required workflow, an empty inventory, and workflow syntax it cannot classify
(anchors, aliases, merge keys, tags, quoted keys, flow mappings, multi-line scalars, tabs, document markers, a `uses` key spelled in another case, a file with no
top-level `jobs:`). It is a narrow line-based reader, not a YAML parser: unsupported syntax fails the check instead of being guessed at.

## Limits (do not read a green result as more than this)
- **Regression gate, not tamper-proof.** Whoever can change the checker, the allowlist and the workflow in the same commit can defeat it. Changes to
  `tools/check_action_pins.py`, `tools/action_pins_allowlist.json`, `tests/test_ci_action_pins.py` or the guard step in `ci.yml` need independent (non-author)
  review.
- **It runs after the workflow's actions were resolved.** It cannot stop the current run from using a bad reference; it stops one from being merged unnoticed.
- **Identity and SHA text only.** It does not show the pinned commit is safe, correctly attributed or maintained. Approving or updating a pin is a separate,
  reviewed change with upstream provenance (see Issues #46, #49).
- **Scope:** only `.github/workflows` is scanned.
- GitHub's own repository setting that requires full-length SHA pinning is a separate owner/administrator decision and is not changed by this tool.
