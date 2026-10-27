//! Recover array cache ownership so derived saved values cannot become authored
//! spill blockers after reopening. Walk stored children, never expand a declared range.
use super::xml::{attr, events, text, xml};
use crate::IoError;
use crate::data::{SheetSink, SheetSource, WorkbookSink, WorkbookSource};
use crate::{Cell, CellAddress, CellStyle, CellValue};
use quick_xml::events::Event;
use std::collections::BTreeMap;

/// Preserve declared source extents, falling back to sparse child bounds for legacy
/// manually attached caches. The export validator rejects authored holes.
pub(crate) fn bounds(
    sheet: &impl SheetSource,
) -> Result<BTreeMap<CellAddress, CellAddress>, IoError> {
    let mut ranges: BTreeMap<_, _> = sheet.spill_extents().collect();
    for (address, _) in sheet.cells() {
        if let Some(owner) = sheet.spill_owner(*address) {
            if owner.row > address.row
                || owner.col > address.col
                || sheet.get(owner).is_none_or(|c| c.formula.is_none())
            {
                return Err(IoError::Unsupported(
                    crate::WorkbookFeatureError::InvalidSpillCacheAnchorGeometry,
                ));
            }
            if sheet
                .spill_extent(owner)
                .is_some_and(|last| address.row > last.row || address.col > last.col)
            {
                return Err(IoError::Unsupported(
                    crate::WorkbookFeatureError::SpillChildLiesOutsideDeclaredExtent,
                ));
            }
            let last = ranges.entry(owner).or_insert(owner);
            last.row = last.row.max(address.row);
            last.col = last.col.max(address.col);
        }
    }
    Ok(ranges)
}
/// Check sparse rectangular ownership using a row sweep without expanding cells.
pub(super) fn validate_disjoint_ranges(
    arrays: &mut [(CellAddress, CellAddress)],
) -> Result<(), IoError> {
    // SAFETY: The comparator keys constructed here contain only primitive u32
    // fields, whose tuple order is total for every possible input coordinate.
    arrays.sort_unstable_by_key(|(anchor, last)| (anchor.row, anchor.col, last.row, last.col));
    let mut active = BTreeMap::<u32, u32>::new();
    let mut endings = BTreeMap::<u32, Vec<u32>>::new();
    for &(anchor, last) in arrays.iter() {
        while endings
            .first_key_value()
            .is_some_and(|(end, _)| *end <= anchor.row)
        {
            if let Some((_, cols)) = endings.pop_first() {
                for col in cols {
                    active.remove(&col);
                }
            }
        }
        // SAFETY: Both queries have one unbounded endpoint, so their bounds
        // cannot be reversed or equal excluded bounds for any u32 column.
        if active
            .range(..=anchor.col)
            .next_back()
            .is_some_and(|(_, end)| *end >= anchor.col)
            || active
                .range(anchor.col..)
                .next()
                .is_some_and(|(start, _)| *start <= last.col)
        {
            return Err(IoError::Read(
                crate::WorkbookReadError::OverlappingArrayFormulaCacheRanges,
            ));
        }
        active.insert(anchor.col, last.col);
        endings
            .entry(last.row.saturating_add(1))
            .or_default()
            .push(anchor.col);
    }
    Ok(())
}
/// Refuse to claim unrelated authored content or another anchor's caches inside
/// a declared XLSX array rectangle. Each disjoint range uses a sparse stored query.
pub(crate) fn validate_spills(workbook: &impl WorkbookSource) -> Result<(), IoError> {
    for sheet in workbook.sheets() {
        let mut ranges = bounds(sheet)?.into_iter().collect::<Vec<_>>();
        validate_disjoint_ranges(&mut ranges)?;
        for (anchor, last) in ranges {
            for (address, cell) in sheet.stored_range(anchor, last) {
                if *address == anchor || sheet.spill_owner(*address) == Some(anchor) {
                    continue;
                }
                if sheet.spill_owner(*address).is_some()
                    || cell.formula.is_some()
                    || cell.value != CellValue::Blank
                {
                    return Err(IoError::Unsupported(
                        crate::WorkbookFeatureError::AuthoredSpillContent {
                            sheet: sheet.name().into(),
                            address: *address,
                        },
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Recover one worksheet's array ownership and modern error caches.
pub(crate) fn apply_sheet_spills(
    bytes: &[u8],
    name: &str,
    workbook: &mut impl WorkbookSink<Sheet: SheetSink>,
) -> Result<(), IoError> {
    let sheet = workbook.sheet_by_name_mut(name).ok_or(IoError::Read(
        crate::WorkbookReadError::MissingArrayWorksheet,
    ))?;
    let mut reader = events(bytes);
    let mut address = None;
    let mut arrays = Vec::new();
    let mut error_cell = false;
    let mut next_row = 0u32;
    let mut row = 0u32;
    let mut next_col = 0u32;
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(e) if e.local_name().as_ref() == "row" => {
                row = match attr(&e, "r")? {
                    Some(r) => r
                        .parse::<u32>()
                        .map_err(xml)?
                        .checked_sub(1)
                        .ok_or(IoError::Read(crate::WorkbookReadError::ZeroRowIndex))?,
                    None => next_row,
                };
                next_row = row
                    .checked_add(1)
                    .ok_or(IoError::Read(crate::WorkbookReadError::RowOverflow))?;
                next_col = 0;
            }
            Event::Start(e) if e.local_name().as_ref() == "c" => {
                error_cell = attr(&e, "t")?.as_deref() == Some("e");
                let at = match attr(&e, "r")? {
                    Some(s) => s.parse::<CellAddress>()?,
                    None => CellAddress::new(row, next_col)?,
                };
                next_col = at
                    .col
                    .checked_add(1)
                    .ok_or(IoError::Read(crate::WorkbookReadError::ColumnOverflow))?;
                address = Some(at);
            }
            Event::End(e) if e.local_name().as_ref() == "c" => address = None,
            Event::Start(e)
                if e.local_name().as_ref() == "f" && attr(&e, "t")?.as_deref() == Some("array") =>
            {
                if let (Some(anchor), Some(range)) = (address, attr(&e, "ref")?) {
                    let (first, last) = range.split_once(':').unwrap_or((&range, &range));
                    let first = first.parse::<CellAddress>()?;
                    let last = last.parse::<CellAddress>()?;
                    if first != anchor || last.row < first.row || last.col < first.col {
                        return Err(IoError::Read(
                            crate::WorkbookReadError::InvalidArrayFormulaCacheRange,
                        ));
                    }
                    arrays.push((anchor, last));
                }
            }
            Event::Start(e) if error_cell && e.local_name().as_ref() == "v" => {
                let raw = text(&mut reader, e.name())?;
                if matches!(raw.as_str(), "#SPILL!" | "#CALC!") {
                    let value = raw
                        .parse::<crate::CellError>()
                        .map_err(crate::ModelError::from)?;
                    let cell = address.and_then(|at| sheet.get_mut(at)).ok_or({
                        IoError::Read(crate::WorkbookReadError::MissingModernErrorCell)
                    })?;
                    cell.value = CellValue::Error(value);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    validate_disjoint_ranges(&mut arrays)?;
    let sheet_index = workbook
        .sheets()
        .iter()
        .position(|sheet| sheet.name() == name)
        .ok_or(IoError::Read(
            crate::WorkbookReadError::MissingArrayWorksheet,
        ))?;
    let mut replacements = Vec::new();
    let mut publication = Vec::new();
    for (anchor, last) in arrays {
        let formula = workbook
            .sheets()
            .get(sheet_index)
            .and_then(|s| s.get(anchor))
            .and_then(|c| c.formula.clone())
            .ok_or(IoError::Read(
                crate::WorkbookReadError::ArrayCacheHasNoFormulaAnchor,
            ))?;
        let mut children = Vec::new();
        for (at, child) in workbook
            .sheets()
            .get(sheet_index)
            .ok_or(IoError::Read(
                crate::WorkbookReadError::MissingArrayWorksheet,
            ))?
            .stored_range(anchor, last)
        {
            if *at == anchor {
                continue;
            }
            if child.formula.is_some() {
                return Err(IoError::Read(
                    crate::WorkbookReadError::ArrayCacheOverlapsAnAuthoredFormula,
                ));
            }
            let source = if child.style == CellStyle::default() {
                None
            } else {
                Some(Cell {
                    value: CellValue::Blank,
                    formula: None,
                    shared_formula: None,
                    style: child.style.clone(),
                })
            };
            replacements.push((sheet_index, *at, source));
            children.push((*at, child.value.clone()));
        }
        publication.push((anchor, last, formula, children));
    }
    // Clear decoded scalar sources in one sparse batch before attaching any
    // ownership; per-child Worksheet::set/remove would repeatedly scan caches.
    workbook.replace_cells(replacements)?;
    let sheet = workbook.sheet_mut(sheet_index).ok_or(IoError::Read(
        crate::WorkbookReadError::MissingArrayWorksheet,
    ))?;
    for (anchor, last, formula, children) in publication {
        sheet.set_spill_extent(anchor, last, &formula)?;
        for (at, value) in children {
            sheet.set_spill_cached_value(at, anchor, &formula, value)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{SheetSource, WorkbookSink};
    use crate::test_support::Workbook;
    use crate::{
        ooxml::{package::Package, xml::part},
        opc::OpcReadPlan,
    };

    /// Apply original package arrays for this module's recovery fixture.
    fn apply_spills(
        package: &Package,
        plan: &OpcReadPlan,
        workbook: &mut Workbook,
    ) -> Result<(), IoError> {
        for identity in plan.sheets.iter().filter(|s| s.supported) {
            apply_sheet_spills(part(package, &identity.part)?, &identity.name, workbook)?;
        }
        Ok(())
    }

    fn at(s: &str) -> CellAddress {
        s.parse().unwrap()
    }
    fn apply(xml: &str, workbook: &mut Workbook) -> Result<(), IoError> {
        let package = Package::from([("sheet.xml".into(), xml.as_bytes().to_vec())]);
        let plan = OpcReadPlan {
            workbook_part: "book.xml".into(),
            styles_part: None,
            theme_part: None,
            shared_strings_part: None,
            warnings: vec![],
            sheets: vec![crate::opc::SheetIdentity {
                source_index: 0,
                name: "S".into(),
                sheet_id: "1".into(),
                relationship_id: "r1".into(),
                part: "sheet.xml".into(),
                supported: true,
            }],
        };
        apply_spills(&package, &plan, workbook)
    }
    fn book() -> Workbook {
        let mut book = Workbook::new();
        book.add_sheet("S")
            .unwrap()
            .set_formula_with_cached_value(at("A1"), "{1,2}", CellValue::Number(1.))
            .unwrap();
        book
    }
    #[test]
    fn sparse_full_grid_extent_visits_only_stored_children() {
        let mut book = book();
        book.sheet_mut(0)
            .unwrap()
            .set_value(at("XFD1048576"), CellValue::Number(2.))
            .unwrap();
        apply(r#"<worksheet><sheetData><row r="1"><c r="A1"><f t="array" ref="A1:XFD1048576">{1,2}</f></c></row></sheetData></worksheet>"#,&mut book).unwrap();
        let sheet = book.sheet(0).unwrap();
        assert_eq!(sheet.cells().len(), 2);
        assert_eq!(sheet.row_indices().len(), 2);
        assert_eq!(sheet.spill_owner(at("XFD1048576")), Some(at("A1")));
        assert!(sheet.source_cell(at("XFD1048576")).is_none());
    }
    #[test]
    fn malformed_or_overlapping_ranges_are_rejected_before_ownership_publication() {
        for range in ["B1:C1", "A1:A0", "A1:XFE1", "A1:A1048577"] {
            let mut book = book();
            let before = book.clone();
            assert!(apply(&format!(r#"<worksheet><sheetData><row><c><f t="array" ref="{range}">{{1,2}}</f></c></row></sheetData></worksheet>"#),&mut book).is_err());
            assert_eq!(book, before);
        }
        let mut book = book();
        book.sheet_mut(0)
            .unwrap()
            .set_formula_with_cached_value(at("B2"), "{3,4}", CellValue::Number(3.))
            .unwrap();
        let before = book.clone();
        assert!(apply(r#"<worksheet><sheetData><row r="1"><c r="A1"><f t="array" ref="A1:B3">{1,2}</f></c></row><row r="2"><c r="B2"><f t="array" ref="B2:C4">{3,4}</f></c></row></sheetData></worksheet>"#,&mut book).is_err());
        assert_eq!(book, before);
    }
    #[test]
    fn many_disjoint_arrays_restore_sparse_children_in_one_batch() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        let mut xml = String::from("<worksheet><sheetData>");
        for row in 0..1000 {
            let anchor = CellAddress::new(row * 2, 0).unwrap();
            let child = CellAddress::new(row * 2, 1).unwrap();
            sheet
                .set_formula_with_cached_value(anchor, "{1,2}", CellValue::Number(1.))
                .unwrap();
            sheet.set_value(child, CellValue::Number(2.)).unwrap();
            xml.push_str(&format!(r#"<row r="{}"><c r="{anchor}"><f t="array" ref="{anchor}:{child}">{{1,2}}</f></c></row>"#,row*2+1));
        }
        xml.push_str("</sheetData></worksheet>");
        apply(&xml, &mut book).unwrap();
        let sheet = book.sheet(0).unwrap();
        assert_eq!(sheet.cells().len(), 2000);
        for row in 0..1000 {
            assert_eq!(
                sheet.spill_owner(CellAddress::new(row * 2, 1).unwrap()),
                Some(CellAddress::new(row * 2, 0).unwrap())
            );
        }
    }
    #[test]
    fn array_declarations_retain_singleton_and_incomplete_extents_but_shared_refs_do_not() {
        let mut singleton = book();
        apply(r#"<worksheet><sheetData><row><c><f t="array" ref="A1">{1,2}</f></c></row></sheetData></worksheet>"#,&mut singleton).unwrap();
        assert_eq!(
            singleton.sheet(0).unwrap().complete_spill_extent(at("A1")),
            Some(at("A1"))
        );
        let mut incomplete = book();
        incomplete
            .sheet_mut(0)
            .unwrap()
            .set_value(at("B2"), 4.)
            .unwrap();
        apply(r#"<worksheet><sheetData><row><c><f t="array" ref="A1:B2">{1,2;3,4}</f></c></row></sheetData></worksheet>"#,&mut incomplete).unwrap();
        assert_eq!(
            incomplete.sheet(0).unwrap().spill_extent(at("A1")),
            Some(at("B2"))
        );
        assert_eq!(
            incomplete.sheet(0).unwrap().complete_spill_extent(at("A1")),
            None
        );
        assert_eq!(
            bounds(incomplete.sheet(0).unwrap()).unwrap().get(&at("A1")),
            Some(&at("B2"))
        );
        let mut shared = book();
        apply(r#"<worksheet><sheetData><row><c><f t="shared" si="0" ref="A1:B2">1+2</f></c></row></sheetData></worksheet>"#,&mut shared).unwrap();
        assert_eq!(shared.sheet(0).unwrap().spill_extent(at("A1")), None);
        assert!(bounds(shared.sheet(0).unwrap()).unwrap().is_empty());
    }
}
