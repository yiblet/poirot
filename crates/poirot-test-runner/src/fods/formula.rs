//! Adapt OpenFormula syntax while preserving arguments and quoted literals.
use regex::{Captures, Regex};
use std::sync::OnceLock;

/// Precompiled source-spelling rewrite patterns shared by ODF formula translation.
struct FormulaRegex {
    references: Regex,
    absolute_sheet: Regex,
    sheet_reference: Regex,
}
fn formula_regex() -> &'static FormulaRegex {
    static REGEX: OnceLock<FormulaRegex> = OnceLock::new();
    REGEX.get_or_init(|| FormulaRegex {
        // SAFETY: These literal regexes have balanced groups/classes and supported syntax; no caller input is compiled.
        references: Regex::new(r"\[([^\]]*)\]").unwrap(),
        // SAFETY: These literal regexes have balanced groups/classes and supported syntax; no caller input is compiled.
        absolute_sheet: Regex::new(r"\$(?:'|[A-Za-z_][^:]*\.)").unwrap(),
        // SAFETY: These literal regexes have balanced groups/classes and supported syntax; no caller input is compiled.
        sheet_reference: Regex::new(r"((?:'[^']*'|[A-Za-z_][A-Za-z_0-9 ]*))\.(\$?[A-Za-z]+\$?\d)")
            .unwrap(),
    })
}
fn translate_unquoted(part: &str) -> String {
    let regex = formula_regex();
    let converted = regex.references.replace_all(part, |m: &Captures<'_>| {
        let Some(reference) = m.get(1).map(|part| part.as_str()) else {
            return m
                .get(0)
                .map_or_else(String::new, |part| part.as_str().to_owned());
        };
        if reference.contains('#') {
            return m
                .get(0)
                .map_or_else(String::new, |part| part.as_str().to_owned());
        }
        let reference = reference.replace(":.", ":");
        let reference = reference.strip_prefix('.').unwrap_or(&reference);
        // The dollar qualifies the sheet token, not the cell address.
        let reference = regex
            .absolute_sheet
            .replace_all(reference, |m: &Captures<'_>| {
                m.get(0).map_or_else(String::new, |part| {
                    part.as_str()
                        .strip_prefix('$')
                        .unwrap_or(part.as_str())
                        .to_owned()
                })
            });
        regex
            .sheet_reference
            .replace_all(&reference, "$1!$2")
            .into_owned()
    });
    converted
        .replace(';', ",")
        .replace('|', ";")
        .replace("COM.MICROSOFT.", "")
        .replace("com.microsoft.", "")
        .replace("ORG.OPENOFFICE.", "")
        .replace("org.openoffice.", "")
}
/// Translate OpenFormula references and separators, preserving double-quoted strings.
pub fn translate(formula: &str) -> String {
    let formula = formula
        .strip_prefix("of:=")
        .or_else(|| formula.strip_prefix("oooc:="))
        .unwrap_or(formula);
    let mut result = String::new();
    let mut start = 0;
    let mut index = 0;
    let bytes = formula.as_bytes();
    while index < bytes.len() {
        // SAFETY: Both scanning loops check index < bytes.len(); the doubled-quote lookahead also checks index + 1 < bytes.len().
        if bytes[index] != b'"' {
            index += 1;
            continue;
        }
        // SAFETY: Quote boundaries are ASCII byte positions found by this loop, and start/index never exceed formula.len(); all emitted slices use those boundaries.
        result.push_str(&translate_unquoted(&formula[start..index]));
        let quoted_start = index;
        index += 1;
        while index < bytes.len() {
            // SAFETY: Both scanning loops check index < bytes.len(); the doubled-quote lookahead also checks index + 1 < bytes.len().
            if bytes[index] == b'"' {
                // SAFETY: Both scanning loops check index < bytes.len(); the doubled-quote lookahead also checks index + 1 < bytes.len().
                if index + 1 < bytes.len() && bytes[index + 1] == b'"' {
                    index += 2;
                } else {
                    index += 1;
                    break;
                }
            } else {
                index += 1;
            }
        }
        // SAFETY: Quote boundaries are ASCII byte positions found by this loop, and start/index never exceed formula.len(); all emitted slices use those boundaries.
        result.push_str(&formula[quoted_start..index]);
        start = index;
    }
    // SAFETY: Quote boundaries are ASCII byte positions found by this loop, and start/index never exceed formula.len(); all emitted slices use those boundaries.
    result.push_str(&translate_unquoted(&formula[start..]));
    result
}

#[cfg(test)]
mod tests {
    use super::translate;
    #[test]
    fn quoted_literals_references_and_array_axes_are_preserved() {
        assert_eq!(
            translate(r#"of:=IF([.A1];"a;b|c";{1;2|3;4})"#),
            r#"IF(A1,"a;b|c",{1,2;3,4})"#
        );
        assert_eq!(
            translate("of:=COM.MICROSOFT.SUMIFS([$'Input data'.$A$1:.$A$5];[.$B$1:.$B$5];1)"),
            "SUMIFS('Input data'!$A$1:$A$5,$B$1:$B$5,1)"
        );
        assert_eq!(
            translate(r#"of:=CONCAT("a"";b";[.$B$2])"#),
            r#"CONCAT("a"";b",$B$2)"#
        );
    }
}
