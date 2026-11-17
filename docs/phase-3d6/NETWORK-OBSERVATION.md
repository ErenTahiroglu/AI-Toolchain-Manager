# Network observation of the real window (Phase 3D-6) — an unexpected finding, reported as found

The task asked to show that **no unapproved network destination is contacted**. This is what was actually observed, and what that does and does not establish.

## Method (read-only)

[`evidence/observe-gui-run.ps1`](evidence/observe-gui-run.ps1) wraps one run of the real window of the armed `jq-pilot` debug build (`tools/gui_jq_pilot_locked.ps1`, an isolated **temporary** data directory — never the pilot directory). Every 400 ms it lists the TCP/UDP endpoints owned by **the application's process tree** (the processes below the application's own process, found by parent id) and labels each by role: the application, the WebView2 renderer that hosts its page, the WebView2 network service, the WebView2 browser process. It reads only; it changes nothing. Result files: [`evidence/gui-jq-armed/`](evidence/gui-jq-armed/).

## Result

| Role (processes of the application's own tree) | Non-loopback TCP connections observed |
| --- | --- |
| the application (`ai-toolchain-manager.exe`) | **none** |
| the WebView2 renderer that hosts the page | **none** |
| WebView2 **network service** (Microsoft's runtime, `msedgewebview2.exe --utility-sub-type=network.mojom.NetworkService`) | **yes** — port 443 of `8.8.8.8` and `8.8.4.4` (Google Public DNS) |
| WebView2 **browser** process (Microsoft's runtime) | **yes** — port 443 of one Microsoft address in `52.97.0.0/16` (`52.97.128.2`, `52.97.141.82` in different captures) |

Loopback: only the WebView2 debugging port `127.0.0.1:9222` that the driver itself requests.

The same pattern appeared in an independent capture that recorded owner pid, parent pid and command line of every connection ([`evidence/process-attribution.txt`](evidence/process-attribution.txt), produced by a small scratch script that is not part of the repository): the `8.8.8.8` connections belong to the runtime's network-service process and the `52.97.x.x` ones to the runtime's browser process, both children of the application. A *different* program's `msedgewebview2` tree (another application on this computer) also talks to `8.8.8.8` and was correctly excluded.

## What this does and does not establish

* **The application's own code opened no connection**: not the application process, not the page's renderer. The page's content-security-policy allows only `ipc:` / `http://ipc.localhost` for `connect-src`; the sources contain no network API (`std::net`, sockets, HTTP clients — scanned by tests, and `reqwest`/`hyper` appear in `Cargo.lock` only, not in this target's dependency graph); the install workflow reads one local file and says so in its preview.
* **The Microsoft WebView2 runtime that hosts the window does make outbound connections of its own**, independent of anything this application asks of it. That is true of **every** real-window run in every earlier phase too (the earlier drivers' header said "no network is used"; that wording was too strong and is corrected in this phase — it was true of the application's code, not of the runtime hosting it).
* **Not determined:** *why* the runtime connects. A plausible, **unverified** explanation for the Google connections is Chromium's automatic DNS-over-HTTPS upgrade, because `8.8.8.8` is one of this computer's configured DNS servers (`8.8.4.4` is Google's pair of it). The Microsoft connection is unexplained. No traffic content was inspected; no setting was changed.
* **No operating-system-level network isolation** is available without administrator rights, so "nothing else on the network" cannot be proven by this method for a window that embeds a browser engine.
* None of this involves the installation itself: the installation reads one local file and writes inside the data directory; jq is never executed; nothing is downloaded by the application.

## What this means for the final Red Team review and the first real installation

The decision belongs to the reviewer and the user; nothing was changed in the application for it.

1. **Accept and record** that the real installation, which needs the real window (the confirmation is the user's click in that window), will run with the WebView2 runtime's own connections present, while the application's code stays offline; or
2. **Ask for a separate, small, reviewed change** that constrains the embedded runtime (for example through the window's additional browser arguments to turn off the DNS-over-HTTPS upgrade, or an operating-system firewall rule the user adds by hand), which this phase deliberately did **not** make because it would change application behaviour the reviewer accepted.

## One capture that was discarded

The first capture of this phase matched `msedgewebview2` processes **by name across the whole computer**, so it attributed other programs' connections (including a Microsoft Exchange-range address) to this run. That was a defect of the observer, not of the application; the capture was deleted and the observer rewritten to follow the application's process tree. It is mentioned so the sequence is not hidden.

## Addendum (NEXT-11): scope of the observations above, and what the `0fa04d8` build adds

This addendum changes nothing above. It only states which binary those observations describe and what a later binary contains.

* **The capture and the statements above describe the Phase 3D-6 binary (the armed `jq-pilot` debug build of the 3D-6 tree).** In that tree the application's own code had no HTTP client, and `reqwest` / `hyper` appeared in `Cargo.lock` only, not in the dependency graph of that target. Those sentences were true of that binary and are kept as written.
* **The `0fa04d8d30b2ce416ff5909f6d27af738c49326e` build is different.** Since `37c58fb` the application depends on the Evidence-Checkpoint-Engine crate with its `github-http` feature (`src-tauri/Cargo.toml`, dependency `ece_lib`, pinned to one revision). That build therefore **contains an HTTP-capable client and a TLS stack** (`reqwest`, `hyper-rustls`, `rustls` and their dependencies), and it exposes one user-triggered command, `assess_atm_checkpoint` (`src-tauri/src/commands.rs`), that makes anonymous requests to the GitHub API when a person presses the button of the CI-evidence panel. The statements "the application's own code opened no connection" and "no network API in the sources" must **not** be read as describing the `0fa04d8` build. A byte scan of the experimental executable of that commit finds the host name `api.github.com` and the client library names.
* **What is NOT known for the `0fa04d8` build:** whether any code path other than that button reaches the client (the source says none does, nothing proves it), whether the HTTP library follows operating-system or environment proxy settings on this computer (the module header of `ece_ci_live.rs` says it may), and how the WebView2 runtime behaves in a run of that exact build with the network available. No observation of the kind described above was made for that build.
* **Offline runs do not prove that connection attempts are impossible.** An owner-observed run with the network adapters disconnected (Issue #25) saw no Established non-loopback TCP connection while it was sampled once per second. That does not show that no attempt was made, and it does not describe a run with the network restored. This document does not claim zero network attempts, and it does not claim complete WebView2 isolation: a process-scoped WebView2 profile limits where the runtime keeps its state, it does not stop the runtime from using the network.
* The decision whether to accept this for one offline, install-only pilot belongs to the owner and an independent reviewer (Issue #25 stays open).
