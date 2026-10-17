//! Deferred pinned-POI engineering execution through the shared Session host.
use crate::adapters::engineering::policy::{
    self as helper, EvaluationPort, Failure, Origin, Output, ResolvedScalar, Scalar,
};
use crate::adapters::failure::{operational_only, project_host};
use crate::host::{CallHost, MissingPolicy, NumericPolicy, OperandOrigin};
use crate::*;
use poirot_functions_core::Input;

fn port_failure(failure: FunctionFailure) -> Failure<EvaluationFailure> {
    project_host(failure, Failure::Spreadsheet, Failure::Operational)
}

/// OperandResolver's decimal grammar deliberately excludes Java hex and suffixes.
fn decimal_text(text: &str) -> Option<f64> {
    let text = text.trim_matches(|c: char| (c as u32) <= 32);
    let bytes = text.as_bytes();
    let mut index = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let mut digits = 0;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
        digits += 1;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if start == index {
            return None;
        }
    }
    if index != bytes.len() {
        return None;
    }
    poirot_numeric::parse_java_number(text).filter(|number| number.is_finite())
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
        CallHost::charge_work(self, units).map_err(operational_only)
    }
    fn check_text(&mut self, bytes: usize) -> Result<(), EvaluationFailure> {
        CallHost::check_text(self, bytes).map_err(operational_only)
    }
    fn coerce_number_to_text(&mut self, number: f64) -> Result<String, Failure<EvaluationFailure>> {
        self.coerce_number_to_text(number, NumericPolicy::PinnedPoi)
            .map_err(port_failure)
    }
    fn coerce_decimal_text(
        &mut self,
        text: &str,
    ) -> Result<Option<f64>, Failure<EvaluationFailure>> {
        CallHost::check_text(self, text.len()).map_err(port_failure)?;
        Ok(decimal_text(text))
    }
    fn coerce_number_text(&mut self, text: &str) -> Result<f64, Failure<EvaluationFailure>> {
        CallHost::coerce_number_text(self, text, NumericPolicy::PinnedPoi).map_err(port_failure)
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
    host.check_values(arguments.len())
        .map_err(operational_only)?;
    let indexes: poirot_functions_core::ParamVec<_> = (0..arguments.len()).collect();
    match helper::evaluate_with_policy(&mut host, function, &indexes, policy) {
        Ok(Output::Number(number)) => Ok(Value::Scalar(Number(number))),
        Ok(Output::Text(text)) => Ok(Value::Scalar(Text(text))),
        Err(Failure::Spreadsheet(error)) => Ok(err(error)),
        Err(Failure::Operational(error)) => Err(error),
        Err(Failure::UnsupportedCapability(capability)) => Err(host.capability_failure(
            CapabilityId::FunctionAdapter,
            format!("engineering coercion requires {capability:?}"),
        )),
    }
}

#[cfg(test)]
mod source_grammar_tests {
    use super::decimal_text;

    #[test]
    fn operand_resolver_decimal_grammar_has_no_java_literal_extensions() {
        // OperandResolver.java fpRegex and parseDouble: ASCII decimal grammar,
        // control-space trimming, finite results, and no date fallback here.
        for text in ["1", "1.", ".5", " +.5e1 ", "\t-0\r", "1e-400"] {
            assert!(decimal_text(text).is_some(), "{text:?}");
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
            assert_eq!(decimal_text(text), None, "{text:?}");
        }
    }
}

/// Lazy engineering operation contracts and compatibility rules.
pub mod policy;
