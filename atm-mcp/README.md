# atm-mcp (S1 prototype): read-only stdio MCP server

**Status: S1_NOT_COMPILED / T01–T25_NOT_RUN.** Author source, never built on the author's machine (Windows App Control blocked
build scripts, os error 4551). Not independently reviewed. Not accepted. This is a standalone package: not a workspace member, no `src-tauri`
dependency, no GUI.

It is an interface proof only. It does **not** provide real ATM install state, a database, or a real ECE verification, and it does not close #60.

## What it does
A locally spawned stdio child. One JSON-RPC message per stdout line. It embeds the public catalog (`data/catalog-*.json`, 24 entries) with
`include_str!` and exposes four closed, versioned, read-only tools:

| Tool | Result |
|---|---|
| `atm_catalog_search_v1` | `query` (1–64 chars: letters, digits, space, `.`, `-`, `_`), optional `kind`, `limit` 1–10. Sorted by id, at most 10. |
| `atm_catalog_get_v1` | Exact id (`^[a-z0-9][a-z0-9-]{0,63}$`). Allowlisted metadata only. |
| `atm_runtime_status_v1` | Static capability facts. No path, user, environment, process or live ATM state. |
| `atm_checkpoint_summary_v1` | Validates commit/run/attempt; always `NOT_ASSESSED` + `LIVE_VERIFICATION_NOT_AVAILABLE_IN_MCP_V1`. No ECE, no network. |

Every result has three separate layers: `sourceDeclaration: PRESENT`, `machineObservation: NOT_OBSERVED`, `humanDecision: NOT_RECORDED`.
No value means accepted, installed, verified, reviewed or authorized. Caller input is never echoed. Output caps: 16 / 8 / 2 / 2 KiB.
Arguments are limited to 1 KiB. `readOnlyHint` etc. are advisory host hints, not operating-system protection.

Protocols: 2026-07-28, 2025-11-25, 2025-06-18 (via `rmcp =3.9.0`, features `server` + `transport-io` only). Handshake, `server/discover`,
era selection and dispatch are rmcp's. See `GUARD_AUDIT.md` for the wire guard that adds duplicate-key rejection and a 16 KiB frame cap.

## Not in this package
No listener, HTTP, SSE, auto-start, GUI, database, file read or write, environment read, child process, ambient credential, ECE call,
`.mcp.json` reader, install/remove, host configuration write.

## Files outside `atm-mcp/` that it reads (compile-time inputs; never edited)
`data/catalog-seed.json`, `data/catalog-candidates-{1b,c01,c02,c03,c04,c05}.json` (embedded), and for parity tests only:
`src-tauri/src/catalog.rs`, `src-tauri/src/ece_ci.rs`, `src-tauri/src/ece_ci_live.rs`, `.github/ece/atm-ci-two-runner-v1.json`.
Relative paths must stay exactly as in the repository (`include_str!("../../...")`).

## Test-to-case map (T01–T25 of the Phase 0 report)
| Case | Test(s) | Where |
|---|---|---|
| T01 modern discover | `t01_modern_discover` | tests/protocol.rs |
| T02 modern list/call, no discover | `t02_modern_list_and_call_without_discover` | protocol.rs |
| T03 legacy 2025-06-18 (Codex default) | `t03_legacy_initialize_2025_06_18_codex_default` | protocol.rs |
| T04 legacy 2025-11-25 | `t04_legacy_initialize_2025_11_25` | protocol.rs |
| T05 legacy 2024-11-05 (expects 2025-11-25) | `t05_legacy_initialize_2024_11_05_is_answered_with_2025_11_25` | protocol.rs |
| T06 `-32022` | `t06_modern_unsupported_version_gets_minus_32022` | protocol.rs |
| T07 `-32602` missing/partial `_meta` | `t07_modern_request_with_missing_or_partial_meta_is_invalid_params` | protocol.rs |
| T08 era mixing | `t08a_...`, `t08b_modern_request_then_initialize_then_requests`, `t08c_initialize_then_modern_request_with_complete_and_partial_meta` (expected outcomes cited from rmcp source) | protocol.rs |
| T09 malformed/odd frames | `t09_malformed_and_odd_frames_do_not_disturb_the_process` | protocol.rs |
| T10 duplicate keys at the wire | `t10_duplicate_keys_at_envelope_and_nested_arguments` (raw bytes, escapes built at run time), `t10b_rejected_frames_in_a_burst_with_eof_are_answered_exactly_once`; unit tests in `src/guard.rs` (escaped/plain/PASS, cancellation, short write, EOF, oversized) | protocol.rs, guard.rs |
| T11 huge / deep frames | `t11_oversized_deep_and_huge_frames` | protocol.rs |
| T12 unknown tool / case / extra keys | `t12_unknown_tool_case_variant_and_extra_argument_keys` | protocol.rs |
| T13 invalid types and ranges | `t13_invalid_types_and_ranges` | tests/tools.rs |
| T14 unknown id | `t14_unknown_catalog_id` | protocol.rs |
| T15 poisoned text | `t15_poisoned_text_is_cleaned_capped_and_labeled` | tools.rs |
| T16 fake PASS | `t16_fake_pass_fields_are_refused_and_never_echoed` (wire), `t13` (unit) | protocol.rs |
| T17 checkpoint NOT_ASSESSED | `t17_checkpoint_is_always_not_assessed` | protocol.rs |
| T18 no positive trust | `t18_no_positive_trust_value_in_any_output` | tools.rs |
| T19 secret/path redaction | `t19_environment_secret_and_working_paths_never_appear_in_output` | tests/process.rs |
| T20 stdout purity | `t20_stdout_is_only_json_rpc_and_stderr_is_quiet`, `t20b_stdout_discipline...` (static) | process.rs, source_scan.rs |
| T21 EOF/repeat/cancel | `t21a`, `t21b1`–`t21b3` (unfinished frame via `send_raw_no_newline`, valid frame without newline, finished malformed line), `t21c`–`t21e` | process.rs |
| T22 no writes | `t22_server_writes_nothing...`, `t22b_...`, `t22c_no_forbidden_apis...` (static) | process.rs, source_scan.rs |
| T23 parity | four `t23_*` tests | tests/parity.rs |
| T24 deterministic + caps | `t24_deterministic_and_within_size_caps` | tools.rs |
| T25 repository boundary | `#[ignore]` placeholder; checked with `git status`/`cargo metadata` in the worktree | source_scan.rs |

