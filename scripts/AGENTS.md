<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-05-09 | Updated: 2026-05-09 -->

# scripts

## Purpose
Operational and migration scripts for maintaining the Stellaris deployment. Currently contains state migration helpers (for moving `.canopus/` state between project roots) and a read-only policy validator used in CI.

## Key Files

| File | Description |
|------|-------------|
| `migrate-canopus-state.sh` | Bash script to migrate per-project `.canopus/` state directories when a project is relocated or restructured |
| `migrate-canopus-state.ps1` | PowerShell equivalent of the migration script for Windows environments |
| `validate-read-only.ps1` | PowerShell script that validates Canopus read-only tool policy — used in CI to assert no write-gateway calls occur in dry-run mode |

## For AI Agents

### Working In This Directory
- Scripts here are run directly by operators; they are not imported or called by Rust/Python source
- `validate-read-only` is exercised by the `validate_read_only_script` integration test in Canopus (`apps/canopus/tests/validate_read_only_script.rs`)
- When adding new scripts, prefer `.sh` for Linux/WSL targets and add a `.ps1` equivalent for Windows parity

### Testing Requirements
- `validate-read-only.ps1` is tested via `cargo test -p canopus --test validate_read_only_script`
- Migration scripts should be tested manually against a disposable copy of the `.canopus/` state directory before running in production

<!-- MANUAL: -->
