//! Shared harness: drives the BUILT `atm-mcp` process over raw wire lines. Every stdout line is checked to be one JSON-RPC message.
//! The harness itself writes only its own empty probe directories under the OS temp dir (reported separately from server behavior).
#![allow(dead_code)]

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

pub const SUPPORTED: [&str; 3] = ["2026-07-28", "2025-11-25", "2025-06-18"];
static COUNTER: AtomicU32 = AtomicU32::new(0);

pub fn exe() -> &'static str {
    env!("CARGO_BIN_EXE_atm-mcp")
}

/// A fresh, empty directory set that stands in for every location the server could write to.
pub struct Probe {
    pub root: PathBuf,
}

impl Probe {
    pub fn new() -> Probe {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("atm-mcp-probe-{}-{n}-unique", std::process::id()));
        for d in ["home", "appdata", "localappdata", "data", "tmp", "cwd"] {
            std::fs::create_dir_all(root.join(d)).expect("probe dir");
        }
        Probe { root }
    }

    pub fn name(&self) -> String {
        self.root.file_name().unwrap().to_string_lossy().into_owned()
    }

    /// Sorted relative listing of everything under the probe root (empty means the server wrote nothing).
    pub fn listing(&self) -> Vec<String> {
        fn walk(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<String>) {
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    out.push(p.strip_prefix(base).unwrap().to_string_lossy().into_owned());
                    if p.is_dir() {
                        walk(&p, base, out);
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &self.root, &mut out);
        out.sort();
        out.retain(|p| !matches!(p.as_str(), "home" | "appdata" | "localappdata" | "data" | "tmp" | "cwd"));
        out
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<String>,
    pub raw: Vec<String>,
    stash: Vec<Value>,
    err_thread: Option<std::thread::JoinHandle<String>>,
}

impl Session {
    pub fn start(probe: &Probe, extra_env: &[(&str, &str)]) -> Session {
        let mut cmd = Command::new(exe());
        cmd.env_clear();
        if let Some(sr) = std::env::var_os("SystemRoot") {
            cmd.env("SystemRoot", sr);
        }
        for (k, d) in [("HOME", "home"), ("USERPROFILE", "home"), ("APPDATA", "appdata"), ("LOCALAPPDATA", "localappdata"), ("ATM_DATA_DIR", "data"), ("TEMP", "tmp"), ("TMP", "tmp")] {
            cmd.env(k, probe.root.join(d));
        }
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        cmd.current_dir(probe.root.join("cwd")).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("spawn atm-mcp");
        let stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                match r.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let s = String::from_utf8_lossy(&line).trim_end_matches(['\n', '\r']).to_string();
                        if tx.send(s).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let err_thread = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = stderr.read_to_string(&mut s);
            s
        });
        let stdin = child.stdin.take();
        Session { child, stdin, rx, raw: Vec::new(), stash: Vec::new(), err_thread: Some(err_thread) }
    }

    pub fn send(&mut self, line: &str) {
        self.send_bytes(line.as_bytes());
    }

    pub fn send_bytes(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        stdin.write_all(bytes).expect("write");
        stdin.write_all(b"\n").expect("write nl");
        stdin.flush().expect("flush");
    }

    /// Writes `bytes` EXACTLY as given. Unlike `send_bytes`, NO newline is appended, so the frame stays unfinished. It exists only to test an
    /// incomplete frame at EOF (T21b); every other test must use `send` / `send_bytes`.
    pub fn send_raw_no_newline(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        stdin.write_all(bytes).expect("write raw");
        stdin.flush().expect("flush raw");
    }

    /// Next stdout message within `ms`; panics if a stdout line is not one JSON-RPC 2.0 message (stdout purity).
    pub fn recv(&mut self, ms: u64) -> Option<Value> {
        match self.rx.recv_timeout(Duration::from_millis(ms)) {
            Ok(line) => {
                self.raw.push(line.clone());
                let v: Value = serde_json::from_str(&line).unwrap_or_else(|_| panic!("stdout line is not JSON: {line:?}"));
                assert_eq!(v.get("jsonrpc").and_then(Value::as_str), Some("2.0"), "stdout line is not JSON-RPC 2.0: {line:?}");
                Some(v)
            }
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => None,
        }
    }

    /// The response whose `id` equals `id`; other messages are kept for later.
    pub fn recv_id(&mut self, id: &Value, ms: u64) -> Option<Value> {
        if let Some(pos) = self.stash.iter().position(|m| m.get("id") == Some(id)) {
            return Some(self.stash.remove(pos));
        }
        let deadline = Instant::now() + Duration::from_millis(ms);
        loop {
            let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u64;
            if left == 0 {
                return None;
            }
            match self.recv(left) {
                None => return None,
                Some(m) if m.get("id") == Some(id) => return Some(m),
                Some(m) => self.stash.push(m),
            }
        }
    }

    pub fn call(&mut self, line: String, id: i64) -> Value {
        self.send(&line);
        self.recv_id(&json!(id), 15_000).unwrap_or_else(|| panic!("no response for id {id}"))
    }

    /// Closes stdin and waits (bounded) for exit. Returns every stdout line, stderr text, exit status and elapsed time.
    pub fn finish(mut self) -> Finished {
        drop(self.stdin.take());
        let start = Instant::now();
        let mut exited = None;
        while start.elapsed() < Duration::from_secs(10) {
            if let Ok(Some(st)) = self.child.try_wait() {
                exited = Some(st);
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let timed_out = exited.is_none();
        if timed_out {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let elapsed = start.elapsed();
        while let Some(m) = self.recv(300) {
            self.stash.push(m);
        }
        let stderr = self.err_thread.take().map(|h| h.join().unwrap_or_default()).unwrap_or_default();
        Finished { stdout_lines: self.raw.clone(), stderr, success: exited.map(|s| s.success()).unwrap_or(false), timed_out, elapsed, leftover: self.stash.clone() }
    }
}

pub struct Finished {
    pub stdout_lines: Vec<String>,
    pub stderr: String,
    pub success: bool,
    pub timed_out: bool,
    pub elapsed: Duration,
    /// Messages that arrived but were never claimed by `recv_id`.
    pub leftover: Vec<Value>,
}

pub fn meta(version: &str) -> Value {
    json!({"io.modelcontextprotocol/protocolVersion": version, "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "atm-mcp-test", "version": "0"}})
}

/// A modern (per-request metadata) request line.
pub fn modern(id: i64, method: &str, mut params: Value) -> String {
    if !params.is_object() {
        params = json!({});
    }
    params["_meta"] = meta("2026-07-28");
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

pub fn modern_call(id: i64, name: &str, args: Value) -> String {
    modern(id, "tools/call", json!({"name": name, "arguments": args}))
}

pub fn legacy_initialize(id: i64, version: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": "initialize", "params": {"protocolVersion": version, "capabilities": {},
        "clientInfo": {"name": "atm-mcp-test", "version": "0"}}})
    .to_string()
}

pub const INITIALIZED: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;

pub fn legacy_call(id: i64, name: &str, args: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args}}).to_string()
}

pub fn discover(id: i64) -> String {
    modern(id, "server/discover", json!({}))
}

/// Sends a modern discover probe and reports whether a result came back (the process is still healthy and in sync).
pub fn healthy(s: &mut Session, id: i64) -> bool {
    s.send(&discover(id));
    s.recv_id(&json!(id), 10_000).map(|m| m.get("result").is_some()).unwrap_or(false)
}

pub fn structured(resp: &Value) -> &Value {
    resp.pointer("/result/structuredContent").expect("structuredContent")
}

pub fn error_code(resp: &Value) -> Option<i64> {
    resp.pointer("/error/code").and_then(Value::as_i64)
}
