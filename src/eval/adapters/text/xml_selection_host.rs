use crate::eval::host::CallHost;
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationError};
use crate::eval::values::FunctionError;
use crate::model::CellError;
use poirot_functions::text::xml::{Host, XmlQueryError};

/// Connect XML budget checks and query failures to the evaluator.
pub(super) struct XmlSelectionHost<'a, 'b, 'c>(
    /// Host used to check budgets and report evaluation failures.
    pub(super) &'a mut CallHost<'b, 'c>,
);

impl XmlSelectionHost<'_, '_, '_> {
    /// Map invalid queries to cell errors and preserve resource or capability
    /// refusals as operational failures that IFERROR cannot catch.
    pub(super) fn failure(&self, error: XmlQueryError<EvaluationError>) -> FunctionError {
        match error {
            XmlQueryError::Invalid | XmlQueryError::XPathLength | XmlQueryError::TextLimit => {
                CellError::Value.into()
            }
            XmlQueryError::Host(error) => error.into(),
            error @ XmlQueryError::Allocation => self
                .0
                .resource_failure(
                    crate::eval::limits::ResourceFailureKind::Allocation,
                    crate::eval::limits::EvaluationReasonError::Xml(std::sync::Arc::new(error)),
                )
                .into(),
            error @ XmlQueryError::ResourceLimit => self
                .0
                .resource_failure(
                    crate::eval::limits::ResourceFailureKind::BackendGuard,
                    crate::eval::limits::EvaluationReasonError::Xml(std::sync::Arc::new(error)),
                )
                .into(),
            error @ (XmlQueryError::UnsupportedXPath
            | XmlQueryError::DocumentType
            | XmlQueryError::NamespaceRebinding) => self.0.unsupported(
                CapabilityId::FunctionAdapter,
                crate::eval::limits::EvaluationReasonError::Xml(std::sync::Arc::new(error)),
            ),
        }
    }
}

impl Host for XmlSelectionHost<'_, '_, '_> {
    type Error = EvaluationError;

    fn work(&mut self, units: u64) -> EvalResult<()> {
        self.0.charge_work(units)
    }

    fn text(&mut self, bytes: usize) -> EvalResult<()> {
        self.0.check_text(bytes)
    }

    fn values(&mut self, count: usize) -> EvalResult<()> {
        self.0.check_values(count)
    }
}
