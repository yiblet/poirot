//! Logical scalar and recovery kernels over reusable source and fallback strategies.
use crate::eval::adapters::elementwise::recovery::{self, ErrorSelector};
use crate::eval::adapters::elementwise::{
    self, Broadcast, Deferred, Kernel, MemberContext, Prepared, Signature, SourceIdentity,
};
use crate::eval::functions::registry::LogicalFunction;
use crate::eval::host::{CallHost, ErrorHandler, OperandKind};
use crate::eval::limits::EvalResult;
use crate::eval::observe::OperationId;
use crate::eval::values::{FunctionError, FunctionResult, VALUE_ERROR, Value, core_projection};
use crate::model::CellError::NA;
use crate::model::CellValue::{Blank, Boolean, Error, Number};
use crate::model::{CellAddress, CellError, CellValue};

/// Kernel for NOT, parity tests, and ERROR.TYPE over one broadcast input.
/// It preserves spreadsheet errors as values until the selected operation decides how to handle them.
pub(super) struct Unary(pub(super) LogicalFunction);

impl Kernel for Unary {
    type Signature = (Broadcast,);

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let value = match arguments.0.read(context) {
            Ok(operand) => operand.value,
            Err(FunctionError::Spreadsheet(error)) => Error(error),
            Err(error) => return Err(error),
        };
        Ok(match self.0 {
            LogicalFunction::ErrorType => match value {
                Error(error) => poirot_functions::logical::error_type(error)
                    .map(|code| Number(f64::from(code)))
                    .unwrap_or(Error(NA)),
                _ => Error(NA),
            },
            LogicalFunction::Not => Boolean(!core_projection::coerce_operand_boolean(&value)?),
            LogicalFunction::IsEven | LogicalFunction::IsOdd => {
                let number = match value {
                    Number(value) => value,
                    Blank => 0.0,
                    Error(error) => return Err(error.into()),
                    _ => return Err(VALUE_ERROR.into()),
                };
                Boolean((number.trunc().abs() % 2.0 == 0.0) == (self.0 == LogicalFunction::IsEven))
            }
            _ => return Err(VALUE_ERROR.into()),
        })
    }
}

/// TRUE or FALSE kernel with no source operands and a single scalar result.
pub(super) struct Constant(pub(super) bool);

impl Kernel for Constant {
    type Signature = ();

    fn apply(
        &self,
        _context: &mut MemberContext<'_, '_, '_>,
        _arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        Ok(Boolean(self.0))
    }
}

/// Inspect reference identity without reading the referenced cell contents.
pub(super) struct IsReference;

impl Kernel for IsReference {
    type Signature = (SourceIdentity,);

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        Ok(Boolean(
            arguments.0.kind(context.host)? == OperandKind::Reference,
        ))
    }
}

/// IFERROR or IFNA policy for shared per-member error recovery.
/// It decides which spreadsheet errors select the deferred fallback while operational failures propagate.
pub(super) struct Recovery(pub(super) LogicalFunction);

impl ErrorSelector for Recovery {
    fn catches(&self, error: CellError) -> bool {
        self.0 == LogicalFunction::IfError || error == NA
    }

    fn uncaught(
        &self,
        host: &mut CallHost<'_, '_>,
        source: Option<OperationId>,
        checked: &Value,
    ) -> EvalResult<()> {
        let checked = ErrorHandler::new(host).inspect_scalar(checked)?;
        ErrorHandler::new(host).mask(self.0, source, &checked)
    }

    fn fallback(
        &self,
        host: &mut CallHost<'_, '_>,
        source: Option<OperationId>,
        checked: &CellValue,
        _role: &Deferred<Broadcast>,
    ) -> EvalResult<Prepared<Broadcast>> {
        ErrorHandler::new(host)
            .fallback(self.0, source, checked)
            .map(Prepared::new)
    }
}

/// Run reusable logical kernels after family-owned arity admission.
pub(super) fn execute(
    host: &mut CallHost<'_, '_>,
    at: CellAddress,
    function: LogicalFunction,
) -> FunctionResult<Value> {
    match function {
        LogicalFunction::True | LogicalFunction::False => {
            elementwise::execute(host, at, Constant(function == LogicalFunction::True))
        }
        LogicalFunction::IsRef => elementwise::execute(host, at, IsReference),
        LogicalFunction::IfError | LogicalFunction::IfNa => {
            let (source, fallback) = <(Deferred<Broadcast>, Deferred<Broadcast>)>::bind(host.len());
            recovery::execute(host, source, fallback, Recovery(function))
        }
        _ => elementwise::execute(host, at, Unary(function)),
    }
}
