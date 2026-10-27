//! Borrowed completion state for delivering a finished cell read.

use crate::eval::config::ArrayResultPolicy;
use crate::eval::coordinates::{CellSet, EvaluationTarget};
use crate::eval::limits::BudgetMeter;
use crate::eval::ports::CellObserver;
use crate::eval::spill;

/// Delivers a finished cell read to dependency and completion observers.
/// It can defer notifications while spill ownership settles, without retaining
/// access to source lookup or formula execution.
pub(super) struct CompletionContext<'s, 'observer> {
    pub(super) array_result_policy: ArrayResultPolicy,
    pub(super) settling_spills: bool,
    pub(super) preparation: &'s mut spill::SpillPreparation,
    pub(super) budget: &'s mut BudgetMeter,
    pub(super) visiting: &'s CellSet,
    pub(super) reads: &'s mut Option<crate::eval::reuse::Reads>,
    pub(super) consumer: Option<EvaluationTarget>,
    pub(super) observer: &'s mut Option<&'observer mut dyn CellObserver>,
}
