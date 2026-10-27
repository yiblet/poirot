use super::reference_kind::ReferenceKind;

/// Locates one formula reference's encoded coordinates in [`ReferenceBindings::data`](super::ReferenceBindings::data).
/// Data uses little-endian u32 fields and strings encoded as u32 byte length + UTF-8.
/// Cell: row,column,row_absolute,column_absolute,optional sheet; column/row: index,
/// absolute,optional sheet; span: first/last sheet,cell start,end-present,optional end.
/// Optional strings have a presence u32 before their length/string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReferenceBinding {
    /// Reference encoding used by this slot.
    pub kind: ReferenceKind,
    /// Byte offset into the reference data buffer.
    pub offset: u32,
    /// Encoded reference length in bytes.
    pub length: u32,
}
