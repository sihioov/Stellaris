# DYNSPHERE CORE LIBRARY

**Generated:** 2026-01-15 18:26:09 UTC

## OVERVIEW
Shared core library providing TaskMessage, TaskStatus, and abstractions for DB and MQ.

## STRUCTURE
```
dysonsphere/src/
├── message.rs       # TaskMessage, TaskType, TaskMeta
├── status.rs        # TaskStatus enum (Pending, Processed, Failed)
├── error.rs         # StellarisError, Result alias
├── db/              # TaskTable trait + file-based impl
│   ├── task_table.rs        # TaskTable trait
│   └── task_table_file.rs  # FileTaskTable implementation
└── mq/              # MessageQueue trait + RabbitMQ impl
    ├── message_queue.rs     # MessageQueue trait
    └── rabbit_mq.rs         # RabbitMQClient implementation
```

## WHERE TO LOOK
| Task | Location | Notes |
|------|----------|-------|
| Task message format | message.rs | TaskMessage struct with payload, meta |
| Task lifecycle | status.rs | Pending → Processed/Failed |
| DB abstraction | db/task_table.rs | TaskTable trait (CRUD operations) |
| File storage | db/task_table_file.rs | JSON file-based TaskTable impl |
| MQ abstraction | mq/message_queue.rs | MessageQueue trait |
| RabbitMQ client | mq/rabbit_mq.rs | RabbitMQ implementation |

## CONVENTIONS
- **Error handling**: Custom StellarisError enum with Display trait
- **Result type**: `type Result<T> = std::result::Result<T, StellarisError>`
- **Async traits**: Uses `async-trait` crate for TaskTable
- **Serialization**: Serde derive for all message/status types

## ANTI-PATTERNS (THIS PROJECT)
- Never use anyhow in dysonsphere (use StellarisError instead)
- Never implement TaskTable without async methods
- Never use JSON files for production (use DB implementations)
