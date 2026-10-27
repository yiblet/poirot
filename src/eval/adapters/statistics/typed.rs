//! Statistical kernels over shared scalar signatures and numeric collections.
use crate::eval::adapters::elementwise::{
    self, Broadcast, Kernel, MemberContext, NumericParameter, NumericSignature, Optional, Variadic,
    WholeCollection,
    collections::{self, Admission, CollectionKernel},
};
use crate::eval::adapters::operational_only::operational_only;
use crate::eval::functions::registry::{FunctionId, StatisticsFunction};
use crate::eval::host::CallHost;
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationError};
use crate::eval::session::Session;
use crate::eval::values::{
    FunctionError, FunctionResult, VALUE_ERROR, Value, err, numeric_outcome,
};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellError::{NA, Num};
use crate::model::{CellAddress, CellValue};
use poirot_functions::statistics as domain;

/// Map numerical computation failures without catching operational aborts.
fn computation(
    host: &CallHost<'_, '_>,
    failure: domain::StatisticalKernelError<EvaluationError>,
) -> FunctionError {
    match failure {
        domain::StatisticalKernelError::Spreadsheet(error) => error.into(),
        domain::StatisticalKernelError::Operational(error) => error.into(),
        domain::StatisticalKernelError::SourceConvergence(source) => host.unsupported(
            CapabilityId::FunctionAdapter,
            crate::eval::limits::EvaluationReasonError::StatisticalConvergence(
                std::sync::Arc::new(source),
            ),
        ),
    }
}

/// Adapts scalar probability functions to broadcast traversal and ordered parameter conversion.
/// Each output invokes the selected independent distribution algorithm with evaluator work metering.
struct Distribution(StatisticsFunction);

impl Kernel for Distribution {
    type Signature = Variadic<Broadcast>;

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        use StatisticsFunction::{
            NormDist, NormDistLegacy, NormInv, NormInvLegacy, NormSDist, NormSDistLegacy, NormSInv,
            NormSInvLegacy, Poisson, PoissonDist, Standardize, TDist, TDist2T, TDistLegacy,
            TDistRt,
        };
        let parameters: &[NumericParameter] = match self.0 {
            NormDist | NormDistLegacy => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::boolean(),
            ],
            NormSDist => &[
                NumericParameter::required(),
                NumericParameter::boolean().missing_as_blank(),
            ],
            NormSDistLegacy | NormSInv | NormSInvLegacy => &[NumericParameter::required()],
            Poisson | PoissonDist | TDist => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::boolean(),
            ],
            TDistRt | TDist2T => &[NumericParameter::required(), NumericParameter::required()],
            _ => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
            ],
        };
        let n = arguments.numbers(context, parameters)?;
        let host = &mut *context.host;
        let value = match self.0 {
            NormDist | NormDistLegacy => domain::normal_dist_metered(
                n.get(0)?,
                n.get(1)?,
                n.get(2)?,
                n.get(3)? != 0.0,
                &mut |w| host.charge_work(w),
            )
            .map_err(|error| computation(host, error))?,
            NormSDist | NormSDistLegacy => domain::normal_dist_metered(
                n.get(0)?,
                0.0,
                1.0,
                self.0 == NormSDistLegacy || n.get(1)? != 0.0,
                &mut |w| host.charge_work(w),
            )
            .map_err(|error| computation(host, error))?,
            NormInv | NormInvLegacy => domain::normal_inverse(n.get(0)?, n.get(1)?, n.get(2)?)?,
            NormSInv | NormSInvLegacy => domain::normal_inverse(n.get(0)?, 0.0, 1.0)?,
            Standardize => domain::standardize(n.get(0)?, n.get(1)?, n.get(2)?)?,
            Poisson | PoissonDist => {
                domain::poisson_metered(n.get(0)?, n.get(1)?, n.get(2)? != 0.0, &mut |w| {
                    host.charge_work(w)
                })
                .map_err(|error| computation(host, error))?
            }
            TDistLegacy => domain::legacy_t_dist(n.get(0)?, n.get(1)?, n.get(2)?)?,
            TDist | TDistRt | TDist2T => domain::student_t(
                n.get(0)?,
                n.get(1)?,
                match self.0 {
                    TDistRt => domain::StudentTKind::RightTail,
                    TDist2T => domain::StudentTKind::TwoTails,
                    _ if n.get(2)? != 0.0 => domain::StudentTKind::Cumulative,
                    _ => domain::StudentTKind::Density,
                },
            )?,
            _ => return Err(VALUE_ERROR.into()),
        };
        Ok(numeric_outcome::convert_checked_numeric_result(value))
    }
}

