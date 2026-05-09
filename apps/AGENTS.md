<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-09 | Updated: 2026-05-09 -->

# apps

## Purpose
Application layer containing user-facing services and automation engines. Unlike the core Stellaris crates (dysonsphere, ton618, laniakea), apps are opinionated and surface-specific: Canopus drives AI-powered task execution, Europa provides the Discord control surface.

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `canopus/` | AI task execution engine — orchestrates agent runtimes (Claude, Codex), manages task lifecycle, artifacts, and per-project state (see `canopus/AGENTS.md`) |
| `europa/` | Discord bot — translates Discord commands into Canopus tasks, manages per-guild project registrations (see `europa/AGENTS.md`) |
| `discord-bot/` | Legacy/alternative Discord integration (currently empty) |

## For AI Agents

### Working In This Directory
- This directory contains no source files itself — all logic lives in subdirectories
- Changes to `canopus/` and `europa/` are tightly coupled: Europa triggers Canopus via HTTP; verify both sides when changing the intake protocol
- See `docs/architecture/boundaries.md` for the canonical surface ↔ engine boundary rules

### Dependencies
- `canopus/` depends on `dysonsphere` (shared contracts) and `laniakea` (dev-dependency for integration tests)
- `europa/` is a standalone Python service — it calls Canopus over HTTP and has no Rust dependency

<!-- MANUAL: -->
