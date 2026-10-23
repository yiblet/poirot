# Evaluation outlier diagnosis

Baseline: `f8d53fb`, before production fixes. These are mechanism probes, not an
engine ranking. All outputs passed independently derived arithmetic checks.

## Method

Native Rust release adapters, serial on CPU 0, 15 fresh-model samples and three
warmups, sizes 64/128/256/512. Engine order was shuffled with seed 42. Both engines
used the same host, affinity and native runtime; there was no Docker CPU quota.
Raw reports and source hashes are in ignored
`results/outlier-diagnosis-controlled/`. An earlier exploratory run overlapped
compilation and is excluded from the numbers below. Existing Docker comparison
results remain unchanged. Sampling/CPU controls differ from those older results;
compare engines within a run, not elapsed times between runs.

Reproduce the paired workflows through the normal Docker runner:

```sh
python3 benchmarks/run.py --diagnostics --size 512 --samples 15 --warmup 3 \
  --engines poirot-evaluation poirot-evaluation-batch poirot-overlay poirot-overlay-batch ironcalc
```

For native probes, build once before timing, emit suites at each size, then feed
identical JSON to `taskset -c CPU benchmarks/rust/target/release/poirot-speed MODE`
and `taskset -c CPU benchmarks/rust/target/release/ironcalc-speed`. Do not run
compilation during measurement:

```sh
cargo build --release --manifest-path benchmarks/rust/Cargo.toml --workspace
python3 benchmarks/diagnostics.py --size 512 > /tmp/poirot-probes.json
BENCHMARK_SUITE=/tmp/poirot-probes.json cargo test --release \
  --manifest-path benchmarks/rust/Cargo.toml -p poirot-speed scaling_execution_counters -- --ignored --nocapture
```

Counters run separately from timing. No hardware profiler attribution is claimed:
`perf_event_paranoid=4` denied task-clock access on this host.

## Findings

1. **API choice exposes shared engine overhead.** The original adapter calls
   `.cell()` for every requested output. At 512 independent branches, Evaluation's
   first edit costs 6.671 ms per-cell versus 0.307 ms batched. Both execute exactly
   512 formulas, read 1,024 cells and perform 512 scalar operations. In
   `overlay/direct.rs::perform`, every successful growing memo scans all keys
   into completion and spill-reset sets. The sum of memo lengths is 262,656 per
   pass, versus 1,024 for one batch. Overlay additionally calls
   `dependencies.rs::Cache::commit` and `rebuild_ranges` after each publication.
   Batch selection is useful, but fixing the engine is required to make ordinary
   per-cell API calls scale. Both adapter paths remain available.
2. **Range overhead survives batching and is not inherent overlap work.**
   Disjoint four-cell ranges perform only 2,048 useful numeric reads at size 512,
   yet the evaluator charges 525,312 reference pairs. The count is exactly
   `4*sum(1..512)`: `spill_publication.rs::discover_spills_before` revisits all
   preceding stored literals to exclude spill anchors. Batched Evaluation still
   takes 10.626 ms for the first edit versus IronCalc's 0.227 ms. Optimize shared
   candidate discovery rather than particular range formulas.
3. **Useful growing-range work must stay ordered.** Prefix sums legitimately read
   131,328 range members at size 512. Counters show 787,968 sparse traversal units
   and 397,568 materializations, in addition to those reads.
   `functions.rs::sparse_addresses` counts then collects coordinates;
   `value_projection.rs` and statistics adapters materialize consumer values.
   Removing redundant traversal/allocation is a general opportunity. Reusing a
   prefix sum algebraically would change floating-point order and is outside scope.
4. **Position costs require a separate investigation.** Moving a shared four-cell
   range below all outputs changes batched Evaluation edit time from 0.964 to
   2.725 ms. Formula executions, cell reads, cache hits, reference pairs and sparse
   entry counters are identical; formula text has longer row numbers. Candidate
   discovery's underlying `run_overlay.rs::ColumnTraversal::charge_rows` still
   walks occupied rows outside the selected column, with no corresponding
   reference-pair charge. This is a source-supported suspect, not timed attribution.
5. **Smaller gaps remain after these fixes.** One-output arithmetic/fan-out,
   conditionals and dependency chains show almost identical batch/per-cell times.
   They cannot be explained by repeated output publication alone. Evaluation
   intentionally clears all memoized results on edits; Overlay keeps dependency
   certificates and must validate/maintain them. Measure source lookup, scalar
   execution, invalidation and certificate publication before choosing follow-ups.

## Scaling

