//! POI concatenation operator preserves all code units.
use super::*;
impl Session<'_> {
    /// Render a scalar into admitted UTF-16 units without replacing surrogate values.
    pub(super) fn text_unit_render(&mut self, value: &CellValue) -> FunctionResult<Vec<u16>> {
        if let Some(text) = value.text_ref() {
            let units = text.len_units();
            let bytes = units
                .checked_mul(2)
                .ok_or_else(|| text_unsupported(poirot_text::Unsupported::SizeOverflow))?;
            self.budget.text(bytes)?;
            self.budget.allocation(units)?;
            self.budget.charge(WorkKind::TextByte, bytes as u64)?;
            return Ok(text.units().collect());
        }
        // NumberToTextConverter has a bounded ASCII representation.
        self.budget.text(512)?;
        self.budget.allocation(512)?;
        self.budget.charge(WorkKind::TextByte, 512)?;
        let rendered = match value {
            Number(value) => poirot_numeric::to_text(*value),
            _ => text(value, &mut self.budget)?,
        };
        Ok(rendered.encode_utf16().collect())
    }
    /// Apply text operators directly to UTF-16 units under the selected source contract.
    pub(super) fn text_unit_operator(
        &mut self,
        left: &CellValue,
        right: &CellValue,
    ) -> FunctionResult<CellValue> {
        let left = self.text_unit_render(left)?;
        let right = self.text_unit_render(right)?;
        let units = left
            .len()
            .checked_add(right.len())
            .ok_or_else(|| text_unsupported(poirot_text::Unsupported::SizeOverflow))?;
        let bytes = units
            .checked_mul(2)
            .ok_or_else(|| text_unsupported(poirot_text::Unsupported::SizeOverflow))?;
        self.budget.text(bytes)?;
        self.budget.allocation(units)?;
        self.budget.charge(WorkKind::TextByte, bytes as u64)?;
        let mut output = Vec::with_capacity(units);
        output.extend_from_slice(&left);
        output.extend_from_slice(&right);
        Ok(CellValue::TextUtf16(poirot_model::ExcelText::from_units(
            output,
        )))
    }
}
