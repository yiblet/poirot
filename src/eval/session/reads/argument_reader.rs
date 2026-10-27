//! Function argument evaluation and matrix materialization.

use crate::eval::limits::{CapabilityId, EvalResult, EvaluationError};
use crate::eval::session;
use crate::eval::session::cells::CellEvaluator;
use crate::eval::session::composition::Session;
use crate::eval::session::observation_recorder::ObservationRecorder;
use crate::eval::session::projection::ResultProjection;
use crate::eval::values::{VALUE_ERROR, Value, convert_legacy_missing_blank};
use crate::eval::vm::{ArgumentsRef, ExpressionRunner};
use crate::model::CellValue::{Blank, Error};
use crate::model::{CellAddress, CellError, CellValue};

/// Evaluates function arguments and materializes matrix operands.
pub(in crate::eval) struct ArgumentReader<'s, 'a> {
    /// The run whose state this executor reads and updates.
    session: &'s mut Session<'a>,
}

impl<'s, 'a> ArgumentReader<'s, 'a> {
    /// Borrow the session for this executor's calls.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Evaluate and scalarize one argument, observing only the requested position.
    pub(in crate::eval) fn argument(
        &mut self,
        s: usize,
        at: CellAddress,
        args: ArgumentsRef<'_>,
        index: usize,
    ) -> EvalResult<CellValue> {
        if self.session.ports.execution_observer.is_none() {
            return match args.get(index) {
                Some(expression) => {
                    let value = ExpressionRunner::new(self.session).expr_with_demand(
                        s,
                        at,
                        expression,
                        session::ExpressionDemand::Scalar,
                    )?;
                    ResultProjection::new(self.session).select_legacy_scalar(at, value)
                }
                None => Ok(Blank),
            };
        }
        ObservationRecorder::new(self.session).observed_scalar_argument(s, at, args, index)
    }

    /// Materialize a supported scalar, array, or single reference as a budgeted matrix.
    pub(in crate::eval) fn matrix(
        &mut self,
        value: Value,
    ) -> EvalResult<Result<Vec<Vec<CellValue>>, CellError>> {
        match value {
            Value::Array(rows) => Ok(Ok(rows.values)),
            Value::Missing => {
                self.session.state.runtime.budget.allocation(1)?;
                Ok(Ok(vec![vec![convert_legacy_missing_blank()]]))
            }
            Value::Scalar(Error(error)) | Value::SelectedScalar(Error(error), _) => Ok(Err(error)),
            Value::Scalar(value) | Value::SelectedScalar(value, _) => {
                self.session.state.runtime.budget.allocation(1)?;
                Ok(Ok(vec![vec![value]]))
            }
            Value::SheetSpan(areas) if areas.len() != 1 => {
                Err(EvaluationError::UnsupportedCapability {
                    capability: CapabilityId::FunctionAdapter,
                    target: None,
                    reason: crate::eval::limits::EvaluationReasonError::MatrixMaterializationOfAMultiSheetReferenceIsNotVerified,
                })
            }
            Value::SignedRefs(areas) => {
                if areas.len() != 1 {
                    return Ok(Err(VALUE_ERROR));
                }
                // SAFETY: The preceding branch rejects areas.len() != 1 before selecting the first area.
                let area = areas[0];
                if area.positive_shape().is_none() {
                    return Err(EvaluationError::UnsupportedCapability { capability: CapabilityId::FunctionAdapter, target: None,
                        reason: crate::eval::limits::EvaluationReasonError::DenseConsumerPolicyForZeroNegativeJavaSourceDimensionsIsNotVerified });
                }
                self.session
                    .state
                    .runtime
                    .budget
                    .allocation_u64(area.size())?;
                let mut rows = Vec::with_capacity(area.height() as usize);
                for row in 0..area.height() {
                    let mut values = Vec::with_capacity(area.width() as usize);
                    for col in 0..area.width() {
                        values.push(CellEvaluator::new(self.session).signed_read(
                            area.read(row, col).ok_or(EvaluationError::StalePlan)?,
                        )?);
                    }
                    rows.push(values);
                }
                Ok(Ok(rows))
            }
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                if areas.len() != 1 {
                    return Ok(Err(VALUE_ERROR));
                }
                // SAFETY: The preceding branch rejects areas.len() != 1 before selecting the first area.
                let area = areas[0];
                self.session
                    .state
                    .runtime
                    .budget
                    .allocation_u64(area.size())?;
                let mut rows = Vec::with_capacity((area.r1 - area.r0 + 1) as usize);
                for row in area.r0..=area.r1 {
                    let mut values = Vec::with_capacity((area.c1 - area.c0 + 1) as usize);
                    for col in area.c0..=area.c1 {
                        values.push(
                            CellEvaluator::new(self.session)
                                .cell(area.sheet, CellAddress { row, col })?,
                        );
                    }
                    rows.push(values);
                }
                Ok(Ok(rows))
            }
        }
    }
}
