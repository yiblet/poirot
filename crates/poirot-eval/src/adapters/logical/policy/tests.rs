use super::{EvaluationPort, Failure, LogicalFunction, coercev2_logical_test, evaluate};
use poirot_functions_core::{CellError, CellValue};

/// Test outcome distinguishing selected values from operational failures.
#[derive(Debug, Clone, PartialEq)]
enum Output {
    Scalar(CellValue),
    Reference(u32),
}
/// Logical test host recording which deferred operands selection actually evaluates.
struct Port {
    inputs: Vec<Result<Output, Failure<&'static str>>>,
    visits: Vec<(char, usize)>,
    // Supplied by the common numeric policy, already on the Excel view.
    truncated: f64,
    comparison_error: Option<Failure<&'static str>>,
}
impl Port {
    fn new(inputs: Vec<Output>) -> Self {
        Self {
            inputs: inputs.into_iter().map(Ok).collect(),
            visits: vec![],
            truncated: 0.0,
            comparison_error: None,
        }
    }
}
impl EvaluationPort for Port {
    /// Selected result channel preserving lazy value/reference identity.
    type Output = Output;
    /// Host abort channel kept outside catchable spreadsheet values.
    type OperationalError = &'static str;
    fn scalar(&mut self, index: usize) -> Result<CellValue, Failure<&'static str>> {
        self.visits.push(('s', index));
        match self.inputs[index].clone()? {
            Output::Scalar(CellValue::Error(e)) => Err(Failure::Cell(e)),
            Output::Scalar(v) => Ok(v),
            Output::Reference(_) => Ok(CellValue::Number(42.0)),
        }
    }
    fn argument(&mut self, index: usize) -> Result<Output, Failure<&'static str>> {
        self.visits.push(('a', index));
        self.inputs[index].clone()
    }
    fn is_reference(&mut self, index: usize) -> Result<bool, Failure<&'static str>> {
        self.visits.push(('r', index));
        Ok(matches!(self.inputs[index].clone()?, Output::Reference(_)))
    }
    fn coercev2_truncated_number(&mut self, index: usize) -> Result<f64, Failure<&'static str>> {
        self.visits.push(('n', index));
        Ok(self.truncated)
    }
    fn equal(
        &mut self,
        left: &CellValue,
        right: &CellValue,
    ) -> Result<bool, Failure<&'static str>> {
        if let Some(e) = &self.comparison_error {
            return Err(e.clone());
        }
        Ok(match (left, right) {
            (CellValue::Text(a), CellValue::Text(b)) => a.eq_ignore_ascii_case(b),
            _ => left == right,
        })
    }
    fn value(value: CellValue) -> Output {
        Output::Scalar(value)
    }
}
fn number(n: f64) -> Output {
    Output::Scalar(CellValue::Number(n))
}
fn boolean(b: bool) -> Output {
    Output::Scalar(CellValue::Boolean(b))
}
fn error(e: CellError) -> Output {
    Output::Scalar(CellValue::Error(e))
}

