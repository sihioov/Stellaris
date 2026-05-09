<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-09 | Updated: 2026-05-10 -->

# docs/solutions

## Purpose
Searchable knowledge store of documented solutions to past problems (bugs, best practices, workflow patterns, architecture decisions). Each file has YAML frontmatter with `module`, `tags`, `problem_type`, `component`, `severity` fields for structured search. Created and maintained by the `/ce-compound` skill.

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `runtime-errors/` | Runtime crash and error solutions |
| `integration-issues/` | Integration and configuration failure solutions |
| `architecture-patterns/` | Reusable architectural patterns (pipeline shape, mode dispatch, structural decisions) |

## For AI Agents

### Working In This Directory
- Search here before implementing features or debugging issues in documented areas
- Use frontmatter fields to filter: `grep -r "module: canopus" docs/solutions/` or `grep -r "tags:.*codex" docs/solutions/`
- Files are named `[problem-slug]-[YYYY-MM-DD].md`
- Do NOT edit these files directly — use `/ce-compound` to create new entries or `/ce-compound-refresh` to update stale ones

### Common Patterns
- `problem_type` determines the track: bug track (`runtime_error`, `integration_issue`, etc.) or knowledge track (`best_practice`, `workflow_issue`, etc.)
- Each doc contains: Problem, Symptoms, What Didn't Work, Solution, Why This Works, Prevention

## Currently Documented Solutions

| File | Problem |
|------|---------|
| `runtime-errors/codex-context-overflow-prior-artifact-2026-05-09.md` | Codex agent runtime stores 2.4MB runtime_log as artifact instead of ~5KB final_message, causing context overflow in downstream stages |
| `integration-issues/canopus-dirty-worktree-on-submit-gitignore-2026-05-09.md` | `canopus submit` rejects run because `.canopus/` state directory is untracked in project repo |
| `architecture-patterns/finalize-mode-enum-extension-2026-05-10.md` | Pattern for adding new privilege levels to mutation pipelines via enum variants instead of bool params or sibling functions (V1.5 LocalCommitOnly synthesis) |

<!-- MANUAL: -->
