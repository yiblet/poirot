//! Pure BIFF record framing and workbook BOF payloads.
use crate::{XlsError, fail};
/// Maximum BIFF record payload size used by continuation planning.
pub(super) const MAX_RECORD: usize = 8224;
/// Append a little-endian sixteen-bit BIFF field.
pub(super) fn short(out: &mut Vec<u8>, v: u16) {
    out.extend(v.to_le_bytes())
}
/// Append a little-endian thirty-two-bit BIFF field.
pub(super) fn long(out: &mut Vec<u8>, v: u32) {
    out.extend(v.to_le_bytes())
}
/// Append one framed BIFF record after checking its payload size.
pub(super) fn record(out: &mut Vec<u8>, id: u16, data: &[u8]) -> Result<(), XlsError> {
    if data.len() > MAX_RECORD {
        return Err(fail("BIFF record exceeds 8224-byte limit"));
    }
    short(out, id);
    short(out, data.len() as u16);
    out.extend(data);
    Ok(())
}
/// Encode a BIFF begin-of-file record for the selected substream.
pub(super) fn bof(kind: u16) -> Vec<u8> {
    let mut d = Vec::new();
    short(&mut d, 0x0600);
    short(&mut d, kind);
    short(&mut d, 0x0dbb);
    short(&mut d, 1997);
    long(&mut d, 0x41);
    long(&mut d, 6);
    d
}
