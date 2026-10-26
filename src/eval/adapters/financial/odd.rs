//! Date-aware rectangular operands for irregular coupons and French depreciation.
use crate::eval::adapters::{
    elementwise::{
        self, Broadcast, Kernel, MemberContext, NumericParameter as P, NumericSignature, Variadic,
    },
    failure::operational_only,
};
use crate::eval::host::CallHost;
use crate::eval::*;
use poirot_functions::coercion::date_time::CalendarEpoch;
use poirot_functions::financial::{
    FinancialError,
    modern::Failure,
    odd::{self as kernel, Amortization, OddFirst, OddLast},
    securities::{Basis, Day, Frequency},
};
fn spreadsheet(error: FinancialError) -> FunctionFailure {
    match error {
        FinancialError::Num => Num,
        FinancialError::Value => VALUE_ERROR,
        FinancialError::DivZero => Div0,
    }
    .into()
}
fn failure<E: Into<FunctionFailure>>(
    host: &CallHost<'_, '_>,
    error: Failure<E>,
) -> FunctionFailure {
    match error {
        Failure::Spreadsheet(e) => spreadsheet(e),
        Failure::Operational(e) => e.into(),
        Failure::ComputationalLimit => host.unsupported(
            CapabilityId::FunctionAdapter,
            "irregular financial calculation exceeds its work envelope",
        ),
    }
}
/// Identify the six date-aware irregular financial operations.
pub(super) fn handles(function: FinancialFunction) -> bool {
    use FinancialFunction::*;
    matches!(
        function,
        Amordegrc | Amorlinc | Oddfprice | Oddfyield | Oddlprice | Oddlyield
    )
}
/// Adapts irregular-coupon and amortization functions to broadcast execution.
/// It converts date and numeric parameters under the workbook epoch before calling independent financial algorithms.
struct OddKernel {
    function: FinancialFunction,
    epoch: CalendarEpoch,
}
impl Kernel for OddKernel {
    type Signature = Variadic<Broadcast>;
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        use FinancialFunction::*;
        let function = self.function;
        let epoch = self.epoch;
        let amortization = matches!(function, Amordegrc | Amorlinc);
        let parameters: &[P] = if amortization {
            &[
                P::calendar_date(),
                P::calendar_date(),
                P::required(),
                P::required(),
                P::required(),
                P::required(),
                P::constant(0.0).integer_choices(&[0, 1, 2, 3, 4]),
            ]
        } else if matches!(function, Oddfprice | Oddfyield) {
            &[
                P::calendar_date(),
                P::calendar_date(),
                P::calendar_date(),
                P::calendar_date(),
                P::required(),
                P::required(),
                P::required(),
                P::required().integer_choices(&[1, 2, 4]),
                P::constant(0.0).integer_choices(&[0, 1, 2, 3, 4]),
            ]
        } else {
            &[
                P::calendar_date(),
                P::calendar_date(),
                P::calendar_date(),
                P::required(),
                P::required(),
                P::required(),
                P::required().integer_choices(&[1, 2, 4]),
                P::constant(0.0).integer_choices(&[0, 1, 2, 3, 4]),
            ]
        };
        let values = if amortization {
            NumericSignature::new(parameters).numbers_roles(
                context,
                [
                    arguments.get(1),
                    arguments.get(2),
                    arguments.get(0),
                    arguments.get(3),
                    arguments.get(4),
                    arguments.get(5),
                    arguments.get(6),
                ],
            )?
        } else {
            arguments.numbers(context, parameters)?
        };
        let host = &mut *context.host;
        let date = |index| Day::from_serial(values.get(index)?, epoch).map_err(spreadsheet);
        let value = if matches!(function, Amordegrc | Amorlinc) {
            let purchased = date(0)?;
            let first = date(1)?;
            let cost = values.get(2)?;
            let salvage = values.get(3)?;
            let period = values.get(4)?;
            let rate = values.get(5)?;
            let basis = Basis::from_number(values.get(6)?).map_err(spreadsheet)?;
            kernel::amortization(
                cost,
                purchased,
                first,
                salvage,
                period,
                rate,
                basis,
                if function == Amorlinc {
                    Amortization::Linear
                } else {
                    Amortization::Declining
                },
                |work| host.charge_work(work),
            )
            .map_err(|e| failure(host, e))?
        } else {
            let settlement = date(0)?;
            let maturity = date(1)?;
            let issue = date(2)?;
            let first = if matches!(function, Oddfprice | Oddfyield) {
                Some(date(3)?)
            } else {
                None
            };
            let start = if first.is_some() { 4 } else { 3 };
            let rate = values.get(start)?;
            let value = values.get(start + 1)?;
            let redemption = values.get(start + 2)?;
            let frequency = Frequency::from_number(values.get(start + 3)?).map_err(spreadsheet)?;
            let basis = Basis::from_number(values.get(start + 4)?).map_err(spreadsheet)?;
            if let Some(first) = first {
                let bond = OddFirst::new(
                    settlement,
                    maturity,
                    issue,
                    first,
                    rate,
                    redemption,
                    frequency,
                    basis,
                    |work| host.charge_work(work),
                )
                .map_err(|e| failure(host, e))?;
                if function == Oddfprice {
                    bond.price(value).map_err(spreadsheet)?
                } else {
                    bond.yield_rate(value, |work| host.charge_work(work))
                        .map_err(|e| failure(host, e))?
                }
            } else {
                let bond = OddLast::new(
                    settlement,
                    maturity,
                    issue,
                    rate,
                    redemption,
                    frequency,
                    basis,
                    |work| host.charge_work(work),
                )
                .map_err(|e| failure(host, e))?;
                if function == Oddlprice {
                    bond.price(value).map_err(spreadsheet)?
                } else {
                    bond.yield_rate(value).map_err(spreadsheet)?
                }
            }
        };
        Ok(Number(value))
    }
}
/// Coerce rectangular inputs, retain date context reads, and publish lifted numerical results.
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
    session.read(DependencyRead::ContextField(run::ContextRead::DateSystem));
    let epoch = match session.workbook.date_system {
        crate::model::DateSystem::Excel1900 => CalendarEpoch::Excel1900,
        crate::model::DateSystem::Excel1904 => CalendarEpoch::Excel1904,
    };
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    let result = elementwise::execute(&mut host, address, OddKernel { function, epoch });
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(e)) => Ok(err(e)),
        Err(e) => Err(operational_only(e)),
    }
}
