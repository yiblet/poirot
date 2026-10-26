# Repository guidelines

Poirot is an MIT-licensed Rust 2024 library for reading, editing, evaluating, and
writing Excel workbooks.

## Working in the codebase

- `src/lib.rs` exposes the public API; component crates live in `crates/`.
- Keep existing crate boundaries. Spreadsheet-function algorithms must stay
  independent of workbook and evaluator implementations.
- Follow surrounding code and rustfmt conventions. Keep changes focused, and
  update examples and API docs when public behavior changes.
- Model meaning with types: use enums for named choices instead of strings,
  booleans for genuinely binary properties, and newtypes where distinct values
  should not be interchangeable.

## API design

Keep APIs small and purposeful: every public or internal entry point adds ongoing
maintenance cost. When replacing or refactoring an API, consolidate equivalent
entry points and remove obsolete wrappers, helpers, aliases, and dead APIs,
updating callers, documentation, and tests together. Poirot is pre-1.0: breaking
API changes are acceptable when they simplify or improve the library. Do not
retain redundant compatibility layers solely for historical use. Preserve distinct
behaviors and use cases when they have a deliberate justification, rather than
reducing the surface mechanically.

## Documentation and tests

- Every public function and method, and every type declaration (including private
  types, traits, and type aliases), requires a short `///` doc comment explaining
  what it is for. Focus on purpose; include constraints or examples only when
  needed to use it correctly.
- Cover new and changed behavior with focused tests, including meaningful edge
  cases and failure paths. Test public contracts through the public API and add
  regression tests for fixes.
- Derive expected spreadsheet results from documented evidence, never Poirot's
  output alone. Distinguish known compatibility gaps from regressions.

## Documentation style

- Write for a developer learning or using the current codebase.
- Keep documentation short. Explain purpose, main concepts, entry points,
  and important constraints.
- Update an existing guide before creating another document.
- Put local implementation details and invariants beside the relevant code.
- Do not add implementation journals, exhaustive design narratives, historical
  investigations, migration histories, or speculative plans to repository docs
  unless explicitly requested.
- Keep experiment logs, benchmark findings, and working notes outside the worktree.
- Preserve required license, provenance, and attribution records.
- Before adding prose, ask: does this help someone use the library or find
  and understand the code? If not, leave it out.

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
