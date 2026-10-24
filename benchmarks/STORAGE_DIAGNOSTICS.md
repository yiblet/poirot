# Optional storage and spill workloads

`storage_workloads.py` emits schema2 protocol JSON for the existing Poirot adapter.
It leaves the default cross-engine suite unchanged: `SEQUENCE` and dynamic spills
are not supported by every comparison engine.

```sh
python3 benchmarks/storage_workloads.py --size 1024 > /tmp/storage-workloads.json
PCRE2_SYS_STATIC=1 cargo build --release --manifest-path benchmarks/rust/Cargo.toml -p poirot-speed
benchmarks/rust/target/release/poirot-speed overlay-batch < /tmp/storage-workloads.json
benchmarks/rust/target/release/poirot-speed evaluation-batch < /tmp/storage-workloads.json
# After building the existing Poirot image as described in README.md:
docker run --rm -i --network none poirot-bench-poirot:local overlay-batch < /tmp/storage-workloads.json
python3 -m unittest discover -s benchmarks -p test_storage_workloads.py
```

The six layouts perform identical independent affine calculations in tall, wide,
rectangular, checkerboard, scattered and many-sheet coordinates. Each has the
same two input edits and output count. Spill controls cover many independent
2x2 producers, a single large four-column rectangle, and a scalar/array switch
with a read of a former child. Expectations come from affine arithmetic and
row-major SEQUENCE enumeration, including blank-child coercion in addition.
These are original synthetic controls, not a claim about workbook prevalence.

`--without-base` emits only these nine diagnostics; otherwise the seven standard
arithmetic/function workloads are included. Size is bounded to 4..5000. The
many-producer case caps producer count at 1024; the large spill retains four
columns and scales to the requested row count. Default samples 3 and warmups 2
are configurable. `unchanged_read` adds a separate no-edit read phase. Input
generation and JSON parsing are outside the adapter's timed phases; edit phases
include one immediate write, calculation and result reads. They do not measure
grouped invalidation or dense bulk edit insertion. For storage construction,
first/warm traversal and allocation controls, use the [storage-layout diagnostic](storage-layout/README.md).

A matched comparison must use the same generated JSON and validate every output
before interpreting timings. Treat process runs as independent samples, cap the
sample count, and report uncertainty and multiple-comparison correction. This
suite does not include private workbook data or introduce engine dependencies.
