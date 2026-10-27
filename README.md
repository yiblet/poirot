# Poirot

Fast Excel calculation in Rust, without running Excel.

Poirot lets you use spreadsheets as calculation models inside your application.
Read or create a workbook, change its inputs, evaluate its formulas, and write
the results to XLSX. XLS is supported as input only.

Use Poirot when you know the workbooks you want to evaluate, when you or an AI
generate the spreadsheets, or when fast recalculation matters. Workbooks with
straightforward formulas under your control are a particularly good fit: you can
validate the model across the input cases your application needs.

[Run the speed benchmarks](https://github.com/yiblet/poirot/blob/master/benchmarks/README.md) to compare calculation modes
with IronCalc and HyperFormula using the same workloads in separate containers.

## Quick start

Poirot requires Rust 1.90 or newer. Version 0.1.0 is being prepared for
publication. Once published, add it to `Cargo.toml`:

```toml
[dependencies]
poirot = "0.1.0"
```

Until then, use the Git dependency:

```toml
[dependencies]
poirot = { git = "https://github.com/yiblet/poirot.git", branch = "master" }
```

Model daily energy use with operating hours in `A1`, power in kilowatts in `B1`,
and energy in kilowatt-hours in `C1`:

```rust
use poirot::Workbook;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Energy")?;
    sheet.set_value("A1", 12)?;
    sheet.set_value("B1", 25)?;
    sheet.set_formula("C1", "=A1*B1")?;
    book.recalculate()?;

    let energy = book.stored_cell("Energy", "C1")?.ok_or("missing energy result")?;
    let kilowatt_hours = energy.value.as_number().ok_or("expected numeric energy")?;
    assert_eq!(kilowatt_hours, 300.);
    book.save_xlsx().write_file("energy.xlsx")?;
    Ok(())
}
```

To use an existing model, start with `Workbook::open("energy.xlsx")` or
`Workbook::from_bytes(&bytes)`. Reading a cell returns its stored result; call
`recalculate()` after editing inputs to update it. Saving writes those results.
Use `as_number()` or `as_boolean()` to extract a scalar without coercion; each
returns `None` for other value kinds. Match `CellValue` when those distinctions
matter.

## Calculate the way your application needs

Poirot is optimized for fast evaluation, including repeated changes to a model.
Choose how much calculation state to keep:

| Workflow | API | What it does |
| --- | --- | --- |
| Edit a workbook and save current results | `Workbook::recalculate` | Calculates the workbook and updates stored values. |
| Query individual cells or formula text | `poirot::calculation::run_cell`, `poirot::calculation::run_formula` | Runs a direct calculation against workbook data. |
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

The component crates let you use formula parsing, file codecs, or pure function
algorithms and display formatting independently. Register custom functions through
[`poirot::calculation::extensions`](https://docs.rs/poirot/latest/poirot/calculation/extensions/); existing built-in functions
keep their implementations.

## Find the API for your task

`poirot::Workbook` is the same type as `poirot::model::Workbook`; no conversion is
needed between ordinary workflows and sparse storage APIs. Common types such as
`Worksheet`, `Cell`, `CellAddress`, `CellValue`, `CellError`, `DateSystem` and
`ModelError` are exported directly from `poirot`.

| Domain | Use it for |
| --- | --- |
| `poirot::cells` | Exact Excel text, text units and formula provenance |
| `poirot::styles` | Cell presentation and number-format display rendering |
| `poirot::calculation` | `Evaluation`, `Overlay`, contexts, limits, direct runs and custom functions |
| `poirot::files` | Import/export options, file metadata and diagnostics |
| `poirot::editing` | Sheet renaming and checked formula-reference translation |
| `poirot::snapshots` | Native source and prepared calculation-state persistence |

Start with `poirot::calculation` for application calculation. The public
`poirot::eval` modules expose compiler, engine, observation and run contracts for
advanced integrations; their types and reporting defaults can differ from the
application `Evaluation` and `Overlay` workflows. `poirot::model` exposes sparse
storage and queries; reading stored cells never calculates formulas.

Use the component crates independently when you need only
[formula/text parsing](https://docs.rs/poirot-parsing),
[pure functions and display formatting](https://docs.rs/poirot-functions), or
[file codecs with your own storage](https://docs.rs/poirot-io).

## Excel compatibility

Poirot targets modern Excel, including dynamic arrays, with partial compatibility.
It does not claim complete Excel equivalence or lossless workbook round trips.
Validate the input cases and edits your application uses. The
[compatibility guide](docs/compatibility.md) lists feature status, default policies,
observable differences, file losses and ways to verify your model. A registered
function or a passing comparison case does not establish every Excel edge case.

XLSX files retain ordinary cell fonts, pattern fills, borders, alignment and number
formats, including styled blank cells. Edit `Cell::style` with `CellStyle` and a
shared `VisualStyle`, or use an overlay's `set_style`. Theme and indexed colors
retain their source identities; Poirot does not render workbook layout.

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
manually supplied group provenance is checked before export. Native snapshots
use source schema 8 and reject older source versions. XLSX saves author fresh
packages from modeled content.

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
After publication, the [API reference](https://docs.rs/poirot/0.1.0/poirot/)
will also be available on docs.rs.
See [Contributing](CONTRIBUTING.md) to work on Poirot.

The application API uses `Evaluation` for direct scenarios and `Overlay` for
incremental scenarios. Both provide explicit editing, evaluation, reporting and
export methods. See the [evaluation guide](docs/evaluation.md#public-vocabulary)
for lifecycle details.

## Acknowledgements

- [Unicode 17.0.0](https://www.unicode.org/Public/17.0.0/ucd/UnicodeData.txt)
  supplies the selected Japanese width and voiced-kana mappings, under
  [Unicode License V3](LICENSES/Unicode-17.txt).
- [Apache POI](https://poi.apache.org/) shaped Poirot from the start: our first few
  commits translated portions of its code into Rust. Our name is a nod to the
  project. Statistical kernels also adapt
  [Apache Commons Math](https://commons.apache.org/proper/commons-math/).
- [LibreOffice](https://www.libreoffice.org/) provided a foundation for checking
  correctness. Its unit tests helped us understand expected spreadsheet behavior
  and validate our implementation.
- We adapted date handling and regression tests from
  [ClosedXML](https://github.com/ClosedXML/ClosedXML).
- [Boost.Math](https://github.com/boostorg/math) supplies the adapted binary64
  Bessel K approximations. See [source attribution and license](LICENSES/Boost-Math.md).

See [LICENSES/ACKNOWLEDGEMENTS](LICENSES/ACKNOWLEDGEMENTS) for credits and notices.
Poirot's code is licensed under [MIT](LICENSES/MIT.txt).

Resource quotas are optional. Ordinary calculation can omit detailed profiling; see
[resource configuration](docs/resources.md) for loading defaults, cycle boundaries,
quota errors, and profiling.
