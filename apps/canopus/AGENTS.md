<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-09 | Updated: 2026-05-09 -->

# canopus

## Purpose
AI task execution engine orchestrating multi-stage agent pipelines (Analyst → Planner → Coder → Reviewer) for different task types (DevMode, BugFix, SecurityAudit, TestWriter, UXImprovement). Receives work intake from Laniakea (HTTP), executes agents via configurable runtimes (mock, Claude CLI, Codex CLI), persists artifacts per project under `.canopus/`, and delivers results back through Stellaris task lifecycle.

## Key Files

| File | Description |
|------|-------------|
| `src/main.rs` | CLI entrypoint; delegates to `cli::run()` after loading `.env` |
| `src/lib.rs` | Module root; exposes `adapters`, `cli`, `core`, `ports`, `test_support` |
| `src/core/pipeline.rs` | `Pipeline` enum mapping task types to agent role sequences; DevMode (default), BugFix, SecurityAudit, TestWriter, UXImprovement |
| `src/core/types.rs` | Core data structures: `Agenda`, `AgentTask`, `Artifact`, `AgentRole`, `AgentRunResult`, `StageRecord`, `ArtifactKind`, workflow metadata |
| `src/core/workflow.rs` | `WorkflowState` enum and transitions (Created → Planned → Executing → Checking → Reviewed → Completed; Failed allowed from any state) |
| `src/core/error.rs` | `CanopusError` and `CanopusResult` types |
| `src/core/run_identity.rs` | Deterministic ID generation and sanitization for agendas, run identity derivation |
| `src/cli/submit.rs` | `canopus submit` — main CLI: accepts task payload, orchestrates pipeline, persists run records, integrates with GitHub/Discord |
| `src/cli/finalize.rs` | `canopus finalize` — delivery notification (Discord webhook) and artifact cleanup |
| `src/cli/commands/` | Command handlers: `work_intake`, `project_register`, `status_artifacts`, `delivery_finalize`, `worktree` |
| `src/ports/agent_runtime.rs` | `AgentRuntime` trait; abstracts agent execution |
| `src/ports/artifact_store.rs` | `ArtifactStore` trait; abstraction for artifact persistence |
| `src/ports/task_backend.rs` | `TaskBackend` trait; marks task complete/failed in source system (Stellaris) |
| `src/ports/tool_gateway.rs` | `ToolGateway` trait; abstracts tool policy and script execution |
| `src/adapters/agent_runtime/` | Runtime implementations: `mock.rs`, `command.rs` (Claude/Codex via shell), `codex.rs` (Codex-specific) |
| `src/adapters/artifact_store/local_file.rs` | Persists artifacts in `.canopus/{agenda_id}/{stage}/` under project repo |
| `src/adapters/task_backend/stellaris.rs` | Updates task state in Stellaris backend via dysonsphere contract |
| `src/adapters/tool_gateway/local.rs` | Local script execution with policy gates |
| `src/adapters/github/` | GitHub integration: issue & project v2 APIs, sync plans, worktree gates |
| `tests/v1_smoke.rs` | End-to-end smoke test (mock runtime) |
| `tests/cli_submit.rs`, `tests/core_workflow.rs`, `tests/mock_agent_runtime.rs` | Integration tests for CLI, workflow, runtimes |
| `Cargo.toml` | Dependencies: `dysonsphere` (shared contracts), `tokio` (async), `serde`/`serde_json` (serialization), `chrono` (timestamps); dev: `laniakea` |
| `run.sh` | Wrapper: `exec cargo run -p canopus -- "$@"` |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `src/adapters/` | Port implementations (runtimes, stores, backends, gateways) |
| `src/adapters/agent_runtime/` | Claude, Codex, and mock agent runtimes |
| `src/adapters/artifact_store/` | Local file-based artifact storage |
| `src/adapters/task_backend/` | Stellaris task lifecycle integration |
| `src/adapters/tool_gateway/` | Tool policy and script execution |
| `src/adapters/github/` | GitHub API clients and sync logic |
| `src/cli/` | CLI argument parsing and command implementations |
| `src/cli/commands/` | Individual CLI command handlers |
| `src/core/` | Business logic: pipelines, types, workflow state, error handling |
| `src/ports/` | Trait definitions (interfaces) for runtime, storage, backends, gateways |
| `tests/` | Integration tests: smoke tests, runtime tests, workflow tests, backend tests |

