//! Fast Excel calculation in Rust, without running Excel.
//!
//! Use Poirot to run known spreadsheet models inside your application, including
//! workbooks you or an AI generate. Start with [`Workbook`] to read or create XLSX
//! and XLS workbooks, edit inputs, calculate formulas, and save results.
//! Straightforward formulas under your control are a particularly good fit;
//! validate the model across the input cases your application needs.
//!
//! # Example: recalculate energy use
//!
//! Suppose `energy.xlsx` has a sheet named `Energy`, with operating hours in `A1`,
//! power in kilowatts in `B1`, and the formula `A1*B1` in `C1` for energy in
//! kilowatt-hours. Change the operating hours, recalculate, and save a copy:
//!
//! ```no_run
//! use poirot::Workbook;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut book = Workbook::open("energy.xlsx")?;
//! let sheet = book.sheet_by_name_mut("Energy").ok_or("missing Energy sheet")?;
//! sheet.set_value("A1".parse()?, 12.)?;
//! book.recalculate()?;
//!
//! let energy = book.cell("Energy", "C1".parse()?).ok_or("missing energy result")?;
//! println!("Energy (kWh): {:?}", energy.value);
//! book.save_xlsx().write_file("energy-updated.xlsx")?;
//! # Ok(())
//! # }
//! ```
//!
//! [`Workbook::open`] detects XLSX or XLS from the file contents.
//! [`Workbook::from_bytes`] reads the same formats from memory. Use
//! [`XlsxSaveBuilder::to_buffer`] when you need bytes instead of a file, or
//! [`Workbook::save_xls`] for XLS output.
//!
//! The main workbook operations are:
//!
//! - [`sheet_by_name_mut`](model::Workbook::sheet_by_name_mut) gives access to a
//!   sheet for editing.
//! - [`Workbook::cell`] reads a stored cell, including its value and formula.
//! - [`Workbook::recalculate`] updates calculated values after input changes.
//! - [`Workbook::save_xlsx`] and [`Workbook::save_xls`] choose the output format;
//!   their builders write a file or return bytes.
//!
//! # Example: create a workbook
//!
//! Build the same energy model from scratch. Cell addresses use A1 notation;
//! formulas accept an optional leading `=`. Twelve hours at 25 kW consume 300 kWh:
//!
//! ```
//! use poirot::{CellValue, Workbook};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut book = Workbook::new();
//! let sheet = book.add_sheet("Energy")?;
//! sheet.set_value("A1".parse()?, 12.)?;
//! sheet.set_value("B1".parse()?, 25.)?;
//! sheet.set_formula("C1".parse()?, "A1*B1")?;
//! book.recalculate()?;
//! let energy = book.cell("Energy", "C1".parse()?).ok_or("missing energy result")?;
//! assert_eq!(energy.value, CellValue::Number(300.));
//! let bytes = book.save_xlsx().to_buffer()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Choose a calculation path
//!
//! Poirot is optimized for fast calculation and repeated edits. Choose how much
//! calculation state your application needs to retain:
//!
//! | Use case | API | Calculation state |
//! | --- | --- | --- |
//! | Edit and save a workbook | [`Workbook::recalculate`] | Updates stored formula results. |
//! | Query a cell or formula text directly | [direct cell evaluation](poirot_eval::run_cell), [direct formula evaluation](poirot_eval::run_formula) | Runs against workbook data. |
//! | Compare scenarios | [`Evaluation`] | Calculates requested dependencies; edits clear results. |
//! | Repeatedly change a model | [`Overlay`] | Retains edits and reuses unaffected results. |
//!
//! # When formulas are calculated
//!
//! A formula cell has both a formula and a stored result. Reading the cell returns
//! that result; it does not run the formula. Loading a file keeps the results saved
//! by the application that wrote it. Those results may be out of date.
//!
//! Changing an input does not update dependent cells until you call
//! [`Workbook::recalculate`]. In the model above, changing operating hours from 12
//! to 24 leaves the old result of 300 kWh in `C1` until recalculation changes it
//! to 600 kWh.
//! Saving also uses the stored results, so calculate before saving when you need
//! the file to contain current values.
//!
//! If recalculation fails, none of its results are written to the workbook. Input
//! edits made before the call remain. A spreadsheet error is different: `1/0`
//! produces the cell value [`CellError::Div0`], which can be read and saved just
//! like a number. It does not cause `recalculate()` to return an error.
//!
//! # Import warnings
//!
//! Loading can change or omit unsupported file content. The workbook keeps these
//! warnings in [`FileMetadata::import_warnings`]. Review them before relying on a
//! saved copy:
//!
//! ```
//! # use poirot::Workbook;
//! # let book = Workbook::new();
//! if let Some(metadata) = &book.file_metadata {
//!     for warning in &metadata.import_warnings {
//!         eprintln!("{warning}");
//!     }
//! }
//! ```
//!
//! # Example: compare operating hours without changing the workbook
//!
//! To compare energy use for 12 and 24 operating hours without editing the original,
//! use an [`Evaluation`]. Set temporary operating hours and ask for energy use:
//!
//! ```
//! use poirot::{CellValue, Evaluation, EvaluationContext, Workbook};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut book = Workbook::new();
//! let sheet = book.add_sheet("Energy")?;
//! sheet.set_value("A1".parse()?, 12.)?;
//! sheet.set_value("B1".parse()?, 25.)?;
//! sheet.set_formula("C1".parse()?, "A1*B1")?;
//!
//! let mut evaluation = Evaluation::with_workbook(&book)
//!     .build(EvaluationContext::default())?;
//! let hours = evaluation.target("Energy!A1")?;
//! let energy = evaluation.target("Energy!C1")?;
//! let mut energies = Vec::new();
//! for hours_per_day in [12., 24.] {
//!     evaluation.set_value(hours, hours_per_day)?;
//!     energies.push(evaluation.cell(energy)?);
//! }
//! assert_eq!(energies, [CellValue::Number(300.), CellValue::Number(600.)]);
//! assert_eq!(book.cell("Energy", "A1".parse()?).ok_or("missing operating hours")?.value,
//!     CellValue::Number(12.));
//! # Ok(())
//! # }
//! ```
//!
//! Creating an evaluation does not calculate any cells. [`Evaluation::cell`]
//! calculates the requested cell and the dependencies needed to obtain its value.
//! Use [`Evaluation::outputs`] for several outputs or [`Evaluation::all`] for all
//! occupied cells and any spill cells produced by formulas.
//!
//! Each input edit clears the evaluation's calculated results. For repeated
//! scenarios in a larger model, [`Overlay`] can keep results whose dependencies
//! have not changed. It maintains dependency information between calculations;
//! select its outputs through [`Overlay::evaluation`]. Both leave the original
//! workbook unchanged unless you explicitly apply their changes.
//!
//! [`EvaluationContext`] supplies time, randomness, and calculation limits.
//! For repeatable results from functions such as `NOW()` or `RAND()`, supply the
//! same clock and seed. The same settings can be used with
//! [`Workbook::recalculate_with_context`].
//!
//! # Excel compatibility
//!
//! Poirot targets modern Excel. Validate the input cases your application uses;
//! see the [compatibility guide](https://github.com/yiblet/poirot/blob/master/docs/compatibility.md)
//! for function and file support, known gaps, test evidence, and reporting issues.
//!
//! # Other APIs
//!
//! [`Workbook`] handles the usual load, edit, calculate, and save workflow. Use
//! the component modules when you need one part of that workflow on its own:
//!
//! - [`model`] stores workbook data without file handling or calculation. Use it
//!   when another part of your application owns those operations.
//! - [`io`] reads XLS and XLSX and writes XLSX; [`xls`] writes XLS. These adapters
//!   work with the data model directly. [`Workbook`] already uses both.
//! - [`parsing`] lets you inspect a formula as an expression tree without running
//!   it. [`mod@format`] renders cell values as display text, such as `12.50` for
//!   the number `12.5` with format `0.00`.
//! - [`eval`] exposes the compiler and custom-function interfaces for embedding
//!   the calculation engine. Register custom functions through
//!   [the extension registry](poirot_eval::extensions); registrations cannot override existing builtins.
//! - [`edit`] updates formula references when changing workbook structure.
//!   [`translation_edit`] adjusts references within existing formulas.
//! - [`artifact`] saves native Poirot snapshots for restoring workbook or
//!   calculation state. Use the XLS/XLSX save methods for files opened in Excel.
//!
//! # Project direction
//!
//! We want to build the fastest Excel calculation engine so developers can use AI
//! to write spreadsheets and run them in applications. The roadmap is more modern
//! Excel compatibility, additional evaluation modes, faster existing modes, and
//! more extensibility, while keeping Poirot lightweight and easy to embed.
//!
//! Component crates and custom functions provide extension points today. Broader
//! support for alternative calculation behavior and execution strategies is a goal:
//! applications should be able to adapt Excel to their use cases through explicit
//! choices. Contributors are welcome, including AI-written PRs; see
//! [Contributing](https://github.com/yiblet/poirot/blob/master/CONTRIBUTING.md).

pub mod artifact;

pub use poirot_eval as eval;
pub use poirot_eval::{NumberLocaleV2, ProducerPolicy};
pub use poirot_format as format;
pub use poirot_io as io;
pub use poirot_model as model;
pub use poirot_parsing as parsing;
pub use poirot_workbook as workbook;
pub use poirot_xls as xls;

pub use poirot_model::{
    Cell, CellAddress, CellError, CellStyle, CellValue, DateSystem, ExcelText, FileFormat,
    FileMetadata, ImportDiagnostic, ImportDiagnosticKind, ModelError, SheetPresentation,
    SheetVisibility, TextRef, TextUnits, TextUnitsError, WorkbookChange, Worksheet,
};
pub use poirot_workbook::{
    CalculationError, CellEvaluation, CompilationUsage, Evaluation, EvaluationBuilder,
    EvaluationContext, EvaluationFailure, EvaluationTarget, LoadBuilder, Overlay, OverlayBuilder,
    OverlayError, OverlayEvaluation, OverlayPolicy, OverlaySnapshot, SavePlan, Workbook,
    WorkbookError, WorkbookSnapshot, WorkspaceState, XlsSaveBuilder, XlsxSaveBuilder, edit,
    translation_edit,
};
