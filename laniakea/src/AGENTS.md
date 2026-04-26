<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-25 | Updated: 2026-04-25 -->

# src (laniakea)

## Purpose
Entry point for the Laniakea worker binary. Currently a placeholder with only `main.rs`. Will consume `TaskMessage` from RabbitMQ, perform AI/data processing, and write results to Redis.

## Key Files

| File | Description |
|------|-------------|
| `main.rs` | Binary entry point — placeholder only |

## For AI Agents

### Working In This Directory
- Do not implement worker logic until the task processing pipeline is defined
- Worker instance naming convention: galaxy names (Andromeda, M87, Milky Way, etc.)
- Will subscribe to RabbitMQ queues using `dysonsphere::mq::MessageQueue`
- Processing results go to Redis — add Redis dependency in Cargo.toml when implementing

### Testing Requirements
```bash
cargo check -p laniakea
```

<!-- MANUAL: -->
