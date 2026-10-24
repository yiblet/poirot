# Storage layout diagnostics

Measure generic cell storage without workbook parsing or formula execution:

```sh
cargo build --release --manifest-path benchmarks/storage-layout/Cargo.toml
benchmarks/storage-layout/target/release/poirot-storage-layout
```

Build and run a container from the repository root:

```sh
docker build -f benchmarks/storage-layout/Dockerfile -t poirot-storage-layout .
docker run --rm --cpuset-cpus=2 poirot-storage-layout
```

The image retains dependency licenses under `/licenses`. Use frozen native
executables for precise comparisons so container startup stays outside timings.

The JSON report covers a full 1,024 by 16 rectangle (`dense`), 2,048 isolated
pages (`sparse`), and two populated columns in distinct page columns (`striped`).
These deliberately exercise different storage densities; they do not estimate
the distribution of real spreadsheets.

Each report separates:

- Row-major and column-major insertion. Construction excludes destruction.
- First row and column scans, followed by explicitly warmed repeated scans.
- Point reads, allocation calls and bytes retained by initial construction.
- Immediate clone/destruction cycles, where allocator reuse is part of the work.
- Retained clone construction, first traversal and destruction, timed separately.

The immediate and retained clone measurements describe different lifetimes.
Neither should be presented as the intrinsic cost of cloning every workbook.
Insertion and clone allocation counters include the counter's runtime overhead;
compare matching builds and measurement conditions. Memory counts describe heap
allocations attributed to this thread, not process RSS or source payloads created
before a probe.

Freeze baseline and candidate executables before comparing them:

```sh
python3 benchmarks/storage-layout/compare.py /tmp/baseline /tmp/candidate \
  /tmp/storage-comparison --cpu 2
```

This runs at most ten alternating independent process pairs, using paired 97%
intervals and Holm correction across timing metrics. Requests within a process
are not treated as independent observations. `--pairs 1` checks the harness
without significance claims. Loading and first evaluation belong in the separate
[incremental replay benchmark](../incremental-replay/README.md), so deferring
index work cannot be reported as a free loading improvement.

```sh
python3 -m unittest discover -s benchmarks/storage-layout -p test_compare.py
cargo test --manifest-path benchmarks/storage-layout/Cargo.toml
cargo fmt --manifest-path benchmarks/storage-layout/Cargo.toml -- --check
cargo clippy --manifest-path benchmarks/storage-layout/Cargo.toml --all-targets -- -D warnings
```

The workloads and harness are original Poirot code. `allocation-counter` 0.8.1
is used under its MIT option (its published metadata is MIT/Apache-2.0); no
upstream implementation or fixture is copied.
