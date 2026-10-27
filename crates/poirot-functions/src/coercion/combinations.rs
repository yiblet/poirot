//! Admission for documented combinations with replacement.
use crate::coercion::{
    CellError,
    compatibility::{DeferredNumber, NumberPolicy, ScalarRef, coerce_number},
};

/// Blank cells and omitted slots supply zero; logical values supply zero/one.
/// Numeric text retains the host's locale-aware metered parsing path.
pub fn coerce_combina_number<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    mut text: impl FnMut(crate::coercion::TextRef<'a>) -> Result<f64, E>,
) -> Result<f64, E> {
    coerce_number(value, NumberPolicy::Finite, |deferred| match deferred {
        DeferredNumber::Missing => Ok(0.0),
        DeferredNumber::Text(value) => text(value),
    })
}

/// Reject raw negatives, truncate both finite operands, then require n >= k.
/// Keeping both counts avoids cancellation when the kernel forms n+k-1.
pub fn coerce_combina_counts(n: f64, k: f64) -> Result<(f64, f64), CellError> {
    if !n.is_finite() || !k.is_finite() || n < 0.0 || k < 0.0 {
        return Err(CellError::Num);
    }
    let n = n.trunc();
    let k = k.trunc();
    if n < k {
        Err(CellError::Num)
    } else {
        Ok((n, k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combina_checks_sign_before_truncation_and_order_after_truncation() {
        assert_eq!(coerce_combina_counts(3.2, 3.8), Ok((3.0, 3.0)));
        for (n, k) in [(-0.2, 0.0), (2.0, -0.2), (1.0, 2.0), (f64::INFINITY, 0.0)] {
            assert_eq!(coerce_combina_counts(n, k), Err(CellError::Num));
        }
        for value in [
            ScalarRef::Blank,
            ScalarRef::Missing,
            ScalarRef::Boolean(false),
        ] {
            assert_eq!(
                coerce_combina_number::<CellError>(value, |_| panic!("no text")),
                Ok(0.0)
            );
        }
    }
}
