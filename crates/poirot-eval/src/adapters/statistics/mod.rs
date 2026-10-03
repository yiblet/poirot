//! Deferred statistical adapter over the budgeted Session host.
use crate::adapters::failure::{operational_only, project_host};
use crate::adapters::statistics::policy::{
    self as adapter, EvaluationPort, Failure, Origin, Scalar,
};
use crate::host::{CallHost, MissingPolicy, NumericPolicy, OperandKind, OperandOrigin};
use crate::*;
use poirot_functions_core::Input;

fn failure(failure: FunctionFailure) -> Failure<EvaluationFailure> {
    project_host(failure, Failure::Spreadsheet, Failure::Operational)
}

fn scalar(
    host: &mut CallHost<'_, '_>,
    value: crate::host::ScalarOperand,
) -> FunctionResult<Scalar> {
    if value.missing {
        return Ok(Scalar::Missing);
    }
    Ok(match value.value {
        Number(number) => Scalar::Number(number),
        Boolean(boolean) => Scalar::Boolean(boolean),
        Text(text) => Scalar::Text(text),
        CellValue::TextUtf16(text) => {
            Scalar::Text(host.project_text(poirot_model::TextRef::Utf16(text.units()))?)
        }
        Blank => Scalar::Blank,
        Error(error) => Scalar::Error(error),
    })
}
impl EvaluationPort for CallHost<'_, '_> {
    fn pinned_numeric_outcomes(&self) -> bool {
        self.numeric_outcome_policy() == NumericOutcomePolicy::PinnedPoi7321209
    }
    /// Deferred argument identity supplied to the family's host port.
    type Argument = usize;
    /// Host abort channel kept outside catchable spreadsheet values.
    type Operational = EvaluationFailure;
    fn resolve_scalar(&mut self, index: &usize) -> Result<Scalar, Failure<EvaluationFailure>> {
        let value = self
            .input(*index, MissingPolicy::Blank)
            .force()
            .map_err(failure)?;
        scalar(self, value).map_err(failure)
    }
    fn collection_len(&mut self, index: &usize) -> Result<usize, Failure<EvaluationFailure>> {
        CallHost::collection_len(self, *index).map_err(failure)
    }
    fn collection_cell(
        &mut self,
        index: &usize,
        offset: usize,
    ) -> Result<(Scalar, Origin), Failure<EvaluationFailure>> {
        let value = CallHost::collection_cell(self, *index, offset).map_err(failure)?;
        let origin = match value.origin {
            OperandOrigin::Direct => Origin::Direct,
            OperandOrigin::ArrayElement => Origin::Array,
            OperandOrigin::ReferencedCell => Origin::Reference,
        };
        Ok((scalar(self, value).map_err(failure)?, origin))
    }
    fn charge_work(&mut self, units: u64) -> Result<(), EvaluationFailure> {
        CallHost::charge_work(self, units).map_err(operational_only)
    }
    fn check_collection(&mut self, elements: usize) -> Result<(), EvaluationFailure> {
        self.check_values(elements).map_err(operational_only)
    }
    fn check_text(&mut self, bytes: usize) -> Result<(), EvaluationFailure> {
        CallHost::check_text(self, bytes).map_err(operational_only)
    }
    fn coerce_text_number(&mut self, text: &str) -> Result<f64, Failure<EvaluationFailure>> {
        self.coerce_number_text(text, NumericPolicy::PinnedPoi)
            .map_err(failure)
    }
    fn is_reference(&mut self, index: &usize) -> Result<bool, Failure<EvaluationFailure>> {
        self.kind(*index)
            .map(|kind| kind == OperandKind::Reference)
            .map_err(failure)
    }
    fn is_array(&mut self, index: &usize) -> Result<bool, Failure<EvaluationFailure>> {
        self.kind(*index)
            .map(|kind| kind == OperandKind::Array)
            .map_err(failure)
    }
    fn is_area(&mut self, index: &usize) -> Result<bool, Failure<EvaluationFailure>> {
        CallHost::is_area(self, *index).map_err(failure)
    }
}

/// Bridge statistical helper execution to the shared lazy call host.
pub(crate) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: adapter::StatisticsFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let policy = if session.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation {
        adapter::SemanticsPolicy::ExcelDocumentation
    } else {
        adapter::SemanticsPolicy::PinnedPoi
    };
    // Reject arity before allocating the adapter's index vector or evaluating args.
    if !function.accepts_with_policy(arguments.len(), policy) {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new(session, sheet, address, arguments);
    host.check_values(arguments.len())
        .map_err(operational_only)?;
    let indices: poirot_functions_core::ParamVec<_> = (0..arguments.len()).collect();
    match adapter::evaluate_with_policy(&mut host, function, &indices, policy) {
        Ok(number) => {
            use adapter::StatisticsFunction::*;
            let policy = host.numeric_outcome_policy();
            let value = if matches!(
                function,
                NormInv
                    | NormInvLegacy
                    | NormSInv
                    | NormSInvLegacy
                    | NormDist
                    | NormDistLegacy
                    | NormSDist
                    | NormSDistLegacy
            ) {
                policy.at(
                    numeric_outcome::NumericConstruction::NumberEvalResult,
                    number,
                )
            } else {
                policy.checked_result(number)
            };
            Ok(value.into())
        }
        Err(Failure::KnownSourceException(exception)) => {
            Err(host.known_source_exception(exception))
        }
        Err(Failure::Spreadsheet(error)) => Ok(err(error)),
        Err(Failure::Operational(error)) => Err(error),
        Err(Failure::UnsupportedCapability(detail)) => {
            Err(host.capability_failure(CapabilityId::FunctionAdapter, detail))
        }
    }
}

/// Lazy statistical operation contracts and compatibility rules.
pub mod policy;

/// Evaluator-owned statistical and aggregation dispatch.
pub(crate) mod execution;
