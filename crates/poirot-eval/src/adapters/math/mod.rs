//! Deferred math-family execution through the calculation-owned host.
use crate::adapters::failure::{operational_only, project_host};
use crate::adapters::math::policy::{
    self as helper, EvaluationPort, Failure, Origin, ResolvedScalar, Scalar,
};
use crate::host::{CallHost, MissingPolicy, NumericPolicy, OperandOrigin};
use crate::*;
use poirot_functions_core::Input;

fn port_failure(failure: FunctionFailure) -> Failure<EvaluationFailure> {
    project_host(failure, Failure::Spreadsheet, Failure::Operational)
}

impl EvaluationPort for CallHost<'_, '_> {
    /// Deferred argument identity supplied to the family's host port.
    type Argument = usize;
    /// Host abort channel kept outside catchable spreadsheet values.
    type Operational = EvaluationFailure;
    fn resolve_scalar(
        &mut self,
        index: &usize,
    ) -> Result<ResolvedScalar, Failure<EvaluationFailure>> {
        let scalar = self
            .input(*index, MissingPolicy::Blank)
            .force()
            .map_err(port_failure)?;
        let origin = match scalar.origin {
            OperandOrigin::Direct => Origin::Direct,
            OperandOrigin::ArrayElement => Origin::Array,
            OperandOrigin::ReferencedCell => Origin::Reference,
        };
        let value = if scalar.missing {
            Scalar::Missing
        } else {
            match scalar.value {
                Number(n) => Scalar::Number(n),
                Boolean(b) => Scalar::Boolean(b),
                Text(text) => Scalar::Text(text),
                CellValue::TextUtf16(text) => Scalar::Text(
                    self.project_text(poirot_model::TextRef::Utf16(text.units()))
                        .map_err(port_failure)?,
                ),
                Blank => Scalar::Blank,
                Error(error) => Scalar::Error(error),
            }
        };
        Ok(ResolvedScalar { value, origin })
    }
    fn random_unit(&mut self) -> Result<f64, Failure<EvaluationFailure>> {
        self.random_draw(None).map_err(port_failure)
    }
    fn random_integer(&mut self, bottom: f64, top: f64) -> Result<f64, Failure<EvaluationFailure>> {
        let interval = crate::random::Interval::new(bottom, top).ok_or(
            Failure::UnsupportedCapability(helper::UnsupportedCapability::RandomIntegerInterval),
        )?;
        self.random_draw(Some(interval)).map_err(port_failure)
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
    fn coercev2_text_number(&mut self, text: &str) -> Result<f64, Failure<EvaluationFailure>> {
        self.coercev2_number_text(text, NumericPolicy::PinnedPoi)
            .map_err(port_failure)
    }
}

/// Bridge lazy mathematical helper execution to shared coercion and random streams.
pub(crate) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: helper::MathFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    // Arity failure cannot consume argument evaluation, allocation, or randomness.
    if !function.accepts(arguments.len()) {
        return Ok(err(VALUE_ERROR));
    }
    let policy = if session.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation {
        helper::SemanticsPolicy::ExcelDocumentation
    } else {
        helper::SemanticsPolicy::PinnedPoi
    };
    let mut host = CallHost::new(session, sheet, address, arguments);
    host.check_values(arguments.len())
        .map_err(operational_only)?;
    CallHost::charge_work(&mut host, arguments.len() as u64).map_err(operational_only)?;
    let indices: poirot_functions_core::ParamVec<_> = (0..arguments.len()).collect();
    match helper::evaluate_with_policy(&mut host, function, &indices, policy) {
        Ok(number) => {
            host.check_values(1).map_err(operational_only)?;
            Ok(numeric_outcome::coerce_numeric_outcome(number).into())
        }
        Err(Failure::Spreadsheet(error)) => Ok(err(error)),
        Err(Failure::Operational(failure)) => Err(failure),
        Err(Failure::UnsupportedCapability(capability)) => Err(operational_only(host.unsupported(
            CapabilityId::FunctionAdapter,
            format!("math {function:?} requires {capability:?}"),
        ))),
    }
}

/// Lazy mathematical operation contracts and compatibility rules.
pub mod policy;

/// Evaluator-owned mathematical builtin dispatch.
pub(crate) mod execution;

/// General positional radix execution.
pub mod radix;

mod series;
