//! Native source and calculation-state persistence with explicit recovery policies.
//!
//! Excel interchange uses [`crate::Workbook::save_xlsx`]. Native snapshots also
//! retain compatible prepared calculation state when requested.
//!
//! ```
//! use poirot::{Workbook, snapshots::PreparedPolicy};
//! let mut book = Workbook::new();
//! book.add_sheet("Data")?.set_value("A1".parse()?, 42.)?;
//! let bytes = book.save_snapshot().to_buffer()?;
//! let restored = Workbook::restore_snapshot()
//!     .prepared_policy(PreparedPolicy::SourceOnly)
//!     .from_bytes(&bytes)?;
//! let mut evaluation = restored.into_evaluation(Default::default())?.evaluation;
//! let target = evaluation.target("Data!A1")?;
//! assert_eq!(evaluation.cell(target)?, poirot::CellValue::Number(42.));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
pub use crate::eval::artifact::{
    PreparedPolicy, PreparedStatus, SnapshotError, SnapshotLimits, SnapshotQuotaFailure,
    SnapshotResource, SnapshotSaveBuilder,
};
pub use crate::workflow::{
    OverlaySnapshot, RestoreBuilder, RestoredEvaluation, RestoredWorkbook, WorkbookSnapshot,
    WorkspaceState,
};