/// Statistical aggregate kernel over complete source collections.
/// The selected operation determines which values and origins enter the sample before the independent reduction runs.
struct Aggregate(StatisticsFunction);

impl CollectionKernel for Aggregate {
    type Signature = Variadic<WholeCollection>;

    fn apply(
        &self,
        host: &mut CallHost<'_, '_>,
        arguments: &Self::Signature,
        _address: CellAddress,
    ) -> FunctionResult<Value> {
        use StatisticsFunction::{
            AveDev, AverageA, DevSq, GeoMean, MaxA, MinA, Mode, StdevA, StdevPA, VarA, VarPA,
        };
        use domain::{Aggregate as A, Collection as C};
        let (operation, admission) = match self.0 {
            AveDev => (A::AveDev, C::NumericAggregate),
            AverageA => (A::Average, C::AggregateA),
            DevSq => (A::DevSq, C::NumericAggregate),
            GeoMean => (A::GeoMean, C::GeoMean),
            MaxA => (A::Maximum, C::ExtremaA),
            MinA => (A::Minimum, C::ExtremaA),
            StdevA => (A::StdevSample, C::AggregateA),
            StdevPA => (A::StdevPopulation, C::AggregateA),
            VarA => (A::VarianceSample, C::AggregateA),
            VarPA => (A::VariancePopulation, C::AggregateA),
            Mode => (A::Mode, C::Mode),
            _ => return Err(VALUE_ERROR.into()),
        };
        let values = collections::statistics(host, arguments.iter(), admission)?;
        host.charge_work((values.len() as u64).saturating_mul(16))?;
        Ok(
            numeric_outcome::convert_checked_numeric_result(domain::aggregate(operation, &values)?)
                .into(),
        )
    }
}

/// Order-statistic kernel that collects one fixed sample before visiting broadcast controls.
/// Its typed signature keeps the sample position distinct from parameters such as rank or percentile.
struct Order<R> {
    function: StatisticsFunction,
    values: Vec<f64>,
    roles: std::marker::PhantomData<R>,
}

impl<R> Order<R> {
    /// Retain the mathematical operation before shared signature admission.
    fn new(function: StatisticsFunction) -> Self {
        Self {
            function,
            values: Vec::new(),
            roles: std::marker::PhantomData,
        }
    }
}

/// Typed order-statistic roles determine the sample position and scalar demand.
trait OrderRoles: elementwise::Signature {
    /// Return the fixed sample from this signature.
    fn sample(&self) -> &WholeCollection;

    /// Coerce the sample operation's scalar parameter slots in order.
    fn numbers(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        function: StatisticsFunction,
    ) -> FunctionResult<elementwise::NumericArguments>;
}

impl OrderRoles for (WholeCollection, Broadcast, Optional<Broadcast>) {
    fn sample(&self) -> &WholeCollection {
        &self.0
    }

    fn numbers(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        function: StatisticsFunction,
    ) -> FunctionResult<elementwise::NumericArguments> {
        if matches!(
            function,
            StatisticsFunction::PercentRank
                | StatisticsFunction::PercentRankInc
                | StatisticsFunction::PercentRankExc
        ) {
            NumericSignature::new(&[
                NumericParameter::required(),
                NumericParameter::constant(3.0),
            ])
            .numbers_roles(context, [Some(self.1), self.2.0])
        } else {
            NumericSignature::new(&[NumericParameter::required()])
                .numbers_roles(context, [Some(self.1)])
        }
    }
}

impl OrderRoles for (Broadcast, WholeCollection, Optional<Broadcast>) {
    fn sample(&self) -> &WholeCollection {
        &self.1
    }

