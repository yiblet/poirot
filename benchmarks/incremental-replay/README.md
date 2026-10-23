# Incremental scenario replay

This standalone benchmark compares three ways to evaluate independent scenarios:

- `persistent` retains an Overlay, restores inputs omitted by the next scenario,
  and writes only changed overrides.
- `reset` restores the baseline before every request while retaining compiled
  programs. It deliberately discards incremental result evidence.
- `fresh` constructs a new Overlay for every request. Its output hashes are the
  correctness control for the other modes.

Build in release mode before measuring:

```sh
PCRE2_SYS_STATIC=1 cargo build --release \
  --manifest-path benchmarks/incremental-replay/Cargo.toml
benchmarks/incremental-replay/target/release/poirot-incremental-replay \
  persistent --synthetic diamond 100
```

Synthetic cases have independent mathematical expectations. `diamond` changes two
rounded inputs while an array reduction's answer remains unchanged; `branches`
switches a lazy condition away from an erroring unused branch; `spill` alternates
blank reads, insertion/removal of a spill anchor and unchanged rounded producer
inputs. All modes must satisfy those expectations. These deliberately exercise
feature behavior; they do not estimate its frequency in real workbooks. The
`diamond` control is `(SUM(1..500) + 500) * 2 + 500 = 252000`.

External scenarios use this JSONL shape (keys within each object are ordered by
serde_json; output order must remain consistent between runs):

```json
{"inputs":{"amount":{"cell":"Sheet1!A1","string":"12"}},"output_cells":{"total":"Sheet1!B1"},"clock_unix_seconds":0}
```

Each request describes overrides relative to the original workbook, not edits to
its predecessor. A `string` input parses as a Java-compatible number or literal
text. On an original formula cell it updates the saved source value and preserves
the formula, matching the older rate-replay harness; the default Compute policy
still evaluates that formula. An explicit `formula` field replaces the formula;
`"blank":true` writes an actual blank. Omitted previous inputs restore baseline
content, including original formulas and absent cells. Replacing saved spill
children is refused because this interface cannot restore their ownership.

```sh
benchmarks/incremental-replay/target/release/poirot-incremental-replay \
  persistent /path/model.xlsx /path/requests.jsonl 100
```

The CLI emits only per-request hashes, counts and timings. Raw inputs and outputs
are never emitted or written to temporary files. Operational failures abort the
run; worksheet errors are counted and the comparison runner refuses them. This
is a performance fixture loader, with XLSX size limits lifted for large local
models. It adds no public API or engine dependencies.

For a matched source comparison, build this exact harness twice against the two
Poirot source trees and supply the frozen binaries:

```sh
python3 benchmarks/incremental-replay/compare.py \
  /path/baseline-replay /path/candidate-replay /tmp/replay-comparison \
  --workbook /path/model.xlsx --requests /path/requests.jsonl --limit 100 --cpu 2
```

The runner alternates version order, rotates mode order, and caps measurement at
10 independent process pairs. Requests within each process are correlated and
are averaged before inference. It reports individual paired 97% intervals and
Holm-adjusted two-sided Student t tests over all reported metrics. `--pairs 1`
is a correctness smoke run without significance claims. Explicit
`--exclude-case-mode CASE:MODE` omissions apply to both versions and are recorded
in metadata; use these only for documented unsupported controls, never silently
discard a failed sample. Every version and mode
must produce identical request hashes, including the independent fresh runs.

File read plus XLSX loading, first evaluation, later evaluations, edit/reset work,
and whole-process peak RSS are distinct measurements. Timed evaluation includes
cycle creation, result collection and publication; hashing, JSON preparation and
input-target resolution are excluded. Mean request work includes reset/build,
input edits and evaluation, but excludes request parsing and output hashing. Peak
RSS includes workbook loading, all prepared requests and hash/report storage.
This intentionally omits display formatting and is not directly comparable with
the older presentation-heavy rate-replay waterfall.

Checks:

```sh
cargo test --manifest-path benchmarks/incremental-replay/Cargo.toml
cargo fmt --manifest-path benchmarks/incremental-replay/Cargo.toml -- --check
cargo clippy --manifest-path benchmarks/incremental-replay/Cargo.toml --all-targets -- -D warnings
python3 -m unittest discover -s benchmarks/incremental-replay -p test_compare.py
```

Input writes use the immediate edit APIs, matching the older rate harness.
This does not measure grouped `overlay.edit` invalidation.

These workloads and the harness are original Poirot code. No external workbook,
formula corpus, implementation or test fixture is included.
