//! Spreadsheet value text retains exact UTF-16 units without changing UTF-8 ingress.
use crate::scalar::{CellError, ExcelText};
use std::{
    borrow::Cow,
    hash::{Hash, Hasher},
};

/// Borrowed spreadsheet text stored as UTF-8 or exact UTF-16 units.
///
/// Text identity uses [`TextRef::units`].
#[derive(Clone, Copy, Debug)]
pub enum TextRef<'a> {
    /// Borrowed Rust text interpreted through UTF-16 unit iteration.
    Utf8(&'a str),
    /// Borrowed exact Excel text, including unpaired surrogates.
    Utf16(&'a [u16]),
}
/// Allocation-free iterator over the authoritative code units.
#[derive(Clone)]
pub enum TextUnits<'a> {
    /// UTF-16 encoding iterator over UTF-8 storage.
    Utf8(std::str::EncodeUtf16<'a>),
    /// Direct iterator over preserved UTF-16 storage.
    Utf16(std::iter::Copied<std::slice::Iter<'a, u16>>),
}
impl Iterator for TextUnits<'_> {
    /// One preserved or UTF-8-encoded UTF-16 code unit.
    type Item = u16;
    fn next(&mut self) -> Option<u16> {
        match self {
            Self::Utf8(i) => i.next(),
            Self::Utf16(i) => i.next(),
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Utf8(i) => i.size_hint(),
            Self::Utf16(i) => i.size_hint(),
        }
    }
}
impl std::iter::FusedIterator for TextUnits<'_> {}
impl<'a> TextRef<'a> {
    /// Iterate UTF-16 units without requiring a normalized string.
    pub fn units(self) -> TextUnits<'a> {
        match self {
            Self::Utf8(text) => TextUnits::Utf8(text.encode_utf16()),
            Self::Utf16(units) => TextUnits::Utf16(units.iter().copied()),
        }
    }
    /// Count Excel text units across either storage representation.
    pub fn len_units(self) -> usize {
        match self {
            Self::Utf8(text) => text.encode_utf16().count(),
            Self::Utf16(u) => u.len(),
        }
    }
    /// Whether the borrowed text contains no code units.
    pub fn is_empty(self) -> bool {
        match self {
            Self::Utf8(text) => text.is_empty(),
            Self::Utf16(u) => u.is_empty(),
        }
    }
    /// Bytes retained by this representation; distinct from semantic unit length.
    pub fn storage_bytes(self) -> usize {
        match self {
            Self::Utf8(text) => text.len(),
            Self::Utf16(units) => units.len().saturating_mul(2),
        }
    }
    /// Validate and measure UTF-8 projection without allocating or replacing units.
    /// Hosts must admit these output bytes before calling `to_utf8()`.
    pub fn utf8_len(self) -> Result<usize, std::char::DecodeUtf16Error> {
        match self {
            Self::Utf8(text) => Ok(text.len()),
            Self::Utf16(_) => std::char::decode_utf16(self.units())
                .try_fold(0usize, |bytes, scalar| {
                    scalar.map(|c| bytes.saturating_add(c.len_utf8()))
                }),
        }
    }
    /// Never substitutes U+FFFD. The UTF-8 representation is borrowed when available.
    pub fn to_utf8(self) -> Result<Cow<'a, str>, std::string::FromUtf16Error> {
        match self {
            Self::Utf8(text) => Ok(Cow::Borrowed(text)),
            Self::Utf16(units) => String::from_utf16(units).map(Cow::Owned),
        }
    }
    /// Copy text into exact UTF-16 storage.
    pub fn to_excel_text(self) -> ExcelText {
        ExcelText::from_units(self.units().collect::<Vec<_>>())
    }
}
impl PartialEq for TextRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.units().eq(other.units())
    }
}
impl Eq for TextRef<'_> {}
impl Hash for TextRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.len_units().hash(state);
        for unit in self.units() {
            unit.hash(state);
        }
    }
}

