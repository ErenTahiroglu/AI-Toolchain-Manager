# CONFIG-02C — threat model, privacy rules and the test plan for the one-file read (design only)

Issue #40, revision **R1** (after the independent decision `CONFIG_02C_DESIGN_ACCEPTED_WITH_REVISIONS`; see the contract's §11). Companion to `READ_ONLY_CONTRACT.md` (section numbers `§` refer to it). **No test in §3 exists yet and none has been run: there is no implementation to test.** Green CI runs of this design (41018361348 at `75fc20e`, and any run of this revision) check documents, text guards and unchanged regressions; they are **not evidence of a reader**. This document says what an implementation must be tested against before it may be approved; it is not evidence of anything except what §1 states. `READ_NOT_GRANTED`.

## 0. Evidence vocabulary (used in every table)

| Word | Means |
|---|---|
| **PASS** | The check was executed, on the named platform, and passed. Says nothing about the other platform |
| **FAIL** | Executed and failed |
| **SKIPPED** | The harness ran but the case could not be constructed (no privilege for a symlink, no `subst`, no mount…). Always machine-visible, never counted as PASS |
| **BLOCKED** | The environment prevented execution (for example Windows Smart App Control blocked the freshly built binary). Not a failure of the code and not a pass |
| **NOT RUN** | Nothing was executed — the implementation does not exist, or it was not attempted |

**Design condition for the future test code (closes finding F8; R1 reviewer decision O5).** Every test case writes **exactly one result line** to a results file whose path comes from the environment variable `CONFIG02C_RESULTS` (default: a file in the test's own temporary area): `CONFIG02C_RESULT <test-id> <os> <PASS|FAIL|SKIPPED|BLOCKED> [reason-code]`. A case that cannot be built (no symlink privilege, no junction, no `subst`, no mount, no permission to deny) writes `SKIPPED` with a reason and returns; it never `eprintln!`s and returns silently. The CI step for this module must (a) run `cargo test config02c -- --nocapture --test-threads=1`, (b) print the results file, (c) **fail if any ID listed in this plan has no result line for that OS** (such an ID is `NOT RUN`, never PASS), (d) upload the results file as a CI artifact next to the log, and (e) produce an ID × OS summary table. A report may say "PASS on both OSs" only for IDs with an explicit `PASS` line on **each** OS; a `SKIPPED` line is never counted as PASS, even when the surrounding test function reports `ok`. 02B's tests are **not** retrofitted by this change; their file-symlink case therefore remains unverifiable from CI output.

**Independence (R1, decision O2).** The reader's tests have their **own fixtures, own IDs and own result lines**. They are not 02B tests renamed or re-run, and a 02B pass or skip never counts toward them. Their negative cases (links, hard link, case alias, race, privacy canaries) must run against the *read-only module*, not against the 02B writer.

**What no CI line can show.** jsdom/vitest results (F3–F6 below) are not proof of real-window behaviour. Real Tauri GUI rendering, keyboard/focus order, screen-reader output, and WebView2 runtime egress or profile state (#25) are `NOT RUN` unless a later phase specifically authorizes them; no row below claims otherwise.

Fixtures are created by the test under the runner's temporary directory (canonicalised first: macOS `/var → /private/var`). No fixture contains a real credential: secrets are unique sentinels of the form `ATM_SENTINEL_<8 random hex>_<position>` generated per run, so a leak can be searched for exactly. No test reads outside a directory it created; no test uses the user's home, Downloads, Documents or a real project.

## 1. Evidence that exists today

All of it concerns **existing** code at `7fde8384733ac5b577835560f69438de40bb94a1`; none of it exercises the proposed read.

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

`tests/test_config02c_contract.py` is a documentation-and-reach guard, in the style of the existing per-phase contract tests. It checks: the three documents exist and state `READ_NOT_GRANTED`, and the contract states revision `CONFIG_02C_DESIGN_R1` and the reviewer decision; the status vocabulary is the same closed list in all three; every `FsError::code()` string in `managed_fs.rs` has a row in the mapping table; **the set of production source files that name `.mcp.json` is exactly the set found in the F1/F2 inspection** (so a new reader cannot be added silently); the command count, capability file, Cargo dependencies, migrations and the absence of any dialog/fs plugin are unchanged; no file under `src/` imports a Tauri dialog/fs plugin; the O1 labels, the O4 decision (D4a recommended, D4b NO/deferred), the no-secure-erase / WebView-exposure statement, the disclosure of ancestor metadata and root entry-name inspection, the result-line format of §0 and the unchecked owner boxes are all present; and none of the overclaims the review withdrew ("nothing outside the file is read", "erased when the call ends") appears. It checks wording and structure only. It deliberately has no Rust counterpart: a Rust "contract test" would need production types that must not exist yet, and the one empirical question a Rust test could answer (how the existing `fsx` reader treats a hard-linked or case-variant `.mcp.json`) is answered by reading `fsx.rs` and is labelled *source-inspected* in the contract (F4).

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

## 4. Residual risks that tests cannot remove

* The pairing alias ↔ folder is a user assertion (A6).
* A same-user process can still change the file between the last check and the end of the read (D8). The digest identifies **the bytes observed**, not who wrote them, not that they are trustworthy, and not the file's state afterwards.
* `absent` means no exactly-named entry was observed in that directory at that moment — not that MCP is unconfigured, and not that the directory is the project's root.
* The handle-based root walk inspects **ancestor directory metadata** (type, link/reparse status, volume), and the reader lists the **root's entry names** in memory to detect a case-variant sibling. Neither is returned or stored, but neither is "nothing outside the file was looked at".
* The existing inventory scan's lenient parse is not Claude Code's runtime behaviour either; this reader's strict refusal is a **local policy**.
* The reader's strictness differs from Claude Code's parser; `present_metadata_only` is not "invalid".
* The typed path exists in the WebView's JavaScript memory and input element, crosses IPC into Rust as request input, and may persist in memory the application does not control (JS string/GC behaviour, the WebView2 runtime, OS memory). Removing it from component state after the call is hygiene, **not** a secure erase, and nothing here hides it from the renderer. #25 (WebView2 runtime egress/profile state) is unresolved and unmeasured by this plan. A picker would keep the path in Rust but adds a dependency and a capability, and is a separate decision.
* Screen sharing: a displayed digest or `serverCount` is visible to anyone who can see the screen. No file text is.
* macOS behaviour is design-only until a macOS CI run executes the above; Windows local Rust execution is currently BLOCKED (§1).

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
