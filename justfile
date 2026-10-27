# Development tasks. `just ci` runs the same checks as .github/workflows/ci.yml.

# List available recipes.
default:
    @just --list

# Run every CI check (linux job, then the MSRV check).
ci: test fmt-check clippy doc release-tools architecture msrv

# Fast local gate before committing: formatting, lints, layout and tests.
check: fmt-check clippy architecture test

# Run the workspace tests.
test *args:
    cargo test --workspace --locked {{args}}

# Format all crates.
fmt:
    cargo fmt --all

# Fail if any crate is not formatted.
fmt-check:
    cargo fmt --all -- --check

# Lint all targets and features, denying warnings.
clippy:
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

# Build API documentation, denying warnings.
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked

# Test the packaging tools and verify release archives offline.
release-tools:
    python3 tools/test_package_contents.py
    python3 tools/test_release.py
    RUSTDOCFLAGS="-D warnings" python3 tools/verify_release.py --offline

# Check crate boundaries, eval module layout, eval dependencies and authored specs.
architecture: layout
    python3 tools/test_function_architecture.py
    python3 tools/test_coercion_architecture.py
    python3 crates/poirot-test-excel-specs/tools/maintain.py --check

# Check the eval module layout and folder dependency rules (see AGENTS.md).
layout:
    python3 tools/test_module_layout.py
    python3 tools/test_eval_dependencies.py

# Check the workspace builds with the minimum supported Rust version.
msrv:
    cargo +1.90.0 check --workspace --all-features --locked

# Run the spreadsheet compatibility corpus and report results.
runner *args:
    cargo run --release -p poirot-test-runner -- run {{args}}

# Build the incremental-replay benchmark in release mode.
bench-build:
    PCRE2_SYS_STATIC=1 cargo build --release --manifest-path benchmarks/incremental-replay/Cargo.toml

# Run the incremental-replay benchmark, e.g. `just bench persistent --synthetic diamond 100`.
bench *args: bench-build
    benchmarks/incremental-replay/target/release/poirot-incremental-replay {{args}}

# Remove build output to free disk space.
clean:
    cargo clean
    cargo clean --manifest-path benchmarks/incremental-replay/Cargo.toml
