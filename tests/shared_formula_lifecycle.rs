//! Shared source formulas remain independent through calculation and persistence.
//! Original fixture: ECMA-376 Part 1 §18.3.1.40 defines shared relative offsets.
use poirot::eval;
use poirot::{CellAddress, CellValue, Workbook};
use std::{
    io::{Cursor, Read, Write},
    sync::Arc,
};

fn at(source: &str) -> CellAddress {
    source.parse().unwrap()
}

fn imported_group() -> Workbook {
    let mut source = Workbook::new();
    source.add_sheet("Inputs").unwrap();
    let bytes = source.save_xlsx().to_buffer().unwrap();
    let mut input = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..input.len() {
        let mut part = input.by_index(index).unwrap();
        let name = part.name().unwrap().into_owned();
        let mut bytes = Vec::new();
        part.read_to_end(&mut bytes).unwrap();
        if name == "xl/worksheets/sheet1.xml" {
            bytes = br#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>10</v></c><c r="B1"><f t="shared" si="7" ref="B1:B3">A1+1</f><v>-1</v></c></row><row r="2"><c r="A2"><v>20</v></c><c r="B2"><f t="shared" si="7"/><v>-1</v></c></row><row r="3"><c r="A3"><v>30</v></c><c r="B3"><f t="shared" si="7"/><v>-1</v></c></row></sheetData></worksheet>"#.to_vec();
        }
        output
            .start_file(&name, zip::write::SimpleFileOptions::default())
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    Workbook::from_bytes(&output.finish().unwrap().into_inner()).unwrap()
}

#[test]
fn imported_shared_sources_reuse_executables_with_distinct_reference_bindings() {
    let book = imported_group();
    let mut compiler = eval::FormulaCompiler::default();
    let policy = eval::PreparationPolicy::default();
    let formulas: Vec<_> = ["B1", "B2", "B3"]
        .into_iter()
        .map(|address| {
            let cell = book.stored_cell("Inputs", at(address)).unwrap();
            assert!(cell.shared_formula.is_none());
            compiler.compile_formula(cell.formula.as_deref().unwrap(), &policy)
        })
        .collect();
    let prepared: Vec<_> = formulas
        .iter()
        .map(|formula| {
            let eval::PreparedFormula::Ready(formula) = formula.as_ref() else {
                panic!("simple arithmetic must compile")
            };
            formula.bytecode()
        })
        .collect();
    for pair in prepared.windows(2) {
        assert!(Arc::ptr_eq(&pair[0].executable, &pair[1].executable));
        assert_ne!(pair[0].references, pair[1].references);
    }
}

#[test]
fn replacing_original_anchor_survives_calculation_snapshot_and_fresh_save() {
    let mut book = imported_group();
    for (address, formula) in [("B1", "A1+1"), ("B2", "A2+1"), ("B3", "A3+1")] {
        let cell = book.stored_cell("Inputs", at(address)).unwrap();
        assert_eq!(cell.formula.as_deref(), Some(formula));
        assert_eq!(cell.value, CellValue::Number(-1.));
    }
    book.recalculate().unwrap();
    for (address, value) in [("B1", 11.), ("B2", 21.), ("B3", 31.)] {
        assert_eq!(
            book.stored_cell("Inputs", at(address)).unwrap().value,
            CellValue::Number(value)
        );
        assert_eq!(
            book.stored_cell("Inputs", at(address))
                .unwrap()
                .shared_formula,
            None
        );
    }
    let sheet = book.sheet_by_name_mut("Inputs").unwrap();
    sheet.set_value(at("B1"), 99.).unwrap();
    sheet.set_value(at("A2"), 40.).unwrap();
    book.recalculate().unwrap();
    let snapshot = book.save_snapshot().to_buffer().unwrap();
    let restored = Workbook::from_model(
        Workbook::restore_snapshot()
            .from_bytes(&snapshot)
            .unwrap()
            .into_artifact()
            .into_parts()
            .0,
    );
    assert_eq!(restored, book);
    let bytes = restored.save_xlsx().to_buffer().unwrap();
    let mut reopened = Workbook::from_bytes(&bytes).unwrap();
    for candidate in [&restored, &reopened] {
        for (address, value, formula) in [
            ("B1", 99., None),
            ("B2", 41., Some("A2+1")),
            ("B3", 31., Some("A3+1")),
        ] {
            let cell = candidate.stored_cell("Inputs", at(address)).unwrap();
            assert_eq!(cell.value, CellValue::Number(value));
            assert_eq!(cell.formula.as_deref(), formula);
        }
        assert!(
            candidate
                .stored_cell("Inputs", at("B1"))
                .unwrap()
                .shared_formula
                .is_none()
        );
    }
    assert!(
        reopened
            .stored_cell("Inputs", at("B2"))
            .unwrap()
            .shared_formula
            .is_none()
    );
    assert!(
        reopened
            .stored_cell("Inputs", at("B3"))
            .unwrap()
            .shared_formula
            .is_none()
    );
    reopened.recalculate().unwrap();
    assert_eq!(
        reopened.stored_cell("Inputs", at("B2")).unwrap().value,
        CellValue::Number(41.)
    );
}
