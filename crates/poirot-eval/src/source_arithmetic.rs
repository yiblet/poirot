//! Selected ExcelArithmetic.approxAdd/approxSub and bounded NumberComparer port.
use super::*;
use std::cmp::Ordering;
// Finite binary64 decimal power is bounded by 324. Reserve two decimal
// normalization branches, their <900-bit powers/reciprocal scratch and output
// limbs before the existing BigUint kernel. These are conservative logical
// work/retention units, not estimates of machine instructions or wall time.
const COMPARISON_WORK: u64 = 1024;
const COMPARISON_SLOTS: usize = 256;
/// Compare numbers using the metered pinned decimal-precision kernel.
pub(super) fn compare(a: f64, b: f64, budget: &mut BudgetMeter) -> FunctionResult<Ordering> {
    budget.allocation(COMPARISON_SLOTS)?;
    budget.charge(WorkKind::Comparison, COMPARISON_WORK)?;
    poirot_numeric::compare(a, b).map_err(Into::into)
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
pub(super) fn add(a: f64, b: f64, budget: &mut BudgetMeter) -> FunctionResult<f64> {
    Ok(if cancels(a, -b, budget)? { 0.0 } else { a + b })
}
/// Subtract numbers with pinned approximate cancellation to zero.
pub(super) fn subtract(a: f64, b: f64, budget: &mut BudgetMeter) -> FunctionResult<f64> {
    Ok(if cancels(a, b, budget)? { 0.0 } else { a - b })
}
