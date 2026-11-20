//! ATM read-only stdio MCP prototype (S1). A standalone package: it embeds public catalog data and opens no file, database, socket or process.
pub mod catalog;
pub mod guard;
pub mod server;
pub mod tools;