## For AI Agents

### Working In This Directory

**Task Intake & Routing:**
- Tasks enter via `canopus submit --agenda-id <id> --request "..." --repo-path <path>` (CLI) or HTTP endpoint via Laniakea
- The `submit` command derives a run identity, selects a pipeline based on `task_type`, and orchestrates the agent sequence
- State is persisted in `.canopus/{agenda_id}/` inside the target project repo (derived from `repo_path`)

**Agent Runtimes:**
- Controlled by `CANOPUS_AGENT_RUNTIME` env var:
  - `mock` (default): returns deterministic fake results, no external calls
  - `claude`: shell wrapper calling `claude` (CLI) with `--max-output N` flag
  - `codex`: shell wrapper calling `codex exec --output-last-message` (extracts final ~5KB artifact, discards 2.4MB log)
- All runtimes return `AgentRunResult { message: String, artifacts: Vec<Artifact> }`

**Artifact Persistence:**
- Artifacts (plan, code, tests, review feedback) are stored per stage: `.canopus/{agenda_id}/{stage_name}/`
- `ArtifactKind` enum defines types: `Plan`, `Code`, `Test`, `Review`, `Analysis`
- Each artifact is a `serde_json` serialized `Artifact` struct with `role`, `kind`, `content`, `metadata`

**Workflow State & Persistence:**
- Every pipeline transition (Created → Planned → Executing → Checking → Reviewed → Completed) is recorded in `.canopus/{agenda_id}/run_records.json`
- On failure at any stage, the run transitions to `Failed`; `run_records` is persisted before returning error
- Use `try_stage!` macro to safely record stage failure and persist state

**GitHub Integration:**
- Tasks can be sourced from GitHub Issues or GitHub Project v2 cards
- Deterministic agenda IDs are derived from `(owner, repo, issue_number)` or `(project_url, item_id)` using `derive_run_identity`
- Use `GitHubClient` to fetch issue/project metadata; integrate with workflows via `GitHubProjectMetadata` and `GitHubIssueMetadata`

**Project Registration:**
- Projects are registered by `repo_path` via `canopus project-register --repo-path <path>`
- Registered projects are tracked in a manifest (location TBD in integration tests)

### Testing Requirements

1. **Unit Tests:** Test individual functions (pipeline selection, ID derivation, state transitions) in `src/core/` near implementation
2. **Integration Tests:** In `tests/`:
   - `v1_smoke.rs` — end-to-end smoke test with mock runtime
   - `cli_submit.rs` — CLI invocation and argument parsing
   - `core_workflow.rs` — full workflow orchestration with mock runtime
   - `mock_agent_runtime.rs` — mock runtime behavior
   - `codex_agent_runtime.rs` — Codex runtime integration (requires `CANOPUS_AGENT_RUNTIME=codex`)
   - `local_file_artifact_store.rs` — artifact persistence and retrieval
   - `local_tool_gateway.rs` — tool policy enforcement
   - `multi_project_state_routing.rs` — state isolation across projects
   - `migration_inflight_guard.rs` — safe migration of in-flight runs
3. **Run Tests:**
   - `cargo test -p canopus` — run all tests with mock runtime (default)
   - `CANOPUS_AGENT_RUNTIME=codex cargo test -p canopus` — test with Codex (requires auth)
   - See `tests/` for `#[tokio::test]` macros and `tokio::sync::Mutex` for ENV_LOCK (multi-test safety)

### Common Patterns

**Selecting a Pipeline:**
```rust
let pipeline = Pipeline::from_task_type(&task.task_type);
let roles = pipeline.agent_roles(); // Vec<AgentRole>
```

