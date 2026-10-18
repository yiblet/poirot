//! Actual staged logical-family execution over the shared deferred host.
use crate::adapters::failure::project_host;
use crate::adapters::logical::policy::{
    EvaluationPort, Failure, LogicalFunction as HelperFunction,
};
use crate::host::{CallHost, HostOperand, MissingPolicy, NumericPolicy, OperandKind};
use crate::*;
use array_switch::execute_switch;

fn port_failure(failure: FunctionFailure) -> Failure<EvaluationFailure> {
    project_host(failure, Failure::Cell, Failure::Operational)
}
impl EvaluationPort for CallHost<'_, '_> {
    /// Selected result channel preserving lazy value/reference identity.
    type Output = HostOperand;
    /// Host abort channel kept outside catchable spreadsheet values.
    type OperationalError = EvaluationFailure;
    fn scalar(&mut self, index: usize) -> Result<CellValue, Failure<EvaluationFailure>> {
        let scalar = self
            .scalar_data(index, MissingPolicy::Blank)
            .map_err(port_failure)?;
        if scalar.missing {
            return Err(Failure::Operational(self.capability_failure(
                CapabilityId::FunctionAdapter,
                "pinned MissingArgEval scalar coercion throws a source exception",
            )));
        }
        match scalar.value {
            Error(error) => Err(Failure::Cell(error)),
            value => Ok(value),
        }
    }
    fn argument(&mut self, index: usize) -> Result<HostOperand, Failure<EvaluationFailure>> {
        CallHost::argument(self, index).map_err(port_failure)
    }
    fn is_reference(&mut self, index: usize) -> Result<bool, Failure<EvaluationFailure>> {
        let pinned = self.builtin_semantics_policy() == BuiltinSemanticsPolicy::PinnedPoi;
        self.kind(index)
            .map(|kind| kind == OperandKind::Reference || (pinned && kind == OperandKind::Array))
            .map_err(port_failure)
    }
    fn coercev2_truncated_number(
        &mut self,
        index: usize,
    ) -> Result<f64, Failure<EvaluationFailure>> {
        let scalar = self
            .scalar_data(index, MissingPolicy::Blank)
            .map_err(port_failure)?;
        if scalar.missing {
            return Err(Failure::Operational(self.capability_failure(
                CapabilityId::NumericConversion,
                "pinned MissingArgEval numeric coercion throws a source exception",
            )));
        }
        let number = self
            .coercev2_scalar_number(&scalar.value, NumericPolicy::PinnedPoi)
            .map_err(port_failure)?;
        let rendered = self
            .coercev2_number_to_text(number, NumericPolicy::PinnedPoi)
            .map_err(port_failure)?;
        poirot_coercionv2::compatibility::coercev2_truncated_text(&rendered).map_err(Failure::Cell)
    }
    fn equal(
        &mut self,
        left: &CellValue,
        right: &CellValue,
    ) -> Result<bool, Failure<EvaluationFailure>> {
        CallHost::equal(self, left, right, NumericPolicy::PinnedPoi).map_err(port_failure)
    }
    fn value(value: CellValue) -> HostOperand {
        HostOperand::from_scalar(value)
    }
}

