# Repository Guidelines

## Project Structure & Module Organization
Stellaris is a Rust workspace for distributed task processing. Core crates live at the repository root: `dysonsphere/` contains shared task contracts, status types, storage, discovery, and queue abstractions; `ton618/` schedules and dispatches pending work; `laniakea/` executes tasks; `hubble/` collects external signals; `kepler/` scans codebase findings. AI development automation lives in `apps/canopus/`, and the Discord control surface lives in `apps/europa/`. Project documentation and architecture notes are under `docs/`; use `docs/architecture.md` for the current core/app responsibility boundary.

## Build, Test, and Development Commands
- `cargo build --workspace` — build all Rust crates.
- `cargo check --workspace` — fast compile/type check without full build artifacts.
- `cargo test --workspace` — run all Rust unit and integration tests.
- `cargo fmt --all -- --check` — verify Rust formatting before review.
- `cargo clippy --workspace --all-targets -- -D warnings` — enforce lint cleanliness.
- `cargo run -p ton618`, `cargo run -p laniakea`, `cargo run -p hubble`, `cargo run -p kepler` — run individual services.
- `python3 -m py_compile apps/europa/europa.py` — syntax-check the Discord bot.

## Coding Style & Naming Conventions
Use Rust 2021 conventions, `rustfmt`, and clear module boundaries. Prefer trait-based abstractions in shared crates and keep Canopus-specific workflow logic inside `apps/canopus`. Use cosmic component names consistently: Dysonsphere for shared contracts, TON618 for scheduling, Laniakea for workers, Hubble/Kepler for discovery. Python code in `apps/europa` should stay small, explicit, and configuration-driven.

## Boundaries
Stellaris is layered Discord(UI) / Canopus(engine) / Dysonsphere(contract).
When adding code, route policy/state mutation to canopus and keep
surface-specific formatting out of canopus. See
docs/architecture/boundaries.md for the full rules and migration backlog.

## Testing Guidelines
Place Rust unit tests near the code they cover and integration tests under each crate’s `tests/` directory, e.g. `apps/canopus/tests/*.rs`. Add regression tests for status transitions, scheduler filtering, tool policy, and discovery deduplication. Run `cargo test --workspace` before committing any Rust behavior change.

## Commit & Pull Request Guidelines
Follow `docs/commit.md`: use `[module] type: summary`, for example `[ton618] fix: filter pending proposal tasks`. Valid types include `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, and `style`. Include `(Closes #N)` for issue-backed work. PRs should describe intent, affected modules, validation commands, linked issues, and screenshots/logs for Discord or CLI UX changes.

## Security & Configuration Tips
Do not commit `.env`, local Codex state, Python bytecode, `target/`, or OS metadata. Keep risky git/shell operations behind Canopus `ToolGateway` policy and require human approval before PR, merge, deploy, or protected-branch actions.
