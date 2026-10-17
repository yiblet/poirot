//! Source-specific deferred cell collection policies.
use super::*;

/// Materialize a numeric matrix under the operation's source collection rules.
pub(super) fn coercev2_matrix_collection<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
    policy: domain::MatrixCollection,
) -> Result<domain::Matrix<f64>, Failure<P::Operational>> {
    let s = describe(p, a)?;
    if MatrixSelection::from(s) == MatrixSelection::FirstCell {
        let c = read(p, a, 0, 0)?;
        let n = coercev2_matrix_number(p, c)?;
        preflight(p, Some(1), Some(1))?;
        return domain::Matrix::new(1, 1, vec![n]).map_err(|e| err(p, e));
    }
    let n = s.rows.checked_mul(s.cols);
    preflight(p, n, n)?;
    let mut values = Vec::with_capacity(n.ok_or_else(|| err(p, domain::Error::BudgetExceeded))?);
    for r in 0..s.rows {
        for c in 0..s.cols {
            let member = read(p, a, r, c)?;
            if let Some(number) =
                poirot_coercionv2::collections::coercev2_matrix_member((&member).into(), policy)?
            {
                values.push(number);
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
pub(super) fn coercev2_trend_collection<P: EvaluationPort>(
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
            let member = read(p, a, r, c)?;
            v.push(poirot_coercionv2::collections::coercev2_trend_member(
                (&member).into(),
            )?);
        }
    }
    domain::Matrix::new(s.rows, s.cols, v)
        .map(Some)
        .map_err(|e| err(p, e))
}
/// Collect numeric FREQUENCY members while preserving source filtering and errors.
pub(super) fn coercev2_frequency_collection<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<Vec<f64>, Failure<P::Operational>> {
    let s = describe(p, a)?;
    let n = s.rows.checked_mul(s.cols);
    preflight(p, n, n)?;
    let mut v = Vec::with_capacity(n.ok_or_else(|| err(p, domain::Error::BudgetExceeded))?);
    for r in 0..s.rows {
        for c in 0..s.cols {
            let member = read(p, a, r, c)?;
            let origin = match s.origin {
                Origin::Direct => poirot_coercionv2::Origin::Direct,
                Origin::Reference => poirot_coercionv2::Origin::Reference,
                Origin::Array => poirot_coercionv2::Origin::Array,
            };
            let number = poirot_coercionv2::collections::coercev2_frequency_member(
                (&member).into(),
                origin,
                |input| match input {
                    poirot_coercionv2::compatibility::DeferredNumber::Text(
                        poirot_coercionv2::TextRef::Utf8(text),
                    ) => {
                        p.check_text(text.len()).map_err(Failure::Operational)?;
                        work(p, text.len())?;
                        p.coercev2_number_text(text)
                    }
                    _ => Err(Failure::Spreadsheet(CellError::Value)),
                },
            )?;
            if let Some(number) = number {
                v.push(number);
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
