//! Selected ExcelArithmetic.approxAdd/approxSub and bounded NumberComparer port.
use crate::eval::limits::{BudgetMeter, WorkKind};
use crate::eval::values::function_result::FunctionResult;
use std::cmp::Ordering;

// Finite binary64 bounds the kernel's decimal powers (at most 324) and
// BigUint scratch (under 900 bits). This is one bounded scalar comparison,
// not a materialized spreadsheet collection or 1024 performed operations.
/// Compare numbers using the metered pinned decimal-precision kernel.
pub(in crate::eval) fn compare(
    a: f64,
    b: f64,
    budget: &mut BudgetMeter,
) -> FunctionResult<Ordering> {
    budget.charge(WorkKind::Comparison, 1)?;
    poirot_functions::numeric::compare(a, b).map_err(Into::into)
}

fn comparable_for_cancellation(a: f64, b: f64) -> bool {
    a != 0.0 && b != 0.0 && (a < 0.0) == (b < 0.0) && a.is_finite() && b.is_finite()
}

fn cancels(a: f64, b: f64, budget: &mut BudgetMeter) -> FunctionResult<bool> {
    if !comparable_for_cancellation(a, b) {
        return Ok(false);
    }
    Ok(compare(a, b, budget)?.is_eq())
}

/// Add numbers with pinned approximate cancellation to zero.
pub(in crate::eval) fn add(a: f64, b: f64, budget: &mut BudgetMeter) -> FunctionResult<f64> {
    Ok(if cancels(a, -b, budget)? { 0.0 } else { a + b })
}

/// Subtract numbers with pinned approximate cancellation to zero.
pub(in crate::eval) fn subtract(a: f64, b: f64, budget: &mut BudgetMeter) -> FunctionResult<f64> {
    Ok(if cancels(a, b, budget)? { 0.0 } else { a - b })
}
