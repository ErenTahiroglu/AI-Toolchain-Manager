# guard.rs audit (author's own, source reading only)

Status: **S1_NOT_COMPILED**. Nothing here was run. "Compared with the SDK" means compared by reading `rmcp 3.9.0` source text
(`src/transport/async_rw.rs`, `src/transport.rs`, `src/service/server.rs`). This is author evidence, not independent review, and it does
not show correctness.

## Why a transport shim exists
rmcp parses each stdin line with `serde_json` straight into typed messages. JSON objects become maps, so a duplicate key (in the envelope or
inside `arguments`) is collapsed before any `ServerHandler` code runs. The SDK's own line cap defaults to 16 MiB. Neither can be changed
from inside a handler. The public `rmcp::transport::Transport` trait is the SDK's supported extension point.

## Where each control sits (line numbers are from bundle v1 and are approximate after R1; search for the named identifiers)
| Concern | Location | Behavior |
|---|---|---|
| Raw bytes are read | `read_frame` 158–190: `self.read.fill_buf()` (line 160) on a `tokio::io::BufReader` over stdin | Only here. The SDK's reader is replaced by `tokio::io::empty()` (line 152). |
| 16 KiB cap before allocation and before JSON parse | `MAX_FRAME_BYTES` (27); check at 173 happens BEFORE `extend_from_slice` (180) | An oversized frame is never buffered: `self.line` is cleared, the rest of the line is consumed and discarded in chunks (`discarding`, 166–170, 177). No reply is sent (the id cannot be trusted). Nothing is parsed. |
| Escaping and duplicate-key comparison | `Strict::visit_map` 79–92: `a.next_key::<String>()` then `HashSet::insert` | Keys come from serde_json's own parser, i.e. AFTER JSON unescaping, so `"id"` and `"id"` collide. Detected at any depth (envelope, `params`, `_meta`, `arguments`). |
| Nesting depth | `MAX_DEPTH = 32` (29), checked in `visit_seq`/`visit_map` (73, 80); serde_json's own 128 limit also applies | Over 32 is `Reject::TooDeep`. |
| Trailing bytes / not JSON | `check_frame` 96–112 uses `de.end()` | A second JSON value after the first, invalid UTF-8 and syntax errors are `Reject::NotJson`. |
| Errors and request id | `readable_id` 121; rejection at 216–221 and 228–231 | Fixed `-32600 "Invalid request"` carrying the request id ONLY when the frame has exactly one readable `id` that is a string or integer. A duplicated `id`, `null` id, non-object or a frame too deep for serde_json to skip yields no reply. Notifications get no reply. |
| Typed parse | 224: `serde_json::from_slice::<RxJsonRpcMessage<RoleServer>>` | The SDK's own message type; the SDK, not the shim, then dispatches. |
| Client responses | 226 | A JSON-RPC response or error from the client is dropped. This server never sends requests, so a client response answers nothing. |
| Cancellation, short writes (R1) | `receive` is polled inside the SDK's `select!`. `read_frame` keeps partial bytes in `self.line` and `fill_buf` consumes nothing when dropped. A rejection reply is popped ONCE from `pending`, turned into a send future stored in `self.inflight`, and driven to completion at the top of `receive`; the future is cleared in the same poll in which it completes. A dropped `receive` therefore leaves it stored and the next call RESUMES it (the writer's partial-write state lives inside the future), so one reply is never encoded twice. | Replaces the earlier `front().cloned()` + pop-after-send design, which could encode one reply twice if the send was dropped between buffering and flushing. Unit tests: `rejected_frame_reply_is_written_exactly_once_under_cancellation_and_short_writes` (3-byte, alternately pending writer), `eof_right_after_a_rejected_frame_still_flushes_its_reply`. NOT_RUN. |
| EOF | `fill_buf` returns empty → `Ok(Some(false))` (162) → `receive` returns `None` (209) | The SDK ends the service, `main` exits. A partial trailing line (no `\n`) is dropped. Queued rejection replies are flushed first (loop top, 202). |
| Output | `send` 196 delegates to the SDK's `AsyncRwTransport::send` | One compact JSON line per message by the SDK's codec (INFERRED from source; NOT_RUN). The shim writes nothing else to stdout. |

## What the shim does NOT do
- It does not replace the handshake, `server/discover`, era selection, version negotiation or dispatch. It never looks at `method` or `_meta`.
- A request with missing or wrong version metadata passes through unchanged and is refused by the SDK (tests T06, T07).
- It writes no non-JSON-RPC text to stdout.

## Differences from the SDK's own `AsyncRwTransport` (each one is a decision for the reviewer)
1. Duplicate keys and nesting > 32 are rejected at the wire (the SDK accepts them). New behavior by design.
2. Frame cap is 16 KiB (SDK: 16 MiB).
3. Unparsable-but-shaped frames: the SDK answers `invalid_request` with `id: null`; the shim answers with the real id when readable, else drops.
4. The SDK silently ignores some unknown notifications via `should_ignore_notification`; the shim drops any frame that does not parse and has no readable id (same observable result).
5. Client responses are dropped.
6. The SDK has `with_max_line_length`; the shim does not use it because it must read the line itself to check it. The supported alternative
   (SDK transport with its own cap) cannot detect duplicate keys, so it would leave caveat 1 open.

## Known weaknesses (not hidden)
- Rejections of an oversized frame are silent, so a client sees a timeout rather than an error.
- Residual: a reply stored in `self.inflight` is only driven while `receive` is polled. If the SDK stops polling `receive` (service shutdown) with a reply still in flight, that reply can be lost. It is flushed first at the top of every `receive` call and before EOF is reported.
- Ids that are valid JSON but not string/integer cannot be answered.
- The shim is ~240 lines of new framing code on a security boundary. If the reviewer prefers, the alternative is the SDK transport with
  `with_max_line_length(16 * 1024)` and a documented CONTRACT_GAP for duplicate keys.

## Wire size is not bounded by the guard (P3, source-only)
The guard limits INPUT frames (16 KiB, before parsing). It does not limit anything the server writes. The only output limit is in
`tools::dispatch`: the compact structured JSON of a result against 16 / 8 / 2 / 2 KiB. The SDK then duplicates that JSON into
`content[0].text` (INFERRED from `rmcp 3.9.0` source; NOT_RUN), where it is escaped a second time, and it adds the envelope and the echoed
request `id` (up to 16 KiB). The entire serialized JSON-RPC response and the `tools/list` reply are not bounded.
Formula and assumptions: README, "Output size". The source-only bound `S + 2*S + E + I` (search 65836, get 41260, status and checkpoint 22828 bytes) is
**INFERRED / NOT_MEASURED**, not a measured maximum and not a memory-safety claim. Later raw-wire byte measurement on the isolated Windows runner is
required. Changing guard framing, liveness or output enforcement is out of scope of P3: DESIGN_DECISION_REQUIRED if wanted.

## Escape note (P3 correction)
Withdrawn: "no escaped Unicode literal appears in the package". `tests/process.rs` has the six-character source literal `\u00fc` in a comment and in the
raw-string fixture `G\u00fcvenlik` (source literal; the server decodes it to U+00FC; the wire carries the six ASCII bytes). Only the T10 and guard-unit
fixtures avoid source literals (run-time `char::from(92u8)` plus a byte-level difference assert). See README, "Escape notation".

## Flags for BLOCKED
No mismatch with the `Transport` trait contract was found by reading (`send` returns a `Send + 'static` future; `receive` is sequential;
`close` closes the writer). That is not a compile or run result.
