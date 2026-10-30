# LICENSE-01 — PolyForm Noncommercial License 1.0.0: provenance, scope and limits

Issue #41. Author handoff, **not** a legal opinion and **not** an independent acceptance. The owner instructed that the project be licensed under PolyForm Noncommercial License 1.0.0; this change implements that instruction on `development` only.

## 1. What was added

| Path | Content |
|---|---|
| `LICENSE.md` | line 1: `Required Notice: Copyright (c) 2026 Eren Tahiroğlu`; a `---` separator; then the **complete, unmodified** official license text (byte-identical to the upstream file, see §2) |
| `package.json` | `"license": "PolyForm-Noncommercial-1.0.0"` (the project's own manifest only) |
| `package-lock.json` | the same line in the root package entry `packages[""]`, produced by `npm install --package-lock-only --offline --ignore-scripts` (one added line, nothing else) |
| `src-tauri/Cargo.toml` | `license = "PolyForm-Noncommercial-1.0.0"` in `[package]` (`publish = false` unchanged; `Cargo.lock` unchanged) |
| `README.md` | a "Lisans" section linking `LICENSE.md`, stating source-available / not OSI-approved / no commercial-use grant / third-party terms unchanged |
| `tests/test_license01_contract.py` | targeted checks only (§5) |

Nothing else was changed: no source header, no third-party package metadata, no dependency, no bundle metadata (`tauri.conf.json` carries no license or copyright field and was left alone), no catalog entry.

## 2. Upstream text — exact provenance

* Repository: `polyformproject/polyform-licenses`; file `PolyForm-Noncommercial-1.0.0.md`.
* Reference used: `1.0.0`. **`1.0.0` is a branch, not a tag** (`refs/heads/1.0.0`; the tag lookup returned 404). On 2026-10-30 it resolved to commit `76a278c402bc43b8d2b561da140b0f3e17263015` (committed 2023-11-20, message "Add Perimeter 1.0.1").
* The file's git blob at that commit is `5ecc88cfc4b1cff608ed640efe913c9dd97935c3` (4,563 bytes, LF). SHA-256 of those bytes: `c0ea4a896d2c8c394b29f9427589996db826cd501c512279ff0ed3ef48fabbe5`. The fetched bytes were hashed as a git blob and matched `5ecc88cf…` before `LICENSE.md` was written.
* Cross-check: the text served at <https://polyformproject.org/licenses/noncommercial/1.0.0> was compared word for word with the upstream file from the word "Acceptance" to the end: 681 of 681 words equal, no mismatch (formatting-insensitive comparison; the site is HTML, so this is a text-equivalence check, not a byte check).
* Because a branch can move, the immutable identifiers are the commit and blob hashes above; `tests/test_license01_contract.py` pins the blob hash and recomputes it from `LICENSE.md` without network access.

## 3. Ownership and third-party checks

* **Git history** (up to `75fc20e`, 82 commits): one human author name (Eren Tahiroğlu) under three e-mail identities (a personal address, a GitHub no-reply address and a machine-local one). No other human author or contributor appears.
* **AI-assisted commits:** 76 commits carry a `Co-Authored-By: Claude Sonnet 5.5` trailer. This is recorded as a fact. Who owns the copyright in AI-assisted output, and whether that affects the owner's licensor position, is a legal question that **was not assessed and is not decided here**; it is flagged for the owner's own judgement.
* **Vendored code:** no vendored-source directory exists. Tracked third-party material is documentation only: research notes about third-party tools (`docs/RESEARCH-SOURCES.md`, catalog evidence) and the draft/final jq third-party notice texts (`docs/phase-3d1/…DRAFT.txt`, `docs/phase-3d2/notices/…`). They are preserved unchanged and keep their own terms. This was a search of file names and license/copyright strings, **not** a line-by-line code-provenance audit.
* **Dependencies** (npm, crates, Tauri, React, Vite, SQLite via `rusqlite`, …) keep their own licenses; their package metadata was not touched. A dependency-license compatibility review (including whether any dependency's terms constrain a noncommercial relicensing of the combined work) was **not performed**.
* **Catalogued tools** (Exa, Claude Code, Playwright, jq, …): the catalog describes them; it neither licenses nor relicenses them.
* No conflicting copyright holder was found, so no stop condition was triggered.

## 4. Metadata acceptance (observed, Windows developer machine)

| Tool | Result |
|---|---|
| `npm pkg get license` | `"PolyForm-Noncommercial-1.0.0"` |
| `npm install --package-lock-only --offline --ignore-scripts` | exit 0, no warning about the identifier; changed `package-lock.json` by exactly one line |
| `cargo metadata --no-deps` | `license = PolyForm-Noncommercial-1.0.0`, `license_file = None`, `publish = []`; `Cargo.lock` unchanged |

No metadata format rejected the identifier, so no alternative representation was needed. `PolyForm-Noncommercial-1.0.0` is the SPDX identifier published for this license; this was not re-verified against the SPDX list in this change.

## 5. What the contract test checks (and does not)

`tests/test_license01_contract.py`: `LICENSE.md` contains the official text byte-for-byte (git-blob hash `5ecc88cf…`) after the Required Notice; the Required Notice line is exactly `Required Notice: Copyright (c) 2026 Eren Tahiroğlu` and sits outside the official block; `package.json`, the lockfile root entry and `Cargo.toml` carry exactly the identifier; the README links `LICENSE.md` and keeps the three statements (source-available / not OSI-approved / no commercial-use grant). It does **not** judge legal sufficiency, ownership, dependency compatibility or what "noncommercial" covers.

## 6. Branch caveat — the default branch is NOT licensed by this change

As of this commit `main` (frozen at `68eda3f0407490fabccb517dcf822ab4818fc0bf`) has **no** `LICENSE.md`, and `main` is the repository's default branch (the repository is public; on 2026-10-30 `gh repo view` reported `licenseInfo: null`). GitHub's license detection and anyone landing on the default branch therefore still see an unlicensed repository. Until a **separately authorized and independently reviewed** promotion (or a narrow license-only update of `main`) happens, the public default branch must not be described as licensed. This change did not touch `main`, did not merge, and did not open a pull request.

## 7. Wording limits

PolyForm Noncommercial is a **source-available** license, not an OSI-approved open-source license. It grants rights for the noncommercial purposes defined in its own text and no commercial-use right. The README and this note do not summarise or interpret its terms beyond those three facts; `LICENSE.md` is the only authority.

## 8. Not done

No relicensing of any third party; no legal review; no change to `main`; no CONTRIBUTING/CLA process for future contributors (a future contributor's rights are not addressed here); no SPDX headers in source files; no change to any dependency or catalog license field.
