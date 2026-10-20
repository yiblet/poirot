//! Bounded planning before calamine's CFB and dense BIFF allocations.
//! Budgets are operational limits, not Excel validity rules.
//!
//! Covers CFB metadata capacities/chains, BIFF record inventory and scan work,
//! Dimensions reservations, actual value/formula dense rectangles, SST decoding
//! and repeated text-cell clones. It does not estimate allocator overhead,
//! backend formula-reconstruction text/stack growth, number-format/name decoding,
//! or the sparse model's final allocation. Input byte limits remain separate.
//! Unknown BIFF records and inaccurate SST counts within budgets remain accepted.
//! Source contracts: POI DimensionsRecord/SSTRecord and calamine0.36.1 CFB/XLS.
//! <https://github.com/tafia/calamine/tree/0a24c2a9f1e38c0932c1299e633270dc730db505/src>
//! Calamine credit is in root LICENSES/ACKNOWLEDGEMENTS; terms in LICENSES/MIT.txt.
use std::{collections::BTreeSet, fmt};

mod cfb;
mod globals;
mod records;
mod sheets;
mod strings;

#[cfg(test)]
const END: u32 = 0xffff_fffe;

/// Limits on BIFF/CFB inspection and backend allocation claims before XLS decoding.
/// These complement [`crate::ReadOptions`] input limits; they are not Excel validity limits.
#[derive(Debug, Clone, Copy)]
pub struct ResourceBudget {
    /// Maximum admitted workbook stream byte size.
    pub max_stream_bytes: usize,
    /// Maximum source metadata entries inspected before backend decoding.
    pub max_metadata_entries: usize,
    /// Maximum BIFF record count.
    pub max_records: usize,
    /// Maximum per-sheet dense cell allocation claim.
    pub max_dense_cells: u64,
    /// Maximum combined dense cell allocation claim across sheets.
    pub max_total_dense_cells: u64,
    /// Maximum shared-string entries.
    pub max_strings: usize,
    /// Maximum character claim for one shared string.
    pub max_string_chars: usize,
    /// Maximum combined shared-string storage claim.
    pub max_string_bytes: usize,
    /// Maximum continuation records admitted for string reconstruction.
    pub max_continuations: usize,
    /// Maximum decoded cell-text storage claim.
    pub max_cell_text_bytes: u64,
}
impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            max_stream_bytes: 32 * 1024 * 1024,
            max_metadata_entries: 1_000_000,
            max_records: 1_000_000,
            max_dense_cells: 1_000_000,
            max_total_dense_cells: 4_000_000,
            max_strings: 1_000_000,
            max_string_chars: 65_535,
            max_string_bytes: 32 * 1024 * 1024,
            max_continuations: 4096,
            max_cell_text_bytes: 128 * 1024 * 1024,
        }
    }
}
/// Malformed or unsupported XLS structure, or a pre-decode resource-budget refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightError {
    /// Source claims exceed an explicit pre-decode resource limit.
    Resource {
        /// Resource category whose admission failed.
        resource: &'static str,
        /// Source amount requested before allocation.
        requested: u64,
        /// Configured maximum for this resource.
        limit: u64,
    },
    /// Compound-file or BIFF framing is structurally invalid.
    Malformed(&'static str),
    /// Source cannot be faithfully represented by the supported backend path.
    Representation(&'static str),
}
impl fmt::Display for PreflightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resource {
                resource,
                requested,
                limit,
            } => write!(f, "{resource} requests {requested}, budget {limit}"),
            Self::Malformed(s) => write!(f, "malformed CFB/BIFF: {s}"),
            Self::Representation(s) => write!(f, "BIFF backend representation: {s}"),
        }
    }
}
impl std::error::Error for PreflightError {}
/// Measured BIFF workbook claims and conservative backend allocation estimates.
/// Inspection retains counts and bounds without constructing a decoded workbook.
#[derive(Debug, Clone, Default)]
pub struct BiffPlan {
    /// Measured workbook stream byte size.
    pub workbook_stream_bytes: usize,
    /// Checked BIFF record count.
    pub records: usize,
    /// Checked worksheet count.
    pub sheets: usize,
    /// Conservative combined dense backend cell allocation estimate.
    pub dense_cells: u64,
    /// Explicit row-presence indices recovered for each sheet.
    pub row_indices: Vec<BTreeSet<u32>>,
    /// Checked shared-string count.
    pub strings: usize,
    /// Estimated shared-string storage bytes.
    pub string_bytes: usize,
}
fn check(resource: &'static str, requested: u64, limit: u64) -> Result<(), PreflightError> {
    if requested > limit {
        Err(PreflightError::Resource {
            resource,
            requested,
            limit,
        })
    } else {
        Ok(())
    }
}
fn u16at(b: &[u8], i: usize) -> Result<u16, PreflightError> {
    let s = b
        .get(
            i..i.checked_add(2)
                .ok_or(PreflightError::Malformed("integer offset overflow"))?,
        )
        .ok_or(PreflightError::Malformed("truncated integer"))?;
    // SAFETY: The preceding get requires an exactly two-byte slice before either byte is selected.
    Ok(u16::from_le_bytes([s[0], s[1]]))
}
fn u32at(b: &[u8], i: usize) -> Result<u32, PreflightError> {
    let s = b
        .get(
            i..i.checked_add(4)
                .ok_or(PreflightError::Malformed("integer offset overflow"))?,
        )
        .ok_or(PreflightError::Malformed("truncated integer"))?;
    // SAFETY: The preceding get requires an exactly four-byte slice, so conversion to [u8; 4] succeeds.
    Ok(u32::from_le_bytes(s.try_into().unwrap()))
}
/// Borrowed checked BIFF record header location and payload.
#[derive(Clone, Copy)]
pub(crate) struct Record<'a> {
    /// BIFF record type identifier.
    pub(crate) id: u16,
    /// Record byte offset within the workbook stream.
    pub(crate) at: usize,
    /// Borrowed payload excluding the BIFF record header.
    pub(crate) data: &'a [u8],
}
/// Validate workbook stream framing and return borrowed BIFF record slices.
pub(crate) fn records(
    bytes: &[u8],
    budget: ResourceBudget,
) -> Result<Vec<Record<'_>>, PreflightError> {
    records::records(bytes, budget)
}