First edit medians, milliseconds; each column doubles the model size.

| Probe / mode | 64 | 128 | 256 | 512 |
| --- | ---: | ---: | ---: | ---: |
| independent_branches / poirot-evaluation | 0.1280 | 0.4337 | 1.5484 | 6.6712 |
| independent_branches / poirot-evaluation-batch | 0.0369 | 0.0734 | 0.1492 | 0.3067 |
| prefix_sums / poirot-evaluation-batch | 0.3941 | 1.3876 | 4.9441 | 19.3046 |
| fixed_disjoint_ranges / poirot-evaluation-batch | 0.2752 | 0.8605 | 2.8785 | 10.6262 |
| fixed_shared_top / poirot-evaluation-batch | 0.1183 | 0.2464 | 0.4788 | 0.9642 |
| fixed_shared_bottom / poirot-evaluation-batch | 0.1420 | 0.3599 | 0.9119 | 2.7254 |

## Original workloads at size 512

All medians in milliseconds. `Eval`/`Overlay` retain the original per-cell paths;
`batch` changes only the output request boundary. Every phase and edit is shown
so small losses remain visible alongside large ones.

| Workload | Phase | Eval | Overlay | Eval batch | Overlay batch | IronCalc |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| arithmetic | initial | 1.1023 | 1.4065 | 1.1236 | 1.3846 | 1.4781 |
| arithmetic | edit 1 | 0.4710 | 0.3775 | 0.4816 | 0.3733 | 0.1069 |
| arithmetic | edit 2 | 0.4618 | 0.3751 | 0.4747 | 0.3702 | 0.1067 |
| fan_out | initial | 1.3152 | 1.6619 | 1.3497 | 1.6355 | 0.8996 |
| fan_out | edit 1 | 0.3885 | 0.7574 | 0.3901 | 0.7522 | 0.0872 |
| fan_out | edit 2 | 0.3834 | 0.7554 | 0.3847 | 0.7555 | 0.0871 |
| prefix_sums | initial | 24.8010 | 32.5070 | 19.5539 | 22.6899 | 3.6058 |
| prefix_sums | edit 1 | 25.3918 | 33.6021 | 19.3046 | 23.9157 | 2.4229 |
| prefix_sums | edit 2 | 25.4354 | 33.7456 | 19.2905 | 23.9726 | 2.4297 |
| conditional | initial | 2.0973 | 2.4493 | 2.0749 | 2.3944 | 3.3546 |
| conditional | edit 1 | 0.7306 | 0.3935 | 0.7159 | 0.3947 | 0.3596 |
| conditional | edit 2 | 0.7255 | 0.3823 | 0.7129 | 0.3901 | 0.3620 |
| exact_lookup | initial | 15.3513 | 17.3244 | 15.1680 | 19.3444 | 66.3408 |
| exact_lookup | edit 1 | 13.4900 | 0.3748 | 13.3547 | 0.3974 | 63.7462 |
| exact_lookup | edit 2 | 13.5659 | 0.3339 | 13.2824 | 0.3381 | 63.8654 |
| dependency_chain | initial | 0.0463 | 0.0614 | 0.0483 | 0.0616 | 0.0622 |
| dependency_chain | edit 1 | 0.0134 | 0.0331 | 0.0135 | 0.0332 | 0.0067 |
| dependency_chain | edit 2 | 0.0124 | 0.0323 | 0.0127 | 0.0326 | 0.0066 |
| independent_branches | initial | 6.0166 | 6.9915 | 0.8304 | 1.0517 | 1.1233 |
| independent_branches | edit 1 | 6.6712 | 0.1811 | 0.3067 | 0.1500 | 0.0988 |
| independent_branches | edit 2 | 6.6064 | 0.1774 | 0.2944 | 0.1451 | 0.0984 |

## Next changes and completion limits

A small retained index of possible spill producers may be justified by the measured
quadratic scans. Keep it tied to source/formula/policy invalidation; preserve
unknown-shape extension handling, dynamic-spill arbitration and newly introduced
producers. A worksheet without eligible producers still needs membership guards
so future edits cannot reuse stale emptiness. Prefer existing compiled metadata
when it provides the same information with less state.

First make completion/publication proportional to newly completed work, preserving
atomic failure and spill semantics. Then remove measured shared range discovery
and traversal costs. Repeat this matrix before grooming smaller scalar/invalidation
optimizations. The board's final acceptance requires Poirot to meet or beat
IronCalc on every measured workload/phase/edit under warmed, repeated comparable
runs; these baseline diagnostics do not satisfy or replace that requirement.
