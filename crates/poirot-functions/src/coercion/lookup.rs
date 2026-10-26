//! Lookup-specific scalar admission rules.
/// Spreadsheet error vocabulary shared by lookup kernels and host adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Invalid operand or argument shape (`#VALUE!`).
    Value,
    /// Invalid reference (`#REF!`).
    Ref,
    /// No matching value (`#N/A`).
    Na,
    /// Invalid numeric argument or result (`#NUM!`).
    Num,
    /// Zero denominator (`#DIV/0!`).
    Div0,
    /// Unresolved name (`#NAME?`).
    Name,
    /// Empty intersection (`#NULL!`).
    Null,
}
/// Workbook-independent scalar channel used by lookup and reference kernels.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    /// Numeric operand.
    Number(f64),
    /// Owned text operand.
    Text(String),
    /// Boolean operand.
    Bool(bool),
    /// Blank operand, retaining empty-cell semantics.
    Blank,
    /// Omitted argument slot.
    Missing,
    /// Catchable spreadsheet error operand.
    Error(Error),
}

/// Convert a lookup option, including omitted-as-false source behavior.
pub fn coercev2_lookup_boolean(v: Scalar) -> Result<bool, Error> {
    match v {
        Scalar::Bool(b) => Ok(b),
        Scalar::Number(n) => Ok(n != 0.0),
        Scalar::Blank | Scalar::Missing => Ok(false),
        Scalar::Text(s) if s.eq_ignore_ascii_case("TRUE") => Ok(true),
        Scalar::Text(s) if s.eq_ignore_ascii_case("FALSE") => Ok(false),
        Scalar::Error(e) => Err(e),
        _ => Err(Error::Value),
    }
}

/// Blank lookup key representation selected by matching policy.
pub fn coercev2_lookup_blank_key(tolerant: bool) -> Scalar {
    if tolerant {
        Scalar::Text(String::new())
    } else {
        Scalar::Number(0.0)
    }
}

/// Lookup numbers preserve existing nonfinite values; missing numeric arguments fail.
pub fn coercev2_lookup_number<E: From<Error>>(
    value: Scalar,
    parse: impl FnOnce(&str) -> Result<f64, E>,
) -> Result<f64, E> {
    match value {
        Scalar::Number(n) => Ok(n),
        Scalar::Bool(b) => Ok(f64::from(u8::from(b))),
        Scalar::Blank => Ok(0.0),
        Scalar::Missing => Err(Error::Value.into()),
        Scalar::Text(t) => parse(&t),
        Scalar::Error(e) => Err(e.into()),
    }
}
/// Finite signed-32 lookup ordinal with the requested floor/truncate rule.
pub fn coercev2_lookup_integer(number: f64, floor: bool) -> Result<i32, Error> {
    crate::coercion::compatibility::coercev2_integer(number, floor).map_err(|_| Error::Num)
}
/// Lookup text supports omitted-as-empty and a fallible, metered number renderer.
pub fn coercev2_lookup_text<E: From<Error>>(
    value: &Scalar,
    render: impl FnOnce(f64) -> Result<String, E>,
) -> Result<String, E> {
    match value {
        Scalar::Text(t) => Ok(t.clone()),
        Scalar::Number(n) => render(*n),
        Scalar::Bool(b) => Ok(if *b { "TRUE" } else { "FALSE" }.into()),
        Scalar::Blank | Scalar::Missing => Ok(String::new()),
        Scalar::Error(e) => Err((*e).into()),
    }
}

/// Omission rule for INDEX's optional area ordinal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexAreaDefault {
    /// The legacy union extension requires an explicit area when there are several.
    SingleArea,
    /// The documented reference form selects its first area when omitted.
    FirstArea,
}

/// Admit a zero-based area selection before selecting any source representation.
/// Explicit area ordinals truncate toward zero; zero/negative values are VALUE,
/// positive out-of-range values are REF. Legacy unions require an explicit area.
pub fn coercev2_index_area(
    number: Option<f64>,
    count: usize,
    default_area: IndexAreaDefault,
) -> Result<usize, crate::coercion::CellError> {
    let number = match number {
        Some(number) => number.trunc(),
        None if count == 1 || default_area == IndexAreaDefault::FirstArea => 1.,
        None => return Err(crate::coercion::CellError::Value),
    };
    if number < 1. {
        Err(crate::coercion::CellError::Value)
    } else if !number.is_finite() || number > count as f64 {
        Err(crate::coercion::CellError::Ref)
    } else {
        // SAFETY: The number is finite and at least one in this branch; its saturating usize cast is therefore at least one.
        Ok(number as usize - 1)
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;
    use crate::coercion::CellError;
    #[test]
    fn area_admission_distinguishes_default_zero_fraction_and_exhausted_areas() {
        assert_eq!(
            coercev2_index_area(None, 2, IndexAreaDefault::FirstArea),
            Ok(0)
        );
        assert_eq!(
            coercev2_index_area(None, 2, IndexAreaDefault::SingleArea),
            Err(CellError::Value)
        );
        for number in [-1., 0., 0.9] {
            assert_eq!(
                coercev2_index_area(Some(number), 2, IndexAreaDefault::FirstArea),
                Err(CellError::Value)
            );
        }
        assert_eq!(
            coercev2_index_area(Some(1.9), 2, IndexAreaDefault::FirstArea),
            Ok(0)
        );
        assert_eq!(
            coercev2_index_area(Some(2.), 2, IndexAreaDefault::FirstArea),
            Ok(1)
        );
        for number in [3., f64::INFINITY, f64::NAN] {
            assert_eq!(
                coercev2_index_area(Some(number), 2, IndexAreaDefault::FirstArea),
                Err(CellError::Ref)
            );
        }
    }
}
