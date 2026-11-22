//! Wire guard: a thin `rmcp::transport::Transport` shim for the two Phase 0 review caveats.
//!
//! WHY IT EXISTS. rmcp 3.9.0 parses each stdin line with `serde_json` straight into typed messages, where JSON objects are maps: a duplicate
//! key (envelope or inside `arguments`) is collapsed (last value wins) before any application code runs, and the SDK's own line cap defaults to
//! 16 MiB. Neither can be changed from inside a `ServerHandler`. The public `Transport` trait is the SDK's supported extension point, so this
//! shim:
//!   1. reads stdin lines itself, with a hard frame cap checked BEFORE each append (an oversized frame is dropped without being buffered);
//!   2. checks every complete frame with the SAME `serde_json` parser through a strict visitor that rejects duplicate keys at any depth and
//!      nesting deeper than `MAX_DEPTH`;
//!   3. only then lets `serde_json` build the SDK's own typed message, which the SDK dispatches (lifecycle, eras, discover and tools stay
//!      entirely in rmcp);
//!   4. sends through the SDK's own `AsyncRwTransport` writer (its codec produces one compact line per message).
//! It does not implement JSON-RPC semantics. Rejections are answered with a fixed `-32600` error that carries the request id when the frame
//! has exactly one readable `id`; otherwise the frame is dropped (the same behavior as the SDK for unreadable input).

use rmcp::model::{ErrorData, JsonRpcMessage, RequestId};
use rmcp::service::{RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::transport::{async_rw::AsyncRwTransport, Transport};
use rmcp::RoleServer;
use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use std::collections::{HashSet, VecDeque};
use std::fmt;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, BufReader};

/// Largest accepted frame (one JSON-RPC line), in bytes. Tool arguments are separately limited to 1 KiB.
pub const MAX_FRAME_BYTES: usize = 16 * 1024;
/// Deepest accepted JSON nesting (serde_json's own limit is 128).
pub const MAX_DEPTH: usize = 32;

#[derive(Debug, PartialEq, Eq)]
pub enum Reject {
    NotJson,
    DuplicateKey,
    TooDeep,
}

struct Strict {
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for Strict {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Strict {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("json")
    }
    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        if self.depth >= MAX_DEPTH {
            return Err(A::Error::custom("ATM_TOO_DEEP"));
        }
        while a.next_element_seed(Strict { depth: self.depth + 1 })?.is_some() {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        if self.depth >= MAX_DEPTH {
            return Err(A::Error::custom("ATM_TOO_DEEP"));
        }
        let mut seen: HashSet<String> = HashSet::new();
        // Keys are compared AFTER JSON unescaping, so "a" and "a" are the same key.
        while let Some(key) = a.next_key::<String>()? {
            if !seen.insert(key) {
                return Err(A::Error::custom("ATM_DUPLICATE_KEY"));
            }
            a.next_value_seed(Strict { depth: self.depth + 1 })?;
        }
        Ok(())
    }
}

/// Strict structural check of one frame (no trailing bytes after the single JSON value).
pub fn check_frame(frame: &[u8]) -> Result<(), Reject> {
    let mut de = serde_json::Deserializer::from_slice(frame);
    let result = Strict { depth: 0 }.deserialize(&mut de).and_then(|()| de.end());
    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("ATM_DUPLICATE_KEY") {
                Err(Reject::DuplicateKey)
            } else if msg.contains("ATM_TOO_DEEP") || msg.contains("recursion limit") {
                Err(Reject::TooDeep)
            } else {
                Err(Reject::NotJson)
            }
        }
    }
}

#[derive(Deserialize)]
struct IdOnly {
    #[serde(default)]
    id: Option<RequestId>,
}

