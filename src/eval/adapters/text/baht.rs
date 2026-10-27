use crate::eval::adapters::elementwise::{self, Broadcast, Kernel, MemberContext, Variadic};
use crate::eval::functions::registry::{FunctionId, TextFunction};
use crate::eval::host::CallHost;
use crate::eval::limits::{CapabilityId, EvalResult};
use crate::eval::values::{FunctionResult, VALUE_ERROR, Value, err};
use crate::model::{CellAddress, CellError, CellValue};

/// Convert each broadcast number to Thai currency text within the text budget.
struct BahtKernel;

impl Kernel for BahtKernel {
    type Signature = Variadic<Broadcast>;

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let number = arguments.number(context, 0, None)?;
        if !number.is_finite() {
            return Err(CellError::Num.into());
        }
        // At and above 2^53, binary64 no longer represents every integer baht.
        // Refuse this domain because its Excel spelling has not been verified.
        if number.abs() >= 9_007_199_254_740_992.0 {
            return Err(context.host.unsupported(
                CapabilityId::FunctionAdapter,
                crate::eval::limits::EvaluationReasonError::BahtMagnitudeOutsideVerifiedDomain,
            ));
        }
        context.host.charge_work(128)?;
        // Use the existing decimal-view rounding contract: nearest satang,
        // ties away from zero. Excel BAHTTEXT tie parity remains unverified.
        let rounded = poirot_functions::math::round(number, 2.0)?;
        let magnitude = rounded.abs();
        let baht = magnitude.trunc() as u64;
        let satang = (magnitude.fract() * 100.0).round() as u8;
        let words = poirot_functions::text::baht::words(baht, satang, rounded < 0.0)
            .ok_or(CellError::Num)?;
        let bytes = words
            .clone()
            .try_fold(0usize, |bytes, word| bytes.checked_add(word.len()))
            .ok_or_else(|| context.host.size_overflow())?;
        context.host.check_text(bytes)?;
        let mut output = String::new();
        output
            .try_reserve_exact(bytes)
            .map_err(|source| context.host.reservation_failure(source))?;
        for word in words {
            // SAFETY: bytes sums all emitted word lengths and was reserved locally.
            output.push_str(word);
        }
        Ok(CellValue::Text(output))
    }
}

/// Execute Thai currency conversion under shared elementwise evaluation.
pub(in crate::eval) fn execute(
    mut host: CallHost<'_, '_>,
    at: CellAddress,
    length: usize,
) -> EvalResult<Value> {
    if !FunctionId::Text(TextFunction::BahtText)
        .descriptor()
        .arity
        .accepts(length)
    {
        return Ok(err(VALUE_ERROR));
    }
    super::outcome::finish(elementwise::execute(&mut host, at, BahtKernel))
}
