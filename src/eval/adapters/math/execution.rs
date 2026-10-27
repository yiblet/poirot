//! Shared typed mathematical dispatch and pure numeric operation selection.
use crate::eval::functions::registry::{FunctionId, MathFunction};
use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::{VALUE_ERROR, Value, finite};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellError::{Div0, Num};
use crate::model::CellValue::Number;
use crate::model::{CellAddress, CellError, CellValue};

/// Execute mathematical operations through shared scalar or collection signatures.
pub(in crate::eval) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    at: CellAddress,
    name: MathFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    super::modern::execute(session, sheet, at, name, args)
}

/// Apply shared math arithmetic to selected and coerced numeric operands.
pub(super) fn scalar(name: MathFunction, a: &[f64]) -> Result<CellValue, CellError> {
    let n = a.len();
    if !FunctionId::Math(name).descriptor().arity.accepts(n) {
        return Err(VALUE_ERROR);
    }
    // Callers select and coerce operands through their own scalar or broadcast policy.
    // This pure arithmetic boundary accepts only those numeric results.
    let num = |i: usize| match a.get(i) {
        Some(value) => Ok(*value),
        _ => Err(VALUE_ERROR),
    };
    let x = if n == 0 { 0.0 } else { num(0)? };
    let result = match name {
        MathFunction::Pi => std::f64::consts::PI,
        MathFunction::Acosh => poirot_functions::math::acosh(x)?,
        MathFunction::Asinh => poirot_functions::math::asinh(x)?,
        MathFunction::Atanh => poirot_functions::math::atanh(x)?,
        MathFunction::Cosh => poirot_functions::math::cosh(x)?,
        MathFunction::Sinh => poirot_functions::math::sinh(x)?,
        MathFunction::Tanh => poirot_functions::math::tanh(x)?,
        MathFunction::Sqrtpi => poirot_functions::math::sqrtpi(x)?,
        MathFunction::Even => poirot_functions::math::even(x)?,
        MathFunction::Odd => poirot_functions::math::odd(x)?,
        MathFunction::Fact => poirot_functions::math::fact(x)?,
        MathFunction::Factdouble => poirot_functions::math::factdouble(x)?,
        MathFunction::Atan2 => poirot_functions::math::atan2(x, num(1)?)?,
        MathFunction::Quotient => poirot_functions::math::quotient(x, num(1)?)?,
        MathFunction::Combin => poirot_functions::math::combin(x, num(1)?)?,
        MathFunction::Mround => poirot_functions::math::mround(x, num(1)?)?,
        MathFunction::Acot => poirot_functions::math::modern::acot(x)?,
        MathFunction::Acoth => poirot_functions::math::modern::acoth(x)?,
        MathFunction::Cot
        | MathFunction::Coth
        | MathFunction::Csc
        | MathFunction::Csch
        | MathFunction::Sec
        | MathFunction::Sech => {
            use poirot_functions::math::modern::{Reciprocal, reciprocal};
            let kind = match name {
                MathFunction::Cot => Reciprocal::Cot,
                MathFunction::Coth => Reciprocal::Coth,
                MathFunction::Csc => Reciprocal::Csc,
                MathFunction::Csch => Reciprocal::Csch,
                MathFunction::Sec => Reciprocal::Sec,
                _ => Reciprocal::Sech,
            };
            reciprocal(x, kind)?
        }
        MathFunction::IsoCeiling if x == 0.0 || num(1)? == 0.0 => 0.0,
        MathFunction::IsoCeiling | MathFunction::CeilingPrecise => {
            poirot_functions::math::ceiling_precise(x, num(1)?)?
        }
        MathFunction::FloorPrecise => poirot_functions::math::floor_precise(x, num(1)?)?,
        MathFunction::CeilingMath | MathFunction::FloorMath => {
            let mode = if num(2)? == 0.0 { 0.0 } else { -1.0 };
            if name == MathFunction::CeilingMath {
                poirot_functions::math::ceiling_math(x, num(1)?, mode)?
            } else {
                poirot_functions::math::floor_math(x, num(1)?, mode)?
            }
        }
        MathFunction::Combina
        | MathFunction::Multinomial
        | MathFunction::Rand
        | MathFunction::Randbetween
        | MathFunction::Gcd
        | MathFunction::Lcm => return Err(VALUE_ERROR),
        MathFunction::Abs => x.abs(),
        MathFunction::Sign => {
            if x == 0. {
                0.
            } else {
                x.signum()
            }
        }
        MathFunction::Sqrt => x.sqrt(),
        MathFunction::Exp => poirot_functions::math::transcendental::exp(x),
        MathFunction::Ln => x.ln(),
        MathFunction::Log10 => poirot_functions::math::transcendental::log10(x),
        MathFunction::Log => {
            let b = if n == 2 { num(1)? } else { 10. };
            if x <= 0. || b <= 0. || b == 1. {
                return Err(Num);
            }
            x.log(b)
        }
        MathFunction::Power => {
            let exponent = num(1)?;
            if x == 0. && exponent == 0. {
                return Err(Num);
            }
            if x == 0. && exponent < 0. {
                return Err(Div0);
            }
            x.powf(exponent)
        }
        MathFunction::Mod => {
            let y = num(1)?;
            if y == 0. {
                return Err(Div0);
            }
            x - y * (x / y).floor()
        }
        MathFunction::Int => x.floor(),
        MathFunction::Sin => x.sin(),
        MathFunction::Cos => x.cos(),
        MathFunction::Tan => x.tan(),
        MathFunction::Asin => x.asin(),
        MathFunction::Acos => x.acos(),
        MathFunction::Atan => x.atan(),
        MathFunction::Degrees => x.to_degrees(),
        MathFunction::Radians => x.to_radians(),
        MathFunction::Ceiling | MathFunction::Floor => {
            let y = num(1)?;
            if y == 0. {
                // POI NumericFunction.FLOOR admits zero/zero before DIV0;
                // both operands have already been coerced in source order.
                return if name == MathFunction::Ceiling || x == 0. {
                    Ok(Number(0.))
                } else {
                    Err(Div0)
                };
            }
            if x > 0. && y < 0. {
                return Err(Num);
            }
            let q = x / y;
            if name == MathFunction::Ceiling {
                q.ceil() * y
            } else {
                q.floor() * y
            }
        }
        MathFunction::Base | MathFunction::Decimal | MathFunction::SeriesSum => {
            return Err(VALUE_ERROR);
        }
        MathFunction::Trunc
        | MathFunction::Round
        | MathFunction::RoundUp
        | MathFunction::RoundDown => {
            let digits = if n == 2 { num(1)? } else { 0. };
            match name {
                MathFunction::Round => poirot_functions::math::round(x, digits)?,
                MathFunction::RoundUp => poirot_functions::math::round_up(x, digits)?,
                _ => poirot_functions::math::round_down(x, digits)?,
            }
        }
    };
    Ok(finite(result))
}
