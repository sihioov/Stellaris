<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-09 | Updated: 2026-05-09 -->

# docs/archive

## Purpose
Historical design documents, planning specs, and architecture snapshots preserved for reference. These documents reflect decisions made at a point in time and may be partially superseded by current implementation. Do not update archive docs — create new docs in `docs/` or `docs/solutions/` instead.

## Key Files

| File | Description |
|------|-------------|
| `stellaris-canopus-architecture-v1.md` | V1 architecture decision record for the Stellaris/Canopus integration |
| `stellaris-canopus-architecture.md` | Original architecture design (pre-V1) |
| `stellaris-canopus-v1-integration-plan.md` | V1 integration plan with phased milestones |
| `stellaris-system-direction.md` | High-level system direction and long-term goals |
| `stellaris_summary.md` | Executive summary of the Stellaris system |
| `canopus-ralph-comparison-design-note.md` | Design note comparing Canopus execution model to Ralph (OMC skill) |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `superpowers/` | Archived plans and specs from the superpowers/OMC planning phase (see `superpowers/plans/` and `superpowers/specs/`) |

## For AI Agents

### Working In This Directory
- Treat all content here as **historical context**, not active guidance
- If an archive doc contradicts current code or `docs/solutions/`, trust the code and current docs
- `superpowers/plans/2026-04-25-canopus-local-patch-mvp.md` is a high-value reference for understanding the original Canopus submit/clean-check design, but it predates the multi-project state routing fix (PR-A/B) and the `artifact_content_for_role` bug fix

<!-- MANUAL: -->
