use super::dependencies::Dependencies;
use super::validity::Validity;
use crate::model::CellValue;
use std::sync::Arc;

/// One retained result and the dependencies needed to validate its reuse.
pub(in crate::eval) struct Cached {
    /// Shared result storage, independent of the evaluation that produced it.
    pub value: Arc<CellValue>,
    /// Last publication at which the observable value changed.
    pub version: u64,
    /// Whether retained evidence is current, forced to execute, or awaiting direct causes.
    pub validity: Validity,
    /// Consumed reads, context flags, and spill ownership for this result.
    pub dependencies: Box<Dependencies>,
}

impl Cached {
    /// Estimate result and dependency storage for retention accounting.
    pub fn retained_bytes(&self) -> usize {
        224usize
            .saturating_add(self.validity.retained_bytes())
            .saturating_add(self.value.text_storage_bytes())
            .saturating_add(self.dependencies.cells.len().saturating_mul(48))
            .saturating_add(
                (self.dependencies.ranges.len()
                    + self.dependencies.point_runs.len()
                    + self.dependencies.spill_ranges.len()
                    + self.dependencies.value_reads.len())
                .saturating_mul(64),
            )
    }
}
