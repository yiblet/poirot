//! Run known spreadsheet models: read or create a workbook, change inputs,
//! calculate outputs, and save the results.
//!
//! Use this workflow for workbooks your application already knows, including models
//! you or an AI generate. Validate the formulas across the input cases you expect.
//!
//! [`Workbook::open`] reads XLS or XLSX files. You can also create an energy model in memory.
//! `A1` holds operating hours, `B1` power in kW, and `C1` energy use in kWh:
//!
//! ```rust
//! use poirot_workbook::Workbook;
//! use poirot_workbook::model::CellValue;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut book = Workbook::new();
//!     let sheet = book.add_sheet("Energy")?;
//!     sheet.set_value("A1".parse()?, 12.)?;
//!     sheet.set_value("B1".parse()?, 25.)?;
//!     sheet.set_formula("C1".parse()?, "A1*B1")?;
//!     book.recalculate()?;
//!
//!     let energy = book.cell("Energy", "C1".parse()?).ok_or("missing energy result")?;
//!     assert_eq!(energy.value, CellValue::Number(300.));
//!     let bytes = book.save_xlsx().to_buffer()?;
//!     Ok(())
//! }
//! ```
//!
//! Reading a cell returns its stored value. After changing inputs, call `recalculate`
//! to update dependent cells. Saving writes the current values without calculating.
//! If calculation fails, it writes none of its results; earlier input edits remain.
//!
//! Use `from_bytes` to load a buffer. Both `save_xlsx` and `save_xls` let you choose
//! between returning bytes and writing a file. Import warnings are kept in
//! `file_metadata`.
//!
//! For temporary inputs, use [`Evaluation`] to query results without editing the
//! workbook. For repeated scenarios, [`Overlay`] tracks dependencies to reuse
//! unaffected results. Both retain edits between requests and leave the baseline
//! unchanged until you explicitly apply changes. Overlays also retain intermediate
//! results across calculations. Native snapshots save Poirot state for later
//! restoration; they are separate from Excel files.
//!
//! Review import warnings before relying on a saved copy. See the
//! [compatibility guide](https://github.com/yiblet/poirot/blob/master/docs/compatibility.md)
//! for formula and file support, known gaps, and reporting issues.
//!
//! Most applications should depend on [poirot](https://docs.rs/poirot), which
//! re-exports these types alongside cell values and addresses.

pub use poirot_eval as eval;
pub use poirot_io as io;
pub use poirot_model as model;
pub use poirot_parsing as parsing;
pub use poirot_xls as xls;

/// Formula-aware structural edit planning and atomic application.
pub mod edit;
// File handling and recalculation implementation; its types are re-exported below.
mod public;
/// Source-checked translation of references within existing formulas.
pub mod translation_edit;

pub use eval::overlay::{
    CompilationUsage, EvaluationBuilder, EvaluationContext, OverlayBuilder, OverlayError,
    OverlayEvaluation, OverlayPolicy,
};
pub use model::WorkbookChange;
pub use public::{
    CalculationError, LoadBuilder, SavePlan, Workbook, WorkbookError, XlsSaveBuilder,
    XlsxSaveBuilder,
};

pub use eval::overlay::snapshot::Snapshot as OverlaySnapshot;
pub use eval::overlay::snapshot::WorkspaceState;
pub use eval::{CellEvaluation, EvaluationFailure, EvaluationTarget, ProducerPolicy};
pub use model::snapshot::Snapshot as WorkbookSnapshot;

/// Incremental scenarios borrowing a public workbook by default.
pub type Overlay<'a, W = &'a Workbook> = eval::overlay::Overlay<'a, W>;
/// Direct evaluations borrowing a public workbook by default.
pub type Evaluation<'a, W = &'a Workbook> = eval::overlay::Evaluation<'a, W>;
