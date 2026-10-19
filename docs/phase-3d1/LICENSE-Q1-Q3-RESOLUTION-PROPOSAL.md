# Proposal for resolving the open licence questions Q1–Q3 (Phase 3D-1 closure)

**jq 1.8.2 / Windows x64 stays `LICENSE_CONDITIONAL`.** This is a short, evidence-based *proposal* to the Red Team; it resolves nothing by itself, accepts nothing, and the recipe's `license.state` stays `unknown` (the validator blocks on it, tested). It is an engineering reading of primary texts, not legal advice.

## New evidence gathered for this closure (all read-only; GitHub API only behind `tools/egress_guard.py`, 0 incidents; one page on `unicode.org` and the Microsoft pages read through the browsing tool)

| Evidence | Result | File |
| --- | --- | --- |
| Heads of the Oniguruma Unicode data files at the pinned commit | `unicode_egcb_data.c` declares `GRAPHEME_BREAK_PROPERTY_VERSION 160000` (Unicode **16.0**); the files carry only K. Kosako's BSD notice, no Unicode notice | `evidence/license/source-excerpts-3.txt` |
| Unicode licence text | **Unicode License V3** (SPDX `Unicode-3.0`): permission to deal in the *Data Files or Software*, "provided that either (a) this copyright and permission notice appear with all copies of the Data Files or Software, or (b) … appear in associated Documentation"; non-endorsement clause. Verbatim section stored with its source commit; cross-read against `unicode.org/license.txt` | `evidence/license/unicode-license-v3.txt` |
| Marker search in the retained executable | `*** stack smashing detected ***: terminated` is present: **libssp is statically linked** (jq is built with `-fstack-protector-all`); winpthreads and the mingw-w64 CRT strings are present; no marker for `getopt`, `gdtoa`, `__mingw_` or Wine-derived code was found; libgcc proper neither confirmed nor excluded. The executable is stripped, so absence is not proof | `evidence/license/pe-static-addendum.json` |

## Q1 — Oniguruma generated Unicode tables

* **What is established:** the compiled-in property/fold/segmentation tables are generated from Unicode 16.0 data (`make_unicode_*` scripts named in the file heads); Oniguruma's own notice (BSD-2-clause) is the only notice in those files; the Unicode licence requires its notice to appear with copies of the Data Files **or Software**, or in the documentation.
* **What is not established:** whether compiled tables count as "copies of the Data Files or Software" for that clause, and whether Oniguruma's upstream considers it covered.
* **Proposal (conservative, cheap):** treat the Unicode licence as applicable. Include the verbatim Unicode License V3 in the notice bundle (done in the draft). Optionally ask Oniguruma upstream whether the notice belongs in its `COPYING`. **Status after this proposal:** resolved *operationally* (the notice is included); *legally* unresolved until accepted by the Red Team or counsel.

## Q2 — which mingw-w64 runtime notices apply

* **What is established:** the executable statically contains the mingw-w64 CRT and winpthreads. The runtime's own notice file says binaries "statically linked against the MinGW-w64 runtime" need the listed notices when distributed in binary form. It lists: the overall (ZPL-style) notice, `getopt`/`getopt_long` (Todd C. Miller), `gdtoa` (David Gay), parts of the math library, some string/memory/time functions, and Wine-derived headers/IDLs. jq's own argument handling and number formatting do not obviously use the `getopt` or `gdtoa` parts, and no marker for them was found, but a stripped binary cannot prove they are absent.
* **Proposal:** because the linked set cannot be demonstrated reliably, **ship the complete `COPYING.MinGW-w64-runtime.txt` and the winpthreads `COPYING` verbatim** (both in the draft bundle with their hashes) instead of a hand-picked subset. A link map or the MSYS2 build's exact package versions would allow narrowing later; neither is available without rebuilding or executing, which is out of scope.
* **Status:** resolved operationally (complete notices included); the exact subset stays unknown and is *not claimed*.

## Q3 — libgcc and the GCC Runtime Library Exception

* **What is established now (new):** libssp, a GCC runtime library, **is** in the executable. Its licence is GPL-3.0-or-later **with the GCC Runtime Library Exception 3.1** (MSYS2 `gcc` package licence line; exception text stored verbatim). The exception lets the combined work be propagated "under terms of your choice" for *Target Code* produced by an *Eligible Compilation Process* (a normal GCC compile without proprietary plugins), which is what the upstream release build is.
* **What is not established:** whether libgcc proper is also linked (no marker), whether the upstream compilation meets the exception's definition in every detail, and the exact GCC revision's licence line (read at `master`, not at the build revision).
* **Proposal:** the notice bundle carries the exception text and an explanatory note (done in the draft). No GPL source-offer obligation is assumed *because of* the exception; that reading is stated, not decided. Counsel (or the Red Team) should accept it explicitly before any redistribution.
* **Status:** the factual question "is a GCC runtime library linked" is **answered yes** (libssp); the legal reading remains open.

## What this changes and what it does not

| | Before | After this proposal |
| --- | --- | --- |
| Decision | `LICENSE_CONDITIONAL` | `LICENSE_CONDITIONAL` (unchanged) |
| Notice bundle | jq, Oniguruma, mingw-w64, runtime notices, winpthreads, exception text | + Unicode License V3, + libssp note |
| Q1–Q3 | open | operationally covered by a conservative complete bundle; legally still for the Red Team/counsel |
| Recipe `license.state` | `unknown` | `unknown` (never set to `accepted` without a documented review) |
| Production | untouched | untouched |

## The future notice sidecar (design; tested only with synthetic files)

The adapter installs exactly one artifact per tool. A notices file next to it must therefore become an *owned, journalled, verified* second file of the same version folder before it exists — otherwise the adapter treats it as an unexpected entry. **Tested today with a synthetic sidecar** (`adapter_tests.rs`, `a_synthetic_notice_sidecar_is_never_deleted_adopted_or_ignored…`): a stray `NOTICE.txt` beside the installed artifact is reported (`installed_folder_has_unexpected_entries`) when the same version is installed again, blocks **uninstall** (`unrecognised_installed_content`) before anything moves, is left byte-for-byte alone when an update to a new version keeps the old folder for rollback, is never deleted and never adopted (only the owner removing it by hand lifts the block); the production filesystem permissions are not widened by any of it. **Not implemented:** the owned-sidecar layout (second `OwnedFile` role, receipt entry, verified removal). That needs its own reviewed change.
