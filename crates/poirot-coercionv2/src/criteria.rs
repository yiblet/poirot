//! Criteria and SUBTOTAL value admission.
/// Spreadsheet error vocabulary used by criteria matching and aggregation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Empty reference intersection (`#NULL!`).
    Null,
    /// Zero denominator (`#DIV/0!`).
    DivZero,
    /// Invalid criterion or operand (`#VALUE!`).
    Value,
    /// Invalid reference (`#REF!`).
    Ref,
    /// Unresolved name (`#NAME?`).
    Name,
    /// Invalid numeric result (`#NUM!`).
    Num,
    /// Unavailable value (`#N/A`).
    Na,
}
/// Borrowed cell projection used to match criteria without workbook ownership.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    /// Blank criterion candidate.
    Blank,
    /// Numeric criterion candidate.
    Number(f64),
    /// Text candidate consumed by comparison or wildcard matching.
    Text(String),
    /// Boolean criterion candidate.
    Bool(bool),
    /// Spreadsheet error retained for family-specific handling.
    Error(Error),
}

/// Origin-sensitive admission of an eligible subtotal member.
pub fn coercev2_subtotal_member(
    value: &Cell,
    referenced: bool,
    kind: i32,
) -> Result<Option<f64>, Error> {
    Ok(match value {
        Cell::Number(n) => Some(*n),
        Cell::Bool(b) if !referenced && kind != 2 && kind != 3 => Some(if *b { 1.0 } else { 0.0 }),
        Cell::Text(s) if !referenced && kind != 2 && kind != 3 => {
            Some(crate::compatibility::coercev2_criteria_number_text(s).ok_or(Error::Value)?)
        }
        Cell::Error(e) if kind != 2 && kind != 3 => return Err(*e),
        _ => None,
    })
}