/// Inspect CFB allocation claims, chains, and the Workbook/Book stream before decoding.
pub fn inspect_xls(bytes: &[u8], budget: ResourceBudget) -> Result<BiffPlan, PreflightError> {
    inspect_biff(&workbook_stream(bytes, budget)?, budget)
}

/// Extract the bounded workbook stream from a compound file.
pub(crate) fn workbook_stream(
    bytes: &[u8],
    budget: ResourceBudget,
) -> Result<Vec<u8>, PreflightError> {
    cfb::workbook_stream(bytes, budget)
}

/// Build an inspectable BIFF plan without constructing cells or decoding strings.
pub fn inspect_biff(bytes: &[u8], budget: ResourceBudget) -> Result<BiffPlan, PreflightError> {
    check(
        "Workbook stream bytes",
        bytes.len() as u64,
        budget.max_stream_bytes as u64,
    )?;
    let rs = records(bytes, budget)?;
    let globals = globals::globals(&rs, budget)?;
    let plan = BiffPlan {
        workbook_stream_bytes: bytes.len(),
        records: rs.len(),
        strings: globals.accounting.strings,
        string_bytes: globals.accounting.string_bytes,
        ..BiffPlan::default()
    };
    sheets::sheets(
        &rs,
        globals.positions,
        &globals.accounting.sizes,
        budget,
        plan,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(id: u16, data: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend(id.to_le_bytes());
        b.extend((data.len() as u16).to_le_bytes());
        b.extend(data);
        b
    }
    fn sheet(data: Vec<u8>) -> Vec<u8> {
        let mut bound = vec![0; 8];
        bound[..4].copy_from_slice(&16u32.to_le_bytes());
        let mut b = record(0x85, &bound);
        b.extend(record(0xa, &[]));
        b.extend(record(0x809, &0x600u16.to_le_bytes()));
        b.extend(data);
        b.extend(record(0xa, &[]));
        b
    }
    fn number(row: u16, col: u16) -> Vec<u8> {
        let mut data = vec![0; 14];
        data[..2].copy_from_slice(&row.to_le_bytes());
        data[2..4].copy_from_slice(&col.to_le_bytes());
        record(0x203, &data)
    }
    #[test]
    fn sparse_extreme_coordinates_are_refused_before_dense_expansion() {
        let mut cells = number(0, 0);
        cells.extend(number(65535, 255));
        let b = sheet(cells);
        assert!(matches!(
            inspect_biff(&b, ResourceBudget::default()),
            Err(PreflightError::Resource {
                resource: "sheet dense cells",
                ..
            })
        ));
    }
    #[test]
    fn huge_dimensions_in_tiny_stream_are_refused() {
        let mut d = vec![0; 14];
        d[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        d[10..12].copy_from_slice(&256u16.to_le_bytes());
        assert!(matches!(
            inspect_biff(&sheet(record(0x200, &d)), ResourceBudget::default()),
            Err(PreflightError::Resource { .. })
        ));
    }
    #[test]
    fn resource_budgets_are_configurable() {
        let mut cells = number(0, 0);
        cells.extend(number(2, 2));
        let b = sheet(cells);
        let budget = ResourceBudget {
            max_dense_cells: 8,
            ..ResourceBudget::default()
        };
        assert!(matches!(
            inspect_biff(&b, budget),
            Err(PreflightError::Resource { .. })
        ));
        assert_eq!(
            inspect_biff(&b, ResourceBudget::default())
                .unwrap()
                .dense_cells,
            18
        );
    }
    #[test]
    fn malformed_lengths_and_sheet_offsets_fail() {
        assert!(matches!(
            inspect_biff(&[3, 2, 255, 255], ResourceBudget::default()),
            Err(PreflightError::Malformed(_))
        ));
        assert!(matches!(
            inspect_biff(
                &record(0x85, &999u32.to_le_bytes()),
                ResourceBudget::default()
            ),
            Err(PreflightError::Malformed(_))
        ));
    }
    #[test]
    fn tiny_sst_huge_count_claim_refused() {
        let mut data = vec![0; 8];
        data[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            inspect_biff(&record(0xfc, &data), ResourceBudget::default()),
            Err(PreflightError::Resource { .. })
        ));
    }
    #[test]
    fn sst_continuation_encoding_switch_and_inaccurate_counts_are_accepted() {
        let mut data = vec![0; 8];
        data.extend([3, 0, 0, b'a']);
        let mut b = record(0xfc, &data);
        b.extend(record(0x3c, &[1, b'b', 0, b'c', 0]));
        let plan = inspect_biff(&b, ResourceBudget::default()).unwrap();
        assert_eq!(plan.strings, 1);
        assert_eq!(plan.string_bytes, 12);
    }
    #[test]
    fn truncated_string_extension_is_reported() {
        let mut data = vec![0; 8];
        data.extend([0, 0, 4]);
        data.extend(100u32.to_le_bytes());
        assert!(matches!(
            inspect_biff(&record(0xfc, &data), ResourceBudget::default()),
            Err(PreflightError::Malformed(_))
        ));
    }
    #[test]
    fn continuation_budget_precedes_string_decoding() {
        let mut b = record(0xfc, &[0; 8]);
        b.extend(record(0x3c, &[]));
        let budget = ResourceBudget {
            max_continuations: 0,
            ..ResourceBudget::default()
        };
        assert!(matches!(
            inspect_biff(&b, budget),
            Err(PreflightError::Resource {
                resource: "SST continuation records",
                ..
            })
        ));
    }
    #[test]
    fn cfb_huge_backend_header_capacity_is_refused() {
        let mut b = vec![0; 512];
        b[..8].copy_from_slice(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]);
        b[30..32].copy_from_slice(&9u16.to_le_bytes());
        b[32..34].copy_from_slice(&6u16.to_le_bytes());
        b[62..66].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            inspect_xls(&b, ResourceBudget::default()),
            Err(PreflightError::Resource { .. })
        ));
    }
    fn compound(stream: &[u8]) -> Vec<u8> {
        let mut b = vec![0; 512 * 11];
        b[..8].copy_from_slice(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]);
        b[26..28].copy_from_slice(&3u16.to_le_bytes());
        b[30..32].copy_from_slice(&9u16.to_le_bytes());
        b[32..34].copy_from_slice(&6u16.to_le_bytes());
        b[44..48].copy_from_slice(&1u32.to_le_bytes());
        b[48..52].copy_from_slice(&1u32.to_le_bytes());
        b[60..64].copy_from_slice(&END.to_le_bytes());
        b[68..72].copy_from_slice(&END.to_le_bytes());
        b[76..512].fill(255);
        b[76..80].copy_from_slice(&0u32.to_le_bytes());
        b[512..1024].fill(255);
        for (id, next) in [
            (0, 0xffff_fffdu32),
            (1, END),
            (2, 3),
            (3, 4),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 8),
            (8, 9),
            (9, END),
        ] {
            b[512 + id * 4..516 + id * 4].copy_from_slice(&next.to_le_bytes());
        }
        b[1024 + 116..1024 + 120].copy_from_slice(&END.to_le_bytes());
        let d = 1024 + 128;
        for (i, c) in "Workbook".encode_utf16().enumerate() {
            b[d + i * 2..d + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
        }
        b[d + 116..d + 120].copy_from_slice(&2u32.to_le_bytes());
        b[d + 120..d + 124].copy_from_slice(&4096u32.to_le_bytes());
        b[1536..1536 + stream.len()].copy_from_slice(stream);
        b
    }
    #[test]
    fn valid_compound_workbook_is_planned() {
        let b = compound(&sheet(number(1, 1)));
        let plan = inspect_xls(&b, ResourceBudget::default()).unwrap();
        assert_eq!(plan.sheets, 1);
        assert_eq!(plan.dense_cells, 2);
    }
    #[test]
    fn cyclic_compound_chain_fails_without_backend_execution() {
        let mut b = compound(&sheet(number(1, 1)));
        b[512 + 9 * 4..516 + 9 * 4].copy_from_slice(&2u32.to_le_bytes());
        assert!(matches!(
            inspect_xls(&b, ResourceBudget::default()),
            Err(PreflightError::Malformed("cyclic sector chain"))
        ));
    }
    #[test]
    fn truncated_compound_sector_is_reported() {
        let mut b = compound(&sheet(number(1, 1)));
        b.truncate(b.len() - 1);
        assert!(matches!(
            inspect_xls(&b, ResourceBudget::default()),
            Err(PreflightError::Malformed("sector beyond input"))
        ));
    }
    #[test]
    fn declared_workbook_stream_budget_precedes_chain_growth() {
        let mut b = compound(&[]);
        let d = 1024 + 128;
        b[d + 120..d + 124].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            inspect_xls(&b, ResourceBudget::default()),
            Err(PreflightError::Resource {
                resource: "CFB stream bytes",
                ..
            })
        ));
    }

    #[test]
    fn orphan_formula_string_default_position_is_in_dense_bounds() {
        let mut cells = number(65535, 255);
        cells.extend(record(0x207, &[1, 0, 0, b'a']));
        assert!(matches!(
            inspect_biff(&sheet(cells), ResourceBudget::default()),
            Err(PreflightError::Resource {
                resource: "sheet dense cells",
                ..
            })
        ));
    }
    #[test]
    fn malformed_name_and_merge_count_fail_before_backend() {
        assert!(matches!(
            inspect_biff(&record(0x18, &[0; 3]), ResourceBudget::default()),
            Err(PreflightError::Malformed(_))
        ));
        assert!(matches!(
            inspect_biff(&sheet(record(0xe5, &[255, 255])), ResourceBudget::default()),
            Err(PreflightError::Malformed(_))
        ));
    }

    #[test]
    fn compound_stream_zero_tail_padding_after_eof_is_permissive() {
        let mut stream = sheet(number(1, 1));
        stream.extend([0, 0, 0]);
        assert_eq!(
            inspect_biff(&stream, ResourceBudget::default())
                .unwrap()
                .dense_cells,
            2
        );
        let mut bad = sheet(number(1, 1));
        bad.extend([0, 0, 1]);
        assert!(matches!(
            inspect_biff(&bad, ResourceBudget::default()),
            Err(PreflightError::Malformed(_))
        ));
    }

    #[test]
    fn global_handoff_accumulates_sst_totals_and_preserves_duplicate_sheets() {
        let mut first = vec![0; 8];
        first.extend([1, 0, 0, b'a']);
        let mut last = vec![0; 8];
        last.extend([3, 0, 0, b'b', b'c', b'd']);
        let mut stream = record(0xfc, &first);
        stream.extend(record(0xfc, &last));
        let sheet_at = (stream.len() + 12 * 2 + 4) as u32;
        let mut bound = vec![0; 8];
        bound[..4].copy_from_slice(&sheet_at.to_le_bytes());
        stream.extend(record(0x85, &bound));
        stream.extend(record(0x85, &bound));
        stream.extend(record(0xa, &[]));
        let mut row = vec![0; 16];
        row[..2].copy_from_slice(&9u16.to_le_bytes());
        stream.extend(record(0x0208, &row));
        stream.extend(record(0xfd, &[0; 10]));
        stream.extend(record(0xa, &[]));

        let plan = inspect_biff(&stream, ResourceBudget::default()).unwrap();
        assert_eq!(plan.strings, 2);
        assert_eq!(plan.string_bytes, 16);
        assert_eq!(plan.sheets, 2);
        assert_eq!(plan.dense_cells, 4);
        assert_eq!(plan.row_indices, vec![BTreeSet::from([9]); 2]);
        let budget = ResourceBudget {
            max_cell_text_bytes: 23,
            ..ResourceBudget::default()
        };
        assert_eq!(
            inspect_biff(&stream, budget).unwrap_err(),
            PreflightError::Resource {
                resource: "cell text bytes",
                requested: 24,
                limit: 23,
            }
        );
    }
}
