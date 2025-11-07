# Repository Guidelines

## Project Structure & Module Organization
`src/main.rs` wires CLI argument parsing into `src/commands`, while `src/postgres`, `src/migration`, and `src/replication` cover connections, snapshot orchestration, and logical replication. `src/interactive.rs` owns terminal UX, `src/filters.rs` defines replication filters, integration coverage sits in `tests/integration_test.rs`, and detailed notes belong in `docs/`. Keep generated artifacts in `target/` and quarantine secrets outside the repo.

## Build, Test, and Development Commands
Stick to Rust 1.70+ and the standard Cargo toolchain:
```bash
cargo fmt
cargo clippy --all-targets --all-features
cargo test
cargo test --test integration_test -- --ignored
cargo run -- validate --source $SRC --target $TGT --yes
```
Use env vars (`TEST_SOURCE_URL`, `TEST_TARGET_URL`) when running CLI flows related to real databases; destructive commands must target disposable instances per CLAUDE.md.

## Coding Style & Naming Conventions
Follow CLAUDE.md’s “doing it right > doing it fast”: idiomatic Rust, 4-space indent, `snake_case` for functions/modules, `UpperCamelCase` types, `SCREAMING_SNAKE_CASE` constants. Lean on `cargo fmt` + `clippy` to enforce consistency. Favor descriptive identifiers over historical names, document public APIs with `///`, and split modules once they approach 400 lines to keep intent obvious.

## Testing Guidelines
TDD is mandatory: add a failing test, watch it fail, implement the fix, and rerun until green. Co-locate unit tests with their modules and reserve `tests/integration_test.rs` for full replication sweeps. Mark destructive cases with `#[ignore]`, describe any Docker setup inline, and assert on measurable outcomes (lag thresholds, checksums) instead of log strings. Never delete a failing test—surface the issue to Taariq.

## Commit & Pull Request Guidelines
Match the repo’s history of concise, imperative subjects (“Fix missing progress output”) capped at 72 chars; wrap body text near 100 columns and annotate issues via `Closes #123` when applicable. PRs must outline the scenario, enumerate tests (`cargo test`, ignored integration runs, manual CLI steps), and attach screenshots for UX changes. Request review from someone familiar with the touched subsystem before merging.

## Agent Collaboration Notes
Per CLAUDE.md: treat Taariq as an equal partner, push back on unclear asks, and never skip steps for speed. Do the smallest change that satisfies requirements, avoid mocks in end-to-end flows, and stop immediately if an instruction conflicts with Rule #1 (“doing it right is better than doing it fast”). Document any rule exceptions in the PR and wait for explicit approval before proceeding.
