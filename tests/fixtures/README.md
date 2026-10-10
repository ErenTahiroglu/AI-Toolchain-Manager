# Synthetic Contract Fixtures

Every identifier, digest, commit, key, approval event, and package in these fixtures is synthetic. The fixture recipe is never executable and does not refer to a real release. These files test schema and semantic guards only; they are not compatibility evidence, approval records, or installation instructions.

Negative cases are built from safe fixtures in `tests/test_phase0_contracts.py` or stored in `audit-passed-without-evidence.synthetic.json` and `sync-with-machine-path.synthetic.json`. Tests must never install, fetch, or execute a fixture artifact.
