# CONFIG-02C — read-only acquisition contract for ONE project `.mcp.json` (design only)

Issue #40. **Status: `CONFIG_02C_DESIGN_READY_FOR_INDEPENDENT_REVIEW`. `READ_NOT_GRANTED`. Nothing in this document is implemented, and nothing here is a permission.** No IPC command, Tauri capability, file picker, dependency, migration, GUI control or production file-reading code was added. `REAL_CONFIG_WRITE_NO_GO` and `REAL_INSTALLATION_NO_GO` stand. Issues #14 and #25 stay open; #34 and #35 are out of scope. The synthetic `#[cfg(test)]` code of CONFIG-02B (`config_txn.rs`, `managed_fs_mcp.rs`) is **not** promoted and is **not** production code.

Wording: *exists today* = found by reading the source at the SHA in §0; *design condition* = something a separately approved implementation must do; *not run* = no executable evidence exists yet. The three are never mixed.

## 0. What source inspection found (this changes the framing of the issue)

Checked at `development` = `origin/development` = `7fde8384733ac5b577835560f69438de40bb94a1`, `main` = `68eda3f0407490fabccb517dcf822ab4818fc0bf`, CI run 40766661500 green on both OSs.

| # | Finding | Where |
|---|---|---|
| F1 | **A production read of a project's `.mcp.json` already exists.** `run_discovery_scan` (Phase 2A, shown to the user as "Bir proje klasörüne bak … yalnızca şu üç dosya okunur") reads `<typed directory>/.mcp.json` (also `.claude/settings.json`, `.claude/settings.local.json`) up to 1 MiB, parses it with `serde_json`, and keeps **only a count** of definitions matching one catalog entry (`codebase-memory`); server names are explicitly "never kept". Nothing from the file is stored except that count and a fixed Turkish sentence. | `commands.rs:221`, `discovery/mod.rs:27-33,99-122`, `discovery/detect.rs:196-202,954-966,1143-1148,1203-1209` |
| F2 | **A command that accepts a real path already exists** (the same scan). The path is typed into a text box on the Inventory page, validated by `validate_project_dir`, "used for this scan only and never stored or logged"; `store.rs` contains no reference to the directory. There is **no native picker, no dialog plugin and no fs plugin** anywhere (`Cargo.toml`, `package.json`, `capabilities/default.json`). | `discovery/mod.rs:52-96`, `pages/Inventory.tsx:273-288,358` |
| F3 | The project registry stores **alias, display name, note — no path, no root identity**. An alias cannot resolve itself to a directory. | `projects.rs:1-6,188-225` |
| F4 | The existing reader is **path-based on Windows**: it checks each ancestor with `symlink_metadata`, opens by path, then compares `GetFinalPathNameByHandle` (lower-cased) with the expected path. It has **no hard-link-count check** (zero occurrences of `nlink`/`NumberOfLinks`), **no identity capture**, **accepts a case-variant file name** (the comparison is case-folded), **silently strips a UTF-8 BOM**, **cannot detect duplicate keys** (`serde_json::Value` keeps the last), reads the file **once** and computes **no digest**. *Source-inspected; not executed.* | `discovery/fsx.rs:412-435,470-519,636-656` |
| F5 | The existing reader maps a **missing** `.mcp.json` to `Configuration::NotConfigured` with `Confidence::Medium` ("… bulunamadı"). An **unreadable** one maps to `Unknown`. The new contract below must not copy the first mapping (§3.3). | `discovery/detect.rs:1008-1010,1014-1019` |
| F6 | A **handle-relative, no-follow** layer exists (`managed_fs`: `NtCreateFile` with a root handle + `FILE_OPEN_REPARSE_POINT` on Windows, `openat(O_NOFOLLOW)` on macOS; identity and link count from the handle; local filesystems only). **Nothing in the application calls it** (`#![allow(dead_code)]`, "NOTHING in the application calls this module yet"). Its public `Dir` API refuses the name `.mcp.json` (`valid_name` rejects a leading dot). | `managed_fs.rs:1-29,119-130,295` |
| F7 | The 02B test-only `ProjectRoot::inspect` already implements exactly the read half wanted here on synthetic roots: list the root, refuse a case-variant sibling, stat the constant name without following, require a regular file with exactly one link, re-check identity from the opened handle, read ≤ 1 MiB, SHA-256. It is **test-compiled only**. | `managed_fs_mcp.rs:122-148` |
| F8 | The 02B tests print `CONFIG02B_SKIPPED:` on **stderr** when a case cannot be built (file symlink without the privilege, junction, hard link) and return. CI hides stderr of passing tests, so a green run **cannot show** whether those cases executed. The file-symlink case must not be called independently verified on both OSs. | `managed_fs_mcp_tests.rs:4,23-25,571-575` |
| F9 | The Windows files are opened with share mode `SHARE_ALL` (read+write+delete): a read does not stop an editor from saving or renaming the file. The root directory handle is held with `SHARE_READ|SHARE_WRITE` (no delete), so renaming/deleting the *directory* is blocked for the duration of one read. | `managed_fs_windows.rs:331,346,431` |
| F10 | The pasted-text preview computes `sha256Hex(pasted text)` and shows its first 16 hex digits as "Verdiğiniz metnin SHA-256 özeti". | `configPlan.ts:182,512`, `ConfigPreview.tsx:37` |
| F11 | The WebView capability grants only 36 typed application commands; no `fs`, `shell`, `http`, `opener`, `dialog`, `clipboard`, `process`, `os` or `path` permission. | `capabilities/default.json`, `commands.rs` (36 `#[tauri::command]`) |

