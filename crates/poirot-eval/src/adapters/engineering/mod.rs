//! Deferred pinned-POI engineering execution through the shared Session host.
use crate::adapters::engineering::policy::{
    self as helper, EvaluationPort, Failure, Origin, Output, ResolvedScalar, Scalar,
};
use crate::adapters::failure::project_host;
use crate::host::{CallHost, MissingPolicy, NumericPolicy, OperandOrigin};
use crate::*;
use poirot_functions_core::Input;

fn port_failure(failure: FunctionFailure) -> Failure<EvaluationFailure> {
    project_host(failure, Failure::Spreadsheet, Failure::Operational)
}

/// OperandResolver's decimal grammar deliberately excludes Java hex and suffixes.
fn coercev2_engineering_decimal_text(text: &str) -> Option<f64> {
    poirot_coercionv2::compatibility::coercev2_financial_decimal_text(text)
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
                Text(s) => Scalar::Text(s),
                CellValue::TextUtf16(text) => Scalar::Text(
                    self.project_text(poirot_model::TextRef::Utf16(text.units()))
                        .map_err(port_failure)?,
                ),
                Blank => Scalar::Blank,
                Error(e) => Scalar::Error(e),
            }
        };
        Ok(ResolvedScalar { value, origin })
    }
    fn charge_work(&mut self, units: u64) -> Result<(), EvaluationFailure> {
        CallHost::charge_work(self, units)
    }
    fn check_text(&mut self, bytes: usize) -> Result<(), EvaluationFailure> {
        CallHost::check_text(self, bytes)
    }
    fn coercev2_number_to_text(
        &mut self,
        number: f64,
    ) -> Result<String, Failure<EvaluationFailure>> {
        self.coercev2_number_to_text(number, NumericPolicy::PinnedPoi)
            .map_err(port_failure)
    }
    fn coercev2_decimal_text(
        &mut self,
        text: &str,
    ) -> Result<Option<f64>, Failure<EvaluationFailure>> {
        CallHost::check_text(self, text.len()).map_err(Failure::Operational)?;
        Ok(coercev2_engineering_decimal_text(text))
    }
    fn coercev2_number_text(&mut self, text: &str) -> Result<f64, Failure<EvaluationFailure>> {
        CallHost::coercev2_number_text(self, text, NumericPolicy::PinnedPoi).map_err(port_failure)
    }
}

/// Bridge engineering helpers to shared scalar coercion and typed failures.
pub(crate) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: helper::EngineeringFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let policy = if session.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation {
        helper::SemanticsPolicy::ExcelDocumentation
    } else {
        helper::SemanticsPolicy::PinnedPoi
    };
    let mut host = CallHost::new(session, sheet, address, arguments);
    // Arity is semantic and must precede allocation or argument execution.
    if !function.accepts_with_policy(arguments.len(), policy) {
        return Ok(err(VALUE_ERROR));
    }
    host.check_values(arguments.len())?;
    let indexes: poirot_functions_core::ParamVec<_> = (0..arguments.len()).collect();
    match helper::evaluate_with_policy(&mut host, function, &indexes, policy) {
        Ok(Output::Number(number)) => Ok(Value::Scalar(Number(number))),
        Ok(Output::Text(text)) => Ok(Value::Scalar(Text(text))),
        Err(Failure::Spreadsheet(error)) => Ok(err(error)),
        Err(Failure::Operational(error)) => Err(error),
        Err(Failure::UnsupportedCapability(capability)) => Err(host.capability_failure(
            CapabilityId::FunctionAdapter,
            format!("engineering execution requires {capability:?}"),
        )),
    }
}

#[cfg(test)]
mod source_grammar_tests {
    use super::coercev2_engineering_decimal_text;

    #[test]
    fn operand_resolver_decimal_grammar_has_no_java_literal_extensions() {
        // OperandResolver.java fpRegex and parseDouble: ASCII decimal grammar,
        // control-space trimming, finite results, and no date fallback here.
        for text in ["1", "1.", ".5", " +.5e1 ", "\t-0\r", "1e-400"] {
            assert!(
                coercev2_engineering_decimal_text(text).is_some(),
                "{text:?}"
            );
        }
        for text in [
            "",
            " ",
            "١",
            "0x1p0",
            "1d",
            "NaN",
            "Infinity",
            "1e400",
            "1e",
            "2020-01-01",
        ] {
            assert_eq!(coercev2_engineering_decimal_text(text), None, "{text:?}");
        }
    }
}

/// Lazy engineering operation contracts and compatibility rules.
pub mod policy;

/// Documented complex execution and retained source-selected engineering helpers.
mod modern_complex;
/// Signed radix, bitwise and physical measurement execution.
mod modern_radix;
/// Documented bounded real engineering special functions.
mod modern_special;

/// Select the typed modern complex path or the existing family policy adapter.
pub(crate) fn execute_function(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: EngineeringFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if session.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation
        && modern_special::handles(function)
    {
        return modern_special::execute(session, sheet, address, function, arguments);
    }
    match function.helper() {
        Some(helper) => execute(session, sheet, address, helper, arguments),
        None if modern_radix::handles(function) => {
            modern_radix::execute(session, sheet, address, function, arguments)
        }
        None => modern_complex::execute(session, sheet, address, function, arguments),
    }
}
