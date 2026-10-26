//! SERIESSUM with broadcast numeric roles and a positional coefficient fold.
use crate::adapters::elementwise::{
    ArgumentRole, Broadcast, Kernel, MemberContext, WholeCollection, collections,
};
use crate::host::{CallHost, NumericPolicy, OperandKind, ScalarOperand};
use crate::*;
use poirot_coercionv2::{
    compatibility::ScalarRef,
    series::{coercev2_seriessum_coefficient, coercev2_seriessum_number},
};

/// Convert a SERIESSUM scalar without admitting logicals or explicit omissions.
fn number(host: &mut CallHost<'_, '_>, scalar: ScalarOperand) -> FunctionResult<f64> {
    let value = if scalar.missing {
        ScalarRef::Missing
    } else {
        (&scalar.value).into()
    };
    coercev2_seriessum_number(value, |text| {
        let text = host.project_text(text)?;
        host.coercev2_number_text(&text, NumericPolicy::PinnedPoi)
    })
}
/// Evaluate each power-series output in source and coefficient order.
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
        let description = context.host.describe(coefficients.argument())?;
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
                    coercev2_seriessum_coefficient(value)?
                };
                let term =
                    poirot_functions_math::series::term(x, initial, step, *index, coefficient)?;
                *total = poirot_functions_math::series::accumulate(*total, term)?;
                *index = index.checked_add(1).ok_or_else(|| host.size_overflow())?;
                Ok(())
            },
        )?;
        Ok(Number(total))
    }
}
