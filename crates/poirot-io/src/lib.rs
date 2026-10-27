//! Read XLSX and BIFF8 XLS into caller-selected storage, and write fresh XLSX
//! packages from borrowed records. Most applications should use
//! [poirot](https://docs.rs/poirot), which provides these operations with an editable
//! workbook, calculation and scenarios. This crate supplies file records and codecs;
//! it does not calculate formulas.
//!
//! Implement `data::WorkbookSink` and its `data::SheetSink` for streaming import.
//! `WorkbookSink` also supplies the `data::WorkbookSource` interface used for borrowed
//! export. Poirot's sparse workbook implements these contracts.
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
//! | Task | Entry point |
//! | --- | --- |
//! | Detect and import a file or byte buffer | [`read_workbook`], [`read_workbook_bytes`] |
//! | Configure admission, external relationships and quotas | [`ReadOptions`], [`opc::OpcImportPolicy`], [`ResourceBudget`]; [`read_workbook_with_options`], [`read_workbook_bytes_with_options`] |
//! | Export to a file or byte buffer | [`write_xlsx`], [`write_xlsx_to_buffer`] |
//! | Configure output quotas or inspect an export before execution | [`WriteOptions`], [`plan_xlsx_export`] |
//! | Inspect fidelity and resource failures | [`IoError`], [`ImportDiagnostic`], [`IoQuotaError`] |
//! | Supply file records and presentation | [`data`], [`cell`], [`styles`], [`file_metadata`] |
//!
//! ## Preservation and limits
//!
//! Imports retain saved formula values and report unsupported or altered content in
//! workbook metadata. Imported shared formulas become independent complete strings;
//! original group IDs and anchors are not retained. Import does not separately
//! validate malformed shared ranges or duplicate masters. Optional manually supplied
//! shared provenance is checked before export. Saving writes stored results without
//! recalculation and always authors a fresh package; untouched results may be stale.
//!
//! XLSX supports exact Excel UTF-16 text, names, date systems, saved cache kinds,
//! array/spill ownership, sparse rows and ordinary cell presentation: number formats,
//! fonts, pattern fills, four-sided borders and common alignment, including styled
//! blank cells. Immutable `VisualStyle` payloads share presentation; `StyleContext`
//! retains default style, theme bytes and the indexed palette. Colors preserve source
//! identity rather than promising resolved RGB values.
//!
//! Charts, drawings, macros, conditional formatting, gradient fills, diagonal borders,
//! row/column style inheritance and unknown formula attributes are not preserved.
//! Layout and font rendering are outside this crate's scope. BIFF8 XLS import supports
//! sparse records, names, formula tokens and exact strings, but does not recover number
//! formats or visual styles; unsupported formula token families retain saved values
//! and add warnings. BIFF5 and earlier and encrypted workbooks are refused. XLS is
//! input-only. See the
//! [compatibility guide](https://github.com/yiblet/poirot/blob/master/docs/compatibility.md)
//! for the full preservation contract.
//!
//! Read size quotas default to unlimited; structural and format validation still
//! apply. Writing defaults to 32 MiB output, 32 MiB per expanded entry, 128 MiB total
//! expanded entries and 10,000 entries, with no source-payload quota. Options are
//! configuration, not consumed counters. Serialization completes before opening an
//! output file; a filesystem error can still leave a partial file.
//!
//! Calamine (MIT) streams XLSX cells, Poirot decodes BIFF8 XLS, and rust_xlsxwriter
//! (used under MIT) supplies the fresh XLSX package skeleton. Focused XML recovery
//! preserves calculation-visible text, cache and spill distinctions; export projects
//! canonical records into the package. See the package's `LICENSES` directory for
//! upstream notices and provenance.

mod allocation_error;
mod biff_representation_error;
mod biff_resource;
mod biff_structure_error;
mod export_limit_error;
mod identifier_error;
mod io_error;
mod io_operation;
mod io_quota_error;
mod io_resource;
mod model_error;
mod numeric_fidelity_error;
mod opc_error;
mod style_decode_error;
mod workbook_feature_error;
mod workbook_read_error;
mod xlsx_decode_error;
mod xml_codec_error;

mod ooxml;
/// XLSX package identity, relationship, and external-link policy validation.
pub mod opc;

mod contract;
mod decoded_cells;
mod export_execution;
mod export_plan;
mod import_conversion;
mod numeric_representation;
mod xls;

// XLS admission limits and structural/resource diagnostics.
pub use xls::{PreflightError, ResourceBudget};
// Import errors and read/write options.
pub use contract::{ReadOptions, WriteOptions};
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
/// Exact binary64 identities for transport and diagnostics.
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
pub use numeric_bits::{IeeeNumberBits, IeeeNumberClass};
pub use numeric_fidelity::{
    FiniteNumericRepresentationPolicy, NumericFidelityReason, NumericFileFormat, NumericLocation,
};
pub use poirot_functions::scalar::{
    CellError, CellValue, DateSystem, ExcelText, TextRef, TextUnits, TextUnitsError,
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

pub use allocation_error::AllocationError;
pub use biff_representation_error::BiffRepresentationError;
pub use biff_resource::BiffResource;
pub use biff_structure_error::BiffStructureError;
pub use export_limit_error::ExportLimitError;
pub use io_error::IoError;
pub use io_operation::IoOperation;
pub use io_quota_error::IoQuotaError;
pub use io_resource::IoResource;
pub use model_error::ModelError;
pub use numeric_fidelity_error::NumericFidelityError;
pub use style_decode_error::StyleDecodeError;
pub use workbook_feature_error::WorkbookFeatureError;
pub use workbook_read_error::WorkbookReadError;
pub use xml_codec_error::XmlCodecError;
