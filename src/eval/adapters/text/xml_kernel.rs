use super::xml_selection_host::XmlSelectionHost;
use crate::eval::adapters::elementwise::{Broadcast, MemberContext, Variadic, text as shared};
use crate::eval::host::{ArgumentCoercion, CallHost};
use crate::eval::limits::EvalResult;
use crate::eval::values::{FunctionError, FunctionResult, Value, err};
use crate::model::{CellAddress, CellError, CellValue};

/// Expand local XPath matches into a vertical dynamic array.
struct XmlKernel;

impl shared::ExpansionKernel for XmlKernel {
    type Signature = Variadic<Broadcast>;

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<shared::Expansion> {
        let xml = arguments.operand(context, 0)?;
        let xpath = arguments.operand(context, 1)?;
        let xml = ArgumentCoercion::new(context.host).coerce_scalar_text(&xml.value)?;
        let xpath = ArgumentCoercion::new(context.host).coerce_scalar_text(&xpath.value)?;
        let mut executor = XmlSelectionHost(context.host);
        let values = poirot_functions::text::xml::select(&xml, &xpath, &mut executor)
            .map_err(|error| executor.failure(error))?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(values.len())
            .map_err(|source| context.host.reservation_failure(source))?;
        for text in values {
            // Microsoft documents locale parsing of returned XML values. Use
            // the captured numeric/date-time conversion and track its dependencies;
            // nonnumeric text remains text, operational refusals propagate.
            let value = if text.is_empty() {
                CellValue::Text(text)
            } else {
                match ArgumentCoercion::new(context.host).coerce_number_text(&text) {
                    Ok(number) => CellValue::Number(number),
                    Err(FunctionError::Spreadsheet(CellError::Value)) => CellValue::Text(text),
                    Err(error) => return Err(error),
                }
            };
            // SAFETY: output reserved one slot per selected string locally.
            output.push(value);
        }
        Ok(shared::Expansion {
            rows: output.len(),
            columns: 1,
            values: output,
        })
    }
}

/// Evaluate FILTERXML with ordinary broadcast arguments and vertical spill output.
pub(in crate::eval) fn execute(
    mut host: CallHost<'_, '_>,
    address: CellAddress,
    length: usize,
) -> EvalResult<Value> {
    if length != 2 {
        return Ok(err(CellError::Value));
    }
    super::outcome::finish(shared::expand(&mut host, address, XmlKernel))
}
