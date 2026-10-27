//! Stateful STRING/CONTINUE decoding of cached formula strings.
use crate::ExcelText;
use crate::xls::preflight::{PreflightError, Record, ResourceBudget};
use crate::xls::result::Result;
use crate::xls::text::{take, word};

/// Reconstructed cached formula string and the last continuation record it consumed.
pub(in crate::xls) struct CachedText {
    /// Exact UTF-16 cached formula result.
    pub(in crate::xls) text: ExcelText,
    /// Index of the final consumed STRING/CONTINUE record.
    pub(in crate::xls) last_record: usize,
}

/// Join a cached formula string across bounded continuation records.
pub(in crate::xls) fn cached_text(
    rs: &[Record<'_>],
    mut i: usize,
    budget: ResourceBudget,
) -> Result<CachedText> {
    let mut b = rs
        .get(i)
        .ok_or(PreflightError::Malformed("missing cached STRING"))?
        .data;
    let n = word(&mut b)? as usize;
    if n > budget.max_string_chars {
        return Err(PreflightError::Resource {
            resource: "cached string characters",
            requested: n as u64,
            limit: budget.max_string_chars as u64,
        });
    }
    let flags = *take(&mut b, 1)?
        .first()
        .ok_or(PreflightError::Malformed("missing byte"))?;
    if flags & 0x0c != 0 {
        return Err(PreflightError::Representation(
            "rich/extended cached STRING records are not supported",
        ));
    }
    let mut wide = flags & 1 != 0;
    let mut units = Vec::with_capacity(n);
    let mut continuations = 0;
    while units.len() < n {
        if b.is_empty() {
            i += 1;
            continuations += 1;
            if continuations > budget.max_continuations {
                return Err(PreflightError::Resource {
                    resource: "cached STRING continuations",
                    requested: continuations as u64,
                    limit: budget.max_continuations as u64,
                });
            }
            let next = rs
                .get(i)
                .filter(|r| r.id == 0x3c)
                .ok_or(PreflightError::Malformed(
                    "missing cached STRING continuation",
                ))?;
            b = next.data;
            wide = *take(&mut b, 1)?
                .first()
                .ok_or(PreflightError::Malformed("missing byte"))?
                & 1
                != 0;
        }
        units.push(if wide {
            word(&mut b)?
        } else {
            *take(&mut b, 1)?
                .first()
                .ok_or(PreflightError::Malformed("missing byte"))? as u16
        });
    }
    let text = ExcelText::from_units(units);
    Ok(CachedText {
        text,
        last_record: i,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumed_index_stops_before_the_next_string() {
        let rs = [
            Record {
                id: 0x207,
                at: 0,
                data: &[2, 0, 0, b'A'],
            },
            Record {
                id: 0x3c,
                at: 8,
                data: &[1, b'B', 0],
            },
            Record {
                id: 0x207,
                at: 15,
                data: &[1, 0, 0, b'C'],
            },
        ];
        let cached = cached_text(&rs, 0, ResourceBudget::default()).unwrap();
        assert_eq!(cached.text.units(), &[65, 66]);
        assert_eq!(cached.last_record, 1);
        let next = cached_text(&rs, cached.last_record + 1, ResourceBudget::default()).unwrap();
        assert_eq!(next.text.units(), &[67]);
        assert_eq!(next.last_record, 2);
    }
}
