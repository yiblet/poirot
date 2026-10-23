//! Text calculations for spreadsheet functions.
//!
//! Includes title casing with [`proper`], Roman numerals with [`roman`], numeric-text
//! operations, and substring planning. The caller evaluates arguments and controls
//! allocation limits.
//!
//! [`java_case`] implements Unicode casing. Case-insensitive comparison uses
//! [poirot-text](https://docs.rs/poirot-text), which also defines text counting and
//! slicing rules.
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

pub use poirot_coercionv2::numeric_text::{Error, HostFault};
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

/// Concrete modern Unicode policy. Category tables are pinned to crate 1.1.0;
/// case mappings use the Rust toolchain's tables. Java runtime-version drift is
/// an explicit compatibility limitation, rather than silently using Alphabetic.
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