**Consequence.** CONFIG-02C is *not* "the first time the application reads a real `.mcp.json`". It is a request for a **different, narrower-input, richer-output, stricter-origin** read with its own consent text and its own typed result. The authorization request must therefore say what is **new** (a dedicated command, a digest, a closed status, handle-relative origin semantics, no persistence) and must not borrow legitimacy from the existing scan. It must also not leave two readers with contradictory verdicts unexplained (§11, open item O1).

## 1. Scope — exactly one candidate

| | |
|---|---|
| Host / surface | Claude Code terminal (`claude-code-terminal`; host id `claude-code-project-mcp`) |
| File | the constant relative name `.mcp.json`, **directly inside** the selected project root. No parameter carries a file name or a path below the root |
| Scope label | project scope (shared, committed with the repository). Never user scope (`~/.claude.json`) or local scope |
| Maximum bytes | **1,048,576** (1 MiB) — the same cap as `MAX_SMALL_BYTES` and the 02B `MAX_FILE_BYTES`, so a file the reader accepts is never one a later writer must refuse for size |
| Locality | local filesystem only; the application opens no connection |
| Out of scope (separate later work) | Antigravity `.agents/mcp_config.json`; Codex `.codex/config.toml`; `~/.claude.json`; `.claude/settings*.json`; `CLAUDE.md`; any other file; any directory listing shown to the user; any write |

## 2. Proposed user journey (nothing below exists)

1. **Project.** The user is looking at a project row in the matrix with surface *Claude Code (terminal)*. The alias is metadata. It is shown, never resolved.
2. **Opt in.** In the existing "Yapılandırma önizlemesi" panel a **collapsed, default-off** section "Dosyadan oku (isteğe bağlı)" is offered. The pasted-text preview above it is unchanged and fully usable whether or not the section is ever opened (§9).
3. **Root.** The user supplies the project's real directory (first release: typed absolute path, same convention as the Inventory scan; a native picker is a **separate** decision, see the authorization request D2). The text is held only in component state and **cleared as soon as the call returns**, success or failure.
4. **Gate.** The application states the exact file `.mcp.json`, scope, maximum 1 MiB, local-only behaviour, what comes back and what never does, and that nothing is stored (Turkish text in the authorization request, §4). Two buttons: *read this file once* / *No, I will paste the text*. No default focus on the read button; no "remember my choice".
5. **One read.** One click performs one bounded read (§5) and returns one typed result (§3).
6. **Result.** Rendered from closed vocabularies (§3, §7). Missing/denied/unsafe never read as "not configured" or "verified".
7. **Clear.** The result and the typed path are removed on: *Temizle*, unmount/navigation (`App.tsx` remounts pages through `ErrorBoundary key={page}`), a change of project/OS/surface/catalog entry, a new read, and a 5-minute client timer. The UI never writes the result to storage, clipboard, logs or the DB.

## 3. Output contract (what may cross IPC)

### 3.1 Request (design)

