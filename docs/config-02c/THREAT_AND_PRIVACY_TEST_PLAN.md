# CONFIG-02C — threat model, privacy rules and the test plan for the one-file read (design only)

Issue #40, revision **R2** (the implementation the owner authorized; flags `READ_IMPLEMENTATION_AUTHORIZED` · `REAL_USER_PROJECT_NOT_READ` · `FEATURE_NOT_INDEPENDENTLY_ACCEPTED`, see `REAL_READ_AUTHORIZATION_REQUEST.md` §8). Companion to `READ_ONLY_CONTRACT.md` (section numbers `§` refer to it). **The cases of §3 are implemented as Rust and vitest tests (map in §3a); this document does not say they passed.** What passed, on which operating system, at which commit, is stated only in the Issue after the exact-SHA CI run, from the `CONFIG02C_RESULT` lines of §0 — a green CI run is supporting evidence, never acceptance. Developer and CI read only synthetic fixtures made under a temporary directory; **no real project file was read** (`REAL_USER_PROJECT_NOT_READ`).

## 0. Evidence vocabulary (used in every table)

| Word | Means |
|---|---|
| **PASS** | The check was executed, on the named platform, and passed. Says nothing about the other platform |
| **FAIL** | Executed and failed |
| **SKIPPED** | The harness ran but the case could not be constructed (no privilege for a symlink, no `subst`, no mount…). Always machine-visible, never counted as PASS |
| **BLOCKED** | The environment prevented execution (for example Windows Smart App Control blocked the freshly built binary). Not a failure of the code and not a pass |
| **NOT RUN** | Nothing was executed — the implementation does not exist, or it was not attempted |

**Result lines (implemented; closes finding F8; R1 reviewer decision O5, revised in R2).** Every case ends in exactly **one** line on standard output, and — when the environment variable `CONFIG02C_RESULTS` names a file — appended to that file: `CONFIG02C_RESULT|<id>|<os>|<status>|<reason>` with `<os>` ∈ {`windows`, `macos`}, `<status>` ∈ {`PASS`, `FAIL`, `SKIPPED`, `BLOCKED`} and `<reason>` a fixed short code (empty for PASS/FAIL; never a path). R1 sketched a space-delimited form; R2 uses pipes so that a reason can never be mistaken for a field. A case that cannot be built (no privilege for a symlink, no junction, no mount, no way to deny access) writes `SKIPPED` with its reason and returns; it never prints to standard error and returns silently. A case that panics writes `FAIL` and still fails the test. **An id with no line for an operating system is `NOT RUN`.** The CI step `CONFIG-02C evidence` (a) runs `cargo test config02c -- --nocapture --test-threads=1` and the vitest file of the panel with the verbose reporter, (b) collects the lines, (c) runs `tools/config02c_results_check.py` against `tests/config02c_expected_results.json`, which **fails the job** if any id has no line, has `FAIL`/`BLOCKED`, has two different lines, or is `SKIPPED` with a reason the manifest does not list for that operating system, (d) uploads the lines and the table as an artifact, and (e) prints the id × status table and one `CONFIG02C_RESULTS|<os>|PASS=n|SKIPPED=n|FAIL=n|BLOCKED=n|NOT_RUN=n` line. A report may say "PASS on both OSs" only for ids with an explicit `PASS` line on **each** OS; a `SKIPPED` line is never counted as PASS, even when the surrounding test function reports `ok`. `tests/test_config02c_results_check.py` runs that gate on synthetic lines (missing id, skip, fail, duplicate, foreign line). 02B's tests are **not** retrofitted by this change; their file-symlink case therefore remains unverifiable from CI output.

**Independence (R1, decision O2).** The reader's tests have their **own fixtures, own IDs and own result lines**. They are not 02B tests renamed or re-run, and a 02B pass or skip never counts toward them. Their negative cases (links, hard link, case alias, race, privacy canaries) must run against the *read-only module*, not against the 02B writer.

