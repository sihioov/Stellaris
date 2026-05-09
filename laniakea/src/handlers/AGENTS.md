<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-09 | Updated: 2026-05-09 -->

# laniakea/src/handlers

## Purpose
HTTP request handlers for the Laniakea task execution service. Each handler implements a specific intake route. The `custom` handler is the primary production path for Discord-initiated Canopus runs.

## Key Files

| File | Description |
|------|-------------|
| `mod.rs` | Module declaration — re-exports handler functions |
| `custom.rs` | Primary handler for Discord `!run` requests: validates payload, derives per-project `.canopus/` state root from task `repo_path` (payload-driven routing), spawns Canopus subprocess |
| `news_a.rs` | Handler for external news/signal intake (Hubble-originated tasks) |

## For AI Agents

### Working In This Directory
- `custom.rs` is the hot path — it was heavily modified in the multi-project state routing refactor (PR-A)
- State root derivation is now **payload-driven**: `repo_path` from the task body determines `.canopus/` location, not the server's working directory
- The `[submit] state_root resolved: <path> (source=payload_repo)` log line confirms correct routing
- Keep policy logic (which state to use, which project) in `custom.rs`; keep surface formatting in Europa (`apps/europa/`)

### Testing Requirements
- `cargo test -p laniakea` — runs handler integration tests in `laniakea/tests/`
- The `cli_submit` and `v1_smoke` tests in `apps/canopus/tests/` exercise the full path through this handler

### Common Patterns
- Each handler takes `Arc<AppState>` + `axum::extract::*` parameters
- Error responses use `canopus_error_response()` helper for consistent JSON error shape
- Payload validation happens at handler entry; downstream code can assume valid structure

## Dependencies

### Internal
- `dysonsphere` — `TaskMessage`, `TaskMeta`, `TaskType`, `TaskStatus` shared contracts
- `apps/canopus` — spawned as a subprocess by `custom.rs` (not linked as a library)

<!-- MANUAL: -->
