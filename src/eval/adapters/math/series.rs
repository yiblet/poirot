//! SERIESSUM with broadcast numeric roles and a positional coefficient fold.
use crate::eval::adapters::elementwise::{
    ArgumentRole, Broadcast, Kernel, MemberContext, WholeCollection, collections,
};
use crate::eval::host::{ArgumentCoercion, CallHost, OperandKind, OperandReader, ScalarOperand};
use crate::eval::values::FunctionResult;
use crate::model::CellValue;
use crate::model::CellValue::Number;
use poirot_functions::coercion::{
    compatibility::ScalarRef,
    series::{coerce_seriessum_coefficient, coerce_seriessum_number},
};

/// Convert a SERIESSUM scalar without admitting logicals or explicit omissions.
fn number(host: &mut CallHost<'_, '_>, scalar: ScalarOperand) -> FunctionResult<f64> {
    let value = if scalar.missing {
        ScalarRef::Missing
    } else {
        (&scalar.value).into()
    };
    coerce_seriessum_number(value, |text| {
        let text = ArgumentCoercion::new(host).project_text(text)?;
        ArgumentCoercion::new(host).coerce_number_text(&text)
    })
}

/// SERIESSUM kernel that broadcasts numeric controls over one coefficient collection.
/// Each output reads coefficients in source order and preserves their scalar-versus-reference coercion rules.
pub(super) struct Series;

impl Kernel for Series {
    type Signature = (Broadcast, Broadcast, Broadcast, WholeCollection);

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        (x, initial, step, coefficients): &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let x = x.read(context)?;
        let x = number(context.host, x)?;
        let initial = initial.read(context)?;
        let initial = number(context.host, initial)?;
        let step = step.read(context)?;
        let step = number(context.host, step)?;
        let description = OperandReader::new(context.host).describe(coefficients.argument())?;
        let scalar = matches!(description.kind, OperandKind::Scalar | OperandKind::Missing)
            || (description.kind == OperandKind::Reference
                && description.rows == 1
                && description.columns == 1
                && description.sheets == 1);
        let (total, _) = collections::fold(
            context.host,
            [WholeCollection::bind(coefficients.argument(), 0, true)],
            (0.0, 0usize),
            |host, (total, index), member| {
                let coefficient = if scalar {
                    number(host, member)?
                } else {
                    let value = if member.missing {
                        ScalarRef::Missing
                    } else {
                        (&member.value).into()
                    };
                    coerce_seriessum_coefficient(value)?
                };
                let term =
                    poirot_functions::math::series::term(x, initial, step, *index, coefficient)?;
                *total = poirot_functions::math::series::accumulate(*total, term)?;
                *index = index.checked_add(1).ok_or_else(|| host.size_overflow())?;
                Ok(())
            },
        )?;
        Ok(Number(total))
    }
}
