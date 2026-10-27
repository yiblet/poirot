/// Capped serializer output that rejects oversized snapshots during encoding.
pub(super) struct Output {
    pub(super) bytes: Vec<u8>,
    pub(super) limit: usize,
}

impl postcard::ser_flavors::Flavor for Output {
    /// Encoded native snapshot bytes.
    type Output = Vec<u8>;

    fn try_push(&mut self, byte: u8) -> postcard::Result<()> {
        self.try_extend(&[byte])
    }

    fn try_extend(&mut self, bytes: &[u8]) -> postcard::Result<()> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(postcard::Error::SerializeBufferFull);
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn finalize(self) -> postcard::Result<Vec<u8>> {
        Ok(self.bytes)
    }
}
