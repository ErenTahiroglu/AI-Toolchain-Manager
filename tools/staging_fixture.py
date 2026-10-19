#!/usr/bin/env python3
"""The staging fixture of the jq pilot review (Phase 3D-3B), reproduced WITHOUT the application's code.

The fixture is a made-up stand-in for the executable: a minimal native header for the platform followed by a fixed ASCII payload. It is not jq,
not a program, and is never executed. This script rebuilds it byte for byte from the format description in
`src-tauri/src/discovery/reference.rs::synthetic_file`, so a reviewer can compare its SHA-256 with the one the Rust tests pin
(`docs/phase-3d3b/staging-fixture.json`) without trusting the Rust implementation.

  python tools/staging_fixture.py print               # JSON with size and SHA-256 for both platforms
  python tools/staging_fixture.py write <new-dir>     # write the two fixture files into a NEW, empty directory (never overwrites)

Standard library only; no network; no program is started.
"""
import hashlib
import json
import sys
from pathlib import Path

PAYLOAD = b"JQ-PILOT-STAGING-FIXTURE-1: a synthetic stand-in; it is not jq and not a program."
FILES = {"windows-x64": "jq-windows-amd64.exe", "macos-arm64": "jq-macos-arm64"}


def fixture(platform: str) -> bytes:
    if platform == "windows-x64":
        h = bytearray(0x80 + 24)
        h[0:2] = b"MZ"
        h[0x3C:0x40] = (0x80).to_bytes(4, "little")
        h[0x80:0x84] = b"PE\0\0"
        h[0x84:0x86] = (0x8664).to_bytes(2, "little")
        return bytes(h) + PAYLOAD
    if platform == "macos-arm64":
        h = bytes([0xCF, 0xFA, 0xED, 0xFE]) + (0x0100000C).to_bytes(4, "little") + bytes(24)
        return h + PAYLOAD
    raise SystemExit(f"unknown platform {platform}")


def describe() -> dict:
    out = {"payloadAscii": PAYLOAD.decode("ascii"), "payloadSha256": hashlib.sha256(PAYLOAD).hexdigest(), "platforms": {}}
    for p, name in FILES.items():
        b = fixture(p)
        out["platforms"][p] = {"fileName": name, "sizeBytes": len(b), "sha256": hashlib.sha256(b).hexdigest()}
    return out


def main(argv):
    if argv[:1] == ["print"]:
        print(json.dumps(describe(), indent=2))
    elif argv[:1] == ["write"] and len(argv) == 2:
        d = Path(argv[1])
        d.mkdir(parents=True, exist_ok=False)
        for p, name in FILES.items():
            with open(d / name, "xb") as f:
                f.write(fixture(p))
        print(f"written to {d}")
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