Additional (P3 output schemas, NOT_RUN): `output_schema_accepts_every_real_success_output`, `output_schema_accepts_every_real_error_output_and_every_closed_error_shape`, `output_schema_rejects_representative_wrong_shapes`, `output_schemas_are_closed_object_schemas_with_a_success_and_an_error_alternative`, `output_schema_bounds_hold_for_the_embedded_catalog`, `input_schemas_are_unchanged_by_the_output_schema_work` (all in tests/tools.rs). Also: `status_build_target_requires_windows_x64_msvc`, `build_target_cfg_requires_target_env_msvc`, `deprecated_rmcp_server_info_alias_is_not_used`, `turkish_queries_work...`, `utf8_turkish_query_travels...` (Unicode/Windows encoding), manifest/lock checks, guard structure checks.

## Output schemas (P3, source-only): declarations, not proof
- Each tool declares a closed `outputSchema`: the root is an object schema with exactly two alternatives (`oneOf`: the success shape, or the
  closed error shape `atm.mcp.error.v1`), and every nested object has `additionalProperties: false`. Types, constants, enums, patterns,
  `maxLength`, `maxItems` and nullable fields were written from the constructors in `src/tools.rs` (`search`, `get`, `status`, `checkpoint`,
  `error_value`), not from the earlier schema. `humanPath` (checkpoint), `repository` and `sourceSnapshot` (get) are now listed as required
  because the handlers always emit them. No field is declared that a handler does not emit.
- A schema is a **declaration for the host**. `rmcp` does not validate server output against it, and this server does not validate its own
  output at run time. Input arguments are still checked explicitly in `src/tools.rs` (`check_args`, `parse_query`, ...) because the pinned
  SDK's default tool lookup can bypass input-schema validation. The four tool names, input schemas and result meanings are unchanged.
- Two bounds are not enforced by the runtime and hold only for the embedded data: `compatibility.maxItems = 8` (platforms x surfaces; the data
  has at most 6 rows per entry and no duplicate pair) and `limitations.maxItems = 7`. A test asserts both against all 24 entries.
- Tests (`output_schema_*` in `tests/tools.rs`): real outputs of all four tools, and every error code with every fixed field name, are checked
  against the declared schemas with a small in-test validator (no new dependency; it panics on any keyword or pattern it does not implement),
  and 34 wrong shapes must be rejected. **NOT_RUN** until built.
- Not claimed: that any host validates or uses these schemas, or that a schema match shows the output is true.

## Output size: what is limited and what is not (P3, source-only)
- `dispatch` measures the compact serialized **structured JSON** of a result and refuses it above 16 / 8 / 2 / 2 KiB
  (search / get / status / checkpoint). That is the only output limit.
- The SDK then also copies the same JSON as text into `content[0].text` (INFERRED from reading `rmcp 3.9.0` `CallToolResult::structured`;
  NOT_RUN). The copy is a JSON string, so every `"` and backslash in it is escaped again. Nothing limits the **whole serialized JSON-RPC
  response envelope**, the copy, or the `tools/list` reply. The input frame cap (16 KiB) and the 1 KiB argument cap do not limit output.
