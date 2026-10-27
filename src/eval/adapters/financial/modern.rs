//! Modern scalar finance lifting with explicitly consumed schedule/date collections.
use crate::eval::adapters::{
    elementwise::{
        self, Broadcast, Kernel, MemberContext, NumericParameter, NumericSignature, Optional,
        Variadic, WholeCollection,
        collections::{self, Admission},
    },
    operational_only::operational_only,
};
use crate::eval::functions::registry::{FinancialFunction, FunctionId};
use crate::eval::host::{ArgumentCoercion, CallHost};
use crate::eval::limits::{CapabilityId, EvalResult};
use crate::eval::session::Session;
use crate::eval::values::{FunctionError, FunctionResult, VALUE_ERROR, Value, err};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellError::{Div0, Num};
use crate::model::CellValue::{Error, Number};
use crate::model::{CellAddress, CellValue};
use poirot_functions::financial::{
    FinancialError,
    modern::{self as kernel, DatedFlow, LoanComponent},
};

pub(super) fn spreadsheet(error: FinancialError) -> FunctionError {
    match error {
        FinancialError::Num => Num,
        FinancialError::DivZero => Div0,
        FinancialError::Value => VALUE_ERROR,
    }
    .into()
}

fn numerical<E: Into<FunctionError>>(
    host: &CallHost<'_, '_>,
    error: poirot_functions::financial::FinancialCalculationError<E>,
) -> FunctionError {
    match error {
        poirot_functions::financial::FinancialCalculationError::Spreadsheet(error) => spreadsheet(error),
        poirot_functions::financial::FinancialCalculationError::Operational(error) => error.into(),
        poirot_functions::financial::FinancialCalculationError::ComputationalLimit => host.unsupported(
            CapabilityId::FunctionAdapter,
            crate::eval::limits::EvaluationReasonError::FinancialComputationExceedsItsBoundedWorkEnvelope,
        ),
    }
}

/// Prepare paired dated flows with checked shared traversal and calendar services.
fn dated_flows(
    host: &mut CallHost<'_, '_>,
    values: &WholeCollection,
    dates: &WholeCollection,
) -> FunctionResult<Vec<DatedFlow>> {
    if values.len(host)? == 0 {
        return Err(Num.into());
    }
    collections::zip_fold(
        host,
        values,
        dates,
        Num,
        (Vec::new(), None),
        |_host, _state, operand| {
            if operand.missing {
                return Err(VALUE_ERROR.into());
            }
            match poirot_functions::coercion::collections::coerce_aggregate_argument(
                operand.value,
                poirot_functions::scalar::Origin::Direct,
            ) {
                Number(value) => Ok(value),
                Error(error) => Err(error.into()),
                _ => Err(VALUE_ERROR.into()),
            }
        },
        |host, state, value, operand| {
            if operand.missing {
                return Err(VALUE_ERROR.into());
            }
            let date = match operand.value {
                Number(number) if number.is_finite() && number >= 0.0 => {
                    ArgumentCoercion::new(host)
                        .coerce_calendar_date(&Number(number.trunc()))
                        .map_err(|error| match error {
                            FunctionError::Spreadsheet(_) => {
                                FunctionError::Spreadsheet(VALUE_ERROR)
                            }
                            other => other,
                        })?
                        .trunc()
                }
                Error(error) => return Err(error.into()),
                _ => return Err(VALUE_ERROR.into()),
            };
            let first = *state.1.get_or_insert(date);
            if date < first {
                return Err(Num.into());
            }
            let len = state
                .0
                .len()
                .checked_add(1)
                .ok_or_else(|| host.size_overflow())?;
            host.check_values(len)?;
            state
                .0
                .try_reserve(1)
                .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
            // SAFETY: one additional slot was reserved immediately above.
            state.0.push(DatedFlow {
                value,
                days: date - first,
            });
            Ok(())
        },
    )
    .map(|state| state.0)
}

/// FVSCHEDULE kernel that prepares one interest-rate schedule and applies it to each broadcast principal.
struct ScheduleFinance {
    values: Vec<f64>,
}

impl Kernel for ScheduleFinance {
    type Signature = (Broadcast, WholeCollection);

    fn prepare(
        &mut self,
        host: &mut CallHost<'_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<()> {
        self.values = collections::collect(host, [arguments.1], Admission::BlankZero)?;
        Ok(())
    }

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let values = NumericSignature::new(&[NumericParameter::required()])
            .numbers_roles(context, [Some(arguments.0)])?;
        context
            .host
            .charge_work((self.values.len() as u64).saturating_mul(2))?;
        kernel::fvschedule(values.get(0)?, &self.values)
            .map(Number)
            .map_err(spreadsheet)
    }
}

/// XNPV kernel that prepares one dated cash-flow sample and discounts it at each broadcast rate.
struct PresentFinance {
    flows: Vec<DatedFlow>,
}

impl Kernel for PresentFinance {
    type Signature = (Broadcast, WholeCollection, WholeCollection);

