<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-09 | Updated: 2026-05-09 -->

# apps/canopus/src

## Purpose
Core Canopus source organized in a ports-and-adapters (hexagonal) architecture. Business logic lives in `core/`, trait contracts in `ports/`, concrete implementations in `adapters/`, and the CLI entry points in `cli/`.

## Key Files

| File | Description |
|------|-------------|
| `lib.rs` | Crate library root — re-exports public API for integration tests |
| `main.rs` | Binary entry point — sets up the HTTP server that Laniakea calls |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `core/` | Business logic: pipeline orchestration, workflow, run identity, shared types, error types (see below) |
| `ports/` | Trait definitions (interfaces): `AgentRuntime`, `ArtifactStore`, `TaskBackend`, `ToolGateway` |
| `adapters/` | Concrete implementations of ports: Codex/mock agent runtimes, local artifact store, GitHub task backend, local tool gateway |
| `cli/` | CLI sub-commands: `submit` (initiates a run), `finalize` (closes a run), `args` (shared CLI arg parsing) |

## core/ Files

| File | Description |
|------|-------------|
| `pipeline.rs` | Multi-stage pipeline: Analyst → Planner → Coder → Reviewer stage orchestration |
| `workflow.rs` | High-level workflow: wraps pipeline with pre/post hooks, task state transitions |
| `run_identity.rs` | Stable run ID generation from task payload (used to scope `.canopus/` state paths) |
| `types.rs` | Shared domain types: `AgentRole`, `ArtifactKind`, `AgentMessage`, `AgentRunResult`, `StageResult` |
| `error.rs` | `CanopusError` enum — covers runtime, IO, config, and agent failures |
| `module_derivation.rs` | Pure function `derive_modules(&[PathBuf]) -> Vec<String>` — maps changed file paths to module names via static prefix table |
| `branch_naming.rs` | Pure functions `derive_branch_name(user_request, run_id) -> String` and `with_collision_suffix(base, n) -> String` — slugify Discord command into kebab-case branch name |
| `commit_message.rs` | Free function `format_commit_message(...)` — assembles `[module] type: summary` + body + trailers (Canopus identity + Agent-Runtime) |

## ports/ Files

| File | Description |
|------|-------------|
| `agent_runtime.rs` | `AgentRuntime` trait — `run_stage(role, context) → AgentRunResult` |
| `artifact_store.rs` | `ArtifactStore` trait — `write_artifact(kind, role, content)`, `read_artifact(kind, role)` |
| `task_backend.rs` | `TaskBackend` trait — task status updates, finalization records |
| `tool_gateway.rs` | `ToolGateway` trait — `ensure_clean_worktree()`, `changed_files()`, git operations |

## adapters/ Subdirectories

| Directory | Purpose |
|-----------|---------|
| `agent_runtime/` | `codex.rs` (Codex CLI via `--output-last-message`), `mock.rs`, module glue |
| `artifact_store/` | `local.rs` — persists artifacts to `.canopus/artifacts/<run_id>/<role>/` |
| `task_backend/` | `stellaris.rs` — updates task state via Dysonsphere `FileTaskTable` |
| `tool_gateway/` | `local.rs` — implements `ensure_clean_worktree()` (git status check) and `changed_files()` |
| `github/` | GitHub API adapter for issue/PR creation (P2+ feature) |

## cli/ Files

| File | Description |
|------|-------------|
| `submit.rs` | `canopus submit` — validates payload, derives state root from `repo_path`, invokes workflow |
| `finalize.rs` | `canopus finalize` — closes the run, writes finalization record, triggers delivery gate |
| `args.rs` | Shared CLI argument parsing; `derive_state_for_run()` / `StateSource` enum for payload-driven state routing |
| `commands/` | Sub-command definitions |

## For AI Agents

### Working In This Directory
- **Ports define the contract; adapters fulfill it** — when adding a new runtime or store, implement the trait in `ports/` first, then write the adapter
- **State root is always derived from task payload** `repo_path` via `derive_state_for_run()` in `args.rs` — never use the server's CWD as state root
- **Artifact content for Codex**: always use `final_message` (~5KB from `--output-last-message`) — never `runtime_log` (2.4MB full stdout). See `adapters/agent_runtime/codex.rs`
- **Clean worktree check**: `cli/submit.rs` calls `tool_gateway.ensure_clean_worktree()` before running — target project must have `.canopus/` in `.gitignore`

### Testing Requirements
- `cargo test -p canopus` — all unit + integration tests
- `cargo test -p canopus --test v1_smoke` — end-to-end smoke test
- `cargo test -p canopus --test cli_submit` — submit path tests
- `cargo test -p canopus --test finalize` — finalization path tests

### Common Patterns
- `AgentRole::Planner | AgentRole::Reviewer` roles produce deliverable artifacts; `Analyst | Coder` roles produce intermediate artifacts
- All roles store only `final_message` as artifact content (not `runtime_log`)
- `run_identity` hash is derived deterministically from task `id` + `repo_path`

## Dependencies

### Internal
- `dysonsphere` — shared `TaskMessage`, `TaskStatus`, `FileTaskTable` contracts

### External
- `tokio` — async runtime
- `serde` / `serde_json` — payload serialization
- `ureq` — HTTP client (GitHub API calls)
- `chrono` — timestamps in finalization records

<!-- MANUAL: -->
