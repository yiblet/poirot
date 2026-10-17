# Contributing

Thanks for helping improve Poirot! Bug reports, fixes, tests, and documentation
improvements are all welcome.

## Getting started

Open an issue to discuss larger changes before starting. For bugs, include a small
reproduction, the expected result, and what happened instead. A sample workbook
or formula is especially helpful.

## Pull requests

Keep changes focused and follow the surrounding code. Add regression tests for
bug fixes, and base expected spreadsheet results on Excel, documented behavior,
or another reliable reference—not Poirot's output alone.

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
