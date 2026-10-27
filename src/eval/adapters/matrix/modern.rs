//! Modern mixed-value array adapters; geometry belongs to the pure matrix crate.
use crate::eval::adapters::elementwise::controls::{EmptyControl, integer_selectors};
use crate::eval::adapters::elementwise::{
    FixedScalar, NumericParameter, WholeCollection, matrices,
};
use crate::eval::functions::registry::MatrixFunction;
use crate::eval::host::CallHost;
use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::{FunctionError, FunctionResult, VALUE_ERROR, Value, err};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellValue::{Blank, Error};
use crate::model::{CellAddress, CellError, CellValue};
use poirot_functions::matrix::{Axis, Ignore, Matrix, MemberKind, ScanOrder, Transform};

use crate::eval::adapters::elementwise::matrices::{Members, failure, matrix};

fn evaluate(
    host: &mut CallHost<'_, '_>,
    function: MatrixFunction,
    length: usize,
    address: CellAddress,
) -> FunctionResult<Value> {
    host.check_values(length)?;
    let first = matrix(
        host,
        0,
        if matches!(function, MatrixFunction::Tocol | MatrixFunction::Torow) {
            Members::Original
        } else {
            Members::Normalized
        },
    )?;
    if matches!(function, MatrixFunction::Tocol | MatrixFunction::Torow) {
        return flatten(host, function, address, first);
    }
    let original_rows = first.rows();
    let original_columns = first.cols();
    let mut inputs = Vec::new();
    inputs
        .try_reserve_exact(length)
        .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
    inputs.push(first);
    let mut padding = Error(CellError::NA);
    let transform = match function {
        MatrixFunction::Hstack | MatrixFunction::Vstack => {
            for index in 1..length {
                inputs.push(matrix(host, index, Members::Normalized)?);
            }
            Transform::Stack(if function == MatrixFunction::Hstack {
                Axis::Columns
            } else {
                Axis::Rows
            })
        }
        MatrixFunction::Take | MatrixFunction::Drop => {
            let rows = FixedScalar::at(1).optional_integer(
                host,
                EmptyControl::Omission,
                CellError::Num,
            )?;
            let columns = FixedScalar::at(2).optional_integer(
                host,
                EmptyControl::Omission,
                CellError::Num,
            )?;
            if function == MatrixFunction::Take {
                Transform::Take { rows, columns }
            } else {
                Transform::Drop { rows, columns }
            }
        }
        MatrixFunction::Choosecols | MatrixFunction::Chooserows => {
            let indices = integer_selectors(host, (1..length).map(WholeCollection::at))?;
            Transform::Choose {
                axis: if function == MatrixFunction::Choosecols {
                    Axis::Columns
                } else {
                    Axis::Rows
                },
                indices,
            }
        }
        MatrixFunction::Wrapcols | MatrixFunction::Wraprows => {
            let n = FixedScalar::at(1)
                .number(host, NumericParameter::constant(0.0))?
                .trunc();
            if !n.is_finite() || n < 1.0 {
                return Err(CellError::Num.into());
            }
            // The documented single-vector result caps large counts before conversion.
            let count = n.min(original_rows.max(original_columns) as f64) as usize;
            padding = FixedScalar::at(2).value(host, padding)?;
            Transform::Wrap {
                axis: if function == MatrixFunction::Wrapcols {
                    Axis::Columns
                } else {
                    Axis::Rows
                },
                count,
            }
        }
        MatrixFunction::Expand => {
            let dimension = |n: Option<i64>, fallback: usize| -> FunctionResult<usize> {
                match n {
                    None => Ok(fallback),
                    Some(n) => usize::try_from(n).map_err(|_| CellError::Value.into()),
                }
            };
            let rows = dimension(
                FixedScalar::at(1).optional_integer(
                    host,
                    EmptyControl::BlankOrOmission,
                    CellError::Num,
                )?,
                original_rows,
            )?;
            let columns = dimension(
                FixedScalar::at(2).optional_integer(
                    host,
                    EmptyControl::BlankOrOmission,
                    CellError::Num,
                )?,
                original_columns,
            )?;
            padding = FixedScalar::at(3).value(host, padding)?;
            Transform::Expand { rows, columns }
        }
        _ => return Err(CellError::Value.into()),
    };
    let (rows, columns) = transform.shape(&inputs).map_err(|e| failure(host, e))?;
    let budget = matrices::admit(host, rows, columns)?;
    let result = transform
        .apply(&inputs, &padding, budget)
        .map_err(|e| failure(host, e))?;
    matrices::publish(host, result, address, |value| value)
}

fn classify(value: &CellValue) -> MemberKind {
    match value {
        Blank => MemberKind::Blank,
        Error(_) => MemberKind::Error,
        _ => MemberKind::Value,
    }
}

