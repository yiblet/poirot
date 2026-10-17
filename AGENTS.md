# Repository guidelines

Poirot is an MIT-licensed Rust 2024 library for reading, editing, evaluating, and
writing Excel workbooks.

## Working in the codebase

- `src/lib.rs` exposes the public API; component crates live in `crates/`.
- Keep existing crate boundaries. Spreadsheet-function algorithms must stay
  independent of workbook and evaluator implementations.
- Follow surrounding code and rustfmt conventions. Keep changes focused, and
  update examples and API docs when public behavior changes.
- Add focused regression tests for fixes. Derive expected spreadsheet results
  from documented evidence, never Poirot's output alone. Distinguish known
  compatibility gaps from regressions.

## Safety

- Forbid unsafe code in every target, including tests and examples. Each package
  must inherit workspace lints with `[lints] workspace = true`.
- Outside tests, every panic-capable operation needs an adjacent `// SAFETY:`
  comment proving it cannot panic using only checks, values, and control flow in
  the same function. Caller contracts and invariants established elsewhere are
  insufficient. Otherwise, use a fallible operation or return an error.
- Do not use `assert!`, `panic!`, `unreachable!`, or `todo!` to handle invalid input
  or operational failures. Tests may use assertions and panic-capable helpers
  without safety comments.

## Upstream code and attribution

- AI agents must explicitly acknowledge any code, tests, data, or algorithms
  copied, translated, or adapted from another library. Record the upstream
  project, source URL, revision, and what was reused in a nearby comment or
  attribution file. Credit substantial influences in the README acknowledgements.
- Only introduce third-party material or dependencies under permissive
  open-source terms compatible with Poirot's MIT license. For dual-licensed
  material, verify and record the permissive option used.
- Preserve required copyright notices, license texts, and attribution. An
  acknowledgement does not replace these obligations or relicense upstream work.
- Do not introduce copyleft, proprietary, custom-restricted, or unverified
  material. If licensing or provenance is unclear, flag it and leave the material
  out until resolved. This applies to tests and fixtures as well as library code.

## Checks

Run relevant tests while working and these checks before submitting:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Run `python3 tools/test_function_architecture.py` when changing function-crate
boundaries.

## Pull requests

Use Conventional Commit titles, such as `fix(eval): handle empty ranges`.
Explain the behavior changed, why, how it was tested, and any compatibility limits.
