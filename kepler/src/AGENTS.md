<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-09 | Updated: 2026-05-09 -->

# kepler/src

## Purpose
Core source for the Kepler codebase scanner. Implements the hourly scan loop and the codebase analysis logic that produces `Discovery` records registered as `PendingProposal` tasks.

## Key Files

| File | Description |
|------|-------------|
| `main.rs` | Entry point: loads env config, initializes `FileTaskTable`, runs the 1-hour `scan → register_discoveries` loop; contains integration test for idempotent discovery registration |
| `scanner.rs` | Implements `scan(repo_path)` — walks the repository tree, runs analysis (clippy, heuristics), emits `Discovery` structs with `DiscoveryKind` (Bug, Improvement, etc.) |

## For AI Agents

### Working In This Directory
- `scanner.rs` uses `dysonsphere::discovery::{discovery_id_fnv1a, Discovery}` — new discovery types must implement the `Discovery` trait from dysonsphere
- `discovery_id_fnv1a` produces a stable hash used as the dedup key in `seen.json`; keep discovery titles and descriptions deterministic to avoid duplicate registrations
- Scan results are idempotent: running twice on the same codebase registers each finding exactly once

### Testing Requirements
- `cargo test -p kepler` — runs the single integration test in `main.rs`
- Test pattern: create temp dir, run `register_discoveries` twice with same findings, assert second call returns `0`

### Common Patterns
- New scanner checks: add an async function in `scanner.rs`, call it from `scan()`, push results to the findings `Vec<Discovery>`
- All findings use `TaskStatus::PendingProposal` — they are proposals awaiting human review before execution

## Dependencies

### Internal
- `dysonsphere::db::FileTaskTable` — task persistence
- `dysonsphere::discovery::{register_discoveries, Discovery, discovery_id_fnv1a}` — dedup and registration logic
- `dysonsphere::message::{TaskMessage, TaskMeta, TaskType}` — task record structure
- `dysonsphere::status::TaskStatus` — `PendingProposal` status

### External
- `tokio::process::Command` — for running `cargo clippy` subprocess in scanner

<!-- MANUAL: -->
