//! Evidence-bounded workbook identifier comparison, independent of spelling storage.
//!
//! Exact strings always match. ASCII and Latin-1 uppercase/lowercase pairs use
//! one-codepoint lowercase mappings shared by pinned POI Locale.ROOT lowercase.
//! Unequal UTF-16 lengths are different, including unsupported case mappings.
//! Other unequal non-ASCII characters produce `UnsupportedMapping`; callers must
//! not invent Unicode normalization, expansion, or Java character equivalence.
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

/// An unresolved case mapping between two preserved identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentifierError {
    /// Existing identifier involved in the unsupported comparison.
    pub stored: String,
    /// Requested identifier involved in the unsupported comparison.
    pub requested: String,
}
impl std::fmt::Display for IdentifierError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unsupported identifier comparison: {:?} versus {:?}",
            self.stored, self.requested
        )
    }
}
impl std::error::Error for IdentifierError {}

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

#[cfg(test)]
mod consumer_tests {
    use crate::{NamedRange, Workbook};
    #[test]
    fn model_lookup_duplicate_and_scope_share_policy() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Ä Data").unwrap();
        workbook.add_sheet("Other").unwrap();
        assert_eq!(workbook.try_sheet_index_by_name("ä dATA"), Ok(Some(0)));
        assert_eq!(workbook.sheet_by_name("ä data").unwrap().name(), "Ä Data");
        assert!(workbook.add_sheet("ä DATA").is_err());
        assert!(workbook.rename_sheet(1, "ä data").is_err());
        workbook.define_name("ÉRate", "1", None).unwrap();
        workbook.define_name("érate", "2", Some(0)).unwrap();
        assert!(workbook.define_name("ÉRATE", "3", Some(0)).is_err());
        assert_eq!(
            workbook
                .try_defined_name("éRATE", 0)
                .unwrap()
                .unwrap()
                .formula,
            "2"
        );
        assert_eq!(
            workbook
                .try_defined_name("érate", 1)
                .unwrap()
                .unwrap()
                .formula,
            "1"
        );
        assert_eq!(workbook.named_ranges[0].name, "ÉRate");
    }
    #[test]
    fn unresolved_case_is_diagnostic_and_raw_import_stays_permissive() {
        let mut workbook = Workbook::new();
        workbook.import_sheet("Σ").unwrap();
        assert!(workbook.try_sheet_index_by_name("σ").is_err());
        assert!(
            workbook
                .add_sheet("σ")
                .unwrap_err()
                .0
                .contains("unsupported identifier")
        );
        workbook.import_sheet("σ").unwrap();
        workbook
            .import_sheet("'Imported invalid authoring name'")
            .unwrap();
        assert_eq!(workbook.try_sheet_index_by_name("σ"), Ok(Some(1)));
        workbook.named_ranges.push(NamedRange {
            name: "Σ".into(),
            formula: "1".into(),
            sheet_scope: Some(0),
        });
        workbook.named_ranges.push(NamedRange {
            name: "σ".into(),
            formula: "2".into(),
            sheet_scope: None,
        });
        assert!(workbook.try_defined_name("σ", 0).is_err());
        assert_eq!(
            workbook.try_defined_name("σ", 1).unwrap().unwrap().formula,
            "2"
        );
    }
}

#[cfg(test)]
mod utf16_length_consumer_tests {
    use crate::Workbook;

    #[test]
    fn distinct_utf16_lengths_allow_sheet_creation_and_checked_lookup() {
        for (stored, requested) in [("Σ", "Étage"), ("😀", "a")] {
            let mut workbook = Workbook::new();
            workbook.add_sheet(stored).unwrap();
            assert_eq!(workbook.try_sheet_index_by_name(requested), Ok(None));
            workbook.add_sheet(requested).unwrap();
            assert_eq!(workbook.try_sheet_index_by_name(requested), Ok(Some(1)));
            assert_eq!(workbook.try_sheet_index_by_name(stored), Ok(Some(0)));
        }
    }

    #[test]
    fn equal_utf16_lengths_keep_sheet_creation_and_lookup_diagnostics() {
        for (stored, requested) in [("Σ", "σ"), ("😀", "ab")] {
            let mut workbook = Workbook::new();
            workbook.add_sheet(stored).unwrap();
            assert!(workbook.try_sheet_index_by_name(requested).is_err());
            assert!(
                workbook
                    .add_sheet(requested)
                    .unwrap_err()
                    .0
                    .contains("unsupported identifier")
            );
            assert_eq!(workbook.sheets().len(), 1);
        }
    }
}