/// A typed spreadsheet value, independent of its display format.
///
/// Blank, empty text, numbers, booleans, and errors remain distinct. Text can retain
/// unpaired UTF-16 surrogates; use [`CellValue::text_ref`] for lossless access.
/// A value carries neither a worksheet address nor formula source: the same type
/// represents a literal input, a saved formula result, or a newly calculated scalar.
/// `Blank` alone does not tell whether a source cell exists.
/// Numeric inputs accept `f64` and exact widening from `f32`, `i8`, `i16`, `i32`,
/// `u8`, `u16`, and `u32`. Convert wider integers explicitly when precision loss
/// is acceptable.
#[derive(Debug, Clone, Default)]
pub enum CellValue {
    /// Empty cell value, independent of whether a source cell exists.
    #[default]
    Blank,
    /// IEEE-754 numeric value; fidelity policy is enforced at boundaries.
    Number(f64),
    /// Compatible ordinary Unicode scalar ingress.
    Text(String),
    /// Exact UTF-16, including isolated surrogates. Prefer `From<ExcelText>`.
    TextUtf16(ExcelText),
    /// Boolean worksheet value.
    Boolean(bool),
    /// Spreadsheet error value, distinct from an operational failure.
    Error(CellError),
}
impl CellValue {
    /// Return the number only for a numeric value, without coercion or rounding.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    /// Return the boolean only for a boolean value, without coercion.
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    /// Text clone payload bytes. Scalar/blank/error variants allocate no text payload.
    pub fn text_storage_bytes(&self) -> usize {
        self.text_ref().map_or(0, TextRef::storage_bytes)
    }
    /// Borrow either text representation without converting its units.
    pub fn text_ref(&self) -> Option<TextRef<'_>> {
        match self {
            Self::Text(text) => Some(TextRef::Utf8(text)),
            Self::TextUtf16(text) => Some(TextRef::Utf16(text.units())),
            _ => None,
        }
    }
}
impl PartialEq for CellValue {
    fn eq(&self, other: &Self) -> bool {
        if let (Some(a), Some(b)) = (self.text_ref(), other.text_ref()) {
            return a == b;
        }
        match (self, other) {
            (Self::Blank, Self::Blank) => true,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::Error(a), Self::Error(b)) => a == b,
            _ => false,
        }
    }
}
impl From<ExcelText> for CellValue {
    fn from(text: ExcelText) -> Self {
        if TextRef::Utf16(text.units()).utf8_len().is_err() {
            return Self::TextUtf16(text);
        }
        match text.to_utf8() {
            Ok(text) => Self::Text(text),
            Err(_) => Self::TextUtf16(text),
        }
    }
}
impl From<f64> for CellValue {
    fn from(v: f64) -> Self {
        Self::Number(v)
    }
}
// Each accepted primitive widens exactly to binary64 using the standard conversion.
macro_rules! from_exact_number {
    ($($number:ty),+ $(,)?) => {
        $(impl From<$number> for CellValue {
            fn from(value: $number) -> Self {
                Self::Number(f64::from(value))
            }
        })+
    };
}
from_exact_number!(i8, i16, i32, u8, u16, u32, f32);