#[test]
fn ifs_preserves_reference_and_never_visits_skipped_results_or_later_conditions() {
    let mut port = Port::new(vec![
        boolean(false),
        error(CellError::Div0),
        boolean(true),
        Output::Reference(99),
        error(CellError::Ref),
        number(1.0),
    ]);
    assert_eq!(
        evaluate(LogicalFunction::Ifs, 6, &mut port),
        Ok(Output::Reference(99))
    );
    assert_eq!(port.visits, [('s', 0), ('s', 2), ('a', 3)]);
}
#[test]
fn ifs_shape_and_condition_precedence() {
    let mut port = Port::new(vec![error(CellError::Ref), number(1.0)]);
    assert_eq!(
        evaluate(LogicalFunction::Ifs, 1, &mut port),
        Err(Failure::Cell(CellError::Value))
    );
    assert!(port.visits.is_empty());
    assert_eq!(
        evaluate(LogicalFunction::Ifs, 2, &mut port),
        Err(Failure::Cell(CellError::Ref))
    );
    assert_eq!(
        evaluate(LogicalFunction::Ifs, 0, &mut port),
        Err(Failure::Cell(CellError::NA))
    );
}
#[test]
fn boolean_coercion_keeps_poi_string_and_nan_quirks() {
    assert_eq!(
        coercev2_logical_test(&CellValue::Text("tRuE".into())),
        Ok(true)
    );
    assert_eq!(
        coercev2_logical_test(&CellValue::Text(" TRUE ".into())),
        Err(CellError::Value)
    );
    assert_eq!(
        coercev2_logical_test(&CellValue::Text("1".into())),
        Err(CellError::Value)
    );
    assert_eq!(coercev2_logical_test(&CellValue::Blank), Ok(false));
    assert_eq!(
        coercev2_logical_test(&CellValue::Number(f64::NAN)),
        Err(CellError::Value)
    );
}
#[test]
fn switch_first_match_and_default_are_reference_preserving() {
    let mut port = Port::new(vec![
        number(2.0),
        number(1.0),
        error(CellError::Div0),
        number(2.0),
        Output::Reference(7),
        error(CellError::Ref),
    ]);
    assert_eq!(
        evaluate(LogicalFunction::Switch, 6, &mut port),
        Ok(Output::Reference(7))
    );
    assert_eq!(port.visits, [('s', 0), ('s', 1), ('s', 3), ('a', 4)]);
    let mut port = Port::new(vec![
        number(9.0),
        number(1.0),
        error(CellError::Div0),
        Output::Reference(8),
    ]);
    assert_eq!(
        evaluate(LogicalFunction::Switch, 4, &mut port),
        Ok(Output::Reference(8))
    );
    assert_eq!(port.visits, [('s', 0), ('s', 1), ('a', 3)]);
}
#[test]
fn switch_maps_scalar_cell_errors_but_not_operational_failures() {
    let mut port = Port::new(vec![error(CellError::Div0), number(1.0), number(2.0)]);
    assert_eq!(
        evaluate(LogicalFunction::Switch, 3, &mut port),
        Err(Failure::Cell(CellError::NA))
    );
    port.inputs[0] = Err(Failure::Operational("budget"));
    assert_eq!(
        evaluate(LogicalFunction::Switch, 3, &mut port),
        Err(Failure::Operational("budget"))
    );
    port.inputs[0] = Ok(number(1.0));
    port.inputs[1] = Ok(error(CellError::Value));
    assert_eq!(
        evaluate(LogicalFunction::Switch, 3, &mut port),
        Err(Failure::Cell(CellError::NA))
    );
}
#[test]
fn switch_comparator_cell_error_is_failed_match_but_operational_error_propagates() {
    let mut port = Port::new(vec![number(1.0), number(1.0), number(2.0), number(9.0)]);
    port.comparison_error = Some(Failure::Cell(CellError::Num));
    assert_eq!(
        evaluate(LogicalFunction::Switch, 4, &mut port),
        Ok(number(9.0))
    );
    port.comparison_error = Some(Failure::Operational("cancelled"));
    assert_eq!(
        evaluate(LogicalFunction::Switch, 4, &mut port),
        Err(Failure::Operational("cancelled"))
    );
}
#[test]
fn error_type_catches_only_cell_errors_and_records_unsupported_poi_codes() {
    for (index, e) in [
        CellError::Null,
        CellError::Div0,
        CellError::Value,
        CellError::Ref,
        CellError::Name,
        CellError::Num,
        CellError::NA,
    ]
    .into_iter()
    .enumerate()
    {
        let mut port = Port::new(vec![error(e)]);
        assert_eq!(
            evaluate(LogicalFunction::ErrorType, 1, &mut port),
            Ok(number((index + 1) as f64))
        );
    }
    let mut port = Port::new(vec![error(CellError::Spill)]);
    assert_eq!(
        evaluate(LogicalFunction::ErrorType, 1, &mut port),
        Err(Failure::UnsupportedPoiError(CellError::Spill))
    );
    port.inputs[0] = Err(Failure::Operational("budget"));
    assert_eq!(
        evaluate(LogicalFunction::ErrorType, 1, &mut port),
        Err(Failure::Operational("budget"))
    );
    port.inputs[0] = Ok(number(1.0));
    assert_eq!(
        evaluate(LogicalFunction::ErrorType, 1, &mut port),
        Err(Failure::Cell(CellError::NA))
    );
}
#[test]
fn parity_uses_shared_truncation_and_doubles_beyond_i64() {
    let mut port = Port::new(vec![number(2.9999999999999996)]);
    port.truncated = 3.0;
    assert_eq!(
        evaluate(LogicalFunction::IsOdd, 1, &mut port),
        Ok(boolean(true))
    );
    port.truncated = -2.0;
    assert_eq!(
        evaluate(LogicalFunction::IsEven, 1, &mut port),
        Ok(boolean(true))
    );
    port.truncated = 1e30;
    assert_eq!(
        evaluate(LogicalFunction::IsEven, 1, &mut port),
        Ok(boolean(true))
    );
    assert_eq!(port.visits, [('n', 0), ('n', 0), ('n', 0)]);
}
#[test]
fn isref_inspects_identity_without_scalarizing_or_propagating_cell_error() {
    let mut port = Port::new(vec![Output::Reference(42)]);
    assert_eq!(
        evaluate(LogicalFunction::IsRef, 1, &mut port),
        Ok(boolean(true))
    );
    port.inputs[0] = Ok(error(CellError::Ref));
    assert_eq!(
        evaluate(LogicalFunction::IsRef, 1, &mut port),
        Ok(boolean(false))
    );
    assert_eq!(port.visits, [('r', 0), ('r', 0)]);
}
