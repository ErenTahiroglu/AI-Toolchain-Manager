# Active version probe — policy PROPOSAL (not implemented, not enabled)

**Status: proposal for a future, separately approved implementation gate.** Phase 2B contains **no** code that starts a discovered program. Nothing in this document is a permission; a later phase may adopt, change or reject it after independent review and the user's explicit authorization.

## Why a policy is needed first

`<tool> --version` looks harmless but is arbitrary code execution when `<tool>` is a file found by name. A matching file name is not identity (see the evidence ladder in [PHASE-2B](PHASE-2B.md)). Running it can read the user's files, use the network, spend tokens (agent CLIs), modify state (update checks, telemetry, config writes) or hang.

## 1. Preconditions (all must hold; any failure = no probe)

| # | Requirement |
| --- | --- |
| 1.1 | **Identity first.** The candidate's identity state is `validated` by a trust anchor stronger than a file name: an installer record naming the catalog-pinned source **and** a pinned digest of the *installed* file (the catalog does not carry one today) or an OS-verified signature chain whose signer matches a catalog-pinned signer. `record_corroborated`, `evidence_available`, `unknown`, `not_assessed` and `conflicted` are never probed — a locally editable installer record or a cache folder is not authentication of the file that would be started; only the reserved `validated` state (artifact authentication bound to the installed file) can qualify. |
| 1.2 | **Linkage.** The record must be tied to the exact file to be started (same install root, resolved without following links); "a package record exists somewhere" is not enough. |
| 1.3 | **Fresh scan.** The identity evidence comes from a scan younger than a few minutes, not from history; the file is re-hashed immediately before launch (check, then start the same handle where the OS allows). |
| 1.4 | **Catalog entry has an allowlisted probe.** Each catalog entry declares at most one probe: an exact argument vector (e.g. `["--version"]`), the expected output grammar (a regular expression with a length cap) and the maximum run time. No probe is invented from README text at run time. |
| 1.5 | **User consent per probe batch**, shown in plain Turkish before anything runs: which programs, which exact command lines, what could happen (see 5), that it may cost tokens, and that it cannot be undone. Default is off; consent is not remembered across app restarts; never pre-ticked; never triggered by a scan, timer or startup. |
| 1.6 | **Not for agents with side effects by default.** Claude Code, Antigravity, MCP servers, Strix, agent-browser and anything that can start a browser, container, model call or network session are *excluded* from the first implementation; only single-purpose command-line tools with a documented offline `--version` qualify. |

## 2. Command construction

* Executed by absolute path from the validated record; **never** through a shell, `cmd /c`, PowerShell, `PATH` lookup, `.cmd`/`.bat` wrappers or `npx`.
* Arguments come only from the catalog allowlist as a fixed vector. No user-supplied text, no environment-derived text, no interpolation.
* Working directory: a fresh empty directory owned by the app (not the project, not the home folder).
* stdin closed (null device); stdout/stderr captured with a hard cap (e.g. 4 KiB each) and parsed only with the entry's output grammar; everything else is discarded and never stored or logged.

## 3. Environment isolation

* Clean environment built from scratch: a minimal `PATH` limited to system directories, `HOME`/`USERPROFILE` pointed at an empty temporary directory, locale fixed, **all** credential-like and proxy variables absent, `NO_COLOR=1`, `CI=1`, and each entry's documented opt-outs for telemetry and update checks (e.g. a tool's own "do not track" variable) set. The list of variables is part of the entry's declaration and shown in the consent text.
* No inherited handles or file descriptors (explicit inheritance list, close-on-exec).

## 4. Time, resources and process tree

* Hard timeout (default 5 s, maximum 15 s per probe, 60 s per batch).
* The child runs in a **job object** (Windows) / its own **process group with a reaper** (macOS) so the whole tree is terminated on timeout, cancel or app exit; the app verifies no descendant survived and reports it if one did.
* Memory and output caps; a probe that exceeds any cap is killed and recorded as `probe_failed`, never retried automatically.
* At most one probe at a time; no parallel fan-out.

## 5. Filesystem and network effects

* Honest statement shown to the user: **a probe may still write files or open sockets inside the tool's own behaviour**; the app cannot guarantee otherwise without an OS sandbox. Mitigations: empty home/working directories (so writes land in a directory that is deleted afterwards), the allowlist excludes tools known to phone home, and macOS `sandbox-exec`/Windows AppContainer or restricted tokens are evaluated as **optional** hardening in the implementation review (their availability and security-policy interaction, including Smart App Control, must be tested; the app never alters a Windows security setting).
* Network: blocked where the OS offers a per-process mechanism; otherwise the entry must declare that the probe is offline, and the implementation review must treat "cannot enforce" as a blocker for that entry.
* After the probe the temporary home/working directory is inspected for new files; any new file outside the expected set marks the result `probe_suspicious` and is shown to the user.

## 6. Token and money cost

* Entries that talk to a model or a cloud service are out of scope (1.6). If a future entry could cost tokens, the consent text must state the maximum possible cost and the probe is forbidden unless the entry proves an offline path.

## 7. Result handling

* A parsed version is stored as `version` with source "etkin yoklama" and trust `probed`, **separately** from the passive trust levels, together with the argument vector id and a time stamp — never raw output.
* A probe result never changes `identity`; it never sets `functionality_tested` (a version string is not a functional test); a mismatch between a probed and a recorded version is shown as a conflict.
* Every probe is an audit event (what, when, outcome class, duration) without arguments' secrets or output text.

## 8. Rollback and irreversibility

* A probe cannot be rolled back. The app can only (a) delete its own temporary directories, (b) report suspicious artefacts and (c) refuse to repeat. State changes the tool made elsewhere (caches, config, telemetry already sent) are outside the app's control; this is why 1.1–1.6 are strict and why the default is *no probe*.

## 9. Gate before any implementation

1. Independent Red Team review of this document and of a threat model for each allowlisted entry.
2. Cross-platform CI plus an adversarial suite: forged file with the right name, swapped file between check and launch, hanging child, forking child, output flood, tool that writes to home, tool that opens a socket, tool that spawns a shell.
3. Explicit user authorization for the first implementation, separate from this document.

## 10. What is already true today (Phase 2B)

* No code path starts a discovered program (a test plants a program that would leave a marker if run and asserts it never does; a source scan bans process/network APIs in the discovery code).
* The catalog pins a source repository per entry but **no digest of the installed file**; signature blocks are read as header metadata only. Items 1.1/1.2 are therefore not satisfiable yet, which is the main blocker.
