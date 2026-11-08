# Contributing

Bug reports, fixes, tests, and documentation improvements are welcome, including
AI-written PRs. Review what you submit and make the behavior easy to verify.

## Getting started

Use Rust 1.90 or newer with rustfmt and Clippy, and Python 3.11 or newer.
Run commands from the repository root. Read the
[architecture guide](docs/architecture.md) for component boundaries and the
[compatibility guide](docs/compatibility.md) for supported behavior and known gaps.

Discuss larger changes in an issue before starting. For bug reports, include a
small workbook or formula, inputs, expected and actual results, and the Excel
version used when comparing against Excel.

## Preparing a change

Keep changes focused, follow the surrounding code, and preserve component
boundaries. Add regression tests for fixes. Derive expected spreadsheet results
from Excel, documentation, or another reliable reference, and record the source
and relevant versions or settings. For performance changes, include a repeatable
workload and before/after measurements.

Update API documentation when behavior changes. For compatibility changes, update
the relevant row in the compatibility guide, including remaining restrictions and
observable errors or warnings. Keep known gaps explicit.

Use module-owned `<Purpose>Error` types with `thiserror` and typed dependency
causes. Avoid generic message variants and dynamic error erasure; spreadsheet
errors remain ordinary values. The [error checker](tools/test_error_patterns.py)
documents the detailed rules and the exception for dependencies exposing only text.

## Submitting a PR

Run these checks before submitting:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
python3 tools/test_module_layout.py
python3 tools/test_eval_dependencies.py
python3 tools/test_error_patterns.py --self-test
python3 tools/test_error_patterns.py
```

CI runs additional checks defined in the [workflow](.github/workflows/ci.yml).
Use a Conventional Commit title, such as `fix(eval): handle empty ranges`.
Scopes are optional; add `!` before the colon for a breaking change. Use `ref`
for refactoring; the [title check](.github/workflows/pr-title.yml) lists accepted
types. Explain what changed, why, how you tested it, and any compatibility limits.

## Crediting others

Use only permissively licensed material compatible with Poirot's MIT license.
For adapted code, tests, data, or algorithms, record the upstream project, source
URL, revision, license, and what was reused. Preserve required notices and credit
substantial influences in the README. Leave material with unclear provenance or
licensing out until resolved.

CI runs for pull requests and pushes to `master`. New commits cancel superseded
checks for the same build configuration. Adding `release` or `all-builds` expands
the test matrix without rerunning formatting, lint, or documentation checks.

## Preparing a release

Release `poirot-parsing`, `poirot-functions`, `poirot-io`, and `poirot` together.
Run the **Prepare release** workflow when ready to start a release PR. Automation
chooses the next version, adds the `release` label, and updates that PR as changes
merge into `master`. After it closes, another manual run starts the next release.
Review the changes and wait for CI and archive validation to pass. Merging the PR
reruns the expanded build matrix and archive validation on that exact commit, then
waits for approval through the `release` environment before creating the version
tag, publishing, and creating a GitHub release.

To verify locally, run `python3 tools/verify_release.py` from a clean checkout.
Validation does not check crates.io ownership, name availability, or registry
acceptance.
