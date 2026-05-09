<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-09 | Updated: 2026-05-09 -->

# docs/architecture

## Purpose
Canonical architecture boundary rules for the Stellaris system. Defines which crate/app owns which concern and tracks the migration backlog for moving misplaced code to the correct layer.

## Key Files

| File | Description |
|------|-------------|
| `boundaries.md` | The authoritative boundary spec: Discord (UI surface) / Canopus (engine) / Dysonsphere (shared contracts). Includes rules for where policy, state mutation, and surface formatting belong, plus migration backlog items |

## For AI Agents

### Working In This Directory
- Read `boundaries.md` before adding any new module or cross-crate dependency — it defines where code must live
- The core rule: route policy/state mutation to Canopus, keep surface-specific formatting out of Canopus, keep shared contracts in Dysonsphere
- When boundaries.md and a PR conflict, boundaries.md wins — update the PR, not the spec (unless explicitly changing the architecture)

<!-- MANUAL: -->