- Source-only upper bound, **INFERRED / NOT_MEASURED**: `wire <= S + 2*S + E + I`
  - `S` = the structured cap of the tool; the copy is `<= 2*S` because escaping at most doubles a byte (loose: real JSON is far from all quotes);
  - `E` = fixed envelope (`jsonrpc`, `resultType`, `isError`, key names), ASSUMED `<= 300` bytes;
  - `I` = the request `id` echoed back, `<= 16384` bytes because a frame above 16 KiB is dropped before parsing (ASSUMED: re-encoding is not longer).

  | Tool | S | 3*S | + E + I | Bound (bytes) |
  |---|---|---|---|---|
  | `atm_catalog_search_v1` | 16384 | 49152 | 16684 | 65836 |
  | `atm_catalog_get_v1` | 8192 | 24576 | 16684 | 41260 |
  | `atm_runtime_status_v1` | 2048 | 6144 | 16684 | 22828 |
  | `atm_checkpoint_summary_v1` | 2048 | 6144 | 16684 | 22828 |

  These are not measured maxima, not a memory-safety claim and not a tested limit. The `tools/list` size is not derived here; it grew with the
  closed schemas. **Required later:** measure the raw stdout bytes per line (largest `search`, `get`, `tools/list`, and an error with a 16 KiB
  `id`) on the isolated Windows runner. If a hard whole-envelope limit is wanted, that is a functional change: DESIGN_DECISION_REQUIRED.

## Escape notation: three different things (P3, source-only)
An earlier author note said that no escaped Unicode literal exists in the package. That was **false** and is withdrawn. Three levels must stay apart:
1. **Source literal.** `tests/process.rs` contains the six characters `\u00fc` twice: in a comment and inside the raw string
   `r#"..."#` that holds `G\u00fcvenlik`. In a Rust raw string the backslash is kept as written. This is intended.
2. **Decoded JSON.** The server's JSON parser turns that escape into the single character U+00FC. The test (`utf8_turkish_query_...`) expects the
   result to equal the result of the query typed as the real character.
3. **Wire bytes.** The pipe carries the six ASCII bytes of the escape (the harness writes the source string unchanged), not the UTF-8 pair `C3 BC`.

The narrower, correct claim: the **T10 and guard-unit-test fixtures** (`tests/protocol.rs`, `src/guard.rs`) do not write a Unicode escape as a source literal.
They build the backslash at run time (`char::from(92u8)`) and assert at byte level that the escaped and the plain fixture differ, because a writing
tool once turned such literals into plain letters (so two "different" fixtures became identical). Other files may legitimately contain a source literal.

## Alternate-host run (ONLY on an isolated, disposable Windows x64 host whose policy allows temporary build scripts)
The source ZIP contains **no crate archives and no vendor directory**. `--offline --locked` therefore needs a crate cache that already
exists on that host. There is no network assumption anywhere in this procedure.

1. Verify the ZIP against `ARCHIVE.sha256` and the extracted files against `MANIFEST.sha256` BEFORE any build.
2. Prepare the dependency source. Exactly one of these, decided by the owner before the run:
   - a pre-existing Cargo registry cache that already holds the 64 crates the lock needs for `x86_64-pc-windows-msvc`, whose archive
     checksums were verified against `Cargo.lock` (the 87 checksums are in `DEPENDENCY_PROVENANCE.md`); or
   - a separately approved offline cache/vendor transfer, verified the same way. That transfer is a new owner decision; it is not part of S1.
   If neither exists, STOP and report BLOCKED. Do not retry online, do not fetch "just the missing crate", do not edit Cargo config.
3. In a fresh directory with no profile, token or private mount, record `rustc -V` and `cargo -V`. 1.99.1 does not test the 1.90 MSRV.
4. `cd atm-mcp`, then, all with `--offline --locked`. The metadata and tree commands MUST name the target, because 23 of the 87 packages
   are target-specific (7 are not in any cache that was filled for Windows x64 only) and an unfiltered command fails offline:
   - `cargo metadata --offline --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc`
   - `cargo tree --offline --locked --target x86_64-pc-windows-msvc -e features`
   - `cargo fmt --check`
   - `cargo clippy --offline --locked --target x86_64-pc-windows-msvc --all-targets -- -D warnings`
   - `cargo test --offline --locked --target x86_64-pc-windows-msvc`
   A different target needs crates this cache may not hold (see `DEPENDENCY_PROVENANCE.md`): STOP, do not fetch.
5. Record each test by name as PASS / FAIL / BLOCKED / SKIP / NOT_RUN. A count is not a result. `t25_...` is `#[ignore]` and must be reported as
   NOT_RUN by cargo; T25 is proven separately in the real worktree (`git status`, `git diff --name-only HEAD`, `cargo metadata --no-deps` of
   `src-tauri`), never as a cargo PASS.
6. Optional OS observation (not done by the tests): process, TCP and file-write monitoring of `atm-mcp.exe` during `cargo test`.

## Observation limits (stated so they are not mistaken for proof)
- The process tests observe stdout, stderr, exit code and the probe directories. They do **not** observe sockets, child processes,
  registry access or the Windows App Control behavior of the final executable.
- The source scan is a supportive check, not a guarantee. `tokio` is built with `net` (via rmcp `transport-io`), which pulls `mio` and
  `socket2`; being compiled in does not show it is ever invoked.
- Cancellation test `t21d` is weak: handlers finish instantly, so a real in-flight cancel cannot be staged from outside.
- Tests assert SDK behavior that was read from `rmcp 3.9.0` source but never run (T05, T08a-c cite the exact source lines in comments). Those
  expectations are INFERRED until a real run.
- No actual Claude Code or Codex host was used. Host behavior is UNKNOWN.
