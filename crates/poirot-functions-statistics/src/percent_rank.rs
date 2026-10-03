use crate::{PercentRankKind, Result, checked, finite};
use bigdecimal::{BigDecimal, RoundingMode, ToPrimitive};
use poirot_functions_core::CellError;

fn percent_round(value: f64, significance: u32) -> Result {
    decimal_round(decimal_value(value)?, significance)
}

fn decimal_value(value: f64) -> std::result::Result<BigDecimal, CellError> {
    // BigDecimal.valueOf in POI uses the shortest decimal representation.
    value.to_string().parse().map_err(|_| CellError::Num)
}

fn decimal_round(value: BigDecimal, significance: u32) -> Result {
    let digits = i64::from(significance.min(20));
    value
        .with_scale_round(digits + 3, RoundingMode::HalfUp)
        .with_scale_round(digits, RoundingMode::Down)
        .to_f64()
        .ok_or(CellError::Num)
}

/// Reproduces POI's count-based rank, including its positive MIN_VALUE sentinel.
pub fn percent_rank(
    values: &[f64],
    number: f64,
    significance: u32,
    kind: PercentRankKind,
) -> Result {
    finite(values)?;
    if values.is_empty() || significance == 0 || !number.is_finite() {
        return Err(CellError::Num);
    }
    fn endpoint(values: &[f64], x: f64, digits: u32, kind: PercentRankKind) -> Result {
        let less = values.iter().filter(|&&v| v < x).count();
        let greater = values.iter().filter(|&&v| v > x).count();
        if kind == PercentRankKind::Exclusive {
            return percent_round((less + 1) as f64 / (values.len() + 1) as f64, digits);
        }
        if less == values.len() || greater == values.len() {
            return Err(CellError::NA);
        }
        Ok(if less + greater == 0 {
            0.0
        } else {
            percent_round(less as f64 / (less + greater) as f64, digits)?
        })
    }
    let mut below = f64::from_bits(1);
    let mut above = f64::MAX;
    let mut min = f64::MAX;
    let mut max = f64::from_bits(1);
    for &v in values {
        if v <= number && v > below {
            below = v;
        }
        if v > number && v < above {
            above = v;
        }
        min = min.min(v);
        max = max.max(v);
    }
    if kind == PercentRankKind::Exclusive && (number < min || number > max) {
        return Err(CellError::NA);
    }
    if below == number || above == number {
        return endpoint(values, number, significance, kind);
    }
    let intermediate = if significance < 5 {
        8
    } else {
        significance.saturating_add(3)
    };
    let low_rank = endpoint(values, below, intermediate, kind)?;
    let high_rank = endpoint(values, above, intermediate, kind)?;
    let result = decimal_value(low_rank)?
        + poirot_numeric::to_text(high_rank - low_rank)
            .parse::<BigDecimal>()
            .map_err(|_| CellError::Num)?
            * decimal_value((number - below) / (above - below))?;
    checked(decimal_round(result, significance)?)
}
