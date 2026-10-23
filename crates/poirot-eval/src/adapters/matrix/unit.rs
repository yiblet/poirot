//! Identity-array generation through the independent matrix kernel.
use super::*;
use crate::host::MissingPolicy;
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
        let dimension =
            host.coercev2_argument_number(0, MissingPolicy::Blank, NumericPolicy::PinnedPoi)?;
        if !dimension.is_finite() || dimension < 1.0 || dimension > f64::from(MAX_COLUMNS) {
            return Err(VALUE_ERROR.into());
        }
        let n = dimension.trunc() as usize;
        let count = n.checked_mul(n).ok_or_else(|| host.size_overflow())?;
        host.check_values(count)?;
        host.check_values(n)?;
        host.charge_work(count as u64)?;
        let matrix = poirot_functions_matrix::unit(
            n,
            poirot_functions_matrix::Budget {
                max_cells: count,
                max_work: count,
            },
        )
        .map_err(|e| modern::failure(&mut host, e))?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(n)
            .map_err(|_| host.allocation_failure())?;
        // SAFETY: The local dimension >= 1 check and truncation establish n >= 1.
        for source in matrix.values().chunks(n) {
            let mut row = Vec::new();
            row.try_reserve_exact(n)
                .map_err(|_| host.allocation_failure())?;
            for &value in source {
                // SAFETY: Matrix chunks have at most n entries, and this row reserved n.
                row.push(Number(value));
            }
            // SAFETY: The square matrix has exactly n chunks; n slots were reserved locally.
            rows.push(row);
        }
        Ok(Value::Array(ArrayView::anchored(rows, Some(address))))
    })();
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
