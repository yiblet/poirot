//! Modern sample contracts with fixed collections and separately lifted scalar slots.
use crate::adapters::{
    elementwise::{
        self, ArgumentRole, Broadcast, Kernel, MemberContext, NumericParameter as P,
        NumericSignature, Optional, Variadic, WholeCollection,
        collections::{self, Admission},
        matrices,
    },
    failure::operational_only,
};
use crate::host::CallHost;
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
/// A collection-only statistical operation, including data-dependent mode output.
struct SampleAggregate(StatisticsFunction);
impl collections::CollectionKernel for SampleAggregate {
    type Signature = Variadic<WholeCollection>;
    fn apply(
        &self,
        host: &mut CallHost<'_, '_>,
        arguments: &Self::Signature,
        address: CellAddress,
    ) -> FunctionResult<Value> {
        use StatisticsFunction::*;
        let function = self.0;
        let mut values = collections::collect(host, arguments.iter(), Admission::Aggregate)?;
        processing(host, values.len())?;
        if matches!(function, ModeSngl | ModeMult) {
            let count = kernel::modes(
                &mut values,
                FunctionFailure::from(host.allocation_failure()),
            )
            .map_err(|e| failure(host, e))?;
            if function == ModeSngl {
                return Ok(Number(*values.first().ok_or(NA)?).into());
            }
            return matrices::array(
                host,
                count,
                1,
                values.iter().take(count).copied().map(Number),
                address,
            );
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
        .map_err(|e| failure(host, e))?;
        Ok(Number(value).into())
    }
}

/// Position-preserving samples used by paired hypothesis tests.
type PositionalSamples = (Vec<Option<f64>>, Vec<Option<f64>>);
/// Aligned admitted numeric samples used by two-sample calculations.
type PairedSamples = (Vec<f64>, Vec<f64>);

/// Typed samples retained independently of broadcast numerical parameters.
struct Sample<R> {
    function: StatisticsFunction,
    values: std::cell::RefCell<Vec<f64>>,
    positional: Option<PositionalSamples>,
    mass: Option<PairedSamples>,
    shape: Option<(usize, usize)>,
    roles: std::marker::PhantomData<R>,
}
impl<R> Sample<R> {
    /// Bind a sample operation before inspecting its typed argument geometry.
    fn new(function: StatisticsFunction) -> Self {
        Self {
            function,
            values: std::cell::RefCell::new(Vec::new()),
            positional: None,
            mass: None,
            shape: None,
            roles: std::marker::PhantomData,
        }
    }
}
/// Mixed collection and scalar signatures supply independently admitted inputs.
trait SampleRoles: elementwise::Signature {
    /// Materialize the samples in source order under their declared admission policy.
    fn prepare(&self, host: &mut CallHost<'_, '_>, sample: &mut Sample<Self>) -> FunctionResult<()>
    where
        Self: Sized;
    /// Coerce broadcast slots in source order through the shared parameter table.
    fn numbers(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        function: StatisticsFunction,
    ) -> FunctionResult<elementwise::NumericArguments>;
    /// Test whether an optional sigma was omitted before numeric coercion.
    fn sigma_missing(&self, _context: &mut MemberContext<'_, '_, '_>) -> FunctionResult<bool> {
        Ok(false)
    }
}
impl<R: SampleRoles> Kernel for Sample<R> {
    type Signature = R;
    fn prepare(&mut self, host: &mut CallHost<'_, '_>, arguments: &R) -> FunctionResult<()> {
        arguments.prepare(host, self)
    }
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &R,
    ) -> FunctionResult<CellValue> {
        use StatisticsFunction::*;
        let function = self.function;
        let numbers = arguments.numbers(context, function)?;
        let sigma_missing = function == ZTest && arguments.sigma_missing(context)?;
        let n = |index, _default: Option<f64>| numbers.get(index);
        let host = &mut *context.host;
        let mut values = self
            .values
            .try_borrow_mut()
            .map_err(|_| FunctionFailure::from(VALUE_ERROR))?;
        let value = match function {
            RSq | Steyx => {
                let (y, x) = self.mass.as_ref().ok_or(VALUE_ERROR)?;
                processing(host, x.len())?;
                kernel::paired(
                    y,
                    x,
                    if function == RSq {
                        kernel::PairKind::RSquared
                    } else {
                        kernel::PairKind::StandardError
                    },
                )
            }
            ChisqTest => {
                let (observed, expected) = self.mass.as_ref().ok_or(VALUE_ERROR)?;
                let (rows, columns) = self.shape.ok_or(VALUE_ERROR)?;
                processing(host, observed.len())?;
                kernel::chi_square_test(observed, expected, rows, columns, |w| host.charge_work(w))
            }
            FTest => {
                let (x, y) = self.mass.as_ref().ok_or(VALUE_ERROR)?;
                processing(host, x.len().saturating_add(y.len()))?;
                kernel::f_test(x, y, |w| host.charge_work(w))
            }

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
                let sigma = if sigma_missing {
                    None
                } else {
                    Some(n(1, None)?)
                };
                processing(host, values.len())?;
                kernel::z_test(&values, hypothesis, sigma)
            }
            Prob => {
                let lower = n(0, Some(0.0))?;
                let upper = n(1, Some(lower))?;
                let (x, p) = self.mass.as_ref().ok_or(VALUE_ERROR)?;
                processing(host, x.len())?;
                kernel::probability(x, p, lower, upper)
            }
            TTest => {
                let (tail, kind) = (n(0, None)?.trunc(), n(1, None)?.trunc());
                if tail != 1.0 && tail != 2.0 || !(1.0..=3.0).contains(&kind) {
                    return Err(Num.into());
                }
                let (a, b) = self.positional.as_ref().ok_or(VALUE_ERROR)?;
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
    }
}

impl SampleRoles for Variadic<Broadcast> {
    fn prepare(
        &self,
        _host: &mut CallHost<'_, '_>,
        _sample: &mut Sample<Self>,
    ) -> FunctionResult<()> {
        Ok(())
    }
    fn numbers(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        function: StatisticsFunction,
    ) -> FunctionResult<elementwise::NumericArguments> {
        let parameters: &[P] = if matches!(
            function,
            StatisticsFunction::Fisher | StatisticsFunction::FisherInv
        ) {
            &[P::required()]
        } else {
            &[P::required(), P::required()]
        };
        self.numbers(context, parameters)
    }
}
impl SampleRoles for (WholeCollection, Broadcast, Optional<Broadcast>) {
    fn prepare(
        &self,
        host: &mut CallHost<'_, '_>,
        sample: &mut Sample<Self>,
    ) -> FunctionResult<()> {
        *sample.values.get_mut() = collections::collect(host, [self.0], Admission::Numeric)?;
        Ok(())
    }
    fn numbers(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        function: StatisticsFunction,
    ) -> FunctionResult<elementwise::NumericArguments> {
        let parameters: &[P] = if function == StatisticsFunction::ZTest {
            &[P::required(), P::constant(f64::NAN)]
        } else {
            &[P::required()]
        };
        if function == StatisticsFunction::ZTest {
            NumericSignature::new(parameters).numbers_roles(context, [Some(self.1), self.2.0])
        } else {
            NumericSignature::new(parameters).numbers_roles(context, [Some(self.1)])
        }
    }
    fn sigma_missing(&self, context: &mut MemberContext<'_, '_, '_>) -> FunctionResult<bool> {
        Ok(self
            .2
            .0
            .map(|role| role.read(context).map(|operand| operand.missing))
            .transpose()?
            .unwrap_or(true))
    }
}
impl SampleRoles for (Broadcast, WholeCollection, Optional<Broadcast>) {
    fn prepare(
        &self,
        host: &mut CallHost<'_, '_>,
        sample: &mut Sample<Self>,
    ) -> FunctionResult<()> {
        *sample.values.get_mut() = collections::collect(host, [self.1], Admission::Numeric)?;
        Ok(())
    }
    fn numbers(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        _function: StatisticsFunction,
    ) -> FunctionResult<elementwise::NumericArguments> {
        NumericSignature::new(&[P::required(), P::constant(0.0)])
            .numbers_roles(context, [Some(self.0), self.2.0])
    }
}
impl SampleRoles
    for (
        WholeCollection,
        WholeCollection,
        Optional<Broadcast>,
        Optional<Broadcast>,
    )
{
    fn prepare(
        &self,
        host: &mut CallHost<'_, '_>,
        sample: &mut Sample<Self>,
    ) -> FunctionResult<()> {
        if sample.function == StatisticsFunction::TTest {
            let a = collections::positions(host, &self.0, Admission::Numeric)?;
            let b_len = self.1.len(host)?;
            host.check_values(a.len().saturating_add(b_len).saturating_mul(2))?;
            let b = collections::positions(host, &self.1, Admission::Numeric)?;
            sample.positional = Some((a, b));
        } else {
            let a = collections::collect(host, [self.0], Admission::Strict)?;
            let b_len = self.1.len(host)?;
            host.check_values(a.len().saturating_add(b_len))?;
            let b = collections::collect(host, [self.1], Admission::Strict)?;
            sample.mass = Some((a, b));
        }
        Ok(())
    }
    fn numbers(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        function: StatisticsFunction,
    ) -> FunctionResult<elementwise::NumericArguments> {
        let parameters = if function == StatisticsFunction::TTest {
            [P::required(), P::required()]
        } else {
            [P::constant(0.0), P::prior(0)]
        };
        NumericSignature::new(&parameters).numbers_roles(context, [self.2.0, self.3.0])
    }
}
impl SampleRoles for (WholeCollection, WholeCollection) {
    fn prepare(
        &self,
        host: &mut CallHost<'_, '_>,
        sample: &mut Sample<Self>,
    ) -> FunctionResult<()> {
        use StatisticsFunction::*;
        sample.mass = Some(match sample.function {
            RSq | Steyx => collections::pairs(host, &self.0, &self.1, Admission::Aggregate)?,
            ChisqTest => {
                let (a, b) = (
                    host.describe(self.0.argument())?,
                    host.describe(self.1.argument())?,
                );
                if a.sheets != 1 || b.sheets != 1 || a.rows != b.rows || a.columns != b.columns {
                    return Err(NA.into());
                }
                sample.shape = Some((a.rows, a.columns));
                let x = collections::collect(host, [self.0], Admission::Strict)?;
                host.check_values(x.len().saturating_mul(2))?;
                let y = collections::collect(host, [self.1], Admission::Strict)?;
                (x, y)
            }
            _ => {
                let x = collections::collect(host, [self.0], Admission::Numeric)?;
                let y_len = self.1.len(host)?;
                host.check_values(x.len().saturating_add(y_len))?;
                let y = collections::collect(host, [self.1], Admission::Numeric)?;
                (x, y)
            }
        });
        Ok(())
    }
    fn numbers(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        _function: StatisticsFunction,
    ) -> FunctionResult<elementwise::NumericArguments> {
        NumericSignature::new(&[]).numbers_roles(context, [])
    }
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
    use StatisticsFunction::*;
    let result = match function {
        HarMean | Kurt | Skew | SkewP | ModeSngl | ModeMult => {
            collections::execute(&mut host, address, SampleAggregate(function))
        }
        RSq | Steyx | ChisqTest | FTest => elementwise::execute(
            &mut host,
            address,
            Sample::<(WholeCollection, WholeCollection)>::new(function),
        ),
        Fisher | FisherInv | Permut | PermutationA => elementwise::execute(
            &mut host,
            address,
            Sample::<Variadic<Broadcast>>::new(function),
        ),
        RankAvg => elementwise::execute(
            &mut host,
            address,
            Sample::<(Broadcast, WholeCollection, Optional<Broadcast>)>::new(function),
        ),
        TTest | Prob => elementwise::execute(
            &mut host,
            address,
            Sample::<(
                WholeCollection,
                WholeCollection,
                Optional<Broadcast>,
                Optional<Broadcast>,
            )>::new(function),
        ),
        _ => elementwise::execute(
            &mut host,
            address,
            Sample::<(WholeCollection, Broadcast, Optional<Broadcast>)>::new(function),
        ),
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