**What no CI line can show.** jsdom/vitest results (F3–F6 below) are not proof of real-window behaviour. Real Tauri GUI rendering, keyboard/focus order, screen-reader output, and WebView2 runtime egress or profile state (#25) are `NOT RUN` unless a later phase specifically authorizes them; no row below claims otherwise.

Fixtures are created by the test under the runner's temporary directory (canonicalised first: macOS `/var → /private/var`). No fixture contains a real credential: secrets are unique sentinels of the form `ATM_SENTINEL_<8 random hex>_<position>` generated per run, so a leak can be searched for exactly. No test reads outside a directory it created; no test uses the user's home, Downloads, Documents or a real project.

## 1. Evidence that exists today

This table is the R1 record about **existing** code at `7fde8384733ac5b577835560f69438de40bb94a1` and is kept for provenance; none of it exercises the reader. The evidence for the reader (R2) is the CI run named in the Issue and its uploaded result lines, not this table.

| Item | Platform | Status | Notes |
|---|---|---|---|
| `cargo test` (Rust unit tests) — local run | Windows 11, developer machine | **BLOCKED** | The freshly built `ai_toolchain_manager_lib-*.exe` was refused by Windows Smart App Control, `os error 4551` ("Uygulama Denetimi ilkesi bu dosyayı engelledi"), CodeIntegrity event 3077 logged for `cargo.exe` at 2026-10-30T10:02:11 and 10:02:33 (+03:00). The identical binary was re-run once (same result). No rebuild to evade it, no policy change. Nothing about Rust behaviour is claimed from this machine |
| Rust unit tests — CI, exact SHA `7fde838…` | Windows x64 + macOS arm64 | **PASS** (aggregate, run 40766661500 attempt 1) | Supporting evidence only. Passing-test stderr is not shown, so per-case SKIPs of the 02B module (F8) are **not visible** |
| `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | Windows | **PASS** | Compile-only; not affected by Smart App Control |
| `npm run typecheck` | Windows | **PASS** | |
| `npm test` (vitest) | Windows | **PASS** | 31 files, 886 tests |
| `python -m unittest discover -s tests` | Windows | **PASS** | 386 run, `OK (skipped=58)` — before this change; the 58 skips are pre-existing and unrelated |
| `tests/test_config02c_contract.py` (added with these documents) | Windows | **PASS** locally; CI result is recorded in the Issue comment after the push | Text/structure guards only (§2). It cannot show any behaviour of a reader |

## 2. What the committed guard test checks (and why it is only that)

`tests/test_config02c_contract.py` is a documentation-and-reach guard, in the style of the existing per-phase contract tests, **updated deliberately for the owner-authorized implementation** (it was not disabled). It checks: the three documents exist and carry the three status flags; the owner's response block is exactly the transcribed authorization record (the ticked boxes are the recorded answers and nothing else, with the provenance link and the sentence that it is not a direct owner comment); the status vocabulary is the same closed list in the contract, the request, the Rust `Status` enum and the TypeScript table; every `FsError::code()` string has a row in the mapping table; **exactly one** new command (`read_project_mcp_config`, 37 in total) and **exactly one** new permission exist and are registered in `build.rs`, `lib.rs`, `capabilities/default.json` and `health.rs`; the set of production files that name `.mcp.json` is exactly the known set; `pub directory: String` appears only in the old scan's request and the new request; the new request type has `deny_unknown_fields` and no `Debug`; the response struct's fields equal the contract's list; the read-only module and the typed-result module contain none of the forbidden calls (G2) and no production file refers to the 02B prototype; the panel component is mounted only by `ConfigPreview` and `api.ts`/the component are the only frontend files that name the command; Cargo dependencies, plugins and migrations are unchanged; the wording the reviewer required (O1–O5), the disclosures and the absence of the withdrawn overclaims are present; every id of the manifest has a case in the Rust or vitest sources and vice versa; and the CI workflow runs the result gate on both operating systems and uploads its output. It checks wording, structure and reach only. Behaviour is what the Rust and vitest cases show, per operating system, through the result lines.

## 3. Threat → test table (for the implementation phase)

Each row is one meaningful threat to the one-file read boundary; there are no repetitive mutation loops. Platform: **W** Windows, **M** macOS, **both**. Status of every row today: **NOT RUN** (no implementation).

### A. Wrong target, wrong authority

| ID | Threat | Fixture / action | Required outcome |
|---|---|---|---|
| A1 | An alias is silently resolved to a directory | request with a valid alias and an empty or whitespace `directory` | refused (`unsafe_path`/`invalid_root_text`) before any filesystem call — asserted with a call counter on the open seam |
| A2 | Unknown or removed alias; sample project | alias not registered / `ornek-web-sitesi` (no folder) | `PROJECT_NOT_FOUND` / refused; nothing opened |
| A3 | Another file or surface smuggled in | JSON with extra members (`file`, `path`, `surface`, `name`) | rejected by `deny_unknown_fields`; no filesystem call |
| A4 | Path traversal in the root text | `..`, `.`, `a\..\b`, UNC `\\host\share`, `\\?\C:\…`, `\\.\PhysicalDrive0`, relative path, drive root `C:\`, `/`, > 260 chars, control characters | each `unsafe_path` (`invalid_root_text` or `root_too_broad`) / `unsupported_fs` for UNC; nothing opened |
| A5 | Consent missing or different | `consent` absent / other string | refused; nothing opened |
| A6 | Root exists but is not the project's | a different real directory typed for the alias | read **succeeds** (the pairing is user-asserted, §4) and the result carries no claim about the project; **documented limitation, not a defect** — the test pins that nothing is persisted or changed |

### B. Filesystem origin

| ID | Threat | Fixture | Required outcome | Plat. |
|---|---|---|---|---|
| B1 | Root is a link | directory symlink (macOS) / junction (Windows) as the typed root | `unsafe_path`/`link`; nothing below it opened; target-side canary file unchanged | both |
| B2 | An ancestor is a link | link in the middle of the chain | `unsafe_path`/`link` | both |
| B3 | `.mcp.json` is a file symlink → outside file | symlink to a canary outside the root | `unsafe_path`/`link`; canary never read (its sentinel absent from everything); **SKIPPED visibly if the privilege is missing** | both |
| B4 | `.mcp.json` is a directory link / directory | junction / symlink / plain directory at the name | `unsafe_path` | both |
| B5 | Hard-linked target | second hard link to the file (and one to an outside file) | `unsafe_path`/`hard_linked`; both names unchanged | both |
| B6 | Case-variant sibling | `.MCP.JSON` next to `.mcp.json` (case-sensitive volume) / only `.MCP.JSON` (case-insensitive volume) | sibling+target ⇒ `unsafe_path`/`case_alias`; only-variant on a case-insensitive volume ⇒ the OS resolves it: the test **records** the observed outcome per platform instead of assuming one | both |
| B7 | Network / unknown filesystem | UNC (text) — executable; mounted `smbfs`/`nfs` (macOS) and a mapped network drive (Windows) — only where available | `unsupported_fs`; real network mounts are **SKIPPED** unless provided, never simulated and reported as PASS. The existing `hook::set_volume` seam may drive the cross-volume branch only, and is reported as such | both |
| B8 | Special file | FIFO at the name (macOS `mkfifo`) — must not hang (`O_NONBLOCK`) | `unsafe_path`/`not_regular_file` within a second | M |
| B9 | Unreadable target / unreadable root | Windows: `icacls … /deny` for the current user; macOS: `chmod 000` (skipped as root) | `denied`; **never** `absent`, never "not configured" | both |
| B10 | Alternate data stream / 8.3 names | cannot be requested (no input forms any part of the name) | no test; stated in §4 | – |
| B11 | Missing root vs missing file | typed directory that does not exist vs an existing empty directory | `root_not_found` vs `absent` — the two must be distinguishable | both |

### C. Content

| ID | Threat | Fixture | Required outcome |
|---|---|---|---|
| C1 | Empty file | 0 bytes | `malformed`/`empty_file` (**not** `absent`) |
| C2 | Exactly at / over the cap | 1,048,576 bytes of valid JSON padding; 1,048,577 | at cap: processed; over: `too_large`, no digest |
| C3 | BOM | UTF-8 BOM + valid JSON | `present_metadata_only`/`byte_order_mark` (size+digest only) |
| C4 | Invalid UTF-8; UTF-16 file | bytes `FF FE …` | `malformed`/`invalid_utf8` |
| C5 | Duplicate keys | `mcpServers` twice; the same server name twice; duplicate via escapes (`"e\u0078a"` vs `"exa"`) | `present_metadata_only`/`duplicate_key`; **neither value is used** |
| C6 | Structure | root array/string/number; `mcpServers` an array; 65-deep nesting; 257 servers; comments; trailing comma | `present_metadata_only` (`root_not_object`, `servers_not_object`, `too_deep`, `too_many_servers`) or `malformed`/`invalid_json` |
| C7 | Lone surrogate | `"\ud800"` | `malformed`/`lone_surrogate` |
| C8 | Active content in values | `"<img src=x onerror=…>"`, Markdown links, `javascript:` | not returned in any form; a vitest render test asserts no element is created from any value of the file because no such string reaches the UI |
| C9 | Reviewed-declaration states | `exa` present / `EXA` / absent; endpoint under another name; `url` not a string; server entry not an object | exactly the documented `nameState`/`endpointState`; no error echo |

### D. Races and re-pointing (use the existing test hook style: `hook::fire("…")` at fixed stages)

| ID | Threat | Action at the hook | Required outcome |
|---|---|---|---|
| D1 | Content changes after open, before read ends | append / overwrite in place | `changed_during_read`; **no digest, no size, no counts** returned |
| D2 | File replaced by another file (new identity, same content) between stat and open | rename-over | `changed_during_read` |
| D3 | Target swapped for a link between stat and open | replace with symlink/junction | `unsafe_path`/`link`; outside canary unread |
| D4 | File deleted after open | unlink (Windows: expected to be allowed by `SHARE_ALL`) | either a complete consistent result of the opened object or `changed_during_read`; **never** a result mixing two objects |
| D5 | Directory renamed / ancestor swapped while the handle is open | rename the project folder or an ancestor | the bytes returned (if any) belong to the root that was walked; never to a directory that now carries the old name. Windows may refuse the rename (handle held without `FILE_SHARE_DELETE`): the test records which, it does not assume |
| D6 | Stale digest reuse | read, edit, read again | two different digests; no value from the first call survives (no cache, no memo, no token) |
| D7 | One-call scope | two consecutive calls | each performs a fresh open (call counter = 2); the second request never reuses the first's root handle or result |
| D8 | Same size and mtime restored by a writer | rewrite with equal size, reset mtime | **documented as undetectable** (§4 residual). The test asserts only that the reported digest equals the SHA-256 of the bytes actually read; it must not claim detection |

### E. Privacy (the sentinel test)

For every position below, the fixture file contains a unique sentinel; the test then serialises the response (all statuses), dumps `diagnostics`, `audit_events`, every table of an in-memory DB, collects `stderr`/panic text/`format!("{:?}")` of every error type involved, and asserts **no sentinel appears anywhere**.

| ID | Position of the sentinel |
|---|---|
| E1 | `env` value; `headers.Authorization`; `args` element; `url` userinfo; `url` query; `command` |
| E2 | A secret that matches **no** heuristic in `redact.rs` (random base64/hex without `sk-`, `ghp_`, `Bearer`, `token=`…) in each of the positions above |
| E3 | A sentinel as a **server name** other than the reviewed one (must not be returned: this contract returns no such names) |
| E4 | A sentinel as an **unknown top-level key**, an unknown key inside a server, and a comment-like string |
| E5 | A sentinel in the file's **name context**: the typed directory text contains the sentinel; it must not appear in any response, log, DB row or error |
| E6 | Parse-error text: a syntactically broken file containing the sentinel near the error position — the error carries a code only |
| E7 | Schema test: serialise one value of every `Status`; assert the key set ⊆ the contract's field list and that every string is the alias, a constant, a closed-enum value, a declaration id or `[0-9a-f]{64}` |
| E8 | The metadata-only fallback is taken (not a best-effort redacted dump) whenever §3.4's conditions are not all met |

### F. Product invariants

| ID | Invariant | Check |
|---|---|---|
| F1 | Preferences, preference history, catalog acceptance, install states, review-queue counts, audit chain, diagnostics, settings unchanged after **every** status, success or failure | hash/row-count of each table before and after |
| F2 | No persistence | the DB file's bytes / `PRAGMA data_version` and table row counts equal before/after; no new table or migration |
| F3 | The pasted-text preview still works when the read is declined, fails, or its section is never opened | vitest (`ConfigPreview` with the section collapsed, refusing, and rejecting an IPC error) |
| F4 | State cleared on *Temizle*, unmount, project/OS/surface/entry change, new read, 5-minute timer; typed directory cleared right after the call | vitest with fake timers and remounts |
| F5 | Forbidden claims never rendered | vitest scans rendered text of every status for the §7 forbidden words and for "yapılandırılmamış" outside the `absent` explanation |
| F6 | `absent`/`denied`/`unsafe_path`/`root_not_found` never produce a `Configuration::NotConfigured`-like value or a verified badge | vitest + a Rust type check: the result type has no field that could carry it |
| F7 | The read never changes the existing Inventory scan's behaviour | the existing discovery tests are unchanged and pass |

### G. No new reach

| ID | Check |
|---|---|
| G1 | Exactly one new `#[tauri::command]` and one new `allow-…` permission; no `fs`/`dialog`/`shell`/`http`/`opener`/`clipboard`/`process` permission; no new Cargo dependency; no migration (extends the existing 02B guard) |
| G2 | The new module's source contains none of: `std::process`, `Command::new`, `std::net`, `reqwest`, `tauri::`, `invoke`, `SystemTime`, `chrono`, `std::env`, `std::fs`, `canonicalize`, `read_dir`, `OpenOptions`, `create_file`, `create_child`, `create_owner_only_child`, `write_all`, `remove_`, `move_file`, `replace_owned_file`, `rename`, `File::create`, `set_permissions`, `set_len` |
| G3 | The module is the only production code that names `.mcp.json` besides the existing scan (the §2 guard, updated deliberately by the implementation change) |
| G4 | No `console.*`, `localStorage`, `sessionStorage`, `navigator.clipboard`, `fetch`, `XMLHttpRequest`, `window.open` in the new component |
| G5 | The CI step for the module prints and uploads its `CONFIG02C_RESULT` lines, fails on a missing ID × OS line, and a `SKIPPED` line is never summarised as PASS (§0) |

## 3a. Implementation map (R2)

Where each id lives and what the manifest (`tests/config02c_expected_results.json`) allows per operating system. Rust cases: `src-tauri/src/mcp_read_tests.rs` (A, C, E, F, mapping) and `src-tauri/src/managed_fs_read_tests.rs` (B, D, G2); support in `config02c_support.rs`. Vitest cases: `src/FileSnapshot.test.tsx` (F3–F6, G4, U1–U3).

| Id | Rows of §3 it covers | Notes |
|---|---|---|
| A1–A6 | A | A1/A2/A4/A5 assert, through a call counter on the root-open seam, that nothing was opened. A6 pins that nothing is persisted |
| B1–B5, B6a, B6b, B11 | B | link, hard link and case-alias cases; **B6b is SKIPPED on a case-insensitive volume** (Windows NTFS and default APFS cannot hold both names) with the reason `case_insensitive_volume_cannot_hold_both`; B6a (a variant name alone) runs everywhere |
| B7a, B7b | B7 | B7a: UNC/device spellings refused before anything is opened (Windows; SKIPPED on macOS, which has no such spelling). B7b: a real network mount cannot be provided on a runner → **SKIPPED by design on both**, never PASS |
| B8 | B8 | FIFO at the name (macOS; SKIPPED on Windows) |
| B9a, B9b | B9 | B9a: unreadable target (`icacls /deny` / `chmod 000`, skipped as root); B9b: a target held without sharing (Windows only; SKIPPED on macOS). Unreadable *root* is not tested |
| B10 | B10 | no test: no part of the name comes from input |
| C1–C9 (+C9b) | C | C9b compares the URL rule with the 02B prototype's `url_identity` on shared inputs |
| D1–D8 (+D1b, D9) | D | D4/D5 record the observed outcome (Windows may refuse the folder rename while the root handle is held); D8 asserts only what the digest is of; D9 asserts the reader leaves content, last-write time and entry count unchanged |
| E1–E8 | E | one unique sentinel per position, searched for in the response (JSON and Debug), the error text and a dump of every table of an in-memory database |
| F1, F2, F6 | F | table dump before/after, no new table or migration, no status that can carry a not-configured or verified value |
| F3, F4, F5, F6ui | F | vitest: the paste preview, clearing (button, new read, context change, close, unmount, 5-minute timer), forbidden wording, no badge |
| G1, G3 | G | `tests/test_config02c_contract.py` |
| G2 | G | the read-only module and the typed-result module contain none of the forbidden calls |
| G4 | G | vitest: the component source |
| G5 | G | `tools/config02c_results_check.py` + `tests/test_config02c_results_check.py` + the CI step |
| R1a, R1b | (R2.1, finding F1) | R1a: the reader opens its root only through `open_anchor_readonly`; the Windows mask has no write-like right and `DIR_ACCESS` is unchanged (macOS: the directory open flags are `O_RDONLY`/`O_NOFOLLOW`). R1b: a synthetic root in which a file or folder cannot be created (Windows `icacls /deny user:(WD,AD)`; macOS `chmod 555`, skipped as root) is still read; the precondition is probed first and a folder that stayed writable is SKIPPED, never PASS; the permission is restored in a drop guard |
| R3a, R3b | (R2.1, finding F3) | a case-only re-spelling of the file after the names were listed (R3a) or after both reads (R3b) is refused with nothing read. SKIPPED with `case_sensitive_volume_has_no_case_only_alias` where the volume treats the new spelling as a different file. The reproduction before the fix is recorded in the Issue, not here |
| R2 | (R2.1, finding F2) | vitest: the gate's network sentence |
| U1–U3 | (added) | one click = one call with exactly `{alias, directory, consent}`, declining = zero calls, strict acceptance of the answer, the digest comparison wording |

Not run and not claimed: real GUI, keyboard focus order, screen-reader output, WebView2 runtime egress and profile state (#25); real network mounts; an unreadable root; macOS behaviour until the macOS run's lines are in the artifact.

## 4. Residual risks that tests cannot remove

* The pairing alias ↔ folder is a user assertion (A6).
* A same-user process can still change the file between the last check and the end of the read (D8). The digest identifies **the bytes observed**, not who wrote them, not that they are trustworthy, and not the file's state afterwards.
* `absent` means no exactly-named entry was observed in that directory at that moment — not that MCP is unconfigured, and not that the directory is the project's root.
* The handle-based root walk inspects **ancestor directory metadata** (type, link/reparse status, volume), and the reader lists the **root's entry names** in memory to detect a case-variant sibling. Neither is returned or stored, but neither is "nothing outside the file was looked at".
* The spelling recheck (R3a/R3b) lists names once more after the last name lookup; a re-spelling after that listing, or a same-size/restored-time content edit (D8), is not detected.
* The gate no longer promises that the application never uses the network. It states only that this read starts none on purpose; #25 (runtime traffic of the embedded WebView) is open and not measured here.
* The existing inventory scan's lenient parse is not Claude Code's runtime behaviour either; this reader's strict refusal is a **local policy**.
* The reader's strictness differs from Claude Code's parser; `present_metadata_only` is not "invalid".
* The typed path exists in the WebView's JavaScript memory and input element, crosses IPC into Rust as request input, and may persist in memory the application does not control (JS string/GC behaviour, the WebView2 runtime, OS memory). Removing it from component state after the call is hygiene, **not** a secure erase, and nothing here hides it from the renderer. #25 (WebView2 runtime egress/profile state) is unresolved and unmeasured by this plan. A picker would keep the path in Rust but adds a dependency and a capability, and is a separate decision.
* Screen sharing: a displayed digest or `serverCount` is visible to anyone who can see the screen. No file text is.
* macOS behaviour is evidenced only by the macOS lines of the CI artifact; Windows local Rust execution has at times been BLOCKED by Smart App Control (never worked around).

## 5. Questions for the independent Red Team

1. Is there any path by which a byte of the file, or user text from it, reaches the WebView, a log, the DB or an error? Try sentinels in places this plan did not think of.
2. Can the typed directory text reach a log, a diagnostic, a panic message or a serialised error?
3. Does the "refuse a case-variant sibling" rule create a denial-of-service the user cannot fix (for example a repository that legitimately contains both names on a case-sensitive volume)? Is `unsafe_path`/`case_alias` shown with an action the user can take?
4. Is refusing hard links too strict for real repositories (some tools hard-link files)? Is the message honest about why?
5. Does the strict-parse refusal (`BOM`, `duplicate_key`) mislead a user into thinking Claude Code rejects the file?
6. Is `reviewedDeclarations` still a closed channel? Could `endpointState` be used as an oracle to probe the content of other servers' URLs (it compares to one public endpoint only)?
7. Can two quick successive calls, or a call during a scan, interleave in a way that returns one object's size with another's digest?
8. Is the `consent` constant sufficient as a *coherence* check, given a compromised WebView could always send it?
9. Does the read-half child module of `managed_fs` really contain no write-capable call (the G2 list), and does using `managed_fs` on a production path while #14 is open need a stated exception?
10. Do the Turkish strings in the authorization request promise anything the implementation will not do? (R1 re-worded the folder-field label and the gate bullets about parent folders; check the new wording against §4 of the contract.)
11. Are the two reader labels (`önceki envanter gözlemi (farklı kapsam/yöntem)` / `dosya anlık görüntüsü (katı JSON sözleşmesi)`) sufficient to prevent a user from reading either as the other, and does any planned screen risk merging them?
12. Is the `CONFIG02C_RESULT` mechanism enough to make a silent skip impossible, including a test that panics before writing its line?
