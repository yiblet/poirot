//! Fixed-rate euro conversion deliberately retains the documented scalar contract.
use crate::adapters::failure::operational_only;
use crate::host::{CallHost, MissingPolicy, NumericPolicy, OperandKind};
use crate::*;
use poirot_functions_financial::euro::{EuroCurrency, euro_convert};
fn currency(host: &mut CallHost<'_, '_>, index: usize) -> FunctionResult<EuroCurrency> {
    let scalar = host.scalar(index, MissingPolicy::Blank)?;
    if let Error(error) = scalar {
        return Err(error.into());
    }
    let text = scalar.text_ref().ok_or(VALUE_ERROR)?;
    // The three-letter ISO contract can be checked losslessly before projection.
    if text.len_units() != 3 {
        return Err(VALUE_ERROR.into());
    }
    EuroCurrency::parse(&host.project_text(text)?).ok_or(VALUE_ERROR.into())
}
/// Execute EUROCONVERT, rejecting arrays instead of manufacturing lifted support.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !(3..=5).contains(&args.len()) {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    let result = (|| {
        for index in 0..args.len() {
            let shape = host.describe(index)?;
            if shape.kind == OperandKind::Array
                || shape.rows != 1
                || shape.columns != 1
                || shape.sheets != 1
            {
                return Err(VALUE_ERROR.into());
            }
        }
        let number =
            host.coercev2_argument_number(0, MissingPolicy::Blank, NumericPolicy::PinnedPoi)?;
        let source = currency(&mut host, 1)?;
        let target = currency(&mut host, 2)?;
        let full = if args.len() < 4 {
            false
        } else {
            let operand = host.scalar_data(3, MissingPolicy::Blank)?;
            !operand.missing && core_projection::coercev2_operand_boolean(&operand.value)?
        };
        let triangulation = if args.len() < 5 {
            None
        } else {
            let operand = host.scalar_data(4, MissingPolicy::Blank)?;
            if operand.missing {
                None
            } else {
                Some(host.coercev2_scalar_number(&operand.value, NumericPolicy::PinnedPoi)?)
            }
        };
        host.charge_work(1)?;
        let value =
            euro_convert(number, source, target, full, triangulation).map_err(
                |error| match error {
                    poirot_functions_financial::FinancialError::Num => Num,
                    poirot_functions_financial::FinancialError::DivZero => Div0,
                    _ => VALUE_ERROR,
                },
            )?;
        Ok(Number(value).into())
    })();
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
