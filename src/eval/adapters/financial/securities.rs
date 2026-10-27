//! Date-aware rectangular lifting for documented security calculations.
use crate::eval::adapters::{
    elementwise::{self, Broadcast, Kernel, MemberContext, NumericParameter, Variadic},
    operational_only::operational_only,
};
use crate::eval::functions::registry::{FinancialFunction, FunctionId};
use crate::eval::host::CallHost;
use crate::eval::limits::{CapabilityId, EvalResult};
use crate::eval::ports::DependencyRead;
use crate::eval::session::{ReadRecorder, Session};
use crate::eval::values::{FunctionError, FunctionResult, VALUE_ERROR, Value, err};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellError::{Div0, Num};
use crate::model::CellValue::Number;
use crate::model::{CellAddress, CellValue};
use poirot_functions::coercion::date_time::CalendarEpoch;
use poirot_functions::financial::{
    FinancialCalculationError, FinancialError,
    securities::{
        self as kernel, Basis, Bond, Day, DiscountOperation as Discount, Frequency,
        MaturityOperation as Maturity, TreasuryOperation as Treasury,
    },
};

fn spreadsheet(error: FinancialError) -> FunctionError {
    match error {
        FinancialError::Num => Num,
        FinancialError::Value => VALUE_ERROR,
        FinancialError::DivZero => Div0,
    }
    .into()
}

/// Identify the security functions handled by this adapter.
pub(super) fn handles(function: FinancialFunction) -> bool {
    use FinancialFunction::{
        Accrint, Accrintm, Coupdaybs, Coupdays, Coupdaysnc, Coupncd, Coupnum, Couppcd, Disc,
        Duration, Intrate, Mduration, Price, Pricedisc, Pricemat, Received, Tbilleq, Tbillprice,
        Tbillyield, Yield, Yielddisc, Yieldmat,
    };
    matches!(
        function,
        Accrint
            | Accrintm
            | Coupdaybs
            | Coupdays
            | Coupdaysnc
            | Coupncd
            | Coupnum
            | Couppcd
            | Disc
            | Duration
            | Intrate
            | Mduration
            | Price
            | Pricedisc
            | Pricemat
            | Received
            | Tbilleq
            | Tbillprice
            | Tbillyield
            | Yield
            | Yielddisc
            | Yieldmat
    )
}

/// Adapts coupon, bond, and other security functions to shared broadcast execution.
/// The workbook epoch and parameter table supply date conversion and validated frequency or basis choices.
struct SecurityKernel {
    function: FinancialFunction,
    epoch: CalendarEpoch,
}

