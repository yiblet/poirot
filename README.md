# Poirot

Fast Excel calculation in Rust, without running Excel.

Poirot lets you use spreadsheets as calculation models inside your application.
Read or create a workbook, change its inputs, evaluate its formulas, and write
the results to XLSX. XLS is supported as input only.

Use Poirot when you know the workbooks you want to evaluate, when you or an AI
generate the spreadsheets, or when fast recalculation matters. Workbooks with
straightforward formulas under your control are a particularly good fit: you can
validate the model across the input cases your application needs.

[Run the speed benchmarks](benchmarks/README.md) to compare calculation modes
with IronCalc and HyperFormula using the same workloads in separate containers.

## Quick start

Poirot requires Rust 1.90 or newer. Add the Git dependency to `Cargo.toml`:

```toml
[dependencies]
poirot = { git = "https://github.com/yiblet/poirot.git", branch = "master" }
```

Model daily energy use with operating hours in `A1`, power in kilowatts in `B1`,
and energy in kilowatt-hours in `C1`:

```rust
use poirot::{CellValue, Workbook};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Energy")?;
    sheet.set_value("A1".parse()?, 12.)?;
    sheet.set_value("B1".parse()?, 25.)?;
    sheet.set_formula("C1".parse()?, "=A1*B1")?;
    book.recalculate()?;

    let energy = book.stored_cell("Energy", "C1".parse()?).ok_or("missing energy result")?;
    assert_eq!(energy.value, CellValue::Number(300.));
    book.save_xlsx().write_file("energy.xlsx")?;
    Ok(())
}
```

To use an existing model, start with `Workbook::open("energy.xlsx")` or
`Workbook::from_bytes(&bytes)`. Reading a cell returns its stored result; call
`recalculate()` after editing inputs to update it. Saving writes those results.

## Calculate the way your application needs

Poirot is optimized for fast evaluation, including repeated changes to a model.
Choose how much calculation state to keep:

| Workflow | API | What it does |
| --- | --- | --- |
| Edit a workbook and save current results | `Workbook::recalculate` | Calculates the workbook and updates stored values. |
| Query individual cells or formula text | `eval::run_cell`, `eval::run_formula` | Runs a direct calculation against workbook data. |
| Compare scenarios without changing the original | `book.evaluation().build()` | Calculates requested outputs and their dependencies; input edits clear calculated results. |
| Repeatedly edit a larger model | `book.overlay().build()` | Retains edits, compiled formulas, and intermediate results; reuses computations unaffected by changes. |

An overlay keeps state between calculations. It leaves the baseline workbook
unchanged until you explicitly apply its changes. Native snapshots let you save
calculation state and resume it later.

Selected outputs need no explicit default context. Calculate first, then export the
scenario directly with `evaluation.save_xlsx()` or `overlay.save_xlsx()`. Export
preserves edits and warm state, and does not calculate untouched formulas; their
saved results may still be stale. See [evaluation and export](docs/evaluation.md)
for modes, cycle boundaries, lifecycle operations and snapshot recovery.

The component crates also let you use parsing, workbook storage, formatting, or
function algorithms independently. Custom functions can be registered through
`eval::extensions`; existing built-in functions keep their implementations.

## Excel compatibility

Poirot targets modern Excel and is useful today for known spreadsheet models.
Validate the input cases your application uses. The [compatibility guide](docs/compatibility.md)
covers function and file support, known gaps, test evidence, and reporting issues.

## Roadmap and contributing

We want to build the fastest Excel calculation engine, so developers can use AI
to write spreadsheets and run them inside their applications. Our directions are:

- **More compatibility:** fill function and edge-case gaps against modern Excel.
- **More evaluation modes:** support workloads that benefit from different
  execution strategies.
- **Faster existing modes:** improve direct evaluation, `Evaluation`, and
  `Overlay`, measuring both workbook calculations and repeated edits.
- **More extensibility:** make Poirot modular enough to adapt Excel to your use
  case, including custom functions, alternative behavior, and execution strategies.
  Intentional variations should be explicit choices.

