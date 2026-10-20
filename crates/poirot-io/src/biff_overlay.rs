//! Bounded BIFF8 fidelity overlay. Layout references: pinned Apache POI
//! StringPtg, NameRecord, AreaPtg, Ref3DPtg and StringRecord serializers.
//! Unknown token families remain backend-owned and produce a fidelity warning.
use crate::biff_preflight::{PreflightError, ResourceBudget, records, workbook_stream};
use poirot_model::{Cell, CellAddress, CellValue, ExcelText, NamedRange, Workbook};

mod backend;
mod formula;
mod reconstruct;
mod sst;
mod text;

/// BIFF fidelity-overlay result sharing preflight structural/resource diagnostics.
type Result<T> = std::result::Result<T, PreflightError>;

/// Reconstructed source formula to restore after backend workbook decoding.
struct Formula {
    sheet: usize,
    address: CellAddress,
    text: String,
}
/// Checked BIFF source details and backend patches used to restore fidelity after decoding.
pub(crate) struct Overlay {
    /// Sanitized compound-file bytes passed to the XLS backend.
    pub backend: Vec<u8>,
    /// Source sheet names in workbook order.
    pub sheet_names: Vec<String>,
    visibility: Vec<poirot_model::SheetVisibility>,
    names: Vec<NamedRange>,
    formulas: Vec<Formula>,
    caches: Vec<(usize, CellAddress, ExcelText)>,
    /// Inspectable fidelity omissions discovered during reconstruction.
    pub warnings: Vec<String>,
}
impl Overlay {
    /// Preflight BIFF8 and reconstruct source details under the import budget.
    pub fn read(bytes: &[u8], budget: ResourceBudget) -> Result<Self> {
        let stream = workbook_stream(bytes, budget)?;
        let rs = records(&stream, budget)?;
        if rs
            .iter()
            .find(|r| r.id == 0x809)
            .is_none_or(|r| r.data.get(..2) != Some(&[0, 6]))
        {
            return Ok(Self {
                backend: bytes.to_vec(),
                sheet_names: Vec::new(),
                visibility: Vec::new(),
                names: Vec::new(),
                formulas: Vec::new(),
                caches: Vec::new(),
                warnings: vec!["BIFF fidelity overlay supports BIFF8 only".into()],
            });
        }
        let visibility = rs
            .iter()
            .take_while(|r| r.id != 0xa)
            .filter(|r| r.id == 0x85)
            .map(|r| match r.data.get(4).map(|v| v & 3) {
                Some(0) => Ok(poirot_model::SheetVisibility::Visible),
                Some(1) => Ok(poirot_model::SheetVisibility::Hidden),
                Some(2) => Ok(poirot_model::SheetVisibility::VeryHidden),
                _ => Err(PreflightError::Malformed(
                    "invalid BIFF worksheet visibility",
                )),
            })
            .collect::<Result<Vec<_>>>()?;
        let mut plan = reconstruct::reconstruct(&rs, budget)?;
        plan.warnings.push(
            "XLS tab colors are not retained; palette and extension tab colors remain unsupported"
                .into(),
        );
        Ok(Self {
            backend: backend::backend_view(bytes, &stream, &plan.patches)?,
            sheet_names: plan.sheets,
            visibility,
            names: plan.names,
            formulas: plan.formulas,
            caches: plan.caches,
            warnings: plan.warnings,
        })
    }
    /// Restore reconstructed formulas, names, and exact cached text into the decoded model.
    pub fn apply(
        self,
        workbook: &mut Workbook,
    ) -> std::result::Result<(), poirot_model::ModelError> {
        for (index, visibility) in self.visibility.into_iter().enumerate() {
            if let Some(sheet) = workbook.sheet_mut(index) {
                let mut p = sheet.presentation();
                p.visibility = visibility;
                sheet.set_presentation(p);
            }
        }
        for f in self.formulas {
            if let Some(sheet) = workbook.sheet_mut(f.sheet) {
                let cached = sheet
                    .get(f.address)
                    .map_or(CellValue::Blank, |c| c.value.clone());
                sheet.set_formula_with_cached_value(f.address, f.text, cached)?;
            }
        }
        for (index, address, text) in self.caches {
            if let Some(sheet) = workbook.sheet_mut(index) {
                if let Some(cell) = sheet.get_mut(address) {
                    cell.value = CellValue::TextUtf16(text);
                } else {
                    sheet.set(address, Cell::new(CellValue::TextUtf16(text)))?;
                }
            }
        }
        if !self.names.is_empty() {
            workbook.named_ranges = self.names;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::formula::formula;
    use super::*;
    use std::io::{Cursor, Write};
    fn record(stream: &mut Vec<u8>, id: u16, data: &[u8]) {
        stream.extend(id.to_le_bytes());
        stream.extend((data.len() as u16).to_le_bytes());
        stream.extend(data);
    }
    fn compound(tokens: &[u8], cache: Option<(&[u8], &[u8])>) -> Vec<u8> {
        let mut stream = Vec::new();
        let mut bof = vec![0; 16];
        bof[..2].copy_from_slice(&0x600u16.to_le_bytes());
        bof[2..4].copy_from_slice(&5u16.to_le_bytes());
        record(&mut stream, 0x809, &bof);
        record(&mut stream, 0x42, &1252u16.to_le_bytes());
        let bound_at = stream.len() + 4;
        let mut bound = vec![0; 8];
        bound[6] = 4;
        bound.extend(b"Data");
        record(&mut stream, 0x85, &bound);
        record(&mut stream, 0xa, &[]);
        let position = stream.len() as u32;
        stream[bound_at..bound_at + 4].copy_from_slice(&position.to_le_bytes());
        bof[2..4].copy_from_slice(&0x10u16.to_le_bytes());
        record(&mut stream, 0x809, &bof);
        let mut data = vec![0; 22];
        data[6..14].copy_from_slice(&1f64.to_le_bytes());
        if cache.is_some() {
            data[6..14].copy_from_slice(&[0, 0, 0, 0, 0, 0, 255, 255]);
        }
        data[20..22].copy_from_slice(&(tokens.len() as u16).to_le_bytes());
        data.extend(tokens);
        record(&mut stream, 6, &data);
        if let Some((first, next)) = cache {
            record(&mut stream, 0x207, first);
            record(&mut stream, 0x3c, next);
        }
        record(&mut stream, 0xa, &[]);
        let mut c =
            cfb::CompoundFile::create_with_version(cfb::Version::V3, Cursor::new(Vec::new()))
                .unwrap();
        c.create_stream("/Workbook")
            .unwrap()
            .write_all(&stream)
            .unwrap();
        c.into_inner().into_inner()
    }
    #[test]
    fn independent_boundsheet_flags_preserve_hidden_and_very_hidden() {
        for (flag, expected) in [
            (0, poirot_model::SheetVisibility::Visible),
            (1, poirot_model::SheetVisibility::Hidden),
            (2, poirot_model::SheetVisibility::VeryHidden),
        ] {
            let original = compound(&[0x1e, 1, 0], None);
            let mut c = cfb::CompoundFile::open(Cursor::new(original)).unwrap();
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut c.open_stream("/Workbook").unwrap(), &mut bytes)
                .unwrap();
            let records = records(&bytes, ResourceBudget::default()).unwrap();
            let at = records.iter().find(|r| r.id == 0x85).unwrap().at;
            // BoundSheet8: four-byte stream offset, then hsState, then dt.
            bytes[at + 4 + 4] = flag;
            c.create_stream("/Workbook")
                .unwrap()
                .write_all(&bytes)
                .unwrap();
            let report = crate::read_workbook_bytes(&c.into_inner().into_inner()).unwrap();
            assert_eq!(report.sheet(0).unwrap().presentation().visibility, expected);
            assert!(
                report
                    .file_metadata
                    .as_ref()
                    .unwrap()
                    .import_warnings
                    .iter()
                    .any(|w| w.detail.contains("XLS tab colors are not retained"))
            );
        }
    }
    #[test]
    fn actual_import_keeps_unicode_name_identity_and_local_scopes() {
        let mut stream = Vec::new();
        let mut bof = vec![0; 16];
        bof[..2].copy_from_slice(&0x600u16.to_le_bytes());
        bof[2..4].copy_from_slice(&5u16.to_le_bytes());
        record(&mut stream, 0x809, &bof);
        record(&mut stream, 0x42, &1252u16.to_le_bytes());
        let mut patches = Vec::new();
        for name in ["ΣA", "σA"] {
            patches.push(stream.len() + 4);
            let units: Vec<u16> = name.encode_utf16().collect();
            let mut b = vec![0; 8];
            b[6] = units.len() as u8;
            b[7] = 1;
            for unit in units {
                b.extend(unit.to_le_bytes());
            }
            record(&mut stream, 0x85, &b);
        }
        for scope in [1u16, 2u16] {
            // NAME header cch=2, cce=3, itab=scope; compressed flag is false.
            let mut name = vec![0, 0, 0, 2, 3, 0, 0, 0];
            name.extend(scope.to_le_bytes());
            name.extend([0, 0, 0, 0, 1, 0x0d, 0x54, 0xf0, 0x79, 0x1e, scope as u8, 0]);
            record(&mut stream, 0x18, &name);
        }
        record(&mut stream, 0xa, &[]);
        for (i, patch) in patches.into_iter().enumerate() {
            let offset = stream.len() as u32;
            stream[patch..patch + 4].copy_from_slice(&offset.to_le_bytes());
            bof[2..4].copy_from_slice(&0x10u16.to_le_bytes());
            record(&mut stream, 0x809, &bof);
            let mut cell = vec![0; 22];
            cell[6..14].copy_from_slice(&((i + 1) as f64).to_le_bytes());
            cell[20..22].copy_from_slice(&5u16.to_le_bytes());
            cell.push(0x23);
            cell.extend(((i + 1) as u32).to_le_bytes());
            record(&mut stream, 6, &cell);
            record(&mut stream, 0xa, &[]);
        }
        let mut c =
            cfb::CompoundFile::create_with_version(cfb::Version::V3, Cursor::new(Vec::new()))
                .unwrap();
        c.create_stream("/Workbook")
            .unwrap()
            .write_all(&stream)
            .unwrap();
        let bytes = c.into_inner().into_inner();
        let raw = records(&stream, ResourceBudget::default()).unwrap();
        let names: Vec<_> = raw.iter().filter(|r| r.id == 0x18).collect();
        let sheet_records: Vec<_> = raw.iter().filter(|r| r.id == 0x85).collect();
        // Original failing source remains CODEPAGE 1252 plus valid UTF16 sheet bytes.
        assert_eq!(raw.iter().find(|r| r.id == 0x42).unwrap().data, &[0xe4, 4]);
        assert_eq!(&sheet_records[0].data[6..], &[2, 1, 0xa3, 3, 0x41, 0]);
        assert_eq!(&sheet_records[1].data[6..], &[2, 1, 0xc3, 3, 0x41, 0]);
        assert_eq!(&names[0].data[14..19], &[1, 0x0d, 0x54, 0xf0, 0x79]);
        assert_eq!(&names[1].data[8..10], &[2, 0]);
        let report = crate::read_workbook_bytes(&bytes).unwrap();
        assert_eq!(report.named_ranges.len(), 2);
        assert_eq!(report.sheet(0).unwrap().name(), "ΣA");
        assert_eq!(report.sheet(1).unwrap().name(), "σA");
        assert!(
            report
                .file_metadata
                .as_ref()
                .unwrap()
                .import_warnings
                .iter()
                .any(|warning| warning.detail.contains("unsupported identifier comparison"))
        );
        for (i, name) in report.named_ranges.iter().enumerate() {
            assert_eq!(name.name, "名称");
            assert_eq!(name.sheet_scope, Some(i));
            assert_eq!(name.formula, (i + 1).to_string());
            let cell = report
                .sheet(i)
                .unwrap()
                .get(CellAddress::new(0, 0).unwrap())
                .unwrap();
            assert_eq!(cell.formula.as_deref(), Some("名称"));
        }
    }
    #[test]
    fn raw_imported_sheet_grammar_is_preserved() {
        let bytes = compound(&[0x1e, 1, 0], None);
        let mut stream = workbook_stream(&bytes, ResourceBudget::default()).unwrap();
        let mut c = cfb::CompoundFile::open(Cursor::new(bytes)).unwrap();
        let raw = records(&stream, ResourceBudget::default()).unwrap();
        let at = raw.iter().find(|r| r.id == 0x85).unwrap().at;
        stream[at + 12..at + 16].copy_from_slice(b"A/?B");
        c.open_stream("/Workbook")
            .unwrap()
            .write_all(&stream)
            .unwrap();
        let bytes = c.into_inner().into_inner();
        let report = crate::read_workbook_bytes(&bytes).unwrap();
        assert_eq!(report.sheet(0).unwrap().name(), "A/?B");
    }
    #[test]
    fn independent_poi_formula_fixture_actual_import() {
        let bytes = include_bytes!("../tests/fixtures/poi-StringFormulas.xls");
        let stream = workbook_stream(bytes, ResourceBudget::default()).unwrap();
        let raw = records(&stream, ResourceBudget::default()).unwrap();
        assert!(raw.iter().any(|r| {
            r.id == 6
                && r.data
                    .windows(6)
                    .any(|b| b == [0x17, 3, 0, b'x', b'y', b'z'])
        }));
        let report = crate::read_workbook_bytes(bytes).unwrap();
        let cell = report
            .sheet(0)
            .unwrap()
            .get(CellAddress::new(0, 0).unwrap())
            .unwrap();
        assert_eq!(cell.formula.as_deref(), Some("UPPER(\"xyz\")"));
    }
    #[test]
    fn independent_poi_unicode_name_fixture_actual_import() {
        let bytes = include_bytes!("../tests/fixtures/poi-unicodeNameRecord.xls");
        let report = crate::read_workbook_bytes(bytes).unwrap();
        assert!(!report.sheets().is_empty());
    }
    #[test]
    fn unicode_string_token_raw_units_and_actual_import() {
        // Independent BIFF StringPtg: two UTF16 units, surrogate pair U+1F600.
        let tokens = [0x17, 2, 1, 0x3d, 0xd8, 0, 0xde];
        let bytes = compound(&tokens, None);
        let original = workbook_stream(&bytes, ResourceBudget::default()).unwrap();
        let rs = records(&original, ResourceBudget::default()).unwrap();
        let source = rs.iter().find(|r| r.id == 6).unwrap();
        assert_eq!(&source.data[22..], &tokens);
        let report = crate::read_workbook_bytes(&bytes).unwrap();
        let cell = report
            .sheet(0)
            .unwrap()
            .get(CellAddress::new(0, 0).unwrap())
            .unwrap();
        assert_eq!(cell.formula.as_deref(), Some("\"😀\""));
        assert_eq!(cell.value, CellValue::Number(1.));
        assert_eq!(
            workbook_stream(&bytes, ResourceBudget::default()).unwrap(),
            original
        );
    }
    #[test]
    fn original_sst_labelsst_lone_units_and_pairs_survive_import() {
        let seed = compound(&[0x1e, 1, 0], None);
        let old = workbook_stream(&seed, ResourceBudget::default()).unwrap();
        let rs = records(&old, ResourceBudget::default()).unwrap();
        let mut stream = Vec::new();
        let mut global = true;
        let mut bound_at = 0;
        let mut sheet_at = 0;
        for r in rs {
            if global && r.id == 0x85 {
                bound_at = stream.len() + 4;
            }
            if global && r.id == 0xa {
                // Two strings: a lone high unit and a valid pair.
                record(
                    &mut stream,
                    0xfc,
                    &[
                        2, 0, 0, 0, 2, 0, 0, 0, 1, 0, 1, 0x3d, 0xd8, 2, 0, 1, 0x3d, 0xd8, 0, 0xde,
                    ],
                );
                record(&mut stream, r.id, r.data);
                sheet_at = stream.len() as u32;
                global = false;
                continue;
            }
            if r.id == 6 {
                record(&mut stream, 0xfd, &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
                record(&mut stream, 0xfd, &[0, 0, 1, 0, 0, 0, 1, 0, 0, 0]);
            } else {
                record(&mut stream, r.id, r.data);
            }
        }
        stream[bound_at..bound_at + 4].copy_from_slice(&sheet_at.to_le_bytes());
        let mut c =
            cfb::CompoundFile::create_with_version(cfb::Version::V3, Cursor::new(Vec::new()))
                .unwrap();
        c.create_stream("/Workbook")
            .unwrap()
            .write_all(&stream)
            .unwrap();
        let bytes = c.into_inner().into_inner();
        let report = crate::read_workbook_bytes(&bytes).unwrap();
        for (column, units) in [(0, vec![0xd83d]), (1, vec![0xd83d, 0xde00])] {
            let cell = report
                .sheet(0)
                .unwrap()
                .get(CellAddress::new(0, column).unwrap())
                .unwrap();
            assert_eq!(
                cell.value.text_ref().unwrap().units().collect::<Vec<_>>(),
                units
            );
        }
        assert_eq!(
            workbook_stream(&bytes, ResourceBudget::default()).unwrap(),
            stream
        );
    }
    #[test]
    fn original_cached_string_lone_unit_survives_backend_projection() {
        let bytes = compound(&[0x1e, 1, 0], Some((&[2, 0, 1, 0x3d, 0xd8], &[1, 65, 0])));
        let original = workbook_stream(&bytes, ResourceBudget::default()).unwrap();
        let report = crate::read_workbook_bytes(&bytes).unwrap();
        let value = &report
            .sheet(0)
            .unwrap()
            .get(CellAddress::new(0, 0).unwrap())
            .unwrap()
            .value;
        assert_eq!(
            value.text_ref().unwrap().units().collect::<Vec<_>>(),
            vec![0xd83d, 65]
        );
        assert_eq!(
            workbook_stream(&bytes, ResourceBudget::default()).unwrap(),
            original
        );
        let budget = ResourceBudget {
            max_cell_text_bytes: 3,
            ..ResourceBudget::default()
        };
        assert!(matches!(
            crate::read_workbook_bytes_with_options(
                &bytes,
                crate::ReadOptions {
                    xls_budget: budget,
                    ..crate::ReadOptions::default()
                }
            ),
            Err(crate::IoError::BiffPreflight(
                PreflightError::Resource { .. }
            ))
        ));
    }
    #[test]
    fn cache_continuation_switch_preserves_all_codepoints() {
        let bytes = compound(
            &[0x1e, 1, 0],
            Some((&[4, 0, 0, b'a'], &[1, 0x3d, 0xd8, 0, 0xde, 0xe9, 0])),
        );
        let report = crate::read_workbook_bytes(&bytes).unwrap();
        let cell = report
            .sheet(0)
            .unwrap()
            .get(CellAddress::new(0, 0).unwrap())
            .unwrap();
        assert_eq!(cell.value, CellValue::Text("a😀é".into()));
        assert_eq!(cell.formula.as_deref(), Some("1"));
    }
    #[test]
    fn long_unicode_cached_string_survives_actual_import() {
        let expected = "a😀é".repeat(2000);
        let units: Vec<u16> = expected.encode_utf16().collect();
        let mut first = Vec::new();
        first.extend((units.len() as u16).to_le_bytes());
        first.push(1);
        let mut next = vec![1];
        for unit in &units[..4000] {
            first.extend(unit.to_le_bytes());
        }
        for unit in &units[4000..] {
            next.extend(unit.to_le_bytes());
        }
        assert!(first.len() <= 8224 && next.len() <= 8224);
        let bytes = compound(&[0x1e, 1, 0], Some((&first, &next)));
        let source = workbook_stream(&bytes, ResourceBudget::default()).unwrap();
        let rs = records(&source, ResourceBudget::default()).unwrap();
        assert_eq!(rs.iter().find(|r| r.id == 0x207).unwrap().data, &first);
        assert_eq!(rs.iter().find(|r| r.id == 0x3c).unwrap().data, &next);
        let report = crate::read_workbook_bytes(&bytes).unwrap();
        assert_eq!(
            report
                .sheet(0)
                .unwrap()
                .get(CellAddress::new(0, 0).unwrap())
                .unwrap()
                .value,
            CellValue::Text(expected)
        );
    }
    #[test]
    fn independent_area_flags_and_quoted_xti_are_reconstructed() {
        let names = [];
        let sheets = ["O'Brien Data".into()];
        let xtis = [(0, 0, 0)];
        let budget = ResourceBudget::default();
        assert_eq!(
            formula(
                &[0x25, 0, 0, 1, 0, 0, 0xc0, 1, 0x40],
                &sheets,
                &names,
                &xtis,
                budget
            )
            .unwrap(),
            Some("A1:B$2".into())
        );
        assert_eq!(
            formula(&[0x3a, 0, 0, 0, 0, 0, 0], &sheets, &names, &xtis, budget).unwrap(),
            Some("'O''Brien Data'!$A$1".into())
        );
    }
    #[test]
    fn truncated_tokens_and_string_continuations_are_typed_errors() {
        for token in [&[0x17, 2, 1, 0x3d][..], &[0x25, 0, 0][..], &[3][..]] {
            assert!(matches!(
                crate::read_workbook_bytes(&compound(token, None)),
                Err(crate::IoError::BiffPreflight(PreflightError::Malformed(_)))
            ));
        }
        let bytes = compound(&[0x1e, 1, 0], Some((&[4, 0, 1, 0x3d, 0xd8], &[1, 0, 0xde])));
        assert!(matches!(
            crate::read_workbook_bytes(&bytes),
            Err(crate::IoError::BiffPreflight(PreflightError::Malformed(_)))
        ));
    }
}
