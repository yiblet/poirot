//! Identity-array generation through the independent matrix kernel.
use super::*;
use crate::adapters::elementwise::{FixedScalar, NumericParameter};
/// Execute MUNIT with checked square geometry before allocation or publication.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new(session, sheet, address, args);
    let result = (|| {
        let dimension = FixedScalar::at(0).number(&mut host, NumericParameter::required())?;
        if !dimension.is_finite() || dimension < 1.0 || dimension > f64::from(MAX_COLUMNS) {
            return Err(VALUE_ERROR.into());
        }
        let n = dimension.trunc() as usize;
        let budget = matrices::admit(&mut host, n, n)?;
        let matrix =
            poirot_functions_matrix::unit(n, budget).map_err(|e| modern::failure(&mut host, e))?;
        crate::adapters::elementwise::matrices::publish(&mut host, matrix, address, Number)
    })();
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
