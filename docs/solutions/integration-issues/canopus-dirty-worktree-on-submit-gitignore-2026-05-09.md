---
title: canopus submit Fails with "worktree is not clean" Due to Untracked .canopus/ Directory
date: 2026-05-09
category: integration-issues
module: canopus/submit
problem_type: integration_issue
component: tooling
severity: medium
symptoms:
  - "canopus submit rejects run with \"worktree is not clean\""
  - ".canopus/ state directory appears as untracked files in git status"
  - No actual uncommitted source changes exist; only the ephemeral state directory is untracked
  - Affects any new project repo registered with Canopus that lacks a .gitignore entry for .canopus/
root_cause: config_error
resolution_type: config_change
related_components:
  - development_workflow
tags:
  - canopus
  - gitignore
  - dirty-worktree
  - submit
  - state-directory
---

# canopus submit Fails with "worktree is not clean" Due to Untracked .canopus/ Directory

## Problem

`canopus submit` rejected a project with a "worktree is not clean" error because the `.canopus/` state directory — created automatically by Canopus on first run — was untracked in the project's git repository, making the working tree appear dirty to Canopus's pre-submit cleanliness check.

## Symptoms

- `canopus submit` exits with "worktree is not clean" for a newly registered project
- `git status` inside the project shows `.canopus/` as an untracked directory
- No actual uncommitted source changes exist — only the ephemeral state directory is untracked
- Issue affects any project repo that has never had a `.canopus/` entry in `.gitignore`

## What Didn't Work

Root cause was identified directly from `git status` output — no failed investigation paths.

## Solution

Add `.canopus/` to the project's `.gitignore` and commit the change:

```bash
echo '.canopus/' >> /path/to/your/project/.gitignore
git -C /path/to/your/project add .gitignore
git -C /path/to/your/project commit -m "[infra] ignore .canopus/ state directory"
```

After this commit, `git status` reports a clean working tree and `canopus submit` proceeds normally.

## Why This Works

`.canopus/` stores per-project run state: artifacts, logs, and finalize records written by the Canopus runtime. It is ephemeral build output, equivalent to a `.build/` or `dist/` directory — correct to ignore, never to track. Canopus's submit path runs `git status --porcelain` and blocks on any non-empty output. Once `.canopus/` is gitignored, git no longer reports it, the working tree is clean, and submit passes.

Note: `changed_files()` in `apps/canopus/src/adapters/tool_gateway/local.rs` already filters `.canopus` paths, but the upstream `ensure_clean_worktree()` check does not share that filter, so the untracked directory is still visible to the pre-submit gate.

## Prevention

- Every repository registered as a Canopus project must have `.canopus/` in its `.gitignore` — add this as a documented prerequisite in the project registration runbook
- The `!register` registration flow should auto-detect the absence of a `.canopus/` gitignore entry and either add it automatically or warn before the first run
- Canopus could self-gitignore `.canopus/` the first time it writes to that directory (similar to how some tools write their own `.gitignore` on init)
- The "worktree is not clean" error message could list the specific untracked paths so users can diagnose the cause without running `git status` manually

## Related Issues

- No GitHub issues on file
- `docs/archive/superpowers/plans/2026-04-25-canopus-local-patch-mvp.md` contains the original `worktree is not clean` error string and the `changed_files` `.canopus` filter — that plan doc predates this fix
- `docs/v1-operator-runbook.md` covers the submit flow but does not document this setup prerequisite