```
ReadMcpConfigRequest {            // serde(deny_unknown_fields); NO field names a file, a surface, a platform or a path below the root
  alias:     string,              // validated by projects::valid_alias and projects::is_registered; kind must be "reference"
  directory: string,              // the user's typed project root; used for this call only
  consent:   "config02c-read-once-v1"   // constant; coherence check, NOT a security boundary against a compromised WebView
}
```

### 3.2 Response (design) — the complete list of fields

```
ReadMcpConfigResult {
  schema:   "config02c.read.v1",
  status:   Status,                       // closed set, §3.3
  reason:   Reason | null,                // closed set per status, §3.3
  alias:    string,                       // echo of the validated alias
  file:     ".mcp.json",  scope: "project",  surface: "claude-code-terminal",  maxBytes: 1048576,   // constants
  sizeBytes: u64 | null,                  // only when the whole file was read
  sha256:    hex64 | null,                // SHA-256 of exactly the bytes read; only when sizeBytes is set
  serverCount:       u32 | null,          // number of members of mcpServers; only for present_summarized
  reviewedDeclarations: [ { id, nameState, endpointState } ] | null,   // closed vocabulary below; only for present_summarized
  notClaims: ["not_loaded","not_trusted","not_installed","not_enabled","not_reachable","not_verified"]   // constant
}
nameState     ∈ { "absent", "present", "present_other_letter_case" }
endpointState ∈ { "absent", "present_under_this_name", "present_under_other_name", "not_evaluable" }
```

`reviewedDeclarations` is computed **in Rust from Rust-owned constants** that mirror the CONFIG-02A declaration (`exa-mcp-server`: server name `exa`, endpoint identity `https://mcp.exa.ai/mcp`). The UI never supplies which declarations to look for. The only strings in the response are: the constants above, the alias the caller already holds, the closed enum values, the declaration id, and a 64-character hex digest. **No user-chosen text from the file is returned: no server names other than the reviewed one, no commands, arguments, URLs, headers, env keys or values, no unknown field, no excerpt, no line numbers, no error text from the parser.**

### 3.3 Status vocabulary (closed) and the total mapping

`not_requested` exists **only in the UI** (initial state); the backend never emits it.

| Status | Meaning (and what it does NOT mean) | Reasons |
|---|---|---|
| `root_not_found` | The typed directory does not exist. **Not** "no file". Distinct from `absent` so a typo can never read as "nothing configured" | – |
| `absent` | The root was opened and **no entry named `.mcp.json` existed at that moment**. **Not** "MCP is not configured" (other scopes exist) and not "will stay absent" | – |
| `present_summarized` | Regular file, one link, within the cap, strict-parse OK, **and** the summary below is defensible | – |
| `present_metadata_only` | Regular file, read in full; size + SHA-256 only. The content is **not** shown or summarised. Strictness here is the application's, **not** Claude Code's: this is not a statement that the file is invalid for Claude Code | `duplicate_key`, `byte_order_mark`, `root_not_object`, `servers_not_object`, `too_deep`, `too_many_servers` |
| `malformed` | Read in full but not valid UTF-8 JSON text (size + SHA-256 only) | `invalid_utf8`, `invalid_json`, `lone_surrogate`, `empty_file` |
| `denied` | Operating system refused access, or another process holds it. Whether the file exists is **unknown** | `access_denied`, `locked` |
| `unsafe_path` | A link/junction/reparse point, a non-directory root, a non-regular target, a hard-linked target or a case-variant sibling was found; nothing was read | `link`, `not_a_directory`, `not_regular_file`, `hard_linked`, `case_alias`, `invalid_root_text`, `root_too_broad` |
| `unsupported_fs` | Network, remote, unknown or cross-volume filesystem, or a platform without the needed primitive. No fallback to an ordinary open | – |
| `too_large` | More than 1 MiB; nothing beyond the cap was read; no digest | – |
| `changed_during_read` | Identity, size, modification time or a second-pass digest differed; **no result of any kind** is returned | – |
| `io_error` | Anything else; no detail | – |

The mapping from the existing `FsError` vocabulary must stay **total**; `tests/test_config02c_contract.py` fails if a code is added to `managed_fs.rs` without a row here.

