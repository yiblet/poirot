//! Mathematical execution with shared numeric tables and collection folds.
use crate::eval::adapters::{
    elementwise::{
        self, Broadcast, Kernel, MemberContext, NumericParameter as P, Variadic, WholeCollection,
        collections::{self, CollectionKernel},
    },
    failure::operational_only,
};
use crate::eval::host::{CallHost, NumericPolicy, OperandOrigin};
use crate::eval::*;

/// Adapts scalar math functions to broadcast traversal and ordered numeric conversion.
/// The selected function supplies parameter defaults and the independent calculation.
struct Numeric(MathFunction);
impl Kernel for Numeric {
    type Signature = Variadic<Broadcast>;
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        use MathFunction::*;
        let parameters: &[P] = match self.0 {
            Pi | Rand => &[],
            Log => &[P::constant(0.0), P::constant(10.0)],
            Trunc => &[P::constant(0.0), P::constant(0.0)],
            CeilingMath | FloorMath => &[P::constant(0.0), P::constant(1.0), P::constant(0.0)],
            CeilingPrecise | FloorPrecise | IsoCeiling => &[P::constant(0.0), P::constant(1.0)],
            Atan2 | Quotient | Combin | Combina | Mround | Power | Mod | Round | RoundUp
            | RoundDown | Ceiling | Floor | Randbetween => &[P::constant(0.0), P::constant(0.0)],
            _ => &[P::constant(0.0)],
        };
        let values = arguments.numbers(context, parameters)?;
        let value = match self.0 {
            Rand => Number(context.host.random_draw(None)?),
            Randbetween => {
                let bottom = values.get(0)?;
                let top = values.get(1)?;
                if bottom > top {
                    return Err(Num.into());
                }
                let bottom = bottom.ceil();
                let top = top.floor().max(bottom);
                let interval =
                    crate::eval::random::Interval::new(bottom, top).ok_or_else(|| {
                        context.host.unsupported(
                            CapabilityId::FunctionAdapter,
                            "random integer interval exceeds the calculation stream's exact domain",
                        )
                    })?;
                Number(context.host.random_draw(Some(interval))?)
            }
            Combina => Number(poirot_functions::math::combina_metered(
                values.get(0)?,
                values.get(1)?,
                &mut || context.host.charge_work(1).map_err(FunctionFailure::from),
            )?),
            function => {
                match function {
                    Fact => context.host.charge_work(170)?,
                    Factdouble => {
                        context.host.check_values(301)?;
                        context.host.charge_work(301)?;
                    }
                    Combin => context.host.charge_work(341)?,
                    Mround | CeilingMath | FloorMath | CeilingPrecise | FloorPrecise => {
                        context.host.check_values(2048)?;
                        context.host.charge_work(2048)?;
                    }
                    _ => {}
                }
                super::execution::scalar(function, values.as_slice())?
            }
        };
        Ok(value)
    }
}

/// GCD, LCM, or MULTINOMIAL kernel over ordered source collections.
/// It applies origin-sensitive numeric admission before invoking or incrementally feeding the independent calculation.
struct Reduction(MathFunction);
impl CollectionKernel for Reduction {
    type Signature = Variadic<WholeCollection>;
    fn apply(
        &self,
        host: &mut CallHost<'_, '_>,
        arguments: &Self::Signature,
        _address: CellAddress,
    ) -> FunctionResult<Value> {
        use poirot_functions::math::modern::Multinomial;
        // Materializing the admitted numbers preserves the independent GCD/LCM
        // algorithms' single-value and zero semantics without duplicating them.
        if self.0 != MathFunction::Multinomial {
            let values = collections::fold(
                host,
                arguments.iter(),
                Vec::new(),
                |host, values, member| {
                    if member.origin != OperandOrigin::Direct
                        && matches!(
                            member.value,
                            Blank | Boolean(_) | Text(_) | CellValue::TextUtf16(_)
                        )
                    {
                        return Ok(());
                    }
                    let number =
                        host.coercev2_scalar_number(&member.value, NumericPolicy::PinnedPoi)?;
                    if number < 0.0 || (self.0 == MathFunction::Gcd && number >= 9007199254740992.0)
                    {
                        return Err(Num.into());
                    }
                    host.check_values(values.len().saturating_add(1))?;
                    values
                        .try_reserve(1)
                        .map_err(|_| host.allocation_failure())?;
                    // SAFETY: one extra slot was reserved immediately above.
                    values.push(number);
                    Ok(())
                },
            )?;
            host.charge_work((values.len() as u64).saturating_mul(94))?;
            let number = if values.is_empty() {
                0.0
            } else if self.0 == MathFunction::Gcd {
                poirot_functions::math::gcd(&values)?
            } else {
                poirot_functions::math::lcm(&values)?
            };
            // Microsoft LCM documents an exclusive 2^53 output ceiling,
            // including a singleton (the independent legacy kernel admits it).
            // https://support.microsoft.com/en-us/excel/functions/lcm-function
            if self.0 == MathFunction::Lcm && number >= 9007199254740992.0 {
                return Err(Num.into());
            }
            return Ok(Number(number).into());
        }
        let result = collections::fold(
            host,
            arguments.iter(),
            Multinomial::new(),
            |host, result, member| {
                if member.origin != OperandOrigin::Direct
                    && matches!(
                        member.value,
                        Blank | Boolean(_) | Text(_) | CellValue::TextUtf16(_)
                    )
                {
                    return Ok(());
                }
                let number =
                    host.coercev2_scalar_number(&member.value, NumericPolicy::PinnedPoi)?;
                result.push(number, || {
                    host.charge_work(1).map_err(FunctionFailure::from)
                })
            },
        )?;
        Ok(Number(result.finish()).into())
    }
}

/// Execute mathematical functions after arity checking, using shared typed roles.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: MathFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !FunctionId::Math(function)
        .descriptor()
        .arity
        .accepts(args.len())
    {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    let result = match function {
        MathFunction::Gcd | MathFunction::Lcm | MathFunction::Multinomial => {
            collections::execute(&mut host, address, Reduction(function))
        }
        MathFunction::Base | MathFunction::Decimal => {
            elementwise::execute(&mut host, address, super::radix::Radix(function))
        }
        MathFunction::SeriesSum => elementwise::execute(&mut host, address, super::series::Series),
        _ => elementwise::execute(&mut host, address, Numeric(function)),
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
