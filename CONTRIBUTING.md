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

For compatibility changes, update the relevant feature or file-loss row in the
[compatibility guide](docs/compatibility.md). Describe supported inputs, remaining
restrictions and observable errors or warnings. Identify whether expectations come
from Excel observations, Microsoft documentation or another implementation; record
versions and settings where relevant. A known-gap baseline is historical evidence,
not a compatibility exemption, and changing it does not fix the underlying gap.

From the repository root, run:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
python3 tools/test_error_patterns.py --self-test
python3 tools/test_error_patterns.py
```

Operation errors use module-owned `<Purpose>Error` types with `thiserror`, typed
categories and contextual payloads. Preserve typed dependency causes. Locally
authored diagnostic strings, generic message variants and dynamic error erasure
are prohibited. Spreadsheet error values remain ordinary values.

The error checker screens production Rust sources; its fixtures also run in CI.
It recognizes operation errors through derives, error trait implementations and
explicit `Result` error arguments in the same file. It follows simple non-generic
type alias chains in their declaring module and checks grouped or renamed
`anyhow`/`eyre` imports. It does not resolve cross-module types, generic or
associated aliases, aliases inside function bodies, or macro expansions. Review
conversions, payload meaning and failure handling as well. Dependency APIs that supply only diagnostic text must have a
specifically named variant with `message: String` and an immediately preceding
`// ERROR-DEPENDENCY: dependency::api` comment recording the actual API. The
comment records provenance for review; it does not exempt other error rules.

Use a Conventional Commit title, such as `fix(eval): handle empty ranges`. Explain
what changed, why, and how you tested it. Mention any known compatibility gaps.

## Crediting others

Use only permissively licensed material compatible with Poirot’s MIT license.
If you adapt code, tests, data, or algorithms, credit the project and record its
source URL, revision, and license. Preserve required notices. If the terms or
provenance are unclear, leave the material out and raise the question in your PR.

## Preparing a release

Release the four production crates together: `poirot-parsing`, `poirot-functions`,
`poirot-io`, and `poirot`. The test runner and authored specifications stay
unpublished. Keep package versions and internal dependency requirements aligned.
Use Rust 1.90 or newer and Python 3.11 or newer for the release tools.

After the workspace checks above, run:

```sh
cargo +1.90.0 check --workspace --all-features --locked
python3 tools/test_package_contents.py
python3 tools/test_release.py
python3 tools/verify_release.py --offline
python3 tools/verify_release.py --order --offline
```

The verification command packages the publishable dependency closure, checks legal
records and README links, then builds all targets, doctests and API documentation
from the normalized archives in a temporary workspace. Local patches point only
to those extracted archives, so unpublished siblings can be verified together.
It writes the current release archives under `target/release-artifacts/package/`.
The final command prints dependency-first publication order.

Offline verification requires dependencies already cached by Cargo. Omit
`--offline` to let Cargo fetch missing dependencies, or add `--allow-dirty` while
reviewing uncommitted changes. Run again from a clean checkout before release.
This does not upload anything or check crates.io name availability, ownership or
registry acceptance. Those checks and publication are a separate release step.

To publish a release, set the four production crate versions and their internal
dependency requirements, commit the changes, then push a matching tag such as
`v0.1.1`. The release workflow checks that the tag matches every production crate,
runs the full CI workflow, publishes in dependency order, and creates a GitHub
release. Publishing requires the repository's `CARGO_REGISTRY_TOKEN` Actions secret.
