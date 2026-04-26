<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-25 | Updated: 2026-04-25 -->

# db

## Purpose
Database abstraction layer for task persistence. Defines the `TaskTable` async trait and provides a file-based JSON implementation. Designed for easy swap to real DB backends (SQLite, PostgreSQL) when needed.

## Key Files

| File | Description |
|------|-------------|
| `mod.rs` | Module declaration — re-exports `TaskTable` and implementations |
| `task_table.rs` | `TaskTable` async trait: CRUD operations for `TaskMessage` |
| `task_table_file.rs` | `FileTaskTable` — JSON file-based `TaskTable` implementation |

## For AI Agents

### Working In This Directory
- All `TaskTable` methods must be `async` (enforced via `async-trait`)
- Error type is `StellarisError` — never use `anyhow` here
- `FileTaskTable` uses a `tokio::sync::Mutex` for concurrent file access — do not use blocking file I/O
- New DB implementations (SQLite, Postgres) should go here as separate files

### Testing Requirements
```bash
cargo test -p dysonsphere
```

### Common Patterns
- `TaskTable` trait methods: create, get, update_status, list_pending
- `FileTaskTable` serializes/deserializes tasks as JSON arrays

## Dependencies

### Internal
- `dysonsphere::message` — `TaskMessage`, `TaskType`
- `dysonsphere::status` — `TaskStatus`
- `dysonsphere::error` — `StellarisError`, `Result`

### External
- `serde_json` — file serialization
- `tokio` — async file I/O and mutex
- `async-trait` — async trait support

<!-- MANUAL: -->
