//! POI concatenation operator preserves all code units.
use super::*;
impl Session<'_> {
    /// Render a scalar into admitted UTF-16 units without replacing surrogate values.
    pub(super) fn coercev2_concat_utf16(&mut self, value: &CellValue) -> FunctionResult<Vec<u16>> {
        if let Some(text) = value.text_ref() {
            let units = text.len_units();
            let bytes = units
                .checked_mul(2)
                .ok_or_else(|| text_unsupported(poirot_text::Unsupported::SizeOverflow))?;
            self.budget.text(bytes)?;
            self.budget.allocation(units)?;
            return Ok(text.units().collect());
        }
        let rendered = coercev2_formula_text(value, &mut self.budget)?;
        let units = rendered.encode_utf16().count();
        self.budget.check_text(units.saturating_mul(2))?;
        self.budget.allocation(units)?;
        Ok(rendered.encode_utf16().collect())
    }
    /// Apply text operators directly to UTF-16 units under the selected source contract.
    pub(super) fn text_unit_operator(
        &mut self,
        left: &CellValue,
        right: &CellValue,
    ) -> FunctionResult<CellValue> {
        let left = self.coercev2_concat_utf16(left)?;
        let right = self.coercev2_concat_utf16(right)?;
        let units = left
            .len()
            .checked_add(right.len())
            .ok_or_else(|| text_unsupported(poirot_text::Unsupported::SizeOverflow))?;
        let bytes = units
            .checked_mul(2)
            .ok_or_else(|| text_unsupported(poirot_text::Unsupported::SizeOverflow))?;
        self.budget.text(bytes)?;
        self.budget.allocation(units)?;
        let mut output = Vec::with_capacity(units);
        output.extend_from_slice(&left);
        output.extend_from_slice(&right);
        Ok(CellValue::TextUtf16(poirot_model::ExcelText::from_units(
            output,
        )))
    }
}