/// Execute lazy logical helper selection without forcing skipped operands.
pub(crate) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: HelperFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if matches!(function, HelperFunction::Ifs | HelperFunction::Switch) {
        session.read(DependencyRead::BuiltinPolicy);
    }
    if function == HelperFunction::Switch
        && session.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation
    {
        return execute_switch(session, sheet, address, arguments);
    }
    if function == HelperFunction::Ifs
        && session.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation
    {
        if !arguments.len().is_multiple_of(2) {
            return Ok(err(VALUE_ERROR));
        }
        let mut host = CallHost::new_inheriting(session, sheet, address, arguments);
        // SAFETY: The literal step 2 is nonzero.
        for ordinal in (0..arguments.len()).step_by(2) {
            match semantic(host.retains_array(ordinal))? {
                Err(error) => return Ok(err(error)),
                Ok(true) => {
                    let condition = match semantic(host.argument(ordinal))? {
                        Ok(operand) => operand.into_value(),
                        Err(error) => return Ok(err(error)),
                    };
                    drop(host);
                    return session.array_ifs(sheet, address, arguments, ordinal, condition);
                }
                Ok(false) => {}
            }
            let condition = match EvaluationPort::scalar(&mut host, ordinal) {
                Ok(value) => value,
                Err(Failure::Cell(error)) => return Ok(err(error)),
                Err(Failure::Operational(failure)) => return Err(failure),
                Err(Failure::UnsupportedPoiError(_)) => {
                    return Err(EvaluationFailure::StalePlan);
                }
            };
            match policy::coercev2_logical_test(&condition) {
                Ok(true) => {
                    return match semantic(
                        host.argument(ordinal.checked_add(1).ok_or(EvaluationFailure::StalePlan)?),
                    )? {
                        Ok(operand) => Ok(operand.into_value()),
                        Err(error) => Ok(err(error)),
                    };
                }
                Ok(false) => {}
                Err(error) => return Ok(err(error)),
            }
        }
        return Ok(err(NA));
    }
    let mut host = match function {
        HelperFunction::Ifs => CallHost::new_selecting(session, sheet, address, arguments, 1, None),
        HelperFunction::Switch => {
            let default = if arguments.len().is_multiple_of(2) {
                arguments.len().checked_sub(1)
            } else {
                None
            };
            CallHost::new_selecting(session, sheet, address, arguments, 2, default)
        }
        _ => CallHost::new(session, sheet, address, arguments),
    };
    // Errortype only resolves the value and reports NA for every non-error,
    // including MissingArgEval; it does not numerically/boolean-coerce Missing.
    if function == HelperFunction::ErrorType && host.len() == 1 {
        match host.kind(0) {
            Ok(OperandKind::Missing) => return Ok(err(NA)),
            Ok(_) => {}
            Err(FunctionFailure::Spreadsheet(error)) => return Ok(err(error)),
            Err(FunctionFailure::Operational(failure)) => return Err(failure),
        }
    }
    match crate::adapters::logical::policy::evaluate(function, host.len(), &mut host) {
        Ok(value) => Ok(value.into_value()),
        Err(Failure::Cell(error)) => Ok(err(error)),
        Err(Failure::Operational(failure)) => Err(failure),
        Err(Failure::UnsupportedPoiError(error)) => Err(host.capability_failure(
            CapabilityId::FunctionAdapter,
            format!("pinned ERRORTYPE does not classify {error}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_isref_formulas_keep_reference_identity_and_error_order() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Sheet1").unwrap();
        let at = CellAddress { row: 0, col: 0 };
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula(at, "1/0", Number(99.))
            .unwrap();
        for (formula, expected) in [
            ("ISREF(A1)", Boolean(true)),
            ("ISREF(A1:B2)", Boolean(true)),
            ("ISREF(INDEX(A1:B2,0,1))", Boolean(true)),
            ("ISREF(INDEX({1,2},1,2))", Boolean(true)),
            ("ISREF({1;2})", Boolean(true)),
            ("ISREF(1/0)", Boolean(false)),
            ("ISREF(UNKNOWN_FUNCTION())", Boolean(false)),
            (r#"ISREF("A1")"#, Boolean(false)),
            ("ISREF(NoSuchSheet!A1)", Boolean(false)),
            ("ISREF()", Error(VALUE_ERROR)),
            ("ISREF(A1,UNKNOWN_FUNCTION())", Error(VALUE_ERROR)),
            ("IF(TRUE,ISREF(A1),UNKNOWN_FUNCTION())", Boolean(true)),
        ] {
            assert_eq!(
                match crate::run_formula(
                    &workbook,
                    0,
                    at,
                    formula,
                    Default::default(),
                    Default::default()
                ) {
                    crate::EvaluationOutcome::Complete { value, .. } => value,
                    outcome => panic!("evaluation aborted: {outcome:?}"),
                },
                expected,
                "{formula}"
            );
        }
        assert_eq!(
            match crate::run_formula(
                &workbook,
                0,
                at,
                "INDEX({1,2},1,2)",
                Default::default(),
                Default::default()
            ) {
                crate::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            Number(2.)
        );
        assert_eq!(
            match crate::run_formula(
                &workbook,
                0,
                at,
                "1+INDEX({1,2},1,2)",
                Default::default(),
                Default::default()
            ) {
                crate::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            Number(3.)
        );
        let plan = bound::FormulaPlan::bind(
            &poirot_parsing::formula::parse_formula("ISREF(A1,UNKNOWN_FUNCTION())").unwrap(),
        )
        .unwrap();
        assert!(matches!(
            run_plan(
                &workbook,
                0,
                at,
                &plan,
                EvaluationInputs::default(),
                EvaluationBudgets {
                    run_work: 1,
                    ..EvaluationBudgets::default()
                }
            ),
            EvaluationOutcome::Complete {
                value: Error(CellError::Value),
                ..
            }
        ));
        let plan = bound::FormulaPlan::bind(
            &poirot_parsing::formula::parse_formula("IFERROR(ISREF({1,2,3}),TRUE)").unwrap(),
        )
        .unwrap();
        assert!(matches!(
            run_plan(
                &workbook,
                0,
                at,
                &plan,
                EvaluationInputs::default(),
                EvaluationBudgets {
                    materialized_values: 2,
                    ..EvaluationBudgets::default()
                }
            ),
            EvaluationOutcome::Aborted { .. }
        ));
    }
}

/// Logical coercion and deferred branch-selection contracts.
pub mod policy;

/// Evaluator-owned logical builtin dispatch.
pub(crate) mod execution;

mod array_choose;
mod array_ifs;
mod array_selection;
mod array_switch;
