<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-09 | Updated: 2026-05-09 -->

# europa

## Purpose
Discord control surface for the Stellaris AI task system. Translates Discord slash commands and `!run` messages into Canopus task requests via HTTP. Routes per-guild projects to the correct git repository and manages Discord-side state (task metadata, project registration, worktree tracking).

## Key Files
| File | Description |
|------|-------------|
| `europa.py` | Main Discord bot entrypoint; event loop and command handlers |
| `canopus_client.py` | HTTP subprocess client that shells out to Canopus for `project-register`, `work-intake`, and `worktree create` operations |
| `config.py` | Environment variable loading and static maps (channel types, status icons, authorization) |
| `payloads.py` | Task payload construction, GitHub agenda ID generation, Discord link builders |
| `projects_store.py` | Project registration persistence; reads/writes `projects.json`, manages worktree state, hydrates legacy records |
| `tasks_store.py` | Task queue JSON file access with exclusive locking (fcntl on Unix, fallback on Windows) |
| `projects.json` | Live project registry: guild category ID → project config (repo_path, GitHub fields, worktrees) |
| `projects.json.example` | Template for new deployments |
| `tasks.json` / `tasks.lock` | Live task state files (shared with TON618/Laniakea in v1 local pipeline) |
| `requirements.txt` | Python dependencies (discord.py, python-dotenv) |
| `run.sh` | Startup wrapper script |
| `README.md` | User-facing setup guide and command reference |
| `test_bot_config.py` | Config validation tests and integration test suite |

## For AI Agents

### Working In This Directory

**Project and Worktree State**
- Projects are keyed by Discord category ID (string); each project record is in `projects_store.PROJECTS_JSON_PATH`
- Call `normalize_project_worktrees(project)` before reading or mutating worktree state — this hydrates legacy records and ensures `base_repo_path`, `active_worktree`, and `worktrees` dict exist
- Worktrees dict maps name → `{repo_path, created_at?}`; the active worktree's `repo_path` is the `repo_path` field at the top level
- When a project is created or `!register` is called, write the result with `write_projects()` using `merge_project_registration()` for upsert semantics

**Task Submission and GitHub Integration**
- `!run <request>` creates a task with `build_task_payload()` — always includes `repo_path`, `agenda_id`, role mode, and Discord message link
- For GitHub-backed projects (when `project.github_owner` and `project.github_repo` exist), call `intake_github_work()` before appending the task
- `intake_github_work()` returns `(result, error)` tuple where `result` is the Canopus `work-intake` JSON response (e.g., `github_issue_number`, `github_project_item_id`) or `None` on failure
- If intake fails, do not append the task; store failure metadata in the task payload and keep the proposal in `PendingProposal` status for retry
- GitHub Project v2 mutations are mutating by default only when `CANOPUS_GITHUB_PROJECT_MODE` is not in `NON_MUTATING_GITHUB_PROJECT_MODES`

**Payload Construction**
- `build_task_payload()` takes context, task_id base, request text, project dict, and role mode → returns dict for JSON serialization
- Agenda ID priority: explicit issue number (from intake result or env) → `gh-{owner}-{repo}-{number}` (deterministic, matches Rust); fallback to `agenda-{task_id}`
- Use `deterministic_agenda_id_for_github_issue()` to match Canopus run-identity sanitization rules (lowercase, dash-collapse, ASCII alphanumeric only)
- Status emoji map in `config.py` maps task status → Discord emoji for `!status` output

**Task Status and Approval**
- Tasks flow through: `PendingProposal` → `Pending` → `PendingReview` → `Processed` (approved) or `Failed` (rejected)
- Approval via `!approve` calls `mark_task_approved()` with optional provenance metadata (user ID, Discord message URL)
- Rejection via `!reject` calls `mark_task_rejected()`; both update task `meta` (for quick reads) and payload JSON (for Canopus)
- `finalize_requested_at` is set only when a task moves to `Processed` — this signals Canopus finalization owner to proceed

**File-level Locking**
- All task writes go through `task_file_lock()` which acquires exclusive lock on `.lock` file (fcntl on Unix, silently no-op on Windows dev)
- Task reads can use shared locking; writes always exclusive
- This allows concurrent access while preventing `tasks.json` corruption in multi-process setup (v1 local TON618 + Europa + Laniakea)

### Testing Requirements

