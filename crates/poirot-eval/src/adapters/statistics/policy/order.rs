//! Aggregate, order-statistic and rank execution with ordered prerequisite reads.
use super::coercionv2::{
    coercev2_statistics_argument_number, coercev2_statistics_floor_i32, collapse, work,
};
use super::collections::coercev2_statistics_collection;
use super::{EvaluationPort, Failure, StatisticsFunction};
use domain::{Aggregate as A, Collection as C};
use poirot_coercionv2::compatibility::{KthRankPolicy, coercev2_statistics_kth_rank};
use poirot_functions_statistics as domain;
use poirot_model::CellError;

/// Execute ranking, percentile, and collection statistics with source coercion rules.
pub(super) fn execute<P: EvaluationPort>(
    p: &mut P,
    f: StatisticsFunction,
    args: &[P::Argument],
    semantics: super::SemanticsPolicy,
) -> Result<f64, Failure<P::Operational>> {
    use StatisticsFunction::*;
    let aggregate = match f {
        AveDev => Some((A::AveDev, C::NumericAggregate)),
        AverageA => Some((A::Average, C::AggregateA)),
        DevSq => Some((A::DevSq, C::NumericAggregate)),
        GeoMean => Some((A::GeoMean, C::GeoMean)),
        MaxA => Some((A::Maximum, C::ExtremaA)),
        MinA => Some((A::Minimum, C::ExtremaA)),
        StdevA => Some((A::StdevSample, C::AggregateA)),
        StdevPA => Some((A::StdevPopulation, C::AggregateA)),
        VarA => Some((A::VarianceSample, C::AggregateA)),
        VarPA => Some((A::VariancePopulation, C::AggregateA)),
        Mode => Some((A::Mode, C::Mode)),
        _ => None,
    };
    if let Some((operation, policy)) = aggregate {
        let v = coercev2_statistics_collection(p, args, policy)?;
        p.check_collection(v.len()).map_err(Failure::Operational)?;
        work(p, (v.len() as u64).saturating_mul(8))?;
        return domain::aggregate_with_policy(
            operation,
            &v,
            match semantics {
                super::SemanticsPolicy::ExcelDocumentation => {
                    domain::SamplePolicy::ExcelDocumentation
                }
                super::SemanticsPolicy::PinnedPoi => domain::SamplePolicy::PinnedPoi,
            },
        )
        .map_err(Failure::Spreadsheet);
    }
    let result = match f {
        Large | Small | Percentile | QuartileInc => {
            // AggregateFunction coerces and range-checks k before reading array.
            let mut k = if f == QuartileInc {
                coercev2_statistics_argument_number(p, args.get(1).ok_or(CellError::Value)?)?
            } else {
                coercev2_statistics_argument_number(p, args.get(1).ok_or(CellError::Value)?)
                    .map_err(|e| collapse(e, CellError::Value))?
            };
            if f == QuartileInc {
                k = k.trunc();
                if !(0.0..=4.0).contains(&k) {
                    return Err(CellError::Num.into());
                }
                k /= 4.0;
            }
            if matches!(f, Percentile | QuartileInc) {
                if !(0.0..=1.0).contains(&k) {
                    return Err(CellError::Num.into());
                }
            } else {
                // SMALL retains its existing admission: Calc's distinct floor rule
                // does not establish that documented SMALL shares LARGE's rule.
                let rank_policy =
                    if f == Large && semantics == super::SemanticsPolicy::ExcelDocumentation {
                        KthRankPolicy::DocumentedLarge
                    } else {
                        KthRankPolicy::PinnedPoi
                    };
                k = coercev2_statistics_kth_rank(k, rank_policy)?;
            }
            let v = coercev2_statistics_collection(
                p,
                args.get(..1).ok_or(CellError::Value)?,
                C::NumericAggregate,
            )?;
            p.check_collection(v.len()).map_err(Failure::Operational)?;
            let n = v.len() as u64;
            work(p, n.saturating_mul(64))?;
            if matches!(f, Percentile | QuartileInc) {
                domain::percentile(&v, k)
            } else {
                domain::order_statistic(&v, k, f == Large)
            }
        }
        Rank | RankEq => {
            let n = coercev2_statistics_argument_number(p, args.first().ok_or(CellError::Value)?)?;
            let order = if args.len() > 2 {
                let order =
                    coercev2_statistics_argument_number(p, args.get(2).ok_or(CellError::Value)?)?;
                if f == Rank {
                    coercev2_statistics_floor_i32(order)? as f64
                } else {
                    order
                }
            } else {
                0.0
            };
            if f == Rank && order != 0.0 && order != 1.0 {
                return Err(CellError::Num.into());
            }
            if !p.is_reference(args.get(1).ok_or(CellError::Value)?)?
                && !(f == RankEq && p.is_array(args.get(1).ok_or(CellError::Value)?)?)
            {
                return Err(CellError::Value.into());
            }
            let v = coercev2_statistics_collection(
                p,
                args.get(1..2).ok_or(CellError::Value)?,
                C::Rank,
            )?;
            work(
                p,
                (v.len() as u64).saturating_mul(if f == RankEq { 2 } else { 1 }),
            )?;
            if f == RankEq && !v.contains(&n) {
                return Err(CellError::NA.into());
            }
            domain::rank(&v, n, order != 0.0)
        }
        PercentRank | PercentRankInc | PercentRankExc => {
            let n = coercev2_statistics_argument_number(p, args.get(1).ok_or(CellError::Value)?)
                .map_err(|e| collapse(e, CellError::Num))?;
            let v = coercev2_statistics_collection(
                p,
                args.get(..1).ok_or(CellError::Value)?,
                C::PercentRank,
            )?;
            if v.is_empty() {
                return Err(CellError::Num.into());
            }
            let significance = if args.len() > 2 {
                coercev2_statistics_floor_i32(coercev2_statistics_argument_number(
                    p,
                    args.get(2).ok_or(CellError::Value)?,
                )?)? as f64
            } else {
                3.0
            };
            if significance < 1.0 {
                return Err(CellError::Num.into());
            }
            p.check_collection(v.len()).map_err(Failure::Operational)?;
            p.check_text(64).map_err(Failure::Operational)?;
            work(p, (v.len() as u64).saturating_mul(8).saturating_add(256))?;
            domain::percent_rank(
                &v,
                n,
                (significance as u32).min(20),
                if f == PercentRankExc {
                    domain::PercentRankKind::Exclusive
                } else {
                    domain::PercentRankKind::Inclusive
                },
            )
        }
        _ => return Err(CellError::Value.into()),
    };
    result.map_err(Failure::Spreadsheet)
}
