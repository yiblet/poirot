//! Date-aware rectangular operands for irregular coupons and French depreciation.
use crate::eval::adapters::{
    elementwise::{
        self, Broadcast, Kernel, MemberContext, NumericParameter, NumericSignature, Variadic,
    },
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
    odd::{self as kernel, Amortization, OddFirst, OddLast},
    securities::{Basis, Day, Frequency},
};

fn spreadsheet(error: FinancialError) -> FunctionError {
    match error {
        FinancialError::Num => Num,
        FinancialError::Value => VALUE_ERROR,
        FinancialError::DivZero => Div0,
    }
    .into()
}

fn failure<E: Into<FunctionError>>(
    host: &CallHost<'_, '_>,
    error: FinancialCalculationError<E>,
) -> FunctionError {
    match error {
        FinancialCalculationError::Spreadsheet(e) => spreadsheet(e),
        FinancialCalculationError::Operational(e) => e.into(),
        FinancialCalculationError::ComputationalLimit => host.unsupported(
            CapabilityId::FunctionAdapter,
            crate::eval::limits::EvaluationReasonError::IrregularFinancialCalculationExceedsItsWorkEnvelope,
        ),
    }
}

/// Identify the six date-aware irregular financial operations.
pub(super) fn handles(function: FinancialFunction) -> bool {
    use FinancialFunction::{Amordegrc, Amorlinc, Oddfprice, Oddfyield, Oddlprice, Oddlyield};
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
        use FinancialFunction::{Amordegrc, Amorlinc, Oddfprice, Oddfyield, Oddlprice};
        let function = self.function;
        let epoch = self.epoch;
        let amortization = matches!(function, Amordegrc | Amorlinc);
        let parameters: &[NumericParameter] = if amortization {
            &[
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::constant(0.0).integer_choices(&[0, 1, 2, 3, 4]),
            ]
        } else if matches!(function, Oddfprice | Oddfyield) {
            &[
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required().integer_choices(&[1, 2, 4]),
                NumericParameter::constant(0.0).integer_choices(&[0, 1, 2, 3, 4]),
            ]
        } else {
            &[
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::calendar_date(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required().integer_choices(&[1, 2, 4]),
                NumericParameter::constant(0.0).integer_choices(&[0, 1, 2, 3, 4]),
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
    ReadRecorder::new(session).read(DependencyRead::ContextField(
        crate::eval::ports::ContextRead::DateSystem,
    ));
    let epoch = match session.sources.workbook.date_system {
        crate::model::DateSystem::Excel1900 => CalendarEpoch::Excel1900,
        crate::model::DateSystem::Excel1904 => CalendarEpoch::Excel1904,
    };
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    let result = elementwise::execute(&mut host, address, OddKernel { function, epoch });
    match result {
        Ok(value) => Ok(value),
        Err(FunctionError::Spreadsheet(e)) => Ok(err(e)),
        Err(e) => Err(operational_only(e)),
    }
}
