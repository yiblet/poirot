use super::call_host::CallHost;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::inputs::EffectiveWorkbook;
use crate::eval::ports::DependencyRead;
use crate::eval::session::ReadRecorder;
use crate::eval::values::{FunctionResult, Value};
use crate::model::{CellAddress, CellError, CellValue};

/// Reads authored formula identity and worksheet counts without executing cell values.
pub(in crate::eval) struct FormulaMetadata<'h, 'call, 'workbook> {
    host: &'h mut CallHost<'call, 'workbook>,
}

impl<'h, 'call, 'workbook> FormulaMetadata<'h, 'call, 'workbook> {
    /// Borrow the host that reads current formula sources and records dependencies.
    pub(in crate::eval) fn new(host: &'h mut CallHost<'call, 'workbook>) -> Self {
        Self { host }
    }

    /// Test for an authored formula without evaluating it, even if its result would fail.
    pub(in crate::eval) fn is_formula(
        &mut self,
        sheet: usize,
        address: CellAddress,
    ) -> FunctionResult<CellValue> {
        self.record(sheet, address)?;
        Ok(CellValue::Boolean(self.source(sheet, address).is_some()))
    }

    /// Return authored formula text, adding a missing equals sign and enforcing
    /// the 8192 UTF-16-unit limit including that sign.
    pub(in crate::eval) fn text(
        &mut self,
        sheet: usize,
        address: CellAddress,
    ) -> FunctionResult<CellValue> {
        self.record(sheet, address)?;
        let source = self.source(sheet, address).ok_or(CellError::NA)?;
        let prefix = usize::from(!source.starts_with('='));
        let bytes = source
            .len()
            .checked_add(prefix)
            .ok_or_else(|| self.host.size_overflow())?;
        // Charge before scanning or copying potentially large authored source.
        self.host.check_text(bytes)?;
        let source = self.source(sheet, address).ok_or(CellError::NA)?;
        if source
            .encode_utf16()
            .take(8193)
            .count()
            .saturating_add(prefix)
            > 8192
        {
            return Err(CellError::NA.into());
        }
        let mut text = String::new();
        text.try_reserve_exact(bytes)
            .map_err(|_| self.host.allocation_failure())?;
        if prefix != 0 {
            // SAFETY: bytes reserves source.len() plus this one-byte prefix above.
            text.push('=');
        }
        // SAFETY: bytes reserves the complete source and optional prefix above.
        text.push_str(source);
        Ok(CellValue::Text(text))
    }

    /// Count represented worksheets; chart, macro and dialog sheets are not modeled.
    pub(in crate::eval) fn sheets(&mut self, value: Option<&Value>) -> FunctionResult<CellValue> {
        ReadRecorder::new(self.host.session).read_lazy(|| DependencyRead::SheetLayout);
        let count = match value {
            None => self.host.session.sources.workbook.sheets().len(),
            Some(Value::SheetSpan(areas)) if !areas.is_empty() => areas.len(),
            Some(Value::Refs(areas)) if !areas.is_empty() => {
                self.host
                    .charge_work((areas.len() as u64).saturating_mul(areas.len() as u64))?;
                areas
                    .iter()
                    .enumerate()
                    .filter(|(index, area)| {
                        // SAFETY: enumerate indices are within areas, hence this prefix is valid.
                        !areas[..*index]
                            .iter()
                            .any(|previous| previous.sheet == area.sheet)
                    })
                    .count()
            }
            Some(Value::SignedRefs(areas)) if areas.len() == 1 => 1,
            Some(
                Value::Scalar(CellValue::Error(error))
                | Value::SelectedScalar(CellValue::Error(error), _),
            ) => return Err((*error).into()),
            _ => return Err(CellError::Ref.into()),
        };
        Ok(CellValue::Number(count as f64))
    }

    /// Record a source-sensitive dependency even when the cell has no authored formula.
    fn record(&mut self, sheet: usize, address: CellAddress) -> FunctionResult<()> {
        address.validate().map_err(|_| CellError::Ref)?;
        if self.host.session.sources.workbook.sheet(sheet).is_none() {
            return Err(CellError::Ref.into());
        }
        self.host.charge_work(1)?;
        ReadRecorder::new(self.host.session).read(DependencyRead::FormulaMetadata(
            EvaluationTarget { sheet, address },
        ));
        Ok(())
    }

    /// Borrow effective authored source, excluding generated spill results.
    fn source(&self, sheet: usize, address: CellAddress) -> Option<&str> {
        EffectiveWorkbook::new(
            self.host.session.sources.workbook,
            self.host.session.sources.inputs,
        )
        .cell(sheet, address)?
        .formula
        .as_deref()
    }
}
