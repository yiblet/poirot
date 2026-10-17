//! SUMPRODUCT selection, failure traversal, and arithmetic schedule.
use super::*;

fn coerce_sumproduct_term<O>(c: Cell, scalar: bool) -> Result<f64, Failure<O>> {
    match c {
        Cell::Number(n) => Ok(n),
        Cell::Bool(b) => Ok(if b { 1.0 } else { 0.0 }),
        Cell::Error(e) => Err(Failure::Spreadsheet(e)),
        Cell::Text(_) | Cell::Blank if !scalar => Ok(0.0),
        _ => value(),
    }
}
/// Execute SUMPRODUCT with source-specific shape checks and ordered member reads.
pub(super) fn evaluate<P: EvaluationPort>(
    p: &mut P,
    args: &[P::Argument],
) -> Result<f64, Failure<P::Operational>> {
    let first = describe(p, args.first().ok_or(CellError::Value)?)?;
    let scalar = first.origin != Origin::Array || (first.rows == 1 && first.cols == 1);
    if scalar {
        let mut result = 1.0;
        for (i, a) in args.iter().enumerate() {
            let s = if i == 0 { first } else { describe(p, a)? };
            if s.sheets > 1 || s.rows != 1 || s.cols != 1 {
                return value();
            }
            let c = read(p, a, 0, 0)?;
            // Top-level first argument error/text/blank never enters scalar path.
            if i == 0 && s.origin == Origin::Direct && !matches!(c, Cell::Number(_) | Cell::Bool(_))
            {
                return value();
            }
            result *= coerce_sumproduct_term(c, true)?;
        }
        return if result.is_finite() {
            Ok(result)
        } else {
            Err(Failure::Spreadsheet(CellError::Num))
        };
    }
    preflight(p, Some(args.len()), Some(args.len()))?;
    let mut shapes = Vec::with_capacity(args.len());
    shapes.push(first);
    for a in args.get(1..).ok_or(CellError::Value)? {
        shapes.push(describe(p, a)?);
    }
    if shapes.iter().any(|s| s.origin != Origin::Array) {
        return value();
    }
    if shapes
        .iter()
        .any(|s| s.rows != first.rows || s.cols != first.cols)
    {
        for (a, s) in args.iter().zip(&shapes).skip(1) {
            for r in 0..s.rows {
                for c in 0..s.cols {
                    if let Cell::Error(e) = read(p, a, r, c)? {
                        return Err(Failure::Spreadsheet(e));
                    }
                }
            }
        }
        return value();
    }
    preflight(
        p,
        Some(0),
        first
            .rows
            .checked_mul(first.cols)
            .and_then(|n| n.checked_mul(args.len())),
    )?;
    let mut result = 0.0;
    for r in 0..first.rows {
        for c in 0..first.cols {
            let mut product = 1.0;
            for a in args {
                product *= coerce_sumproduct_term(read(p, a, r, c)?, false)?;
            }
            result += product;
        }
    }
    if result.is_finite() {
        Ok(result)
    } else {
        Err(Failure::Spreadsheet(CellError::Num))
    }
}
