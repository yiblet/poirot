use super::modern;
use crate::eval::adapters::elementwise::{ArgumentRole, WholeCollection, collections, matrices};
use crate::eval::functions::registry::MatrixFunction;
use crate::eval::host::{CallHost, OperandReader, SelectionAccess};
use crate::eval::limits::{EvalResult, EvaluationError};
use crate::eval::session::Session;
use crate::eval::values::{
    FunctionError, FunctionResult, VALUE_ERROR, Value, err, numeric_outcome,
};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellAddress;
use crate::model::CellValue::{Blank, Number};
use poirot_functions::matrix as domain;

pub(super) const UNLIMITED: domain::Budget = domain::Budget {
    max_cells: usize::MAX,
    max_work: usize::MAX,
};

/// Execute numeric algebra and shape-preserving transpose through shared matrix services.
pub(super) fn algebra(
    host: &mut CallHost<'_, '_>,
    function: MatrixFunction,
    address: CellAddress,
) -> FunctionResult<Value> {
    if function == MatrixFunction::Sumproduct {
        let value = matrices::reduce_aligned(
            host,
            0.0,
            || 1.0,
            |_, product, operand| {
                if operand.origin == crate::eval::values::OperandOrigin::Direct
                    && operand.value == Blank
                {
                    return Err(VALUE_ERROR.into());
                }
                *product *= poirot_functions::coercion::collections::coerce_sumproduct_member(
                    (&operand.value).into(),
                    false,
                )?;
                Ok(())
            },
            |sum, product| *sum += product,
        )?;
        return Ok(numeric_outcome::convert_checked_numeric_result(value).into());
    }
    if function == MatrixFunction::Frequency {
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
    if function == MatrixFunction::Transpose {
        if OperandReader::new(host).describe(0)?.missing {
            return Err(VALUE_ERROR.into());
        }
        let matrix = matrices::matrix(host, 0, matrices::Members::Original)?;
        let result = domain::transpose(&matrix, UNLIMITED)
            .map_err(|error| matrices::failure(host, error))?;
        return matrices::publish(host, result, address, |value| value);
    }
    let first = if matches!(function, MatrixFunction::Mdeterm | MatrixFunction::Minverse) {
        matrices::square(host, 0)?
    } else {
        matrices::numeric(host, 0)?
    };
    if function == MatrixFunction::Mmult {
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
    if function == MatrixFunction::Mdeterm {
        return domain::determinant(&first, UNLIMITED)
            .map(|number| Number(number).into())
            .map_err(|error| matrices::failure(host, error));
    }
    let result =
        domain::inverse(&first, UNLIMITED).map_err(|error| matrices::failure(host, error))?;
    matrices::publish(host, result, address, Number)
}

/// Dispatch modern matrix execution without a second family policy or operand port.
pub(in crate::eval) fn execute(
    function: MatrixFunction,
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !matches!(
        function,
        MatrixFunction::Mdeterm
            | MatrixFunction::Minverse
            | MatrixFunction::Mmult
            | MatrixFunction::Transpose
            | MatrixFunction::Frequency
            | MatrixFunction::Sumproduct
    ) {
        return modern::execute(function, session, sheet, address, args);
    }
    let valid = match function {
        MatrixFunction::Mmult | MatrixFunction::Frequency => args.len() == 2,
        MatrixFunction::Sumproduct => (1..=255).contains(&args.len()),
        _ => args.len() == 1,
    };
    if !valid {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    match algebra(&mut host, function, address) {
        Ok(value) => Ok(value),
        Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionError::Operational(error)) => Err(error),
    }
}

/// Coerce modern SEQUENCE arguments and admit its independently generated numeric array.
pub(in crate::eval) fn sequence(
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
    let supplied = (0..count).any(|index| !SelectionAccess::new(&mut host).is_missing(index));
    if !supplied {
        return Ok(err(VALUE_ERROR));
    }
    let parameters = [crate::eval::adapters::elementwise::NumericParameter::constant(1.0); 4];
    let arguments = match crate::eval::adapters::elementwise::NumericSignature::new(&parameters)
        .scalars(&mut host, 0)
    {
        Ok(arguments) => arguments,
        Err(FunctionError::Spreadsheet(error)) => return Ok(err(error)),
        Err(FunctionError::Operational(error)) => return Err(error),
    };
    let [rows, columns, start, step] = match arguments.as_slice() {
        &[rows, columns, start, step] => [rows, columns, start, step],
        _ => return Err(EvaluationError::StalePlan),
    };
    // Excel's documented grid is also the maximum admissible footprint here.
    // Fractional positive dimensions follow the existing integer coercion convention.
    if !rows.is_finite()
        || !columns.is_finite()
        || rows < 1.0
        || columns < 1.0
        || rows > f64::from(crate::model::MAX_ROWS)
        || columns > f64::from(crate::model::MAX_COLUMNS)
    {
        return Ok(err(VALUE_ERROR));
    }
    let rows = rows.trunc() as usize;
    let columns = columns.trunc() as usize;
    let budget = match matrices::admit(&mut host, rows, columns) {
        Ok(budget) => budget,
        Err(FunctionError::Spreadsheet(error)) => return Ok(err(error)),
        Err(FunctionError::Operational(error)) => return Err(error),
    };
    match poirot_functions::matrix::sequence(rows, columns, start, step, budget) {
        Ok(matrix) => match matrices::publish(&mut host, matrix, address, Number) {
            Ok(value) => Ok(value),
            Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
            Err(FunctionError::Operational(error)) => Err(error),
        },
        Err(error) => match matrices::failure(&mut host, error) {
            FunctionError::Spreadsheet(error) => Ok(err(error)),
            FunctionError::Operational(failure) => Err(failure),
        },
    }
}
