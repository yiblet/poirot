//! Original UTF-16 algorithms for the Microsoft modern text contracts.
//! <https://support.microsoft.com/en-us/excel/functions/textsplit-function>
//! <https://support.microsoft.com/en-us/excel/functions/textbefore-function>
//! <https://support.microsoft.com/en-us/excel/functions/textafter-function>
//! <https://support.microsoft.com/en-us/excel/functions/arraytotext-function>
//! <https://support.microsoft.com/en-us/excel/functions/valuetotext-function>
use super::modern_text_error::ModernTextError;
use std::ops::Range;

/// Check Rust vector byte-capacity representation before a fallible reservation.
fn capacity<T>(count: usize) -> Result<(), ModernTextError> {
    if count
        .checked_mul(std::mem::size_of::<T>())
        .is_some_and(|bytes| bytes <= isize::MAX as usize)
    {
        Ok(())
    } else {
        Err(ModernTextError::SizeOverflow)
    }
}
/// Whether a literal delimiter comparison distinguishes letter case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    /// Compare exact UTF-16 units.
    Sensitive,
    /// Compare Unicode lowercase scalars and preserve isolated units.
    Insensitive,
}
/// Which side of the selected delimiter is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// Return the prefix preceding the delimiter.
    Before,
    /// Return the suffix following the delimiter.
    After,
}
fn equal(a: &[u16], b: &[u16], case: Case) -> bool {
    if case == Case::Sensitive {
        return a == b;
    }
    let mut a = char::decode_utf16(a.iter().copied());
    let mut b = char::decode_utf16(b.iter().copied());
    loop {
        match (a.next(), b.next()) {
            (None, None) => return true,
            (Some(Ok(a)), Some(Ok(b))) if a.to_lowercase().eq(b.to_lowercase()) => {}
            (Some(Err(a)), Some(Err(b))) if a.unpaired_surrogate() == b.unpaired_surrogate() => {}
            _ => return false,
        }
    }
}
fn at(text: &[u16], offset: usize, delimiter: &[u16], case: Case) -> bool {
    offset
        .checked_add(delimiter.len())
        .and_then(|end| text.get(offset..end))
        .is_some_and(|candidate| equal(candidate, delimiter, case))
}
/// Select a prefix or suffix without copying the input or allocating match lists.
/// The excessive-occurrence bound counts UTF-16 units; Excel Version 2 behavior
/// for this particular bound has not yet been independently verified.
pub fn extract(
    text: &[u16],
    delimiter: &[u16],
    occurrence: i64,
    case: Case,
    match_end: bool,
    side: Side,
) -> Result<Range<usize>, ModernTextError> {
    if occurrence == 0 {
        return Err(ModernTextError::Value);
    }
    if text.is_empty() {
        return Ok(0..0);
    }
    if occurrence.unsigned_abs() > text.len() as u64 {
        return Err(ModernTextError::Value);
    }
    let negative = occurrence < 0;
    if delimiter.is_empty() {
        let boundary = if negative { text.len() } else { 0 };
        return Ok(match side {
            Side::Before => 0..boundary,
            Side::After => boundary..text.len(),
        });
    }
    let mut remaining = occurrence.unsigned_abs();
    let mut found = None;
    if negative {
        let mut end = text.len();
        while let Some(start) = end.checked_sub(delimiter.len()) {
            if at(text, start, delimiter, case) {
                remaining = remaining.checked_sub(1).ok_or(ModernTextError::Value)?;
                if remaining == 0 {
                    found = Some(start..end);
                    break;
                }
                end = start;
            } else if end > 0 {
                end = end.checked_sub(1).ok_or(ModernTextError::Value)?;
            } else {
                break;
            }
        }
    } else {
        let mut start = 0;
        while start <= text.len().saturating_sub(delimiter.len()) && delimiter.len() <= text.len() {
            if at(text, start, delimiter, case) {
                remaining = remaining.checked_sub(1).ok_or(ModernTextError::Value)?;
                if remaining == 0 {
                    found = Some(
                        start
                            ..start
                                .checked_add(delimiter.len())
                                .ok_or(ModernTextError::SizeOverflow)?,
                    );
                    break;
                }
                start = start
                    .checked_add(delimiter.len())
                    .ok_or(ModernTextError::SizeOverflow)?;
            } else {
                start = start.checked_add(1).ok_or(ModernTextError::SizeOverflow)?;
            }
        }
    }
    if found.is_none() && match_end && remaining == 1 {
        let boundary = if negative { 0 } else { text.len() };
        found = Some(boundary..boundary);
    }
    let found = found.ok_or(ModernTextError::NotFound)?;
    Ok(match side {
        Side::Before => 0..found.start,
        Side::After => found.end..text.len(),
    })
}
/// TEXTSPLIT delimiter and empty-field rules reused for planning and extraction.
/// Running both scans with the same rules keeps reserved geometry aligned with the emitted fields.
pub struct Split<'a> {
    /// Column separators, in matching precedence order.
    pub columns: &'a [Vec<u16>],
    /// Row separators, in matching precedence order.
    pub rows: &'a [Vec<u16>],
    /// Suppress zero-length fields and rows containing no retained fields.
    pub ignore_empty: bool,
    /// Literal comparison policy.
    pub case: Case,
}
/// TEXTSPLIT output dimensions and retained-field count computed before allocation.
/// The widest retained row determines padding, while the field count sizes the span buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    /// Number of retained rows.
    pub rows: usize,
    /// Width of the widest retained row.
    pub columns: usize,
    /// Number of non-padding members.
    pub fields: usize,
}
/// A TEXTSPLIT field identified by its input UTF-16 span and output coordinate.
/// Extraction can copy the original units without reparsing delimiters or storing text in the plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// Result row.
    pub row: usize,
    /// Result column.
    pub column: usize,
    /// Lossless interval into the original text.
    pub span: Range<usize>,
}
impl Split<'_> {
    fn scan(
        &self,
        text: &[u16],
        mut visit: impl FnMut(Field) -> Result<(), ModernTextError>,
    ) -> Result<Shape, ModernTextError> {
        if self.columns.iter().chain(self.rows).all(Vec::is_empty) {
            return Err(ModernTextError::Value);
        }
        let mut shape = Shape {
            rows: 0,
            columns: 0,
            fields: 0,
        };
        let mut offset = 0;
        let mut start = 0;
        let mut column = 0;
        loop {
            let row_separator = self
                .rows
                .iter()
                .find(|d| !d.is_empty() && at(text, offset, d, self.case));
            let col_separator = if row_separator.is_none() {
                self.columns
                    .iter()
                    .find(|d| !d.is_empty() && at(text, offset, d, self.case))
            } else {
                None
            };
            let end = offset == text.len();
            if end || row_separator.is_some() || col_separator.is_some() {
                if !self.ignore_empty || offset > start {
                    visit(Field {
                        row: shape.rows,
                        column,
                        span: start..offset,
                    })?;
                    column = column.checked_add(1).ok_or(ModernTextError::SizeOverflow)?;
                    shape.fields = shape
                        .fields
                        .checked_add(1)
                        .ok_or(ModernTextError::SizeOverflow)?;
                }
                if (end || row_separator.is_some()) && column > 0 {
                    shape.rows = shape
                        .rows
                        .checked_add(1)
                        .ok_or(ModernTextError::SizeOverflow)?;
                    shape.columns = shape.columns.max(column);
                    column = 0;
                }
                if end {
                    break;
                }
                let width = row_separator.or(col_separator).map_or(0, Vec::len);
                offset = offset
                    .checked_add(width)
                    .ok_or(ModernTextError::SizeOverflow)?;
                start = offset;
            } else {
                offset = offset.checked_add(1).ok_or(ModernTextError::SizeOverflow)?;
            }
        }
        if shape.fields == 0 {
            Err(ModernTextError::Empty)
        } else {
            Ok(shape)
        }
    }
    /// Inspect output geometry without allocating spans or text.
    pub fn shape(&self, text: &[u16]) -> Result<Shape, ModernTextError> {
        self.scan(text, |_| Ok(()))
    }
    /// Materialize admitted spans; the caller checks rectangular output bounds first.
    pub fn fields(&self, text: &[u16], expected: Shape) -> Result<Vec<Field>, ModernTextError> {
        capacity::<Field>(expected.fields)?;
        let mut fields = Vec::new();
        fields
            .try_reserve_exact(expected.fields)
            .map_err(|_| ModernTextError::AllocationFailure)?;
        let actual = self.scan(text, |field| {
            if fields.len() >= expected.fields {
                return Err(ModernTextError::InvalidGeometry);
            }
            // SAFETY: The local bound check limits pushes to the successfully reserved expected.fields slots.
            fields.push(field);
            Ok(())
        })?;
        if actual != expected {
            return Err(ModernTextError::InvalidGeometry);
        }
        Ok(fields)
    }
    /// Bound comparisons in each scan before the caller admits work.
    pub fn work(&self, length: usize) -> Result<usize, ModernTextError> {
        let widths = self
            .columns
            .iter()
            .chain(self.rows)
            .try_fold(0usize, |sum, d| {
                sum.checked_add(d.len().max(1))
                    .ok_or(ModernTextError::SizeOverflow)
            })?;
        length
            .checked_add(1)
            .and_then(|n| n.checked_mul(widths.max(1)))
            .ok_or(ModernTextError::SizeOverflow)
    }
}
/// Calculate storage needed to escape one already-rendered scalar.
pub fn escaped_len(text: &[u16], quoted: bool) -> Result<usize, ModernTextError> {
    let extra = if quoted {
        text.iter()
            .filter(|u| **u == 34)
            .count()
            .checked_add(2)
            .ok_or(ModernTextError::SizeOverflow)?
    } else {
        0
    };
    text.len()
        .checked_add(extra)
        .ok_or(ModernTextError::SizeOverflow)
}
/// Escape one admitted scalar, doubling internal quotation marks in strict text.
pub fn escape(text: &[u16], quoted: bool) -> Result<Vec<u16>, ModernTextError> {
    let quotes = if quoted {
        text.iter().filter(|unit| **unit == 34).count()
    } else {
        0
    };
    let length = text
        .len()
        .checked_add(quotes)
        .and_then(|n| n.checked_add(if quoted { 2 } else { 0 }))
        .ok_or(ModernTextError::SizeOverflow)?;
    capacity::<u16>(length)?;
    let mut out = Vec::new();
    out.try_reserve_exact(length)
        .map_err(|_| ModernTextError::AllocationFailure)?;
    // SAFETY: The checked local length and reservation include both enclosing quotes and every doubled input quote.
    if quoted {
        out.push(34);
    }
    for &unit in text {
        // SAFETY: Exactly text.len() input units plus the counted quote duplicates fit the local reserved length.
        out.push(unit);
        // SAFETY: The local quotes count reserves one extra slot for each such input unit.
        if quoted && unit == 34 {
            out.push(34);
        }
    }
    // SAFETY: The checked local length and reservation include both enclosing quotes and every doubled input quote.
    if quoted {
        out.push(34);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }
    #[test]
    fn signed_selection_boundaries_and_domain_errors() {
        let text = units("oak::elm::ash");
        let delimiter = units("::");
        assert_eq!(
            extract(&text, &delimiter, 2, Case::Sensitive, false, Side::Before),
            Ok(0..8)
        );
        assert_eq!(
            extract(&text, &delimiter, -1, Case::Sensitive, false, Side::After),
            Ok(10..13)
        );
        assert_eq!(
            extract(&text, &delimiter, -2, Case::Sensitive, false, Side::Before),
            Ok(0..3)
        );
        assert_eq!(
            extract(&text, &delimiter, 3, Case::Sensitive, true, Side::Before),
            Ok(0..13)
        );
        assert_eq!(
            extract(&text, &delimiter, -3, Case::Sensitive, true, Side::After),
            Ok(0..13)
        );
        assert_eq!(
            extract(&text, &delimiter, 3, Case::Sensitive, false, Side::After),
            Err(ModernTextError::NotFound)
        );
        assert_eq!(
            extract(&text, &delimiter, 0, Case::Sensitive, false, Side::Before),
            Err(ModernTextError::Value)
        );
        assert_eq!(
            extract(&text, &delimiter, 14, Case::Sensitive, false, Side::Before),
            Err(ModernTextError::Value)
        );
        assert_eq!(
            extract(&text, &[], 1, Case::Sensitive, false, Side::After),
            Ok(0..13)
        );
        assert_eq!(
            extract(&text, &[], -1, Case::Sensitive, false, Side::Before),
            Ok(0..13)
        );
    }
    #[test]
    fn case_matching_preserves_unicode_and_unpaired_units() {
        let text = vec![0xd800, 0x61, 0x3a, 0xdc00];
        assert_eq!(
            extract(
                &text,
                &units("A:"),
                1,
                Case::Insensitive,
                false,
                Side::After
            ),
            Ok(3..4)
        );
        assert_eq!(
            extract(
                &units("Ä:Ω"),
                &units("ä:"),
                1,
                Case::Insensitive,
                false,
                Side::After
            ),
            Ok(2..3)
        );
        assert_eq!(
            extract(&text, &[0xd800], 1, Case::Sensitive, false, Side::After),
            Ok(1..4)
        );
    }
    #[test]
    fn split_delimiter_sets_rows_empty_fields_and_geometry() {
        let cols = vec![units(","), units("|")];
        let rows = vec![units(";")];
        let plan = Split {
            columns: &cols,
            rows: &rows,
            ignore_empty: false,
            case: Case::Sensitive,
        };
        let text = units("a,,b;c|d;");
        let shape = plan.shape(&text).unwrap();
        assert_eq!(
            shape,
            Shape {
                rows: 3,
                columns: 3,
                fields: 6
            }
        );
        let fields = plan.fields(&text, shape).unwrap();
        assert_eq!(
            fields
                .iter()
                .map(|f| (f.row, f.column, f.span.clone()))
                .collect::<Vec<_>>(),
            vec![
                (0, 0, 0..1),
                (0, 1, 2..2),
                (0, 2, 3..4),
                (1, 0, 5..6),
                (1, 1, 7..8),
                (2, 0, 9..9)
            ]
        );
        let plan = Split {
            ignore_empty: true,
            ..plan
        };
        assert_eq!(
            plan.shape(&text).unwrap(),
            Shape {
                rows: 2,
                columns: 2,
                fields: 4
            }
        );
        assert_eq!(plan.shape(&units(",;|")), Err(ModernTextError::Empty));
    }
    #[test]
    fn split_case_and_original_intervals_do_not_normalize_input() {
        let cols = vec![units("Ä")];
        let plan = Split {
            columns: &cols,
            rows: &[],
            ignore_empty: false,
            case: Case::Insensitive,
        };
        let text = vec![0xd800, 0xe4, 0xdc00];
        let shape = plan.shape(&text).unwrap();
        assert_eq!(
            shape,
            Shape {
                rows: 1,
                columns: 2,
                fields: 2
            }
        );
        let fields = plan.fields(&text, shape).unwrap();
        assert_eq!(fields[0].span, 0..1);
        assert_eq!(fields[1].span, 2..3);
        assert_eq!(
            plan.fields(&text, Shape { fields: 1, ..shape }),
            Err(ModernTextError::InvalidGeometry)
        );
        assert_eq!(
            plan.fields(
                &text,
                Shape {
                    fields: usize::MAX,
                    ..shape
                }
            ),
            Err(ModernTextError::SizeOverflow)
        );
    }
    #[test]
    fn strict_serialization_only_escapes_quotation_units() {
        let text = [0xd800, 34, 10, 0xdc00];
        assert_eq!(escaped_len(&text, true), Ok(7));
        assert_eq!(
            escape(&text, true).unwrap(),
            vec![34, 0xd800, 34, 34, 10, 0xdc00, 34]
        );
        assert_eq!(escape(&text, false).unwrap(), text);
    }
}
