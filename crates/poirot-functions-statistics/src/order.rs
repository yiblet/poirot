use crate::{Result, checked, finite};
use poirot_functions_core::CellError;

/// Select the requested ordered sample member without changing the caller's input.
pub fn order_statistic(values: &[f64], k: f64, largest: bool) -> Result {
    finite(values)?;
    if !k.is_finite() || k < 1.0 || k.ceil() > values.len() as f64 {
        return Err(CellError::Num);
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = k.ceil() as usize - 1;
    let selected = if largest {
        sorted
            .len()
            .checked_sub(1)
            .and_then(|last| last.checked_sub(index))
            .ok_or(CellError::Num)?
    } else {
        index
    };
    sorted.get(selected).copied().ok_or(CellError::Num)
}

/// Interpolate a percentile using the selected endpoint convention.
pub fn percentile(values: &[f64], fraction: f64) -> Result {
    finite(values)?;
    if values.is_empty()
        || values.len() > 8191
        || !fraction.is_finite()
        || !(0.0..=1.0).contains(&fraction)
    {
        return Err(CellError::Num);
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let position = (sorted.len() - 1) as f64 * fraction;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    // SAFETY: The local guard requires a nonempty sample and fraction in 0..=1; floor/ceil therefore lie in 0..sorted.len().
    checked(sorted[lower] + (position - lower as f64) * (sorted[upper] - sorted[lower]))
}

/// Rank a value with the selected ascending or descending convention.
pub fn rank(values: &[f64], number: f64, ascending: bool) -> Result {
    finite(values)?;
    checked(number)?;
    // Rank.java computes insertion rank, including values absent from the reference.
    Ok(1.0
        + values
            .iter()
            .filter(|&&v| if ascending { v < number } else { v > number })
            .count() as f64)
}