Keeping Poirot lightweight and easy to embed is part of that work. These are
ongoing goals; custom functions and component crates are available today, with
broader customization still ahead.

We're looking for contributors. AI-written PRs are welcome: review the changes,
include evidence for expected behavior, and follow [Contributing](CONTRIBUTING.md).

Imported shared formulas become independent, complete formula strings; replacing
an original anchor leaves its former followers independent. The compiler reuses
executable templates without retaining the original shared encoding. Optional
manually supplied group provenance is checked before export. Native snapshot
source version 5 can also carry legacy opaque source bytes, but XLSX I/O neither
retains original packages nor uses those bytes when saving.

## Examples

- [Edit a file](examples/public_workbook.rs): load a workbook, change an input,
  calculate, and save.
- [Query a model](examples/direct_evaluation.rs): try different inputs without
  changing the original workbook.
- [Reuse calculations](examples/flush_overlay.rs): retain overlay results across
  edits and flush inputs into an owned workbook.
- [Save calculation state](examples/snapshot_file.rs): save a checkpoint and
  resume it later.

The file example changes `A1` in the first sheet. Use a workbook where that cell
is an input:

```sh
cargo run --example public_workbook -- input.xlsx output.xlsx
```

Browse the API and task guide with `cargo doc -p poirot --no-deps --open`.
See [Contributing](CONTRIBUTING.md) to work on Poirot.

The application API uses `Evaluation` for direct scenarios and `Overlay` for
incremental scenarios. Both provide explicit editing, evaluation, reporting and
export methods. See the [evaluation guide](docs/evaluation.md#public-vocabulary)
for lifecycle details.

## Acknowledgements

- [Calamine](https://github.com/tafia/calamine) provides streaming XLSX decoding and shared-formula translation.
- [rxls](https://github.com/HyunjoJung/rxls) provides XLS decoding and checked XLSX authoring.
- [Apache POI](https://poi.apache.org/) shaped Poirot from the start: our first few
  commits translated portions of its code into Rust. Our name is a nod to the
  project. Statistical kernels also adapt
  [Apache Commons Math](https://commons.apache.org/proper/commons-math/).
- [LibreOffice](https://www.libreoffice.org/) provided a foundation for checking
  correctness. Its unit tests helped us understand expected spreadsheet behavior
  and validate our implementation.
- We adapted date handling and regression tests from
  [ClosedXML](https://github.com/ClosedXML/ClosedXML).
  [IronCalc](https://github.com/ironcalc/IronCalc) provided design inspiration.

- [PCRE2](https://github.com/PCRE2Project/pcre2) provides the regex engine through
  the MIT-licensed [Rust bindings](https://github.com/BurntSushi/rust-pcre2).
  See [PCRE2 provenance and notices](LICENSES/PCRE2.md).

- [Boost.Math](https://github.com/boostorg/math) supplies the adapted binary64
  Bessel K approximations. See [source attribution and license](LICENSES/Boost-Math.md).

- [MLIR shape inference](https://mlir.llvm.org/docs/ShapeInference/) and
  [Futhark size types](https://futhark.readthedocs.io/en/latest/language-reference.html#size-types)
  and [Julia broadcasting](https://docs.julialang.org/en/v1/manual/arrays/#Broadcasting)
  informed the independently implemented formula type system. Their documentation,
  consulted on 2026-10-23, helped separate element types, dimensions and inference
  rules; no upstream implementation or tests were copied or translated. See the
  [architecture overview](docs/architecture.md#types-and-spills).

See [LICENSES/ACKNOWLEDGEMENTS](LICENSES/ACKNOWLEDGEMENTS) for credits and notices.
Poirot's code is licensed under [MIT](LICENSES/MIT.txt).

Resource quotas are optional. Ordinary calculation can omit detailed profiling; see
[resource configuration](docs/resources.md) for loading defaults, cycle boundaries,
quota errors, and profiling.