    fn numbers(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        _function: StatisticsFunction,
    ) -> FunctionResult<elementwise::NumericArguments> {
        NumericSignature::new(&[
            NumericParameter::required(),
            NumericParameter::constant(0.0),
        ])
        .numbers_roles(context, [Some(self.0), self.2.0])
    }
}

impl<R: OrderRoles> Kernel for Order<R> {
    type Signature = R;

    fn prepare(&mut self, host: &mut CallHost<'_, '_>, arguments: &R) -> FunctionResult<()> {
        use StatisticsFunction::{PercentRank, PercentRankExc, PercentRankInc, Rank, RankEq};
        let policy = match self.function {
            Rank | RankEq => domain::Collection::Rank,
            PercentRank | PercentRankInc | PercentRankExc => domain::Collection::PercentRank,
            _ => domain::Collection::NumericAggregate,
        };
        self.values = collections::statistics(host, [*arguments.sample()], policy)?;
        Ok(())
    }

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &R,
    ) -> FunctionResult<CellValue> {
        use StatisticsFunction::{
            Large, PercentRank, PercentRankExc, PercentRankInc, Percentile, QuartileInc, Rank,
            RankEq, Small,
        };
        let n = arguments.numbers(context, self.function)?;
        let first = n.get(0)?;
        context
            .host
            .charge_work((self.values.len() as u64).saturating_mul(64))?;
        let value = match self.function {
            Large | Small => {
                if self.function == Small && first < 1.0 {
                    return Err(poirot_functions::scalar::CellError::Num.into());
                }
                let rank =
                    poirot_functions::coercion::compatibility::coerce_statistics_kth_rank(first)?;
                domain::order_statistic(&self.values, rank, self.function == Large)?
            }
            Percentile => domain::percentile(&self.values, first)?,
            QuartileInc => {
                let quartile = first.trunc();
                if !(0.0..=4.0).contains(&quartile) {
                    return Err(Num.into());
                }
                domain::percentile(&self.values, quartile / 4.0)?
            }
            Rank | RankEq => {
                if !self.values.contains(&first) {
                    return Err(NA.into());
                }
                domain::rank(&self.values, first, n.get(1)? != 0.0)?
            }
            PercentRank | PercentRankInc | PercentRankExc => {
                let significance = n.get(1)?.floor();
                if significance < 1.0 {
                    return Err(Num.into());
                }
                domain::percent_rank(
                    &self.values,
                    first,
                    (significance as u32).min(20),
                    if self.function == PercentRankExc {
                        domain::PercentRankKind::Exclusive
                    } else {
                        domain::PercentRankKind::Inclusive
                    },
                )?
            }
            _ => return Err(VALUE_ERROR.into()),
        };
        Ok(numeric_outcome::convert_checked_numeric_result(value))
    }
}

/// Paired-statistics kernel that collects aligned X and Y observations once.
/// A forecast signature can broadcast target coordinates over those same pairs.
struct Paired<R> {
    function: StatisticsFunction,
    y: Vec<f64>,
    x: Vec<f64>,
    roles: std::marker::PhantomData<R>,
}

impl<R> Paired<R> {
    /// Retain a paired mathematical operation until samples have been admitted.
    fn new(function: StatisticsFunction) -> Self {
        Self {
            function,
            y: Vec::new(),
            x: Vec::new(),
            roles: std::marker::PhantomData,
        }
    }
}

/// Typed pair signatures retain collection positions and an optional scalar forecast.
trait PairRoles: elementwise::Signature {
    /// Return samples in their source acquisition order.
    fn samples(&self) -> (&WholeCollection, &WholeCollection);

    /// Coerce the forecast coordinate only for signatures that declare one.
    fn target(&self, context: &mut MemberContext<'_, '_, '_>) -> FunctionResult<Option<f64>>;
}

impl PairRoles for (WholeCollection, WholeCollection) {
    fn samples(&self) -> (&WholeCollection, &WholeCollection) {
        (&self.0, &self.1)
    }

    fn target(&self, _context: &mut MemberContext<'_, '_, '_>) -> FunctionResult<Option<f64>> {
        Ok(None)
    }
}

impl PairRoles for (Broadcast, WholeCollection, WholeCollection) {
    fn samples(&self) -> (&WholeCollection, &WholeCollection) {
        (&self.1, &self.2)
    }

