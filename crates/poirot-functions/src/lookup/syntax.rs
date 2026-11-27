use crate::lookup::operands::{charge, checked_product};
use crate::lookup::{DynamicReference, LookupError, LookupValueError, Work, Workbook};
/// Whether a sheet spelling needs quotes to avoid reference ambiguity.
pub(super) fn sheet_name_requires_quote(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_digit())
        || name
            .chars()
            .any(|c| !(c.is_alphanumeric() || c == '_' || c == '.'))
        || is_a1_cell(name)
        || name.eq_ignore_ascii_case("TRUE")
        || name.eq_ignore_ascii_case("FALSE")
        || starts_r1c1(name)
}
fn starts_r1c1(name: &str) -> bool {
    let mut chars = name.chars().map(|c| c.to_ascii_uppercase());
    match (chars.next(), chars.next(), chars.next()) {
        (Some('R' | 'C'), None, _) | (Some('R'), Some('C'), None) => true,
        (Some('R'), Some('C'), Some(third)) => third.is_ascii_digit(),
        (Some('R' | 'C'), Some(second), _) => second.is_ascii_digit(),
        _ => false,
    }
}
fn is_a1_cell(name: &str) -> bool {
    let letters = name.bytes().take_while(u8::is_ascii_alphabetic).count();
    // SAFETY: letters counts an initial ASCII byte run, so it is a valid UTF-8 boundary within name.
    if letters == 0 || letters == name.len() || !name[letters..].bytes().all(|b| b.is_ascii_digit())
    {
        return false;
    }
    let mut col = 0u64;
    // SAFETY: letters counts an initial ASCII byte run, so it is a valid UTF-8 boundary within name.
    for b in name[..letters].bytes() {
        col = col
            .saturating_mul(26)
            .saturating_add((b.to_ascii_uppercase() - b'A' + 1) as u64);
    }
    col <= 16_384
        // SAFETY: letters counts an initial ASCII byte run, so it is a valid UTF-8 boundary within name.
        && name[letters..]
            .parse::<u32>()
            .is_ok_and(|r| (1..=1_048_576).contains(&r))
}
/// Decode supported A1/R1C1 reference text without reading workbook cells.
pub fn parse_dynamic_reference(
    text: &str,
    a1: bool,
    w: &mut impl Workbook,
) -> Result<DynamicReference, LookupError> {
    admit_dynamic_reference(text, w)?;
    let (prefix, reference) = split_qualifier(text);
    parse_qualifiers(prefix, reference, a1)
}

/// Admit the full text before scanning it or allocating parsed reference fields.
pub(super) fn admit_dynamic_reference(
    text: &str,
    w: &mut impl Workbook,
) -> Result<(), LookupError> {
    charge(w, Work::SearchText, checked_product(text.len(), 4)?)
}

/// Split on the last sheet separator outside workbook and table brackets.
pub(super) fn split_qualifier(text: &str) -> (Option<&str>, &str) {
    if !text.contains('[') {
        return text
            .rsplit_once('!')
            .map_or((None, text), |(prefix, reference)| {
                (Some(prefix), reference)
            });
    }
    let mut depth = 0usize;
    let mut separator = None;
    let mut characters = text.char_indices().peekable();
    while let Some((index, character)) = characters.next() {
        match character {
            '[' => depth = depth.saturating_add(1),
            ']' => depth = depth.saturating_sub(1),
            '\'' if depth != 0 => {
                characters.next_if(|(_, next)| matches!(next, '[' | ']' | '#' | '\'' | '@'));
            }
            '!' if depth == 0 => separator = Some(index),
            _ => {}
        }
    }
    separator
        .and_then(|index| text.split_at_checked(index))
        .and_then(|(prefix, suffix)| Some((Some(prefix), suffix.strip_prefix('!')?)))
        .unwrap_or((None, text))
}

/// Decode previously admitted qualifier and reference text.
pub(super) fn parse_qualifiers(
    prefix: Option<&str>,
    reference: &str,
    a1: bool,
) -> Result<DynamicReference, LookupError> {
    let mut workbook = None;
    let mut sheet = None;
    if let Some(prefix) = prefix {
        if prefix.is_empty() || prefix.trim() != prefix {
            return Err(LookupValueError::Ref.into());
        }
        let quoted = prefix.starts_with('\'');
        let body = if quoted {
            if prefix.len() < 2 || !prefix.ends_with('\'') {
                return Err(LookupValueError::Ref.into());
            }
            // SAFETY: The quoted branch checks two ASCII quote endpoints and length >= 2, making both interior boundaries valid.
            &prefix[1..prefix.len() - 1]
        } else {
            prefix
        };
        if body.chars().next().is_some_and(char::is_whitespace) {
            return Err(LookupValueError::Ref.into());
        }
        let (wb, sh) = if let Some(rest) = body.strip_prefix('[') {
            let (wb, sh) = rest
                .rsplit_once(']')
                .ok_or(LookupError::Spreadsheet(LookupValueError::Ref))?;
            if wb.trim() != wb {
                return Err(LookupValueError::Ref.into());
            }
            (Some(wb), sh)
        } else {
            (None, body)
        };
        let decode = |s: &str| -> Result<String, LookupError> {
            if !quoted {
                return Ok(s.into());
            }
            let mut chars = s.chars();
            let mut out = String::new();
            while let Some(ch) = chars.next() {
                if ch == '\'' && chars.next() != Some('\'') {
                    return Err(LookupValueError::Ref.into());
                }
                out.push(ch);
            }
            Ok(out)
        };
        if !quoted && sh.trim() != sh {
            return Err(LookupValueError::Ref.into());
        }
        workbook = wb.map(decode).transpose()?;
        sheet = Some(decode(sh)?);
    }
    let (first, last) = match reference.split_once(':') {
        Some((f, l)) => (f.trim().into(), Some(l.trim().into())),
        None => (reference.trim().into(), None),
    };
    Ok(DynamicReference {
        workbook,
        sheet,
        first,
        last,
        a1,
    })
}
