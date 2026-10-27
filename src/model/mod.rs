//! Store and edit workbook data independently of file handling and calculation.
//!
//! Use this module as the data layer when you want to supply your own evaluator or
//! file adapters, or build a workbook model programmatically.
//!
//! [`Workbook`](crate::model::Workbook) holds sparse worksheets, cell values, formulas, named formulas, and
//! a date system. [`Worksheet`] provides cell access through zero-based [`CellAddress`]
//! coordinates.
//!
//! A formula cell stores both its formula and a saved result. Changing workbook data
//! does not calculate a new result. This module stores data without running file codecs or the evaluator;
//! use [poirot](https://docs.rs/poirot) for those workflows.
//!
//! Each formula cell owns its complete source. Optional [`SharedFormula`](crate::model::SharedFormula)
//! provenance can describe an XLSX group; its original anchor is not a live
//! dependency. Source setters detach membership, while cache/style edits preserve
//! it. Executable templates belong to the evaluator, independently of these cells.
//!
//! ```rust
//! use poirot::model::{CellValue, Workbook};
//!
//! let mut book = Workbook::new();
//! book.add_sheet("Sales")?.set_value("B2".parse()?, 125.)?;
//! book.define_name("TaxRate", "0.08", None)?;
//! let sheet = book.sheet_by_name_mut("sales").ok_or("missing sheet")?;
//! sheet.set_formula_with_cached_value("C2".parse()?, "B2*TaxRate", CellValue::Number(10.))?;
//! assert_eq!(sheet.get("C2".parse()?).ok_or("missing result")?.value, CellValue::Number(10.));
//! // Source edits preserve the saved result until an evaluator publishes a new one.
//! sheet.set_value("B2".parse()?, 200.)?;
//! assert_eq!(sheet.get("C2".parse()?).ok_or("missing result")?.value, CellValue::Number(10.));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
mod formula_types;
use poirot_io::styles;
pub use styles::{
    Alignment, BorderLine, BorderSide, Borders, ColorSource, FontFamily, FontScheme, FontSize,
    FontStyle, HorizontalAlignment, IndexedColor, PatternFill, PatternType, ReadingOrder,
    RotationDegrees, StyleColor, StyleContext, TextRotation, ThemeColor, Tint, Underline,
    VerticalAlignment, VisualStyle,
};
// Possible formula-result vocabulary, independent of parsing and execution.
pub use formula_types::{FormulaType, ScalarKinds, ShapeBounds};
use poirot_io::numeric_fidelity;
// Numeric file-representation checks shared by import and export adapters.
pub use numeric_fidelity::{
    FiniteNumericRepresentationPolicy, NumericFidelityReason, NumericFidelityRefusal,
    NumericFileFormat, NumericLocation,
};
use poirot_io::numeric_bits;

// Exact binary64 identity and pinned source exceptions retained across evaluation boundaries.
pub use numeric_bits::{CommonsConvergence, IeeeNumberBits, IeeeNumberClass, KnownSourceException};
// Shared scalar, exact-text, and error values used by source cells and formula kernels.
pub use poirot_functions::scalar::{
    CellError, CellValue, ExcelText, TextRef, TextUnits, TextUnitsError, ValueError as ModelError,
};
mod cell_pages;
// Generic sparse storage and ordered views used by workbook and overlay grids.
pub use cell_pages::{CellRange, CellStore, ColumnCells, OccupiedRows, WorksheetCells};
mod rows;
// Borrowed row access and explicit visibility metadata.
pub use poirot_io::RowVisibility;
pub use rows::{Row, RowMut};
mod changes;
/// Inspection of the source data copied when cloning a workbook.
pub mod clone_footprint;
mod edit;
/// Owned source/cache mutations applied atomically to a workbook.
pub use changes::WorkbookChange;
/// Checked case-insensitive comparison for worksheet and defined-name identities.
pub use poirot_io::identifiers;
/// Bounded sparse query views that distinguish stored blanks from absent coordinates.
pub mod query;
pub use edit::{EditOutcome, FormulaSource};
pub(crate) use edit::{FormulaReplacement, RenameSheetPlan};

use poirot_io::address;
mod cell;
use poirot_io::file_metadata;
mod workbook;
mod worksheet;

pub use address::{CellAddress, MAX_COLUMNS, MAX_ROWS};
pub use cell::CellRead;
use cell::normalize_formula;
pub use file_metadata::{FileFormat, FileMetadata, ImportDiagnostic, ImportDiagnosticKind};
pub use poirot_functions::scalar::DateSystem;
pub use poirot_io::NamedRange;
pub use poirot_io::{Cell, CellStyle, SharedFormula, SharedFormulaId};
pub use poirot_io::{SheetPresentation, SheetVisibility};
pub use workbook::Workbook;
pub use worksheet::Worksheet;
pub(in crate::model) use worksheet::{SavedSpillExtent, SpillCell};

/// Serialization of workbook data and stored formula results.
pub mod snapshot;
/// Unit tests for workbook, cell storage, style and shared-formula contracts.
#[cfg(test)]
mod tests;
