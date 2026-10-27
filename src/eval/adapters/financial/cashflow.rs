//! Whole cash-flow admission through shared typed collection traversal.
use crate::eval::adapters::{
    elementwise::{
        self, Broadcast, BroadcastCollections, Kernel, MemberContext, NumericParameter,
        NumericSignature, Optional, WholeCollection, collections,
    },
    failure::operational_only,
};
use crate::eval::functions::registry::{FinancialFunction, FunctionId};
use crate::eval::host::{ArgumentCoercion, CallHost};
use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::{
    FunctionFailure, FunctionResult, OperandOrigin, VALUE_ERROR, Value, err,
};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellValue::Number;
use crate::model::{CellAddress, CellValue};
use poirot_functions::financial as domain;

/// Source-ordered cash-flow collection retaining reference and array admission rules.
fn flows(
    host: &mut CallHost<'_, '_>,
    roles: impl IntoIterator<Item = WholeCollection>,
) -> FunctionResult<Vec<f64>> {
    collections::fold(host, roles, Vec::new(), |host, values, operand| {
        let origin = match operand.origin {
            OperandOrigin::Direct => poirot_functions::scalar::Origin::Direct,
            OperandOrigin::ArrayElement => poirot_functions::scalar::Origin::Array,
            OperandOrigin::ReferencedCell => poirot_functions::scalar::Origin::Reference,
        };
        let scalar = if operand.missing {
            poirot_functions::coercion::compatibility::ScalarRef::Missing
        } else {
            (&operand.value).into()
        };
        let value = poirot_functions::coercion::collections::coerce_cashflow_member::<
            FunctionFailure,
        >(scalar, origin, |input| match input {
            poirot_functions::coercion::compatibility::DeferredNumber::Text(text) => {
                let text = ArgumentCoercion::new(host).project_text(text)?;
                poirot_functions::coercion::compatibility::coerce_financial_decimal_text(&text)
                    .ok_or(VALUE_ERROR.into())
            }
            _ => Err(VALUE_ERROR.into()),
        })?;
        if let Some(value) = value {
            host.check_values(
                values
                    .len()
                    .checked_add(1)
                    .ok_or_else(|| host.size_overflow())?,
            )?;
            values
                .try_reserve(1)
                .map_err(|_| host.allocation_failure())?;
            // SAFETY: one additional slot was reserved immediately above.
            values.push(value);
        }
        Ok(())
    })
}

/// NPV kernel that broadcasts rates over complete, ordered cash-flow collections.
/// Each output converts the rate before collecting its flows to retain that demand order.
struct PresentFlows;

impl Kernel for PresentFlows {
    type Signature = BroadcastCollections;

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let rate = NumericSignature::new(&[NumericParameter::required()])
            .numbers_roles(context, [Some(arguments.scalar)])?
            .get(0)?;
        let values = flows(context.host, arguments.collections.iter())?;
        context.host.charge_work(values.len() as u64)?;
        domain::npv(rate, &values)
            .map(Number)
            .map_err(super::modern::spreadsheet)
    }
}

/// IRR kernel that collects one cash-flow sample before broadcasting initial guesses.
/// Each output searches for a return rate against the same admitted flows.
struct ReturnFlows(Vec<f64>);

impl Kernel for ReturnFlows {
    type Signature = (WholeCollection, Optional<Broadcast>);

    fn prepare(
        &mut self,
        host: &mut CallHost<'_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<()> {
        self.0 = flows(host, [arguments.0])?;
        Ok(())
    }

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let guess = NumericSignature::new(&[NumericParameter::constant(0.1)])
            .numbers_roles(context, [arguments.1.0])?
            .get(0)?;
        context.host.check_values(36)?;
        context.host.charge_work(
            (self.0.len() as u64)
                .saturating_mul(3038)
                .saturating_add(4096),
        )?;
        domain::irr(&self.0, guess)
            .map(Number)
            .map_err(super::modern::spreadsheet)
    }
}

/// MIRR kernel that reuses one cash-flow sample across broadcast finance and reinvestment rates.
struct ModifiedFlows(Vec<f64>);

impl Kernel for ModifiedFlows {
    type Signature = (WholeCollection, Broadcast, Broadcast);

    fn prepare(
        &mut self,
        host: &mut CallHost<'_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<()> {
        self.0 = flows(host, [arguments.0])?;
        Ok(())
    }

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let values = NumericSignature::new(&[NumericParameter::required(); 2])
            .numbers_roles(context, [Some(arguments.1), Some(arguments.2)])?;
        context.host.charge_work(self.0.len() as u64)?;
        domain::mirr(&self.0, values.get(0)?, values.get(1)?)
            .map(Number)
            .map_err(super::modern::spreadsheet)
    }
}

/// Execute cash-flow functions through shared whole-collection and broadcast admission.
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
        FinancialFunction::Npv => elementwise::execute(&mut host, address, PresentFlows),
        FinancialFunction::Irr => elementwise::execute(&mut host, address, ReturnFlows(Vec::new())),
        FinancialFunction::Mirr => {
            elementwise::execute(&mut host, address, ModifiedFlows(Vec::new()))
        }
        _ => return Ok(err(VALUE_ERROR)),
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
