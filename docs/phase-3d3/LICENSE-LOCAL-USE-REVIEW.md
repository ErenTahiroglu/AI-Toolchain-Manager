# Licence review record R34 — jq 1.8.2 / Windows x64, ONE local, non-redistributing use (Phase 3D-3)

**Decision: `LICENSE_PASS_LOCAL_USE`** for exactly the use evaluated below, and **nothing else**. For any bundled, hosted, mirrored or redistributed use the decision stays **`LICENSE_CONDITIONAL`** (Phase 3D-1 / 3D-2 findings unchanged). This is an engineering evaluation of primary texts, **not legal advice**; the one legal assumption it rests on is stated in §4. The record becomes effective in the application only if a reviewed change compiles `evidence/jq-1.8.2-windows-x64.local-use-license.json` into `BUILT_IN_EVIDENCE_JSON`; nothing is compiled in by this phase.

## 1. The evaluated use (and only this)

One person, on their own Windows 11 x64 computer, has their own copy of the upstream release file `jq-windows-amd64.exe` (1,035,264 bytes, SHA-256 `a6fc67fe…c627`; the copy retained from Request A, verified against the publisher's attestation, checksum list and GPG signature). The application copies that one file, unchanged, into **its own per-user folder** on the same computer, after a visible preview and an explicit in-app confirmation, and can remove it again. The application:

* does **not** host, mirror, bundle, re-sign, upload or otherwise make the file available to anyone else (no jq byte is in the repository or in any application installer);
* does **not** download the file in this use (network list empty; no `network_download` permission);
* does **not** run it, add it to PATH, or touch any agent configuration;
* shows the complete third-party notices to that person (application-presented, digest-bound).

The recipe is held to this by the validator: `accepted_local_use` is accepted only if a record covers the exact file digest, the exact notice text digest, the SPDX label, an empty network list, no download permission and lifecycle actions limited to install/uninstall (`attestation.rs`, tested).

## 2. Conditions C1–C5 (Phase 3D-1 review §5)

| # | Condition | Evaluation for this use |
| --- | --- | --- |
| C1 | The project does not host, mirror, bundle or re-sign the executable | **Met.** The repository contains no jq byte (a contract test hashes every tracked file against the digest). The file never leaves the user's computer through this application. |
| C2 | Notices inside the application **and** stored next to the installed version as a manager-written file | **Replaced for this local-use pilot only**, as instructed by the Red Team in Phase 3D-3: the application presents the complete notice bundle and binds its digest into the confirmed preview and the audit row; no file is written next to the version (a sidecar would be foreign content to the adapter). This amendment is **not** a precedent for a bundled release. |
| C3 | Q1–Q3 accepted or resolved in writing | **Evaluated in §3.** None blocks this use; all remain open for redistribution. |
| C4 | Any move toward redistribution re-opens the review | **Enforced in data and code:** the licence state is `accepted_local_use`, not `accepted`; `redistribution_permitted()` is true only for `accepted`; the record is bound to one digest and one notice text. |
| C5 | `license.state` stays `unknown` until C1–C3 are recorded | C1–C3 are recorded here. The state is **not** set to `accepted`; it is the new, narrower `accepted_local_use`, and only inside a candidate that is not registered anywhere. |

## 3. Q1–Q3: legal uncertainty versus engineering evidence

| Q | Engineering evidence (established) | What stays legally uncertain | Does it prevent THIS use? |
| --- | --- | --- | --- |
| Q1 Unicode data in Oniguruma | the tables derive from Unicode 16.0 (`GRAPHEME_BREAK_PROPERTY_VERSION 160000`); only Kosako's BSD notice is in those files; Unicode License V3 asks that its notice accompany *copies of the Data Files or Software* or appear in documentation | whether compiled tables are "Data Files or Software" for that clause | **No.** The clause concerns *copies provided to others*; this use makes no copy available to anyone but the person who already holds the file. The notice is shown anyway (conservative). Open for redistribution. |
| Q2 mingw-w64 runtime parts | the executable statically contains the mingw-w64 CRT and winpthreads; the exact subset is unknowable from a stripped file; the complete runtime notice file is in the bundle | which of getopt / gdtoa / Wine-derived notices apply | **No**, same reasoning (notice duties attach to distribution of binaries). Open for redistribution. |
| Q3 GCC runtime | libssp is statically linked (string evidence); libgcc proper is neither confirmed nor excluded; the exception text and an explanatory note are in the bundle | whether the build meets every detail of the GCC Runtime Library Exception; whether a GPL source offer would ever be owed | **No.** GPL-family obligations arise on *conveying* a binary to others; running or holding one's own copy imposes none, and the manager conveys nothing. Open for redistribution. |

What would genuinely prevent this operation: **nothing found**, except if the single assumption below were wrong.

## 4. The one legal assumption

> Copying a person's own copy of a file from one folder to another folder **of the same account on the same computer**, by a program that person runs, is **not** "redistribution" or "conveying" to a third party.

If counsel or the Red Team reads it otherwise, this record is withdrawn and the decision returns to `LICENSE_CONDITIONAL` (remove the record from the compiled-in list; the recipe then stops validating).

## 5. Not decided here

Any hosting, mirroring, bundling, bundled installer, automatic download by the manager for other users, or update path; the completeness of jq's build-tool licences beyond `src/`; whether the upstream publisher's own distribution satisfied every notice duty (not our question for local use).

## 6. Schema adjustment (minimal)

`recipe.license.state` accepts one additional value, `accepted_local_use`; `authentication[].kind` accepts one additional value, `source_attestation_evidence` (`schemas/reviewed-recipe.schema.json`). Existing synthetic recipes are unaffected (`accepted` unchanged). Without a covering record, both new values are refused by the validator.