impl From<String> for CellValue {
    fn from(v: String) -> Self {
        Self::Text(v)
    }
}
impl From<&str> for CellValue {
    fn from(v: &str) -> Self {
        Self::Text(v.into())
    }
}
impl From<bool> for CellValue {
    fn from(v: bool) -> Self {
        Self::Boolean(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    fn hash(text: TextRef<'_>) -> u64 {
        let mut h = DefaultHasher::new();
        text.hash(&mut h);
        h.finish()
    }
    #[test]
    fn strict_scalar_inspectors_preserve_values() {
        for number in [
            0.0,
            -0.0,
            f64::MIN,
            f64::MAX,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::from_bits(0x7ff8_0000_0000_0042),
            f64::from_bits(0xfff8_0000_0000_0042),
        ] {
            let value = CellValue::Number(number);
            assert_eq!(value.as_number().unwrap().to_bits(), number.to_bits());
            assert_eq!(value.as_boolean(), None);
        }
        for boolean in [false, true] {
            let value = CellValue::Boolean(boolean);
            assert_eq!(value.as_boolean(), Some(boolean));
            assert_eq!(value.as_number(), None);
        }
    }

    #[test]
    fn strict_scalar_inspectors_reject_other_variants() {
        let mut values = vec![
            CellValue::Blank,
            CellValue::Text("42".into()),
            CellValue::Text("TRUE".into()),
            CellValue::TextUtf16(ExcelText::from("42")),
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800])),
        ];
        values.extend(
            [
                CellError::Null,
                CellError::Div0,
                CellError::Value,
                CellError::Ref,
                CellError::Name,
                CellError::Num,
                CellError::NA,
                CellError::GettingData,
                CellError::Spill,
                CellError::Calc,
            ]
            .map(CellValue::Error),
        );
        for value in values {
            assert_eq!(value.as_number(), None, "{value:?}");
            assert_eq!(value.as_boolean(), None, "{value:?}");
        }
    }

    #[test]
    fn integer_ingress_preserves_sign_and_exact_boundaries() {
        for (input, expected) in [
            (CellValue::from(i32::MIN), -2_147_483_648.),
            (CellValue::from(i32::MAX), 2_147_483_647.),
            (CellValue::from(u32::MAX), 4_294_967_295.),
            (CellValue::from(i16::MIN), -32_768.),
            (CellValue::from(u16::MAX), 65_535.),
            (CellValue::from(i8::MIN), -128.),
            (CellValue::from(u8::MAX), 255.),
            (CellValue::from(0), 0.),
        ] {
            assert_eq!(input, CellValue::Number(expected));
        }
    }

    #[test]
    fn float32_ingress_widens_without_changing_numeric_policy() {
        for input in [
            0.0f32,
            -0.0,
            0.1,
            f32::MIN,
            f32::MAX,
            f32::MIN_POSITIVE,
            f32::from_bits(1),
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let CellValue::Number(widened) = CellValue::from(input) else {
                panic!("numeric input must remain numeric");
            };
            assert_eq!(widened.to_bits(), f64::from(input).to_bits());
        }
        // NaN stays a numeric NaN, as with direct f64 ingress.
        assert!(matches!(CellValue::from(f32::NAN), CellValue::Number(n) if n.is_nan()));
    }

    #[test]
    fn exact_units_survive_model_and_projection() {
        for units in [vec![0xd83d], vec![0xde00], vec![0xd83d, 0x61], vec![0xfffd]] {
            let value = CellValue::from(ExcelText::from_units(units.clone()));
            assert_eq!(value.text_ref().unwrap().units().collect::<Vec<_>>(), units);
        }
        let high = CellValue::from(ExcelText::from_units(vec![0xd83d]));
        let low = CellValue::from(ExcelText::from_units(vec![0xde00]));
        assert_ne!(high, low);
        assert_ne!(high, CellValue::from("�"));
        assert!(high.text_ref().unwrap().to_utf8().is_err());
    }
    #[test]
    fn equivalent_valid_storage_has_equal_identity_and_hash() {
        for text in ["", "ASCII", "é", "😀", "a\0b"] {
            let a = CellValue::Text(text.into());
            let b = CellValue::TextUtf16(ExcelText::from(text));
            assert_eq!(a, b);
            assert_eq!(hash(a.text_ref().unwrap()), hash(b.text_ref().unwrap()));
            assert_eq!(b.text_ref().unwrap().to_utf8().unwrap(), text);
            assert!(matches!(
                CellValue::from(ExcelText::from(text)),
                CellValue::Text(_)
            ));
        }
    }
    #[test]
    fn utf8_projection_measurement_validates_before_allocation() {
        for text in ["", "ASCII", "é", "漢", "😀", "a\0b"] {
            let units: Vec<u16> = text.encode_utf16().collect();
            let view = TextRef::Utf16(&units);
            assert_eq!(view.utf8_len().unwrap(), text.len());
            assert_eq!(view.storage_bytes(), units.len() * 2);
            assert_eq!(TextRef::Utf8(text).utf8_len().unwrap(), text.len());
            assert_eq!(
                CellValue::Text(text.into()).text_storage_bytes(),
                text.len()
            );
        }
        for units in [vec![0xd83d], vec![0xde00], vec![0xd83d, 0x61]] {
            assert!(TextRef::Utf16(&units).utf8_len().is_err());
            assert_eq!(
                CellValue::TextUtf16(ExcelText::from_units(units.clone())).text_storage_bytes(),
                units.len() * 2
            );
        }
        assert_eq!(CellValue::Blank.text_storage_bytes(), 0);
    }
}
