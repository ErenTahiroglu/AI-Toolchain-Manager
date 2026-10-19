# Licence and notices: practical resolution for the first pilot (Phase 3D-2)

**jq 1.8.2 / Windows x64 stays `LICENSE_CONDITIONAL`. The recipe's `license.state` stays `unknown`** (the validator blocks the recipe on it; tested). All Phase 3D-1 findings are retained: [LICENSE-REVIEW](../phase-3d1/LICENSE-REVIEW-JQ-1.8.2-WINDOWS.md), [Q1–Q3 proposal](../phase-3d1/LICENSE-Q1-Q3-RESOLUTION-PROPOSAL.md). This is an engineering reading, not legal advice.

## Premise for the first pilot

The application does **not bundle jq**. jq is obtained from its upstream publisher for the user's own local use (for the first pilot: the retained, re-verified Request A file, which is *not* downloaded again). The application's own installer therefore carries no jq binary and no jq notices; the notices are shown **by the application**, next to the preview and in the installed-tool details.

## Decision: a manager-owned notice manifest, presented by the application — not a sidecar

| Option | Verdict | Why |
| --- | --- | --- |
| A notice file inside the version folder | **rejected** | The adapter owns exactly one file there. Tested (3D-1): a stray `NOTICE.txt` is *unexpected content* — it blocks repeat installs and uninstall and is never deleted or adopted. An owned-sidecar layout (second owned role, receipt entry, verified removal) would be a separate reviewed adapter change; it is not needed. |
| **A reviewed manager-owned notice manifest + bundle text, compiled into the application and presented by it** | **chosen** | No file is created in any tool folder; nothing for the adapter to mistake for foreign content; the text survives uninstall by construction (it belongs to the application, not to the tool folder). |
| Only a link to the licences | rejected | Not conservative: the Unicode and mingw-w64 licences ask that notices accompany distributions; a link is not the text. |

How it works (`install_flow.rs`, `NoticeBundle`):

* The notice bundle for a recipe is part of the **confirmed preview**: its title, components, unresolved points and SHA-256 are inside the preview digest. A different notice text is a different confirmation (tested).
* Its digest is written to the audit row of every install (`evidenceDigest`).
* After installation, and after removal, the full text is shown in the installed-tool details (tested: notices survive install → removal → reinstall with the same digest, no notice/licence file ever exists in the manager folder).
* Tested with **synthetic fixtures only** (`install_flow_tests.rs`): preservation, idempotency (reinstall presents the identical digest), uninstall not blocked, no sidecar file, binding into the confirmation.

**Effect on condition C2 of the Phase 3D-1 review.** C2 asked for the notices "inside the application **and** stored next to the installed version as a manager-written text file". This decision keeps the first half and **replaces the second half** (the file next to the version) by the digest-bound, application-presented manifest above. That is a proposed amendment of C2; it is **not** in force until the Red Team accepts it in writing.

## The package

[`docs/phase-3d2/notices/`](notices/): `THIRD-PARTY-NOTICES.jq-1.8.2.windows-x64.txt` (verbatim primary texts, each section naming its source and SHA-256) and `jq-1.8.2-windows-x64.notices-manifest.json` (components, evidence hashes, link state, legal status, open points). It contains: **jq** (MIT, dtoa, decNumber under the ICU licence, Heimdal-derived code, NetBSD strptime), **Oniguruma** (BSD-2-Clause), **mingw-w64** (ZPL-2.1 text and the *complete* runtime notice file), **winpthreads**, the **GCC Runtime Library Exception 3.1**, and the **Unicode License V3**. `tests/test_phase3d2_contract.py` re-checks every evidence hash and that each verbatim text is contained in the bundle.

## Unresolved legal facts — kept explicit, not hidden (they also appear in the preview of a recipe that carries this bundle)

| # | Open point | State |
| --- | --- | --- |
| Q1 | Do Oniguruma's compiled Unicode 16.0 tables count as "Data Files or Software" under Unicode License V3? | notice included conservatively; legal reading **open** |
| Q2 | Which mingw-w64 runtime parts (getopt, gdtoa, Wine-derived) are linked? Executable is stripped. | complete notice file included; exact subset **unknown, not claimed** |
| Q3 | Is libgcc proper linked; does the upstream build meet every detail of the GCC Runtime Library Exception? (libssp **is** linked.) | exception text and note included; legal reading **open** |
| C1–C5 | The conditions of the licence review (no hosting/mirroring/bundling; notices shown; …) must be accepted in writing | **not accepted** |
| SPDX | The draft recipe says `MIT`; the bundle needs a composite expression (the transition test uses `MIT AND BSD-2-Clause AND Unicode-3.0`, ≤ 40 characters) | to be settled in the licence review |

Nothing here sets `license.state` to `accepted`. That happens only with a written `LICENSE_PASS` that names C1–C5 and Q1–Q3, recorded as a RESEARCH-SOURCES entry (`R…`, the validator requires it).
