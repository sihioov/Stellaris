<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-25 | Updated: 2026-04-25 -->

# task

## Purpose
Task dispatch layer for TON618. Routes `TaskMessage` instances to appropriate Laniakea worker queues using round-robin selection with task-type affinity. Bridges the scheduler output to the message queue.

## Key Files

| File | Description |
|------|-------------|
| `mod.rs` | Module declaration |
| `dispatcher.rs` | `TaskDispatcher` trait + `RabbitMQTaskDispatcher` implementation |

## For AI Agents

### Working In This Directory
- `dispatcher.rs` contains both the `TaskDispatcher` trait and the `RabbitMQTaskDispatcher` impl
- Worker selection: task-type affinity first, then round-robin across all active workers
- Uses `dysonsphere::StellarisError` (not `anyhow`) because it calls through to dysonsphere MQ layer
- `WorkerInfo` and `WorkerStatus` track worker state (Active, Busy, Inactive)
- `RabbitMQTaskDispatcher` uses `Arc<RabbitMQClient>` — safe to clone across threads

### Testing Requirements
```bash
cargo test -p ton618
# task_distribution test in dispatcher.rs is a TODO — implement before shipping
```

### Common Patterns
```rust
// Register a worker for specific task types
dispatcher.register_worker("andromeda".to_string(), vec![TaskType::Processing]);

// Dispatch a task (selects worker automatically)
dispatcher.dispatch(task_message).await?;
```

## Dependencies

### Internal
- `ton618::datasource` — `TaskDataSource` (for status lookups)
- `ton618::file` — `FileDataSource` (current data source impl)
- `dysonsphere::message` — `TaskMessage`, `TaskType`
- `dysonsphere::status` — `TaskStatus`
- `dysonsphere::error` — `StellarisError`, `Result`
- `dysonsphere::mq::rabbit_mq` — `RabbitMQClient`

### External
- `async-trait` — async trait impl
- `chrono` — `last_seen` timestamp on `WorkerInfo`

<!-- MANUAL: -->
