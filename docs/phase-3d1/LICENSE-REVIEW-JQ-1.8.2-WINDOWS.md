# jq 1.8.2 / Windows x64 — licence and distribution-notice review (Phase 3D-1)

**Decision: `LICENSE_CONDITIONAL`.** This is an engineering review of primary texts, **not legal advice**. Every licence found is permissive and none forbids what the pilot would do, but the executable is a *statically linked* bundle of several components whose notices upstream does not ship with it, and three questions are open (§4). The decision therefore does not turn into `LICENSE_PASS` until the conditions in §5 are met.

## 1. Method and evidence

| Step | What was done | Where the evidence is |
| --- | --- | --- |
| Source of the licence texts | Read-only requests to `api.github.com` only, **behind `tools/egress_guard.py`** (guard reports: 0 incidents, one contacted host). Refs resolved to commits: `jqlang/jq` tag `jq-1.8.2` = `34f7186b…4108`; submodule `vendor/oniguruma` = `kkos/oniguruma@4ef89209a239c1aea328cf13c05a2807e5c146d1`; `mingw-w64/mingw-w64`, `gcc-mirror/gcc`, `msys2/MINGW-packages` at their `master` at fetch time (stated in the manifest). | [`evidence/license/`](evidence/license/) with `evidence-manifest.json` (SHA-256 of every stored file) |
| What is in the *binary* | Static analysis of the **retained** `jq-windows-amd64.exe` bytes (read as data; never executed, loaded or mapped): PE imports, section list, embedded strings. | `evidence/license/pe-static.json` |
| Whether `COPYING` is exhaustive | **Not assumed.** All 78 files under `src/` and `vendor/decNumber/` of jq and all 64 `src/*.c|h` files of Oniguruma were scanned for copyright/licence lines; deviations from `COPYING` are listed in §3. | `jq-src-notice-scan.json`, `onig-src-notice-scan.json`, `source-excerpts-*.txt` |
| What the build is | The release workflow (`ci.yml` at the tag): Windows amd64 job `msystem: UCRT64`, `./configure --disable-docs --with-oniguruma=builtin --disable-shared --enable-static --enable-all-static CFLAGS="-O2 -pthread -fstack-protector-all -Wl,--stack,8388608"`. The same configure string is embedded in the executable. | `jq-ci.yml.txt`, `pe-static.json` |

**Facts read from the binary (static):** `GCC: (Rev5, Built by MSYS2 project) 16.1.0`; `Mingw-w64 runtime failure:` (the mingw-w64 CRT is linked in); the source path `../mingw-w64/mingw-w64-libraries/winpthreads/src/rwlock.c` (winpthreads is linked in statically — **no** `libwinpthread-1.dll` import); `ONIG-MONITOR` and `You didn't call onig_initialize() explicitly` (Oniguruma built in); `decNumber 3.68`; imports only `KERNEL32.dll`, `SHLWAPI.dll` and the Universal-CRT API sets `api-ms-win-crt-*` (system components, not redistributed by anyone here). No `libgcc` string was found; whether any libgcc object is linked cannot be told from strings.

## 2. Component and obligation matrix

`B` = evidence that the component is in the executable (static analysis); `S` = evidence from the source/licence text only.