| `FsError::code()` | Stage | Status / reason |
|---|---|---|
| `not_found` | opening the root | `root_not_found` |
| `not_found` | the entry `.mcp.json` (stat returns nothing) | `absent` |
| `exists` | creation only — unreachable in a read | `io_error` |
| `link_refused` | any | `unsafe_path` / `link` |
| `not_a_directory` | root | `unsafe_path` / `not_a_directory` |
| `not_a_regular_file` | target | `unsafe_path` / `not_regular_file` |
| `other_volume` | any | `unsupported_fs` |
| `network_or_unknown_filesystem` | root | `unsupported_fs` |
| `access_denied` | any | `denied` / `access_denied` |
| `locked_by_another_process` | any | `denied` / `locked` |
| `too_large` | target | `too_large` |
| `invalid_name` | unreachable — the name is a constant | `io_error` |
| `not_empty` | removal only — unreachable in a read | `io_error` |
| `changed` | any | `changed_during_read` |
| `unsupported` | any | `unsupported_fs` |
| `io` | any | `io_error` |

Not an `FsError`, decided by the reader: hard-link count ≠ 1 → `unsafe_path`/`hard_linked`; case-variant sibling → `unsafe_path`/`case_alias`; size 0 → `malformed`/`empty_file`.

### 3.4 What is "defensible" to summarise

Summarise (`present_summarized`) only when **all** hold: strict parse succeeded (no BOM, valid UTF-8, no lone surrogate, **no duplicate key at any depth**, nesting ≤ 64, root is an object); `mcpServers` is absent or an object with ≤ 256 members. Otherwise `present_metadata_only`.

* An **absent `mcpServers`** is `present_summarized` with `serverCount = 0` — the file exists and declares no server.
* Computing `endpointState` reads a server's `url` string **only to compare it, inside Rust**, with the reviewed endpoint identity (scheme + host + path, case/port/query/fragment/userinfo/trailing-slash normalised — the rule of the 02B `url_identity`, to be re-implemented and re-reviewed, not imported). The URL is never returned, logged or stored. A value that is not a string, or a server entry that is not an object, is `not_evaluable`, never an error echo.
* Everything else in the file — including unknown top-level keys, other servers' names and values — is neither returned nor used. **If unknown content cannot be summarised without returning user text, the result is metadata-only.** Server names other than the reviewed one are *not* returned in this contract (an optional, separate, later decision; see the authorization request D4).

## 4. Trust boundaries and exact on-disk origin semantics

| Boundary | Rule |
|---|---|
| WebView → Rust | The WebView is semi-trusted input. It supplies an alias, one directory text and a constant. It cannot name a file. It never receives a path, an excerpt or free text from the file. The consent constant is a coherence check; the actual boundary is the fixed command surface + CSP + the closed result type |
| Alias ↔ directory | **The pairing is a user assertion**, not a verified fact: nothing proves the typed folder is "the project's". The result is labelled with the alias and the fact that *the folder the user typed* was read, is never used to change a preference, and is never persisted. See §10 |
| Root | Opened by walking the chain from the volume root **without following any link** (`NtCreateFile`+`FILE_OPEN_REPARSE_POINT` / `openat(O_NOFOLLOW\|O_DIRECTORY)`); ancestors may not be links (a project reached via `~/code → /elsewhere` is refused — conservative and documented); root must be on a local, recognised filesystem; drive/filesystem roots are refused. The identity (volume+object / dev+ino) is taken **from the open handle** and kept only for the duration of the call |
| Target | Addressed **relative to the root handle** by the compile-time constant `.mcp.json`; no path is resolved a second time. Must be a regular file with **exactly one hard link**; must not be a link, junction, directory or special file; **no sibling whose name equals `.mcp.json` ignoring ASCII case** may exist (ambiguity is refused rather than guessed). Alternate data streams and 8.3 aliases cannot be requested: no part of the name comes from input |
| Size/format | ≤ 1 MiB read through the handle with `take(cap+1)`; a larger file is `too_large`, never truncated. UTF-8 only; strict JSON with limits (§3.4) |
| Race | Identity re-read from the opened handle must equal the identity of the pre-open stat; after the read the handle's identity/size/modification time are re-read and the file is read **a second time** and hashed again; any difference ⇒ `changed_during_read`. A final stat by name must still show the same identity. **Residual (not hidden):** a writer that restores size and modification time and replaces content inside the window cannot be detected; the digest describes *the bytes read at that moment*, not the file afterwards |
| Locality | Network, remote, unknown filesystems and cross-volume steps ⇒ `unsupported_fs`; macOS volumes where the needed primitive is missing ⇒ `unsupported_fs`, no fallback |
| Egress | No logging of path, bytes or parse errors; no `diagnostics::log`, no audit event, no DB write. Rust error values carry a code and nothing else |

