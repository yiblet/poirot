//! Pure statistical algorithms. Reference traversal, budgets and dispatch belong to adapters.
use poirot_functions_core::CellError;

mod aggregate;
mod collection;
mod commons_gamma;
mod distribution;
mod inverse_erf;
mod iteration;
mod order;
mod paired;
mod percent_rank;
mod poisson_mass;

/// Statistical reduction over already collected numeric operands.
pub use aggregate::aggregate;
/// Origin-sensitive admission of scalar and paired statistical inputs.
pub use collection::{coerce_statistics_collection, coerce_statistics_pairs};
/// Distribution probability/inverse kernels with optional host metering.
pub use distribution::{
    legacy_t_dist, normal_dist, normal_dist_metered, normal_dist_source_metered, normal_inverse,
    normal_inverse_source, poisson, poisson_metered, standardize, student_t,
};
/// Failure channels separating spreadsheet values, host aborts, and source convergence exceptions.
pub use iteration::{KernelFailure, KernelResult, SourceConvergenceFailure};
/// Ordered sample selection, percentile interpolation, and ranking.
pub use order::{order_statistic, percentile, rank};
/// Aligned-pair statistics and fitted-line prediction.
pub use paired::{forecast, paired, regression};
/// Percentile rank with an explicit inclusive/exclusive endpoint contract.
pub use percent_rank::percent_rank;

/// Scalar statistical result or spreadsheet error for an unmetered kernel call.
pub type Result = std::result::Result<f64, CellError>;

/// Text parsing is performed by the adapter's Excel numeric parser, never by this crate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// Numeric input before statistical collection.
    Number(f64),
    /// Boolean input whose admission depends on origin and collection mode.
    Boolean(bool),
    /// Text with optional adapter-parsed numeric interpretation.
    Text {
        /// Numeric interpretation supplied by the adapter, if parsing succeeded.
        numeric: Option<f64>,
    },
    /// Blank input, distinct from an omitted argument.
    Blank,
    /// Omitted argument slot.
    Missing,
    /// Spreadsheet error retained until collection applies error precedence.
    Error(CellError),
}
/// Operand provenance controlling statistical text, boolean, and blank coercion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Direct scalar expression or literal.
    Direct,
    /// Value selected from a worksheet reference.
    Reference,
    /// Member selected from a materialized array.
    Array,
}
/// A statistical value with its direct/reference/array origin. Origin controls
/// whether text, booleans, and blanks are ignored, coerced, or rejected.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Operand {
    /// Uncoerced value admitted by the host.
    pub value: Value,
    /// Provenance used to select collection rules.
    pub origin: Origin,
}
/// Value-admission rules selected before a statistical aggregate runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Collection {
    /// Ordinary numeric aggregate coercion and reference filtering.
    NumericAggregate,
    /// A-family aggregation admitting booleans and text as specified.
    AggregateA,
    /// A-family minimum/maximum collection rules.
    ExtremaA,
    /// Collection rules for geometric mean.
    GeoMean,
    /// Collection rules for most-frequent numeric value.
    Mode,
    /// Collection rules for percentile-rank input.
    PercentRank,
    /// Collection rules for ranking a value within a sample.
    Rank,
}

/// Admission rule for positionally aligned X/Y operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairCollection {
    /// Retain pairs whose members are both numeric.
    NumericOnly,
    /// Apply numeric coercion before retaining aligned pairs.
    NumericCoercion,
}
/// Positionally matched numeric X/Y vectors after pair filtering.
/// `original_len` preserves the pre-filter length for source algorithms whose
/// denominator includes skipped positions.
#[derive(Clone, Debug, PartialEq)]
pub struct NumericPairs {
    /// Filtered X values, positionally aligned with Y.
    pub x: Vec<f64>,
    /// Filtered Y values, positionally aligned with X.
    pub y: Vec<f64>,
    /// Input pair count before filtering.
    pub original_len: usize,
}

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
