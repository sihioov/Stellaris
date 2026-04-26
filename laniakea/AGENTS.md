<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-25 | Updated: 2026-04-25 -->

# laniakea

## Purpose
The task worker binary. Named after the Laniakea Supercluster. Receives TaskMessages dispatched by TON618, processes them (AI/data processing), and saves results to Redis. Worker instances use galaxy names (e.g., Andromeda, M87). Currently a placeholder — only `main.rs` exists.

## Key Files

| File | Description |
|------|-------------|
| `Cargo.toml` | Crate manifest |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `src/` | Binary source code (see `src/AGENTS.md`) |

## For AI Agents

### Working In This Directory
- This crate is a placeholder — core processing logic is not yet implemented
- When implemented, it will consume tasks from RabbitMQ and write results to Redis
- Worker instances should be named after galaxies (Andromeda, M87, Milky Way, etc.)
- Will consume `dysonsphere::message::TaskMessage` for the task payload format

### Testing Requirements
```bash
cargo check -p laniakea
```

## Dependencies

### Internal
- Will depend on `dysonsphere` for TaskMessage consumption when implemented

### External (planned)
- Redis client for result persistence

<!-- MANUAL: -->
