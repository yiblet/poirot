#![doc = include_str!("../README.md")]

/// Versioned snapshot files, bounded decoding, and recoverable prepared state.
pub mod artifact;

/// Formula compilation, overlay evaluation, and bounded execution contracts.
pub use poirot_eval as eval;
/// Validated numeric input tokens, independent of value display formatting.
pub use poirot_eval::NumberLocaleV2;
/// Pure compilation and rendering of Excel number/date formats.
pub use poirot_format as format;
/// Checked XLS/XLSX import and XLSX export with fidelity diagnostics.
pub use poirot_io as io;
/// Canonical sparse workbook source, typed values, and owned changes.
pub use poirot_model as model;
/// Formula scanning/full parsing and shared number, criteria, and date/time grammars.
pub use poirot_parsing as parsing;
/// File-backed spreadsheet facade and formula-aware source editing.
pub use poirot_workbook as workbook;
/// Checked BIFF/XLS formula planning and workbook serialization.
pub use poirot_xls as xls;

/// Common model types exposed directly for source construction and mutation.
pub use poirot_model::{
    Cell, CellAddress, CellError, CellStyle, CellValue, DateSystem, ExcelText, TextRef, TextUnits,
    TextUnitsError, Workbook, WorkbookChange, Worksheet,
};
/// Primary overlay/evaluation workflow, snapshot views, and checked file operations.
pub use poirot_workbook::{
    CellEvaluation, CompilationUsage, Evaluation, EvaluationBuilder, EvaluationContext,
    EvaluationFailure, EvaluationTarget, Overlay, OverlayBuilder, OverlayError, OverlayEvaluation,
    OverlayPolicy, OverlaySnapshot, SavePlan, Spreadsheet, WorkbookError, WorkbookSnapshot,
    WorkspaceState, edit, public, translation_edit,
};
