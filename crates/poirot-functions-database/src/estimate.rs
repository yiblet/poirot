//! Pure conservative shape and string work estimates.
use super::{DatabasePlan, WorkEstimate};

/// Estimate database selection and aggregation costs before allocating scratch storage.
pub(super) fn work_estimate(plan: &DatabasePlan<'_>) -> WorkEstimate {
    // Each condition may scan all database headers, then fetch a target cell.
    // Variance uses two streaming passes to avoid an unbounded values buffer.
    let rows = plan.database.rows.saturating_sub(1) as u64;
    let tests = (plan.criteria.rows.saturating_sub(1) as u64)
        .saturating_mul(plan.criteria.columns as u64)
        .saturating_mul((plan.database.columns as u64).saturating_add(3));
    let string_bytes = plan
        .database
        .cells
        .iter()
        .chain(plan.criteria.cells)
        .filter_map(|value| {
            value
                .text_ref()
                .map(|text| text.storage_bytes().saturating_mul(3) as u64)
        })
        .max()
        .unwrap_or(0)
        .max(32);
    let cell_visits = rows
        .saturating_mul(tests.saturating_add(1))
        .saturating_mul(if plan.function.variance() { 2 } else { 1 });
    WorkEstimate {
        cell_visits,
        // Wildcard star retry can revisit pattern characters for each input
        // character. Numeric normalization additionally requires bounded
        // BigUint multiplication/division (maximum finite exponent 1023).
        work_units: cell_visits.saturating_mul(string_bytes.saturating_mul(string_bytes).max(4096)),
        scratch_bytes: string_bytes.saturating_mul(64).saturating_add(4096),
    }
}