/// The request id of a frame, only when the frame is an object with at most one `id` key that is a string or an integer.
fn readable_id(frame: &[u8]) -> Option<RequestId> {
    // Serde reads a one-element JSON array as a one-field struct, so `[1]` would give an id. Require an object root first.
    let first = frame.iter().find(|b| !matches!(**b, b' ' | b'\t' | b'\r' | b'\n'))?;
    if *first != b'{' {
        return None;
    }
    serde_json::from_slice::<IdOnly>(frame).ok().and_then(|v| v.id)
}

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

fn trim_frame(raw: &[u8]) -> &[u8] {
    let line = raw.strip_suffix(b"\n").unwrap_or(raw);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    line.strip_prefix(UTF8_BOM).unwrap_or(line)
}

/// A reply send that has been started. It is kept in the struct, never dropped half-way, so a cancelled `receive` resumes it instead of
/// re-encoding the reply (which would put two copies of one error reply on stdout).
type SendFuture = std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>> + Send>>;

pub struct GuardedTransport<R: AsyncRead, W: AsyncWrite> {
    read: BufReader<R>,
    line: Vec<u8>,
    discarding: bool,
    pending: VecDeque<TxJsonRpcMessage<RoleServer>>,
    inflight: Option<SendFuture>,
    out: AsyncRwTransport<RoleServer, tokio::io::Empty, W>,
}

impl<R, W> GuardedTransport<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send + 'static,
{
    pub fn new(read: R, write: W) -> Self {
        Self {
            read: BufReader::new(read),
            line: Vec::new(),
            discarding: false,
            pending: VecDeque::new(),
            inflight: None,
            out: AsyncRwTransport::new(tokio::io::empty(), write),
        }
    }

    /// Reads one `\n`-terminated frame into `self.line`, checking the cap before every append. Cancel-safe: `fill_buf` consumes nothing
    /// when dropped, and partial bytes stay in `self.line`. Returns `Ok(true)` for a complete frame, `Ok(false)` at EOF.
    async fn read_frame(&mut self) -> std::io::Result<Option<bool>> {
        loop {
            let available = self.read.fill_buf().await?;
            if available.is_empty() {
                return Ok(Some(false));
            }
            let newline = available.iter().position(|b| *b == b'\n');
            let take = newline.map_or(available.len(), |i| i + 1);
            if self.discarding {
                self.read.consume(take);
                if newline.is_some() {
                    self.discarding = false;
                }
                continue;
            }
            if self.line.len().saturating_add(take) > MAX_FRAME_BYTES {
                // Oversized: nothing of it is kept; skip to the end of the line.
                self.line.clear();
                self.read.consume(take);
                self.discarding = newline.is_none();
                return Ok(None);
            }
            self.line.extend_from_slice(&available[..take]);
            self.read.consume(take);
            if newline.is_some() {
                return Ok(Some(true));
            }
        }
    }
}

impl<R, W> Transport<RoleServer> for GuardedTransport<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send + 'static,
{
    type Error = std::io::Error;

    fn send(&mut self, item: TxJsonRpcMessage<RoleServer>) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send + 'static {
        self.out.send(item)
    }

    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleServer>> {
        loop {
            // Rejection replies first. A reply is popped ONCE, turned into a send future that lives in `self.inflight`, and driven to
            // completion here. If `receive` is cancelled while that future is pending, the future stays stored and the next call resumes it
            // (the writer's partial-write state is inside the future). It is cleared in the same poll in which it completes.
            if self.inflight.is_none() {
                if let Some(reply) = self.pending.pop_front() {
                    self.inflight = Some(Box::pin(self.out.send(reply)));
                }
            }
            if let Some(fut) = self.inflight.as_mut() {
                let sent = fut.await;
                self.inflight = None;
                if sent.is_err() {
                    return None;
                }
                continue;
            }
            match self.read_frame().await {
                Err(_) | Ok(Some(false)) => return None,
                Ok(None) => continue, // oversized frame dropped
                Ok(Some(true)) => {}
            }
            let frame = trim_frame(&self.line).to_vec();
            self.line.clear();
            if frame.is_empty() {
                continue;
            }
            if let Err(_reason) = check_frame(&frame) {
                if let Some(id) = readable_id(&frame) {
                    self.pending.push_back(TxJsonRpcMessage::<RoleServer>::error(ErrorData::invalid_request("Invalid request", None), Some(id)));
                }
                continue;
            }
            match serde_json::from_slice::<RxJsonRpcMessage<RoleServer>>(&frame) {
                // A JSON-RPC response or error from the client answers nothing (this server sends no requests): dropped.
                Ok(JsonRpcMessage::Response(_)) | Ok(JsonRpcMessage::Error(_)) => continue,
                Ok(message) => return Some(message),
                Err(_) => {
                    if let Some(id) = readable_id(&frame) {
                        self.pending.push_back(TxJsonRpcMessage::<RoleServer>::error(ErrorData::invalid_request("Invalid request", None), Some(id)));
                    }
                }
            }
        }
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        self.out.close().await
    }
}