**Creating an Artifact:**
```rust
let artifact = Artifact {
    agenda_id: agenda.id.clone(),
    stage_name: AgentRole::Planner.as_str().to_string(),
    role: AgentRole::Planner,
    kind: ArtifactKind::Plan,
    content: plan_text,
    metadata: Some(serde_json::json!({})),
};
artifact_store.save(&artifact)?;
```

**Recording a Stage:**
```rust
let stage = StageRecord::new(stage_name);
records.push(stage.record("success", vec![artifact]));
persist_run_records(&state_path, &agenda_id, &records)?;
```

**Calling a Runtime:**
```rust
let task = AgentTask::for_agenda(agenda_id, &agenda, AgentRole::Planner);
let result = runtime.run(&task, &context, &prior_artifacts).await?;
// result.message is the agent response; result.artifacts are new artifacts
```

**Deriving a Deterministic ID:**
```rust
let run_id = derive_run_identity(&format!("{}:{}", owner, repo))?;
```

**Handling GitHub Issues:**
```rust
let agenda = Agenda::from_github_issue("owner", "repo", 42, "Fix the bug")?;
// Automatically derives deterministic ID from (owner, repo, number)
```

**Finalizing a Run:**
- Use `canopus finalize --agenda-id <id> --repo-path <path>` to notify Discord and clean up temporary state
- Integrates with `notify_discord()` webhook handler

### Local commit auto-flow (V1.5)

**Trigger:** When `CANOPUS_ALLOW_LOCAL_COMMIT=1` and a task arrives via watch with `approval_state == "approved"`, Canopus automatically:
1. Creates a new branch matching pattern `canopus/<task-slug>-<run-id-short>` (with `-N` suffix on collision)
2. Commits all changes with an AI-generated message in `[module] type: summary` convention

**Pre-flight requirements** (target project must satisfy):
- `.canopus/` MUST be in `.gitignore`
- Working branch must NOT be detached HEAD
- Index must be clean (no pre-staged changes)

**Commit message format:**
```
[<modules>] feat: complete agenda <id>


User-Request: !run <original Discord command>

Co-Authored-By: Canopus <noreply@stellaris.local>
Agent-Runtime: <runtime> (<model>)
```

**Rollback recipe:**
```bash
git checkout main
git branch -D <branch>
```

**What does NOT happen automatically:**
- No `git push` (push to origin still requires manual action or future V2 gate)
- No PR creation (V2)
- No merge or deploy (V2/V3)

**Limitations (V1.5):**
- `user_request`, `reviewer_summary`, `body` fields are currently empty in commit body — those will be plumbed in a follow-up
- `commit_type` is hardcoded to `feat` for V1.5; type inference is a follow-up

## Dependencies

### Internal
- **dysonsphere:** Shared task contracts (`TaskType`, task state enums, artifact schemas)
- **laniakea:** Dev-only dependency for integration tests; provides HTTP task intake mock

### External
- **tokio (1.44.2):** Async runtime (`rt-multi-thread`, `macros`, `time`, `sync`)
- **async-trait (0.1):** Async trait methods
- **serde / serde_json (1):** Serialization/deserialization for artifacts and metadata
- **ureq (2):** Synchronous HTTP client (GitHub API, Laniakea callbacks)
- **chrono (0.4.40):** Timestamps for run records and artifacts
- **log (0.4):** Structured logging
- **dotenvy (0.15):** `.env` file loading

### Behavioral Features
- **Environment Variables:**
  - `CANOPUS_AGENT_RUNTIME` — runtime selection: `mock`, `claude`, `codex` (default: `mock`)
  - `CLAUDE_API_KEY` — Claude CLI authentication (if using `claude` runtime)
  - `CODEX_API_KEY` — Codex CLI authentication (if using `codex` runtime)
  - `.env` loaded via `dotenvy::dotenv().ok()` in `main.rs`
- **State Isolation:** Each project's `.canopus/` is independent; state paths are derived from `repo_path`
- **Artifact Extraction:** Codex runtime uses `--output-last-message` to avoid storing 2.4MB logs; keep only final response (~5KB)
- **Clean Git Requirement:** `canopus submit` requires clean working tree in target project repo

<!-- MANUAL: -->
