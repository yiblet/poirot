//! SpreadsheetML escape codec. XML entity decoding precedes this codec.
//! Work with UTF16 units; decoding is one pass, never recursive.
use crate::ExcelText;

fn escape_at(units: &[u16], at: usize) -> Option<u16> {
    let s = units.get(at..at.checked_add(7)?)?;
    // SAFETY: The preceding get selects exactly seven units; indices zero/one/six and the 2..6 subrange fit that slice.
    if s[0] != 95 || s[1] != 120 || s[6] != 95 {
        return None;
    }
    let mut value = 0u16;
    // SAFETY: The preceding get selects exactly seven units; indices zero/one/six and the 2..6 subrange fit that slice.
    for &unit in &s[2..6] {
        let digit = match unit {
            48..=57 => unit - 48,
            65..=70 => unit - 65 + 10,
            97..=102 => unit - 97 + 10,
            _ => return None,
        };
        value = value * 16 + digit;
    }
    Some(value)
}

/// Decode OOXML text escapes while retaining exact Excel UTF-16 units.
pub(super) fn decode(text: &str) -> ExcelText {
    let input: Vec<_> = text.encode_utf16().collect();
    let mut output = Vec::with_capacity(input.len());
    let mut at = 0;
    while at < input.len() {
        if let Some(unit) = escape_at(&input, at) {
            output.push(unit);
            at += 7;
        } else {
            // SAFETY: The enclosing loop checks at < input.len() before this unit access.
            output.push(input[at]);
            at += 1;
        }
    }
    ExcelText::from_units(output)
}

/// Produces legal XML text, before XML entity escaping. Escape literal escape
/// prefixes, disallowed XML units, and lone surrogates; preserve valid pairs.
pub(super) fn encode(text: &ExcelText) -> String {
    use std::fmt::Write;
    let units = text.units();
    let mut output = String::new();
    let mut at = 0;
    while at < units.len() {
        // SAFETY: The loop bounds at; the pair branch checks the next low surrogate and computes U+10000..=U+10FFFF. The BMP branch excludes surrogate code points; formatting numeric units into String cannot return fmt::Error.
        let unit = units[at];
        if (0xd800..=0xdbff).contains(&unit)
            && units
                .get(at + 1)
                .is_some_and(|u| (0xdc00..=0xdfff).contains(u))
        {
            // SAFETY: The loop bounds at; the pair branch checks the next low surrogate and computes U+10000..=U+10FFFF. The BMP branch excludes surrogate code points; formatting numeric units into String cannot return fmt::Error.
            let low = units[at + 1];
            let scalar = 0x10000 + ((unit as u32 - 0xd800) << 10) + low as u32 - 0xdc00;
            // SAFETY: The loop bounds at; the pair branch checks the next low surrogate and computes U+10000..=U+10FFFF. The BMP branch excludes surrogate code points; formatting numeric units into String cannot return fmt::Error.
            output.push(char::from_u32(scalar).expect("surrogate pair scalar"));
            at += 2;
            continue;
        }
        if escape_at(units, at).is_some()
            || unit == 13 // XML line-ending normalization would otherwise erase CR.
            || (0xd800..=0xdfff).contains(&unit)
            || !matches!(unit, 9 | 10 | 13 | 0x20..=0xd7ff | 0xe000..=0xfffd)
        {
            // SAFETY: The loop bounds at; the pair branch checks the next low surrogate and computes U+10000..=U+10FFFF. The BMP branch excludes surrogate code points; formatting numeric units into String cannot return fmt::Error.
            write!(&mut output, "_x{unit:04X}_").expect("String write");
        } else {
            // SAFETY: The loop bounds at; the pair branch checks the next low surrogate and computes U+10000..=U+10FFFF. The BMP branch excludes surrogate code points; formatting numeric units into String cannot return fmt::Error.
            output.push(char::from_u32(unit as u32).expect("BMP scalar"));
        }
        at += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn true_escapes_literal_collisions_and_lone_units_roundtrip() {
        for units in [
            vec![0xd83d, 0xde00],
            vec![0xd83d],
            vec![0xde00],
            vec![0, 9, 10, 13, 0xffff],
            "_xD83D_".encode_utf16().collect(),
            "_x005F_xD83D_".encode_utf16().collect(),
        ] {
            let text = ExcelText::from_units(units);
            assert_eq!(decode(&encode(&text)), text);
        }
        assert_eq!(decode("_xD83D__xDE00_").units(), &[0xd83d, 0xde00]);
        assert_eq!(decode("_x005F_xD83D_").to_utf8().unwrap(), "_xD83D_");
        assert_eq!(encode(&ExcelText::from("_xD83D_")), "_x005F_xD83D_");
        assert_eq!(
            decode("_XD83D_ _xzzzz_").to_utf8().unwrap(),
            "_XD83D_ _xzzzz_"
        );
    }
}
