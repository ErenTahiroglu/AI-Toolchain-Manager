#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
//! atm-mcp: read-only stdio MCP server prototype. stdout carries ONLY protocol messages (one JSON-RPC message per line).
//! Logging, if any, is a fixed text on stderr with no paths and no caller data.

use atm_mcp::{guard::GuardedTransport, server::AtmMcp};
use rmcp::ServiceExt;
use std::io::Write;

fn fixed_stderr(text: &str) {
    let _ = writeln!(std::io::stderr(), "{text}");
}

fn main() {
    std::panic::set_hook(Box::new(|_| fixed_stderr("atm-mcp: internal error")));
    if atm_mcp::catalog::catalog().is_err() {
        fixed_stderr("atm-mcp: embedded catalog data invalid");
        std::process::exit(2);
    }
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_time().build() {
        Ok(rt) => rt,
        Err(_) => {
            fixed_stderr("atm-mcp: runtime start failed");
            std::process::exit(1);
        }
    };
    let code = runtime.block_on(async {
        let transport = GuardedTransport::new(tokio::io::stdin(), tokio::io::stdout());
        match AtmMcp.serve(transport).await {
            Ok(running) => match running.waiting().await {
                Ok(_) => 0,
                Err(_) => 1,
            },
            // The peer may close stdin before starting an MCP session. That is a normal shutdown.
            Err(rmcp::service::ServerInitializeError::ConnectionClosed(_)) => 0,
            Err(_) => 1,
        }
    });
    // Exit directly: a blocked stdin read thread must never delay process exit.
    std::process::exit(code);
}
