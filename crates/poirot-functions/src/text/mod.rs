//! Text calculations for spreadsheet functions.
//!
//! Includes title casing with [`proper`](crate::text::proper), Roman numerals with [`roman`](crate::text::roman), numeric-text
//! operations, and substring planning. The caller evaluates arguments and controls
//! allocation limits.
//!
//! [`java_case`](crate::text::java_case) implements Unicode casing. Case-insensitive comparison uses
//! [text_units](https://docs.rs/poirot-functions/latest/poirot_functions/text_units), which also defines text counting and
//! slicing rules.
mod case_asset_error;

mod java_case_error;

pub use java_case_error::JavaCaseError;

mod text_plan_error;

pub use text_plan_error::TextPlanError;

mod modern_text_error;

pub use modern_text_error::ModernTextError;

mod regex_error;

pub use regex_error::RegexError;
/// Japanese ASCII and katakana width conversion.
pub mod width;

/// Thai currency spelling from integer baht and satang.
pub mod baht;

/// Unicode15 full-case mapping with contextual final sigma.
pub mod java_case;
/// Modern delimiter selection, splitting, and text serialization.
pub mod modern;
/// NUMBERVALUE parsing with spreadsheet/source failure separation.
pub mod numeric;
/// Bounded PCRE2 matching, extraction and replacement.
pub mod regex;
/// Allocation-free UTF-16 selection and output-reservation plans.
pub mod units;
/// Allocation-free UTF-8 URL component encoding.
pub mod url;
/// Bounded local XML and XPath selection.
pub mod xml;

pub use crate::coercion::{NumericTextError, NumericTextSourceError};
/// Java processes UTF-16 `char`s. Supplementary letters retain their spelling
/// and reset the next BMP character's word state instead of being case-mapped.
pub fn proper_with<P: UnicodePolicy>(text: &str, policy: &P) -> String {
    let mut output = String::new();
    let mut upper = true;
    for c in text.chars() {
        if c.len_utf16() == 2 {
            output.push(c);
            upper = true;
            continue;
        }
        output.push_str(&policy.case(c, upper));
        upper = !policy.is_letter(c);
    }
    output
}

/// Host-provided character classification and case mapping used by PROPER.
pub trait UnicodePolicy {
    /// Classify a code point using the selected source Unicode contract.
    fn is_letter(&self, character: char) -> bool;
    /// Apply the requested source-compatible case mapping.
    fn case(&self, character: char, uppercase: bool) -> String;
}

/// Unicode letter classification and case conversion used by modern text operations.
/// Category tables come from unicode-general-category 1.1.0 and case mappings from the Rust toolchain, so results can differ from a particular Java runtime.
pub struct ModernUnicode;
impl UnicodePolicy for ModernUnicode {
    fn is_letter(&self, character: char) -> bool {
        use unicode_general_category::{GeneralCategory, get_general_category};
        matches!(
            get_general_category(character),
            GeneralCategory::UppercaseLetter
                | GeneralCategory::LowercaseLetter
                | GeneralCategory::TitlecaseLetter
                | GeneralCategory::ModifierLetter
                | GeneralCategory::OtherLetter
        )
    }
    fn case(&self, character: char, uppercase: bool) -> String {
        if uppercase {
            character.to_uppercase().collect()
        } else {
            character.to_lowercase().collect()
        }
    }
}

/// Capitalize word starts using the supplied Unicode policy.
pub fn proper(text: &str) -> String {
    proper_with(text, &ModernUnicode)
}

/// Render an admitted number using the selected Roman-numeral form.
pub fn roman(mut number: u16, form: u8) -> String {
    let mut result = String::new();
    for (n, s) in [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ] {
        while number >= n {
            number -= n;
            result.push_str(s);
        }
    }
    let replacements: &[&[(&str, &str)]] = &[
        &[
            ("XLV", "VL"),
            ("XCV", "VC"),
            ("CDL", "LD"),
            ("CML", "LM"),
            ("CMVC", "LMVL"),
        ],
        &[
            ("CDXC", "LDXL"),
            ("CDVC", "LDVL"),
            ("CMXC", "LMXL"),
            ("XCIX", "VCIV"),
            ("XLIX", "VLIV"),
        ],
        &[
            ("XLIX", "IL"),
            ("XCIX", "IC"),
            ("CDXC", "XD"),
            ("CDVC", "XDV"),
            ("CDIC", "XDIX"),
            ("LMVL", "XMV"),
            ("CMIC", "XMIX"),
            ("CMXC", "XM"),
        ],
        &[
            ("XDV", "VD"),
            ("XDIX", "VDIV"),
            ("XMV", "VM"),
            ("XMIX", "VMIV"),
        ],
        &[("VDIV", "ID"), ("VMIV", "IM")],
    ];
    if form > 0 {
        for (i, pairs) in replacements.iter().enumerate().take(form as usize + 1) {
            if i == 1 && form > 1 {
                continue;
            }
            for (from, to) in *pairs {
                result = result.replace(from, to);
            }
        }
    }
    result
}
