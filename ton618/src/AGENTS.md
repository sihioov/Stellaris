<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-01-15 | Updated: 2026-04-25 -->

# TON618 SCHEDULER BINARY

**Generated:** 2026-01-15 18:26:09 UTC

## OVERVIEW
Task queue and scheduler with priority-based job scheduling, data source abstraction, and task dispatch.

## STRUCTURE
```
ton618/src/
├── main.rs          # Entry point: main loop with fixed interval scheduling
├── datasource.rs    # TaskDataSource trait (fetch_pending, mark_processed, get_task)
├── file.rs          # FileDataSource implementation
├── scheduler/       # Core scheduling engine
│   ├── job.rs       # Job trait (name, execute, max_retries)
│   ├── queue.rs     # Priority queue implementation
│   ├── runner.rs    # Job execution logic
│   └── schedule.rs  # Schedule type (fixed interval + cron)
├── task/            # Task dispatch
│   └── dispatcher.rs # Routes tasks to handlers
├── nosql/           # NoSQL data sources (MongoDB - placeholder)
│   └── mongo.rs
└── rdb/             # Relational DB data sources (placeholder)
    ├── postgres.rs
    ├── sqlite.rs
    └── rdb_datasource.rs
```

## WHERE TO LOOK
| Task | Location | Notes |
|------|----------|-------|
| Main loop | main.rs | Polls datasource with fixed 10s interval |
| Data source abstraction | datasource.rs | TaskDataSource trait |
| File-based source | file.rs | FileDataSource (JSON file with mutex lock) |
| Job definition | scheduler/job.rs | Job trait with name(), execute(), max_retries() |
| Priority queue | scheduler/queue.rs | Custom priority queue implementation |
| Job execution | scheduler/runner.rs | Runs jobs with retry logic |
| Scheduling | scheduler/schedule.rs | Schedule enum: Fixed/Duration/Cron |
| Task routing | task/dispatcher.rs | Routes tasks by task_type |

## CONVENTIONS
- **Error handling**: Uses `anyhow` for Result (unlike dysonsphere)
- **Async runtime**: Tokio multi-threaded
- **Logging**: env_logger + log crate
- **Data sources**: FileDataSource implemented, DBs planned

## ANTI-PATTERNS (THIS PROJECT)
- Never use StellarisError in ton618 (use anyhow instead)
- Never implement TaskDataSource without async methods
- Never skip mark_processed after task completion
- Never use blocking file I/O in async context (use tokio::fs or mutex)
