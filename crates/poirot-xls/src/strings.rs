use super::*;
use poirot_model::{TextRef, TextUnits};

/// Shared allocation-free grouping for writer and admission accounting. Only a
/// high unit followed by a low unit forms an indivisible pair.
pub(super) fn character_groups(text: TextRef<'_>) -> impl Iterator<Item = usize> + '_ {
    let mut units = text.units().peekable();
    std::iter::from_fn(move || {
        let unit = units.next()?;
        let paired = (0xd800..=0xdbff).contains(&unit)
            && units
                .peek()
                .is_some_and(|next| (0xdc00..=0xdfff).contains(next));
        if paired {
            units.next();
        }
        Some(if paired { 2 } else { 1 })
    })
}

/// BIFF8 Unicode strings with CONTINUE encoding flags only during character data.
pub(super) struct Continued {
    id: u16,
    /// Current bounded shared-string record payload.
    pub(super) data: Vec<u8>,
    records: Vec<(u16, Vec<u8>)>,
}
impl Continued {
    /// Start BIFF string emission with explicit continuation accounting.
    pub(super) fn new(id: u16) -> Self {
        Self {
            id,
            data: Vec::new(),
            records: Vec::new(),
        }
    }
    fn next(&mut self, characters: bool) {
        self.records.push((self.id, std::mem::take(&mut self.data)));
        self.id = 0x003c;
        if characters {
            self.data.push(1)
        }
    }
    /// Append exact string units, splitting only at valid BIFF continuation boundaries.
    pub(super) fn string(&mut self, text: TextRef<'_>) -> Result<(), XlsError> {
        let length = text.len_units();
        if length > 32767 {
            return Err(XlsError::ResourceLimit(
                "text UTF-16 units exceeds export limit 32767".into(),
            ));
        }
        if self.data.len() + 3 > MAX_RECORD {
            self.next(false)
        }
        short(&mut self.data, length as u16);
        self.data.push(1);
        let mut units: TextUnits<'_> = text.units();
        for count in character_groups(text) {
            if self.data.len() + count * 2 > MAX_RECORD {
                self.next(true)
            }
            for _ in 0..count {
                short(
                    &mut self.data,
                    units
                        .next()
                        .ok_or_else(|| crate::fail("truncated UTF16 group"))?,
                );
            }
        }
        Ok(())
    }
    /// Return the completed SST/CONTINUE record bytes.
    pub(super) fn finish(mut self, out: &mut Vec<u8>) -> Result<(), XlsError> {
        self.records.push((self.id, self.data));
        for (id, data) in self.records {
            record(out, id, &data)?
        }
        Ok(())
    }
}
