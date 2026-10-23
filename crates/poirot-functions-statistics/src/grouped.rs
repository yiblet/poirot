//! Shared aggregation identities and kernels for independently selected subsets.
use crate::{Aggregate, NumericReduction, NumericReductionState, Result, SamplePolicy};
use poirot_functions_core::CellError;

/// Built-in reductions admitted as contextual grouping handles, without callable values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinAggregator {
    /// Sum of numeric members.
    Sum,
    /// Sum of squared numeric members.
    SumSq,
    /// Arithmetic mean.
    Average,
    /// Arithmetic mean admitting text and logical members under A-family rules.
    AverageA,
    /// Smallest value under A-family admission.
    MinimumA,
    /// Largest value under A-family admission.
    MaximumA,
    /// Sample standard deviation under A-family admission.
    StdevA,
    /// Population standard deviation under A-family admission.
    StdevPA,
    /// Sample variance under A-family admission.
    VarA,
    /// Population variance under A-family admission.
    VarPA,
    /// Smallest numeric member.
    Minimum,
    /// Largest numeric member.
    Maximum,
    /// Product of numeric members.
    Product,
    /// Number of numeric members.
    Count,
    /// Number of nonblank members, including errors and empty text.
    CountA,
    /// Number of blank or empty-text members.
    CountBlank,
    /// Middle ordered numeric observation.
    Median,
    /// Sample standard deviation.
    StdevSample,
    /// Population standard deviation.
    StdevPopulation,
    /// Sample variance.
    VarianceSample,
    /// Population variance.
    VariancePopulation,
    /// Mean absolute deviation.
    AveDev,
    /// Squared deviations from the mean.
    DevSq,
    /// Geometric mean.
    GeoMean,
    /// Most frequent numeric observation.
    Mode,
    /// Text concatenation in source order.
    Concat,
    /// Sum of the subset divided by the independently supplied total set.
    PercentOf,
}
impl BuiltinAggregator {
    /// Resolve the explicit supported ledger of built-in aggregation spellings.
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.strip_prefix("_xlfn.").unwrap_or(name);
        let entries = [
            ("AVERAGEA", Self::AverageA),
            ("MINA", Self::MinimumA),
            ("MAXA", Self::MaximumA),
            ("STDEVA", Self::StdevA),
            ("STDEVPA", Self::StdevPA),
            ("VARA", Self::VarA),
            ("VARPA", Self::VarPA),
            ("SUM", Self::Sum),
            ("SUMSQ", Self::SumSq),
            ("AVERAGE", Self::Average),
            ("MIN", Self::Minimum),
            ("MAX", Self::Maximum),
            ("PRODUCT", Self::Product),
            ("COUNT", Self::Count),
            ("COUNTA", Self::CountA),
            ("COUNTBLANK", Self::CountBlank),
            ("MEDIAN", Self::Median),
            ("STDEV", Self::StdevSample),
            ("STDEV.S", Self::StdevSample),
            ("STDEVP", Self::StdevPopulation),
            ("STDEV.P", Self::StdevPopulation),
            ("VAR", Self::VarianceSample),
            ("VAR.S", Self::VarianceSample),
            ("VARP", Self::VariancePopulation),
            ("VAR.P", Self::VariancePopulation),
            ("AVEDEV", Self::AveDev),
            ("DEVSQ", Self::DevSq),
            ("GEOMEAN", Self::GeoMean),
            ("MODE", Self::Mode),
            ("MODE.SNGL", Self::Mode),
            ("CONCAT", Self::Concat),
            ("PERCENTOF", Self::PercentOf),
        ];
        entries
            .into_iter()
            .find_map(|(n, operation)| name.eq_ignore_ascii_case(n).then_some(operation))
    }
    /// Canonical label used for generated aggregation headers.
    pub fn name(self) -> &'static str {
        match self {
            Self::AverageA => "AVERAGEA",
            Self::MinimumA => "MINA",
            Self::MaximumA => "MAXA",
            Self::StdevA => "STDEVA",
            Self::StdevPA => "STDEVPA",
            Self::VarA => "VARA",
            Self::VarPA => "VARPA",
            Self::Sum => "SUM",
            Self::SumSq => "SUMSQ",
            Self::Average => "AVERAGE",
            Self::Minimum => "MIN",
            Self::Maximum => "MAX",
            Self::Product => "PRODUCT",
            Self::Count => "COUNT",
            Self::CountA => "COUNTA",
            Self::CountBlank => "COUNTBLANK",
            Self::Median => "MEDIAN",
            Self::StdevSample => "STDEV.S",
            Self::StdevPopulation => "STDEV.P",
            Self::VarianceSample => "VAR.S",
            Self::VariancePopulation => "VAR.P",
            Self::AveDev => "AVEDEV",
            Self::DevSq => "DEVSQ",
            Self::GeoMean => "GEOMEAN",
            Self::Mode => "MODE.SNGL",
            Self::Concat => "CONCAT",
            Self::PercentOf => "PERCENTOF",
        }
    }
    /// Choose the existing origin-sensitive admission contract before reduction.
    pub fn collection(self) -> crate::Collection {
        match self {
            Self::AverageA | Self::StdevA | Self::StdevPA | Self::VarA | Self::VarPA => {
                crate::Collection::AggregateA
            }
            Self::MinimumA | Self::MaximumA => crate::Collection::ExtremaA,
            Self::GeoMean => crate::Collection::GeoMean,
            Self::Mode => crate::Collection::NumericAggregate,
            _ => crate::Collection::NumericAggregate,
        }
    }
    /// Reduce numeric members already admitted by an origin-sensitive caller.
    pub fn numeric(self, values: &mut [f64]) -> Result {
        use NumericReduction as R;
        let reduction = match self {
            Self::Sum | Self::PercentOf => Some(R::Sum),
            Self::SumSq => Some(R::SumSq),
            Self::Average | Self::AverageA => Some(R::Average),
            Self::Minimum | Self::MinimumA => Some(R::Minimum),
            Self::Maximum | Self::MaximumA => Some(R::Maximum),
            Self::Product => Some(R::Product),
            _ => None,
        };
        if let Some(operation) = reduction {
            let mut state = NumericReductionState::new(operation);
            for &value in values.iter() {
                state.push(value);
            }
            return state
                .finish()
                .ok_or(CellError::Div0)
                .and_then(crate::checked);
        }
        let operation = match self {
            Self::Median => return median(values),
            Self::StdevSample | Self::StdevA => Aggregate::StdevSample,
            Self::StdevPopulation | Self::StdevPA => Aggregate::StdevPopulation,
            Self::VarianceSample | Self::VarA => Aggregate::VarianceSample,
            Self::VariancePopulation | Self::VarPA => Aggregate::VariancePopulation,
            Self::AveDev => Aggregate::AveDev,
            Self::DevSq => Aggregate::DevSq,
            Self::GeoMean => Aggregate::GeoMean,
            Self::Mode => Aggregate::Mode,
            _ => return Err(CellError::Value),
        };
        crate::aggregate_with_policy(operation, values, SamplePolicy::ExcelDocumentation)
    }
}
/// Select the median after host admission, sharing the ordinary aggregate kernel.
pub fn median(values: &mut [f64]) -> Result {
    let length = values.len();
    if length == 0 {
        return Err(CellError::Num);
    }
    // SAFETY: f64::total_cmp is a fixed total order with no caller-supplied comparator.
    values.sort_by(f64::total_cmp);
    let high = *values.get(length / 2).ok_or(CellError::Num)?;
    if length.is_multiple_of(2) {
        let low = *values.get(length / 2 - 1).ok_or(CellError::Num)?;
        crate::checked(low / 2.0 + high / 2.0)
    } else {
        crate::checked(high)
    }
}
/// Count a member from admitted presence facts, shared by ordinary and grouped consumers.
pub fn count_member(
    operation: BuiltinAggregator,
    numeric: bool,
    nonblank: bool,
    empty: bool,
) -> u64 {
    u64::from(match operation {
        BuiltinAggregator::Count => numeric,
        BuiltinAggregator::CountA => nonblank,
        BuiltinAggregator::CountBlank => !nonblank || empty,
        _ => false,
    })
}
