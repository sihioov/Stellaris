<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-25 | Updated: 2026-04-25 -->

# dysonsphere

## Purpose
The only library crate in the Stellaris workspace. Provides shared core types, abstractions, and implementations for task messaging, database persistence, and message queue communication. All other crates depend on dysonsphere.

## Key Files

| File | Description |
|------|-------------|
| `Cargo.toml` | Crate manifest — depends on serde, tokio, async-trait, lapin (RabbitMQ), chrono |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `src/` | Library source code (see `src/AGENTS.md`) |

## For AI Agents

### Working In This Directory
- This is a `lib` crate — changes here affect all consuming crates (ton618, laniakea, hubble)
- Error handling must use `StellarisError`, never `anyhow`
- All trait methods that involve I/O must be `async`
- Run `cargo check --workspace` after any change to catch downstream breakage

### Testing Requirements
```bash
cargo test -p dysonsphere
cargo check --workspace   # verify no downstream breakage
```

### Common Patterns
- Trait-based abstractions: `TaskTable`, `MessageQueue`
- Serde derive on all message/status types
- `async-trait` crate for async trait methods

## Dependencies

### External
- `serde` / `serde_json` — serialization
- `tokio` — async runtime (sync feature for mutexes)
- `async-trait` — async trait method support
- `lapin` — RabbitMQ AMQP client
- `chrono` — date/time with serde support
- `futures-util` — async utilities

<!-- MANUAL: -->
