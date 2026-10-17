//! SERIESSUM's single documented contract; collection admission is exact.
use crate::host::{CallHost, MissingPolicy, NumericPolicy, OperandKind};
use crate::*;
use poirot_coercionv2::{
    compatibility::ScalarRef,
    series::{coercev2_seriessum_coefficient, coercev2_seriessum_number},
};
use poirot_functions_core::Input;

fn number(host: &mut CallHost<'_, '_>, index: usize) -> FunctionResult<f64> {
    host.charge_work(1)?;
    let scalar = host.input(index, MissingPolicy::Blank).force()?;
    let value = if scalar.missing {
        ScalarRef::Missing
    } else {
        (&scalar.value).into()
    };
    coercev2_seriessum_number(value, |text| {
        let text = host.project_text(text)?;
        host.coercev2_number_text(&text, NumericPolicy::PinnedPoi)
    })
}
fn evaluate(host: &mut CallHost<'_, '_>) -> FunctionResult<f64> {
    let x = number(host, 0)?;
    let initial = number(host, 1)?;
    let step = number(host, 2)?;
    let description = host.describe(3)?;
    let scalar = matches!(description.kind, OperandKind::Scalar | OperandKind::Missing)
        || (description.kind == OperandKind::Reference
            && description.rows == 1
            && description.columns == 1
            && description.sheets == 1);
    let length = host.collection_len(3)?;
    host.check_values(length)?;
    host.charge_work(length as u64)?;
    let mut total = 0.0;
    for index in 0..length {
        let coefficient = if scalar {
            number(host, 3)?
        } else {
            let member = host.collection_cell(3, index)?;
            let value = if member.missing {
                ScalarRef::Missing
            } else {
                (&member.value).into()
            };
            coercev2_seriessum_coefficient(value)?
        };
        let term = poirot_functions_math::series::term(x, initial, step, index, coefficient)?;
        total = poirot_functions_math::series::accumulate(total, term)?;
    }
    Ok(total)
}

pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    at: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if args.len() != 4 {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new(session, sheet, at, args);
    match evaluate(&mut host) {
        Ok(value) => Ok(Number(value).into()),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}
