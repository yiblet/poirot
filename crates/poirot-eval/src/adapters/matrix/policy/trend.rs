//! Complete TREND collection, admission, and source result presentation.
use super::*;

/// Collect regression operands, admit resources, and preserve TREND's source result shape.
pub(super) fn evaluate<P: EvaluationPort>(
    p: &mut P,
    args: &[P::Argument],
) -> Result<Output, Failure<P::Operational>> {
    let y = coerce_trend_collection(p, args.first().ok_or(CellError::Value)?)?
        .ok_or(Failure::Spreadsheet(CellError::Value))?;
    let x = if args.len() > 1 {
        coerce_trend_collection(p, args.get(1).ok_or(CellError::Value)?)?
    } else {
        None
    };
    let nx = if args.len() > 2 {
        coerce_trend_collection(p, args.get(2).ok_or(CellError::Value)?)?
    } else {
        None
    };
    let constant = if args.len() == 4 {
        let s = describe(p, args.get(3).ok_or(CellError::Value)?)?;
        if s.origin != Origin::Direct {
            return value();
        }
        match read(p, args.get(3).ok_or(CellError::Value)?, 0, 0)? {
            Cell::Bool(b) => b,
            _ => return value(),
        }
    } else {
        true
    };
    ResourcePlan::trend(&y, x.as_ref(), nx.as_ref(), constant).admit(p)?;
    let result =
        domain::trend(&y, x.as_ref(), nx.as_ref(), constant, UNLIMITED).map_err(|e| err(p, e))?;
    // Trend.java resultSize is original new_x/known_x/y, while vals is
    // newX.length. CacheAreaEval reads row*resultWidth+column without a
    // length check. One prediction bypasses CacheAreaEval entirely.
    let shape = nx.as_ref().or(x.as_ref()).unwrap_or(&y);
    match presentation(result.values().len(), shape.rows(), shape.cols()) {
        Presentation::Scalar => Ok(Output::Number(
            *result.values().first().ok_or(CellError::Value)?,
        )),
        Presentation::Array => Ok(Output::Array(result)),
        Presentation::Malformed(capability) => Err(Failure::UnsupportedCapability(capability)),
    }
}

/// Source selects a scalar before consulting the original argument geometry.
enum Presentation {
    Scalar,
    Array,
    Malformed(UnsupportedCapability),
}

fn presentation(values: usize, rows: usize, cols: usize) -> Presentation {
    if values == 1 {
        Presentation::Scalar
    } else if rows.checked_mul(cols) == Some(values) {
        Presentation::Array
    } else {
        Presentation::Malformed(UnsupportedCapability::MalformedTrendArea { rows, cols, values })
    }
}
