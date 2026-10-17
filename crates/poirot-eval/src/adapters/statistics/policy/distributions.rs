//! Normal, Poisson and Student-t scalar protocols.
use super::coercion::{
    coerce_statistics_argument_number, coerce_statistics_boolean,
    coerce_statistics_required_boolean, coerce_statistics_text_number, collapse, work,
};
use super::{EvaluationPort, Failure, StatisticsFunction};
use poirot_functions_statistics as domain;
use poirot_model::CellError;

fn kernel<O>(pinned: bool, result: domain::KernelResult<O>) -> Result<f64, Failure<O>> {
    match result {
        Ok(value) => Ok(value),
        Err(domain::KernelFailure::Spreadsheet(error)) => Err(Failure::Spreadsheet(error)),
        Err(domain::KernelFailure::Operational(error)) => Err(Failure::Operational(error)),
        // Commons throws an unchecked convergence exception. It is not a POI
        // spreadsheet NUM result; preserve an explicit unsupported policy.
        Err(domain::KernelFailure::SourceConvergence(reason)) => {
            use domain::SourceConvergenceFailure::*;
            if pinned {
                let reason = match reason {
                    IterationLimit => poirot_model::CommonsConvergence::IterationLimit,
                    InfiniteContinuedFraction => {
                        poirot_model::CommonsConvergence::InfiniteContinuedFraction
                    }
                    NanContinuedFraction => poirot_model::CommonsConvergence::NanContinuedFraction,
                };
                return Err(Failure::KnownSourceException(
                    poirot_model::KnownSourceException::CommonsConvergence(reason),
                ));
            }
            Err(Failure::UnsupportedCapability(match reason {
                IterationLimit => "Commons Math source iteration limit exceeded",
                InfiniteContinuedFraction => "Commons Math infinite continued fraction",
                NanContinuedFraction => "Commons Math NaN continued fraction",
            }))
        }
    }
}

