# What would happen during the first REAL installation (procedure, not yet authorised)

Nothing below has been done. It becomes possible only after blockers B1–B3 of [READINESS-DECISION](READINESS-DECISION.md).

## Before

1. **Pilot data folder.** A dedicated, empty folder is used through `ATM_DATA_DIR` (never the person's normal application data), so the whole experiment is confined and can be deleted as one unit afterwards.
2. **Evidence check (read-only).** The retained Request A session is re-hashed against its recorded listing (54 files, as in the Phase 3D-1 closure); the retained `jq-windows-amd64.exe` must have exactly the pinned size and SHA-256. Any difference stops the pilot.
3. **Inventory 0.** `node tools/fs_inventory.mjs snapshot <ATM_DATA_DIR> inventory-0.json` (lists, never opens for writing).
4. **The input folder.** The retained file is copied (the source is only read) into `<ATM_DATA_DIR>/pilot-input/jq-windows-amd64.exe` — the one fixed place the application reads. The copy is hashed again.
5. The application is started **from a build that includes the activation change** (B2). The `jq` entry is `Kurulabilir` only if every gate holds (recipe validated, reference, attestation, licence record, notice text, enablement, file present with the right size and digest); otherwise it says why and nothing can be previewed.

## The installation

6. The person opens the preview. It shows: source repository and pinned commit (attested), version, platform, file name, size, the full SHA-256, licence state *"accepted for local use on this computer — NOT for redistribution"*, the complete third-party notices (with their digest and the open legal points), every filesystem effect (relative to the manager folder), what will **not** happen, the rollback expectations, and what the confirmation is and is not. **Nothing has changed yet.**
7. **The person** ticks the acknowledgement and presses *Onaylıyorum ve kur*. (The agent never presses it.) Rust then, in this order: spends the single-use consent; rebuilds the preview and requires the same digest; re-reads the file from the input folder and re-checks size, format, SHA-256 and the authenticated reference; writes the mandatory *approval* audit row (if that fails: nothing happens); takes the tool's lock; writes the journal; stages a private copy; verifies it; creates the version folder; switches the one pointer file; writes the receipt; commits the journal; **reads the state back**; writes the outcome audit row.
8. The result shows the receipt (or the refusal / rollback report). Possible honest outcomes: `Tamamlandı` · `Reddedildi` (nothing changed) · `Başarısız — geri alındı` (with the rollback report) · `Yapıldı — ama dikkat gerekiyor` (audit row missing or read-back differs; the true state is shown; recovery is explicit).

## What exists afterwards

* `versions/jq/1.8.2/artifact.bin` (the file, byte-identical to the retained one), `state/jq.json` (active-version pointer), `receipts/<txn>.json`, `journal/<txn>.json`, and the audit rows in the application's database. **Nothing else.**
* The retained file and the input copy are untouched. No PATH entry, no agent configuration, no registry key, no shortcut, no scheduled task. **jq is never started**: the file is named `artifact.bin`, is not on any search path, and no code path of the application launches installed files.

## After

9. **Inventory 1** and a diff against inventory 0: only the manager-folder entries above (and the SQLite files) may be new. Any other difference stops the pilot and is reported **without deleting anything**.
10. Record, without changing any setting: whether Smart App Control or Defender reacted to the file being **written** (H2) — an event, a quarantine, a vanished file. A vanished or changed file is shown by the application as *needs attention* (`installed_artifact_missing` / `…_drift`), never repaired.
11. The installed-tool details show five live checks. The person decides whether to **remove** it (a second, separate preview and confirmation); a removal returns the folder to its pre-install inventory except for the journal, the receipt and the audit rows.

## Stop conditions (any one ends the pilot; nothing is deleted or "fixed")

A hash differs from the pin · the application refuses (read the code) · the preview shows anything not listed here · the diff shows an entry outside the manager folder · a SAC/Defender block or quarantine · a result other than `Tamamlandı` · any wish of the person.