    fn prepare(
        &mut self,
        host: &mut CallHost<'_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<()> {
        self.flows = dated_flows(host, &arguments.1, &arguments.2)?;
        Ok(())
    }

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let values = NumericSignature::new(&[NumericParameter::required()])
            .numbers_roles(context, [Some(arguments.0)])?;
        context
            .host
            .charge_work((self.flows.len() as u64).saturating_mul(3))?;
        kernel::xnpv(values.get(0)?, &self.flows)
            .map(Number)
            .map_err(spreadsheet)
    }
}

/// XIRR kernel that prepares one dated cash-flow sample and solves it from each broadcast guess.
struct ReturnFinance {
    flows: Vec<DatedFlow>,
}

impl Kernel for ReturnFinance {
    type Signature = (WholeCollection, WholeCollection, Optional<Broadcast>);

    fn prepare(
        &mut self,
        host: &mut CallHost<'_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<()> {
        self.flows = dated_flows(host, &arguments.0, &arguments.1)?;
        Ok(())
    }

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let values = NumericSignature::new(&[NumericParameter::constant(0.1)])
            .numbers_roles(context, [arguments.2.0])?;
        kernel::xirr(&self.flows, values.get(0)?, |work| {
            context.host.charge_work(work)
        })
        .map(Number)
        .map_err(|error| numerical(context.host, error))
    }
}

/// Adapts scalar financial functions to broadcast traversal with declared numeric defaults.
/// Each output invokes the selected independent financial calculation after conversion.
struct NumericFinance(FinancialFunction);

impl Kernel for NumericFinance {
    type Signature = Variadic<Broadcast>;

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        use FinancialFunction::{
            Cumipmt, Cumprinc, Db, Ddb, Effect, Ispmt, Nominal, Pduration, Rri, Sln, Syd, Vdb,
        };
        let function = self.0;
        let parameters: &[NumericParameter] = match function {
            Effect | Nominal => &[NumericParameter::required(); 2],
            Pduration | Rri | Sln => &[NumericParameter::required(); 3],
            Syd | Ispmt => &[NumericParameter::required(); 4],
            Db => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::constant(12.0),
            ],
            Ddb => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::constant(2.0),
            ],
            Cumipmt | Cumprinc => &[NumericParameter::required(); 6],
            Vdb => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::constant(2.0),
                NumericParameter::boolean_default(false),
            ],
            _ => return Err(VALUE_ERROR.into()),
        };
        let values = arguments.numbers(context, parameters)?;
        let host = &mut *context.host;
        let n = |index| values.get(index);
        let value = match function {
            Effect => kernel::effect(n(0)?, n(1)?).map_err(spreadsheet)?,
            Nominal => kernel::nominal(n(0)?, n(1)?).map_err(spreadsheet)?,
            Pduration => kernel::pduration(n(0)?, n(1)?, n(2)?).map_err(spreadsheet)?,
            Rri => kernel::rri(n(0)?, n(1)?, n(2)?).map_err(spreadsheet)?,
            Sln => kernel::sln(n(0)?, n(1)?, n(2)?).map_err(spreadsheet)?,
            Syd => kernel::syd(n(0)?, n(1)?, n(2)?, n(3)?).map_err(spreadsheet)?,
            Ispmt => kernel::ispmt(n(0)?, n(1)?, n(2)?, n(3)?).map_err(spreadsheet)?,
            Db => kernel::db(n(0)?, n(1)?, n(2)?, n(3)?, n(4)?).map_err(spreadsheet)?,
            Ddb => kernel::ddb(n(0)?, n(1)?, n(2)?, n(3)?, n(4)?).map_err(spreadsheet)?,
            Cumipmt | Cumprinc => kernel::cumulative(
                if function == Cumipmt {
                    LoanComponent::Interest
                } else {
                    LoanComponent::Principal
                },
                n(0)?,
                n(1)?,
                n(2)?,
                n(3)?,
                n(4)?,
                n(5)?,
            )
            .map_err(spreadsheet)?,
            Vdb => {
                let (cost, salvage, life, start, end, factor) =
                    (n(0)?, n(1)?, n(2)?, n(3)?, n(4)?, n(5)?);
                let no_switch = n(6)? != 0.0;
                kernel::vdb(
                    kernel::Asset {
                        cost,
                        salvage,
                        life,
                    },
                    start,
                    end,
                    factor,
                    no_switch,
                    |work| host.charge_work(work),
                )
                .map_err(|error| numerical(host, error))?
            }
            _ => return Err(VALUE_ERROR.into()),
        };
        Ok(Number(value))
    }
}

/// Evaluate documented financial functions without changing pinned helper behavior.
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
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    let result = match function {
        FinancialFunction::Fvschedule => {
            elementwise::execute(&mut host, address, ScheduleFinance { values: Vec::new() })
        }
        FinancialFunction::Xnpv => {
            elementwise::execute(&mut host, address, PresentFinance { flows: Vec::new() })
        }
        FinancialFunction::Xirr => {
            elementwise::execute(&mut host, address, ReturnFinance { flows: Vec::new() })
        }
        _ => elementwise::execute(&mut host, address, NumericFinance(function)),
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