#[cfg(test)]
mod tests {
    //! Each test states the pass/reject contract it covers. NOT_RUN: nothing here has been compiled or executed.
    use super::*;
    use std::io;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll};
    use std::time::Duration;
    use tokio::io::{AsyncWrite, AsyncWriteExt};

    /// One JSON escape sequence for one character, built at run time: the fixture really contains a backslash, `u` and four hex digits.
    fn esc(c: char) -> String {
        format!("{}u{:04x}", char::from(92u8), c as u32)
    }

    // ---- check_frame: duplicate-key contract ---------------------------------------------------------------------------------------------

    /// CONTRACT: two textually identical keys in one object are a duplicate, at the root, inside nested objects and inside arrays.
    #[test]
    fn plain_duplicate_keys_are_rejected_at_every_depth() {
        assert_eq!(check_frame(br#"{"a":1,"a":2}"#), Err(Reject::DuplicateKey));
        assert_eq!(check_frame(br#"{"p":{"arguments":{"id":"x","id":"y"}}}"#), Err(Reject::DuplicateKey));
        assert_eq!(check_frame(br#"{"p":[{"k":1,"k":1}]}"#), Err(Reject::DuplicateKey));
    }

    /// CONTRACT: keys are compared AFTER JSON unescaping. The fixtures are raw JSON text that spells one key with an escape sequence, so
    /// they differ from the plain fixtures above at the byte level (asserted) but are the same key once decoded.
    #[test]
    fn escaped_duplicate_keys_are_rejected_after_unescaping() {
        let ascii = format!("{{\"a\":1,\"{}\":2}}", esc('a'));
        assert!(ascii.contains("u0061"), "fixture must contain a real escape sequence");
        assert_ne!(ascii, r#"{"a":1,"a":2}"#);
        assert_eq!(check_frame(ascii.as_bytes()), Err(Reject::DuplicateKey));
        let nested = format!("{{\"params\":{{\"arguments\":{{\"id\":1,\"{}d\":2}}}}}}", esc('i'));
        assert!(nested.contains("u0069d"));
        assert_eq!(check_frame(nested.as_bytes()), Err(Reject::DuplicateKey));
        let meta = format!("{{\"params\":{{\"_meta\":{{}},\"{}meta\":{{}}}}}}", esc('_'));
        assert!(meta.contains("u005fmeta"));
        assert_eq!(check_frame(meta.as_bytes()), Err(Reject::DuplicateKey));
        let non_ascii = format!("{{\"\u{e9}\":1,\"{}\":2}}", esc('\u{e9}'));
        assert!(non_ascii.contains("u00e9"));
        assert_eq!(check_frame(non_ascii.as_bytes()), Err(Reject::DuplicateKey));
    }

    /// CONTRACT (the PASS side): different keys never collide, an escape that decodes to a DIFFERENT key is not a duplicate, and one key
    /// spelled with an escape is still a single key.
    #[test]
    fn distinct_and_single_escaped_keys_pass() {
        assert_eq!(check_frame(br#"{"a":1,"b":{"a":2}}"#), Ok(()));
        let other = format!("{{\"a\":1,\"{}\":2}}", esc('b'));
        assert_eq!(check_frame(other.as_bytes()), Ok(()));
        let single = format!("{{\"{}\":1}}", esc('a'));
        assert_eq!(check_frame(single.as_bytes()), Ok(()));
        assert_eq!(check_frame(br#"{"a":[{"a":1},{"a":2}]}"#), Ok(()));
    }

    /// CONTRACT: nesting deeper than MAX_DEPTH is refused, exactly MAX_DEPTH passes, absurd depth does not crash, garbage is NotJson.
    #[test]
    fn depth_and_garbage_are_rejected() {
        let deep = format!("{}1{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1));
        assert_eq!(check_frame(deep.as_bytes()), Err(Reject::TooDeep));
        let ok = format!("{}1{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert_eq!(check_frame(ok.as_bytes()), Ok(()));
        let very_deep = format!("{}1{}", "[".repeat(5000), "]".repeat(5000));
        assert!(check_frame(very_deep.as_bytes()).is_err());
        assert_eq!(check_frame(b"{"), Err(Reject::NotJson));
        assert_eq!(check_frame(br#"{"a":1} {"b":2}"#), Err(Reject::NotJson));
        assert_eq!(check_frame(b"\xff\xfe"), Err(Reject::NotJson));
    }

    /// CONTRACT: a rejected frame is answered only when its request id is unambiguous. A duplicated id, including one spelled with an
    /// escape, a null id, a missing id and a non-object give no id (no reply).
    #[test]
    fn id_is_read_only_when_unambiguous() {
        assert!(readable_id(br#"{"id":7,"method":"x"}"#).is_some());
        assert!(readable_id(br#"{"id":"a","id":"b"}"#).is_none());
        let escaped_dup = format!("{{\"id\":1,\"{}d\":2}}", esc('i'));
        assert!(readable_id(escaped_dup.as_bytes()).is_none());
        assert!(readable_id(br#"{"id":null}"#).is_none());
        assert!(readable_id(br#"{"method":"x"}"#).is_none());
        assert!(readable_id(b"[1]").is_none());
        assert!(readable_id(br#"{"id":7}"#).is_some());
        assert!(readable_id(b"  \t\r\n{\"id\":7}").is_some());
        assert!(readable_id(br#"["id"]"#).is_none());
        assert!(readable_id(b"null").is_none());
        assert!(readable_id(b"123").is_none());
        assert!(readable_id(b"").is_none());
    }

    // ---- transport: reply queue, cancellation, short writes, EOF -----------------------------------------------------------------------

    /// A writer that accepts at most 3 bytes per call and answers every other call with Pending (after waking itself).
    struct Throttled {
        out: Arc<Mutex<Vec<u8>>>,
        flip: bool,
    }

    impl AsyncWrite for Throttled {
        fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, data: &[u8]) -> Poll<io::Result<usize>> {
            self.flip = !self.flip;
            if self.flip {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            let n = data.len().min(3);
            self.out.lock().unwrap().extend_from_slice(&data[..n]);
            Poll::Ready(Ok(n))
        }
        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    const DUP_PING: &[u8] = br#"{"jsonrpc":"2.0","id":5,"method":"ping","method":"ping"}"#;
    const GOOD_PING: &[u8] = br#"{"jsonrpc":"2.0","id":6,"method":"ping"}"#;

    fn replies(out: &Arc<Mutex<Vec<u8>>>) -> Vec<serde_json::Value> {
        let text = String::from_utf8(out.lock().unwrap().clone()).expect("utf8 output");
        text.lines().map(|l| serde_json::from_str(l).expect("each output line is JSON")).collect()
    }

    /// CONTRACT: one rejected frame produces exactly ONE reply line, even when `receive` is cancelled many times while the reply is being
    /// written and the writer accepts 3 bytes at a time. (With the earlier design, a cancelled send could be encoded twice.)
    #[tokio::test]
    async fn rejected_frame_reply_is_written_exactly_once_under_cancellation_and_short_writes() {
        let out = Arc::new(Mutex::new(Vec::new()));
        let (rd, mut peer) = tokio::io::duplex(4096);
        let mut t = GuardedTransport::new(rd, Throttled { out: out.clone(), flip: false });
        peer.write_all(DUP_PING).await.unwrap();
        peer.write_all(b"\n").await.unwrap();
        for _ in 0..600 {
            let _ = tokio::time::timeout(Duration::from_millis(2), t.receive()).await;
        }
        let lines = replies(&out);
        assert_eq!(lines.len(), 1, "expected exactly one reply line, got {lines:?}");
        assert_eq!(lines[0]["id"], 5);
        assert_eq!(lines[0]["error"]["code"], -32600);
        assert!(lines[0].get("result").is_none());
    }

    /// CONTRACT: a valid frame after a rejected one is delivered to the SDK, and the rejected one still got exactly one reply.
    #[tokio::test]
    async fn valid_frame_after_a_rejected_frame_is_delivered() {
        let out = Arc::new(Mutex::new(Vec::new()));
        let (rd, mut peer) = tokio::io::duplex(4096);
        let mut t = GuardedTransport::new(rd, Throttled { out: out.clone(), flip: false });
        peer.write_all(DUP_PING).await.unwrap();
        peer.write_all(b"\n").await.unwrap();
        peer.write_all(GOOD_PING).await.unwrap();
        peer.write_all(b"\n").await.unwrap();
        let message = t.receive().await.expect("the valid frame is delivered");
        assert!(matches!(message, JsonRpcMessage::Request(_)));
        let lines = replies(&out);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["id"], 5);
    }

    /// CONTRACT: when stdin reaches EOF right after a rejected frame, the reply is flushed BEFORE `receive` reports the end.
    #[tokio::test]
    async fn eof_right_after_a_rejected_frame_still_flushes_its_reply() {
        let out = Arc::new(Mutex::new(Vec::new()));
        let (rd, mut peer) = tokio::io::duplex(4096);
        let mut t = GuardedTransport::new(rd, Throttled { out: out.clone(), flip: false });
        peer.write_all(DUP_PING).await.unwrap();
        peer.write_all(b"\n").await.unwrap();
        drop(peer);
        assert!(t.receive().await.is_none());
        let lines = replies(&out);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["id"], 5);
    }

    /// CONTRACT: an unfinished frame (no newline) at EOF is dropped without a reply and without a hang.
    #[tokio::test]
    async fn unfinished_frame_at_eof_is_dropped_silently() {
        let out = Arc::new(Mutex::new(Vec::new()));
        let (rd, mut peer) = tokio::io::duplex(4096);
        let mut t = GuardedTransport::new(rd, Throttled { out: out.clone(), flip: false });
        peer.write_all(GOOD_PING).await.unwrap(); // no newline
        drop(peer);
        assert!(t.receive().await.is_none());
        assert!(replies(&out).is_empty());
    }

    /// CONTRACT: a frame over the cap is dropped without a reply and the next frame is still read (the stream stays in sync).
    #[tokio::test]
    async fn oversized_frame_is_dropped_and_the_next_frame_is_delivered() {
        let out = Arc::new(Mutex::new(Vec::new()));
        let (rd, mut peer) = tokio::io::duplex(64 * 1024);
        let mut t = GuardedTransport::new(rd, Throttled { out: out.clone(), flip: false });
        let big = vec![b'x'; MAX_FRAME_BYTES + 1000];
        peer.write_all(&big).await.unwrap();
        peer.write_all(b"\n").await.unwrap();
        peer.write_all(GOOD_PING).await.unwrap();
        peer.write_all(b"\n").await.unwrap();
        let message = t.receive().await.expect("the frame after the oversized one is delivered");
        assert!(matches!(message, JsonRpcMessage::Request(_)));
        assert!(replies(&out).is_empty());
    }
}
