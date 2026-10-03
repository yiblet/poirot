//! Ordered host effects and numeric coercion at the deferred boundary.
use super::*;

/// Charge matrix helper work while preserving operational failures.
pub(super) fn work<P: EvaluationPort>(p: &mut P, n: usize) -> Result<(), Failure<P::Operational>> {
    p.charge_work(n as u64).map_err(Failure::Operational)
}
/// Admit checked collection and work bounds before matrix allocation or algorithms.
pub(super) fn preflight<P: EvaluationPort>(
    p: &mut P,
    cells: Option<usize>,
    work_units: Option<usize>,
) -> Result<(), Failure<P::Operational>> {
    let (Some(c), Some(w)) = (cells, work_units) else {
        return Err(Failure::Operational(p.budget_exceeded()));
    };
    p.check_collection(c).map_err(Failure::Operational)?;
    work(p, w)
}
/// Project matrix helper failures into semantic, operational, or capability domains.
pub(super) fn err<P: EvaluationPort>(p: &mut P, e: domain::Error) -> Failure<P::Operational> {
    match e {
        domain::Error::Formula(e) => Failure::Spreadsheet(e),
        domain::Error::BudgetExceeded => Failure::Operational(p.budget_exceeded()),
        domain::Error::SourceUnsupported(s) => {
            Failure::UnsupportedCapability(UnsupportedCapability::SourceException(s))
        }
    }
}
/// Return the matrix adapter's standard spreadsheet `#VALUE!` failure.
pub(super) fn value<O, T>() -> Result<T, Failure<O>> {
    Err(Failure::Spreadsheet(CellError::Value))
}
/// Helper budget used only after host admission of operation resource bounds.
pub(super) const UNLIMITED: domain::Budget = domain::Budget {
    max_cells: usize::MAX,
    max_work: usize::MAX,
};
/// Resolve and admit nonempty operand dimensions without traversing members.
pub(super) fn describe<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<Shape, Failure<P::Operational>> {
    work(p, 1)?;
    let s = p.describe(a)?;
    if !s.missing && (s.rows == 0 || s.cols == 0) {
        return value();
    }
    preflight(p, s.rows.checked_mul(s.cols), Some(0))?;
    Ok(s)
}
/// Charge and read one logical matrix member.
pub(super) fn read<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
    r: usize,
    c: usize,
) -> Result<Cell, Failure<P::Operational>> {
    work(p, 1)?;
    p.read_cell(a, r, c)
}
/// Coerce a matrix member using shared host text and omission rules.
pub(super) fn numeric<P: EvaluationPort>(
    p: &mut P,
    c: Cell,
) -> Result<f64, Failure<P::Operational>> {
    match c {
        Cell::Number(n) => Ok(n),
        Cell::Bool(b) => Ok(if b { 1.0 } else { 0.0 }),
        Cell::Blank => Ok(0.0),
        Cell::Missing => p.missing_number(),
        Cell::Error(e) => Err(Failure::Spreadsheet(e)),
        Cell::Text(s) => {
            p.check_text(s.len()).map_err(Failure::Operational)?;
            work(p, s.len())?;
            p.coerce_number_text(&s)
        }
    }
}
