//! Lazy statistical execution over engine-owned references and validated arrays.
/// Operand provenance used by statistical coercion rules.
pub use poirot_functions_core::Origin;
/// Helper-domain scalar representation.
pub use poirot_functions_core::Scalar;
use poirot_model::CellError;
/// Statistical operations implemented through the lazy host port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatisticsFunction {
    /// Average absolute deviation.
    AveDev,
    /// Average including text and logical values under A-family rules.
    AverageA,
    /// Sum of squared deviations.
    DevSq,
    /// Geometric mean.
    GeoMean,
    /// Maximum under A-family coercion rules.
    MaxA,
    /// Minimum under A-family coercion rules.
    MinA,
    /// Sample standard deviation under A-family rules.
    StdevA,
    /// Population standard deviation under A-family rules.
    StdevPA,
    /// Sample variance under A-family rules.
    VarA,
    /// Population variance under A-family rules.
    VarPA,
    /// Most frequent value.
    Mode,
    /// Kth largest value.
    Large,
    /// Kth smallest value.
    Small,
    /// Inclusive percentile.
    Percentile,
    /// Inclusive quartile.
    QuartileInc,
    /// Rank with equal values sharing rank.
    RankEq,
    /// Legacy rank contract.
    Rank,
    /// Legacy percentage rank.
    PercentRank,
    /// Inclusive percentage rank.
    PercentRankInc,
    /// Exclusive percentage rank.
    PercentRankExc,
    /// Correlation coefficient.
    Correl,
    /// Pearson correlation coefficient.
    Pearson,
    /// Legacy population covariance.
    Covar,
    /// Population covariance.
    CovarianceP,
    /// Sample covariance.
    CovarianceS,
    /// Legacy linear forecast.
    Forecast,
    /// Linear regression forecast.
    ForecastLinear,
    /// Regression intercept.
    Intercept,
    /// Regression slope.
    Slope,
    /// Sum of differences of squares.
    SumX2MY2,
    /// Sum of sums of squares.
    SumX2PY2,
    /// Sum of squared differences.
    SumXMY2,
    /// Normal distribution.
    NormDist,
    /// Legacy normal distribution contract.
    NormDistLegacy,
    /// Inverse normal distribution.
    NormInv,
    /// Legacy inverse normal distribution contract.
    NormInvLegacy,
    /// Inverse standard normal distribution.
    NormSInv,
    /// Legacy inverse standard normal contract.
    NormSInvLegacy,
    /// Standard normal distribution.
    NormSDist,
    /// Legacy standard normal distribution contract.
    NormSDistLegacy,
    /// Normalize a value by its mean and standard deviation.
    Standardize,
    /// Legacy Poisson distribution contract.
    Poisson,
    /// Poisson distribution.
    PoissonDist,
    /// Legacy Student's t distribution contract.
    TDistLegacy,
    /// Student's t distribution.
    TDist,
    /// Right-tail Student's t distribution.
    TDistRt,
    /// Two-tail Student's t distribution.
    TDist2T,
}
/// Statistical adapter failure, preserving operational aborts and source exceptions.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure<O> {
    /// Catchable spreadsheet error.
    Spreadsheet(CellError),
    /// Host operational abort.
    Operational(O),
    /// Required host behavior is unavailable.
    UnsupportedCapability(&'static str),
    /// Identified exceptional behavior in the pinned source.
    KnownSourceException(poirot_model::KnownSourceException),
}
impl<O> From<CellError> for Failure<O> {
    fn from(e: CellError) -> Self {
        Self::Spreadsheet(e)
    }
}
/// Shape inspection must not evaluate cells. Indexed access is lazy, sheet-first
/// and row-major; direct scalars have length one. Invalid rectangular arrays are
/// rejected by the engine before exposing a length. All port faults stay typed.
/// Returned owned Text must be checked through check_text BEFORE cloning or
/// allocating in the engine. Collection lengths are checked without cell reads;
/// scalar resolution uses OperandResolver caller-coordinate selection, never
/// eagerly traverses a reference. RefEval sheet selection is source-specific.
pub trait EvaluationPort {
    /// Whether raw non-finite results follow pinned numeric outcome rules.
    fn pinned_numeric_outcomes(&self) -> bool {
        false
    }
    /// Deferred engine-owned operand handle.
    type Argument;
    /// Host operational failure type.
    type Operational;
    /// Resolve only the source-selected scalar.
    fn resolve_scalar(
        &mut self,
        arg: &Self::Argument,
    ) -> Result<Scalar, Failure<Self::Operational>>;
    /// Inspect collection length without traversing its members.
    fn collection_len(&mut self, arg: &Self::Argument)
    -> Result<usize, Failure<Self::Operational>>;
    /// Read one member and its coercion provenance in source traversal order.
    fn collection_cell(
        &mut self,
        arg: &Self::Argument,
        index: usize,
    ) -> Result<(Scalar, Origin), Failure<Self::Operational>>;
    /// Admit work before performing it.
    fn charge_work(&mut self, units: u64) -> Result<(), Self::Operational>;
    /// Check collection capacity before allocation.
    fn check_collection(&mut self, elements: usize) -> Result<(), Self::Operational>;
    /// Check text capacity before copying or allocating.
    fn check_text(&mut self, bytes: usize) -> Result<(), Self::Operational>;
    /// Engine Excel numeric grammar/date-time fallback, after a text budget check.
    fn coercev2_text_number(&mut self, text: &str) -> Result<f64, Failure<Self::Operational>>;
    /// Apply explicit omitted-argument numeric coercion or reject the capability.
    fn coercev2_missing_number(&mut self) -> Result<f64, Failure<Self::Operational>> {
        Err(Failure::UnsupportedCapability(
            "MissingArgEval numeric coercion",
        ))
    }
    /// RANK accepts references/areas/reference lists, but no direct/array literal.
    fn is_reference(&mut self, arg: &Self::Argument) -> Result<bool, Failure<Self::Operational>>;
    /// Inspect a validated array operand without traversing its members.
    /// Defaults preserve ports that only support pinned reference collections.
    fn is_array(&mut self, _arg: &Self::Argument) -> Result<bool, Failure<Self::Operational>> {
        Ok(false)
    }
    /// MODE area members use Array origin (TwoDEval); singleton refs use Reference.
    /// Other collections preserve Reference origin for area members.
    fn is_area(&mut self, arg: &Self::Argument) -> Result<bool, Failure<Self::Operational>>;
}
mod coercionv2;
mod collections;
mod distributions;
mod order;
mod paired;

