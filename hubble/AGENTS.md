<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-25 | Updated: 2026-04-25 -->

# hubble

## Purpose
The data scraper/collector binary. Named after the Hubble Space Telescope. Responsible for crawling external data sources and persisting them to the task database for TON618 to pick up. Currently a placeholder — only `main.rs` exists.

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
- This crate is a placeholder — do not implement scraping logic until the architecture is finalized
- The intended stack is Python (per `docs/stellaris_summary.md`), but a Rust binary skeleton exists
- When implemented, it should write to the same data store that TON618 reads from

### Testing Requirements
```bash
cargo check -p hubble
```

## Dependencies

### Internal
- Will depend on `dysonsphere` for TaskMessage and data store abstractions when implemented

<!-- MANUAL: -->
