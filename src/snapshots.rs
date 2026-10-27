//! Native source and calculation-state persistence with explicit recovery policies.
//!
//! Excel interchange uses [`crate::Workbook::save_xlsx`]. Native snapshots also
//! retain compatible prepared calculation state when requested.
//!
//! ```
//! use poirot::{Workbook, snapshots::PreparedPolicy};
//! let mut book = Workbook::new();
//! book.add_sheet("Data")?.set_value("A1", 42.)?;
//! let bytes = book.save_snapshot().to_buffer()?;
//! let restored = Workbook::restore_snapshot()
//!     .prepared_policy(PreparedPolicy::SourceOnly)
//!     .from_bytes(&bytes)?;
//! let mut evaluation = restored.into_evaluation(Default::default())?.evaluation;
//! let target = evaluation.target("Data!A1")?;
//! assert_eq!(evaluation.cell(target)?, poirot::CellValue::Number(42.));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use crate::calculation::{Evaluation, EvaluationContext, EvaluationPolicy, Overlay};
use crate::model::Workbook;
use crate::{eval, model};
use std::path::Path;

pub use crate::eval::artifact::{
    PreparedPolicy, PreparedStatus, SnapshotError, SnapshotLimits, SnapshotQuotaError,
    SnapshotResource, SnapshotSaveBuilder,
};
pub use crate::eval::overlay::WorkspaceState;
pub use crate::eval::overlay::snapshot::Snapshot as OverlaySnapshot;
pub use crate::model::snapshot::Snapshot as WorkbookSnapshot;

/// Snapshot restoration with default source recovery and explicit preparation disposition.
///
/// ```
/// use poirot::Workbook;
/// use poirot::eval::artifact::{PreparedPolicy, PreparedStatus};
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_value("A1", 42.)?;
/// let bytes = book.save_snapshot().to_buffer()?;
/// let restored = Workbook::restore_snapshot().from_bytes(&bytes)?;
/// assert_eq!(restored.prepared_status(), PreparedStatus::Absent);
/// assert!(Workbook::restore_snapshot().prepared_policy(PreparedPolicy::RequireCompatible)
///     .from_bytes(&bytes).is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Default)]
pub struct RestoreBuilder {
    inner: eval::artifact::SnapshotLoadBuilder,
}
impl RestoreBuilder {
    /// Replace snapshot input quotas without changing export defaults.
    pub fn limits(mut self, limits: eval::artifact::SnapshotLimits) -> Self {
        self.inner = self.inner.limits(limits);
        self
    }
    /// Choose compatible reuse, strict preparation (also refusing absent state), or source-only recovery.
    pub fn prepared_policy(mut self, policy: eval::artifact::PreparedPolicy) -> Self {
        self.inner = self.inner.prepared_policy(policy);
        self
    }
    /// Reattach shared runtime providers when using restored custom-function preparation.
    pub fn shared_extensions(
        mut self,
        registry: std::sync::Arc<eval::functions::extensions::ExtensionRegistry>,
    ) -> Self {
        self.inner = self.inner.shared_extensions(registry);
        self
    }
    /// Decode native state into a usable restoration object, keeping preparation status visible.
    pub fn from_bytes(
        self,
        bytes: &[u8],
    ) -> Result<RestoredWorkbook, eval::artifact::SnapshotError> {
        Ok(RestoredWorkbook {
            inner: self.inner.from_bytes(bytes)?,
        })
    }
    /// Read native state without evaluating formulas.
    pub fn open(
        self,
        path: impl AsRef<Path>,
    ) -> Result<RestoredWorkbook, eval::artifact::SnapshotError> {
        Ok(RestoredWorkbook {
            inner: self.inner.open(path)?,
        })
    }
}
/// Validated snapshot source and optional preparation, ready to become a calculation.
///
/// ```
/// use poirot::Workbook;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_value("A1", 42.)?;
/// let bytes = book.save_snapshot().to_buffer()?;
/// let restored = Workbook::restore_snapshot().from_bytes(&bytes)?;
/// let run = restored.into_evaluation(Default::default())?;
/// let baseline = run.evaluation.into_workbook();
/// assert_eq!(baseline.stored_cell("Data", "A1")?.ok_or("missing input")?.value,
///     poirot::model::CellValue::Number(42.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct RestoredWorkbook {
    inner: eval::artifact::RestoredArtifact,
}
impl RestoredWorkbook {
    /// Inspect whether optional preparation survived decoding.
    pub fn prepared_status(&self) -> eval::artifact::PreparedStatus {
        self.inner.prepared_status
    }
    /// Restore direct calculation; compatible saved semantics win over fallback defaults.
    /// Source-only recovery uses default semantics. The supplied context starts one new cycle.
    pub fn into_evaluation(
        self,
        context: EvaluationContext,
    ) -> Result<RestoredEvaluation, eval::artifact::SnapshotError> {
        self.into_evaluation_with_policy(context, EvaluationPolicy::default())
    }
    /// Set fallback semantics only when preparation is absent or discarded.
    /// Compatible saved policies are preserved, rather than replaced by this fallback.
    pub fn into_evaluation_with_policy(
        self,
        context: EvaluationContext,
        recovery_policy: EvaluationPolicy,
    ) -> Result<RestoredEvaluation, eval::artifact::SnapshotError> {
        let restored = self.inner.into_evaluation(context, recovery_policy)?;
        Ok(RestoredEvaluation {
            evaluation: Evaluation::from_calculation(restored.evaluation.into_calculation(false)),
            prepared_status: restored.prepared_status,
        })
    }
    /// Recover authored source without promoting calculated results to inputs.
    pub fn into_workbook(self) -> Result<Workbook, eval::artifact::SnapshotError> {
        self.inner.into_workbook()
    }
    /// Restore warm incremental state, or create default state after source-only recovery.
    pub fn into_overlay(
        self,
    ) -> Result<Overlay<'static, model::Workbook>, eval::artifact::SnapshotError> {
        self.inner.into_overlay().map(Overlay::from_overlay)
    }
    /// Recover the advanced artifact API without copying source or prepared state.
    pub fn into_artifact(self) -> eval::artifact::RestoredArtifact {
        self.inner
    }
}
/// A usable owned evaluation together with its explicit preparation disposition.
///
/// ```
/// use poirot::Workbook;
/// use poirot::eval::artifact::PreparedStatus;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_value("A1", 42.)?;
/// let bytes = book.save_snapshot().to_buffer()?;
/// let restored = Workbook::restore_snapshot().from_bytes(&bytes)?;
/// let mut run = restored.into_evaluation(Default::default())?;
/// assert_eq!(run.prepared_status, PreparedStatus::Absent);
/// let input = run.evaluation.target("Data!A1")?;
/// assert_eq!(run.evaluation.cell(input)?, poirot::model::CellValue::Number(42.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct RestoredEvaluation {
    /// Ordinary direct execution, with effective file export.
    pub evaluation: Evaluation<'static, model::Workbook>,
    /// Whether optional preparation was reused, absent, or discarded.
    pub prepared_status: eval::artifact::PreparedStatus,
}
