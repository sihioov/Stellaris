<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-25 | Updated: 2026-04-25 -->

# docs

## Purpose
Project documentation covering architecture, commit conventions, code review guidelines, and AI orchestration pipeline notes for the Stellaris distributed data processing system.

## Key Files

| File | Description |
|------|-------------|
| `stellaris_summary.md` | Full project architecture overview: components, message flow, tech stack, git strategy |
| `stellaris-deck.md` | Project deck / presentation slides content |
| `stellaris.drawio` | Architecture diagram (draw.io format) |
| `commit.md` | Commit message conventions — module prefix format, types, examples |
| `review.md` | Code review guidelines and checklist |
| `snippet.md` | Useful code snippets and patterns for the project |
| `ai_rule.md` | Rules for AI agents working in this codebase |
| `ai_orchestration_pipeline_summary.md` | AI orchestration pipeline design summary |

## For AI Agents

### Working In This Directory
- `commit.md` is authoritative for commit format — always reference it before committing
- `ai_rule.md` contains project-specific AI agent rules — read it when starting work
- Do not regenerate documentation from this directory; these are manually maintained references

### Common Patterns
- Architecture documents describe the intended pipeline: Hubble → TON618 → Laniakea
- Korean-language content appears in some docs (the project supports bilingual documentation)

## Dependencies

### Internal
- References all four crates: dysonsphere, ton618, laniakea, hubble

<!-- MANUAL: -->
