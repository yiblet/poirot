use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

/// Seekable ZIP append storage that borrows the archive before its central directory.
pub(super) struct ZipAppendBuffer<'a> {
    original: &'a [u8],
    offset: usize,
    position: u64,
    suffix: Vec<u8>,
    limit: usize,
    end: u64,
    writing: bool,
    discarded: bool,
}

impl<'a> ZipAppendBuffer<'a> {
    /// Check the prefix and bound memory reserved for the replacement suffix.
    pub(super) fn new(
        original: &'a [u8],
        offset: usize,
        estimate: usize,
        limit: usize,
    ) -> Option<Self> {
        if offset > original.len() || estimate > limit {
            return None;
        }
        let mut suffix = Vec::new();
        suffix.try_reserve_exact(estimate).ok()?;
        if suffix.capacity() > limit {
            return None;
        }
        Some(Self {
            original,
            offset,
            position: 0,
            suffix,
            limit,
            end: original.len() as u64,
            writing: false,
            discarded: false,
        })
    }

    /// Release the finished archive only when its complete suffix was retained.
    pub(super) fn finish(self) -> Option<(&'a [u8], Vec<u8>)> {
        if self.discarded || !self.writing {
            return None;
        }
        Some((self.original.get(..self.offset)?, self.suffix))
    }
}

impl Read for ZipAppendBuffer<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if !self.writing {
            let mut cursor = Cursor::new(self.original);
            cursor.set_position(self.position);
            let count = cursor.read(bytes)?;
            self.position = cursor.position();
            return Ok(count);
        }
        let (contents, base) = if self.position < self.offset as u64 {
            (self.original.get(..self.offset).unwrap_or_default(), 0)
        } else {
            (self.suffix.as_slice(), self.offset as u64)
        };
        let mut cursor = Cursor::new(contents);
        cursor.set_position(self.position.saturating_sub(base));
        let count = cursor.read(bytes)?;
        self.position = self.position.saturating_add(count as u64);
        Ok(count)
    }
}

impl Write for ZipAppendBuffer<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.writing {
            self.end = self.offset as u64;
            self.writing = true;
        }
        let end = self.position.saturating_add(bytes.len() as u64);
        let start = usize::try_from(self.position)
            .ok()
            .and_then(|p| p.checked_sub(self.offset));
        let len = start.and_then(|p| p.checked_add(bytes.len()));
        if !self.discarded
            && let (Some(start), Some(len)) = (start, len)
            && len <= self.limit
            && self
                .suffix
                .try_reserve_exact(len.saturating_sub(self.suffix.len()))
                .is_ok()
            && self.suffix.capacity() <= self.limit
        {
            if len > self.suffix.len() {
                // SAFETY: The successful reservation above covers the complete suffix.
                self.suffix.resize(len, 0);
            }
            if let Some(target) = self.suffix.get_mut(start..len) {
                // SAFETY: len was computed from start plus the length of bytes.
                target.copy_from_slice(bytes);
            }
        } else {
            // Keep the writer's cleanup seeks and writes valid after discarding the attempt.
            self.discarded = true;
        }
        self.position = end;
        self.end = self.end.max(end);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Seek for ZipAppendBuffer<'_> {
    fn seek(&mut self, target: SeekFrom) -> io::Result<u64> {
        self.position = match target {
            SeekFrom::Start(at) => Some(at),
            SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            SeekFrom::End(offset) => self.end.checked_add_signed(offset),
        }
        .ok_or(io::ErrorKind::InvalidInput)?;
        Ok(self.position)
    }
}

#[cfg(test)]
mod tests {
    use super::super::zip_segments::ZipSegments;
    use super::ZipAppendBuffer;
    use std::io::{Read, Seek, SeekFrom, Write};

    #[test]
    fn append_and_rewrite_preserve_original_prefix_and_replace_old_tail() {
        let original = b"prefix old directory";
        let mut buffer = ZipAppendBuffer::new(original, 7, 4, 4).unwrap();
        let mut input = Vec::new();
        buffer.read_to_end(&mut input).unwrap();
        assert_eq!(input, original);

        buffer.seek(SeekFrom::Start(7)).unwrap();
        buffer.write_all(b"123").unwrap();
        buffer.seek(SeekFrom::Current(-2)).unwrap();
        buffer.write_all(b"XY").unwrap();
        assert_eq!(buffer.seek(SeekFrom::End(0)).unwrap(), 10);

        let (prefix, suffix) = buffer.finish().unwrap();
        assert_eq!(prefix.as_ptr(), original.as_ptr());
        let mut reader = ZipSegments::new(prefix, &suffix);
        reader.seek(SeekFrom::End(-4)).unwrap();
        let mut boundary = [0; 4];
        reader.read_exact(&mut boundary).unwrap();
        assert_eq!(&boundary, b" 1XY");
        assert_eq!(reader.read(&mut boundary).unwrap(), 0);
    }

    #[test]
    fn rejects_output_cap_and_writes_over_the_borrowed_prefix() {
        let input = b"before old directory";
        for (position, data) in [(7, b"12345".as_slice()), (6, b"1".as_slice())] {
            let mut buffer = ZipAppendBuffer::new(input, 7, 4, 4).unwrap();
            buffer.seek(SeekFrom::Start(position)).unwrap();
            buffer.write_all(data).unwrap();
            assert!(buffer.finish().is_none());
        }
    }
}
