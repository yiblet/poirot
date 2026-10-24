//! Modern sample contracts with fixed collections and separately lifted scalar slots.
use crate::adapters::{elementwise::Lift, failure::operational_only};
use crate::host::{CallHost, NumericPolicy, OperandKind, OperandOrigin};
use crate::*;
use poirot_functions_statistics::{modern::Failure, modern_sample as kernel};

/// Identify the modern descriptive-statistics and hypothesis-test handlers.
pub(super) fn handles(function: StatisticsFunction) -> bool {
    use StatisticsFunction::*;
    matches!(
        function,
        ChisqTestLegacy
            | FTestLegacy
            | TTestLegacy
            | ZTestLegacy
            | ChisqTest
            | FTest
            | Fisher
            | FisherInv
            | HarMean
            | Kurt
            | ModeMult
            | ModeSngl
            | PercentileExc
            | Permut
            | PermutationA
            | Prob
            | QuartileExc
            | RankAvg
            | RSq
            | Skew
            | SkewP
            | Steyx
            | TTest
            | TrimMean
            | ZTest
    )
}
/// Numeric admission chosen independently from positional traversal.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Admission {
    /// Count direct logical/numeric text, ignore them within arrays and references.
    Aggregate,
    /// Retain only actual numbers, preserving an excluded position as None.
    Numeric,
    /// Require every position to be numeric rather than silently changing a table.
    Strict,
}
fn member(
    host: &mut CallHost<'_, '_>,
    index: usize,
    offset: usize,
    admission: Admission,
) -> FunctionResult<Option<f64>> {
    let operand = host.collection_cell(index, offset)?;
    if operand.missing {
        return Err(VALUE_ERROR.into());
    }
    match &operand.value {
        Error(error) => return Err((*error).into()),
        Number(value) => return Ok(Some(*value)),
        _ => {}
    }
    if admission == Admission::Strict {
        return Err(VALUE_ERROR.into());
    }
    let direct =
        operand.origin == OperandOrigin::Direct || host.kind(index)? == OperandKind::Scalar;
    if admission == Admission::Aggregate && direct {
        return match operand.value {
            Blank => Ok(None),
            value => host
                .coercev2_scalar_number(&value, NumericPolicy::PinnedPoi)
                .map(Some),
        };
    }
    Ok(None)
}
/// Charge and collect one operand without erasing excluded pair positions.
fn positions(
    host: &mut CallHost<'_, '_>,
    index: usize,
    admission: Admission,
) -> FunctionResult<Vec<Option<f64>>> {
    let len = host.collection_len(index)?;
    host.check_values(len)?;
    host.charge_work((len as u64).saturating_mul(8))?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(len)
        .map_err(|_| host.allocation_failure())?;
    for offset in 0..len {
        let value = member(host, index, offset, admission)?;
        // SAFETY: len slots were reserved and the loop emits exactly len entries.
        result.push(value);
    }
    Ok(result)
}
/// Gather one or more samples, charging before copying their admitted members.
fn collect(
    host: &mut CallHost<'_, '_>,
    indices: impl Iterator<Item = usize>,
    admission: Admission,
) -> FunctionResult<Vec<f64>> {
    let mut result = Vec::new();
    for index in indices {
        let len = host.collection_len(index)?;
        let total = result
            .len()
            .checked_add(len)
            .ok_or_else(|| host.size_overflow())?;
        host.check_values(total)?;
        host.charge_work((len as u64).saturating_mul(12))?;
        result
            .try_reserve(len)
            .map_err(|_| host.allocation_failure())?;
        for offset in 0..len {
            if let Some(value) = member(host, index, offset, admission)? {
                // SAFETY: len slots were reserved for at most len admitted members.
                result.push(value);
            }
        }
    }
    Ok(result)
}
/// Preserve positional associations while excluding a pair containing a nonnumber.
fn pairs(
    host: &mut CallHost<'_, '_>,
    admission: Admission,
) -> FunctionResult<(Vec<f64>, Vec<f64>)> {
    let a_len = host.collection_len(0)?;
    let b_len = host.collection_len(1)?;
    if a_len != b_len {
        return Err(NA.into());
    }
    host.check_values(a_len.saturating_mul(4))?;
    let a = positions(host, 0, admission)?;
    let b = positions(host, 1, admission)?;
    let (mut x, mut y) = (Vec::new(), Vec::new());
    x.try_reserve_exact(a_len)
        .map_err(|_| host.allocation_failure())?;
    y.try_reserve_exact(a_len)
        .map_err(|_| host.allocation_failure())?;
    for (a, b) in a.into_iter().zip(b) {
        if let (Some(a), Some(b)) = (a, b) {
            // SAFETY: a_len slots were reserved and zip emits at most a_len pairs.
            x.push(a);
            // SAFETY: a_len slots were reserved and zip emits at most a_len pairs.
            y.push(b);
        }
    }
    Ok((x, y))
}
fn failure<E: Into<FunctionFailure>>(
    host: &CallHost<'_, '_>,
    failure: Failure<E>,
) -> FunctionFailure {
    match failure {
        Failure::Spreadsheet(error) => error.into(),
        Failure::Operational(error) => error.into(),
        Failure::NonConvergence => Num.into(),
        Failure::ComputationalLimit => host.unsupported(
            CapabilityId::FunctionAdapter,
            "sample statistics exceeds bounded numerical work",
        ),
    }
}
/// Budget sorting, mode storage and repeated scalar computations before work starts.
fn processing(host: &mut CallHost<'_, '_>, len: usize) -> EvalResult<()> {
    host.check_values(len.saturating_mul(3))?;
    host.charge_work((len as u64).saturating_mul(16 + u64::from(usize::BITS - len.leading_zeros())))
}
/// Execute modern samples with all collection reads outside scalar-parameter lifting.
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
    let function = match function {
        StatisticsFunction::ChisqTestLegacy => StatisticsFunction::ChisqTest,
        StatisticsFunction::FTestLegacy => StatisticsFunction::FTest,
        StatisticsFunction::TTestLegacy => StatisticsFunction::TTest,
        StatisticsFunction::ZTestLegacy => StatisticsFunction::ZTest,
        other => other,
    };
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    let result = (|| {
        use StatisticsFunction::*;
        if matches!(
            function,
            HarMean | Kurt | Skew | SkewP | ModeSngl | ModeMult
        ) {
            let mut values = collect(&mut host, 0..args.len(), Admission::Aggregate)?;
            processing(&mut host, values.len())?;
            if matches!(function, ModeSngl | ModeMult) {
                let count = kernel::modes(
                    &mut values,
                    FunctionFailure::from(host.allocation_failure()),
                )
                .map_err(|e| failure(&host, e))?;
                if function == ModeSngl {
                    return Ok(Number(*values.first().ok_or(NA)?).into());
                }
                host.check_values(count)?;
                host.charge_work(count as u64)?;
                let mut rows = Vec::new();
                rows.try_reserve_exact(count)
                    .map_err(|_| host.allocation_failure())?;
                for &value in values.iter().take(count) {
                    let mut row = Vec::new();
                    row.try_reserve_exact(1)
                        .map_err(|_| host.allocation_failure())?;
                    // SAFETY: one slot was reserved immediately above.
                    row.push(Number(value));
                    // SAFETY: count rows were reserved and take(count) emits at most count rows.
                    rows.push(row);
                }
                return Ok(Value::Array(ArrayView::anchored(rows, Some(address))));
            }
            let value = if function == HarMean {
                kernel::harmonic_mean::<EvaluationFailure>(&values)
            } else {
                kernel::moment::<EvaluationFailure>(
                    &values,
                    match function {
                        Kurt => kernel::MomentKind::Kurtosis,
                        Skew => kernel::MomentKind::SkewSample,
                        _ => kernel::MomentKind::SkewPopulation,
                    },
                )
            }
            .map_err(|e| failure(&host, e))?;
            return Ok(Number(value).into());
        }
        if matches!(function, RSq | Steyx | ChisqTest | FTest) {
            let value = match function {
                RSq | Steyx => {
                    let (y, x) = pairs(&mut host, Admission::Aggregate)?;
                    processing(&mut host, x.len())?;
                    kernel::paired(
                        &y,
                        &x,
                        if function == RSq {
                            kernel::PairKind::RSquared
                        } else {
                            kernel::PairKind::StandardError
                        },
                    )
                }
                ChisqTest => {
                    let (a, b) = (host.describe(0)?, host.describe(1)?);
                    if a.sheets != 1 || b.sheets != 1 || a.rows != b.rows || a.columns != b.columns
                    {
                        return Err(NA.into());
                    }
                    let observed = collect(&mut host, 0..1, Admission::Strict)?;
                    host.check_values(observed.len().saturating_mul(2))?;
                    let expected = collect(&mut host, 1..2, Admission::Strict)?;
                    processing(&mut host, observed.len())?;
                    kernel::chi_square_test(&observed, &expected, a.rows, a.columns, |w| {
                        host.charge_work(w)
                    })
                }
                _ => {
                    let x = collect(&mut host, 0..1, Admission::Numeric)?;
                    let y_len = host.collection_len(1)?;
                    host.check_values(x.len().saturating_add(y_len))?;
                    let y = collect(&mut host, 1..2, Admission::Numeric)?;
                    processing(&mut host, x.len().saturating_add(y.len()))?;
                    kernel::f_test(&x, &y, |w| host.charge_work(w))
                }
            }
            .map_err(|e| failure(&host, e))?;
            return Ok(Number(value).into());
        }
        let lift = match function {
            Fisher | FisherInv | Permut | PermutationA => Lift::new(&mut host, args.len())?,
            RankAvg => Lift::new_selected(&mut host, if args.len() == 3 { &[0, 2] } else { &[0] })?,
            TTest | Prob => {
                Lift::new_at(&mut host, 2, args.len().checked_sub(2).ok_or(VALUE_ERROR)?)?
            }
            _ => Lift::new_at(&mut host, 1, args.len().checked_sub(1).ok_or(VALUE_ERROR)?)?,
        };
        // Keep collections fixed while selectors/bounds broadcast. T.TEST's type
        // selects paired admission per output member, so retain both positions.
        let mut values = match function {
            PercentileExc | QuartileExc | TrimMean | ZTest => {
                collect(&mut host, 0..1, Admission::Numeric)?
            }
            RankAvg => collect(&mut host, 1..2, Admission::Numeric)?,
            _ => Vec::new(),
        };
        let positional = if function == TTest {
            let a = positions(&mut host, 0, Admission::Numeric)?;
            let b_len = host.collection_len(1)?;
            host.check_values(a.len().saturating_add(b_len).saturating_mul(2))?;
            let b = positions(&mut host, 1, Admission::Numeric)?;
            Some((a, b))
        } else {
            None
        };
        let mass = if function == Prob {
            let a = collect(&mut host, 0..1, Admission::Strict)?;
            let b_len = host.collection_len(1)?;
            host.check_values(a.len().saturating_add(b_len))?;
            let b = collect(&mut host, 1..2, Admission::Strict)?;
            Some((a, b))
        } else {
            None
        };
        lift.evaluate(&mut host, address, |host, row, column| {
            let mut n = |index, default| lift.number(host, index, row, column, default);
            let value = match function {
                Fisher | FisherInv => kernel::fisher(n(0, None)?, function == FisherInv),
                Permut | PermutationA => {
                    let (a, b) = (n(0, None)?, n(1, None)?);
                    host.charge_work(1025)?;
                    kernel::permutations(a, b, function == PermutationA)
                }
                PercentileExc | QuartileExc | TrimMean => {
                    let parameter = n(0, None)?;
                    processing(host, values.len())?;
                    match function {
                        TrimMean => kernel::trimmed_mean(&mut values, parameter),
                        QuartileExc => {
                            let quart = parameter.trunc();
                            if !(1.0..=3.0).contains(&quart) {
                                return Err(Num.into());
                            }
                            kernel::percentile(&mut values, quart * 0.25)
                        }
                        _ => kernel::percentile(&mut values, parameter),
                    }
                }
                RankAvg => {
                    let (target, order) = (n(0, None)?, n(1, Some(0.0))?);
                    processing(host, values.len())?;
                    kernel::rank_average(&values, target, order != 0.0)
                }
                ZTest => {
                    let hypothesis = n(0, None)?;
                    let sigma = if args.len() < 3 || lift.member(host, 1, row, column)?.missing {
                        None
                    } else {
                        Some(lift.number(host, 1, row, column, None)?)
                    };
                    processing(host, values.len())?;
                    kernel::z_test(&values, hypothesis, sigma)
                }
                Prob => {
                    let lower = n(0, Some(0.0))?;
                    let upper = n(1, Some(lower))?;
                    let (x, p) = mass.as_ref().ok_or(VALUE_ERROR)?;
                    processing(host, x.len())?;
                    kernel::probability(x, p, lower, upper)
                }
                TTest => {
                    let (tail, kind) = (n(0, None)?.trunc(), n(1, None)?.trunc());
                    if tail != 1.0 && tail != 2.0 || !(1.0..=3.0).contains(&kind) {
                        return Err(Num.into());
                    }
                    let (a, b) = positional.as_ref().ok_or(VALUE_ERROR)?;
                    if kind == 1.0 && a.len() != b.len() {
                        return Err(NA.into());
                    }
                    processing(host, a.len().saturating_add(b.len()))?;
                    let (mut x, mut y) = (Vec::new(), Vec::new());
                    x.try_reserve_exact(a.len())
                        .map_err(|_| host.allocation_failure())?;
                    y.try_reserve_exact(b.len())
                        .map_err(|_| host.allocation_failure())?;
                    if kind == 1.0 {
                        for (a, b) in a.iter().zip(b) {
                            if let (Some(a), Some(b)) = (a, b) {
                                // SAFETY: a.len slots reserved; zip emits at most that many.
                                x.push(*a);
                                // SAFETY: b.len slots reserved; zip emits at most that many.
                                y.push(*b);
                            }
                        }
                    } else {
                        // SAFETY: exact full original lengths reserved, and flatten emits no more.
                        x.extend(a.iter().flatten().copied());
                        // SAFETY: exact full original lengths reserved, and flatten emits no more.
                        y.extend(b.iter().flatten().copied());
                    }
                    kernel::t_test(
                        &x,
                        &y,
                        if kind == 1.0 {
                            kernel::TestKind::Paired
                        } else if kind == 2.0 {
                            kernel::TestKind::Pooled
                        } else {
                            kernel::TestKind::Welch
                        },
                        tail == 2.0,
                        |w| host.charge_work(w),
                    )
                }
                _ => return Err(VALUE_ERROR.into()),
            }
            .map_err(|e| failure(host, e))?;
            Ok(Number(value))
        })
    })();
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
