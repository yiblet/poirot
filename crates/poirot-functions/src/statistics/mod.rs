//! Statistical calculations for spreadsheet functions.
//!
//! Includes aggregates, distributions, ranks, percentiles, paired samples,
//! forecasting, and regression. Entry points include [`aggregate`](crate::statistics::aggregate), [`percentile`](crate::statistics::percentile),
//! and [`regression`](crate::statistics::regression).
//!
//! The caller collects numeric inputs and selects the relevant sample policy.
//! Iterative calculations report convergence failures separately from spreadsheet
//! errors. Workbook traversal and argument evaluation belong to the caller.
use crate::scalar::CellError;

mod aggregate;
mod grouped;
pub use grouped::{BuiltinAggregator, count_member, median};
mod commons_gamma;
mod distribution;
pub mod ets;
mod inverse_erf;
mod iteration;
pub mod modern;
pub mod modern_inverse;
pub mod modern_sample;
pub mod modern_tails;
mod order;
mod paired;
mod percent_rank;
mod poisson_mass;
mod reduction;

// Statistical reduction over already collected numeric operands.
pub use aggregate::{aggregate, aggregate_with_policy};
pub use reduction::{NumericReduction, NumericReductionState};
// Distribution probability/inverse kernels with optional host metering.
pub use distribution::{
    legacy_t_dist, normal_dist, normal_dist_metered, normal_dist_source_metered, normal_inverse,
    normal_inverse_source, poisson, poisson_metered, standardize, student_t,
};
// Failure channels separating spreadsheet values, host aborts, and source convergence exceptions.
pub use iteration::{KernelFailure, KernelResult, SourceConvergenceFailure};
// Ordered sample selection, percentile interpolation, and ranking.
pub use order::{order_statistic, percentile, rank};
// Aligned-pair statistics and fitted-line prediction.
pub use paired::{forecast, paired, paired_with_policy, regression};
/// Percentile rank with an explicit inclusive/exclusive endpoint contract.
pub use percent_rank::percent_rank;
// Origin-sensitive admission of scalar and paired statistical inputs.
pub use crate::coercion::statistics::{coercev2_statistics_collection, coercev2_statistics_pairs};

/// Scalar statistical result or spreadsheet error for an unmetered kernel call.
pub type Result = std::result::Result<f64, CellError>;

pub use crate::coercion::statistics::{
    Collection, NumericPairs, Operand, Origin, PairCollection, Value,
};

fn checked(value: f64) -> Result {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(CellError::Num)
    }
}
fn finite(values: &[f64]) -> std::result::Result<(), CellError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(CellError::Num)
    }
}

/// Minimum-observation contract, selected independently of value admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamplePolicy {
    /// Preserve the pinned POI/StatsLib minimum sizes and errors.
    PinnedPoi,
    /// Use the documented sample or population denominator requirements.
    ExcelDocumentation,
}

/// Minimum admitted sample size and the spreadsheet error to return when it is not met.
/// Aggregate and paired statistics choose these together to preserve their sample-policy rules.
struct SampleRequirement {
    minimum: usize,
    insufficient: CellError,
}
impl SampleRequirement {
    fn validate(self, admitted: usize) -> std::result::Result<(), CellError> {
        if admitted < self.minimum {
            Err(self.insufficient)
        } else {
            Ok(())
        }
    }
}

/// Statistical reduction selected after operand collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aggregate {
    /// Mean absolute deviation from the sample mean.
    AveDev,
    /// Arithmetic mean.
    Average,
    /// Sum of squared deviations from the mean.
    DevSq,
    /// Geometric mean.
    GeoMean,
    /// Largest admitted value.
    Maximum,
    /// Smallest admitted value.
    Minimum,
    /// Most frequent admitted value.
    Mode,
    /// Variance with a sample denominator.
    VarianceSample,
    /// Variance with a population denominator.
    VariancePopulation,
    /// Square root of sample variance.
    StdevSample,
    /// Square root of population variance.
    StdevPopulation,
}

/// Endpoint convention for percentile rank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PercentRankKind {
    /// Include sample endpoints in the zero-to-one rank interval.
    Inclusive,
    /// Exclude sample endpoints from the zero-to-one rank interval.
    Exclusive,
}

/// Statistical reduction over aligned X/Y pairs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairOperation {
    /// Pearson correlation coefficient.
    Correlation,
    /// Population covariance.
    CovariancePopulation,
    /// Sample covariance.
    CovarianceSample,
    /// Sum each squared X minus squared Y.
    SumX2MinusY2,
    /// Sum each squared X plus squared Y.
    SumX2PlusY2,
    /// Sum squared pairwise differences.
    SumXMinusYSquared,
}

/// Slope and intercept of the fitted line `y = slope * x + intercept`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Regression {
    /// Coefficient multiplying X in the fitted line.
    pub slope: f64,
    /// Fitted Y value at X equal to zero.
    pub intercept: f64,
}

/// Requested Student-t density or probability-tail convention.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StudentTKind {
    /// Probability density at the supplied value.
    Density,
    /// Cumulative probability through the supplied value.
    Cumulative,
    /// Probability above the supplied value.
    RightTail,
    /// Combined probability in both symmetric tails.
    TwoTails,
}
