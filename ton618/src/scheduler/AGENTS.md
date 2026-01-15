# SCHEDULER SUBSYSTEM

**Generated:** 2026-01-15 18:26:09 UTC

## OVERVIEW
Core scheduling engine with Job trait, priority queue, runner, and Schedule types.

## STRUCTURE
```
ton618/src/scheduler/
├── mod.rs           # Module exports, Schedule re-export
├── job.rs           # Job trait definition
├── queue.rs         # Priority queue implementation
├── runner.rs        # Job execution with retry logic
└── schedule.rs      # Schedule enum (Fixed/Duration/Cron)
```

## WHERE TO LOOK
| Task | Location | Notes |
|------|----------|-------|
| Job contract | job.rs | Job trait: name(), execute(), max_retries() |
| Queue management | queue.rs | Priority queue with scheduled jobs |
| Job execution | runner.rs | Runs jobs, handles retries |
| Scheduling | schedule.rs | Schedule::Fixed(Duration), Schedule::Cron(expr) |
| Module exports | mod.rs | Re-exports Schedule type |

## CONVENTIONS
- **Job execution**: Async trait, returns anyhow::Result
- **Default retries**: max_retries() defaults to 0
- **Schedule types**: Fixed interval or Cron expression
- **Priority queue**: Custom implementation (not BinaryHeap)

## ANTI-PATTERNS (THIS PROJECT)
- Never implement Job without Send + Sync bounds
- Never modify job state during execute() (use mutex if needed)
- Never skip retry logic in runner for failed jobs
