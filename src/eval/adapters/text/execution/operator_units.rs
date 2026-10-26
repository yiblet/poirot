//! POI concatenation operator preserves all code units.
use super::*;
/// Render a scalar into admitted UTF-16 units without replacing surrogate values.
fn coercev2_concat_utf16(
    host: &mut CallHost<'_, '_>,
    value: &CellValue,
) -> FunctionResult<Vec<u16>> {
    if let Some(text) = value.text_ref() {
        let units = text.len_units();
        let bytes = units.checked_mul(2).ok_or_else(|| {
            text_unsupported(poirot_functions::text_units::Unsupported::SizeOverflow)
        })?;
        host.check_text(bytes)?;
        host.check_values(units)?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(units)
            .map_err(|_| host.allocation_failure())?;
        // SAFETY: units is the exact unit count reserved locally.
        output.extend(text.units());
        return Ok(output);
    }
    let rendered = host.coercev2_scalar_text(value)?;
    let units = rendered.encode_utf16().count();
    host.check_text_capacity(units.saturating_mul(2))?;
    host.check_values(units)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(units)
        .map_err(|_| host.allocation_failure())?;
    // SAFETY: units counts exactly the same UTF-16 iterator, reserved locally.
    output.extend(rendered.encode_utf16());
    Ok(output)
}
/// Apply text operators directly to UTF-16 units under the selected source contract.
pub(super) fn text_unit_operator(
    host: &mut CallHost<'_, '_>,
    left: &CellValue,
    right: &CellValue,
) -> FunctionResult<CellValue> {
    let left = coercev2_concat_utf16(host, left)?;
    let right = coercev2_concat_utf16(host, right)?;
    let units = left
        .len()
        .checked_add(right.len())
        .ok_or_else(|| text_unsupported(poirot_functions::text_units::Unsupported::SizeOverflow))?;
    let bytes = units
        .checked_mul(2)
        .ok_or_else(|| text_unsupported(poirot_functions::text_units::Unsupported::SizeOverflow))?;
    host.check_text(bytes)?;
    host.check_values(units)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(units)
        .map_err(|_| host.allocation_failure())?;
    // SAFETY: units is the locally checked sum of both appended lengths.
    output.extend_from_slice(&left);
    // SAFETY: units reserves the checked sum of both appended lengths.
    output.extend_from_slice(&right);
    Ok(CellValue::TextUtf16(crate::model::ExcelText::from_units(
        output,
    )))
}