/// Resolve distribution parameters in source order and run the selected numeric helper.
pub(super) fn execute<P: EvaluationPort>(
    p: &mut P,
    f: StatisticsFunction,
    args: &[P::Argument],
) -> Result<f64, Failure<P::Operational>> {
    use StatisticsFunction::*;
    let result = match f {
        Poisson | PoissonDist => {
            let x = coerce_statistics_argument_number(p, args.first().ok_or(CellError::Value)?)
                .map_err(|e| collapse(e, CellError::Value))?;
            let mean = coerce_statistics_argument_number(p, args.get(1).ok_or(CellError::Value)?)?;
            if x == 0.0 && mean == 0.0 {
                return Ok(1.0);
            }
            if x < 0.0 || mean < 0.0 {
                return Err(CellError::Num.into());
            }
            let c = coerce_statistics_boolean(p, args.get(2).ok_or(CellError::Value)?)?;
            // POI reads cumulative before Commons constructs the distribution.
            // Its unchecked mean-zero constructor exception is not catchable NUM.
            if mean == 0.0 {
                if p.pinned_numeric_outcomes() {
                    return Err(Failure::KnownSourceException(
                        poirot_model::KnownSourceException::PoissonMeanZeroConstructor,
                    ));
                }
                return Err(Failure::UnsupportedCapability(
                    "Commons Math Poisson mean-zero constructor exception",
                ));
            }
            work(p, 512)?;
            return kernel(
                p.pinned_numeric_outcomes(),
                domain::poisson_metered(x, mean, c, &mut |n| p.charge_work(n)),
            );
        }
        NormDist | NormDistLegacy => {
            let x = coerce_statistics_text_number(p, args.first().ok_or(CellError::Value)?)?;
            let m = coerce_statistics_text_number(p, args.get(1).ok_or(CellError::Value)?)?;
            let s = coerce_statistics_text_number(p, args.get(2).ok_or(CellError::Value)?)?;
            if s <= 0.0 {
                return Err(CellError::Num.into());
            }
            let c =
                coerce_statistics_required_boolean(p, args.get(3).ok_or(CellError::Value)?, true)?;
            work(p, 512)?;
            if p.pinned_numeric_outcomes() {
                return kernel(
                    true,
                    domain::normal_dist_source_metered(x, m, s, c, &mut |n| p.charge_work(n)),
                );
            }
            return kernel(
                p.pinned_numeric_outcomes(),
                domain::normal_dist_metered(x, m, s, c, &mut |n| p.charge_work(n)),
            );
        }
        NormInv | NormInvLegacy => {
            let x = coerce_statistics_text_number(p, args.first().ok_or(CellError::Value)?)?;
            if x <= 0.0 || x >= 1.0 {
                return Err(CellError::Num.into());
            }
            let m = coerce_statistics_text_number(p, args.get(1).ok_or(CellError::Value)?)?;
            let s = coerce_statistics_text_number(p, args.get(2).ok_or(CellError::Value)?)?;
            if s <= 0.0 {
                return Err(CellError::Num.into());
            }
            work(p, 512)?;
            if p.pinned_numeric_outcomes() {
                domain::normal_inverse_source(x, m, s)
            } else {
                domain::normal_inverse(x, m, s)
            }
        }
        NormSInv | NormSInvLegacy => {
            let x = coerce_statistics_text_number(p, args.first().ok_or(CellError::Value)?)?;
            if x <= 0.0 || x >= 1.0 {
                return Err(CellError::Num.into());
            }
            work(p, 512)?;
            if p.pinned_numeric_outcomes() {
                domain::normal_inverse_source(x, 0.0, 1.0)
            } else {
                domain::normal_inverse(x, 0.0, 1.0)
            }
        }
        NormSDist | NormSDistLegacy => {
            let x = coerce_statistics_text_number(p, args.first().ok_or(CellError::Value)?)?;
            if x < 0.0 {
                return Err(CellError::Num.into());
            }
            work(p, 512)?;
            if p.pinned_numeric_outcomes() {
                return kernel(
                    true,
                    domain::normal_dist_source_metered(x, 0.0, 1.0, true, &mut |n| {
                        p.charge_work(n)
                    }),
                );
            }
            return kernel(
                p.pinned_numeric_outcomes(),
                domain::normal_dist_metered(x, 0.0, 1.0, true, &mut |n| p.charge_work(n)),
            );
        }
        Standardize => {
            let x = coerce_statistics_text_number(p, args.first().ok_or(CellError::Value)?)?;
            let m = coerce_statistics_text_number(p, args.get(1).ok_or(CellError::Value)?)?;
            let s = coerce_statistics_text_number(p, args.get(2).ok_or(CellError::Value)?)?;
            domain::standardize(x, m, s)
        }
        TDistLegacy | TDist | TDistRt | TDist2T => {
            let x = coerce_statistics_text_number(p, args.first().ok_or(CellError::Value)?)?;
            if matches!(f, TDistLegacy | TDist2T) && x < 0.0 {
                return Err(CellError::Num.into());
            }
            let degrees = coerce_statistics_text_number(p, args.get(1).ok_or(CellError::Value)?)?;
            if (degrees as i32) < 1 {
                return Err(CellError::Num.into());
            }
            work(p, 512)?;
            if f == TDistLegacy {
                let tails = coerce_statistics_text_number(p, args.get(2).ok_or(CellError::Value)?)?;
                domain::legacy_t_dist(x, degrees, tails)
            } else {
                let kind = match f {
                    TDistRt => domain::StudentTKind::RightTail,
                    TDist2T => domain::StudentTKind::TwoTails,
                    _ => {
                        if coerce_statistics_required_boolean(
                            p,
                            args.get(2).ok_or(CellError::Value)?,
                            false,
                        )? {
                            domain::StudentTKind::Cumulative
                        } else {
                            domain::StudentTKind::Density
                        }
                    }
                };
                domain::student_t(x, degrees, kind)
            }
        }
        _ => return Err(CellError::Value.into()),
    };
    result.map_err(Failure::Spreadsheet)
}

/// Excel's modern standard normal accepts both signs and a PDF/CDF selector.
pub(super) fn excel_standard_normal<P: EvaluationPort>(
    p: &mut P,
    args: &[P::Argument],
) -> Result<f64, Failure<P::Operational>> {
    let x = coerce_statistics_argument_number(p, args.first().ok_or(CellError::Value)?)?;
    let cumulative = super::coercion::coerce_statistics_documented_boolean(
        p,
        args.get(1).ok_or(CellError::Value)?,
    )?;
    work(p, 512)?;
    kernel(
        false,
        domain::normal_dist_metered(x, 0.0, 1.0, cumulative, &mut |n| p.charge_work(n)),
    )
}

#[cfg(test)]
mod iteration_policy_tests {
    use super::*;

    #[test]
    fn source_exception_policy_does_not_become_spreadsheet_num() {
        use domain::SourceConvergenceFailure::*;
        for reason in [
            IterationLimit,
            InfiniteContinuedFraction,
            NanContinuedFraction,
        ] {
            let result =
                kernel::<&str>(false, Err(domain::KernelFailure::SourceConvergence(reason)));
            assert!(matches!(result, Err(Failure::UnsupportedCapability(_))));
        }
        assert_eq!(
            kernel(false, Err(domain::KernelFailure::Operational("sentinel"))),
            Err(Failure::Operational("sentinel"))
        );
    }
}
