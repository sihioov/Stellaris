<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-25 | Updated: 2026-04-25 -->

# src (hubble)

## Purpose
Entry point for the Hubble scraper binary. Currently a placeholder with only `main.rs`. Will be expanded with data crawling logic when the scraping architecture is finalized.

## Key Files

| File | Description |
|------|-------------|
| `main.rs` | Binary entry point — placeholder only |

## For AI Agents

### Working In This Directory
- Do not implement scraping logic until the data source and storage schema are decided
- The intended output is persisted task records readable by TON618
- When implemented, use `tokio` async for all network I/O

### Testing Requirements
```bash
cargo check -p hubble
```

<!-- MANUAL: -->
