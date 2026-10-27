use super::prepared_status::PreparedStatus;
use super::restored_checkpoint::RestoredCheckpoint;
use super::restored_evaluation::RestoredEvaluation;
use super::snapshot_error::SnapshotError;
use crate::eval::config::{EvaluationContext, EvaluationPolicy};
use crate::eval::overlay::Evaluation;
use crate::eval::overlay::WorkspaceState;
use crate::model::Workbook;

/// Validated workbook and optional detached workspace, ready for ordinary evaluation.
/// With a workspace, the workbook is its exact baseline; otherwise it is effective authored
/// source including all persisted edits. The pair stays private so the conversions below
/// can attach the workspace without comparing it to its own baseline again.
pub struct RestoredArtifact {
    pub(super) workbook: Workbook,
    pub(super) workspace: Option<WorkspaceState>,
    /// Explicit disposition of optional prepared state.
    pub prepared_status: PreparedStatus,
}

impl RestoredArtifact {
    /// Baseline for the workspace, or effective source when no prepared state was retained.
    pub fn workbook(&self) -> &Workbook {
        &self.workbook
    }

    /// Separate the workbook and the validated compiled/direct or incremental state.
    /// Runtime providers are never stored; attaching the state checks the workbook again.
    pub fn into_parts(self) -> (Workbook, Option<WorkspaceState>) {
        (self.workbook, self.workspace)
    }

    /// Attach reusable preparation, or expose recovered source without choosing new policies.
    pub fn into_checkpoint(self) -> Result<RestoredCheckpoint, SnapshotError> {
        match self.workspace {
            Some(state) => Ok(RestoredCheckpoint::Prepared(Box::new(
                state
                    .attach_restored(self.workbook)
                    .map_err(SnapshotError::Workspace)?,
            ))),
            None => Ok(RestoredCheckpoint::Source {
                workbook: self.workbook,
                status: self.prepared_status,
            }),
        }
    }

    /// Restore direct evaluation and report whether preparation survived decoding.
    pub fn into_evaluation(
        self,
        context: EvaluationContext,
        recovery_policy: EvaluationPolicy,
    ) -> Result<RestoredEvaluation, SnapshotError> {
        let prepared_status = self.prepared_status;
        let evaluation = match self.workspace {
            Some(state) => state
                .into_restored_evaluation(self.workbook, context)
                .map_err(SnapshotError::Workspace)?,
            None => Evaluation::with_workbook(self.workbook)
                .with_policy(recovery_policy)
                .build(context)
                .map_err(SnapshotError::Workspace)?,
        };
        Ok(RestoredEvaluation {
            evaluation,
            prepared_status,
        })
    }
}
