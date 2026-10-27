//! Read XLS and XLSX workbooks and write XLSX files.
//!
//! This crate decodes directly into a caller-selected [`data::WorkbookSink`] and
//! exports borrowed [`data::WorkbookSource`] records. Poirot implements these
//! contracts for its sparse workbook model. Imports
//! retain saved formula results and record unsupported or altered content in workbook
//! metadata. Writing preserves saved results; it does not calculate formulas.
//!
//! Calamine streams XLSX decoding; rxls supplies XLS decoding and fresh XLSX authoring.
//! Imports project backend cells,
//! number formats, names and presentation directly. Existing focused XML recovery
//! preserves calculation-visible text units, missing caches and array/spill ownership.
//! Backend sheets are released during conversion; original XML recovery then expands
//! shared strings once and one worksheet at a time, without retaining the full archive.
//! Charts, drawings, macros, rich styling and original shared-group encoding are
//! omitted; saving authors a fresh package from the modeled workbook data.
//!
//! Imported shared formulas have independent complete source strings. Optional
//! manually supplied group provenance is verified before sharing on export;
//! singletons, stale metadata and unsupported translations use ordinary formulas.
//!
//! ```rust
//! use poirot_io::{IoError, read_workbook_bytes, write_xlsx_to_buffer};
//! use poirot_io::data::{SheetSink, WorkbookSink};
//!
//! // The caller supplies storage; import writes directly into that storage type.
//! fn roundtrip<W: WorkbookSink<Sheet: SheetSink>>(book: &W) -> Result<W, IoError> {
//!     let bytes = write_xlsx_to_buffer(book)?;
//!     read_workbook_bytes(&bytes)
//! }
//! ```
//!
//! [`read_workbook`] reads a file; [`read_workbook_bytes`] reads an in-memory buffer.
//! Both detect XLS or XLSX from the contents and construct the caller-selected sink.
//! [`ReadOptions`] configures import policies and limits.
//!
//! XLS is supported as input only; saves produce XLSX.
//! Most applications should use [poirot](https://docs.rs/poirot), which provides the
//! same file operations alongside editing and calculation.
/// Checked BIFF fidelity recovery around the backend XLS decoder.
pub mod biff_overlay;
mod biff_preflight;
// Pre-decode BIFF/CFB resource admission and structural diagnostics.
pub use biff_preflight::{BiffPlan, PreflightError, ResourceBudget};
mod ooxml;
/// XLSX package identity, relationship, and external-link policy validation.
pub mod opc;

mod contract;
mod decoded_cells;
mod export_execution;
mod export_plan;
mod import_conversion;
mod numeric_representation;

// Import errors and read/write options.
pub use contract::{IoError, IoQuotaFailure, IoResource, ReadOptions, WriteOptions};
// XLSX serialization to a complete byte buffer before file publication.
pub use export_execution::{
    write_xlsx, write_xlsx_to_buffer, write_xlsx_to_buffer_with_options, write_xlsx_with_options,
};
// Checked immutable XLSX export plans and selected feature/cache contracts.
pub use export_plan::{
    ExportFeatures, ExportFormat, ExportPlan, TypedCachePolicy, plan_xlsx_export,
};
// Signature-aware file and byte import with retained fidelity diagnostics.
pub use import_conversion::{
    read_workbook, read_workbook_bytes, read_workbook_bytes_with_options,
    read_workbook_with_options,
};

/// Checked worksheet addresses and Excel dimensions.
pub mod address;
/// File cell records and formula normalization.
pub mod cell;
/// File provenance and import diagnostics.
pub mod file_metadata;
/// Checked workbook identifier comparison.
pub mod identifiers;
/// Exact numeric identities and source exceptions.
pub mod numeric_bits;
/// File numeric fidelity policies.
pub mod numeric_fidelity;
mod rows_records;
/// Stored presentation records.
pub mod styles;
/// Lossless saved scalar serialization.
pub mod value_wire;
mod workbook_records;
mod worksheet_records;
pub use address::{CellAddress, MAX_COLUMNS, MAX_ROWS};
pub use cell::{Cell, CellStyle, SharedFormula, SharedFormulaId};
pub use file_metadata::{FileFormat, FileMetadata, ImportDiagnostic, ImportDiagnosticKind};
pub use numeric_bits::{CommonsConvergence, IeeeNumberBits, IeeeNumberClass, KnownSourceException};
pub use numeric_fidelity::{
    FiniteNumericRepresentationPolicy, NumericFidelityReason, NumericFidelityRefusal,
    NumericFileFormat, NumericLocation,
};
pub use poirot_functions::scalar::{
    CellError, CellValue, DateSystem, ExcelText, TextRef, TextUnits, TextUnitsError,
    ValueError as ModelError,
};
pub use rows_records::RowVisibility;
pub use styles::{
    Alignment, BorderLine, BorderSide, Borders, ColorSource, FontFamily, FontScheme, FontSize,
    FontStyle, HorizontalAlignment, IndexedColor, PatternFill, PatternType, ReadingOrder,
    RotationDegrees, StyleColor, StyleContext, TextRotation, ThemeColor, Tint, Underline,
    VerticalAlignment, VisualStyle,
};
pub use workbook_records::NamedRange;
pub use worksheet_records::{SheetPresentation, SheetVisibility};

/// Borrowed export and direct streaming import contracts.
pub mod data;

#[cfg(test)]
mod test_support;