| # | Component | Licence (primary text) | In the exe? | What the text requires when the binary is distributed | Status |
| --- | --- | --- | --- | --- | --- |
| 1 | jq itself (Stephen Dolan) | MIT (`COPYING`) | yes (it is the program) | the copyright notice and permission notice in all copies or substantial portions | **confirmed** |
| 2 | David M. Gay `dtoa.c`, `g_fmt.c` (Lucent) | permissive notice in `COPYING`; also in `src/jv_dtoa.c` | S: `jv_dtoa.c` is compiled in | the entire notice "is included in all copies of any software which is or includes a copy or modification" | **confirmed text; applicability to a binary-only copy is the conservative reading** |
| 3 | IBM decNumber 3.68 | ICU License (`COPYING`, `vendor/decNumber/ICU-license.html`) | **B**: string `decNumber 3.68` | copyright + permission notice "appear in all copies of the Software and … in supporting documentation" | **confirmed** |
| 4 | Heimdal-derived code (`jv_thread.h`, parts of `jv.c`) | two-condition BSD-style (`COPYING`; header of `src/jv.c`) | S | binary form: reproduce the notice in documentation / other materials | **confirmed text** |
| 5 | NetBSD `strptime` (modified, `src/util.c`) | BSD 2-clause style (`COPYING`; header of `src/util.c`) | S: used where the C library lacks `strptime` (not proven for this build) | same | **confirmed text; use in this build unverified** |
| 6 | MurmurHash3-derived string hash (`src/jv.c`) | "placed in the public domain" (source comment) | S | none | confirmed (comment) |
| 7 | Bison-generated `src/parser.c` / `parser.h` (Bison 3.8.2 skeleton) | GPL-3.0-or-later **with the Bison parser-skeleton special exception** (file header; **not mentioned in `COPYING`**) | S: the parser is compiled in | the exception lets the larger work be distributed "under terms of your choice"; no GPL obligation on the binary if the exception applies | **confirmed text; `COPYING` is demonstrably not exhaustive** |
| 8 | Oniguruma (submodule `4ef89209…`) | BSD 2-clause (`COPYING` of the submodule; same header in all `src/` files) — by K. Kosako; two files also credit others (`cp1251.c`: "Byte", `gb18030.c`: K. Kubo) under the same BSD terms | **B**: `ONIG-MONITOR` strings | "Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials" | **confirmed**; jq's `COPYING` does **not** list Oniguruma |
| 9 | Oniguruma `src/st.c` | "public domain general purpose hash table package written by Peter Moore @ UCB" (file head) | S | none | confirmed (comment) |
| 10 | Oniguruma generated Unicode tables (`unicode_*_data.c`) | only K. Kosako's copyright in the file heads; **no Unicode data-file notice appears** | S | the Unicode data licence normally asks that its copyright/permission notice accompany copies of data files **or derived data**; whether that reaches these tables is not answered by the texts read | **UNRESOLVED (Q1)** |
| 11 | mingw-w64 CRT (statically linked) | ZPL 2.1-style terms for the project's own code; parts public domain / BSD / LGPL "prominently marked" (`COPYING`); the runtime's own notice file says the binary "was built using MinGW-w64 and statically linked against the MinGW-w64 runtime" and that **some parts require copyright and licence notices to be included when distributing the code in binary form** (`COPYING.MinGW-w64-runtime.txt`, 12 155 bytes, includes e.g. a Todd C. Miller getopt notice) | **B**: `Mingw-w64 runtime failure:` | ZPL clause 2: reproduce the notice/conditions in binary redistribution; plus whichever listed third-party parts are actually linked | **confirmed that notices are required; WHICH listed parts are linked is UNRESOLVED (Q2)** |
| 12 | winpthreads (statically linked) | MIT-style (mingw-w64 project) **plus** code derived from "Posix Threads library for Microsoft Windows" © 2010 Lockless Inc. under BSD-3 with a non-endorsement clause (`winpthreads/COPYING`) | **B**: embedded source path `winpthreads/src/rwlock.c` | MIT: notice in copies; BSD-3: reproduce in binary form, no endorsement | **confirmed** |
| 13 | libgcc (compiler runtime) | GPL-3.0-or-later **WITH GCC-exception-3.1** (MSYS2 `gcc` package licence line; exception text `COPYING.RUNTIME`) | **not shown**: no libgcc string; objects linked unknown | with a qualifying "Eligible Compilation Process" the combined binary may be propagated under terms of one's choice; no GPL source-offer obligation | **UNRESOLVED (Q3): whether any libgcc object is linked and whether the exception's conditions hold (they depend on the upstream build, not on this project)** |
| 14 | Universal CRT, KERNEL32, SHLWAPI | Microsoft system components, imported dynamically | yes (imports) | none for this project: not copied, not bundled; the machine must provide UCRT (inbox on supported Windows) | confirmed (imports) |
| 15 | jq documentation (CC BY 3.0) and the docs website components | CC BY 3.0 and others (`COPYING`) | **no**: build flag `--disable-docs`; no manual text found in the binary | n/a for the executable | confirmed by flag; not proven byte-exhaustively |

