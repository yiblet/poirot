//! Per-member policies; traversal, shape validation, and allocation stay with hosts.
use crate::coercion::compatibility::{DeferredNumber, NumberPolicy, ScalarRef, coercev2_number};
use crate::coercion::{CellError, Origin};
/// Source matrix area admission policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatrixCollection {
    /// Require a square numeric matrix; invalid geometry or members precede embedded errors.
    SquareNumeric,
    /// Reject blanks, ignore text and booleans.
    Determinant,
    /// Ignore nonnumeric cells, compact then pad the original rectangle.
    Inverse,
    /// Numeric members for matrix multiplication.
    Multiply,
    /// Treat blanks as zero, ignore text and booleans.
    Transpose,
}
/// Admit one matrix area member without reading any other member.
pub fn coercev2_matrix_member(
    value: ScalarRef<'_>,
    policy: MatrixCollection,
) -> Result<Option<f64>, CellError> {
    match value {
        ScalarRef::Number(n) => Ok(Some(n)),
        ScalarRef::Error(e) => Err(e),
        _ if policy == MatrixCollection::SquareNumeric => Err(CellError::Value),
        ScalarRef::Blank if policy == MatrixCollection::Determinant => Err(CellError::Value),
        ScalarRef::Blank if policy == MatrixCollection::Transpose => Ok(Some(0.0)),
        _ => Ok(None),
    }
}
/// Admission state for a complete row-major matrix. Strict admission scans
/// every member before rejecting nonnumbers or the first embedded error; pinned policies retain
/// their original immediate failures and compaction rules. Hosts own allocation.
pub struct MatrixAdmission {
    policy: MatrixCollection,
    invalid: bool,
    error: Option<CellError>,
    numbers: usize,
}
impl MatrixAdmission {
    /// Start admission without allocating matrix storage.
    pub fn new(policy: MatrixCollection, rows: usize, cols: usize) -> Self {
        Self {
            policy,
            invalid: policy == MatrixCollection::SquareNumeric
                && (rows == 0 || cols == 0 || rows != cols),
            error: None,
            numbers: 0,
        }
    }
    /// Admit one member, retaining the first embedded error in strict mode.
    pub fn coercev2_matrix_member(
        &mut self,
        value: ScalarRef<'_>,
    ) -> Result<Option<f64>, CellError> {
        if self.policy == MatrixCollection::SquareNumeric {
            match value {
                ScalarRef::Number(n) => return Ok(Some(n)),
                ScalarRef::Error(e) => {
                    self.error.get_or_insert(e);
                }
                _ => self.invalid = true,
            }
            return Ok(None);
        }
        let number = coercev2_matrix_member(value, self.policy)?;
        if number.is_some() {
            self.numbers = self.numbers.saturating_add(1);
        }
        Ok(number)
    }
    /// Reject incomplete strict matrices before their numeric storage is used.
    pub fn coercev2_finish(self) -> Result<(), CellError> {
        if self.invalid {
            return Err(CellError::Value);
        }
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.policy != MatrixCollection::SquareNumeric
            && (self.numbers == 0
                || (self.numbers == 1 && self.policy != MatrixCollection::Transpose))
        {
            return Err(CellError::Value);
        }
        Ok(())
    }
}
/// TREND accepts numeric and logical members; all other kinds yield VALUE.
pub fn coercev2_trend_member(value: ScalarRef<'_>) -> Result<f64, CellError> {
    match value {
        ScalarRef::Number(n) => Ok(n),
        ScalarRef::Boolean(b) => Ok(f64::from(u8::from(b))),
        _ => Err(CellError::Value),
    }
}
/// SUMPRODUCT requests either strict scalar or zero-admitting text/blank handling.
/// The caller selects admission; singleton geometry does not determine it.
/// Boolean-to-number admission is retained in both modes.
pub fn coercev2_sumproduct_member(value: ScalarRef<'_>, scalar: bool) -> Result<f64, CellError> {
    match value {
        ScalarRef::Number(n) => Ok(n),
        ScalarRef::Boolean(b) => Ok(f64::from(u8::from(b))),
        ScalarRef::Error(e) => Err(e),
        ScalarRef::Text(_) | ScalarRef::Blank if !scalar => Ok(0.0),
        _ => Err(CellError::Value),
    }
}
/// Financial cash flows admit direct text/logical inputs and ignore referenced ones.
pub fn coercev2_cashflow_member<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    origin: Origin,
    parse: impl FnMut(DeferredNumber<'a>) -> Result<f64, E>,
) -> Result<Option<f64>, E> {
    match value {
        ScalarRef::Number(_) | ScalarRef::Error(_) => {
            coercev2_number(value, NumberPolicy::Finite, parse).map(Some)
        }
        ScalarRef::Boolean(_) | ScalarRef::Text(_) if origin == Origin::Direct => {
            coercev2_number(value, NumberPolicy::Finite, parse).map(Some)
        }
        _ => Ok(None),
    }
}
/// FREQUENCY applies the same origin filtering while preserving nonfinite numbers.
pub fn coercev2_frequency_member<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    origin: Origin,
    parse: impl FnMut(DeferredNumber<'a>) -> Result<f64, E>,
) -> Result<Option<f64>, E> {
    match value {
        ScalarRef::Number(_) | ScalarRef::Error(_) => {
            coercev2_number(value, NumberPolicy::Preserve, parse).map(Some)
        }
        ScalarRef::Boolean(_) | ScalarRef::Text(_) if origin == Origin::Direct => {
            coercev2_number(value, NumberPolicy::Preserve, parse).map(Some)
        }
        _ => Ok(None),
    }
}

