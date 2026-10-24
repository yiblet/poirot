# Contributing

Help make Poirot a fast, lightweight, Excel-compatible calculation engine.
Bug reports, fixes, tests, and documentation improvements are welcome, including
AI-written PRs. Review what you submit and make the behavior easy to verify.

## Where to contribute

Our roadmap is more modern Excel compatibility, new evaluation modes, faster
existing modes, and more extensibility. Useful contributions include a small
reproduction of an Excel edge case, a missing function, a measured speedup, or an
extension point that lets applications adapt calculation to their needs.

Read the [compatibility guide](docs/compatibility.md) for current scope and known
gaps. Keep intentional deviations explicit and preserve default compatibility
behavior.

Read the [architecture guide](docs/architecture.md) for the component boundaries
and a staged walkthrough of an ordinary cell calculation.

## Getting started

Open an issue to discuss larger changes before starting. For bugs, include a small
reproduction, input values, the Excel version used, the expected result, and what
happened instead. A small workbook or formula is especially helpful. Edge cases
involving blanks, text, errors, or references are worth reporting even when the
formula itself looks simple.

## Pull requests

Keep changes focused and follow the surrounding code. Add regression tests for
bug fixes, and base expected spreadsheet results on Excel, documented behavior,
or another reliable reference. Record that evidence rather than deriving expected
results from Poirot's output alone. For performance changes, include a repeatable
workload and before/after measurements. Keep component boundaries intact so
applications can use the parts they need.

From the repository root, run:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Use a Conventional Commit title, such as `fix(eval): handle empty ranges`. Explain
what changed, why, and how you tested it. Mention any known compatibility gaps.

## Crediting others

Use only permissively licensed material compatible with Poirot’s MIT license.
If you adapt code, tests, data, or algorithms, credit the project and record its
source URL, revision, and license. Preserve required notices. If the terms or
provenance are unclear, leave the material out and raise the question in your PR.
