//! Ordered global declarations, name formulas and sheet reconstruction.
use crate::xls::cached_text::cached_text;
use crate::xls::cells::scalar_values;
use crate::xls::formula::{AddIns, ParsedFormula, formula_with_addins};
use crate::xls::preflight::{PreflightError, Record, ResourceBudget, Substream};
use crate::xls::result::Result;
use crate::xls::sheet_formula::SheetFormula;
use crate::xls::sst::sst;
use crate::xls::text::{take, text, units, word};
use crate::{CellAddress, CellValue, ExcelText, NamedRange};
use std::collections::BTreeMap;

fn charge(total: &mut u64, bytes: usize, limit: u64, resource: crate::BiffResource) -> Result<()> {
    *total = total
        .checked_add(bytes as u64)
        .ok_or(PreflightError::Representation(
            crate::BiffRepresentationError::BiffTextTotalOverflow,
        ))?;
    if *total > limit {
        return Err(PreflightError::Resource {
            resource,
            requested: *total,
            limit,
        });
    }
    Ok(())
}

/// BIFF source recovery plan applied around backend decoding.
pub(super) struct Reconstruction {
    /// Source worksheet spellings in index order.
    pub(super) sheets: Vec<String>,
    /// DATEMODE selects the 1904 calendar.
    pub(super) date1904: bool,
    /// Recovered workbook and sheet-scoped defined names.
    pub(super) names: Vec<NamedRange>,
    /// Decoded numeric, boolean, error and empty-text cell values.
    pub(super) values: Vec<(usize, CellAddress, CellValue)>,
    /// Recovered formula source assigned to sheet coordinates.
    pub(super) formulas: Vec<SheetFormula>,
    /// Exact UTF-16 formula text caches to restore after decoding.
    pub(super) caches: Vec<(usize, CellAddress, ExcelText)>,
    /// Fidelity omissions retained for the import report.
    pub(super) warnings: Vec<String>,
}