/// Numeric aggregates count a directly computed blank as a numeric observation;
/// blanks selected from references or arrays remain blank and can be ignored.
/// Syntactically omitted arguments must retain omission metadata at the host.
pub fn coercev2_aggregate_argument(
    value: crate::coercion::CellValue,
    origin: Origin,
) -> crate::coercion::CellValue {
    if origin == Origin::Direct && matches!(value, crate::coercion::CellValue::Blank) {
        crate::coercion::CellValue::Number(0.)
    } else {
        value
    }
}

/// Business-day consumer's explicit holiday member contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HolidayPolicy {
    /// Preserve the historical date-argument conversion for every member.
    PinnedPoi,
    /// NETWORKDAYS ignores text held in worksheet holiday ranges.
    NetworkDays,
    /// WORKDAY requests date conversion for text members from every origin.
    WorkDay,
}
/// Admit a holiday member, retaining provenance until the consuming function
/// requests date conversion. Bounds and date-text context belong to the host.
pub fn coercev2_holiday_member<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    origin: Origin,
    policy: HolidayPolicy,
    date: impl FnOnce() -> Result<f64, E>,
) -> Result<Option<f64>, E> {
    if policy != HolidayPolicy::PinnedPoi {
        match value {
            ScalarRef::Blank => return Ok(None),
            ScalarRef::Boolean(_) if policy == HolidayPolicy::NetworkDays => return Ok(None),
            ScalarRef::Text(_)
                if policy == HolidayPolicy::NetworkDays && origin == Origin::Reference =>
            {
                return Ok(None);
            }
            ScalarRef::Error(error) => return Err(error.into()),
            _ => (),
        }
    }
    date().map(Some)
}

/// Consumer-specific validity of seven-character weekend masks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeekendConsumer {
    /// WORKDAY cannot advance when every weekday is excluded.
    WorkDay,
    /// NETWORKDAYS permits an empty working week and returns zero.
    NetworkDays,
}
/// Modern seven-character weekend masks start with Monday; numeric weekend
/// codes remain handled by weekend_policy. WORKDAY forbids an all-weekend mask.
pub fn coercev2_weekend_mask(
    text: &str,
    consumer: WeekendConsumer,
) -> Result<[bool; 7], CellError> {
    if text.len() != 7 || !text.bytes().all(|byte| matches!(byte, b'0' | b'1')) {
        return Err(CellError::Value);
    }
    let mut weekends = [false; 7];
    for (index, byte) in text.bytes().enumerate() {
        // SAFETY: modulo seven bounds the index in the seven-entry local array.
        weekends[(index + 1) % 7] = byte == b'1';
    }
    if consumer == WeekendConsumer::WorkDay && weekends.iter().all(|day| *day) {
        return Err(CellError::Value);
    }
    Ok(weekends)
}

#[cfg(test)]
mod business_day_tests {
    use super::*;
    #[test]
    fn weekend_masks_keep_consumer_specific_empty_week_rules() {
        assert_eq!(
            coercev2_weekend_mask("1111111", WeekendConsumer::NetworkDays),
            Ok([true; 7])
        );
        assert_eq!(
            coercev2_weekend_mask("1111111", WeekendConsumer::WorkDay),
            Err(CellError::Value)
        );
        assert_eq!(
            coercev2_weekend_mask("0000000", WeekendConsumer::WorkDay),
            Ok([false; 7])
        );
        assert_eq!(
            coercev2_weekend_mask("0000011", WeekendConsumer::WorkDay),
            Ok([true, false, false, false, false, false, true])
        );
        for text in ["", "1", "00000000", "000000x", "000000é"] {
            assert_eq!(
                coercev2_weekend_mask(text, WeekendConsumer::WorkDay),
                Err(CellError::Value)
            );
        }
    }
}