fn flatten(
    host: &mut CallHost<'_, '_>,
    function: MatrixFunction,
    address: CellAddress,
    input: Matrix<CellValue>,
) -> FunctionResult<Value> {
    let ignore_number = FixedScalar::at(1).number(host, NumericParameter::constant(0.0))?;
    let ignore = match ignore_number.trunc() {
        0.0 => Ignore::None,
        1.0 => Ignore::Blanks,
        2.0 => Ignore::Errors,
        3.0 => Ignore::BlanksAndErrors,
        _ => return Err(CellError::Value.into()),
    };
    let scan = if FixedScalar::at(2).boolean(host, false)? {
        ScanOrder::Columns
    } else {
        ScanOrder::Rows
    };
    let retained = input
        .values()
        .iter()
        .filter(|v| ignore.retains(classify(v)))
        .count();
    let limit = if function == MatrixFunction::Tocol {
        crate::model::MAX_ROWS
    } else {
        crate::model::MAX_COLUMNS
    };
    if retained > limit as usize {
        return Err(CellError::Num.into());
    }
    let work = input
        .values()
        .len()
        .checked_add(retained)
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(retained)?;
    host.charge_work(work as u64)?;
    let result = poirot_functions::matrix::flatten(
        &input,
        if function == MatrixFunction::Tocol {
            Axis::Columns
        } else {
            Axis::Rows
        },
        scan,
        ignore,
        classify,
        poirot_functions::matrix::Budget {
            max_cells: retained,
            max_work: work,
        },
    )
    .map_err(|e| failure(host, e))?;
    matrices::publish(
        host,
        result,
        address,
        poirot_functions::coercion::compatibility::coerce_array_member,
    )
}

/// Execute a typed modern array operation, retaining operational failures outside cell errors.
pub(in crate::eval) fn execute(
    function: MatrixFunction,
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if matches!(
        function,
        MatrixFunction::Growth
            | MatrixFunction::Trend
            | MatrixFunction::Linest
            | MatrixFunction::Logest
    ) {
        return super::regression::execute(function, session, sheet, address, args);
    }
    if function == MatrixFunction::Munit {
        return super::unit::execute(session, sheet, address, args);
    }
    if matches!(
        function,
        MatrixFunction::Randarray | MatrixFunction::Trimrange
    ) {
        return super::random_trim::execute(function, session, sheet, address, args);
    }
    if function == MatrixFunction::Pivotby {
        return super::pivot_executor::execute(session, sheet, address, args);
    }
    if matches!(
        function,
        MatrixFunction::Groupby | MatrixFunction::Percentof
    ) {
        return super::grouping::execute(function, session, sheet, address, args);
    }
    if matches!(
        function,
        MatrixFunction::Filter
            | MatrixFunction::Sort
            | MatrixFunction::Sortby
            | MatrixFunction::Unique
    ) {
        return super::selection::execute(function, session, sheet, address, args);
    }
    if function == MatrixFunction::Sequence {
        return super::dispatch::sequence(session, sheet, address, args);
    }
    let length = args.len();
    let valid = match function {
        MatrixFunction::Hstack | MatrixFunction::Vstack => (1..=255).contains(&length),
        MatrixFunction::Take | MatrixFunction::Drop => (2..=3).contains(&length),
        MatrixFunction::Choosecols | MatrixFunction::Chooserows => (2..=255).contains(&length),
        MatrixFunction::Expand => (2..=4).contains(&length),
        MatrixFunction::Tocol | MatrixFunction::Torow => (1..=3).contains(&length),
        MatrixFunction::Wrapcols | MatrixFunction::Wraprows => (2..=3).contains(&length),
        _ => false,
    };
    if !valid {
        return Ok(err(VALUE_ERROR));
    }
    let arrays = if matches!(
        function,
        MatrixFunction::Hstack
            | MatrixFunction::Vstack
            | MatrixFunction::Choosecols
            | MatrixFunction::Chooserows
    ) {
        length
    } else {
        1
    };
    let mut host = CallHost::new_array(session, sheet, address, args, arrays);
    match evaluate(&mut host, function, length, address) {
        Ok(value) => Ok(value),
        Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionError::Operational(error)) => Err(error),
    }
}

#[cfg(test)]
mod resource_mapping_tests {
    use super::*;
    use crate::eval::coordinates::EvaluationTarget;
    use crate::eval::limits::{EvaluationError, ResourceFailureKind};
    use crate::model::Workbook;

    #[test]
    fn pure_matrix_failure_categories_and_target_attribution_survive_adaptation() {
        let book = Workbook::new();
        let mut session = Session::with_context(&book, Default::default());
        let address = CellAddress { row: 2, col: 3 };
        let mut host = CallHost::new_array(&mut session, 0, address, ArgumentsRef::Empty, 0);
        for (error, expected) in [
            (
                poirot_functions::matrix::MatrixError::SizeOverflow,
                ResourceFailureKind::SizeOverflow,
            ),
            (
                poirot_functions::matrix::MatrixError::AllocationFailure,
                ResourceFailureKind::Allocation,
            ),
            (
                poirot_functions::matrix::MatrixError::BudgetExceeded,
                ResourceFailureKind::BackendGuard,
            ),
        ] {
            assert!(
                matches!(failure(&mut host, error), FunctionError::Operational(EvaluationError::Resource { kind, target: Some(source), .. }) if kind == expected && source == EvaluationTarget { sheet: 0, address })
            );
        }
        assert!(matches!(
            failure(
                &mut host,
                poirot_functions::matrix::MatrixError::Formula(CellError::Value)
            ),
            FunctionError::Spreadsheet(CellError::Value)
        ));
    }
}
