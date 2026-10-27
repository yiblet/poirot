//! Bounded numeric lifting for documented real engineering special functions.
use crate::eval::adapters::elementwise::{Broadcast, MemberContext, NumericParameter, Variadic};
use crate::eval::functions::registry::EngineeringFunction;
use crate::eval::limits::{CapabilityId, EvalResult};
use crate::eval::session::Session;
use crate::eval::values::{FunctionFailure, FunctionResult, VALUE_ERROR, Value};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellError::Num;
use crate::model::CellValue::Number;
use crate::model::{CellAddress, CellValue};
use poirot_functions::engineering::special::{self as kernel, Bessel};

/// Recognize the documented special-function execution path.
pub(super) fn handles(function: EngineeringFunction) -> bool {
    use EngineeringFunction::{
        Besseli, Besselj, Besselk, Bessely, Delta, Erf, ErfPrecise, Erfc, ErfcPrecise, Gestep,
    };
    matches!(
        function,
        Besseli
            | Besselk
            | Bessely
            | Besselj
            | Delta
            | Erf
            | ErfPrecise
            | Erfc
            | ErfcPrecise
            | Gestep
    )
}

/// Runs real-valued engineering functions with shared ordered numeric conversion.
/// The selected function supplies defaults and chooses the independent algorithm for each output member.
struct Special(EngineeringFunction);

impl crate::eval::adapters::elementwise::Kernel for Special {
    type Signature = Variadic<Broadcast>;

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        use EngineeringFunction::{
            Besseli, Besselj, Besselk, Bessely, Delta, Erf, ErfPrecise, Erfc, ErfcPrecise, Gestep,
        };
        let parameters: &[NumericParameter] = match self.0 {
            Gestep | Delta => &[
                NumericParameter::required(),
                NumericParameter::constant(0.0),
            ],
            Erf if arguments.len() > 1 => &[
                NumericParameter::required(),
                NumericParameter::constant(0.0),
            ],
            Erf | ErfPrecise | Erfc | ErfcPrecise => &[NumericParameter::required()],
            _ => &[NumericParameter::required(), NumericParameter::required()],
        };
        let numbers = arguments.numbers(context, parameters)?;
        let x = numbers.get(0)?;
        let value = match self.0 {
            Gestep | Delta => {
                let y = numbers.get(1)?;
                if self.0 == Gestep {
                    kernel::gestep(x, y)
                } else {
                    poirot_functions::engineering::delta(x, y)
                }
            }
            Erf => {
                if arguments.len() > 1 {
                    kernel::erf_interval(x, numbers.get(1)?)
                } else {
                    kernel::erf(x)
                }
            }
            ErfPrecise => kernel::erf(x),
            Erfc | ErfcPrecise => kernel::erfc(x),
            Besseli | Besselk | Bessely | Besselj => {
                let order = numbers.get(1)?;
                let kind = match self.0 {
                    Besseli => Bessel::ModifiedFirst,
                    Besselk => Bessel::ModifiedSecond,
                    Besselj => Bessel::First,
                    _ => Bessel::Second,
                };
                kernel::bessel(kind, x, order, |units| context.host.charge_work(units)).map_err(
                    |error| match error {
                        kernel::Failure::Number => FunctionFailure::Spreadsheet(Num),
                        kernel::Failure::Operational(error) => error.into(),
                        kernel::Failure::ComputationalLimit => context.host.unsupported(
                            CapabilityId::FunctionAdapter,
                            "Bessel computation exceeds the bounded order/series envelope",
                        ),
                    },
                )?
            }
            _ => return Err(VALUE_ERROR.into()),
        };
        if value.is_finite() {
            Ok(Number(value))
        } else {
            Err(Num.into())
        }
    }
}

/// Evaluate special functions through shared typed argument admission and lifting.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: EngineeringFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    super::dispatch::run(
        session,
        sheet,
        address,
        function,
        arguments,
        Special(function),
    )
}
