<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-09 | Updated: 2026-05-09 -->

# kepler

## Purpose
Codebase scanner that continuously monitors the repository for potential improvements, bugs, and findings, and registers them as `PendingProposal` tasks in the shared task table. Runs on a 1-hour polling loop.

## Key Files

| File | Description |
|------|-------------|
| `Cargo.toml` | Crate manifest — depends on `dysonsphere` for `FileTaskTable` and `register_discoveries` |
| `src/main.rs` | Entry point: reads `REPO_PATH`, `TASKS_JSON_PATH`, `KEPLER_SEEN_JSON_PATH` from env; runs the hourly scan loop |
| `src/scanner.rs` | Implements `scan()` — walks the repo and emits `Discovery` structs (Bug, Improvement, etc.) |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `src/` | All Kepler source code (see `src/AGENTS.md`) |

## For AI Agents

### Working In This Directory
- Kepler writes its de-duplication state to `.canopus/kepler/seen.json` inside the target repo (controlled by `KEPLER_SEEN_JSON_PATH`)
- `register_discoveries` is idempotent: the same finding is only registered once even across restarts
- Add new discovery types by extending `DiscoveryKind` in `scanner.rs` and implementing the `Discovery` trait from `dysonsphere`

### Testing Requirements
- `cargo test -p kepler` — runs the idempotency test in `main.rs`
- Test fixture: creates a temp dir, runs `register_discoveries` twice, asserts count == 1 on second call

### Environment Variables
- `REPO_PATH` — path to the repository to scan (defaults to CWD)
- `TASKS_JSON_PATH` — path to tasks.json (defaults to `tasks.json`)
- `KEPLER_SEEN_JSON_PATH` — path to seen.json dedup state (defaults to `$REPO_PATH/.canopus/kepler/seen.json`)

## Dependencies

### Internal
- `dysonsphere` — `FileTaskTable`, `register_discoveries`, `Discovery` trait

### External
- `tokio` — async runtime for the hourly sleep loop
- `log` / `env_logger` — structured logging
- `dotenvy` — `.env` file loading

<!-- MANUAL: -->