**Unit Tests in test_bot_config.py**
- Config validation: env overrides, ALLOWED_USER_IDS parsing, per-category `tasks.json` path vs. shared path
- Worktree normalization: legacy record hydration, default worktree injection, active worktree switching
- Agenda ID generation: deterministic `gh-{owner}-{repo}-{number}`, uppercase/space sanitization, non-ASCII fallback
- Task payload: includes repo_path, agenda_id, role_mode, GitHub metadata, Discord provenance, GitHub Project mode filtering
- Approval/rejection: status transitions, metadata recording, finalize signal setting
- GitHub integration: work-intake call signature, partial-failure handling, proposal promotion
- Worktree operations: validation (unsafe names, duplicates), creation via Canopus, success/failure side effects

**Running Tests**
```bash
cd /home/sihioov/project/stellaris/Stellaris/apps/europa
python -m pytest test_bot_config.py -v
# or
python test_bot_config.py
```

### Common Patterns

**Registering a New Project**
1. User calls `!new-project <name> <repo_path>` (or `!register <repo_path>` in existing category)
2. Parse and validate repo_path and optional GitHub flags (`--github owner/repo --project-owner org:name`)
3. If GitHub opts present: call `register_github_project()` → Canopus validates and returns IDs (strict-live)
4. Merge result into project record with `merge_project_registration()`
5. Write to `projects.json` with `write_projects()`

**Creating a Worktree**
1. User calls `!worktree create <name>`
2. Validate name with `validate_worktree_name()` (no `..`, no leading `.`, alphanumeric + `._-` only)
3. Call `create_worktree(base_repo_path, name)` → Canopus creates git worktree and returns result
4. On success: call `record_project_worktree()`, write projects, report success with path
5. On failure: do not update projects.json; report error

**Switching Active Worktree**
1. User calls `!worktree switch <name>`
2. Call `switch_project_worktree(project, name)` → validates name exists and has repo_path
3. Returns updated project record; write it with `write_projects()`
4. Future `!run` commands use the new active worktree's repo_path in payload

**Handling !run Submission with GitHub Integration**
1. Validate user is authorized (check `ALLOWED_USER_IDS`)
2. Get or create project record for category_id
3. Call `normalize_project_worktrees()` to ensure state is hydrated
4. Build payload with `build_task_payload()` — includes active worktree's repo_path
5. If `github_owner` and `github_repo` present:
   - Call `intake_github_work(project, task_id, agenda_id, request, discord_message_url)`
   - On error: send failure message, record error in task payload, keep PendingProposal status, do NOT append
   - On success: merge intake response into payload (github_issue_number, github_project_item_id, etc.)
6. Append task with `append_task_locked(tasks_path, task)`

**Environment Variables for GitHub**
- `GITHUB_OWNER` / `GITHUB_REPO` — used when no intake result (fallback for agenda ID and issue URL generation)
- `GITHUB_PROJECT_ID` — GraphQL node ID (copied into payload metadata for GitHub Project mutations)
- `GITHUB_PROJECT_OWNER_KIND` / `GITHUB_PROJECT_OWNER` / `GITHUB_PROJECT_NUMBER` — lookup metadata when node ID absent
- `CANOPUS_GITHUB_PROJECT_MODE` — default `dry-run-offline`; set to `mutate-live` only with appropriate Canopus gates enabled
- `CANOPUS_ALLOW_GITHUB_REGISTRATION_MUTATION`, `CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION`, `CANOPUS_ALLOW_GITHUB_REPO_CREATE` — gates for live mutations (default 0)
- `EUROPA_INTAKE_FAILURE_LOCAL_ONLY=1` — makes intake failures non-fatal (useful for dev without GitHub)

## Dependencies

### Internal
- `dysonsphere` — shared task/payload contracts (via Canopus subprocess boundary, not direct import)
- `canopus` — invoked as subprocess via `CANOPUS_COMMAND` env var; calls to `project-register`, `work-intake`, `worktree create`
- `laniakea` — (optional, v1 local integration only) shares `tasks.json` when `TASKS_JSON_PATH` is set
- `ton618` — (optional, v1 local integration only) reads task status from same shared `tasks.json`

### External
- `discord.py>=2.0.0` — Discord bot client and event loop
- `python-dotenv>=1.0.0` — `.env` file loading

<!-- MANUAL: -->
