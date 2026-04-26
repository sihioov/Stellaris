<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-25 | Updated: 2026-04-25 -->

# rdb

## Purpose
Relational database data source implementations for TON618. Contains PostgreSQL, SQLite, and a shared RDB datasource abstraction. All are currently placeholder stubs implementing `TaskDataSource`.

## Key Files

| File | Description |
|------|-------------|
| `mod.rs` | Module declaration |
| `rdb_datasource.rs` | Shared RDB abstraction or base type for relational backends |
| `postgres.rs` | PostgreSQL `TaskDataSource` implementation — placeholder |
| `sqlite.rs` | SQLite `TaskDataSource` implementation — placeholder |

## For AI Agents

### Working In This Directory
- All files are placeholders — check each file for TODO/stub markers before implementing
- Use `anyhow::Result` for error handling (ton618 crate convention)
- Add `sqlx` or equivalent dependency in `ton618/Cargo.toml` before implementing
- SQLite is the preferred first implementation (no server required, good for local dev)
- All data source methods must be `async`

### Testing Requirements
```bash
cargo test -p ton618
# DB integration tests require appropriate server (Postgres) or file (SQLite)
```

### Common Patterns
- Implement `TaskDataSource` trait: `fetch_pending`, `mark_processed`, `get_task`
- Use connection pooling (e.g., `sqlx::Pool`) for production implementations

## Dependencies

### Internal
- `ton618::datasource` — `TaskDataSource` trait
- `dysonsphere::message` — `TaskMessage`
- `dysonsphere::status` — `TaskStatus`

### External (planned)
- `sqlx` — async SQL toolkit for Postgres/SQLite

<!-- MANUAL: -->
