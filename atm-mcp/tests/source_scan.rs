//! Supportive static checks only. A source scan is NOT an operating-system guarantee: it cannot show that no socket, file or process is
//! ever created by a dependency, and a feature compiled into tokio is not proof it was invoked. These tests only catch an accidental
//! use of an obviously forbidden API in THIS package's own source.

use std::path::PathBuf;

fn sources() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).expect("src dir").flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) == Some("rs") {
            out.push((p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&p).expect("read source")));
        }
    }
    out.sort();
    out
}

/// Code lines only: whole-line comments are dropped (they describe, they do not call).
fn code(src: &str) -> String {
    src.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n")
}

#[test]
fn t22c_no_forbidden_apis_in_the_package_source() {
    let forbidden = [
        "std::fs", "std::net", "std::env::", "std::os::", "tokio::fs", "tokio::net", "tokio::process", "tokio::signal", "TcpListener", "TcpStream", "UdpSocket",
        "UnixListener", "Command::new", "File::", "OpenOptions", "read_dir", "create_dir", "remove_file", "winreg", "RegOpenKey", "reqwest", "hyper", "http::",
        "println!", "eprintln!", "print!(", "dbg!", "unsafe", "std::process::Command", "spawn_blocking", "rusqlite", "tauri",
    ];
    for (name, src) in sources() {
        let c = code(&src).replace("std::process::exit", "EXIT_ONLY");
        for f in forbidden {
            assert!(!c.contains(f), "{name} contains forbidden API {f:?}");
        }
        assert!(!c.contains("std::process"), "{name} uses std::process beyond exit");
    }
}

#[test]
fn t20b_stdout_discipline_is_enforced_by_lint_and_main_writes_only_to_stderr() {
    let main = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/main.rs")).unwrap();
    assert!(main.contains("#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]"));
    assert!(main.contains("std::io::stderr()"));
    assert!(!main.contains("stdout()") || main.contains("tokio::io::stdout()"), "main may use stdout only as the transport writer");
    // the panic hook prints a fixed text and never the payload
    assert!(main.contains("set_hook(Box::new(|_|"), "panic hook must ignore the payload");
    assert!(!main.contains("PanicInfo") && !main.contains("PanicHookInfo"));
    let toml = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert!(toml.contains("panic = \"abort\""));
}

#[test]
fn guard_checks_the_frame_cap_before_every_append_and_never_replaces_rmcp_dispatch() {
    let guard = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/guard.rs")).unwrap();
    let cap_check = guard.find("> MAX_FRAME_BYTES").expect("cap check");
    let append = guard.find("self.line.extend_from_slice").expect("append");
    assert!(cap_check < append, "the cap must be checked before the append");
    assert!(guard.contains("check_frame(&frame)") && guard.contains("serde_json::from_slice::<RxJsonRpcMessage<RoleServer>>"));
    let server = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/server.rs")).unwrap();
    for needle in ["fn initialize", "fn discover", "fn ping"] {
        assert!(!code(&server).contains(needle), "server.rs must not override {needle}: handshake and discovery stay in rmcp");
    }
    assert!(server.contains("fn supported_protocol_versions"));
}

/// Static: the build-target constant must require the MSVC ABI, not merely Windows on x86_64.
#[test]
fn build_target_cfg_requires_target_env_msvc() {
    let tools = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/tools.rs")).unwrap();
    assert!(tools.contains("cfg!(all(target_os = \"windows\", target_arch = \"x86_64\", target_env = \"msvc\"))"));
    assert!(!tools.contains("cfg!(all(target_os = \"windows\", target_arch = \"x86_64\"))"));
    assert_eq!(tools.matches("x86_64-pc-windows-msvc").count(), 1, "the target name appears only in the cfg-guarded constant");
}

/// Static: the pinned SDK's deprecated `ServerInfo` alias must not be used (strict clippy denies deprecation warnings).
#[test]
fn deprecated_rmcp_server_info_alias_is_not_used() {
    for (name, src) in sources() {
        assert!(!code(&src).contains("ServerInfo"), "{name} uses the deprecated alias ServerInfo; use ServerConfig");
    }
}

#[test]
fn dependency_manifest_is_pinned_narrow_and_standalone() {
    let toml = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert!(toml.contains("rmcp = { version = \"=3.9.0\", default-features = false, features = [\"server\", \"transport-io\"] }"));
    assert!(toml.contains("\n[workspace]\n"), "standalone package, not a workspace member");
    for banned in ["git =", "path =", "registry =", "reqwest", "streamable", "auth", "transport-child-process"] {
        assert!(!toml.contains(banned), "manifest mentions {banned}");
    }
    let lock = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock")).unwrap();
    assert!(!lock.contains("source = \"git+") && lock.matches("source = \"registry+https://github.com/rust-lang/crates.io-index\"").count() == 87);
    for banned in ["name = \"reqwest\"", "name = \"hyper\"", "name = \"oauth2\"", "name = \"rustls\"", "name = \"native-tls\"", "name = \"jsonwebtoken\"", "name = \"axum\""] {
        assert!(!lock.contains(banned), "lock contains {banned}");
    }
}

/// T25 is a repository-level check (the diff touches nothing outside `atm-mcp/`, and `src-tauri` still has exactly one lib, one bin and one
/// build script). It needs a Git worktree, which a source bundle does not have, so it is not a cargo test: the author report records it.
#[test]
#[ignore = "T25 is verified with `git status` / `cargo metadata` in the real worktree, not inside a bundle"]
fn t25_repository_boundary_is_checked_outside_cargo() {}
