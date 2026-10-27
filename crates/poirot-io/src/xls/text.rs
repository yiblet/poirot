//! Checked BIFF payload cursor reads and inline Unicode strings.
use crate::ExcelText;
use crate::xls::preflight::PreflightError;
use crate::xls::result::Result;

/// Read a checked byte slice and advance the BIFF payload cursor.
pub(in crate::xls) fn take<'a>(b: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    let v = b.get(..n).ok_or(PreflightError::Malformed(
        crate::BiffStructureError::TruncatedBiffTokenString,
    ))?;
    // SAFETY: The preceding get(..n) verifies n <= b.len() before advancing the same slice.
    *b = &b[n..];
    Ok(v)
}

/// Read a little-endian BIFF word from a checked payload.
pub(in crate::xls) fn word(b: &mut &[u8]) -> Result<u16> {
    let v = take(b, 2)?;
    // SAFETY: The immediately preceding take(b, 2) performs a checked two-byte read before either byte is selected.
    Ok(u16::from_le_bytes([v[0], v[1]]))
}

/// Decode a BIFF string to formula/metadata text.
pub(in crate::xls) fn text(b: &mut &[u8], n: usize) -> Result<String> {
    units(b, n)?.to_utf8().map_err(|_| {
        PreflightError::Representation(
            crate::BiffRepresentationError::BiffSyntaxTextContainsNonScalarUtf16,
        )
    })
}

/// Decode BIFF character storage without repairing UTF-16 units.
pub(in crate::xls) fn units(b: &mut &[u8], n: usize) -> Result<ExcelText> {
    let wide = *take(b, 1)?.first().ok_or(PreflightError::Malformed(
        crate::BiffStructureError::MissingByte,
    ))? & 1
        != 0;
    let bytes = take(b, n * if wide { 2 } else { 1 })?;
    let units: Vec<u16> = if wide {
        // SAFETY: The chunk size is the nonzero literal 2.
        let (pairs, remainder) = bytes.as_chunks::<2>();
        if !remainder.is_empty() {
            return Err(PreflightError::Malformed(
                crate::BiffStructureError::SplitUtf16CodeUnit,
            ));
        }
        pairs.iter().map(|v| u16::from_le_bytes(*v)).collect()
    } else {
        bytes.iter().map(|v| *v as u16).collect()
    };
    Ok(ExcelText::from_units(units))
}
