//! Error-handler (IFERROR, IFNA) observation and fallback evaluation.
use super::call_host::CallHost;
use crate::eval::functions::registry::LogicalFunction;
use crate::eval::limits::{EvalResult, EvaluationError};
use crate::eval::observe;
use crate::eval::session::ObservationRecorder;
use crate::eval::session::ResultProjection;
use crate::eval::values::Value;
use crate::model::CellValue;

/// Observes an error handler's checked value and evaluates its caught fallback.
pub(in crate::eval) struct ErrorHandler<'h, 'call, 'workbook> {
    /// The call whose checked and fallback arguments are observed.
    host: &'h mut CallHost<'call, 'workbook>,
}

impl<'h, 'call, 'workbook> ErrorHandler<'h, 'call, 'workbook> {
    /// Borrow the call host for this executor's observations.
    pub(in crate::eval) fn new(host: &'h mut CallHost<'call, 'workbook>) -> Self {
        Self { host }
    }

    /// Inspect a retained error-handler source for scalar observation evidence.
    pub(in crate::eval) fn inspect_scalar(&mut self, value: &Value) -> EvalResult<CellValue> {
        // Keep the existing noncharging clone for the observation-only projection.
        ResultProjection::new(self.host.session)
            .select_legacy_scalar(self.host.address, value.clone())
    }

    /// Publish an uncaught scalar check under its original source operation.
    pub(in crate::eval) fn mask(
        &mut self,
        function: LogicalFunction,
        source: Option<observe::OperationId>,
        checked: &CellValue,
    ) -> EvalResult<()> {
        ObservationRecorder::new(self.host.session)
            .observe_mask(function, source, checked, None, None)
    }

    /// Evaluate the caught handler's fallback with its actual observation identity.
    pub(in crate::eval) fn fallback(
        &mut self,
        function: LogicalFunction,
        source: Option<observe::OperationId>,
        checked: &CellValue,
    ) -> EvalResult<Value> {
        let expression = self
            .host
            .arguments
            .get(1)
            .ok_or(EvaluationError::StalePlan)?;
        ObservationRecorder::new(self.host.session).observed_fallback(
            self.host.sheet,
            self.host.address,
            expression,
            function,
            source,
            checked,
        )
    }
}
