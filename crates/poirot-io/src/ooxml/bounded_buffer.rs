use std::cell::Cell;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

/// Temporary ZIP backing storage that discards output after reaching its limit.
pub(super) struct BoundedBuffer<'a> {
    inner: Cursor<Vec<u8>>,
    limit: usize,
    discarded: &'a Cell<bool>,
    discarded_len: u64,
}

impl<'a> BoundedBuffer<'a> {
    /// Reserve the estimated output size up to the archive limit.
    pub(super) fn new(estimate: usize, limit: usize, discarded: &'a Cell<bool>) -> Option<Self> {
        if estimate > limit {
            return None;
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(estimate).ok()?;
        Some(Self {
            inner: Cursor::new(bytes),
            limit,
            discarded,
            discarded_len: 0,
        })
    }

    /// Release the completed archive bytes after checking the discard state.
    pub(super) fn into_bytes(self) -> Option<Vec<u8>> {
        (!self.discarded.get()).then(|| self.inner.into_inner())
    }
}

impl Read for BoundedBuffer<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.inner.read(bytes)
    }
}

impl Write for BoundedBuffer<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let end = self.inner.position().saturating_add(bytes.len() as u64);
        if !self.discarded.get() && end <= self.limit as u64 {
            let storage = self.inner.get_mut();
            if storage
                .try_reserve_exact((end as usize).saturating_sub(storage.len()))
                .is_ok()
            {
                // SAFETY: The checked end and successful reservation cover this entire write.
                return self.inner.write(bytes);
            }
        }
        // A discarded archive must still accept the ZIP writer's cleanup writes.
        self.discarded.set(true);
        self.discarded_len = self
            .discarded_len
            .max(end)
            .max(self.inner.get_ref().len() as u64);
        self.inner.set_position(end);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl Seek for BoundedBuffer<'_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        if self.discarded.get()
            && let SeekFrom::End(offset) = position
        {
            let at = self
                .discarded_len
                .checked_add_signed(offset)
                .ok_or(io::ErrorKind::InvalidInput)?;
            self.inner.set_position(at);
            return Ok(at);
        }
        self.inner.seek(position)
    }
}

#[cfg(test)]
mod tests {
    use super::BoundedBuffer;
    use std::cell::Cell;
    use std::io::{Seek, SeekFrom, Write};

    #[test]
    fn permits_header_rewrites_and_discards_oversized_archives() {
        let discarded = Cell::new(false);
        let mut buffer = BoundedBuffer::new(4, 8, &discarded).unwrap();
        buffer.write_all(b"12345678").unwrap();
        buffer.seek(SeekFrom::Start(1)).unwrap();
        buffer.write_all(b"AB").unwrap();
        assert_eq!(buffer.into_bytes().unwrap(), b"1AB45678");

        let mut buffer = BoundedBuffer::new(4, 8, &discarded).unwrap();
        buffer.write_all(b"123456789").unwrap();
        assert!(discarded.get());
        assert_eq!(buffer.seek(SeekFrom::End(-1)).unwrap(), 8);
        buffer.write_all(b"XYZ").unwrap();
        assert_eq!(buffer.seek(SeekFrom::End(0)).unwrap(), 11);
        assert!(buffer.into_bytes().is_none());
    }
}
