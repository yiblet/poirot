//! Exact saved-value wire representation.
use crate::{CellError, CellValue, ExcelText};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
/// Lossless snapshot value encoding numeric bits and exact UTF-16 units.
#[derive(PartialEq, Eq, Serialize, Deserialize)]
pub enum Value<'a> {
    /// Empty saved value.
    Blank,
    /// Exact binary64 bits, preserving signed zero and NaN payloads.
    Number(u64),
    /// UTF-8 text payload.
    Text(Cow<'a, str>),
    /// Exact UTF-16 units, including unpaired surrogates.
    Units(Cow<'a, [u16]>),
    /// Boolean saved value.
    Boolean(bool),
    /// Spreadsheet error saved value.
    Error(CellError),
}
impl<'a> Value<'a> {
    /// Borrow or encode a cell value without normalizing its payload.
    pub fn capture(value: &'a CellValue) -> Self {
        match value {
            CellValue::Blank => Self::Blank,
            CellValue::Number(n) => Self::Number(n.to_bits()),
            CellValue::Text(s) => Self::Text(Cow::Borrowed(s)),
            CellValue::TextUtf16(s) => Self::Units(Cow::Borrowed(s.units())),
            CellValue::Boolean(b) => Self::Boolean(*b),
            CellValue::Error(e) => Self::Error(*e),
        }
    }
    /// Detach a snapshot value from borrowed source storage.
    pub fn into_owned(self) -> Value<'static> {
        match self {
            Self::Blank => Value::Blank,
            Self::Number(n) => Value::Number(n),
            Self::Text(s) => Value::Text(Cow::Owned(s.into_owned())),
            Self::Units(s) => Value::Units(Cow::Owned(s.into_owned())),
            Self::Boolean(b) => Value::Boolean(b),
            Self::Error(e) => Value::Error(e),
        }
    }
    /// Reconstruct the exact runtime cell value.
    pub fn restore(self) -> CellValue {
        match self {
            Self::Blank => CellValue::Blank,
            Self::Number(n) => CellValue::Number(f64::from_bits(n)),
            Self::Text(s) => CellValue::Text(s.into_owned()),
            Self::Units(s) => CellValue::TextUtf16(ExcelText::from_units(s.into_owned())),
            Self::Boolean(b) => CellValue::Boolean(b),
            Self::Error(e) => CellValue::Error(e),
        }
    }
}
/// Field adapter that preserves numeric bits and exact UTF-16 with every Serde codec.
pub mod value_serde {
    use super::*;
    /// Serialize cell values through the lossless snapshot encoding.
    pub fn serialize<S: serde::Serializer>(
        value: &CellValue,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Value::capture(value).serialize(serializer)
    }
    /// Decode lossless snapshot values into canonical model values.
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<CellValue, D::Error> {
        Ok(Value::<'static>::deserialize(deserializer)?.restore())
    }
}