**Read authority is not write authority.** This contract grants nothing toward a write: a successful read does not enable any Apply control, authorise any later write, or satisfy `docs/config-01/FUTURE_WRITE_GATE.md`. In particular, the write gate's §2 (native chooser + **stored** root identity) is *not* met by a per-call typed path. The 02B Windows silent lost-update, the missing lock, the missing recovery journal and the missing permission/ACL preservation remain a hard NO-GO for any write regardless of how this read evolves. A digest from this read must never be accepted as the "expected prior state" of a write without that write phase re-reading under its own authority.

## 5. The single read, step by step (design condition)

1. Reject any request whose alias is not a registered `reference` project, whose consent constant differs, or whose directory text fails the text checks of `validate_project_dir` (absolute, ≤ 260 chars, no control characters, no `.`/`..`, not UNC/device, not a filesystem root). Nothing has been opened.
2. Open the root handle (`Dir::open_anchor`). Map errors per §3.3.
3. List the root (bounded to `MAX_LIST_ENTRIES`); a name ≠ `.mcp.json` that equals it ignoring ASCII case ⇒ `unsafe_path`/`case_alias`. (The list is **not** returned or stored.)
4. `stat` the constant name without following. None ⇒ `absent`. Link ⇒ `unsafe_path`/`link`. Not a regular file ⇒ `unsafe_path`/`not_regular_file`. Link count ≠ 1 ⇒ `unsafe_path`/`hard_linked`.
5. Open the file relative to the root handle, no follow; re-read kind/identity/links from the handle; any mismatch ⇒ `changed_during_read`.
6. Read ≤ 1 MiB + 1; over ⇒ `too_large`. Hash. Re-read identity/size/mtime from the handle, read and hash again, compare; mismatch ⇒ `changed_during_read`. Final stat by name; mismatch ⇒ `changed_during_read`.
7. Strict-parse the bytes; classify (§3.4); build the response from constants and counters only. Drop the bytes.
8. Close all handles. Nothing is retained: no cache, no memo, no token valid for a later call.

## 6. Reuse of existing primitives — what exists and what is missing

| Need | Exists | Verdict |
|---|---|---|
| Handle-relative root walk, no-follow, local-FS check, identity from handle | `managed_fs::Dir::open_anchor` (+ `imp::*`) | **Reuse.** Not on any production path today (F6) |
| Constant name `.mcp.json` | `managed_fs_mcp.rs` addresses `imp` directly because `valid_name` rejects a leading dot | **Do not widen `valid_name`.** A new *read-only* sibling child-module of `managed_fs`, with only the read half of `ProjectRoot::inspect`, is the smallest change. Not a copy of the 02B module: no `commit`, `rollback`, staging or backup code, no `FileH::write_all`, `create_*`, `remove_*`, `move_file` |
| Bounded read + SHA-256 from a handle | `FileH::read_bounded`, `sha256_bounded` | Reuse |
| Link count, case-alias refusal | 02B `inspect` (test-only) | Re-implement in the read-only module; re-review |
| Strict JSON (no BOM, no duplicate key, depth limit) | `config_txn::parse_strict` — **test-only, not promoted** | **Missing in production.** `serde_json` is already a dependency; a duplicate-key-detecting visitor over `serde_json::Deserializer` (it already rejects a BOM, lone surrogates and > 128 nesting) is the smallest route. Needs its own tests; no new crate |
| Reviewed-declaration comparison (name, endpoint identity) | `config_txn::url_identity` and the TS twin — test-only / TS | **Missing in Rust production.** Small, pure, must be re-implemented and tested against the shared golden data that 02B already keeps |
| Typed result with closed enums | none | **Missing** |
| Consent coherence constant | `SCAN_NOT_AUTHORIZED` pattern in `discovery::authorize` | Pattern exists |
| A permission identifier for one new typed command | the 36-command allowlist pattern | One new `allow-…` entry; **no plugin permission** |
| Native folder picker returning a handle | **none** (no dialog plugin) | Not needed for the first step; a separate, later decision |
| Persisted project root / identity | **none** (F3) | Not needed for the first step; required before any write (FUTURE_WRITE_GATE §2) |
| Reader visibility of skips in CI | **none** (F8) | **Missing**; see the test plan §0 |

