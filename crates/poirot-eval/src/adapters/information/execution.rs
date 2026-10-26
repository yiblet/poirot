//! Exact value inspection through shared typed argument and result traversal.
use crate::adapters::elementwise::{
    Broadcast, Kernel, MemberContext, SourceIdentity, execute as execute_kernel,
};
use crate::host::CallHost;
use crate::*;

/// Inspect one value without coercing errors, text, blank, or omission.
struct Information(InformationFunction);
impl Kernel for Information {
    type Signature = (Broadcast,);
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        (argument,): &Self::Signature,
    ) -> FunctionResult<CellValue> {
        use InformationFunction::*;
        let operand = match argument.read(context) {
            Ok(operand) => operand,
            Err(FunctionFailure::Spreadsheet(error)) => crate::host::ScalarOperand {
                value: Error(error),
                missing: false,
                origin: crate::host::OperandOrigin::Direct,
            },
            Err(error) => return Err(error),
        };
        let value = operand.value;
        let present = !operand.missing;
        Ok(match self.0 {
            IsError => Boolean(present && matches!(value, Error(_))),
            IsErr => Boolean(present && matches!(value, Error(error) if error != NA)),
            IsNa => Boolean(present && value == Error(NA)),
            IsNumber => Boolean(present && matches!(value, Number(_))),
            IsText => Boolean(present && value.text_ref().is_some()),
            IsBlank => Boolean(present && value == Blank),
            IsLogical => Boolean(present && matches!(value, Boolean(_))),
            IsNonText => Boolean(!present || value.text_ref().is_none()),
            N => poirot_coercionv2::compatibility::coercev2_information_n(value),
            T => poirot_coercionv2::compatibility::coercev2_information_t(value),
            Type | Na => return Err(EvaluationFailure::StalePlan.into()),
        })
    }
}
/// TYPE identifies materialized arrays before requesting scalar source contents.
struct TypeInformation;
impl Kernel for TypeInformation {
    type Signature = (SourceIdentity,);
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        (source,): &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let kind = source.kind(context.host)?;
        if kind == crate::host::OperandKind::Array {
            return Ok(Number(64.));
        }
        if kind == crate::host::OperandKind::Reference {
            let shape = source.describe(context.host)?;
            if shape.rows > 1 || shape.columns > 1 || shape.sheets > 1 {
                return Ok(Number(64.));
            }
        }
        let value = match source.scalar(context.host) {
            Ok(value) => value.value,
            Err(FunctionFailure::Spreadsheet(error)) => Error(error),
            Err(error) => return Err(error),
        };
        Ok(Number(match value {
            Number(_) | Blank => 1.,
            Text(_) | CellValue::TextUtf16(_) => 2.,
            Boolean(_) => 4.,
            Error(_) => 16.,
        }))
    }
}
/// Produce the constant error without preparing any source operand.
struct NotAvailable;
impl Kernel for NotAvailable {
    type Signature = ();
    fn apply(&self, _: &mut MemberContext<'_, '_, '_>, _: &()) -> FunctionResult<CellValue> {
        Ok(Error(NA))
    }
}
/// Execute information predicates and conversions over the admitted rectangle.
pub(crate) fn execute(
    session: &mut Session<'_>,
    s: usize,
    at: CellAddress,
    function: InformationFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !FunctionId::Information(function)
        .descriptor()
        .arity
        .accepts(args.len())
    {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_array(session, s, at, args, args.len());
    let result = if function == InformationFunction::Na {
        execute_kernel(&mut host, at, NotAvailable)
    } else if function == InformationFunction::Type {
        execute_kernel(&mut host, at, TypeInformation)
    } else {
        execute_kernel(&mut host, at, Information(function))
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}
