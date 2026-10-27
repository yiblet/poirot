//! Workbook-stream admission: record framing, encryption refusal and budget charges.
use super::biff_plan::BiffPlan;
use super::checks::check;
use super::globals_plan::globals;
use super::preflight_error::PreflightError;
use super::record::Record;
use super::records::records;
use super::resource_budget::ResourceBudget;
use super::sheets::sheets;

/// Admit a workbook stream: frame its records, refuse encryption, charge
/// globals and every worksheet traversal. Decoding must use the returned
/// records and [`BiffPlan::sheet_starts`], so it reads only admitted records.
pub(in crate::xls) fn admit(
    bytes: &[u8],
    budget: ResourceBudget,
) -> Result<(Vec<Record<'_>>, BiffPlan), PreflightError> {
    check(
        "Workbook stream bytes",
        bytes.len() as u64,
        budget.max_stream_bytes as u64,
    )?;
    let rs = records(bytes, budget)?;
    let globals = globals(&rs, budget)?;
    let plan = sheets(
        &rs,
        &globals.positions,
        &globals.accounting.sizes,
        budget,
        BiffPlan::default(),
    )?;
    Ok((rs, plan))
}

#[cfg(test)]
mod tests {
    use super::super::cfb::workbook_stream;
    use super::*;
    use crate::data::SheetSource;
    use std::collections::BTreeMap;

    const END: u32 = 0xffff_fffe;

    /// Inspect CFB allocation claims, chains, and the Workbook/Book stream before decoding.
    fn inspect_xls(bytes: &[u8], budget: ResourceBudget) -> Result<BiffPlan, PreflightError> {
        inspect_biff(&workbook_stream(bytes, budget)?, budget)
    }

    /// Build an inspectable BIFF plan without constructing cells or decoding strings.
    fn inspect_biff(bytes: &[u8], budget: ResourceBudget) -> Result<BiffPlan, PreflightError> {
        admit(bytes, budget).map(|(_, plan)| plan)
    }

    /// Shared-string count and storage estimate admitted from workbook globals.
    fn sst_totals(stream: &[u8]) -> (usize, usize) {
        let budget = ResourceBudget::default();
        let accounting = globals(&records(stream, budget).unwrap(), budget)
            .unwrap()
            .accounting;
        (accounting.strings, accounting.string_bytes)
    }

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

    fn importable_compound(cells: Vec<u8>) -> Vec<u8> {
        use std::io::{Cursor, Write};
        // Original BIFF8 fixture: global BOF, named worksheet, sheet BOF and
        // sparse NUMBER records. BoundSheet8 points to the sheet substream.
        let mut bof = vec![0; 16];
        bof[..2].copy_from_slice(&0x600u16.to_le_bytes());
        bof[2..4].copy_from_slice(&5u16.to_le_bytes());
        let mut stream = record(0x809, &bof);
        let bound_at = stream.len() + 4;
        let mut bound = vec![0; 8];
        bound[6] = 4;
        bound.extend(b"Data");
        stream.extend(record(0x85, &bound));
        stream.extend(record(0xa, &[]));
        let sheet_at = stream.len() as u32;
        stream[bound_at..bound_at + 4].copy_from_slice(&sheet_at.to_le_bytes());
        bof[2..4].copy_from_slice(&0x10u16.to_le_bytes());
        stream.extend(record(0x809, &bof));
        stream.extend(cells);
        stream.extend(record(0xa, &[]));
        let mut compound =
            ::cfb::CompoundFile::create_with_version(::cfb::Version::V3, Cursor::new(Vec::new()))
                .unwrap();
        compound
            .create_stream("/Workbook")
            .unwrap()
            .write_all(&stream)
            .unwrap();
        compound.into_inner().into_inner()
    }

    #[test]
    fn sparse_extreme_coordinates_do_not_claim_dense_storage() {
        let mut cells = number(0, 0);
        cells.extend(number(65535, 255));
        let bytes = importable_compound(cells);
        let plan = inspect_xls(&bytes, ResourceBudget::default()).unwrap();
        assert_eq!(plan.sheets, 1);
        let imported = crate::read_workbook_bytes::<crate::test_support::Workbook>(&bytes).unwrap();
        let sheet = imported.sheet(0).unwrap();
        assert_eq!(sheet.cells().count(), 2);
        assert!(
            sheet
                .get(crate::CellAddress::new(65535, 255).unwrap())
                .is_some()
        );
    }

    #[test]
    fn unused_dimensions_do_not_reserve_storage() {
        let mut dimensions = vec![0; 14];
        dimensions[4..8].copy_from_slice(&65536u32.to_le_bytes());
        dimensions[10..12].copy_from_slice(&256u16.to_le_bytes());
        let mut cells = record(0x200, &dimensions);
        cells.extend(number(0, 0));
        let bytes = importable_compound(cells);
        assert!(inspect_xls(&bytes, ResourceBudget::default()).is_ok());
        let imported = crate::read_workbook_bytes::<crate::test_support::Workbook>(&bytes).unwrap();
        assert_eq!(imported.sheet(0).unwrap().cells().count(), 1);
    }

    #[test]
    fn record_resource_budgets_are_configurable() {
        let bytes = sheet(number(0, 0));
        let budget = ResourceBudget {
            max_records: 0,
            ..ResourceBudget::default()
        };
        assert!(matches!(
            inspect_biff(&bytes, budget),
            Err(PreflightError::Resource { .. })
        ));
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
            inspect_biff(
                &record(0xfc, &data),
                ResourceBudget::default().max_strings(1_000_000)
            ),
            Err(PreflightError::Resource { .. })
        ));
    }

    #[test]
    fn sst_continuation_encoding_switch_and_inaccurate_counts_are_accepted() {
        let mut data = vec![0; 8];
        data.extend([3, 0, 0, b'a']);
        let mut b = record(0xfc, &data);
        b.extend(record(0x3c, &[1, b'b', 0, b'c', 0]));
        inspect_biff(&b, ResourceBudget::default()).unwrap();
        assert_eq!(sst_totals(&b), (1, 12));
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
            inspect_xls(
                &b,
                ResourceBudget::default().max_metadata_entries(1_000_000)
            ),
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
            inspect_xls(
                &b,
                ResourceBudget::default().max_stream_bytes(32 * 1024 * 1024)
            ),
            Err(PreflightError::Resource {
                resource: "CFB stream bytes",
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
                .sheets,
            1
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
        assert_eq!(sst_totals(&stream), (2, 16));
        assert_eq!(plan.sheets, 2);
        assert_eq!(
            plan.rows,
            vec![BTreeMap::from([(9, crate::RowVisibility::default())]); 2]
        );
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