/// Inspect checked BIFF records and plan source recovery without mutating the workbook.
/// `starts` are preflight's admitted sheet traversals, one per BoundSheet.
pub(super) fn reconstruct(
    rs: &[Record<'_>],
    starts: &[usize],
    budget: ResourceBudget,
) -> Result<Reconstruction> {
    let mut sheets = Vec::new();
    let mut names = Vec::new();
    let mut name_tokens = Vec::new();
    let mut xtis = Vec::new();
    let mut addins = AddIns::default();
    let mut warnings = Vec::new();
    let mut date1904 = false;
    let mut formula_bytes = 0u64;
    let mut cache_bytes = 0u64;
    for r in rs.iter().take_while(|r| r.id != 0xa) {
        addins.observe(r.id, r.data, budget)?;
        match r.id {
            0x22 => {
                let mut mode = r.data;
                date1904 = word(&mut mode)? == 1;
            }
            0x42 => {
                let mut codepage = r.data;
                if word(&mut codepage)? != 1200 {
                    warnings.push("BIFF8 CODEPAGE differs from UTF16; raw sheet/name/formula/cell/cache text units are kept".into());
                }
            }
            0x85 => {
                let mut b = r.data;
                take(&mut b, 6)?;
                let n = *take(&mut b, 1)?.first().ok_or(PreflightError::Malformed(
                    crate::BiffStructureError::MissingByte,
                ))? as usize;
                sheets.push(text(&mut b, n)?);
            }
            0x17 => {
                let mut b = r.data;
                let count = word(&mut b)? as usize;
                let total = xtis
                    .len()
                    .checked_add(count)
                    .ok_or(PreflightError::Representation(
                        crate::BiffRepresentationError::XtiCountOverflow,
                    ))?;
                if total > budget.max_metadata_entries {
                    return Err(PreflightError::Resource {
                        resource: crate::BiffResource::XtiDeclarations,
                        requested: total as u64,
                        limit: budget.max_metadata_entries as u64,
                    });
                }
                if b.len() != count * 6 {
                    return Err(PreflightError::Malformed(
                        crate::BiffStructureError::ExternsheetRecordLength,
                    ));
                }
                for _ in 0..count {
                    xtis.push((word(&mut b)?, word(&mut b)?, word(&mut b)?));
                }
            }
            0x18 => {
                let mut b = r.data;
                let flags = word(&mut b)?;
                take(&mut b, 1)?;
                let n = *take(&mut b, 1)?.first().ok_or(PreflightError::Malformed(
                    crate::BiffStructureError::MissingByte,
                ))? as usize;
                let tokens = word(&mut b)? as usize;
                take(&mut b, 2)?;
                let scope = word(&mut b)?;
                take(&mut b, 4)?;
                let decoded = text(&mut b, n)?;
                let name = if flags & 0x20 != 0 {
                    match decoded.chars().next().map(u32::from) {
                        Some(6) => "_xlnm.Print_Area".into(),
                        Some(7) => "_xlnm.Print_Titles".into(),
                        Some(13) => "_xlnm._FilterDatabase".into(),
                        _ => {
                            warnings.push("Unsupported built-in BIFF name identity retained as raw code point".into());
                            decoded
                        }
                    }
                } else {
                    decoded
                };
                charge(
                    &mut formula_bytes,
                    name.len(),
                    budget.max_string_bytes as u64,
                    crate::BiffResource::BiffFormulaNameBytes,
                )?;
                names.push(NamedRange {
                    name,
                    formula: String::new(),
                    sheet_scope: scope.checked_sub(1).map(usize::from),
                });
                let rgce = take(&mut b, tokens)?;
                name_tokens.push(ParsedFormula { rgce, rgcb: b });
            }
            _ => {}
        }
    }
    for (i, tokens) in name_tokens.iter().enumerate() {
        if let Some(f) =
            formula_with_addins(*tokens, &sheets, &names, &xtis, &addins, None, budget)?
        {
            charge(
                &mut formula_bytes,
                f.len(),
                budget.max_string_bytes as u64,
                crate::BiffResource::BiffFormulaNameBytes,
            )?;
            // SAFETY: names and name_tokens are pushed together locally; record cursors come from enumeration or checked binary search and are guarded by rs.len(). The four-byte conversion follows take(..., 4).
            names[i].formula = f;
        } else {
            warnings.push(format!(
                "Unsupported BIFF defined-name tokens for {}",
                // SAFETY: names and name_tokens are pushed together locally; record cursors come from enumeration or checked binary search and are guarded by rs.len(). The four-byte conversion follows take(..., 4).
                names[i].name
            ));
        }
    }
    let mut shared = Vec::new();
    // Only globals SST records are decoded, matching preflight's admission.
    for (at, _) in rs
        .iter()
        .enumerate()
        .take_while(|(_, r)| r.id != 0xa)
        .filter(|(_, r)| r.id == 0xfc)
    {
        // SAFETY: names and name_tokens are pushed together locally; record cursors come from enumeration or checked binary search and are guarded by rs.len(). The four-byte conversion follows take(..., 4).
        let end = rs[at + 1..]
            .iter()
            .position(|r| r.id != 0x3c)
            .map_or(rs.len(), |n| at + 1 + n);
        // SAFETY: names and name_tokens are pushed together locally; record cursors come from enumeration or checked binary search and are guarded by rs.len(). The four-byte conversion follows take(..., 4).
        shared = sst(&rs[at..end], budget)?;
    }
    let mut formulas = Vec::new();
    let mut caches = Vec::new();
    let mut values = Vec::new();
    if starts.len() != sheets.len() {
        return Err(PreflightError::Malformed(
            crate::BiffStructureError::SheetBoundary,
        ));
    }
    for (sheet, &start) in starts.iter().enumerate() {
        let mut substream = Substream::new(rs, start);
        let mut shared_formulas = BTreeMap::new();
        for r in substream.clone() {
            if r.id == 0x4bc {
                let mut b = r.data;
                let first_row = word(&mut b)?;
                let last_row = word(&mut b)?;
                let range = take(&mut b, 4)?;
                let n = word(&mut b)? as usize;
                // SAFETY: take(&mut b, 4) above returned exactly four bytes, so indexes 0 and 1 exist.
                let (first_col, last_col) = (u16::from(range[0]), u16::from(range[1]));
                let rgce = take(&mut b, n)?;
                shared_formulas.insert(
                    (first_row, first_col),
                    (last_row, last_col, ParsedFormula { rgce, rgcb: b }),
                );
            }
        }
        let mut pending = None;
        while let Some((i, r)) = substream.next_indexed() {
            if matches!(r.id, 0xfd | 0x204 | 0xd6) {
                let mut b = r.data;
                let row = word(&mut b)?;
                let col = word(&mut b)?;
                let address = CellAddress::new(row as u32, col as u32).map_err(|_| {
                    PreflightError::Representation(crate::BiffRepresentationError::TextCoordinate)
                })?;
                take(&mut b, 2)?;
                let text = if r.id == 0xfd {
                    let raw = take(&mut b, 4)?;
                    // SAFETY: take(&mut b, 4) above returned exactly four bytes, so the conversion succeeds.
                    let index = u32::from_le_bytes(raw.try_into().expect("four bytes")) as usize;
                    shared.get(index).cloned().ok_or(PreflightError::Malformed(
                        crate::BiffStructureError::SharedStringIndex,
                    ))?
                } else {
                    let n = word(&mut b)? as usize;
                    if n > budget.max_string_chars {
                        return Err(PreflightError::Resource {
                            resource: crate::BiffResource::LabelStringCharacters,
                            requested: n as u64,
                            limit: budget.max_string_chars as u64,
                        });
                    }
                    units(&mut b, n)?
                };
                charge(
                    &mut cache_bytes,
                    text.units().len() * 2,
                    budget.max_cell_text_bytes,
                    crate::BiffResource::BiffCachedTextBytes,
                )?;
                caches.push((sheet, address, text));
            }
            for (address, value) in scalar_values(r)? {
                values.push((sheet, address, value));
            }
            if r.id == 6 {
                let mut b = r.data;
                let row = word(&mut b)?;
                let col = word(&mut b)?;
                let address = CellAddress::new(row as u32, col as u32).map_err(|_| {
                    PreflightError::Representation(
                        crate::BiffRepresentationError::FormulaCoordinate,
                    )
                })?;
                take(&mut b, 16)?;
                let n = word(&mut b)? as usize;
                let tokens = ParsedFormula {
                    rgce: take(&mut b, n)?,
                    rgcb: b,
                };
                pending = if r.data.get(12..14) == Some(&[255, 255]) && r.data.get(6) == Some(&0) {
                    Some(address)
                } else {
                    None
                };
                // PtgExp alone names the first cell of a SHRFMLA (or ARRAY/TABLE)
                // range; shared tokens are relative to the formula's own cell.
                let decoded = match tokens.rgce {
                    [0x01, r1, r2, c1, c2] => {
                        let master = (
                            u16::from_le_bytes([*r1, *r2]),
                            u16::from_le_bytes([*c1, *c2]),
                        );
                        match shared_formulas.get(&master) {
                            Some((last_row, last_col, shared_tokens))
                                if (master.0..=*last_row).contains(&row)
                                    && (master.1..=*last_col).contains(&col) =>
                            {
                                formula_with_addins(
                                    *shared_tokens,
                                    &sheets,
                                    &names,
                                    &xtis,
                                    &addins,
                                    Some(address),
                                    budget,
                                )?
                            }
                            _ => None,
                        }
                    }
                    _ => {
                        formula_with_addins(tokens, &sheets, &names, &xtis, &addins, None, budget)?
                    }
                };
                if let Some(text) = decoded {
                    charge(
                        &mut formula_bytes,
                        text.len(),
                        budget.max_string_bytes as u64,
                        crate::BiffResource::BiffFormulaNameBytes,
                    )?;
                    formulas.push(SheetFormula {
                        sheet,
                        address,
                        text,
                    });
                } else {
                    warnings.push(format!(
                        "Unsupported BIFF formula tokens on sheet {sheet} at {address}"
                    ));
                }
            }
            if r.id == 0x207 {
                let address = pending.take().ok_or(PreflightError::Malformed(
                    crate::BiffStructureError::CachedStringWithoutFormula,
                ))?;
                let cached = cached_text(rs, i, budget)?;
                substream.skip_to(cached.last_record);
                let text = cached.text;
                charge(
                    &mut cache_bytes,
                    text.units().len() * 2,
                    budget.max_cell_text_bytes,
                    crate::BiffResource::BiffCachedTextBytes,
                )?;
                caches.push((sheet, address, text));
            }
        }
    }
    Ok(Reconstruction {
        sheets,
        date1904,
        names,
        values,
        formulas,
        caches,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    //! Original synthetic BIFF8 streams; record layouts follow [MS-XLS] section 2.4.
    use crate::data::{SheetSource, WorkbookSource};
    use crate::test_support::Workbook;
    use crate::{CellAddress, CellError, CellValue, DateSystem, IoError, PreflightError};
    use std::io::{Cursor, Write};

    fn record(id: u16, data: &[u8]) -> Vec<u8> {
        let mut out = id.to_le_bytes().to_vec();
        out.extend((data.len() as u16).to_le_bytes());
        out.extend(data);
        out
    }

    fn bof(kind: u16) -> Vec<u8> {
        let mut data = vec![0; 16];
        data[..2].copy_from_slice(&0x600u16.to_le_bytes());
        data[2..4].copy_from_slice(&kind.to_le_bytes());
        record(0x809, &data)
    }

    /// FORMULA record with a cached result and RPN tokens.
    fn formula(row: u16, col: u16, result: [u8; 8], tokens: &[u8]) -> Vec<u8> {
        formula_with_extra(row, col, result, tokens, &[])
    }

    /// FORMULA record whose RPN tokens are followed by RgbExtra data.
    fn formula_with_extra(
        row: u16,
        col: u16,
        result: [u8; 8],
        tokens: &[u8],
        extra: &[u8],
    ) -> Vec<u8> {
        let mut data = row.to_le_bytes().to_vec();
        data.extend(col.to_le_bytes());
        data.extend([0, 0]);
        data.extend(result);
        data.extend([0; 6]);
        data.extend((tokens.len() as u16).to_le_bytes());
        data.extend(tokens);
        data.extend(extra);
        record(6, &data)
    }

    /// PtgArray token (array class 0x60) with its seven unused bytes.
    const ARRAY: [u8; 8] = [0x60, 0, 0, 0, 0, 0, 0, 0];

    /// PtgFuncVar SUM (function 4) with `count` arguments.
    fn sum(count: u8) -> [u8; 4] {
        [0x42, count, 4, 0]
    }

    /// PtgExtraArray header (columns - 1, rows - 1) followed by SerAr values.
    fn array(columns: u8, rows: u16, values: &[&[u8]]) -> Vec<u8> {
        let mut out = vec![columns - 1];
        out.extend((rows - 1).to_le_bytes());
        for value in values {
            out.extend(*value);
        }
        out
    }

    fn ser_number(value: f64) -> Vec<u8> {
        let mut out = vec![0x01];
        out.extend(value.to_le_bytes());
        out
    }

    fn cell(id: u16, row: u16, col: u16, payload: &[u8]) -> Vec<u8> {
        let mut data = row.to_le_bytes().to_vec();
        data.extend(col.to_le_bytes());
        data.extend([0, 0]);
        data.extend(payload);
        record(id, &data)
    }

    /// One-worksheet compound file; `declared` controls BoundSheet's lbPlyPos.
    fn workbook(globals: &[u8], cells: &[u8], declared: bool) -> Vec<u8> {
        let mut stream = bof(5);
        stream.extend(globals);
        let bound = stream.len() + 4;
        let mut sheet = vec![0; 8];
        sheet[6] = 4;
        sheet.extend(b"Data");
        stream.extend(record(0x85, &sheet));
        stream.extend(record(0xa, &[]));
        if declared {
            let at = stream.len() as u32;
            stream[bound..bound + 4].copy_from_slice(&at.to_le_bytes());
        }
        stream.extend(bof(0x10));
        stream.extend(cells);
        stream.extend(record(0xa, &[]));
        let mut file =
            cfb::CompoundFile::create_with_version(cfb::Version::V3, Cursor::new(Vec::new()))
                .unwrap();
        file.create_stream("/Workbook")
            .unwrap()
            .write_all(&stream)
            .unwrap();
        file.into_inner().into_inner()
    }

    fn read(bytes: &[u8]) -> Workbook {
        crate::read_workbook_bytes::<Workbook>(bytes).unwrap()
    }

    fn at(book: &Workbook, reference: &str) -> crate::Cell {
        book.sheets()[0]
            .get(reference.parse::<CellAddress>().unwrap())
            .unwrap()
            .clone()
    }

    const TYPED: [u8; 2] = [0xff, 0xff];

    fn typed(kind: u8, value: u8) -> [u8; 8] {
        [kind, 0, value, 0, 0, 0, TYPED[0], TYPED[1]]
    }

    #[test]
    fn scalar_records_decode_numbers_rk_booleans_and_errors() {
        // RkNumber (MS-XLS 2.5.217): bit 0 divides by 100, bit 1 selects a
        // signed 30-bit integer, otherwise the upper 30 bits of an IEEE double.
        let mut cells = cell(0x203, 0, 0, &1.5f64.to_le_bytes());
        cells.extend(cell(0x27e, 0, 1, &((123u32 << 2) | 2).to_le_bytes()));
        cells.extend(cell(0x27e, 0, 2, &((12345u32 << 2) | 3).to_le_bytes()));
        cells.extend(cell(
            0x27e,
            0,
            3,
            &(((-4i32) << 2) as u32 | 2).to_le_bytes(),
        ));
        // MULRK: row, first column, (ixfe, RK) pairs, last column.
        let mut mulrk = 1u16.to_le_bytes().to_vec();
        mulrk.extend(0u16.to_le_bytes());
        for value in [0x3ff0_0000u32, (7 << 2) | 2] {
            mulrk.extend([0, 0]);
            mulrk.extend(value.to_le_bytes());
        }
        mulrk.extend(1u16.to_le_bytes());
        cells.extend(record(0xbd, &mulrk));
        cells.extend(cell(0x205, 2, 0, &[1, 0]));
        cells.extend(cell(0x205, 2, 1, &[0x2a, 1]));
        cells.extend(cell(0x201, 2, 2, &[]));
        let book = read(&workbook(&[], &cells, true));
        for (reference, expected) in [
            ("A1", CellValue::Number(1.5)),
            ("B1", CellValue::Number(123.)),
            ("C1", CellValue::Number(123.45)),
            ("D1", CellValue::Number(-4.)),
            ("A2", CellValue::Number(1.)),
            ("B2", CellValue::Number(7.)),
            ("A3", CellValue::Boolean(true)),
            ("B3", CellValue::Error(CellError::NA)),
        ] {
            assert_eq!(at(&book, reference).value, expected, "{reference}");
        }
        // BLANK records carry formatting only; blanks are not stored.
        assert!(book.sheets()[0].get("C3".parse().unwrap()).is_none());
        assert_eq!(book.date_system(), DateSystem::Excel1900);
    }

    #[test]
    fn formula_caches_keep_typed_results_and_datemode_selects_1904() {
        let mut cells = formula(0, 0, 2.5f64.to_le_bytes(), &[0x1e, 2, 0]);
        cells.extend(formula(1, 0, typed(1, 1), &[0x1d, 1]));
        cells.extend(formula(2, 0, typed(2, 0x07), &[0x1c, 0x07]));
        cells.extend(formula(3, 0, typed(3, 0), &[0x17, 0, 0]));
        let book = read(&workbook(&record(0x22, &[1, 0]), &cells, true));
        assert_eq!(book.date_system(), DateSystem::Excel1904);
        for (reference, formula, value) in [
            ("A1", "2", CellValue::Number(2.5)),
            ("A2", "TRUE", CellValue::Boolean(true)),
            ("A3", "#DIV/0!", CellValue::Error(CellError::Div0)),
            ("A4", "\"\"", CellValue::Text(String::new())),
        ] {
            let cell = at(&book, reference);
            assert_eq!(cell.formula.as_deref(), Some(formula), "{reference}");
            assert_eq!(cell.value, value, "{reference}");
        }
    }

    #[test]
    fn shared_formulas_translate_relative_tokens_for_each_cell() {
        // A2:A3 share `A1+1` relative to A2: PtgRefN row offset -1 with both
        // relative flags, PtgInt 1 and PtgAdd (MS-XLS 2.4.260 ShrFmla).
        let exp = [0x01, 1, 0, 0, 0];
        let mut cells = formula(1, 0, 2f64.to_le_bytes(), &exp);
        let tokens = [0x2c, 0xff, 0xff, 0, 0xc0, 0x1e, 1, 0, 0x03];
        let mut shared = vec![1, 0, 2, 0, 0, 0, 0, 2];
        shared.extend((tokens.len() as u16).to_le_bytes());
        shared.extend(tokens);
        cells.extend(record(0x4bc, &shared));
        cells.extend(formula(2, 0, 3f64.to_le_bytes(), &exp));
        // A follower outside the shared range keeps its value without source.
        cells.extend(formula(4, 0, 9f64.to_le_bytes(), &exp));
        let book = read(&workbook(&[], &cells, true));
        assert_eq!(at(&book, "A2").formula.as_deref(), Some("A1+1"));
        assert_eq!(at(&book, "A3").formula.as_deref(), Some("A2+1"));
        assert_eq!(at(&book, "A3").value, CellValue::Number(3.));
        let outside = at(&book, "A5");
        assert_eq!(
            (outside.formula, outside.value),
            (None, CellValue::Number(9.))
        );
        assert!(
            book.file_metadata
                .unwrap()
                .import_warnings
                .iter()
                .any(|w| w.detail == "Unsupported BIFF formula tokens on sheet 0 at A5")
        );
    }

    #[test]
    fn array_constants_keep_formula_source() {
        // =SUM({1,2}), ={1,2;3,4} and ={"a",TRUE,#N/A}: PtgArray tokens with
        // their PtgExtraArray values after rgce in the FORMULA record.
        let mut tokens = ARRAY.to_vec();
        tokens.extend(sum(1));
        let one_two = [ser_number(1.), ser_number(2.)];
        let mut cells = formula_with_extra(
            0,
            0,
            3f64.to_le_bytes(),
            &tokens,
            &array(2, 1, &[&one_two[0], &one_two[1]]),
        );
        let grid: Vec<_> = (1..=4).map(|v| ser_number(f64::from(v))).collect();
        cells.extend(formula_with_extra(
            1,
            0,
            1f64.to_le_bytes(),
            &ARRAY,
            &array(2, 2, &[&grid[0], &grid[1], &grid[2], &grid[3]]),
        ));
        let mixed = array(
            3,
            1,
            &[
                &[0x02, 1, 0, 0, b'a'],
                &[0x04, 1, 0, 0, 0, 0, 0, 0, 0],
                &[0x10, 0x2a, 0, 0, 0, 0, 0, 0, 0],
            ],
        );
        cells.extend(formula_with_extra(2, 0, typed(0, 0), &ARRAY, &mixed));
        let book = read(&workbook(&[], &cells, true));
        for (reference, source) in [
            ("A1", "SUM({1,2})"),
            ("A2", "{1,2;3,4}"),
            ("A3", "{\"a\",TRUE,#N/A}"),
        ] {
            assert_eq!(
                at(&book, reference).formula.as_deref(),
                Some(source),
                "{reference}"
            );
        }
        assert_eq!(at(&book, "A1").value, CellValue::Number(3.));
        assert!(
            !book
                .file_metadata
                .unwrap()
                .import_warnings
                .iter()
                .any(|w| w.detail.starts_with("Unsupported BIFF formula tokens"))
        );
    }

    #[test]
    fn shared_formulas_and_defined_names_read_array_constants_after_their_tokens() {
        // A2:A3 share SUM(A1,{1,2}) relative to A2; the SHRFMLA record holds
        // the PtgExtraArray after its rgce. NAME `Pair` is ={1,2}.
        let exp = [0x01, 1, 0, 0, 0];
        let extra = array(2, 1, &[&ser_number(1.), &ser_number(2.)]);
        let mut tokens = vec![0x2c, 0xff, 0xff, 0, 0xc0];
        tokens.extend(ARRAY);
        tokens.extend(sum(2));
        let mut shared = vec![1, 0, 2, 0, 0, 0, 0, 2];
        shared.extend((tokens.len() as u16).to_le_bytes());
        shared.extend(&tokens);
        shared.extend(&extra);
        let mut cells = formula(1, 0, 3f64.to_le_bytes(), &exp);
        cells.extend(record(0x4bc, &shared));
        cells.extend(formula(2, 0, 3f64.to_le_bytes(), &exp));
        // NAME: flags, chKey, cch, cce, reserved, itab (0 = workbook), four
        // empty optional-string lengths, the name, rgce and its RgbExtra.
        let mut name = vec![0, 0, 0, 4];
        name.extend((ARRAY.len() as u16).to_le_bytes());
        name.extend([0, 0, 0, 0, 0, 0, 0, 0]);
        name.push(0);
        name.extend(b"Pair");
        name.extend(ARRAY);
        name.extend(&extra);
        let book = read(&workbook(&record(0x18, &name), &cells, true));
        assert_eq!(at(&book, "A2").formula.as_deref(), Some("SUM(A1,{1,2})"));
        assert_eq!(at(&book, "A3").formula.as_deref(), Some("SUM(A2,{1,2})"));
        assert_eq!(book.named_ranges.len(), 1);
        assert_eq!(book.named_ranges[0].name, "Pair");
        assert_eq!(book.named_ranges[0].formula, "{1,2}");
    }

    #[test]
    fn truncated_or_oversized_array_constants_are_malformed() {
        let full = array(2, 1, &[&ser_number(1.), &ser_number(2.)]);
        // Missing RgbExtra, a missing second value, a split value, and the
        // largest declared shape (256 x 65,536) with no values.
        for extra in [&[][..], &full[..12], &full[..15], &[0xff, 0xff, 0xff][..]] {
            let cells = formula_with_extra(0, 0, 3f64.to_le_bytes(), &ARRAY, extra);
            assert!(
                matches!(
                    crate::read_workbook_bytes::<Workbook>(&workbook(&[], &cells, true)),
                    Err(IoError::BiffPreflight(PreflightError::Malformed(_)))
                ),
                "{extra:?}"
            );
        }
    }

    #[test]
    fn deleted_reference_tokens_render_ref_errors() {
        let mut cells = formula(0, 0, typed(2, 0x17), &[0x2a, 0, 0, 0, 0]);
        cells.extend(formula(
            1,
            0,
            typed(2, 0x17),
            &[0x4b, 0, 0, 0, 0, 0, 0, 0, 0, 0x1e, 1, 0, 0x03],
        ));
        let book = read(&workbook(&[], &cells, true));
        assert_eq!(at(&book, "A1").formula.as_deref(), Some("#REF!"));
        assert_eq!(at(&book, "A2").formula.as_deref(), Some("#REF!+1"));
        assert_eq!(at(&book, "A2").value, CellValue::Error(CellError::Ref));
    }

    #[test]
    fn unset_sheet_offsets_use_stream_order_and_nested_substreams_are_skipped() {
        // An embedded chart substream sits between worksheet cells; its records
        // belong to the chart, and its EOF does not end the worksheet.
        let mut cells = cell(0x203, 0, 0, &1f64.to_le_bytes());
        cells.extend(bof(0x20));
        cells.extend(cell(0x203, 5, 5, &9f64.to_le_bytes()));
        cells.extend(record(0xa, &[]));
        cells.extend(cell(0x203, 1, 0, &2f64.to_le_bytes()));
        for declared in [true, false] {
            let book = read(&workbook(&[], &cells, declared));
            let sheet = &book.sheets()[0];
            assert_eq!(sheet.name(), "Data");
            assert_eq!(at(&book, "A1").value, CellValue::Number(1.));
            assert_eq!(at(&book, "A2").value, CellValue::Number(2.));
            assert!(sheet.get("F6".parse().unwrap()).is_none());
        }
    }

    /// ROW record (MS-XLS 2.4.221) marking `row` as explicitly present.
    fn row(row: u16) -> Vec<u8> {
        let mut data = vec![0; 16];
        data[..2].copy_from_slice(&row.to_le_bytes());
        record(0x208, &data)
    }

    fn read_with(bytes: &[u8], budget: crate::ResourceBudget) -> Result<Workbook, IoError> {
        crate::read_workbook_bytes_with_options::<Workbook>(
            bytes,
            crate::ReadOptions {
                xls_budget: budget,
                ..crate::ReadOptions::default()
            },
        )
    }

    #[test]
    fn duplicate_sheet_starts_after_a_nested_substream_are_charged_per_traversal() {
        // 200 BoundSheets share one worksheet whose first records are an empty
        // embedded chart substream, followed by 20,000 NUMBER cells.
        let mut stream = bof(5);
        let mut patches = Vec::new();
        for i in 0..200 {
            patches.push(stream.len() + 4);
            let name = format!("S{i}");
            let mut sheet = vec![0; 8];
            sheet[6] = name.len() as u8;
            sheet.extend(name.as_bytes());
            stream.extend(record(0x85, &sheet));
        }
        stream.extend(record(0xa, &[]));
        let at = stream.len() as u32;
        for patch in patches {
            stream[patch..patch + 4].copy_from_slice(&at.to_le_bytes());
        }
        stream.extend(bof(0x10));
        stream.extend(bof(0x20));
        stream.extend(record(0xa, &[]));
        for i in 0..20_000u32 {
            stream.extend(cell(
                0x203,
                (i / 100) as u16,
                (i % 100) as u16,
                &1f64.to_le_bytes(),
            ));
        }
        stream.extend(record(0xa, &[]));
        let mut file =
            cfb::CompoundFile::create_with_version(cfb::Version::V3, Cursor::new(Vec::new()))
                .unwrap();
        file.create_stream("/Workbook")
            .unwrap()
            .write_all(&stream)
            .unwrap();
        let bytes = file.into_inner().into_inner();
        let started = std::time::Instant::now();
        let result = read_with(&bytes, crate::ResourceBudget::default().max_records(50_000));
        // Each traversal reads the nested BOF/EOF pair and 20,000 cells, so the
        // third traversal exceeds the budget.
        assert!(matches!(
            result,
            Err(IoError::BiffPreflight(PreflightError::Resource {
                resource: crate::BiffResource::BiffScanWork,
                requested: 50_001,
                limit: 50_000,
            }))
        ));
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        // Decoded cells are charged on every traversal too.
        let cells = crate::ResourceBudget::default().max_cells(30_000);
        assert!(matches!(
            read_with(&bytes, cells),
            Err(IoError::BiffPreflight(PreflightError::Resource {
                resource: crate::BiffResource::DecodedCells,
                requested: 30_001,
                limit: 30_000,
            }))
        ));
    }

    #[test]
    fn decoded_cell_budget_counts_each_mulrk_cell() {
        // MULRK at A1:C1 (three RK pairs), then one NUMBER at A2.
        let mut mulrk = 0u16.to_le_bytes().to_vec();
        mulrk.extend(0u16.to_le_bytes());
        for _ in 0..3 {
            mulrk.extend([0, 0]);
            mulrk.extend(0x3ff0_0000u32.to_le_bytes());
        }
        mulrk.extend(2u16.to_le_bytes());
        let mut cells = record(0xbd, &mulrk);
        cells.extend(cell(0x203, 1, 0, &2f64.to_le_bytes()));
        let bytes = workbook(&[], &cells, true);
        assert!(matches!(
            read_with(&bytes, crate::ResourceBudget::default().max_cells(3)),
            Err(IoError::BiffPreflight(PreflightError::Resource {
                resource: crate::BiffResource::DecodedCells,
                requested: 4,
                limit: 3,
            }))
        ));
        let book = read_with(&bytes, crate::ResourceBudget::default().max_cells(4)).unwrap();
        assert_eq!(book.sheets()[0].cells().count(), 4);
    }

    #[test]
    fn rows_after_nested_substreams_and_on_unset_offsets_are_recovered() {
        let mut cells = row(0);
        cells.extend(bof(0x20));
        cells.extend(row(3));
        cells.extend(record(0xa, &[]));
        cells.extend(row(7));
        for declared in [true, false] {
            let book = read(&workbook(&[], &cells, declared));
            assert_eq!(
                book.sheets()[0].row_indices().collect::<Vec<_>>(),
                vec![0, 7],
                "declared={declared}"
            );
        }
    }

    #[test]
    fn encrypted_workbook_with_a_shared_string_table_is_refused_as_encrypted() {
        // FILEPASS (RC4) precedes an SST whose payload is ciphertext and does
        // not parse as a string table.
        let mut globals = record(0x2f, &[1, 0, 1, 0, 1, 0]);
        let mut sst = 1u32.to_le_bytes().to_vec();
        sst.extend(1u32.to_le_bytes());
        sst.extend([0x7f, 0x13, 0x9c]);
        globals.extend(record(0xfc, &sst));
        let bytes = workbook(&globals, &cell(0xfd, 0, 0, &[0; 4]), true);
        assert!(matches!(
            crate::read_workbook_bytes::<Workbook>(&bytes),
            Err(IoError::BiffPreflight(PreflightError::Representation(
                crate::BiffRepresentationError::EncryptedXls
            )))
        ));
    }

    #[test]
    fn biff5_and_encrypted_workbooks_are_refused() {
        let mut biff5 = workbook(&[], &[], true);
        let at = biff5
            .windows(4)
            .position(|w| w == [0x09, 0x08, 16, 0])
            .unwrap();
        biff5[at + 4] = 0;
        biff5[at + 5] = 5;
        let encrypted = workbook(&record(0x2f, &[0, 0, 0, 0, 0, 0]), &[], true);
        for (bytes, detail) in [
            (
                biff5,
                crate::BiffRepresentationError::UnsupportedBiffVersion,
            ),
            (encrypted, crate::BiffRepresentationError::EncryptedXls),
        ] {
            assert!(matches!(
                crate::read_workbook_bytes::<Workbook>(&bytes),
                Err(IoError::BiffPreflight(PreflightError::Representation(found))) if found == detail
            ));
        }
    }
}
