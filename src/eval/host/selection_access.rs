//! Selected-value access for selecting consumers such as IF, CHOOSE and elementwise
//! broadcasting: arguments transferred with their value channel, selected rectangles and
//! call-local branch masks.
use super::call_host::CallHost;
use super::selected_reader::SelectedReader;
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationFailure};
use crate::eval::observe;
use crate::eval::session::{Coercer, ObservationRecorder};
use crate::eval::values::{FunctionResult, Value};
use crate::eval::vm::{BranchSelector, RectangularTraversal, Selection, Shape};
use crate::model::CellValue;

/// Transfers selected arguments without scalarizing them, reads and builds selected
/// rectangles, and admits, expands and inspects per-member branch masks.
pub(in crate::eval) struct SelectionAccess<'h, 'call, 'workbook> {
    /// The call whose arguments, budget and session serve the selections.
    host: &'h mut CallHost<'call, 'workbook>,
}

impl<'h, 'call, 'workbook> SelectionAccess<'h, 'call, 'workbook> {
    /// Borrow the call host for this executor's selections.
    pub(in crate::eval) fn new(host: &'h mut CallHost<'call, 'workbook>) -> Self {
        Self { host }
    }

    /// Admit a call-local selection vector with the caller's source diagnostic.
    pub(in crate::eval) fn buffer(
        &mut self,
        count: usize,
        fill: Selection,
        allocation_detail: &'static str,
    ) -> EvalResult<Vec<Selection>> {
        self.host.session.state.runtime.budget.allocation(count)?;
        let mut selected = Vec::new();
        selected
            .try_reserve_exact(count)
            .map_err(|_| EvaluationFailure::Resource {
                kind: crate::eval::limits::ResourceFailureKind::Allocation,
                capability: CapabilityId::FunctionAdapter,
                source: self.host.session.state.runtime.stack.consumer,
                detail: allocation_detail.into(),
            })?;
        // SAFETY: Reservation admitted exactly count slots; resize fills no more than count.
        selected.resize(count, fill);
        Ok(selected)
    }

    /// Expand selected coordinates with the evaluator's checked broadcast logic.
    pub(in crate::eval) fn expand(
        &mut self,
        selected: Vec<Selection>,
        previous: Shape,
        output: Shape,
    ) -> EvalResult<Vec<Selection>> {
        BranchSelector::new(self.host.session).expand(selected, previous, output)
    }

    /// Inspect a selected rectangle or return the evaluator's spill diagnostic.
    pub(in crate::eval) fn shape(&self, value: &Value, detail: &'static str) -> EvalResult<Shape> {
        Shape::inspect(value).ok_or_else(|| {
            crate::eval::spill::unsupported(self.host.sheet, self.host.address, detail)
        })
    }

    /// Transfer one selected argument with its observation identity and original value channel.
    /// Callers retain the returned value and invoke this once per selected ordinal.
    pub(in crate::eval) fn argument(
        &mut self,
        index: usize,
    ) -> EvalResult<(Option<observe::OperationId>, Value)> {
        let expression = self
            .host
            .arguments
            .get(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        ObservationRecorder::new(self.host.session).observed_value_argument(
            self.host.sheet,
            self.host.address,
            expression,
            index,
        )
    }

    /// Inspect whether a selected expression is syntactically omitted without forcing it.
    pub(in crate::eval) fn is_missing(&self, index: usize) -> bool {
        self.host
            .arguments
            .get(index)
            .is_some_and(|arg| arg.is_missing())
    }

    /// Inspect whether a selected value retains array selection semantics.
    pub(in crate::eval) fn retains_array(&self, value: &Value) -> bool {
        self.host
            .session
            .state
            .expression_demand
            .retains_array(value)
    }

    /// Coerce a selected scalar with the evaluator's exact text and locale charges.
    pub(in crate::eval) fn number(&mut self, value: &CellValue) -> FunctionResult<f64> {
        Coercer::new(self.host.session).coerce_scalar_number(value)
    }

    /// Read one selected rectangular member using the evaluator's broadcast policy.
    pub(in crate::eval) fn member(
        &mut self,
        value: &Value,
        shape: Shape,
        row: usize,
        column: usize,
    ) -> EvalResult<CellValue> {
        RectangularTraversal::new(self.host.session).element(value, shape, row, column)
    }

    /// Construct a selected rectangle with existing output admission and lazy member reads.
    pub(in crate::eval) fn array(
        &mut self,
        shape: Shape,
        mut member: impl FnMut(&mut SelectedReader<'_, '_>, usize, usize) -> EvalResult<CellValue>,
    ) -> EvalResult<Value> {
        RectangularTraversal::new(self.host.session).values(
            self.host.address,
            shape,
            |session, row, column| member(&mut SelectedReader { session }, row, column),
        )
    }

    /// Resolve selected rectangular branches through the evaluator's checked selection path.
    pub(in crate::eval) fn array_results(
        &mut self,
        selected: Vec<Selection>,
        output: Shape,
        result_ordinals: impl Iterator<Item = usize>,
    ) -> EvalResult<Value> {
        BranchSelector::new(self.host.session).selected_array_results(
            self.host.sheet,
            self.host.address,
            self.host.arguments,
            selected,
            output,
            result_ordinals,
        )
    }
}
