//! Shared timeline admission and scalar-slot lifting for additive ETS functions.
use crate::adapters::{
    elementwise::{
        self, Broadcast, Kernel, MemberContext, NumericParameter as P, NumericSignature, Optional,
        WholeCollection, collections,
    },
    failure::operational_only,
};
use crate::host::CallHost;
use crate::*;
use poirot_coercionv2::date_time::CalendarEpoch;
use poirot_functions_statistics::{ets as kernel, modern::Failure};
/// Identify the additive forecasting family independently of legacy FORECAST.
pub(super) fn handles(function: StatisticsFunction) -> bool {
    use StatisticsFunction::*;
    matches!(
        function,
        ForecastEts | ForecastEtsConfint | ForecastEtsSeasonality | ForecastEtsStat
    )
}
fn failure(host: &CallHost<'_, '_>, value: Failure<EvaluationFailure>) -> FunctionFailure {
    match value {
        Failure::Spreadsheet(error) => error.into(),
        Failure::Operational(error) => error.into(),
        Failure::NonConvergence => host.unsupported(
            CapabilityId::FunctionAdapter,
            "bounded ETS optimizer did not converge",
        ),
        Failure::ComputationalLimit => host.unsupported(
            CapabilityId::FunctionAdapter,
            "ETS preparation exceeds its admitted allocation or work envelope",
        ),
    }
}
/// A prepared timeline and the typed scalar signature for an ETS operation.
struct Ets<R> {
    function: StatisticsFunction,
    epoch: CalendarEpoch,
    observed: Vec<(f64, f64)>,
    roles: std::marker::PhantomData<R>,
}
impl<R> Ets<R> {
    /// Bind a forecasting operation before shared signature admission.
    fn new(function: StatisticsFunction, epoch: CalendarEpoch) -> Self {
        Self {
            function,
            epoch,
            observed: Vec::new(),
            roles: std::marker::PhantomData,
        }
    }
}
macro_rules! ets_kernel {
    ($roles:ty, $values:tt, $timeline:tt, [$($slot:tt => $optional:ident),*], [$($parameter:expr),*]) => {
        impl Kernel for Ets<$roles> {
            type Signature = $roles;
            fn prepare(&mut self, host: &mut CallHost<'_, '_>, arguments: &Self::Signature) -> FunctionResult<()> {
                self.observed = collections::points(host, &arguments.$values, &arguments.$timeline)?;
                Ok(())
            }
            fn apply(&self, context: &mut MemberContext<'_, '_, '_>, arguments: &Self::Signature) -> FunctionResult<CellValue> {
                use StatisticsFunction::*;
                let function = self.function;
                let forecasting = matches!(function, ForecastEts | ForecastEtsConfint);
                let numbers = NumericSignature::new(&[$($parameter),*]).numbers_roles(context, [$(ets_kernel!(@role arguments.$slot, $optional)),*])?;
                let n = |index, _default: Option<f64>| numbers.get(index);
                let host = &mut *context.host;
            let target = if forecasting { Some(n(0, None)?) } else { None };
            let selector = if function == ForecastEtsStat {
                n(0, None)?
            } else {
                0.0
            };
            let confidence = if function == ForecastEtsConfint {
                n(1, Some(0.95))?
            } else {
                0.95
            };
            let seasonal_slot = if function == ForecastEtsConfint {
                2
            } else if function == ForecastEtsSeasonality {
                0
            } else {
                1
            };
            let seasonality = if function == ForecastEtsSeasonality {
                kernel::Seasonality::Automatic
            } else {
                kernel::Seasonality::from_number(n(seasonal_slot, Some(1.0))?)?
            };
            let completion_slot = if function == ForecastEtsSeasonality {
                0
            } else {
                seasonal_slot + 1
            };
            let completion = match n(completion_slot, Some(1.0))? {
                0.0 => kernel::Completion::Zero,
                1.0 => kernel::Completion::Interpolate,
                _ => return Err(Num.into()),
            };
            let aggregation = kernel::Aggregation::from_number(n(completion_slot + 1, Some(1.0))?)?;
            if function == ForecastEtsStat
                && (!(1.0..=8.0).contains(&selector) || selector != selector.trunc())
            {
                return Err(Num.into());
            }
            // Preparation can insert at most 3/7 as many missing points as supplied observations.
            let maximum = self.observed
                .len()
                .saturating_mul(10)
                .saturating_div(7)
                .saturating_add(2);
            host.check_values(maximum.saturating_mul(12))?;
            host.charge_work(self.observed.len() as u64)?;
            let mut copied = Vec::new();
            copied
                .try_reserve_exact(self.observed.len())
                .map_err(|_| host.allocation_failure())?;
            // SAFETY: self.observed.len slots reserved for exactly self.observed.len copied pairs.
            copied.extend(self.observed.iter().copied());
            let series = kernel::prepare(copied, aggregation, completion, self.epoch, maximum, |w| {
                host.charge_work(w)
            })
            .map_err(|e| failure(host, e))?;
            // Step is a preparation fact; do not run an optimizer just to obtain it.
            if function == ForecastEtsStat && selector == 8.0 {
                return Ok(Number(series.step()));
            }
            let horizon = target.map(|t| series.horizon(t)).transpose()?;
            let model = kernel::fit(&series, seasonality, |w| host.charge_work(w))
                .map_err(|e| failure(host, e))?;
            let value = match function {
                ForecastEts => model.forecast(horizon.ok_or(VALUE_ERROR)?),
                ForecastEtsConfint => model.confidence(horizon.ok_or(VALUE_ERROR)?, confidence),
                ForecastEtsSeasonality => Ok(model.period() as f64),
                ForecastEtsStat => model.statistic(selector as usize),
                _ => return Err(VALUE_ERROR.into()),
            }
            .map_err(|e| failure(host, e))?;
            Ok(numeric_outcome::coerce_checked_numeric_result(value))
            }
        }
    };
    (@role $role:expr, required) => { Some($role) };
    (@role $role:expr, optional) => { $role.0 };
}
ets_kernel!((Broadcast, WholeCollection, WholeCollection, Optional<Broadcast>, Optional<Broadcast>, Optional<Broadcast>), 1, 2,
    [0 => required, 3 => optional, 4 => optional, 5 => optional],
    [P::required(), P::constant(1.0), P::constant(1.0), P::constant(1.0)]);
