//! SERIESSUM admission: scalar numeric text differs from coefficient collections.
use crate::coercion::{
    CellError,
    compatibility::{DeferredNumber, NumberPolicy, ScalarRef, coercev2_number},
};

/// Scalar operands admit blanks and contextual numeric text, but not logicals.
pub fn coercev2_seriessum_number<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    mut text: impl FnMut(crate::coercion::TextRef<'a>) -> Result<f64, E>,
) -> Result<f64, E> {
    if matches!(value, ScalarRef::Boolean(_) | ScalarRef::Missing) {
        return Err(CellError::Value.into());
    }
    coercev2_number(value, NumberPolicy::Finite, |request| match request {
        DeferredNumber::Text(value) => text(value),
        DeferredNumber::Missing => Err(CellError::Value.into()),
    })
}

/// Every collection position contributes a term; blanks supply zero without compaction.
pub fn coercev2_seriessum_coefficient(value: ScalarRef<'_>) -> Result<f64, CellError> {
    match value {
        ScalarRef::Number(number) if number.is_finite() => Ok(number),
        ScalarRef::Number(_) => Err(CellError::Num),
        ScalarRef::Blank => Ok(0.0),
        ScalarRef::Error(error) => Err(error),
        _ => Err(CellError::Value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scalar_and_collection_numeric_text_have_distinct_admission() {
        let text = crate::coercion::TextRef::Utf8("2");
        assert_eq!(
            coercev2_seriessum_number::<CellError>(ScalarRef::Text(text), |_| Ok(2.)),
            Ok(2.)
        );
        assert_eq!(
            coercev2_seriessum_coefficient(ScalarRef::Text(text)),
            Err(CellError::Value)
        );
        for value in [ScalarRef::Missing, ScalarRef::Boolean(true)] {
            assert_eq!(
                coercev2_seriessum_number::<CellError>(value, |_| panic!("no parser")),
                Err(CellError::Value)
            );
            assert_eq!(coercev2_seriessum_coefficient(value), Err(CellError::Value));
        }
        assert_eq!(coercev2_seriessum_coefficient(ScalarRef::Blank), Ok(0.));
    }
}
