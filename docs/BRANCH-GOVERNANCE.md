# Branch Governance — main + development

**Status:** USER-APPROVED (2026-10-17)

## Long-lived branches
- `main`: last independently validated release/baseline. No direct implementation commits. Promote only from tested `development` after Red Team approval, exact commit-SHA CI checks and explicit user approval. Keep release/phase evidence in GitHub docs, issues, tags and commits rather than creating phase branches.
- `development`: the **only normal development branch**. Each numbered phase is tracked via a phase-specific directory/report, commit range, CI evidence and an issue/milestone, not a new branch.

## Source of truth
- Phase 0 archived baseline: `68eda3f0407490fabccb517dcf822ab4818fc0bf`; preserved on `main` and in published Phase 0 files.
- Approved Prompt Intelligence architecture is on `development`; initial sources: prompts.chat, the-prompt-library and Fabric, classified as **untrusted read-only reference data**, not executable installers.
- Current near-term work is Phase 1A only; no production installs, agent config mutations, auto-updates or live cloud sync.

## Promotion workflow
1. Develop and test on `development`, including negative/adversarial tests.
2. Freeze exact candidate commit and review diff against `main`.
3. Require green CI for that exact SHA and independent Red Team PASS.
4. Obtain user's authorization, then promote without losing or rewriting history (prefer FF-only when possible).
5. Never silently advance `main` from agent work, an unrelated feature proposal or a documentation edit.

## Legacy branches and repo setting
Initial one-off branches `phase0/architecture` and `phase1a/desktop-shell` are deprecated. Their necessary committed content is already in `main` (Phase 0) and `development` (Prompt Intelligence ADR). **Do not delete branches until the remote default branch has been changed to `main`, the ancestry and file content are independently verified and the user has verified no worktrees or unpushed local changes depend on them.**

The GitHub connector currently does not provide repository-default-branch changes or remote-branch deletion. The user or an explicitly authorized repository operator must finish this cleanup using GitHub repository settings and carefully validated Git operations.

## Local safety
Never change an agent's working branch or delete a local worktree without inspecting its current uncommitted files. Keep generated, private and sensitive data out of public Git history.
## Legacy branch cleanup — completed 2026-10-17
Verified before deletion: remote default branch is `main`; `main` = `68eda3f`; `phase0/architecture` (`68eda3f`) is `main`; `phase1a/desktop-shell` (`6e427d1`) is an ancestor of `development`; zero commits unique to either branch; no open PRs, tags, protection, local worktrees, stashes or unpushed commits depend on them. Both branches were then deleted from `origin` with a normal (non-force) delete. Their tips remain reachable from `main`/`development`.
