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

/// Admit a supplied Excel criterion after scalar selection and omission handling.
/// An empty reference (including one forwarded by selection) requests numeric
/// zero. Empty text remains text; candidate range members are not converted.
pub fn coerce_criteria_excel_operand(value: Cell) -> Cell {
    match value {
        Cell::Blank => Cell::Number(0.0),
        value => value,
    }
}

/// Admit parenthesized negative numeric criteria without changing literal text grammars.
/// Saved Excel sumifs.xls criteria and independent Calc sibling checks establish
/// the accounting form; the classic plain-number parser remains unchanged.
pub fn coerce_criteria_accounting_threshold(value: Cell) -> Cell {
    if let Cell::Text(text) = &value {
        let text = text.trim_matches(|c: char| c <= '\u{20}');
        if let Some(inner) = text.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            let inner = inner.trim_matches(|c: char| c <= '\u{20}');
            if !inner.starts_with(['+', '-'])
                && let Some(number) =
                    crate::coercion::compatibility::coerce_criteria_number_text(inner)
                && number >= 0.
            {
                return Cell::Number(-number);
            }
        }
    }
    value
}

/// Resolve an ordered criterion's remaining text threshold with the consumer's contextual
/// number parser. Non-numeric text stays literal; operational failures escape.
/// The consumer must select an ordered comparison (<, <=, >, >=); equality and
/// inequality retain literal matching. Already typed operands keep their precedence.
pub fn coerce_criteria_ordered_threshold<E>(
    value: Cell,
    parse: impl FnOnce(&str) -> Result<Option<f64>, E>,
) -> Result<Cell, E> {
    match value {
        Cell::Text(text) => Ok(match parse(&text)? {
            Some(number) => Cell::Number(number),
            None => Cell::Text(text),
        }),
        value => Ok(value),
    }
}

/// Origin-sensitive admission of an eligible subtotal member.
pub fn coerce_subtotal_member(
    value: &Cell,
    referenced: bool,
    kind: i32,
) -> Result<Option<f64>, Error> {
    Ok(match value {
        Cell::Number(n) => Some(*n),
        Cell::Bool(b) if !referenced && kind != 2 && kind != 3 => Some(if *b { 1.0 } else { 0.0 }),
        Cell::Text(s) if !referenced && kind != 2 && kind != 3 => Some(
            crate::coercion::compatibility::coerce_criteria_number_text(s).ok_or(Error::Value)?,
        ),
        Cell::Error(e) if kind != 2 && kind != 3 => return Err(*e),
        _ => None,
    })
}
