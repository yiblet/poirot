use crate::lookup::operands::{charge, checked_product};
use crate::lookup::{DynamicReference, Error, Failure, Work, Workbook};
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
) -> Result<DynamicReference, Failure> {
    charge(w, Work::SearchText, checked_product(text.len(), 4)?)?;
    parse_qualifiers(text, a1)
}
/// Budget-free qualifier grammar; callers admit storage before allocation.
fn parse_qualifiers(text: &str, a1: bool) -> Result<DynamicReference, Failure> {
    let (prefix, reference) = match text.rsplit_once('!') {
        Some((p, r)) => (Some(p), r),
        None => (None, text),
    };
    let mut workbook = None;
    let mut sheet = None;
    if let Some(prefix) = prefix {
        if prefix.is_empty() || prefix.trim() != prefix {
            return Err(Error::Ref.into());
        }
        let quoted = prefix.starts_with('\'');
        let body = if quoted {
            if prefix.len() < 2 || !prefix.ends_with('\'') {
                return Err(Error::Ref.into());
            }
            // SAFETY: The quoted branch checks two ASCII quote endpoints and length >= 2, making both interior boundaries valid.
            &prefix[1..prefix.len() - 1]
        } else {
            prefix
        };
        if body.chars().next().is_some_and(char::is_whitespace) {
            return Err(Error::Ref.into());
        }
        let (wb, sh) = if let Some(rest) = body.strip_prefix('[') {
            let (wb, sh) = rest
                .rsplit_once(']')
                .ok_or(Failure::Spreadsheet(Error::Ref))?;
            if wb.trim() != wb {
                return Err(Error::Ref.into());
            }
            (Some(wb), sh)
        } else {
            (None, body)
        };
        let decode = |s: &str| -> Result<String, Failure> {
            if !quoted {
                return Ok(s.into());
            }
            let mut chars = s.chars();
            let mut out = String::new();
            while let Some(ch) = chars.next() {
                if ch == '\'' && chars.next() != Some('\'') {
                    return Err(Error::Ref.into());
                }
                out.push(ch);
            }
            Ok(out)
        };
        if !quoted && sh.trim() != sh {
            return Err(Error::Ref.into());
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
