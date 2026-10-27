//! Scalar parameter lifting for documented modern distributions.
use crate::eval::adapters::{
    elementwise::{self, Broadcast, Kernel, MemberContext, NumericParameter, Variadic},
    operational_only::operational_only,
};
use crate::eval::functions::registry::{FunctionId, StatisticsFunction};
use crate::eval::host::CallHost;
use crate::eval::limits::{CapabilityId, EvalResult};
use crate::eval::session::Session;
use crate::eval::values::{FunctionError, FunctionResult, VALUE_ERROR, Value, err};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellError::{NA, Num};
use crate::model::CellValue::Number;
use crate::model::{CellAddress, CellError, CellValue};
use poirot_functions::statistics::modern::{self as kernel, DistributionKind as Kind};
use poirot_functions::statistics::{modern_inverse as inverse, modern_tails::Tail};

/// Identify the dedicated modern distribution handlers.
pub(super) fn handles(function: StatisticsFunction) -> bool {
    use StatisticsFunction::{
        BetaDist, BetaDistLegacy, BetaInv, BetaInvLegacy, BinomDist, BinomDistLegacy,
        BinomDistRange, BinomInv, BinomInvLegacy, ChisqDist, ChisqDistRt, ChisqDistRtLegacy,
        ChisqInv, ChisqInvRt, ChisqInvRtLegacy, ConfidenceNorm, ConfidenceNormLegacy, ConfidenceT,
        ExponDist, ExponDistLegacy, FDist, FDistRt, FDistRtLegacy, FInv, FInvRt, FInvRtLegacy,
        Gamma, GammaDist, GammaDistLegacy, GammaInv, GammaInvLegacy, Gammaln, GammalnPrecise,
        Gauss, HypgeomDist, HypgeomDistLegacy, LognormDist, LognormDistLegacy, LognormInv,
        LognormInvLegacy, NegbinomDist, NegbinomDistLegacy, Phi, TInv, TInv2T, TInv2TLegacy,
        WeibullDist, WeibullDistLegacy,
    };
    matches!(
        function,
        BetaDistLegacy
            | BetaInvLegacy
            | BinomDistLegacy
            | ChisqDistRtLegacy
            | ChisqInvRtLegacy
            | ConfidenceNormLegacy
            | BinomInvLegacy
            | ExponDistLegacy
            | FDistRtLegacy
            | FInvRtLegacy
            | GammaDistLegacy
            | GammaInvLegacy
            | HypgeomDistLegacy
            | LognormInvLegacy
            | LognormDistLegacy
            | NegbinomDistLegacy
            | TInv2TLegacy
            | WeibullDistLegacy
            | BetaInv
            | BinomInv
            | ChisqInv
            | ChisqInvRt
            | ConfidenceNorm
            | ConfidenceT
            | FInv
            | FInvRt
            | GammaInv
            | LognormInv
            | TInv
            | TInv2T
            | BetaDist
            | BinomDist
            | BinomDistRange
            | ChisqDist
            | ChisqDistRt
            | ExponDist
            | FDist
            | FDistRt
            | Gamma
            | GammaDist
            | Gammaln
            | GammalnPrecise
            | Gauss
            | HypgeomDist
            | LognormDist
            | NegbinomDist
            | Phi
            | WeibullDist
    )
}

/// Map numerical search exhaustion according to the selected worksheet contract.
fn nonconvergence(function: StatisticsFunction) -> CellError {
    if matches!(
        function,
        StatisticsFunction::ChisqInvRt
            | StatisticsFunction::FInvRt
            | StatisticsFunction::GammaInv
            | StatisticsFunction::ChisqInvRtLegacy
            | StatisticsFunction::FInvRtLegacy
            | StatisticsFunction::GammaInvLegacy
            | StatisticsFunction::TInv2TLegacy
    ) {
        NA
    } else {
        Num
    }
}

fn failure<E: Into<FunctionError>>(
    function: StatisticsFunction,
    host: &CallHost<'_, '_>,
    failure: poirot_functions::statistics::DistributionError<E>,
) -> FunctionError {
    match failure {
        poirot_functions::statistics::DistributionError::Spreadsheet(error) => error.into(),
        poirot_functions::statistics::DistributionError::Operational(error) => error.into(),
        poirot_functions::statistics::DistributionError::NonConvergence => nonconvergence(function).into(),
        poirot_functions::statistics::DistributionError::ComputationalLimit => host.unsupported(
            CapabilityId::FunctionAdapter,
            crate::eval::limits::EvaluationReasonError::DistributionComputationExceedsItsBoundedWorkEnvelope,
        ),
    }
}

/// Adapts modern probability functions to broadcast traversal and declared numeric conversion rules.
/// Each output calls an independent distribution algorithm with evaluator work metering.
struct Distribution(StatisticsFunction);

