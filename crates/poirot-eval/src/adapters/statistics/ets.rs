//! Shared timeline admission and scalar-slot lifting for additive ETS functions.
use crate::adapters::{elementwise::Lift, failure::operational_only};
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
fn failure(host: &CallHost<'_, '_>, value: Failure<FunctionFailure>) -> FunctionFailure {
    match value {
        Failure::Spreadsheet(error) => error.into(),
        Failure::Operational(error) => error,
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
/// Collect the original paired geometry without independently filtering either axis.
fn points(
    host: &mut CallHost<'_, '_>,
    values: usize,
    timeline: usize,
) -> FunctionResult<Vec<(f64, f64)>> {
    let a = host.describe(values)?;
    let b = host.describe(timeline)?;
    if a.sheets != 1 || b.sheets != 1 {
        return Err(VALUE_ERROR.into());
    }
    if a.rows != b.rows || a.columns != b.columns {
        return Err(NA.into());
    }
    let count = host.collection_len(values)?;
    host.check_values(count.saturating_mul(12))?;
    host.charge_work((count as u64).saturating_mul(8))?;
    let mut points = Vec::new();
    points
        .try_reserve_exact(count)
        .map_err(|_| host.allocation_failure())?;
    for offset in 0..count {
        let value = host.collection_cell(values, offset)?;
        let time = host.collection_cell(timeline, offset)?;
        if let Error(error) = value.value {
            return Err(error.into());
        }
        if let Error(error) = time.value {
            return Err(error.into());
        }
        let (Number(value), Number(time)) = (value.value, time.value) else {
            return Err(VALUE_ERROR.into());
        };
        // SAFETY: count slots reserved and the loop emits exactly count aligned pairs.
        points.push((time, value));
    }
    Ok(points)
}
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
    let result = (|| {
        use StatisticsFunction::*;
        let forecasting = matches!(function, ForecastEts | ForecastEtsConfint);
        let base = usize::from(forecasting);
        let observed = points(&mut host, base, base.checked_add(1).ok_or(VALUE_ERROR)?)?;
        let selected: &[usize] = match function {
            ForecastEts => &[0, 3, 4, 5],
            ForecastEtsConfint => &[0, 3, 4, 5, 6],
            ForecastEtsSeasonality => &[2, 3],
            ForecastEtsStat => &[2, 3, 4, 5],
            _ => return Err(VALUE_ERROR.into()),
        };
        let mut actual = poirot_functions_core::ParamVec::new();
        actual
            .try_reserve(selected.len())
            .map_err(|_| host.allocation_failure())?;
        // SAFETY: selected.len slots reserved for at most selected.len present positions.
        actual.extend(selected.iter().copied().filter(|i| *i < args.len()));
        let lift = Lift::new_selected(&mut host, &actual)?;
        lift.evaluate(&mut host, address, |host, row, column| {
            let mut n = |index, default| lift.number(host, index, row, column, default);
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
            let maximum = observed
                .len()
                .saturating_mul(10)
                .saturating_div(7)
                .saturating_add(2);
            host.check_values(maximum.saturating_mul(12))?;
            host.charge_work(observed.len() as u64)?;
            let mut copied = Vec::new();
            copied
                .try_reserve_exact(observed.len())
                .map_err(|_| host.allocation_failure())?;
            // SAFETY: observed.len slots reserved for exactly observed.len copied pairs.
            copied.extend(observed.iter().copied());
            let series = kernel::prepare(copied, aggregation, completion, epoch, maximum, |w| {
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
        })
    })();
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
