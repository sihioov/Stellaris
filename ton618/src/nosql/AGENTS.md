<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-25 | Updated: 2026-04-25 -->

# nosql

## Purpose
NoSQL data source implementations for TON618. Currently contains a MongoDB placeholder. Implements the `TaskDataSource` trait from `ton618::datasource` for document-store backends.

## Key Files

| File | Description |
|------|-------------|
| `mod.rs` | Module declaration |
| `mongo.rs` | MongoDB `TaskDataSource` implementation — placeholder, not yet implemented |

## For AI Agents

### Working In This Directory
- `mongo.rs` is a placeholder — the `TaskDataSource` trait is in `ton618::datasource`
- Use `anyhow::Result` (not `StellarisError`) for error handling in this crate
- Add `mongodb` crate dependency in `ton618/Cargo.toml` before implementing
- All data source methods must be `async`

### Testing Requirements
```bash
cargo test -p ton618
# MongoDB integration tests require a running instance
```

### Common Patterns
- Implement `TaskDataSource` trait: `fetch_pending`, `mark_processed`, `get_task`

## Dependencies

### Internal
- `ton618::datasource` — `TaskDataSource` trait
- `dysonsphere::message` — `TaskMessage`
- `dysonsphere::status` — `TaskStatus`

### External (planned)
- `mongodb` — MongoDB async driver

<!-- MANUAL: -->