## 3. Where `COPYING` is not enough

`COPYING` of jq 1.8.2 covers rows 1–5 and the documentation statement. It does **not** mention: Oniguruma (row 8), the Bison exception (row 7), `st.c` (row 9), the Unicode tables question (row 10), the MinGW-w64 runtime (row 11), winpthreads (row 12) or libgcc (row 13). The release workflow has no step that adds those notices to the artifact; **the published `jq-windows-amd64.exe` ships as a bare executable with no notice file.** The draft bundle [`THIRD-PARTY-NOTICES.jq-1.8.2.windows-x64.DRAFT.txt`](THIRD-PARTY-NOTICES.jq-1.8.2.windows-x64.DRAFT.txt) assembles the verbatim primary texts of rows 1–5, 8, 11, 12 (and the exception text of row 13) with their SHA-256 values. It is a draft: not installed, not shown, not shipped.

## 4. Open questions (not answerable from the primary texts read)

* **Q1 — Unicode data in Oniguruma.** Does the Unicode data licence reach the generated tables compiled into the binary, and if so is the missing notice upstream's gap or a non-issue? Needs the Unicode terms of the data version Oniguruma used (not fetched here) and a legal reading.
* **Q2 — which mingw-w64 runtime parts are linked.** The runtime notice file lists several third-party parts; a link map or the MSYS2 build's package versions would tell which. Neither is available without executing or rebuilding. Conservative handling: treat the whole `COPYING.MinGW-w64-runtime.txt` as applicable.
* **Q3 — libgcc and the GCC Runtime Library Exception.** Whether any libgcc object is in the executable, and whether the build meets the exception's "Eligible Compilation Process" conditions (e.g. no proprietary compiler plugin). The exact toolchain was GCC 16.1.0 from MSYS2 (string in the binary); the MSYS2 licence line for that package was read at `master`, not at the exact build revision.
* **Q4 — is a manager-made copy a "distribution"?** The pilot design downloads the file from the upstream release onto the user's own computer at the user's request and keeps it in a manager-owned folder. This project does **not** host, mirror, bundle or sign the binary. Whether the manager's copy counts as the project distributing the program is a legal characterisation; the safe design assumption is *no redistribution by the project*, and the notices still travel as a courtesy (condition C2).
* **Q5 — a future signed installer or bundled copy.** Bundling jq in the application installer *would* be redistribution; the full notice bundle (and resolution of Q1–Q3) would then be mandatory, and the distribution of the *source* of any GPL-with-exception part would need a fresh look.

Not determined by this review either: the licences of jq's *build tooling* (autotools, flex) — they do not extend to the output as far as the texts read go, but their generated-file headers were not scanned beyond `src/`.

## 5. Conditions for upgrading to `LICENSE_PASS`

* **C1.** The project does not host, mirror, bundle or re-sign the executable: it is fetched by the user's computer from the pinned upstream release.
* **C2.** A reviewed `THIRD-PARTY-NOTICES` text (derived from the draft) is available to the person inside the application (about/licences page) **and** stored next to the installed version as a manager-written text file in a separate, explicitly reviewed step; the current adapter installs one artifact per tool, so this needs its own recipe/plan decision and is **not** implemented in this phase.
* **C3.** The Red Team (or counsel) accepts or resolves Q1–Q3 in writing.
* **C4.** Any change toward redistribution (Q5) re-opens this review.
* **C5.** The recipe's `license.state` stays `unknown` until C1–C3 are recorded; the validator blocks the recipe on it (`license_not_accepted`), tested.

## 6. Result in one line

All identified components are permissively licensed and none forbids the pilot's design; the executable is a static bundle whose required notices are not distributed with it upstream, three questions are open, so: **LICENSE_CONDITIONAL**.