ets_kernel!((Broadcast, WholeCollection, WholeCollection, Optional<Broadcast>, Optional<Broadcast>, Optional<Broadcast>, Optional<Broadcast>), 1, 2,
    [0 => required, 3 => optional, 4 => optional, 5 => optional, 6 => optional],
    [P::required(), P::constant(0.95), P::constant(1.0), P::constant(1.0), P::constant(1.0)]);
ets_kernel!((WholeCollection, WholeCollection, Optional<Broadcast>, Optional<Broadcast>), 0, 1,
    [2 => optional, 3 => optional], [P::constant(1.0), P::constant(1.0)]);
ets_kernel!((WholeCollection, WholeCollection, Broadcast, Optional<Broadcast>, Optional<Broadcast>, Optional<Broadcast>), 0, 1,
    [2 => required, 3 => optional, 4 => optional, 5 => optional],
    [P::required(), P::constant(1.0), P::constant(1.0), P::constant(1.0)]);

/// Prepare and fit a genuine AAA model for each lifted scalar setting combination.
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
    session.read(DependencyRead::ContextField(run::ContextRead::DateSystem));
    let epoch = match session.workbook.date_system {
        poirot_model::DateSystem::Excel1900 => CalendarEpoch::Excel1900,
        poirot_model::DateSystem::Excel1904 => CalendarEpoch::Excel1904,
    };
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    let result = match function {
        StatisticsFunction::ForecastEts => elementwise::execute(
            &mut host,
            address,
            Ets::<(
                Broadcast,
                WholeCollection,
                WholeCollection,
                Optional<Broadcast>,
                Optional<Broadcast>,
                Optional<Broadcast>,
            )>::new(function, epoch),
        ),
        StatisticsFunction::ForecastEtsConfint => elementwise::execute(
            &mut host,
            address,
            Ets::<(
                Broadcast,
                WholeCollection,
                WholeCollection,
                Optional<Broadcast>,
                Optional<Broadcast>,
                Optional<Broadcast>,
                Optional<Broadcast>,
            )>::new(function, epoch),
        ),
        StatisticsFunction::ForecastEtsSeasonality => elementwise::execute(
            &mut host,
            address,
            Ets::<(
                WholeCollection,
                WholeCollection,
                Optional<Broadcast>,
                Optional<Broadcast>,
            )>::new(function, epoch),
        ),
        StatisticsFunction::ForecastEtsStat => elementwise::execute(
            &mut host,
            address,
            Ets::<(
                WholeCollection,
                WholeCollection,
                Broadcast,
                Optional<Broadcast>,
                Optional<Broadcast>,
                Optional<Broadcast>,
            )>::new(function, epoch),
        ),
        _ => Err(VALUE_ERROR.into()),
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
