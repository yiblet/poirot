//! Checked decoding of operand bytes written by the bytecode `Bytes` encoder.

use crate::eval::limits::{EvalResult, EvaluationFailure};

/// Cursor decoding checked reference words and UTF-8 strings.
pub(in crate::eval) struct OperandCursor<'a>(pub(in crate::eval) &'a [u8]);

impl<'a> OperandCursor<'a> {
    /// Read one little-endian word.
    pub(in crate::eval) fn word(&mut self) -> EvalResult<u32> {
        let (value, rest) = self
            .0
            .split_at_checked(4)
            .ok_or(EvaluationFailure::StalePlan)?;
        self.0 = rest;
        let bytes = value.try_into().map_err(|_| EvaluationFailure::StalePlan)?;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Read a length-prefixed UTF-8 string.
    pub(in crate::eval) fn string(&mut self) -> EvalResult<&'a str> {
        let count = self.word()? as usize;
        let (text, rest) = self
            .0
            .split_at_checked(count)
            .ok_or(EvaluationFailure::StalePlan)?;
        self.0 = rest;
        std::str::from_utf8(text).map_err(|_| EvaluationFailure::StalePlan)
    }

    /// Read a presence flag followed by an optional string.
    pub(in crate::eval) fn optional(&mut self) -> EvalResult<Option<&'a str>> {
        if self.word()? != 0 {
            self.string().map(Some)
        } else {
            Ok(None)
        }
    }

    /// Read an unqualified cell endpoint.
    pub(in crate::eval) fn cell(&mut self) -> EvalResult<poirot_parsing::formula::CellReference> {
        let row = self.word()?;
        let column = self.word()?;
        let row_absolute = self.word()? != 0;
        let column_absolute = self.word()? != 0;
        if self.optional()?.is_some() {
            return Err(EvaluationFailure::StalePlan);
        }
        Ok(poirot_parsing::formula::CellReference {
            row,
            column,
            row_absolute,
            column_absolute,
            sheet: None,
        })
    }
}
