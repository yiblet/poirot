use crate::scalar::CellError;
use crate::statistics::aggregate_input::AggregateInput;
use crate::statistics::distribution_error::DistributionError;
use crate::statistics::grouped::{BuiltinAggregator, median};
use crate::statistics::order::order_statistic;
use crate::statistics::{Result, checked, finite, modern_sample};
use std::convert::Infallible;

/// Calculate AGGREGATE results from values that the caller has filtered and converted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateFunction {
    /// Arithmetic mean (selector 1).
    Average,
    /// Number of numeric members (selector 2).
    Count,
    /// Number of nonblank members (selector 3).
    CountA,
    /// Largest numeric member (selector 4).
    Maximum,
    /// Smallest numeric member (selector 5).
    Minimum,
    /// Product of numeric members (selector 6).
    Product,
    /// Sample standard deviation (selector 7).
    StdevSample,
    /// Population standard deviation (selector 8).
    StdevPopulation,
    /// Sum of numeric members (selector 9).
    Sum,
    /// Sample variance (selector 10).
    VarianceSample,
    /// Population variance (selector 11).
    VariancePopulation,
    /// Middle ordered numeric observation (selector 12).
    Median,
    /// Smallest most-frequent numeric observation (selector 13).
    Mode,
    /// K-th largest numeric observation (selector 14).
    Large,
    /// K-th smallest numeric observation (selector 15).
    Small,
    /// Percentile including endpoints (selector 16).
    PercentileInclusive,
    /// Quartile including endpoints (selector 17).
    QuartileInclusive,
    /// Percentile excluding endpoints (selector 18).
    PercentileExclusive,
    /// Quartile excluding endpoints (selector 19).
    QuartileExclusive,
}

impl AggregateFunction {
    /// Decode selectors 1–19; return #VALUE! for any other number.
    pub fn from_number(number: u8) -> std::result::Result<Self, CellError> {
        Ok(match number {
            1 => Self::Average,
            2 => Self::Count,
            3 => Self::CountA,
            4 => Self::Maximum,
            5 => Self::Minimum,
            6 => Self::Product,
            7 => Self::StdevSample,
            8 => Self::StdevPopulation,
            9 => Self::Sum,
            10 => Self::VarianceSample,
            11 => Self::VariancePopulation,
            12 => Self::Median,
            13 => Self::Mode,
            14 => Self::Large,
            15 => Self::Small,
            16 => Self::PercentileInclusive,
            17 => Self::QuartileInclusive,
            18 => Self::PercentileExclusive,
            19 => Self::QuartileExclusive,
            _ => return Err(CellError::Value),
        })
    }

    /// Calculate a result from the supplied values.
    /// Selectors 14–19 require `k` and return #VALUE! if it is absent.
    ///
    /// The caller excludes rows and nested totals according to the selected options.
    /// For numeric reductions, it propagates any source errors that remain. It then
    /// supplies the numeric sample and nonblank count.
    /// The count must include every numeric member. Extra `k` is ignored for
    /// selectors 1–13; quartile indices truncate toward zero.
    pub fn calculate(self, input: AggregateInput<'_>, k: Option<f64>) -> Result {
        if input.nonempty_count < input.numbers.len() {
            return Err(CellError::Value);
        }
        if self == Self::CountA {
            return Ok(input.nonempty_count as f64);
        }
        if self == Self::Count {
            return Ok(input.numbers.len() as f64);
        }
        let values = input.numbers;
        finite(values)?;
        let reduction = match self {
            Self::Average => Some(BuiltinAggregator::Average),
            Self::Maximum => Some(BuiltinAggregator::Maximum),
            Self::Minimum => Some(BuiltinAggregator::Minimum),
            Self::Product => Some(BuiltinAggregator::Product),
            Self::StdevSample => Some(BuiltinAggregator::StdevSample),
            Self::StdevPopulation => Some(BuiltinAggregator::StdevPopulation),
            Self::Sum => Some(BuiltinAggregator::Sum),
            Self::VarianceSample => Some(BuiltinAggregator::VarianceSample),
            Self::VariancePopulation => Some(BuiltinAggregator::VariancePopulation),
            Self::Mode => Some(BuiltinAggregator::Mode),
            _ => None,
        };
        if let Some(reduction) = reduction {
            let mut values = values.to_vec();
            if self == Self::Mode {
                // Sorting chooses the smallest of equally frequent modes.
                // SAFETY: total_cmp is a total order and invokes no caller code.
                values.sort_unstable_by(f64::total_cmp);
            }
            return reduction.numeric(&mut values);
        }
        if self == Self::Median {
            return median(&mut values.to_vec());
        }
        let k = k.ok_or(CellError::Value)?;
        match self {
            Self::Large | Self::Small => order_statistic(values, k, self == Self::Large),
            Self::PercentileInclusive => inclusive_percentile(values, k),
            Self::QuartileInclusive | Self::QuartileExclusive => {
                if !k.is_finite() {
                    return Err(CellError::Num);
                }
                let quart = k.trunc();
                if self == Self::QuartileInclusive {
                    inclusive_percentile(values, quart / 4.0)
                } else {
                    exclusive_percentile(values, quart / 4.0)
                }
            }
            Self::PercentileExclusive => exclusive_percentile(values, k),
            _ => Err(CellError::Value),
        }
    }
}

/// Interpolate between adjacent ranks, including the sample endpoints at 0 and 1.
fn inclusive_percentile(values: &[f64], k: f64) -> Result {
    if values.is_empty() || !k.is_finite() || !(0.0..=1.0).contains(&k) {
        return Err(CellError::Num);
    }
    let mut sorted = values.to_vec();
    // SAFETY: total_cmp is a total order and invokes no caller code.
    sorted.sort_unstable_by(f64::total_cmp);
    let position = (sorted.len() - 1) as f64 * k;
    let a = *sorted
        .get(position.floor() as usize)
        .ok_or(CellError::Num)?;
    let b = *sorted.get(position.ceil() as usize).ok_or(CellError::Num)?;
    let fraction = position - position.floor();
    checked(if (b - a).is_finite() {
        a + (b - a) * fraction
    } else {
        a * (1.0 - fraction) + b * fraction
    })
}

/// Calculate an exclusive percentile and return #NUM! if computation fails.
fn exclusive_percentile(values: &[f64], k: f64) -> Result {
    modern_sample::percentile::<Infallible>(&mut values.to_vec(), k).map_err(|error| match error {
        DistributionError::Spreadsheet(error) => error,
        DistributionError::Operational(never) => match never {},
        DistributionError::NonConvergence | DistributionError::ComputationalLimit => CellError::Num,
    })
}
