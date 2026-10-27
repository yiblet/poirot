//! Evidence-bounded workbook identifier comparison, independent of spelling storage.
//!
//! Exact strings always match. ASCII and Latin-1 uppercase/lowercase pairs use
//! one-codepoint lowercase mappings shared by pinned POI Locale.ROOT lowercase.
//! Unequal UTF-16 lengths are different, including unsupported case mappings.
//! Other unequal non-ASCII characters produce `UnsupportedMapping`; callers must
//! not invent Unicode normalization, expansion, or Java character equivalence.
//!
//! ```rust
//! use poirot_io::identifiers::{checked_matches, compare, IdentifierComparison};
//!
//! assert!(checked_matches("Revenue", "REVENUE")?);
//! assert!(!checked_matches("Revenue", "Costs")?);
//! // Applications can surface unresolved comparisons instead of guessing identity.
//! assert_eq!(compare("Σ", "σ"), IdentifierComparison::UnsupportedMapping);
//! assert!(checked_matches("Σ", "σ").is_err());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
pub use crate::identifier_error::IdentifierError;

/// Checked identifier comparison that retains unsupported Unicode mappings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierComparison {
    /// Spellings match under the supported identifier policy.
    Equal,
    /// Spellings are distinguishable under the supported policy.
    Different,
    /// Comparison needs case mappings the identifier policy cannot justify.
    UnsupportedMapping,
}

/// Compare identifiers, refusing unsupported mapping instead of guessing a match.
pub fn checked_matches(stored: &str, requested: &str) -> Result<bool, IdentifierError> {
    match compare(stored, requested) {
        IdentifierComparison::Equal => Ok(true),
        IdentifierComparison::Different => Ok(false),
        IdentifierComparison::UnsupportedMapping => Err(IdentifierError {
            stored: stored.into(),
            requested: requested.into(),
        }),
    }
}

fn supported_lower(c: char) -> Option<char> {
    match c {
        'A'..='Z' => Some(c.to_ascii_lowercase()),
        '\u{00c0}'..='\u{00d6}' | '\u{00d8}'..='\u{00de}' => char::from_u32(c as u32 + 32),
        c if c.is_ascii()
            || c == 'ß'
            || matches!(c, '\u{00e0}'..='\u{00f6}' | '\u{00f8}'..='\u{00fe}') =>
        {
            Some(c)
        }
        _ => None,
    }
}

/// No allocation, locale dependence, normalization, or multi-character expansion.
/// Unsupported means unresolved compatibility, not an assertion of inequality.
pub fn compare(left: &str, right: &str) -> IdentifierComparison {
    if left == right {
        return IdentifierComparison::Equal;
    }
    let mut unsupported = false;
    let mut left_chars = left.chars();
    let mut right_chars = right.chars();
    loop {
        match (left_chars.next(), right_chars.next()) {
            (None, None) => break,
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => match (supported_lower(a), supported_lower(b)) {
                (Some(a), Some(b)) if a == b => {}
                (Some(_), Some(_)) => {
                    return if unsupported {
                        unsupported_comparison(left, right)
                    } else {
                        IdentifierComparison::Different
                    };
                }
                _ => unsupported = true,
            },
            (Some(c), None) | (None, Some(c)) => {
                return if supported_lower(c).is_some() && !unsupported {
                    IdentifierComparison::Different
                } else {
                    unsupported_comparison(left, right)
                };
            }
        }
    }
    if unsupported {
        unsupported_comparison(left, right)
    } else {
        IdentifierComparison::Equal
    }
}

// POI StringUtil.equalsIgnoreCase rejects unequal Java String lengths before folding.
// Keep this scan confined to otherwise unsupported comparisons.
fn unsupported_comparison(left: &str, right: &str) -> IdentifierComparison {
    if left.encode_utf16().count() != right.encode_utf16().count() {
        IdentifierComparison::Different
    } else {
        IdentifierComparison::UnsupportedMapping
    }
}

/// Conservative matching for legacy Option-returning lookup APIs.
pub fn matches(left: &str, right: &str) -> bool {
    compare(left, right) == IdentifierComparison::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_identifier_contract() {
        for (left, right, expected) in [
            ("Data", "dATA", IdentifierComparison::Equal),
            ("ÄÖÜÉ", "äöüé", IdentifierComparison::Equal),
            ("東京A", "東京a", IdentifierComparison::Equal),
            ("Σ", "Σ", IdentifierComparison::Equal),
            ("ß", "SS", IdentifierComparison::Different),
            ("é", "e\u{301}", IdentifierComparison::Different),
            ("İ", "i", IdentifierComparison::UnsupportedMapping),
            ("İ", "i\u{307}", IdentifierComparison::Different),
            ("Σ", "Étage", IdentifierComparison::Different),
            ("😀", "ab", IdentifierComparison::UnsupportedMapping),
            ("😀", "a", IdentifierComparison::Different),
            ("😀", "Σ", IdentifierComparison::Different),
            ("Σ", "é", IdentifierComparison::UnsupportedMapping),
            ("Σ", "σ", IdentifierComparison::UnsupportedMapping),
            ("Σ", "ς", IdentifierComparison::UnsupportedMapping),
            ("K", "k", IdentifierComparison::UnsupportedMapping),
            ("ẞ", "ß", IdentifierComparison::UnsupportedMapping),
            ("I", "ı", IdentifierComparison::UnsupportedMapping),
        ] {
            assert_eq!(compare(left, right), expected, "{left:?} / {right:?}");
            assert_eq!(compare(right, left), expected);
        }
    }
}