/// Select the documented Excel contract without changing the pinned entry point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticsPolicy {
    /// Pinned Apache POI behavior.
    PinnedPoi,
    /// Documented Excel behavior.
    ExcelDocumentation,
}
impl StatisticsFunction {
    /// Check pinned argument-count rules without evaluating operands.
    pub const fn accepts(self, n: usize) -> bool {
        use StatisticsFunction::*;
        match self {
            AveDev | AverageA | DevSq | GeoMean | MaxA | MinA | StdevA | StdevPA | VarA | VarPA => {
                n >= 1 && n <= 255
            }
            Mode => n >= 1,
            PercentRank | PercentRankInc | PercentRankExc => n >= 2,
            Rank | RankEq => n == 2 || n == 3,
            Large | Small | Percentile | QuartileInc | Correl | Pearson | Covar | CovarianceP
            | CovarianceS | Intercept | Slope | SumX2MY2 | SumX2PY2 | SumXMY2 | TDistRt
            | TDist2T => n == 2,
            NormSDist | NormSDistLegacy | NormSInv | NormSInvLegacy => n == 1,
            NormDist | NormDistLegacy => n == 4,
            _ => n == 3,
        }
    }
    /// Check argument-count rules under the selected compatibility policy.
    pub const fn accepts_with_policy(self, n: usize, policy: SemanticsPolicy) -> bool {
        if matches!(policy, SemanticsPolicy::ExcelDocumentation) && matches!(self, Self::NormSDist)
        {
            n == 2
        } else {
            self.accepts(n)
        }
    }
}
/// Execute a statistical operation under pinned compatibility rules.
pub fn evaluate<P: EvaluationPort>(
    p: &mut P,
    f: StatisticsFunction,
    args: &[P::Argument],
) -> Result<f64, Failure<P::Operational>> {
    evaluate_with_policy(p, f, args, SemanticsPolicy::PinnedPoi)
}
/// Execute lazy statistical operands under explicit compatibility rules.
pub fn evaluate_with_policy<P: EvaluationPort>(
    p: &mut P,
    f: StatisticsFunction,
    args: &[P::Argument],
    policy: SemanticsPolicy,
) -> Result<f64, Failure<P::Operational>> {
    use StatisticsFunction::*;
    if !f.accepts_with_policy(args.len(), policy) {
        return Err(CellError::Value.into());
    }
    coercionv2::work(p, 1)?;
    if f == NormSDist && policy == SemanticsPolicy::ExcelDocumentation {
        return distributions::excel_standard_normal(p, args);
    }
    match f {
        AveDev | AverageA | DevSq | GeoMean | MaxA | MinA | StdevA | StdevPA | VarA | VarPA
        | Mode | Large | Small | Percentile | QuartileInc | Rank | RankEq | PercentRank
        | PercentRankInc | PercentRankExc => order::execute(
            p,
            if f == Rank && policy == SemanticsPolicy::ExcelDocumentation {
                RankEq
            } else {
                f
            },
            args,
        ),
        Correl | Pearson | Covar | CovarianceP | CovarianceS | Forecast | ForecastLinear
        | Intercept | Slope | SumX2MY2 | SumX2PY2 | SumXMY2 => paired::execute(p, f, args, policy),
        NormDist | NormDistLegacy | NormInv | NormInvLegacy | NormSInv | NormSInvLegacy
        | NormSDist | NormSDistLegacy | Standardize | Poisson | PoissonDist | TDistLegacy
        | TDist | TDistRt | TDist2T => distributions::execute(p, f, args),
    }
}

#[cfg(test)]
#[path = "policy/tests.rs"]
mod adapter_tests;
