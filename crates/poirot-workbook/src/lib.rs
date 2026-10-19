//! Workbook editing, evaluation contexts and explicit save planning for poirot.

/// Formula compilation and bounded overlay execution APIs.
pub use poirot_eval as eval;
/// Checked file import and XLSX export adapters.
pub use poirot_io as io;
/// Canonical sparse workbook source and typed values.
pub use poirot_model as model;
/// Formula scanning/full parsing and shared spreadsheet text grammars.
pub use poirot_parsing as parsing;
/// Checked BIFF/XLS export planning and serialization.
pub use poirot_xls as xls;

/// Formula-aware structural edit planning and atomic application.
pub mod edit;
/// Editable workbooks, incremental calculation and borrowed save plans.
pub mod public;
/// Source-checked translation of references within existing formulas.
pub mod translation_edit;

/// Primary mutable overlay and bounded evaluation workflow.
pub use eval::overlay::{
    CompilationUsage, Evaluation, EvaluationBuilder, EvaluationContext, Overlay, OverlayBuilder,
    OverlayError, OverlayEvaluation, OverlayPolicy,
};
/// Source workbook and owned changes that can be applied atomically.
pub use model::{Workbook, WorkbookChange};
/// File-backed source facade and borrowed checked export plans.
pub use public::{SavePlan, Spreadsheet, WorkbookError, XlsSaveBuilder, XlsxSaveBuilder};

/// Serde view retaining overlay source, compiled programs, and incremental results.
pub use eval::overlay::snapshot::Snapshot as OverlaySnapshot;
/// Detached source, compiled formulas, and optional incremental results awaiting attachment.
pub use eval::overlay::snapshot::WorkspaceState;
/// Public output coordinates, values, and operational evaluation failures.
pub use eval::{CellEvaluation, EvaluationFailure, EvaluationTarget};
/// Serde view of canonical workbook source and saved cache metadata.
pub use model::snapshot::Snapshot as WorkbookSnapshot;
