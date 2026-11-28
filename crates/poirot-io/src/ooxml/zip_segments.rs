use std::io::{self, Cursor, Read, Seek, SeekFrom};

/// Read a ZIP archive from borrowed original bytes followed by an appended suffix.
pub(crate) struct ZipSegments<'a> {
    prefix: &'a [u8],
    suffix: &'a [u8],
    position: u64,
}

impl<'a> ZipSegments<'a> {
    /// Borrow both contiguous regions of the rewritten archive.
    pub(super) fn new(prefix: &'a [u8], suffix: &'a [u8]) -> Self {
        Self {
            prefix,
            suffix,
            position: 0,
        }
    }
}

impl Read for ZipSegments<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let (data, start) = if self.position < self.prefix.len() as u64 {
            (self.prefix, 0)
        } else {
            (self.suffix, self.prefix.len() as u64)
        };
        let mut cursor = Cursor::new(data);
        cursor.set_position(self.position.saturating_sub(start));
        let count = cursor.read(bytes)?;
        self.position = self.position.saturating_add(count as u64);
        Ok(count)
    }
}

impl Seek for ZipSegments<'_> {
    fn seek(&mut self, target: SeekFrom) -> io::Result<u64> {
        let len = (self.prefix.len() as u64).saturating_add(self.suffix.len() as u64);
        self.position = match target {
            SeekFrom::Start(at) => Some(at),
            SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            SeekFrom::End(offset) => len.checked_add_signed(offset),
        }
        .ok_or(io::ErrorKind::InvalidInput)?;
        Ok(self.position)
    }
}
