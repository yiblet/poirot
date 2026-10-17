//! Family-owned evaluation; generic argument coercion remains in the execution core.
use crate::host::{CallHost, MissingPolicy, NumericPolicy};
use crate::*;
impl Session<'_> {
    /// Dispatch mathematical builtins while preserving argument and numeric outcome policies.
    pub(crate) fn math_function(
        &mut self,
        s: usize,
        at: CellAddress,
        name: MathFunction,
        args: ArgumentsRef<'_>,
    ) -> EvalResult<Value> {
        if name == MathFunction::SeriesSum {
            return super::series::execute(self, s, at, args);
        }
        if matches!(name, MathFunction::Base | MathFunction::Decimal) {
            if !FunctionId::Math(name)
                .descriptor()
                .arity
                .accepts(args.len())
            {
                return Ok(err(VALUE_ERROR));
            }
            let mut host = CallHost::new(self, s, at, args);
            host.check_values(args.len())
                .map_err(crate::adapters::failure::operational_only)?;
            let indices: poirot_functions_core::ParamVec<_> = (0..args.len()).collect();
            return match super::radix::evaluate(&mut host, name == MathFunction::Base, &indices) {
                Ok(value) => Ok(value.into()),
                Err(super::policy::Failure::Spreadsheet(error)) => Ok(err(error)),
                Err(super::policy::Failure::Operational(failure)) => Err(failure),
                Err(super::policy::Failure::UnsupportedCapability(capability)) => Err(
                    crate::adapters::failure::operational_only(host.unsupported(
                        CapabilityId::FunctionAdapter,
                        format!("radix requires {capability:?}"),
                    )),
                ),
            };
        }
        if let Some(function) = name.helper() {
            return adapters::math::execute(self, s, at, function, args);
        }
        if name == MathFunction::Pi {
            return Ok(if args.is_empty() {
                Number(std::f64::consts::PI).into()
            } else {
                err(VALUE_ERROR)
            });
        }
        // NumericFunction.oneDouble/twoDouble, LOG and TRUNC check arity
        // before resolving arguments, then coerce each operand in source order.
        // Keep arithmetic below separate from OperandResolver's numeric policy.
        if !FunctionId::Math(name)
            .descriptor()
            .arity
            .accepts(args.len())
        {
            return Ok(err(VALUE_ERROR));
        }
        let values = {
            let mut host = CallHost::new(self, s, at, args);
            match host.check_values(args.len()) {
                Ok(()) => (),
                Err(FunctionFailure::Operational(failure)) => return Err(failure),
                Err(FunctionFailure::Spreadsheet(error)) => return Ok(err(error)),
            }
            let mut values = Vec::with_capacity(args.len());
            for index in 0..args.len() {
                match host.coercev2_argument_number(
                    index,
                    MissingPolicy::Blank,
                    NumericPolicy::PinnedPoi,
                ) {
                    Ok(value) => values.push(Number(value)),
                    Err(FunctionFailure::Spreadsheet(error)) => return Ok(err(error)),
                    Err(FunctionFailure::Operational(failure)) => return Err(failure),
                }
            }
            values
        };
        Ok(match self.math_scalar(name, &values) {
            Ok(value) => value.into(),
            Err(error) => err(error),
        })
    }
    fn math_scalar(&self, name: MathFunction, a: &[CellValue]) -> Result<CellValue, CellError> {
        let n = a.len();
        if !FunctionId::Math(name).descriptor().arity.accepts(n) {
            return Err(VALUE_ERROR);
        }
        // The caller already performed the budgeted, source-selected coercion.
        // This pure arithmetic boundary accepts only those numeric results.
        let num = |i: usize| match a.get(i) {
            Some(Number(value)) => Ok(*value),
            _ => Err(VALUE_ERROR),
        };
        let x = num(0)?;
        let result = match name {
            MathFunction::Rand
            | MathFunction::Randbetween
            | MathFunction::Acosh
            | MathFunction::Asinh
            | MathFunction::Atan2
            | MathFunction::Atanh
            | MathFunction::CeilingMath
            | MathFunction::CeilingPrecise
            | MathFunction::Combina
            | MathFunction::Combin
            | MathFunction::Cosh
            | MathFunction::Even
            | MathFunction::Fact
            | MathFunction::Factdouble
            | MathFunction::FloorMath
            | MathFunction::FloorPrecise
            | MathFunction::Gcd
            | MathFunction::Lcm
            | MathFunction::Mround
            | MathFunction::Odd
            | MathFunction::Quotient
            | MathFunction::Sinh
            | MathFunction::Sqrtpi
            | MathFunction::Tanh => return Err(VALUE_ERROR),
            MathFunction::Abs => x.abs(),
            MathFunction::Sign => {
                if x == 0. {
                    0.
                } else {
                    x.signum()
                }
            }
            MathFunction::Sqrt => x.sqrt(),
            MathFunction::Exp => poirot_functions_math::transcendental::exp(x),
            MathFunction::Ln => x.ln(),
            MathFunction::Log10 => poirot_functions_math::transcendental::log10(x),
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
            MathFunction::Pi
            | MathFunction::Base
            | MathFunction::Decimal
            | MathFunction::SeriesSum => {
                return Err(VALUE_ERROR);
            }
            MathFunction::Trunc
            | MathFunction::Round
            | MathFunction::RoundUp
            | MathFunction::RoundDown => {
                let digits = if n == 2 { num(1)? } else { 0. };
                match name {
                    MathFunction::Round => poirot_functions_math::round(x, digits)?,
                    MathFunction::RoundUp => poirot_functions_math::round_up(x, digits)?,
                    _ => poirot_functions_math::round_down(x, digits)?,
                }
            }
        };
        Ok(finite(result))
    }
}
