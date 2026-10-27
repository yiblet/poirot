// Byte sinks are shared by counting and final emission, so exact capacities cannot drift.
/// Binary operand writer used to encode the executable's constant/reference storage.
pub(super) trait Bytes {
    fn bytes(&mut self, data: &[u8]);

    fn word(&mut self, n: u32) {
        self.bytes(&n.to_le_bytes());
    }

    fn string(&mut self, s: &str) {
        self.word(s.len() as u32);
        self.bytes(s.as_bytes());
    }

    fn optional(&mut self, s: Option<&str>) {
        self.word(u32::from(s.is_some()));
        if let Some(s) = s {
            self.string(s);
        }
    }

    fn cell(&mut self, c: &poirot_parsing::formula::CellReference) {
        self.word(c.row);
        self.word(c.column);
        self.word(c.row_absolute.into());
        self.word(c.column_absolute.into());
        self.optional(c.sheet.as_deref());
    }
}

impl Bytes for Vec<u8> {
    fn bytes(&mut self, data: &[u8]) {
        self.extend_from_slice(data);
    }
}

impl Bytes for usize {
    fn bytes(&mut self, data: &[u8]) {
        *self += data.len();
    }
}
