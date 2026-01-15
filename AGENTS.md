# STELLARIS PROJECT KNOWLEDGE BASE

**Generated:** 2026-01-15 18:26:09 UTC
**Commit:** bfe6588
**Branch:** main

## OVERVIEW
High-performance distributed data processing system in Rust. Workspace with 4 crates: dysonsphere (core lib), ton618 (scheduler), laniakea (worker), hubble (collector).

## STRUCTURE
```
./
├── dysonsphere/    # Shared core library: TaskMessage, TaskStatus, db/mq abstractions
├── ton618/         # Task queue & scheduler with priority-based job scheduling
├── laniakea/       # Task processor (placeholder)
├── hubble/         # Data collector (placeholder)
├── docs/           # Project documentation (commit rules, architecture, snippets)
└── target/         # Rust build artifacts (ignore)
```

## WHERE TO LOOK
| Task | Location | Notes |
|------|----------|-------|
| Core data structures | dysonsphere/src/message.rs, status.rs | TaskMessage, TaskStatus, TaskMeta |
| DB abstraction | dysonsphere/src/db/ | TaskTable trait + file-based impl |
| Message queue | dysonsphere/src/mq/ | RabbitMQ abstraction |
| Scheduler engine | ton618/src/scheduler/ | Job, Queue, Runner, Schedule |
| Data sources | ton618/src/file.rs, nosql/, rdb/ | File-based implemented, DBs planned |
| Task dispatch | ton618/src/task/dispatcher.rs | Routes tasks to handlers |
| Commit format | docs/commit.md | Module prefix required: [ton618], [dysonsphere], etc. |

## CONVENTIONS
- **Module prefix in commits**: `[module] type: summary (Closes #N)` format
- **Module prefixes**: ton618, dysonsphere, hubble, laniakea, infra, docs
- **Commit types**: feat, fix, refactor, docs, test, chore, style
- **Async traits**: Uses `async-trait` crate
- **Error handling**: Uses `anyhow` in ton618, custom Result in dysonsphere

## ANTI-PATTERNS (THIS PROJECT)
- Never use `[ton618/#12]` in commits - GitHub won't parse
- Never use lowercase `closes` - use `Closes #N` for auto-close
- Never mix module prefixes - one module per commit
- Never skip issue numbers in commits

## UNIQUE STYLES
- **Cosmic naming**: Modules named after cosmic structures (Dysonsphere, TON618 black hole, Laniakea supercluster, Hubble telescope)
- **Trait-based abstraction**: TaskDataSource, Job traits for extensibility
- **Scheduler priority queue**: Custom implementation with Schedule type (fixed interval + cron)
- **Tokio runtime**: Multi-threaded async across all crates

## COMMANDS
```bash
# Build entire workspace
cargo build

# Run specific module
cargo run -p ton618
cargo run -p laniakea
cargo run -p hubble

# Check all crates
cargo check --workspace

# Run tests
cargo test --workspace
```

## NOTES
- dysonsphere is the only library crate; others are binaries
- ton618 is most complex with scheduler subsystem
- hubble and laniakea are currently placeholders (only main.rs)
- File-based storage implemented (task_table_file.rs); DB implementations planned
- RabbitMQ dependency in dysonsphere for message queue (currently minimal)
