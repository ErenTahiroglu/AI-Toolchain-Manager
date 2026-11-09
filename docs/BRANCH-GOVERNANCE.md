# Branch Governance — main + development

**Status:** USER-APPROVED (2026-10-17); reconciled with the promoted baseline in Stage 1A (2026-11-10). Sections marked *(historical)* record facts as they were on the stated date and are kept unchanged.

## Long-lived branches
- `main`: the stable, owner-approved, CI-qualified baseline, **with disclosed acceptance limitations** (see "Current baseline and bounded promotion exception"). It is not a production release and is not independently accepted. No direct implementation commits. Promote only from tested `development` (see the promotion workflow). Keep release/phase evidence in GitHub docs, issues, tags and commits rather than creating phase branches.
- `development`: the **only normal development branch**. Each numbered phase is tracked via a phase-specific directory/report, commit range, CI evidence and an issue/milestone, not a new branch.

## Source of truth
- Phase 0 archived baseline: `68eda3f0407490fabccb517dcf822ab4818fc0bf`; preserved in the history of `main` and `development` and in published Phase 0 files.
- Approved Prompt Intelligence architecture is on `development`; initial sources: prompts.chat, the-prompt-library and Fabric, classified as **untrusted read-only reference data**, not executable installers.
- *(historical, 2026-10-17)* Current near-term work is Phase 1A only; no production installs, agent config mutations, auto-updates or live cloud sync.
- Standing limits still apply: no production installs, agent config mutations, auto-updates or live cloud sync are authorized by this document.

## Promotion workflow (normal requirement for every future promotion)
1. Develop and test on `development`, including negative/adversarial tests.
2. Freeze exact candidate commit and review diff against `main`.
3. Require green CI for that exact SHA and independent Red Team PASS.
4. Obtain user's authorization, then promote without losing or rewriting history (prefer FF-only when possible).
5. Never silently advance `main` from agent work, an unrelated feature proposal or a documentation edit.

## Legacy branches and repo setting *(historical, 2026-10-17)*
Initial one-off branches `phase0/architecture` and `phase1a/desktop-shell` are deprecated. Their necessary committed content is already in `main` (Phase 0) and `development` (Prompt Intelligence ADR). **Do not delete branches until the remote default branch has been changed to `main`, the ancestry and file content are independently verified and the user has verified no worktrees or unpushed local changes depend on them.**

The GitHub connector currently does not provide repository-default-branch changes or remote-branch deletion. The user or an explicitly authorized repository operator must finish this cleanup using GitHub repository settings and carefully validated Git operations.

## Local safety
Never change an agent's working branch or delete a local worktree without inspecting its current uncommitted files. Keep generated, private and sensitive data out of public Git history.
## Legacy branch cleanup — completed 2026-10-17
Verified before deletion: remote default branch is `main`; `main` = `68eda3f`; `phase0/architecture` (`68eda3f`) is `main`; `phase1a/desktop-shell` (`6e427d1`) is an ancestor of `development`; zero commits unique to either branch; no open PRs, tags, protection, local worktrees, stashes or unpushed commits depend on them. Both branches were then deleted from `origin` with a normal (non-force) delete. Their tips remain reachable from `main`/`development`.

## Current baseline and bounded promotion exception
Recorded in Stage 1A (Issue #45). Facts below were verified against the repository and CI at that time.

- **Two long-lived branches only:** `main` and `development`. Both point to `c8b6e8013e6aa2eed514b7dc730d2eb98ba9b92e`. The remote has no other branches.
- **Shared baseline commit:** `c8b6e8013e6aa2eed514b7dc730d2eb98ba9b92e`, a normal (non-fast-forward) merge commit. Parent 1 is `development` at `82a1075c9f7ed2657511d7b66c324b00deb0a721` ("feat(catalog): add unreviewed EGM seed entry"); parent 2 is the old `main` license commit `f53a51dc9cbbc4509bfc3d7f24f7bdd7a26a51b0` (LICENSE-MAIN-01). The merge keeps that license history reachable. Compared with `82a1075`, it adds only `.github/workflows/license-check.yml` (macOS runner label `macos-15`).
- **Preserved original Phase 0 baseline:** `68eda3f0407490fabccb517dcf822ab4818fc0bf` (see Source of truth). It remains in history.
- **CI evidence (CI validation only):**
  - `development` at the merge commit: CI run `42694542085`, completed/success on Windows x64 and macOS ARM64.
  - `main` at the same commit: CI run `42698764130` (attempt 1), completed/success on Windows x64 and macOS ARM64; `license-check` run `42698764005` and `Dependency Graph` run `42698758882` also completed/success.
- **Obsolete review branches removed:** `review/atm-gov-02-p2c-ci-contract-r1`, `review/atm-gov-02-p2c-playwrightchoice-ci-r1` and `review/atm-gov-02-p2c-wp2-r1`. Each was an ancestor of `development` with no unique commits and no open PR, and was deleted from `origin` by exact name with a normal (non-force) delete. Their tips remain reachable from `main`/`development`.
- **Bounded owner exception (single, one-time):** the owner explicitly authorized promoting the CI-qualified `development` state to `main` although independent Red Team acceptance (step 3 of the promotion workflow) and GUI acceptance were incomplete. This exception covers that one promotion only. It does not set a precedent and does not change the requirements below.
- **What this baseline is not:** it is **not** independently accepted, **not** GUI-accepted and **not** a production release. No tag, release or binary was created. Disclosed limitations include: CATALOG-04 GUI verification blocked (Windows Smart App Control in R1; no isolated Windows environment in R2); VA-01/04/09 awaiting human review; Goldfish convergence `NOT_ACHIEVED`; the EGM catalog record remaining `unreviewed` / `manual-only` / `not_managed`; and other open ATM-GOV, configuration, installation-safety and security issues.

### Distinctions used in this document
- **CI validation:** automated checks passed for an exact SHA. It is evidence, not acceptance.
- **Independent acceptance:** a review by someone other than the author, recorded against a frozen SHA.
- **Owner exception:** an explicit, documented owner decision to proceed without a normally required step. It is not acceptance.
- **Production readiness:** a separate decision that has not been made for this repository.

### Future promotions
Every future promotion from `development` to `main` still requires **all** of: applicable tests, green CI for the exact candidate SHA, independent review (Red Team PASS), and explicit owner authorization. A step may be waived only if the owner separately documents a new exception for that promotion. Branch protection settings on GitHub are not changed by this document.
