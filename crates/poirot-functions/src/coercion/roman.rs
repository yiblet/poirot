//! Roman numeral argument policies, separate from the formatter's canonical syntax.
use crate::coercion::{CellError, TextRef, compatibility::ScalarRef};

/// Pinned POI requires two operands; documented Excel admits an omitted form.
pub fn coercev2_roman_arity(count: usize, documented: bool) -> bool {
    count == 2 || (documented && count == 1)
}

/// Preserve omission and logical identity until the ROMAN form consumer asks.
/// Other scalars continue through the host's contextual numeric conversion.
pub fn coercev2_roman_form_default(
    missing: bool,
    boolean: Option<bool>,
    documented: bool,
) -> Option<i32> {
    if !documented {
        None
    } else if missing {
        Some(0)
    } else {
        boolean.map(|value| if value { 0 } else { 4 })
    }
}

/// ARABIC accepts text and empty cells, preserving source errors and omission.
/// It does not turn numbers or logical values into Roman text.
pub fn coercev2_arabic_scalar(value: ScalarRef<'_>) -> Result<TextRef<'_>, CellError> {
    match value {
        ScalarRef::Text(text) => Ok(text),
        ScalarRef::Blank => Ok(TextRef::Utf8("")),
        ScalarRef::Error(error) => Err(error),
        _ => Err(CellError::Value),
    }
}

/// Parse up to 255 UTF-16 units, ignoring ASCII outer spaces and letter case.
/// A single leading minus is admitted. Smaller symbols subtract from their
/// immediately following larger symbol; other symbols add. This deliberately
/// avoids imposing the formatter's canonical grammar. Undocumented noncanonical
/// spellings are an arithmetic extension, not a claim of native Excel parity.
pub fn coercev2_arabic_text(text: &str) -> Result<i32, CellError> {
    if text.encode_utf16().take(256).count() > 255 {
        return Err(CellError::Value);
    }
    let text = text.trim_matches(' ');
    let (negative, text) = match text.strip_prefix('-') {
        Some("") => return Err(CellError::Value),
        Some(text) => (true, text),
        None => (false, text),
    };
    let mut symbols = text
        .bytes()
        .map(|byte| match byte.to_ascii_uppercase() {
            b'I' => Ok(1i32),
            b'V' => Ok(5),
            b'X' => Ok(10),
            b'L' => Ok(50),
            b'C' => Ok(100),
            b'D' => Ok(500),
            b'M' => Ok(1000),
            _ => Err(CellError::Value),
        })
        .peekable();
    let mut total = 0i32;
    while let Some(symbol) = symbols.next() {
        let symbol = symbol?;
        let term = match symbols.peek().copied().transpose()? {
            Some(next) if symbol < next => {
                symbols.next();
                next.checked_sub(symbol).ok_or(CellError::Value)?
            }
            _ => symbol,
        };
        total = total.checked_add(term).ok_or(CellError::Value)?;
    }
    if negative {
        total.checked_neg().ok_or(CellError::Value)
    } else {
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arabic_documented_domains_and_independent_calc_examples() {
        for (text, expected) in [
            ("mcmxii", 1912),
            ("MXMVII", 1997),
            ("", 0),
            ("  ", 0),
            (" -MMXI ", -2011),
            ("IIII", 4),
            ("IL", 49),
            ("IC", 99),
            ("IM", 999),
        ] {
            assert_eq!(coercev2_arabic_text(text), Ok(expected), "{text}");
        }
        assert_eq!(coercev2_arabic_text(&"M".repeat(255)), Ok(255000));
        for text in [
            "+I", "-", "--I", "- I", "I I", "I-", "\tI", "I\n", "Ⅳ", "N", "\u{a0}I",
        ] {
            assert_eq!(coercev2_arabic_text(text), Err(CellError::Value), "{text}");
        }
        assert_eq!(
            coercev2_arabic_text(&"M".repeat(256)),
            Err(CellError::Value)
        );
        assert_eq!(
            coercev2_arabic_text(&format!(" {}", "M".repeat(255))),
            Err(CellError::Value)
        );
    }
    #[test]
    fn arbitrary_symbol_sequences_stay_in_the_documented_magnitude_bound() {
        // These spellings have no native Excel oracle. Verify only totality
        // and the bound, without freezing a particular undocumented value.
        for text in ["IIV", "XIL", "VV", "IXC", "XIXI"] {
            if let Ok(value) = coercev2_arabic_text(text) {
                assert!(value.abs() <= 1000 * text.len() as i32);
            }
        }
    }
}
