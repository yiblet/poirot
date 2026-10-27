use crate::eval::compile::compiler::FormulaCompiler;
use crate::eval::config::EvaluationPolicy;
use crate::eval::reuse::Cache;
use crate::model::Workbook;
use std::cell::RefCell;

/// Type-erased borrowed source; snapshots do not depend on the workbook handle type.
pub(super) struct BorrowedSource<'a> {
    pub(super) workbook: &'a Workbook,
    pub(super) inputs: &'a crate::eval::inputs::RunInputs,
    pub(super) compiler: &'a RefCell<FormulaCompiler>,
    pub(super) spill_candidates: &'a RefCell<crate::eval::spill::CandidateIndex>,
    pub(super) policy: &'a EvaluationPolicy,
    pub(super) preparation: &'a crate::eval::compile::compiler::PreparationPolicy,
    pub(super) cache: Option<&'a RefCell<Cache>>,
}
