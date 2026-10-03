//! Source-specific deferred cell collection policies.
use super::*;

/// Materialize a numeric matrix under the operation's source collection rules.
pub(super) fn matrix<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
    policy: domain::MatrixCollection,
) -> Result<domain::Matrix<f64>, Failure<P::Operational>> {
    let s = describe(p, a)?;
    if MatrixSelection::from(s) == MatrixSelection::FirstCell {
        let c = read(p, a, 0, 0)?;
        let n = numeric(p, c)?;
        preflight(p, Some(1), Some(1))?;
        return domain::Matrix::new(1, 1, vec![n]).map_err(|e| err(p, e));
    }
    let n = s.rows.checked_mul(s.cols);
    preflight(p, n, n)?;
    let mut values = Vec::with_capacity(n.ok_or_else(|| err(p, domain::Error::BudgetExceeded))?);
    for r in 0..s.rows {
        for c in 0..s.cols {
            match read(p, a, r, c)? {
                Cell::Number(n) => values.push(n),
                Cell::Error(e) => return Err(Failure::Spreadsheet(e)),
                Cell::Blank if policy == domain::MatrixCollection::Determinant => return value(),
                Cell::Blank if policy == domain::MatrixCollection::Transpose => values.push(0.0),
                _ => {}
            }
        }
    }
    if values.is_empty() || (values.len() == 1 && policy != domain::MatrixCollection::Transpose) {
        return value();
    }
    values.resize(n.ok_or_else(|| err(p, domain::Error::BudgetExceeded))?, 0.0);
    domain::Matrix::new(s.rows, s.cols, values).map_err(|e| err(p, e))
}
/// Collect a TREND operand, preserving omitted inputs as `None`.
pub(super) fn trend_matrix<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<Option<domain::Matrix<f64>>, Failure<P::Operational>> {
    let s = describe(p, a)?;
    if s.missing {
        return Ok(None);
    }
    if s.sheets > 1 {
        return value();
    }
    let n = s.rows.checked_mul(s.cols);
    preflight(p, n, n)?;
    let mut v = Vec::with_capacity(n.ok_or_else(|| err(p, domain::Error::BudgetExceeded))?);
    for r in 0..s.rows {
        for c in 0..s.cols {
            v.push(match read(p, a, r, c)? {
                Cell::Number(n) => n,
                Cell::Bool(b) => {
                    if b {
                        1.0
                    } else {
                        0.0
                    }
                }
                _ => return value(),
            });
        }
    }
    domain::Matrix::new(s.rows, s.cols, v)
        .map(Some)
        .map_err(|e| err(p, e))
}
/// Collect numeric FREQUENCY members while preserving source filtering and errors.
pub(super) fn frequency_values<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<Vec<f64>, Failure<P::Operational>> {
    let s = describe(p, a)?;
    let n = s.rows.checked_mul(s.cols);
    preflight(p, n, n)?;
    let mut v = Vec::with_capacity(n.ok_or_else(|| err(p, domain::Error::BudgetExceeded))?);
    for r in 0..s.rows {
        for c in 0..s.cols {
            match read(p, a, r, c)? {
                Cell::Number(n) => v.push(n),
                Cell::Error(e) => return Err(Failure::Spreadsheet(e)),
                Cell::Bool(b) if s.origin == Origin::Direct => v.push(if b { 1.0 } else { 0.0 }),
                Cell::Text(t) if s.origin == Origin::Direct => v.push(numeric(p, Cell::Text(t))?),
                _ => {}
            }
        }
    }
    Ok(v)
}
/// Origin, rather than geometry, selects the scalar coercion protocol.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MatrixSelection {
    FirstCell,
    Area,
}

impl From<Shape> for MatrixSelection {
    fn from(shape: Shape) -> Self {
        if shape.origin == Origin::Array {
            Self::Area
        } else {
            Self::FirstCell
        }
    }
}
