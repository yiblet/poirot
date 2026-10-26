use crate::operands::{charge, coercev2_lookup_boolean, coercev2_lookup_text, scalar, Arguments};
use crate::{parse_dynamic_reference, Context, Error, Failure, Unsupported, Value, Work, Workbook};
fn indirect_failure(failure: Failure) -> Failure {
    match failure {
        Failure::Spreadsheet(_) => Error::Ref.into(),
        other => other,
    }
}
/// Absolute axes used when spelling an ADDRESS reference.
#[derive(Clone, Copy)]
pub enum AddressAxes {
    /// Both axes are absolute.
    Both,
    /// Only the row is absolute.
    Row,
    /// Only the column is absolute.
    Column,
    /// Both axes are relative.
    Relative,
}
/// Coordinate notation used by ADDRESS.
#[derive(Clone, Copy)]
pub enum AddressStyle {
    /// Alphabetic column followed by numeric row.
    A1,
    /// Explicit numeric row and column components.
    R1C1,
}
/// Format admitted ADDRESS coordinates without workbook access or argument evaluation.
pub fn address_text(
    row: u32,
    col: u32,
    axes: AddressAxes,
    style: AddressStyle,
    sheet: Option<&str>,
) -> Result<String, Failure> {
    if !(1..=1_048_576).contains(&row) || !(1..=16_384).contains(&col) {
        return Err(Error::Value.into());
    }
    let absolute_row = matches!(axes, AddressAxes::Both | AddressAxes::Row);
    let absolute_column = matches!(axes, AddressAxes::Both | AddressAxes::Column);
    let capacity = sheet
        .map_or(Some(32), |name| {
            name.len().checked_mul(4).and_then(|n| n.checked_add(32))
        })
        .ok_or(Failure::Operational(
            crate::OperationalFailure::ResourceOverflow,
        ))?;
    let mut result = String::new();
    result
        .try_reserve_exact(capacity)
        .map_err(|_| Failure::Operational(crate::OperationalFailure::HostUnavailable))?;
    if let Some(name) = sheet {
        if name.contains(['\n', '\r', '\t']) {
            return Err(Failure::Unsupported(
                Unsupported::AddressSheetNameControlCharacter,
            ));
        }
        let quote = crate::syntax::sheet_name_requires_quote(name);
        // SAFETY: capacity reserves four times the sheet bytes plus 32, enough for doubled apostrophes, quotes and address text.
        if quote {
            result.push('\'');
        }
        for character in name.chars() {
            // SAFETY: Every character and doubled apostrophe fits the locally reserved four-times input bound.
            result.push(character);
            if quote && character == '\'' {
                // SAFETY: Each apostrophe occupies at most two reserved bytes.
                result.push('\'');
            }
        }
        // SAFETY: capacity includes both enclosing quotes and the address suffix.
        if quote {
            result.push('\'');
        }
        // SAFETY: the reservation above includes the sheet separator.
        result.push('!');
    }
    use std::fmt::Write;
    if matches!(style, AddressStyle::R1C1) {
        // SAFETY: The local reservation includes 32 bytes for at most two u32 spellings, brackets and axis labels.
        write!(
            result,
            "R{}{}{}C{}{}{}",
            if !absolute_row { "[" } else { "" },
            row,
            if !absolute_row { "]" } else { "" },
            if !absolute_column { "[" } else { "" },
            col,
            if !absolute_column { "]" } else { "" }
        )
        .map_err(|_| Failure::Operational(crate::OperationalFailure::HostUnavailable))?;
    } else {
        let mut letters = [0u8; 7];
        let mut position = letters.len();
        let mut column = col;
        while column > 0 {
            // SAFETY: The loop condition proves the column is positive before subtracting one.
            column -= 1;
            position = position.checked_sub(1).ok_or(Error::Value)?;
            // SAFETY: The remainder is at most 25, so adding ASCII A cannot exceed 90.
            *letters.get_mut(position).ok_or(Error::Value)? = b'A' + (column % 26) as u8;
            column /= 26;
        }
        let letters = std::str::from_utf8(letters.get(position..).ok_or(Error::Value)?)
            .map_err(|_| Error::Value)?;
        // SAFETY: The local reservation includes 32 bytes for at most two u32 spellings, brackets and axis labels.
        write!(
            result,
            "{}{}{}{}",
            if absolute_column { "$" } else { "" },
            letters,
            if absolute_row { "$" } else { "" },
            row
        )
        .map_err(|_| Failure::Operational(crate::OperationalFailure::HostUnavailable))?;
    }
    Ok(result)
}

/// Parse and resolve a computed reference through the host.
pub(super) fn indirect(
    args: &Arguments<'_>,
    c: Context,
    w: &mut impl Workbook,
) -> Result<Value, Failure> {
    if args.is_empty() {
        return Err(Error::Value.into());
    }
    let text = coercev2_lookup_text(&scalar(&args.get(0, c, w)?, c, w)?, w)?;
    let a1 = match args.len() {
        1 => true,
        2 => coercev2_lookup_boolean(scalar(&args.get(1, c, w)?, c, w)?)?,
        _ => return Err(Error::Value.into()),
    };
    let query = parse_dynamic_reference(&text, a1, w)?;
    charge(w, Work::QueryReturn, 1)?;
    // Structured reference syntax is resolved by the workbook parser, with its table metadata.
    if a1 && query.first.contains('[') && query.first.ends_with(']') {
        w.structured_reference(text.rsplit_once('!').map_or(text.as_str(), |(_, r)| r), c)
            .map_err(indirect_failure)
    } else {
        w.dynamic_reference(&query, c).map_err(indirect_failure)
    }
}

/// Select admitted INDEX coordinates without changing the source representation.
/// Zero retains a complete dimension; vector form requires one singleton axis.
pub fn index_selection(
    vector: bool,
    height: usize,
    width: usize,
    row: usize,
    column: usize,
) -> Option<(usize, usize)> {
    let (row, column) = if vector {
        if width == 1 {
            (row, 0)
        } else if height == 1 {
            (0, row)
        } else {
            return None;
        }
    } else {
        (row, column)
    };
    (height > 0 && width > 0 && row <= height && column <= width).then_some((row, column))
}

#[cfg(test)]
mod index_tests {
    use super::index_selection;
    #[test]
    fn index_selection_retains_dimensions_and_rejects_empty_or_exhausted_sources() {
        assert_eq!(index_selection(false, 2, 3, 0, 0), Some((0, 0)));
        assert_eq!(index_selection(false, 2, 3, 2, 0), Some((2, 0)));
        assert_eq!(index_selection(false, 2, 3, 0, 4), None);
        assert_eq!(index_selection(false, 0, 3, 0, 1), None);
        assert_eq!(index_selection(true, 2, 3, 1, 1), None);
        assert_eq!(index_selection(true, 1, 3, 2, 1), Some((0, 2)));
        assert_eq!(index_selection(true, 3, 1, 2, 1), Some((2, 0)));
    }
}
