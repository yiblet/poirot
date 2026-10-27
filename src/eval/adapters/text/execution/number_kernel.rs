use crate::eval::adapters::elementwise::{Broadcast, Kernel, MemberContext, Optional};
use crate::eval::host::{ArgumentCoercion, ContextReads};
use crate::eval::limits::CapabilityId;
use crate::eval::values::{FunctionFailure, FunctionResult, VALUE_ERROR};
use crate::model::CellError::Num;
use crate::model::CellValue;
use crate::model::CellValue::Number;

/// NUMBERVALUE kernel that parses each broadcast text member using explicit or locale-default separators.
/// Omitted separator options remain distinguishable until conversion chooses their defaults.
pub(super) struct NumberKernel;

impl Kernel for NumberKernel {
    type Signature = (Broadcast, Optional<Broadcast>, Optional<Broadcast>);

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        (source, decimal, grouping): &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let source = source.read(context)?;
        let decimal = decimal
            .0
            .as_ref()
            .map(|role| role.read(context))
            .transpose()?;
        let grouping = grouping
            .0
            .as_ref()
            .map(|role| role.read(context))
            .transpose()?;
        let host = &mut *context.host;
        let text = ArgumentCoercion::new(host).coerce_scalar_text(&source.value)?;
        let decimal = decimal
            .as_ref()
            .filter(|operand| !operand.missing)
            .map(|operand| ArgumentCoercion::new(host).coerce_scalar_text(&operand.value))
            .transpose()?;
        let grouping = grouping
            .as_ref()
            .filter(|operand| !operand.missing)
            .map(|operand| ArgumentCoercion::new(host).coerce_scalar_text(&operand.value))
            .transpose()?;
        if decimal.is_none() || grouping.is_none() {
            ContextReads::new(host).read_number_locale();
        }
        let bound = text
            .len()
            .checked_add(1)
            .ok_or_else(|| host.size_overflow())?;
        host.check_text(bound)?;
        host.check_values(bound.saturating_mul(3))?;
        host.charge_work(bound.saturating_mul(8) as u64)?;
        let number = poirot_functions::text::numeric::coerce_number_value_excel(
            &text,
            ContextReads::new(host).number_locale_snapshot(),
            decimal.as_deref(),
            grouping.as_deref(),
        )
        .map_err(|error| match error {
            poirot_functions::text::numeric::ParseFailure::Spreadsheet(
                poirot_functions::text::Error::Value,
            ) => FunctionFailure::Spreadsheet(VALUE_ERROR),
            poirot_functions::text::numeric::ParseFailure::Spreadsheet(
                poirot_functions::text::Error::Num,
            ) => FunctionFailure::Spreadsheet(Num),
            poirot_functions::text::numeric::ParseFailure::Source(reason) => host.unsupported(
                CapabilityId::FunctionAdapter,
                format!("NUMBERVALUE separator representation: {reason:?}"),
            ),
        })?;
        Ok(Number(number))
    }
}
