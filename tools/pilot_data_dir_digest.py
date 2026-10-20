#!/usr/bin/env python3
r"""Prints the digest that identifies an isolated pilot data directory (Phase 3D-5), the value the one-installation allowance is armed for.

  python tools/pilot_data_dir_digest.py "C:\path\to\the\isolated\data-dir"

It reads nothing and touches nothing: it hashes the TEXT of the path you give, normalised exactly as the application does
(`pilot_allowance::data_dir_digest`): forward slashes become backslashes, lower case, no trailing separator. The application must later be started with
`ATM_DATA_DIR` set to that same directory; a different directory (or a copied database) cannot use the allowance. The path itself is never stored.
"""
import hashlib
import sys


def data_dir_digest(path: str) -> str:
    normalised = path.replace("/", "\\").lower().rstrip("\\")
    return hashlib.sha256(f"atm-pilot-data-dir-v1|{normalised}".encode("utf-8")).hexdigest()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    print(data_dir_digest(sys.argv[1]))
