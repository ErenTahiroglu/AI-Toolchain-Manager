# Full synthetic dry run of the pilot path (Phase 3D-1)

**This is a rehearsal with made-up bytes and FAKE approval credentials, not a jq installation.** Nothing was downloaded, started or installed; no real key, prompt or credential exists in it. The tests are in [`adapter_tests.rs`](../../src-tauri/src/adapter_tests.rs) (section "Phase 3D-1: the SYNTHETIC pilot rehearsal"), run on Windows and macOS.

## 1. What is real in the rehearsal, what is fake

| Piece | In the rehearsal |
| --- | --- |
| Managed root, transactions, journal, snapshots, receipts, recovery, handle-relative filesystem layer | **the real code** (`managed_root`, `managed_fs`, `adapter`), on a temporary directory of the machine running the tests |
| Approval ledger and audit journal | **the real durable store** (`approval_store.rs`: SQLite file, hash-chained audit, single-use consumption) |
| Approval key | a **fake OS credential backend** (`FakeOsBackend`) that behaves like the interface: it creates a key only for an `EnrollmentPermit`, signs only through `sign_with_presence` and yields **no signature** when the fake person declines — it declares itself `SoftwareFake`, so `TrustPolicy::production()` refuses it |
| User presence | the fake backend's signing operation (counted: one prompt for the key + one per issued approval) |
| Recipe / plan / Phase 3A approval | synthetic (`synthetic-tool`, made-up bytes) — the real jq recipe is a blocked draft |
| Downloading | **absent**: the adapter receives bytes; the downloader (`fetch_pinned.py`) is a separate tool and is not part of the application |

## 2. The path that was rehearsed

`preflight → reviewed recipe → canonical plan → (fake) approval → filesystem preparation → atomic activation → receipt → idempotence → rollback → uninstall → recovery`, in one test (`the_full_synthetic_pilot_rehearsal_…`):

1. **Preflight** changes nothing (managed tree byte-identical), issues and consumes no approval.
2. **Reviewed recipe / canonical plan**: validated recipe; the plan itself is not an approval (`user_approval_missing`).
3. **Approval**: Phase 3A binding + a device approval proven present by the signing operation, recorded as `issued` in the durable ledger.
4. **Filesystem preparation + atomic activation + receipt** in one transaction; version folder, pointer, receipt; no staging/lock left.
5. The approval is `consumed` before the first change; presenting the used approval again while the state already matches is an idempotent no-op; once the state has moved on, **the same approval cannot start a new transaction** (also after reopening the store = restart).
6. **Idempotence**: a fresh valid approval for the same state returns the same receipt and changes not one byte.
7. **Rollback**: a failure injected at the moment the pointer has switched puts version 1 back (pointer byte-identical; the rolled-back update leaves only its journal record).
8. **Uninstall**: program and pointer removed; **the user data of two different tools is byte-identical** before and after.
9. **Recovery**: a crash in the middle of a fresh install leaves a stale lock; new work is refused until `recover()`, which rolls back; a second `recover()` is a no-op; the tool installs again afterwards.
10. **End checks**: unrelated files unchanged, all journals terminal, audit chain verifies, every consumption audited first, every approval id issued once, prompts = 1 (key) + approvals issued.

Two more tests run the same path through the durable trust components for **every** fault point of an install (`Fault::Fail` and `Fault::Crash`; the ambiguous-orphan window is left to the Phase 3C tests that need the owner's explicit resolution): rolled back or recovered, no unrelated file changed, a consumed approval never works twice, and a new, present person can install afterwards. A last test shows that the same wiring cannot authorise `Adapter::production` and that a device approval for another recipe version cannot install this one.

## 3. What is NOT exercised by any of this (the production path)

* A real download (the fetch step), a real release, a real artifact; the downloader's behaviour against the live host (the host policy and egress guard exist; only the offline/loopback behaviour is tested).
* Windows Hello: key creation, the prompt, a real signature, attestation, deletion, prompt parenting for a Tauri window.
* `TrustPolicy::production()` accepting any real key (it accepts none that can exist today).
* The reviewed-recipe path with a real authenticated reference, a real review and a real licence acceptance (all absent on purpose).
* Any user interface or IPC command (none exists for installing).
* Real power loss or a killed process: faults are injected at defined points in-process, not by pulling the plug; real file-system caches, anti-virus filter drivers, Smart App Control, OneDrive redirection, another volume, another user.
* The Windows file-replacement window under attack (accepted risk, not exercised by an attacker).
* Update of jq (not offered), a notice file next to the installed version, executing the installed program, PATH, agent configuration.
* The same code on a second independent PC or a real Mac.
