/// Lookup partition of a search prefix: sheet, orientation (vertical first), and fixed
/// column or row.
pub(in crate::eval) type PrefixPartition = (usize, bool, u32);
