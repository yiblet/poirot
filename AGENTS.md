# Repository Guidelines

## Project Structure & Module Organization

Poirot is a Rust 2024 workspace for reading, editing, evaluating, and writing Excel workbooks. `src/lib.rs` exposes the public facade; `src/artifact/` handles snapshot files. Component crates in `crates/` separate workbook models, formula parsing, evaluation, formatting, and XLS/XLSX I/O. Spreadsheet functions live in `poirot-functions-*` crates. Keep their algorithms independent of workbook and evaluator implementations; internal dependencies are limited to function-family crates and `poirot-numeric`.

Root `tests/` exercises public workflows; individual crates contain unit and integration tests. Compatibility fixtures live in `crates/poirot-test-fods/corpus/` and Excel specifications in `crates/poirot-test-excel-specs/test-specs/`. See `examples/` for runnable workflows and `docs/` for API and persistence contracts.

## Build, Test, and Development Commands

Run commands from the repository root:

- `cargo build --workspace` builds every workspace package.
- `cargo run --example direct_evaluation` demonstrates direct formula evaluation.
- `cargo test --workspace` runs workspace unit, integration, and documentation tests. Plain `cargo test` selects only the root package and FODS runner by default.
- `cargo test -p poirot-eval` targets evaluator tests.
- `cargo run --bin poirot-test-fods -- run --filter text` runs text compatibility fixtures; add `--strict` to fail on known gaps too.
- `python3 tools/test_function_architecture.py` checks function-crate dependency boundaries; requires Python 3.11+.
- `cargo fmt --all -- --check` checks formatting.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` checks lints.

## Coding Style & Naming Conventions

Use rustfmt's standard formatting and four-space indentation. Use `snake_case` for modules, functions, and test names; `UpperCamelCase` for types; and `SCREAMING_SNAKE_CASE` for constants. Follow existing crate boundaries and keep public API documentation aligned with examples and behavioral contracts.

## Safety and Panic Policy

- Forbid unsafe code in every workspace package and target, including tests, examples, and binaries. Inherit the workspace's `unsafe_code = "forbid"` lint with `[lints] workspace = true` in each package manifest, including new packages. Do not weaken or bypass the lint.
- Outside tests, every use of a panic-capable operation must have an adjacent `// SAFETY:` comment explaining why it cannot panic. This includes `unwrap()`, `expect()`, indexing or slicing that can panic, and calls with documented panic conditions. The comment must state a concrete proof, not merely claim that the operation is safe.
- That proof must follow entirely from the immediate function's implementation: locally constructed values, control flow, or explicit checks. Do not rely on caller behavior, unchecked properties of parameters, invariants established by another function, or external state remaining valid. A parameter contract or a comment documenting that contract is not sufficient.
- If the proof requires such outside context, use a fallible operation, validate the condition within the function and return an error on failure, or restructure the code to eliminate the panic. Do not use `assert!`, `panic!`, `unreachable!`, or `todo!` as substitutes for handling an invalid input or operational failure.
- Tests may use assertions and panic-capable helpers without `// SAFETY:` comments. The unsafe-code ban still applies to tests.

## Testing Guidelines

Use Rust's built-in `#[test]` framework with descriptive behavior-based names. Add focused regression tests alongside the affected module or in its crate's `tests/`; cover public behavior in root integration tests. No numeric coverage threshold is configured. For Excel specifications, derive expected results from documented evidence, never Poirot's output. Distinguish compatibility gaps from regressions.

## Commit & Pull Request Guidelines

Use Conventional Commits: `type(scope): description`, with an optional scope and a concise, imperative description. Examples: `feat(eval): support a new function`, `fix(io): preserve cached values`, and `docs: update contributor guide`. Pull requests should explain the behavior changed, link relevant issues, list validation commands and outcomes, and identify compatibility limitations. Update documentation and examples when public contracts change.
