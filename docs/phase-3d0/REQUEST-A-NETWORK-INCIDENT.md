# Request A — network-policy incident and its closure (Issue #19)

**This record is permanent. It is not edited to make the incident disappear; later corrections are appended as new sections.**

## 1. What happened

During the first authorised download-and-verify session (`jq-review-20261018T2240Z`), the online form of `gh attestation verify` was run for both jq files. The approved network list ([REQUEST-A.md](REQUEST-A.md) §2) named `api.github.com` and the two Sigstore TUF hosts for the GitHub CLI, but **not** `tmaproduction.blob.core.windows.net`. The GitHub CLI nevertheless fetched the attestation bundle from that host. That is a **policy violation regardless of the cryptographic outcome**: the verification succeeded, and the authorisation contract was still broken. **The contact is NOT authorised, and it is NOT retroactively authorised.** The host is not added to any list by this closure.

## 2. Observed versus inferred

| Statement | Status |
| --- | --- |
| The attestations API response stored in Phase 3D-0 contains, per attestation, a `bundle_url` pointing to `https://tmaproduction.blob.core.windows.net/attestations/…` with a signed query (expiring, read-only), in addition to an inline `bundle` | **observed** (stored evidence) |
| The `bundle_url` field of the CLI's own JSON output is empty | **observed** (`*.gh.json`) |
| Re-running the same command through a local deny-by-default logging proxy: the CLI tunnelled `tuf-repo-cdn.sigstore.dev`, `tuf-repo.github.com`, `api.github.com`, then made **8 CONNECT attempts to `tmaproduction.blob.core.windows.net`, all refused**, and failed with "failed to fetch bundle with URL … Forbidden" | **observed** (`proxy-observation-hosts.log.txt`, `proxy-observation-jq-windows-amd64.exe.online.err`) |
| The first, unproxied online run really connected to that host and downloaded the bundle | **inferred** from the fact that it succeeded and produced the bundle, while the same command fails with "failed to fetch bundle with URL" as soon as that host is refused; there was no logging at the time, so it was **not directly observed** |
| Why: the GitHub CLI follows the API-supplied `bundle_url` to fetch the bundle. The Red Team brief states that GitHub's REST API supports signed attestation-bundle URLs hosted on Azure Blob Storage | **observed** (the attempts and the error text); the stated GitHub documentation was **not independently verified** here |
| The role of the signed URL: its query string is a short-lived read-only storage signature that is the only authorisation for the read; the bundle is public attestation data | **inferred** from the shape of the URL (`se`, `sp=r`, `sr=b`); not confirmed with the provider |
| The offline replay made **no** network connection at all | **observed** (logging proxy: 0 events; guard run: 0 contacted hosts, 0 incidents) |

## 3. Credentials

There is **no evidence** that any credential of this project was sent to that host, but this was **not independently proven**. What is known: telemetry and update notices were switched off (`GH_TELEMETRY=false`, `GH_NO_UPDATE_NOTIFIER=1`); the project's secrets are not used by the verification at all; the CLI's stored GitHub login is meant for GitHub hosts and the storage URL carries its own signature; the proxy re-run only ever saw the destination name of the refused attempts, never request headers (the TLS tunnel was never opened), so it **cannot show** what the first, unproxied run sent. What the first run certainly exposed is the connection itself: this computer's network address and the signed URL to the storage provider.

## 4. Root cause

The allowlist of Request A was enforced only by `tools/fetch_pinned.py`. The GitHub CLI is a separate program that chooses its own destinations from API responses, and nothing watched or constrained it. The risk was foreseeable (the stored API response contained `bundle_url`) and was not read closely enough before the online run. An application-level host check cannot monitor another process's traffic.

## 5. Corrective design (this closure)

* **Offline by default.** The mandatory official verification is the offline replay with the retained bundle and the trusted root already obtained; it needs no host. The online form is no longer part of the default procedure.
* **`tools/egress_guard.py`** — a deny-by-default local HTTPS CONNECT proxy and a `run` mode that runs one command with every proxy variable pointing at it. The destination is decided from the CONNECT line **before any upstream socket exists**: unlisted, malformed, IP-literal, non-443 and non-CONNECT requests are refused, an empty allowlist (offline mode) refuses everything, and any refused attempt is an incident (exit 97) even if the child ignores it and exits 0. Tested with local mocks only, including a mapping to a reachable server that proves zero upstream connections; also shown with the real GitHub CLI against an empty allowlist (it was refused at the first TUF host, exit 97, nothing left the machine).
* **If an online check needs a new host, stop** and ask for a separate explicit authorisation that names every host before connecting. The guard enforces this mechanically (a new host is a refusal, never an automatic addition).
* **Honest limits.** The guard is not an operating-system firewall. A program that ignores proxy variables and connects directly is neither stopped nor seen (a test documents this). A real per-program firewall rule needs administrator rights, which this project does not use. So the guard is a second layer on top of preferring offline, and a report that cannot show the guard was used is classified as **not compliant**.
* **Reporting model.** The reviewer now reports six separate facts — `artifact_crypto_verified`, `exact_bytes_verified`, `official_verifier_passed`, `network_policy_compliant`, `installation_authorized`, `execution_authorized` — and exits 3 (not 0) when everything is consistent but the network policy was violated or cannot be shown to have been kept. Installation and execution are always `false` in its output.

## 6. Verdict for the executed session

`artifact_crypto_verified: true`, `exact_bytes_verified: true`, `official_verifier_passed: true`, **`network_policy_compliant: false`**, `installation_authorized: false`, `execution_authorized: false`. Overall: `CRYPTO_CONSISTENT_BUT_NETWORK_POLICY_NOT_COMPLIANT`. The 151-check result is cryptographic and consistency evidence only. Data: `evidence/request-a-session-20261018/network-incident.json` (permanent incident record), `review.json` (original report, unchanged) and `review-v2.json` (same 151 checks plus the six facts).

jq is **not** promoted to any authenticated-reference registry by any of this.
