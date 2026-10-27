use super::dependency_read::DependencyRead;
use crate::eval::coordinates::EvaluationTarget;

/// An observed read and its consuming cell, if the read has a cell attribution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadEvent {
    /// Formula cell consuming this read, when execution has a cell attribution.
    pub consumer: Option<EvaluationTarget>,
    /// Dependency or setting actually observed.
    pub read: DependencyRead,
}
