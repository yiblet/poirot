use crate::scalar::CellError;
use crate::statistics::iteration::{KernelFailure, KernelResult, unmetered};
use crate::statistics::{Result, StudentTKind, checked, finite};
use statrs::distribution::{Continuous, ContinuousCDF, StudentsT};

/// Convert a value to its standardized deviation from the mean.
pub fn standardize(number: f64, mean: f64, stdev: f64) -> Result {
    if stdev <= 0.0 {
        return Err(CellError::Num);
    }
    checked((number - mean) / stdev)
}

/// Compute normal density or cumulative probability with spreadsheet domain checks.
pub fn normal_dist(number: f64, mean: f64, stdev: f64, cumulative: bool) -> Result {
    unmetered(normal_dist_metered(
        number,
        mean,
        stdev,
        cumulative,
        &mut |_| Ok(()),
    ))
}

/// Charge each Gamma iteration before its arithmetic; preserve port errors.
pub fn normal_dist_metered<O, M: FnMut(u64) -> std::result::Result<(), O>>(
    number: f64,
    mean: f64,
    stdev: f64,
    cumulative: bool,
    meter: &mut M,
) -> KernelResult<O> {
    finite(&[number, mean, stdev]).map_err(KernelFailure::Spreadsheet)?;
    if stdev <= 0.0 {
        return Err(KernelFailure::Spreadsheet(CellError::Num));
    }
    checked(if cumulative {
        let dev = number - mean;
        if dev.abs() > 40.0 * stdev {
            if dev < 0.0 { 0.0 } else { 1.0 }
        } else {
            let x = -dev / (stdev * std::f64::consts::SQRT_2);
            let q = if x.abs() > 40.0 {
                0.0
            } else {
                crate::statistics::commons_gamma::gamma_q(0.5, x * x, 1e-15, 10000, meter)?
            };
            0.5 * if x < 0.0 { 2.0 - q } else { q }
        }
    } else {
        let z = (number - mean) / stdev;
        ((-0.5 * z) * z - (stdev.ln() + 0.5 * std::f64::consts::TAU.ln())).exp()
    })
    .map_err(KernelFailure::Spreadsheet)
}

/// Selected NormDist direct NumberEval contract; keep source intermediates raw.
pub fn normal_dist_source_metered<O, M: FnMut(u64) -> std::result::Result<(), O>>(
    number: f64,
    mean: f64,
    stdev: f64,
    cumulative: bool,
    meter: &mut M,
) -> KernelResult<O> {
    finite(&[number, mean, stdev]).map_err(KernelFailure::Spreadsheet)?;
    if stdev <= 0.0 {
        return Err(KernelFailure::Spreadsheet(CellError::Num));
    }
    Ok(if cumulative {
        let dev = number - mean;
        if dev.abs() > 40.0 * stdev {
            if dev < 0.0 { 0.0 } else { 1.0 }
        } else {
            let x = -dev / (stdev * std::f64::consts::SQRT_2);
            let q = if x.abs() > 40.0 {
                0.0
            } else {
                let squared = x * x;
                // Commons Gamma.regularizedGammaQ returns NaN before iteration.
                if squared.is_nan() {
                    f64::NAN
                } else {
                    crate::statistics::commons_gamma::gamma_q(0.5, squared, 1e-15, 10000, meter)?
                }
            };
            0.5 * if x < 0.0 { 2.0 - q } else { q }
        }
    } else {
        let z = (number - mean) / stdev;
        ((-0.5 * z) * z - (stdev.ln() + 0.5 * std::f64::consts::TAU.ln())).exp()
    })
}

/// Inspected NormInv/NormSInv result contract: NumberEval without checkValue.
/// Same finite ingress/domain gates and arithmetic graph as the legacy helper.
pub fn normal_inverse_source(probability: f64, mean: f64, stdev: f64) -> Result {
    finite(&[probability, mean, stdev])?;
    if probability <= 0.0 || probability >= 1.0 || stdev <= 0.0 {
        return Err(CellError::Num);
    }
    Ok(mean
        + (stdev * std::f64::consts::SQRT_2)
            * crate::statistics::inverse_erf::inverse_erf(2.0 * probability - 1.0))
}
/// Compute the normal quantile for an admitted probability.
pub fn normal_inverse(probability: f64, mean: f64, stdev: f64) -> Result {
    finite(&[probability, mean, stdev])?;
    if probability <= 0.0 || probability >= 1.0 {
        return Err(CellError::Num);
    }
    if stdev <= 0.0 {
        return Err(CellError::Num);
    }
    checked(
        mean + (stdev * std::f64::consts::SQRT_2)
            * crate::statistics::inverse_erf::inverse_erf(2.0 * probability - 1.0),
    )
}

/// Compute Poisson mass or cumulative probability, including a zero-mean distribution.
pub fn poisson(number: f64, mean: f64, cumulative: bool) -> Result {
    unmetered(poisson_metered(number, mean, cumulative, &mut |_| Ok(())))
}

/// Charge Gamma and saddle-point iterations without replacing source limits.
pub fn poisson_metered<O, M: FnMut(u64) -> std::result::Result<(), O>>(
    number: f64,
    mean: f64,
    cumulative: bool,
    meter: &mut M,
) -> KernelResult<O> {
    finite(&[number, mean]).map_err(KernelFailure::Spreadsheet)?;
    if number < 0.0 || mean < 0.0 {
        return Err(KernelFailure::Spreadsheet(CellError::Num));
    }
    // POI coerceDoubleToInt clamps to Java's signed integer range.
    let count = number.trunc().min(i32::MAX as f64) as u64;
    if mean == 0.0 {
        return Ok(if cumulative || count == 0 { 1.0 } else { 0.0 });
    }
    checked(if cumulative {
        if count == i32::MAX as u64 {
            1.0
        } else {
            crate::statistics::commons_gamma::gamma_q(
                count as f64 + 1.0,
                mean,
                1e-12,
                10_000_000,
                meter,
            )?
        }
    } else {
        crate::statistics::poisson_mass::probability(count, mean, meter)?
    })
    .map_err(KernelFailure::Spreadsheet)
}
/// Compute Student-t density or a selected probability tail.
pub fn student_t(number: f64, degrees: f64, kind: StudentTKind) -> Result {
    finite(&[number, degrees])?;
    let degrees = degrees.trunc().min(i32::MAX as f64);
    if degrees < 1.0 || (kind == StudentTKind::TwoTails && number < 0.0) {
        return Err(CellError::Num);
    }
    let distribution = StudentsT::new(0.0, 1.0, degrees).map_err(|_| CellError::Num)?;
    checked(match kind {
        StudentTKind::Density => distribution.pdf(number),
        StudentTKind::Cumulative => distribution.cdf(number),
        StudentTKind::RightTail => 1.0 - distribution.cdf(number),
        StudentTKind::TwoTails => 2.0 * (1.0 - distribution.cdf(number)),
    })
}

/// Preserve the legacy TDIST tail-count and argument rules.
pub fn legacy_t_dist(number: f64, degrees: f64, tails: f64) -> Result {
    finite(&[number, degrees, tails])?;
    if number < 0.0 {
        return Err(CellError::Num);
    }
    match tails.trunc() {
        1.0 => student_t(number, degrees, StudentTKind::RightTail),
        2.0 => student_t(number, degrees, StudentTKind::TwoTails),
        _ => Err(CellError::Num),
    }
}
