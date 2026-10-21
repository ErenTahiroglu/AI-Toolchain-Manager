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