Using `managed_fs` for a **read** does not touch the deletion paths that Issue #14 is about and does **not** resolve #14; the implementation approval must say so explicitly and #14 stays open.

## 7. What may be displayed (UI contract)

* Only the closed-vocabulary result of §3.2, rendered as **React text nodes** (no `dangerouslySetInnerHTML`, no Markdown/HTML rendering path exists for it). The only non-constant strings are the alias, the declaration id and the digest.
* Digest shown as the first 16 hex digits plus "…", with the sentence that it identifies the bytes read, not their origin.
* **Pasted-text bridge (the user-visible gain without raw text over IPC):** if the user has also pasted text, the UI compares `sha256Hex(pasted text)` (already computed) with the file's `sha256`: equal ⇒ "Yapıştırdığınız metin bu dosyayla bayt bayt aynı." Not equal ⇒ "Özetler farklı; metnin bu dosyayla aynı olduğu kanıtlanamadı (satır sonu veya kodlama farkı olabilir)." Equality proves identical bytes; inequality proves nothing else. The comparison is computed at display time and never stored.
* Always-visible statements: *okuma anındaki anlık görüntüdür*; *dosya değişmiş olabilir*; the six `notClaims` rendered as one sentence ("yüklendiğini, güvenildiğini, kurulduğunu, etkin olduğunu, erişilebildiğini veya doğrulandığını göstermez"); that the user may still see the file in their own editor.
* Forbidden anywhere in this UI: "doğrulandı/verified", "yüklü/loaded", "güvenli/safe", "çalışıyor", "bağlı/connected", "yapılandırılmamış/not configured" derived from `absent`/`denied`/`unsafe_path`/`root_not_found`.

## 8. State the application may keep

None persisted. In memory: the result object while the panel is mounted (§2 step 7). Not kept anywhere: the typed directory after the call, file bytes, the parsed document, the directory listing, the identity. `ProjectView`, preferences, catalog acceptance, install states, review-queue counts, audit chain, diagnostics and settings are unchanged by any outcome.

## 9. Saying NO

The read is optional in three independent ways: the owner may decline the implementation (§ authorization request); a user may never open the section; a user may click *No, I will paste the text* at the gate. In every case the existing pasted-text preview (`ConfigPreview`, `planConfigPreview`) behaves exactly as today. A refused or failed read **never** blocks, changes or pre-fills the preview.

## 10. Binding a selected root to a project without silently persisting a path

Three options were considered:

| Option | Persisted | Verdict |
|---|---|---|
| **A. Per-call pairing** (alias + typed directory in one request; nothing remembered) | nothing | **Proposed first step.** Honest about the user-asserted pairing; no migration |
| B. In-process map alias → root identity for the session | identity in memory only | Adds "same root as last time" (`root_changed`) without disk state. Optional refinement; not needed for a one-shot read |
| C. Registry column holding a root identity (volume+object / dev+ino), set through a native chooser | identity on disk (needs a migration) | **Required before any write**, not needed for a read. A path string is never stored; an identity is a device-specific identifier and must be shown to the user as stored |

## 11. Open items for the independent review (not resolved here)

* **O1 — two readers, two verdicts.** The existing scan lenient-parses (BOM stripped, last duplicate wins) and says `Configured`/`NotConfigured`; this contract is strict and may say `present_metadata_only` for the same file. The UI must not show them as contradictory; the review should decide whether the Inventory sentence for a *missing* `.mcp.json` ("NotConfigured, Medium") should be corrected in a separate change (F5). Not changed here.
* **O2 — `managed_fs` on a production path.** First production use of the layer, read-only. The review decides whether that is acceptable while #14 is open, or whether the read should extend `fsx` instead (weaker on Windows: F4).
* **O3 — typed path versus picker.** The write gate wants a chooser; a read does not. Confirm that a typed path is acceptable for the *read* only.
* **O4 — server names.** Whether a later step may return user-chosen server names under a strict grammar (authorization request D4).
* **O5 — platform evidence.** Everything about macOS behaviour is design until a macOS CI run executes it; local evidence here is Windows only, and the local Rust test binary is currently blocked by Smart App Control (see the test plan §1).
