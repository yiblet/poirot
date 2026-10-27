//! Scalar BIFF8 cell records and non-text formula caches.
use crate::xls::formula::error_text;
use crate::xls::preflight::{PreflightError, Record};
use crate::xls::result::Result;
use crate::xls::text::{take, word};
use crate::{CellAddress, CellValue};

/// Decode a 30-bit RK number (MS-XLS 2.5.217).
fn rk(value: u32) -> f64 {
    let number = if value & 2 != 0 {
        f64::from((value as i32) >> 2)
    } else {
        f64::from_bits(u64::from(value & 0xffff_fffc) << 32)
    };
    if value & 1 != 0 {
        number / 100.
    } else {
        number
    }
}

/// Read a little-endian 32-bit integer from a checked payload offset.
fn dword(data: &[u8], at: usize) -> Result<u32> {
    let bytes = data
        .get(
            at..at.checked_add(4).ok_or(PreflightError::Malformed(
                crate::BiffStructureError::IntegerOffsetOverflow,
            ))?,
        )
        .ok_or(PreflightError::Malformed(
            crate::BiffStructureError::TruncatedCellRecord,
        ))?;
    // SAFETY: get above selects exactly four bytes or returns an error, so [u8; 4] conversion succeeds.
    Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
}

/// Read a little-endian IEEE double from a checked payload offset.
fn double(data: &[u8], at: usize) -> Result<f64> {
    let bytes = data
        .get(
            at..at.checked_add(8).ok_or(PreflightError::Malformed(
                crate::BiffStructureError::IntegerOffsetOverflow,
            ))?,
        )
        .ok_or(PreflightError::Malformed(
            crate::BiffStructureError::TruncatedCellRecord,
        ))?;
    // SAFETY: get above selects exactly eight bytes or returns an error, so [u8; 8] conversion succeeds.
    Ok(f64::from_le_bytes(bytes.try_into().expect("eight bytes")))
}

/// Convert a BIFF8 BErr code to a typed cell error.
fn error_value(code: u8) -> Result<CellValue> {
    error_text(code)?
        .parse()
        .map(CellValue::Error)
        .map_err(|_| PreflightError::Representation(crate::BiffRepresentationError::BiffErrorCode))
}

/// Decode non-text scalar cell records and non-string formula caches (MS-XLS 2.4).
/// Text cells and cached formula strings are decoded separately as exact UTF-16.
pub(in crate::xls) fn scalar_values(r: Record<'_>) -> Result<Vec<(CellAddress, CellValue)>> {
    let mut b = r.data;
    if !matches!(r.id, 0x203 | 0x27e | 0xbd | 0x205 | 0x6) {
        return Ok(Vec::new());
    }
    let row = u32::from(word(&mut b)?);
    let col = word(&mut b)?;
    let address = |col: u16| {
        CellAddress::new(row, u32::from(col)).map_err(|_| {
            PreflightError::Representation(crate::BiffRepresentationError::CellCoordinate)
        })
    };
    Ok(match r.id {
        0x203 => vec![(address(col)?, CellValue::Number(double(r.data, 6)?))],
        0x27e => vec![(address(col)?, CellValue::Number(rk(dword(r.data, 6)?)))],
        0xbd => {
            // Rgrkrec: (ixfe, RK) pairs after row and first column, then colLast.
            let count = r.data.len().saturating_sub(6) / 6;
            let pairs = r.data.get(4..).ok_or(PreflightError::Malformed(
                crate::BiffStructureError::TruncatedCellRecord,
            ))?;
            let mut cells = Vec::with_capacity(count);
            // SAFETY: the chunk size is the nonzero literal 6.
            let (pairs, _) = pairs.as_chunks::<6>();
            for (k, pair) in pairs.iter().take(count).enumerate() {
                let col = col
                    .checked_add(u16::try_from(k).map_err(|_| {
                        PreflightError::Malformed(crate::BiffStructureError::MulrkLength)
                    })?)
                    .ok_or(PreflightError::Malformed(
                        crate::BiffStructureError::MulrkCoordinatesLength,
                    ))?;
                cells.push((address(col)?, CellValue::Number(rk(dword(pair, 2)?))));
            }
            cells
        }
        0x205 => {
            take(&mut b, 2)?;
            let value = take(&mut b, 2)?;
            // SAFETY: take(&mut b, 2) above returned exactly two bytes, so indexes 0 and 1 exist.
            let (value, is_error) = (value[0], value[1] != 0);
            vec![(
                address(col)?,
                if is_error {
                    error_value(value)?
                } else {
                    CellValue::Boolean(value != 0)
                },
            )]
        }
        _ => {
            // FormulaValue (MS-XLS 2.5.133): fExprO 0xFFFF marks a typed non-number.
            let result = r.data.get(6..14).ok_or(PreflightError::Malformed(
                crate::BiffStructureError::TruncatedCellRecord,
            ))?;
            // SAFETY: get(6..14) above returned exactly eight bytes, so indexes 0..=7 exist.
            if result[6..8] != [0xff, 0xff] {
                vec![(address(col)?, CellValue::Number(double(r.data, 6)?))]
            } else {
                // SAFETY: result has exactly eight bytes, so indexes 0 and 2 exist.
                match (result[0], result[2]) {
                    (0, _) => Vec::new(),
                    (1, value) => vec![(address(col)?, CellValue::Boolean(value != 0))],
                    (2, code) => vec![(address(col)?, error_value(code)?)],
                    (3, _) => vec![(address(col)?, CellValue::Text(String::new()))],
                    _ => {
                        return Err(PreflightError::Malformed(
                            crate::BiffStructureError::FormulaCachedResultType,
                        ));
                    }
                }
            }
        }
    })
}