impl Kernel for SecurityKernel {
    type Signature = Variadic<Broadcast>;

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        use FinancialFunction::{
            Accrint, Accrintm, Coupdaybs, Coupdays, Coupdaysnc, Coupncd, Coupnum, Couppcd, Disc,
            Duration, Intrate, Mduration, Price, Pricedisc, Pricemat, Received, Tbilleq,
            Tbillprice, Tbillyield, Yield, Yielddisc, Yieldmat,
        };
        let function = self.function;
        let epoch = self.epoch;
        let parameters: &[NumericParameter] = match function {
            Coupdaybs | Coupdays | Coupdaysnc | Coupncd | Coupnum | Couppcd => &[
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::required().integer_choices(&[1, 2, 4]),
                NumericParameter::constant(0.0).integer_choices(&[0, 1, 2, 3, 4]),
            ],
            Price | Yield => &[
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required().integer_choices(&[1, 2, 4]),
                NumericParameter::constant(0.0).integer_choices(&[0, 1, 2, 3, 4]),
            ],
            Duration | Mduration => &[
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required().integer_choices(&[1, 2, 4]),
                NumericParameter::constant(0.0).integer_choices(&[0, 1, 2, 3, 4]),
            ],
            Accrint => &[
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::required(),
                NumericParameter::constant(1000.0),
                NumericParameter::required().integer_choices(&[1, 2, 4]),
                NumericParameter::constant(0.0).integer_choices(&[0, 1, 2, 3, 4]),
                NumericParameter::boolean_default(true),
            ],
            Accrintm => &[
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::required(),
                NumericParameter::constant(1000.0),
                NumericParameter::constant(0.0).integer_choices(&[0, 1, 2, 3, 4]),
            ],
            Pricemat | Yieldmat => &[
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::constant(0.0).integer_choices(&[0, 1, 2, 3, 4]),
            ],
            Tbilleq | Tbillprice | Tbillyield => &[
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::required(),
            ],
            Disc | Intrate | Pricedisc | Received | Yielddisc => &[
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::constant(0.0).integer_choices(&[0, 1, 2, 3, 4]),
            ],
            _ => return Err(VALUE_ERROR.into()),
        };
        let values = arguments.numbers(context, parameters)?;
        let host = &mut *context.host;
        let first = Day::from_serial(values.get(0)?, epoch).map_err(spreadsheet)?;
        let second = Day::from_serial(values.get(1)?, epoch).map_err(spreadsheet)?;
        let n = |index| values.get(index);
        let value = match function {
            Coupdaybs | Coupdays | Coupdaysnc | Coupncd | Coupnum | Couppcd => {
                let frequency = Frequency::from_number(n(2)?).map_err(spreadsheet)?;
                let basis = Basis::from_number(n(3)?).map_err(spreadsheet)?;
                let period =
                    kernel::coupon_period(first, second, frequency, basis).map_err(spreadsheet)?;
                match function {
                    Coupdaybs => period.accrued,
                    Coupdays => period.denominator,
                    Coupdaysnc => period.remaining,
                    Coupncd => period.next.serial(epoch),
                    Coupnum => f64::from(period.count),
                    _ => period.previous.serial(epoch),
                }
            }
            Price | Yield | Duration | Mduration => {
                let rate = n(2)?;
                let value = n(3)?;
                let duration = matches!(function, Duration | Mduration);
                let redemption = if duration { 100.0 } else { n(4)? };
                let frequency = Frequency::from_number(n(if duration { 4 } else { 5 })?)
                    .map_err(spreadsheet)?;
                let basis =
                    Basis::from_number(n(if duration { 5 } else { 6 })?).map_err(spreadsheet)?;
                let bond = Bond::new(first, second, rate, redemption, frequency, basis)
                    .map_err(spreadsheet)?;
                match function {
                    Price => bond.price(value).map_err(spreadsheet)?,
                    Duration => bond.duration(value).map_err(spreadsheet)?,
                    Mduration => bond.modified_duration(value).map_err(spreadsheet)?,
                    _ => bond
                        .yield_rate(value, |work| host.charge_work(work))
                        .map_err(|error| match error {
                            FinancialCalculationError::Spreadsheet(error) => spreadsheet(error),
                            FinancialCalculationError::Operational(error) => error.into(),
                            FinancialCalculationError::ComputationalLimit => host.unsupported(
                                CapabilityId::FunctionAdapter,
                                crate::eval::limits::EvaluationReasonError::BondYieldExceedsItsNumericalEnvelope,
                            ),
                        })?,
                }
            }
            Accrint => {
                let settlement = Day::from_serial(values.get(2)?, epoch).map_err(spreadsheet)?;
                let rate = values.get(3)?;
                let par = values.get(4)?;
                let frequency = Frequency::from_number(values.get(5)?).map_err(spreadsheet)?;
                let basis = Basis::from_number(values.get(6)?).map_err(spreadsheet)?;
                let from_issue = values.get(7)? != 0.0;
                kernel::accrued_interest(
                    first,
                    second,
                    settlement,
                    rate,
                    par,
                    frequency,
                    basis,
                    from_issue,
                    |work| host.charge_work(work),
                )
                .map_err(|error| match error {
                    FinancialCalculationError::Spreadsheet(error) => spreadsheet(error),
                    FinancialCalculationError::Operational(error) => error.into(),
                    FinancialCalculationError::ComputationalLimit => host.unsupported(
                        CapabilityId::FunctionAdapter,
                        crate::eval::limits::EvaluationReasonError::AccruedInterestExceedsItsQuasiCouponEnvelope,
                    ),
                })?
            }
            Accrintm => kernel::accrued_at_maturity(
                first,
                second,
                n(2)?,
                n(3)?,
                Basis::from_number(n(4)?).map_err(spreadsheet)?,
            )
            .map_err(spreadsheet)?,
            Pricemat | Yieldmat => {
                let issue = Day::from_serial(values.get(2)?, epoch).map_err(spreadsheet)?;
                let rate = values.get(3)?;
                let value = values.get(4)?;
                let basis = Basis::from_number(values.get(5)?).map_err(spreadsheet)?;
                kernel::maturity_security(
                    first,
                    second,
                    issue,
                    rate,
                    value,
                    basis,
                    if function == Pricemat {
                        Maturity::Price
                    } else {
                        Maturity::Yield
                    },
                )
                .map_err(spreadsheet)?
            }
            Tbilleq | Tbillprice | Tbillyield => kernel::treasury(
                first,
                second,
                n(2)?,
                match function {
                    Tbilleq => Treasury::Equivalent,
                    Tbillprice => Treasury::Price,
                    _ => Treasury::Yield,
                },
            )
            .map_err(spreadsheet)?,
            Disc | Intrate | Pricedisc | Received | Yielddisc => kernel::discount(
                first,
                second,
                n(2)?,
                n(3)?,
                Basis::from_number(n(4)?).map_err(spreadsheet)?,
                match function {
                    Disc => Discount::Discount,
                    Pricedisc => Discount::Price,
                    Received => Discount::Received,
                    _ => Discount::Yield,
                },
            )
            .map_err(spreadsheet)?,
            _ => return Err(VALUE_ERROR.into()),
        };
        Ok(Number(value))
    }
}

/// Coerce security inputs, publish lifted outputs and retain operational work failures.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: FinancialFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !FunctionId::Financial(function)
        .descriptor()
        .arity
        .accepts(args.len())
    {
        return Ok(err(VALUE_ERROR));
    }
    ReadRecorder::new(session).read(DependencyRead::ContextField(
        crate::eval::ports::ContextRead::DateSystem,
    ));
    let epoch = match session.sources.workbook.date_system {
        crate::model::DateSystem::Excel1900 => CalendarEpoch::Excel1900,
        crate::model::DateSystem::Excel1904 => CalendarEpoch::Excel1904,
    };
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    let result = elementwise::execute(&mut host, address, SecurityKernel { function, epoch });
    match result {
        Ok(value) => Ok(value),
        Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
