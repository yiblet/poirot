//! Matrix adapters use shared typed rectangle services and independent numeric algorithms.
use crate::adapters::elementwise::{ArgumentRole, WholeCollection, collections, matrices};
use crate::adapters::failure::operational_only;
use crate::host::{CallHost, NumericPolicy, OperandKind};
use crate::registry::MatrixFunction as F;
use crate::*;
use poirot_functions_matrix as domain;

const UNLIMITED: domain::Budget = domain::Budget {
    max_cells: usize::MAX,
    max_work: usize::MAX,
};

/// Execute numeric algebra and shape-preserving transpose through shared matrix services.
fn algebra(
    host: &mut CallHost<'_, '_>,
    function: F,
    address: CellAddress,
) -> FunctionResult<Value> {
    if function == F::Sumproduct {
        let value = matrices::reduce_aligned(
            host,
            0.0,
            || 1.0,
            |_, product, operand| {
                if operand.origin == crate::host::OperandOrigin::Direct && operand.value == Blank {
                    return Err(VALUE_ERROR.into());
                }
                *product *= poirot_coercionv2::collections::coercev2_sumproduct_member(
                    (&operand.value).into(),
                    false,
                )?;
                Ok(())
            },
            |sum, product| *sum += product,
        )?;
        return Ok(numeric_outcome::coerce_checked_numeric_result(value).into());
    }
    if function == F::Frequency {
        let mut sample = |index| {
            collections::collect(
                host,
                [WholeCollection::bind(index, 0, true)],
                collections::Admission::Aggregate,
            )
        };
        let data = sample(0)?;
        let bins = sample(1)?;
        let cells = bins
            .len()
            .checked_add(1)
            .ok_or_else(|| host.size_overflow())?;
        host.check_values(cells)?;
        host.charge_work((data.len() as u64).saturating_mul(usize::BITS as u64 + 1))?;
        let result = domain::frequency(&data, &bins, UNLIMITED)
            .map_err(|error| matrices::failure(host, error))?;
        return matrices::publish(host, result, address, Number);
    }
    if function == F::Transpose {
        if host.describe(0)?.missing {
            return Err(VALUE_ERROR.into());
        }
        let matrix = matrices::matrix(host, 0, matrices::Members::Original)?;
        let result = domain::transpose(&matrix, UNLIMITED)
            .map_err(|error| matrices::failure(host, error))?;
        return matrices::publish(host, result, address, |value| value);
    }
    let first = if matches!(function, F::Mdeterm | F::Minverse) {
        matrices::square(host, 0)?
    } else {
        matrices::numeric(host, 0)?
    };
    if function == F::Mmult {
        let second = matrices::numeric(host, 1)?;
        let budget = matrices::admit(host, first.rows(), second.cols())?;
        host.charge_work((budget.max_cells as u64).saturating_mul(first.cols() as u64))?;
        let result = domain::multiply(&first, &second, UNLIMITED)
            .map_err(|error| matrices::failure(host, error))?;
        return matrices::publish(host, result, address, Number);
    }
    let n = first.rows();
    let scratch = first
        .values()
        .len()
        .checked_mul(3)
        .and_then(|cells| cells.checked_add(n))
        .ok_or_else(|| host.size_overflow())?;
    let work = n
        .checked_mul(n)
        .and_then(|n| n.checked_mul(n))
        .and_then(|n| n.checked_mul(3))
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(scratch)?;
    host.charge_work(work as u64)?;
    if function == F::Mdeterm {
        return domain::determinant(&first, UNLIMITED)
            .map(|number| Number(number).into())
            .map_err(|error| matrices::failure(host, error));
    }
    let result =
        domain::inverse(&first, UNLIMITED).map_err(|error| matrices::failure(host, error))?;
    matrices::publish(host, result, address, Number)
}
/// Dispatch modern matrix execution without a second family policy or operand port.
pub(crate) fn execute(
    function: F,
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !matches!(
        function,
        F::Mdeterm | F::Minverse | F::Mmult | F::Transpose | F::Frequency | F::Sumproduct
    ) {
        return modern::execute(function, session, sheet, address, args);
    }
    let valid = match function {
        F::Mmult | F::Frequency => args.len() == 2,
        F::Sumproduct => (1..=255).contains(&args.len()),
        _ => args.len() == 1,
    };
    if !valid {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    match algebra(&mut host, function, address) {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}

/// Coerce modern SEQUENCE arguments and admit its independently generated numeric array.
pub(crate) fn sequence(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !(1..=4).contains(&args.len()) {
        return Ok(err(VALUE_ERROR));
    }
    let count = args.len();
    let mut host = CallHost::new(session, sheet, address, args);
    let supplied = (0..count).any(|index| !host.selected_is_missing(index));
    if !supplied {
        return Ok(err(VALUE_ERROR));
    }
    let parameters = [crate::adapters::elementwise::NumericParameter::constant(1.0); 4];
    let arguments = match crate::adapters::elementwise::NumericSignature::new(&parameters)
        .scalars(&mut host, 0)
    {
        Ok(arguments) => arguments,
        Err(FunctionFailure::Spreadsheet(error)) => return Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => return Err(error),
    };
    let [rows, columns, start, step] = match arguments.as_slice() {
        &[rows, columns, start, step] => [rows, columns, start, step],
        _ => return Err(EvaluationFailure::StalePlan),
    };
    // Excel's documented grid is also the maximum admissible footprint here.
    // Fractional positive dimensions follow the existing integer coercion convention.
    if !rows.is_finite()
        || !columns.is_finite()
        || rows < 1.0
        || columns < 1.0
        || rows > f64::from(poirot_model::MAX_ROWS)
        || columns > f64::from(poirot_model::MAX_COLUMNS)
    {
        return Ok(err(VALUE_ERROR));
    }
    let rows = rows.trunc() as usize;
    let columns = columns.trunc() as usize;
    let budget = match matrices::admit(&mut host, rows, columns) {
        Ok(budget) => budget,
        Err(FunctionFailure::Spreadsheet(error)) => return Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => return Err(error),
    };
    match poirot_functions_matrix::sequence(rows, columns, start, step, budget) {
        Ok(matrix) => match matrices::publish(&mut host, matrix, address, Number) {
            Ok(value) => Ok(value),
            Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
            Err(FunctionFailure::Operational(error)) => Err(error),
        },
        Err(error) => match modern::failure(&mut host, error) {
            FunctionFailure::Spreadsheet(error) => Ok(err(error)),
            FunctionFailure::Operational(failure) => Err(failure),
        },
    }
}

mod grouping;
mod modern;
mod random_trim;
mod regression;
mod selection;
mod unit;

#[cfg(test)]
mod lossless_port_tests {
    use super::*;

    fn evaluate(
        formula: &str,
        literal: CellValue,
        budgets: EvaluationBudgets,
    ) -> EvalResult<CellValue> {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data").unwrap();
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value("A1".parse().unwrap(), literal)
            .unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.budget = BudgetMeter::new(budgets).unwrap();
        let at = "B1".parse().unwrap();
        let expression =
            crate::bound::bind(&poirot_parsing::formula::parse_formula(formula).unwrap());
        let value = session.expr(0, at, &expression)?;
        session.select_legacy_scalar(at, value)
    }

    #[test]
    fn consumed_helper_text_preserves_operational_projection_failure() {
        for formula in ["IFERROR(PV(A1,1,1),7)", "IFERROR(NORMDIST(A1,0,1,TRUE),7)"] {
            let literal = CellValue::TextUtf16(poirot_model::ExcelText::from_units(vec![0xd83d]));
            assert!(
                matches!(
                    evaluate(formula, literal, EvaluationBudgets::default()),
                    Err(EvaluationFailure::UnsupportedCapability { .. })
                ),
                "{formula}"
            );
        }
    }

    #[test]
    fn valid_helper_text_projection_matches_utf8_and_obeys_budget() {
        for formula in ["PV(A1,1,1)", "NORMDIST(A1,0,1,TRUE)", "MATCH(A1,A1:A1,0)"] {
            let plain = evaluate(formula, Text("12".into()), EvaluationBudgets::default());
            let projected = evaluate(
                formula,
                CellValue::TextUtf16(poirot_model::ExcelText::from("12")),
                EvaluationBudgets::default(),
            );
            assert_eq!(plain, projected, "{formula}");
        }
        let budgets = EvaluationBudgets {
            text_bytes: 1,
            ..EvaluationBudgets::default()
        };
        assert!(matches!(
            evaluate(
                "IFERROR(PV(A1,1,1),7)",
                CellValue::TextUtf16(poirot_model::ExcelText::from("12")),
                budgets,
            ),
            Err(EvaluationFailure::Budget(_))
        ));
        assert_eq!(
            evaluate(
                "CHOOSE(1,7,A1)",
                CellValue::TextUtf16(poirot_model::ExcelText::from_units(vec![0xd83d])),
                EvaluationBudgets::default(),
            )
            .unwrap(),
            Number(7.)
        );
    }
}