    fn target(&self, context: &mut MemberContext<'_, '_, '_>) -> FunctionResult<Option<f64>> {
        NumericSignature::new(&[NumericParameter::required()])
            .numbers_roles(context, [Some(self.0)])?
            .get(0)
            .map(Some)
    }
}

impl<R: PairRoles> Kernel for Paired<R> {
    type Signature = R;

    fn prepare(&mut self, host: &mut CallHost<'_, '_>, arguments: &R) -> FunctionResult<()> {
        let (first, second) = arguments.samples();
        (self.y, self.x) = collections::pairs(host, first, second, Admission::Numeric)?;
        Ok(())
    }

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &R,
    ) -> FunctionResult<CellValue> {
        use StatisticsFunction::{
            Correl, Covar, CovarianceP, CovarianceS, Intercept, Pearson, Slope, SumX2MY2, SumX2PY2,
            SumXMY2,
        };
        let target = arguments.target(context)?;
        context
            .host
            .charge_work((self.x.len() as u64).saturating_mul(12))?;
        // Worksheet regression calls supply known Y before known X; the
        // independent regression algorithms accept X before Y.
        let value = if let Some(target) = target {
            domain::forecast(target, &self.x, &self.y)?
        } else if matches!(self.function, Intercept | Slope) {
            let regression = domain::regression(&self.x, &self.y, self.x.len())?;
            if self.function == Slope {
                regression.slope
            } else {
                regression.intercept
            }
        } else {
            let operation = match self.function {
                Correl | Pearson => domain::PairOperation::Correlation,
                Covar | CovarianceP => domain::PairOperation::CovariancePopulation,
                CovarianceS => domain::PairOperation::CovarianceSample,
                SumX2MY2 => domain::PairOperation::SumX2MinusY2,
                SumX2PY2 => domain::PairOperation::SumX2PlusY2,
                SumXMY2 => domain::PairOperation::SumXMinusYSquared,
                _ => return Err(VALUE_ERROR.into()),
            };
            domain::paired(operation, &self.y, &self.x)?
        };
        Ok(numeric_outcome::convert_checked_numeric_result(value))
    }
}

/// Execute standard statistics using shared typed scalar and collection strategies.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: StatisticsFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    use StatisticsFunction::{
        AveDev, AverageA, Correl, Covar, CovarianceP, CovarianceS, DevSq, Forecast, ForecastLinear,
        GeoMean, Intercept, Large, MaxA, MinA, Mode, Pearson, PercentRank, PercentRankExc,
        PercentRankInc, Percentile, QuartileInc, Rank, RankEq, Slope, Small, StdevA, StdevPA,
        SumX2MY2, SumX2PY2, SumXMY2, VarA, VarPA,
    };
    let function = if function == StatisticsFunction::QuartileIncLegacy {
        StatisticsFunction::QuartileInc
    } else {
        function
    };
    if !FunctionId::Statistics(function)
        .descriptor()
        .arity
        .accepts(arguments.len())
    {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_array(session, sheet, address, arguments, arguments.len());
    let result = match function {
        AveDev | AverageA | DevSq | GeoMean | MaxA | MinA | StdevA | StdevPA | VarA | VarPA
        | Mode => collections::execute(&mut host, address, Aggregate(function)),
        Rank | RankEq => elementwise::execute(
            &mut host,
            address,
            Order::<(Broadcast, WholeCollection, Optional<Broadcast>)>::new(function),
        ),
        Large | Small | Percentile | QuartileInc | PercentRank | PercentRankInc
        | PercentRankExc => elementwise::execute(
            &mut host,
            address,
            Order::<(WholeCollection, Broadcast, Optional<Broadcast>)>::new(function),
        ),
        Forecast | ForecastLinear => elementwise::execute(
            &mut host,
            address,
            Paired::<(Broadcast, WholeCollection, WholeCollection)>::new(function),
        ),
        Correl | Pearson | Covar | CovarianceP | CovarianceS | Intercept | Slope | SumX2MY2
        | SumX2PY2 | SumXMY2 => elementwise::execute(
            &mut host,
            address,
            Paired::<(WholeCollection, WholeCollection)>::new(function),
        ),
        _ => elementwise::execute(&mut host, address, Distribution(function)),
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
