<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-25 | Updated: 2026-04-25 -->

# ton618

## Purpose
The task queue and scheduler binary. Named after the TON 618 black hole. Reads pending tasks from a data source, dispatches them to Laniakea workers via message queue, and runs scheduled jobs on fixed intervals or cron expressions.

## Key Files

| File | Description |
|------|-------------|
| `Cargo.toml` | Crate manifest — depends on anyhow, tokio, env_logger, cron, and dysonsphere |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `src/` | Binary source code (see `src/AGENTS.md`) |

## For AI Agents

### Working In This Directory
- This is a binary crate (`cargo run -p ton618`)
- Error handling uses `anyhow` — do NOT import `StellarisError` directly in ton618 logic
- Logging via `env_logger` + `log` macros (`log::info!`, `log::error!`, etc.)
- Depends on dysonsphere for core types; check dysonsphere API before adding new code

### Testing Requirements
```bash
cargo test -p ton618
cargo run -p ton618     # smoke test the binary
```

### Common Patterns
- `async-trait` for `TaskDataSource` and `Job` implementations
- `tokio::time::sleep` for polling loops
- Round-robin worker selection in task dispatcher

## Dependencies

### Internal
- `dysonsphere` — TaskMessage, TaskStatus, TaskTable, MessageQueue, StellarisError

### External
- `anyhow` — error handling
- `tokio` — async runtime
- `env_logger` / `log` — logging
- `cron` — cron expression parsing
- `serde` / `serde_json` — serialization
- `chrono` — timestamps

<!-- MANUAL: -->
