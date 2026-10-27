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
//! sheet.set_value("A1", 12)?;
//! book.recalculate()?;
//!
//! let energy = book.stored_cell("Energy", "C1")?.ok_or("missing energy result")?;
//! println!("Energy (kWh): {:?}", energy.value);
//! book.save_xlsx().write_file("energy-updated.xlsx")?;
//! # Ok(())
//! # }
//! ```
//!
//! [`Workbook::open`] detects XLSX or XLS from the file contents.
//! [`Workbook::from_bytes`] reads the same formats from memory. Use
//! [`files::XlsxSaveBuilder::to_buffer`] when you need bytes instead of a file. Saves
//! produce XLSX; legacy XLS is supported as input only.
//!
//! Calamine streams XLSX input, Poirot decodes BIFF8 XLS input, and rust_xlsxwriter
//! authors XLSX packages. Saves encode the modeled calculation
//! data; charts, drawings, macros, rich styling and other unmodeled features are
//! omitted. Review import warnings before relying on an exported copy.
//!
//! Imported shared formulas become complete, independent [`Cell::formula`] strings.
//! Replacing the original anchor leaves followers independent. Optional manually
//! supplied [`cells::SharedFormula`] provenance is verified before saving shared records.
//! Executable template reuse belongs to the compiler and does not depend on file
//! provenance. XLSX saves always author a fresh modeled package.
//!
//! The main workbook operations are:
//!
//! - [`sheet_by_name_mut`](Workbook::sheet_by_name_mut) gives access to a
//!   sheet for editing.
//! - [`Workbook::stored_cell`] reads a stored cell, including its value and formula.
//! - [`Workbook::recalculate`] updates calculated values after input changes.
//! - [`Workbook::save_xlsx`] writes XLSX to a file or returns bytes.
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
//! sheet.set_value("A1", 12)?;
//! sheet.set_value("B1", 25)?;
//! sheet.set_formula("C1", "A1*B1")?;
//! book.recalculate()?;
//! let energy = book.stored_cell("Energy", "C1")?.ok_or("missing energy result")?;
//! assert_eq!(energy.value, CellValue::Number(300.));
//! let bytes = book.save_xlsx().to_buffer()?;
//! let mut loaded = Workbook::from_bytes(&bytes)?;
//! loaded.sheet_by_name_mut("Energy").ok_or("missing Energy")?
//!     .set_value("A1", 24)?;
//! loaded.recalculate()?;
//! assert_eq!(loaded.stored_cell("Energy", "C1")?.ok_or("missing result")?.value,
//!     CellValue::Number(600.));
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
//! | Query a cell or formula text directly | [direct cell evaluation](crate::calculation::run_cell), [direct formula evaluation](crate::calculation::run_formula) | Runs against workbook data. |
//! | Compare scenarios | [`Workbook::evaluation`] | Calculates requested dependencies; edits clear results. |
//! | Repeatedly change a model | [`Workbook::overlay`] | Retains edits and reuses unaffected results. |
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
//! warnings in [`files::FileMetadata::import_warnings`]. Review them before relying on a
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
//! use a [`calculation::Evaluation`]. Set temporary operating hours and ask for energy use:
//!
//! ```
//! use poirot::{CellValue, Workbook};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut book = Workbook::new();
//! let sheet = book.add_sheet("Energy")?;
//! sheet.set_value("A1", 12)?;
//! sheet.set_value("B1", 25)?;
//! sheet.set_formula("C1", "A1*B1")?;
//!
//! let mut evaluation = book.evaluation().build()?;
//! let hours = evaluation.target("Energy!A1")?;
//! let energy = evaluation.target("Energy!C1")?;
//! let mut energies = Vec::new();
//! for hours_per_day in [12., 24.] {
//!     evaluation.set_value(hours, hours_per_day)?;
//!     energies.push(evaluation.cell(energy)?);
//! }
//! assert_eq!(energies, [CellValue::Number(300.), CellValue::Number(600.)]);
//! assert_eq!(book.stored_cell("Energy", "A1")?.ok_or("missing operating hours")?.value,
//!     CellValue::Number(12.));
//! # Ok(())
//! # }
//! ```
//!
//! Creating an evaluation does not calculate any cells. [`calculation::Evaluation::cell`]
//! calculates the requested cell and the dependencies needed to obtain its value.
//! Use [`calculation::Evaluation::outputs`] for several outputs or [`calculation::Evaluation::all`] for all
//! occupied cells and any spill cells produced by formulas.
//!
//! Each input edit clears the evaluation's calculated results. For repeated
//! scenarios in a larger model, [`calculation::Overlay`] can keep results whose dependencies
//! have not changed. It maintains dependency information between calculations;
//! select its outputs through [`calculation::Overlay::evaluation`]. Borrowed scenarios
//! leave the original workbook unchanged. For an owned overlay,
//! [`calculation::Overlay::apply_edits_to_workbook`] applies authored edits while
//! retaining warm calculations; it does not publish calculated results.
//!
//! Export a scenario with [`calculation::Evaluation::save_xlsx`] or
//! [`calculation::Overlay::save_xlsx`]. Export captures edits and valid results without
//! calculation, baseline mutation or loss of warm state. Untouched saved formula
//! values may be stale. See the
//! [evaluation guide](https://github.com/yiblet/poirot/blob/master/docs/evaluation.md)
//! for cycle boundaries, lifecycle operations and snapshot recovery.
//!
//! [`calculation::EvaluationContext`] supplies time, randomness, and calculation limits.
//! For repeatable results from functions such as `NOW()` or `RAND()`, supply the
//! same clock and seed. The same settings can be used with
//! [`Workbook::recalculate_with_context`].
//!
//! # Excel compatibility
//!
//! Poirot targets modern Excel, including dynamic arrays, with partial compatibility.
//! It does not claim complete Excel equivalence or lossless workbook round trips.
//! Validate the input cases and ordered edits your application uses;
//! see the [compatibility guide](https://github.com/yiblet/poirot/blob/master/docs/compatibility.md)
//! for feature status, default policies, observable differences, file losses and
//! validation steps. Registered functions and passing cases are not proof of full parity.
//!
//! # Other APIs
//!
//! [`Workbook`] handles the usual load, edit, calculate, and save workflow. Use
//! the six consumer domains for more focused jobs:
//!
//! - [`cells`] supplies shared-formula provenance and lossless Excel text.
//! - [`styles`] inspects presentation and renders values with number formats.
//! - [`calculation`] configures direct and incremental scenarios and their limits.
//! - [`files`] configures imports and Excel exports and exposes diagnostics.
//! - [`editing`] renames sheets and translates formula references atomically.
//! - [`snapshots`] saves native state and restores source with visible preparation status.
//!
//! [`model`] exposes sparse storage and queries; its reads never calculate formulas.
//! [`eval`] exposes public compiler, engine, observation and run contracts for advanced
//! integrations. Start with [`calculation`] for application [`calculation::Evaluation`]
//! and [`calculation::Overlay`]; engine types and reporting defaults can differ.
//! The root [`Workbook`] and [`model::Workbook`] are the same type.
//!
//! Component crates work independently: [poirot-parsing](https://docs.rs/poirot-parsing)
//! supplies formula/text syntax, [poirot-functions](https://docs.rs/poirot-functions)
//! supplies pure algorithms and display formatting, and
//! [poirot-io](https://docs.rs/poirot-io) supplies codecs for caller-owned storage.
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

/// Direct and incremental calculation, semantics, contexts, and resource limits.
pub mod calculation;
/// Text storage, text units, and shared-formula provenance.
pub mod cells;
/// Structural edits and source-checked formula reference translations.
pub mod editing;
/// Advanced compiler, engine, observation and run contracts; applications start with [`calculation`].
pub mod eval;
/// Excel loading, export, source metadata, and import diagnostics.
pub mod files;
/// Editable source storage and workbook queries.
pub mod model;
/// Native source and calculation-state persistence with explicit recovery policies.
pub mod snapshots;
/// Cell presentation and display formatting.
pub mod styles;

mod calculation_error;
mod edit_error;
mod file_adapter;
mod xlsx_save_error;

pub use crate::model::{
    Cell, CellAddress, CellError, CellValue, DateSystem, ModelError, Workbook, Worksheet,
};
