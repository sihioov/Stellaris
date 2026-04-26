<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-25 | Updated: 2026-04-25 -->

# mq

## Purpose
Message queue abstraction layer. Defines the `MessageQueue` async trait and provides a RabbitMQ implementation via the `lapin` AMQP client. Enables TON618 to publish tasks to Laniakea workers.

## Key Files

| File | Description |
|------|-------------|
| `mod.rs` | Module declaration — re-exports `MessageQueue` trait |
| `message_queue.rs` | `MessageQueue` async trait: `publish` and `subscribe` operations |
| `rabbit_mq.rs` | `RabbitMQClient` — `lapin`-based RabbitMQ implementation |

## For AI Agents

### Working In This Directory
- Error type is `StellarisError` — never use `anyhow` here
- All trait methods must be `async` (enforced via `async-trait`)
- RabbitMQ connection configuration comes from environment variables — do not hardcode connection strings
- `lapin` is configured with `default-features = false` — only enable features that are needed

### Testing Requirements
```bash
cargo test -p dysonsphere
# RabbitMQ integration tests require a running broker
```

### Common Patterns
- `MessageQueue::publish(queue: &str, message: TaskMessage)` — publish to a named queue
- `RabbitMQClient::new(uri: &str)` — create client with AMQP URI

## Dependencies

### Internal
- `dysonsphere::message` — `TaskMessage` (the payload type)
- `dysonsphere::error` — `StellarisError`, `Result`

### External
- `lapin` — AMQP 0-9-1 client for RabbitMQ
- `futures-util` — async stream utilities for consumers
- `async-trait` — async trait support

<!-- MANUAL: -->
