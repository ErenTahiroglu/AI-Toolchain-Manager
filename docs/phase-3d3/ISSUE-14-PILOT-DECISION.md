# Issue #14 — the limited, single-machine pilot path (Phase 3D-3)

**Issue #14 stays open.** The independent Red Team allowed a narrower acceptance path for **one explicitly authorised, local, installation-only pilot** on the existing Windows development computer; a second physical Windows computer is **not** mandatory for that single experiment. This is not a claim about any other computer, profile, file system or security product, and it does not authorise distribution. The assessment of Phase 3D-2 ([ISSUE-14-LIMITED-PILOT-ASSESSMENT](../phase-3d2/ISSUE-14-LIMITED-PILOT-ASSESSMENT.md)) stands; this note adds how the remaining Windows pointer-replacement race is handled conservatively.

## The race, and what the pilot does about it

On Windows a destination file cannot be pinned against deletion and replaced in one operation: the identity of the file to be replaced is checked immediately before the replacing rename, and **a window remains**. For the pilot this means:

| Rule | How it is enforced (and tested) |
| --- | --- |
| **Never overwrite an unrelated file** | A first installation *creates* the pointer with a no-clobber move; an existing file of that name is refused atomically. Replacement exists only for an update (not offered in the pilot: the recipe lists install and uninstall only). The journal write replaces only the one file this transaction last wrote or read. |
| **Refuse unexpected identity or conflicting entries** | A foreign file or folder in the version folder, a link in place of a managed folder, a changed pointer, a second link to the artifact, a different size or digest are refused or reported as drift (`install_flow_tests.rs`: foreign files, link redirection, tampering at four places; `artifact_provider_tests.rs`: folder, hard link, symbolic link, link as the input folder). |
| **Verify state after the operation** | New in this phase: after every completed operation the state is **read back** independently of the transaction's own bookkeeping; a result other than the expected one is `needs_attention` (`post_verification_failed`) and is audited as `unknown`, never as a success (test: the installed file altered inside the window right before the journal commits). |
| **Report unresolved ownership as NeedsAttention** | A file whose ownership cannot be proven (the create-to-identity window, an unreadable journal) is kept and reported on every recovery; the tool stays locked from new operations (tested at the stage file and the pointer temp file). |
| **Preserve user files and unrelated content** | Rollback and recovery delete only a file that matches its journal record (identity, single link, content). The audit reconciliation touches no file at all. The tool's own data folder is never read, changed or deleted (tested byte-for-byte). |

## Evidence prepared for the pilot

* **Before/after inventories** (`tools/fs_inventory.mjs`: read-only listing without following links, sizes, SHA-256): the real-window run records `inventory-0-start`, `-1-before-hello-install`, `-2-after-install`, `-3-final` and their diff; the automated test compares the whole test tree before and after an install/removal cycle (only the manager's journal and receipts are new). The same tool is the inventory of the real pilot ([PILOT-PROCEDURE](PILOT-PROCEDURE.md)).
* **Failure-recovery evidence:** crash injected at 13 points → recovered after a restart; failures at 5 points → rolled back with a `restored` report; open file (Windows) → removal stops and everything is put back; audit failures → needs-attention and reconciliation ([AUDIT-INTEGRITY](AUDIT-INTEGRITY.md)).
* **Local NTFS acceptance probe** and the Smart App Control record: [ISSUE-14-LIMITED-PILOT-ASSESSMENT](../phase-3d2/ISSUE-14-LIMITED-PILOT-ASSESSMENT.md), [SMART-APP-CONTROL-RECORD](SMART-APP-CONTROL-RECORD.md). No Windows security setting was touched.

## What a general release still needs (unchanged)

An independent physical Windows 11 PC, anti-virus and OneDrive/mount-point behaviour, a non-ASCII account path, a signed application, and for macOS a real Apple Silicon Mac — see Phase 3D-1's H1–H8. **The pilot's result must be recorded as "works on this one machine".**
