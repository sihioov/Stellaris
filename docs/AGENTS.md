<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-25 | Updated: 2026-04-25 -->

# docs

## Purpose
Project documentation covering architecture, commit conventions, code review guidelines, and AI orchestration pipeline notes for the Stellaris distributed data processing system.

## Key Files

| File | Description |
|------|-------------|
| `architecture.md` | Current source of truth for Stellaris core, Canopus app, and discovery-source boundaries |
| `canopus-v1.md` | Current Canopus v1 app/workload scope, modules, safety model, and validation |
| `commit.md` | Commit message conventions — module prefix format, types, examples |
| `review.md` | Code review guidelines and checklist |
| `snippet.md` | Useful code snippets and patterns for the project |
| `ai_rule.md` | Rules for AI agents working in this codebase |
| `ai_orchestration_pipeline_summary.md` | AI orchestration pipeline design summary |
| `archive/` | Historical architecture drafts, comparison notes, and implementation plans |

## For AI Agents

### Working In This Directory
- `commit.md` is authoritative for commit format — always reference it before committing
- `ai_rule.md` contains project-specific AI agent rules — read it when starting work
- Do not regenerate documentation from this directory; these are manually maintained references

### Common Patterns
- Architecture documents define the intended core/app split: Hubble/Dysonsphere/TON618/Laniakea as core, Canopus as app workload, Kepler/Hubble as discovery sources
- Korean-language content appears in some docs (the project supports bilingual documentation)

## Dependencies

### Internal
- References workspace crates and apps: dysonsphere, ton618, laniakea, hubble, kepler, and apps/canopus

<!-- MANUAL: -->
