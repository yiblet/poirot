use super::joined::Joined;
use crate::eval::adapters::elementwise::{
    self, FixedScalar, Variadic, WholeCollection, collections,
};
use crate::eval::host::CallHost;
use crate::eval::values::{FunctionResult, VALUE_ERROR, Value};
use crate::model::CellAddress;

/// TEXTJOIN kernel that combines complete collections with repeating delimiters.
/// Its ignore-empty option is selected as a fixed scalar rather than contributing output geometry.
pub(super) struct JoinKernel;

impl collections::CollectionKernel for JoinKernel {
    type Signature = Variadic<WholeCollection>;

    fn apply(
        &self,
        host: &mut CallHost<'_, '_>,
        arguments: &Self::Signature,
        _address: CellAddress,
    ) -> FunctionResult<Value> {
        if !(3..=254).contains(&arguments.len()) {
            return Err(VALUE_ERROR.into());
        }
        // Microsoft TEXTJOIN example 3 uses a repeating delimiter range.
        // https://support.microsoft.com/en-gb/excel/functions/textjoin-function
        let delimiters = elementwise::text::collection_units(host, arguments.iter().take(1))?;
        let ignore = FixedScalar::at(1).read(host)?;
        let ignore = crate::eval::values::core_projection::coerce_operand_boolean(&ignore.value)?;
        // Absent cells are empty text: skipped when ignoring empties, otherwise
        // they still advance the delimiter cycle and contribute separators.
        let joined = collections::fold_stored(
            host,
            arguments.iter().skip(2),
            Joined::default(),
            |host, joined, blanks| {
                if ignore {
                    return Ok(());
                }
                if delimiters.iter().all(Vec::is_empty) {
                    joined.count = joined
                        .count
                        .checked_add(blanks)
                        .ok_or_else(|| host.size_overflow())?;
                    return Ok(());
                }
                // Each nonempty separator grows the output, so the length limit
                // ends this loop within 32767 nonempty separators.
                for _ in 0..blanks {
                    joined.push(host, &delimiters, &[])?;
                }
                Ok(())
            },
            |host, joined, operand| {
                let text = elementwise::text::units(host, &operand)?;
                if ignore && text.is_empty() {
                    return Ok(());
                }
                joined.push(host, &delimiters, &text)
            },
        )?;
        Ok(elementwise::text::value(joined.output).into())
    }
}