impl Kernel for Distribution {
    type Signature = Variadic<Broadcast>;

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        use StatisticsFunction::{
            BetaDist, BetaDistLegacy, BetaInv, BetaInvLegacy, BinomDist, BinomDistLegacy,
            BinomDistRange, BinomInv, BinomInvLegacy, ChisqDist, ChisqDistRt, ChisqDistRtLegacy,
            ChisqInv, ChisqInvRt, ChisqInvRtLegacy, ConfidenceNorm, ConfidenceNormLegacy,
            ConfidenceT, ExponDist, ExponDistLegacy, FDist, FDistRt, FDistRtLegacy, FInv, FInvRt,
            FInvRtLegacy, Gamma, GammaDist, GammaDistLegacy, GammaInv, GammaInvLegacy, Gammaln,
            GammalnPrecise, Gauss, HypgeomDist, HypgeomDistLegacy, LognormDist, LognormDistLegacy,
            LognormInv, LognormInvLegacy, NegbinomDist, NegbinomDistLegacy, Phi, TInv, TInv2T,
            TInv2TLegacy, WeibullDist, WeibullDistLegacy,
        };
        let function = self.0;
        let parameters: &[NumericParameter] = match function {
            BetaDistLegacy | BetaInv | BetaInvLegacy => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::constant(0.0),
                NumericParameter::constant(1.0),
            ],
            BetaDist => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::boolean(),
                NumericParameter::constant(0.0),
                NumericParameter::constant(1.0),
            ],
            BinomDistRange => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::prior(2),
            ],
            Phi | Gauss | Gamma | Gammaln | GammalnPrecise => &[NumericParameter::required()],
            ChisqInv | ChisqInvRt | ChisqInvRtLegacy | TInv | TInv2T | TInv2TLegacy
            | ChisqDistRt | ChisqDistRtLegacy => {
                &[NumericParameter::required(), NumericParameter::required()]
            }
            ExponDist | ExponDistLegacy | ChisqDist => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::boolean(),
            ],
            BinomDist | BinomDistLegacy | WeibullDist | WeibullDistLegacy | LognormDist
            | GammaDist | GammaDistLegacy | FDist | NegbinomDist => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::boolean(),
            ],
            HypgeomDist => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::boolean(),
            ],
            HypgeomDistLegacy => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
            ],
            GammaInv | GammaInvLegacy => &[
                NumericParameter::nontext(),
                NumericParameter::nontext(),
                NumericParameter::nontext(),
            ],
            _ => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
            ],
        };
        let numbers = arguments.numbers(context, parameters)?;
        let n = |index, _default: Option<f64>| numbers.get(index);
        let selector = |index| numbers.get(index).map(|value| value != 0.0);
        let host = &mut *context.host;
        let operation = match function {
            BetaDistLegacy => BetaDist,
            BetaInvLegacy => BetaInv,
            BinomDistLegacy => BinomDist,
            ChisqDistRtLegacy => ChisqDistRt,
            ChisqInvRtLegacy => ChisqInvRt,
            ConfidenceNormLegacy => ConfidenceNorm,
            BinomInvLegacy => BinomInv,
            ExponDistLegacy => ExponDist,
            FDistRtLegacy => FDistRt,
            FInvRtLegacy => FInvRt,
            GammaDistLegacy => GammaDist,
            GammaInvLegacy => GammaInv,
            HypgeomDistLegacy => HypgeomDist,
            LognormInvLegacy => LognormInv,
            LognormDistLegacy => LognormDist,
            NegbinomDistLegacy => NegbinomDist,
            TInv2TLegacy => TInv2T,
            WeibullDistLegacy => WeibullDist,
            other => other,
        };
        let value = match operation {
            BetaInv => {
                let (p, a, b) = (n(0, None)?, n(1, None)?, n(2, None)?);
                let (lower, upper) = (n(3, Some(0.0))?, n(4, Some(1.0))?);
                inverse::beta(p, a, b, lower, upper, |w| host.charge_work(w))
            }
            BinomInv | FInv | FInvRt | GammaInv | LognormInv | ConfidenceNorm | ConfidenceT => {
                let (p, a, b) = (n(0, None)?, n(1, None)?, n(2, None)?);
                match operation {
                    BinomInv => inverse::binomial(p, a, b, |w| host.charge_work(w)),
                    FInv | FInvRt => {
                        if function == FInvRtLegacy && (a.trunc() >= 1e10 || b.trunc() >= 1e10) {
                            return Err(Num.into());
                        }
                        inverse::fisher_f(
                            p,
                            a,
                            b,
                            if operation == FInv {
                                Tail::Lower
                            } else {
                                Tail::Upper
                            },
                            |w| host.charge_work(w),
                        )
                    }
                    GammaInv => inverse::gamma(p, a, b, Tail::Lower, |w| host.charge_work(w)),
                    LognormInv => inverse::lognormal(p, a, b),
                    _ => inverse::confidence(
                        p,
                        a,
                        b,
                        if operation == ConfidenceT {
                            inverse::ConfidenceKind::Student
                        } else {
                            inverse::ConfidenceKind::Normal
                        },
                        |w| host.charge_work(w),
                    ),
                }
            }
            ChisqInv | ChisqInvRt | TInv | TInv2T => {
                let (p, df) = (n(0, None)?, n(1, None)?);
                if matches!(operation, ChisqInv | ChisqInvRt) {
                    inverse::chi_square(
                        p,
                        df,
                        if operation == ChisqInv {
                            Tail::Lower
                        } else {
                            Tail::Upper
                        },
                        |w| host.charge_work(w),
                    )
                } else {
                    inverse::student_t(p, df, operation == TInv2T, |w| host.charge_work(w))
                }
            }
            Phi => kernel::phi(n(0, None)?),
            Gauss => kernel::gauss(n(0, None)?),
            Gamma => kernel::gamma_value(n(0, None)?),
            Gammaln | GammalnPrecise => kernel::gamma_log(n(0, None)?),
            ExponDist => {
                let (x, rate) = (n(0, None)?, n(1, None)?);
                kernel::exponential(x, rate, selector(2)?)
            }
            WeibullDist | LognormDist | GammaDist => {
                let (x, a, b) = (n(0, None)?, n(1, None)?, n(2, None)?);
                let cumulative = if function == LognormDistLegacy {
                    true
                } else {
                    selector(3)?
                };
                match operation {
                    WeibullDist => kernel::weibull(x, a, b, cumulative),
                    LognormDist => kernel::lognormal(x, a, b, cumulative),
                    _ => kernel::gamma_dist(x, a, b, cumulative, |w| host.charge_work(w)),
                }
            }
            BetaDist => {
                let (x, a, b) = (n(0, None)?, n(1, None)?, n(2, None)?);
                let legacy = function == BetaDistLegacy;
                let cumulative = if legacy { true } else { selector(3)? };
                let lower = n(if legacy { 3 } else { 4 }, Some(0.0))?;
                let upper = n(if legacy { 4 } else { 5 }, Some(1.0))?;
                kernel::beta_dist(x, a, b, cumulative, lower, upper, |w| host.charge_work(w))
            }
            ChisqDist | ChisqDistRt => {
                let (x, df) = (n(0, None)?, n(1, None)?);
                let kind = if operation == ChisqDistRt {
                    Kind::Upper
                } else if selector(2)? {
                    Kind::Lower
                } else {
                    Kind::Density
                };
                kernel::chi_square(x, df, kind, |w| host.charge_work(w))
            }
            FDist | FDistRt => {
                let (x, a, b) = (n(0, None)?, n(1, None)?, n(2, None)?);
                if function == FDistRtLegacy && (a.trunc() >= 1e10 || b.trunc() >= 1e10) {
                    return Err(Num.into());
                }
                let kind = if operation == FDistRt {
                    Kind::Upper
                } else if selector(3)? {
                    Kind::Lower
                } else {
                    Kind::Density
                };
                kernel::fisher_f(x, a, b, kind, |w| host.charge_work(w))
            }
            BinomDist | NegbinomDist => {
                let (k, n, p) = (n(0, None)?, n(1, None)?, n(2, None)?);
                let cumulative = if function == NegbinomDistLegacy {
                    false
                } else {
                    selector(3)?
                };
                if operation == BinomDist {
                    kernel::binomial(k, n, p, cumulative, |w| host.charge_work(w))
                } else {
                    kernel::negative_binomial(k, n, p, cumulative, |w| host.charge_work(w))
                }
            }
            BinomDistRange => {
                let (trials, p, low) = (n(0, None)?, n(1, None)?, n(2, None)?);
                let high = n(3, Some(low))?;
                kernel::binomial_range(trials, p, low, high, |w| host.charge_work(w))
            }
            HypgeomDist => {
                let (k, n, successes, population) =
                    (n(0, None)?, n(1, None)?, n(2, None)?, n(3, None)?);
                let cumulative = if function == HypgeomDistLegacy {
                    false
                } else {
                    selector(4)?
                };
                kernel::hypergeometric(k, n, successes, population, cumulative, |w| {
                    host.charge_work(w)
                })
            }
            _ => return Err(VALUE_ERROR.into()),
        }
        .map_err(|e| failure(function, host, e))?;
        Ok(Number(value))
    }
}

/// Execute modern distribution contracts without altering pinned statistical helpers.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: StatisticsFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !FunctionId::Statistics(function)
        .descriptor()
        .arity
        .accepts(args.len())
    {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    let result = elementwise::execute(&mut host, address, Distribution(function));
    match result {
        Ok(value) => Ok(value),
        Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_student_search_exhaustion_is_na_without_changing_modern_policy() {
        assert_eq!(nonconvergence(StatisticsFunction::TInv2TLegacy), NA);
        assert_eq!(nonconvergence(StatisticsFunction::TInv2T), Num);
        for function in [
            StatisticsFunction::ChisqInvRtLegacy,
            StatisticsFunction::FInvRtLegacy,
            StatisticsFunction::GammaInvLegacy,
        ] {
            assert_eq!(nonconvergence(function), NA);
        }
    }
}
