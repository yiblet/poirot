//! Fixed-rate euro conversion using typed broadcast argument roles.
use crate::eval::adapters::{
    elementwise::{
        self, Broadcast, Kernel, MemberContext, NumericParameter, NumericSignature, Optional,
    },
    failure::operational_only,
};
use crate::eval::host::{ArgumentCoercion, CallHost};
use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::{FunctionFailure, FunctionResult, VALUE_ERROR, Value, err};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellValue::{Error, Number};
use crate::model::{CellAddress, CellValue};
use poirot_functions::financial::euro::{EuroCurrency, euro_convert};

/// Admit a three-letter ISO currency code from one broadcast member.
fn currency(
    context: &mut MemberContext<'_, '_, '_>,
    role: &Broadcast,
) -> FunctionResult<EuroCurrency> {
    let operand = role.read(context)?;
    if let Error(error) = operand.value {
        return Err(error.into());
    }
    let text = operand.value.text_ref().ok_or(VALUE_ERROR)?;
    if text.len_units() != 3 {
        return Err(VALUE_ERROR.into());
    }
    EuroCurrency::parse(&ArgumentCoercion::new(context.host).project_text(text)?)
        .ok_or(VALUE_ERROR.into())
}

/// EUROCONVERT kernel that broadcasts amounts, currency codes, and precision options together.
/// It converts those distinct roles before applying the independent euro conversion rules.
struct EuroKernel;

impl Kernel for EuroKernel {
    type Signature = (
        Broadcast,
        Broadcast,
        Broadcast,
        Optional<Broadcast>,
        Optional<Broadcast>,
    );

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let number = NumericSignature::new(&[NumericParameter::required()])
            .numbers_roles(context, [Some(arguments.0)])?
            .get(0)?;
        let source = currency(context, &arguments.1)?;
        let target = currency(context, &arguments.2)?;
        let full = NumericSignature::new(&[NumericParameter::boolean_default(false)])
            .numbers_roles(context, [arguments.3.0])?
            .get(0)?
            != 0.0;
        let triangulation = if let Some(role) = arguments.4.0 {
            if role.read(context)?.missing {
                None
            } else {
                Some(
                    NumericSignature::new(&[NumericParameter::required()])
                        .numbers_roles(context, [Some(role)])?
                        .get(0)?,
                )
            }
        } else {
            None
        };
        euro_convert(number, source, target, full, triangulation)
            .map(Number)
            .map_err(super::modern::spreadsheet)
    }
}

/// Execute euro conversion through shared shape admission and member publication.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !(3..=5).contains(&args.len()) {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    match elementwise::execute(&mut host, address, EuroKernel) {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
