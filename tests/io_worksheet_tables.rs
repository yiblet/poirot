use poirot::model::{ModelError, Workbook};
use poirot_io::{IoError, read_workbook_bytes, write_xlsx_to_buffer};
use std::io::{Cursor, Read, Write};

// Original minimal SpreadsheetML authored for this feature. No third-party data.
// Selector meanings follow https://support.microsoft.com/en-us/excel/using-structured-references-with-excel-tables.
const FIXTURE: &[u8] = include_bytes!("assets/structured-references.xlsx");

#[test]
fn tables_and_formula_text_round_trip_through_xlsx_and_source_snapshot() {
    let mut book = read_workbook_bytes::<Workbook>(FIXTURE).unwrap();
    let sheet = book.sheet(0).unwrap();
    let table = &sheet.tables()[0];
    assert_eq!(table.id, 1);
    assert_eq!(table.display_name, "Sales");
    assert_eq!(table.first.to_string(), "A1");
    assert_eq!(table.last.to_string(), "B4");
    assert!(table.header_row && table.totals_row);
    assert_eq!(
        table
            .columns
            .iter()
            .map(|col| (col.id, col.name.as_str()))
            .collect::<Vec<_>>(),
        [(1, "Amount"), (2, "Twice")]
    );
    let snapshot = book.save_snapshot().to_buffer().unwrap();
    let restored = Workbook::restore_snapshot()
        .from_bytes(&snapshot)
        .unwrap()
        .into_evaluation(Default::default())
        .unwrap()
        .evaluation
        .into_workbook();
    assert_eq!(restored.sheet(0).unwrap().tables(), sheet.tables());

    let bytes = write_xlsx_to_buffer(&book).unwrap();
    let reopened = read_workbook_bytes::<Workbook>(&bytes).unwrap();
    let output = reopened.sheet(0).unwrap();
    assert_eq!(output.tables(), sheet.tables());
    for (address, formula) in [
        ("B2", "Sales[[#This Row],[Amount]]*2"),
        ("B3", "Sales[[#This Row],[Amount]]*2"),
        ("D1", "SUM(Sales[Amount])"),
        ("D2", "SUM(Sales[#Totals])"),
    ] {
        let load = |sheet: &poirot::model::Worksheet| {
            sheet
                .get(address)
                .unwrap()
                .unwrap()
                .formula
                .clone()
                .unwrap()
        };
        assert_eq!(load(sheet).trim_start_matches('='), formula);
        assert_eq!(load(output), load(sheet));
    }
    assert!(matches!(
        book.duplicate_sheet(0, "Copy"),
        Err(ModelError::DuplicateWorksheetTables)
    ));
    assert_eq!(book.sheets().len(), 1);
}

#[test]
fn empty_table_selector_reads_only_the_data_rows() {
    use poirot::model::CellValue;

    // MS-OI29500 defines [] as an alias for [#Data].
    let mut book = read_workbook_bytes::<Workbook>(FIXTURE).unwrap();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_formula("D1", "SUM(Sales[])").unwrap();
    sheet.set_formula("D2", "SUM(Sales[#Data])").unwrap();
    book.recalculate().unwrap();
    for address in ["D1", "D2"] {
        assert_eq!(
            book.stored_cell("Data", address).unwrap().unwrap().value,
            CellValue::Number(30.)
        );
    }
}

#[test]
fn dynamic_spills_cannot_cross_table_boundaries() {
    use poirot::model::{CellError, CellValue};

    // Excel rejects spilled array formulas in tables, even when cells are blank.
    // https://support.microsoft.com/en-us/excel/how-to-correct-a-spill-error
    let bytes = rewrite_table(|xml| xml.replace(r#"ref="A1:B4""#, r#"ref="B2:C5""#));
    let mut book = read_workbook_bytes::<Workbook>(&bytes).unwrap();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.remove("B3").unwrap();
    sheet.set_formula("B2", "SEQUENCE(1)").unwrap();
    sheet.set_formula("A3", "SEQUENCE(1,2)").unwrap();
    sheet.set_formula("B5", "SEQUENCE(2)").unwrap();
    sheet.set_formula("D5", "SEQUENCE(2)").unwrap();

    book.recalculate().unwrap();
    let sheet = book.sheet(0).unwrap();
    for (address, expected) in [
        ("B2", CellValue::Number(1.)),
        ("A3", CellValue::Error(CellError::Spill)),
        ("B5", CellValue::Error(CellError::Spill)),
        ("D5", CellValue::Number(1.)),
        ("D6", CellValue::Number(2.)),
    ] {
        assert_eq!(sheet.get(address).unwrap().unwrap().value, expected);
    }
    for address in ["B3", "B6"] {
        assert!(sheet.get(address).unwrap().is_none());
    }
}

#[test]
fn invalid_column_metadata_is_refused() {
    let bytes =
        rewrite_table(|xml| xml.replace(r#"id="2" name="Twice""#, r#"id="1" name="Twice""#));
    assert!(matches!(
        read_workbook_bytes::<Workbook>(&bytes),
        Err(IoError::Model(
            poirot_io::ModelError::InvalidWorksheetTable { .. }
        ))
    ));
}

#[test]
fn optional_name_and_xml_escaped_caption_round_trip() {
    let bytes = rewrite_table(|xml| {
        xml.replace(r#" name="Sales""#, "")
            .replace(r#"name="Twice""#, r#"name="Quoted &quot;A &amp; B&quot;""#)
    });
    let book = read_workbook_bytes::<Workbook>(&bytes).unwrap();
    let table = &book.sheet(0).unwrap().tables()[0];
    assert_eq!(table.name, None);
    assert_eq!(table.columns[1].name, "Quoted \"A & B\"");
    let reopened = read_workbook_bytes::<Workbook>(&write_xlsx_to_buffer(&book).unwrap()).unwrap();
    assert_eq!(
        reopened.sheet(0).unwrap().tables(),
        std::slice::from_ref(table)
    );
}

fn rewrite_table(rewrite: impl Fn(&str) -> String) -> Vec<u8> {
    let mut input = zip::ZipArchive::new(Cursor::new(FIXTURE)).unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..input.len() {
        let mut entry = input.by_index(index).unwrap();
        writer
            .start_file(
                entry.name().unwrap(),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        let mut xml = String::new();
        entry.read_to_string(&mut xml).unwrap();
        if entry.name().unwrap() == "xl/tables/table1.xml" {
            xml = rewrite(&xml);
        }
        writer.write_all(xml.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn qualified_table_selectors_use_the_imported_sections_and_columns() {
    use poirot::eval::{EvaluationOutcome, run_formula};
    use poirot::model::{CellError, CellValue};

    let mut book = read_workbook_bytes::<Workbook>(FIXTURE).unwrap();
    book.add_sheet("Summary").unwrap();
    // The fixture's data rows are 3 and 7, with separate headers and a literal 10
    // in the totals row. Selector meanings follow the Microsoft reference above.
    for (formula, expected) in [
        ("SUM(Sales[Amount])", CellValue::Number(10.)),
        ("SUM(sALES[[#DATA],[aMOUNT]])", CellValue::Number(10.)),
        ("SUM(Sales[[#Totals],[Amount]])", CellValue::Number(10.)),
        ("SUM(Sales[[#All],[Amount]])", CellValue::Number(20.)),
        (
            "INDEX(Sales[[#Headers],[Amount]],1)",
            CellValue::Text("Amount".into()),
        ),
        ("COLUMNS(Sales[[Amount]:[Twice]])", CellValue::Number(2.)),
        ("ROWS(Sales)", CellValue::Number(2.)),
        ("SUM(Sales[Unknown])", CellValue::Error(CellError::Ref)),
        (
            "SUM(UnknownTable[Amount])",
            CellValue::Error(CellError::Name),
        ),
    ] {
        let result = run_formula(
            &book,
            1,
            "B1".parse().unwrap(),
            formula,
            Default::default(),
            Default::default(),
        );
        let EvaluationOutcome::Complete { value, .. } = result else {
            panic!("{formula}: {result:?}");
        };
        assert_eq!(value, expected, "{formula}");
    }
}

#[test]
fn table_area_reads_invalidate_incremental_results_after_cell_edits() {
    use poirot::calculation::EvaluationContext;
    use poirot::model::CellValue;

    let mut book = read_workbook_bytes::<Workbook>(FIXTURE).unwrap();
    book.add_sheet("Summary")
        .unwrap()
        .set_formula("A1", "SUM(Sales[Amount])")
        .unwrap();
    let mut scenario = book.overlay().build().unwrap();
    let local = scenario.target("Data!D1").unwrap();
    let cross = scenario.target("Summary!A1").unwrap();
    let totals = scenario.target("Data!D2").unwrap();
    let input = scenario.target("Data!A2").unwrap();
    let context = EvaluationContext::default();
    for (amount, expected) in [(3., 10.), (5., 12.)] {
        scenario.set_value(input, amount).unwrap();
        let mut run = scenario.evaluation(&context).unwrap();
        assert_eq!(run.cell(local).unwrap(), CellValue::Number(expected));
        assert_eq!(run.cell(cross).unwrap(), CellValue::Number(expected));
        assert_eq!(run.cell(totals).unwrap(), CellValue::Number(10.));
    }

    let bytes = scenario.save_snapshot().to_buffer().unwrap();
    let mut restored = Workbook::restore_snapshot()
        .prepared_policy(poirot::snapshots::PreparedPolicy::RequireCompatible)
        .from_bytes(&bytes)
        .unwrap()
        .into_overlay()
        .unwrap();
    restored.set_value(input, 9.).unwrap();
    let mut run = restored.evaluation(&context).unwrap();
    assert_eq!(run.cell(local).unwrap(), CellValue::Number(16.));
    assert_eq!(run.cell(cross).unwrap(), CellValue::Number(16.));
}

#[test]
fn formula_copy_preserves_table_column_spelling_and_translates_cell_addresses() {
    use poirot_parsing::formula::{FormulaLocation, TranslationPlan, TranslationPolicy};

    let translated = TranslationPlan::new(
        "SUM(Sales[[#Headers],[A1]])+A1+ROWS(Sales['#Count])",
        FormulaLocation { row: 0, column: 0 },
        FormulaLocation { row: 1, column: 1 },
        TranslationPolicy::default(),
    )
    .unwrap();
    assert_eq!(
        translated.replacement().trim_start_matches('='),
        "SUM(Sales[[#Headers],[A1]])+B2+ROWS(Sales['#Count])"
    );
}

#[test]
fn current_row_selectors_use_the_formula_row_and_plain_local_columns_use_data_rows() {
    use poirot::eval::{EvaluationOutcome, run_formula};
    use poirot::model::{CellError, CellValue};

    let mut book = read_workbook_bytes::<Workbook>(FIXTURE).unwrap();
    book.add_sheet("Summary").unwrap();
    // The source table spans A1:B4. Rows 2 and 3 contain 3 and 7.
    // See Microsoft Structure References, MS-OI29500 section 3.2.3.1.1.
    for (sheet, address, formula, expected) in [
        (0, "B2", "[@Amount]", CellValue::Number(3.)),
        (0, "B3", "[@[Amount]]", CellValue::Number(7.)),
        (0, "B2", "[[#This Row],[Amount]]", CellValue::Number(3.)),
        (0, "B3", "SUM([Amount])", CellValue::Number(10.)),
        (0, "D3", "Sales[@Amount]", CellValue::Number(7.)),
        (
            0,
            "D2",
            "COLUMNS(Sales[@[Amount]:[Twice]])",
            CellValue::Number(2.),
        ),
        (
            1,
            "D2",
            "Sales[[#This Row],[Amount]]",
            CellValue::Number(3.),
        ),
        (0, "B1", "[@Amount]", CellValue::Error(CellError::Value)),
        (
            0,
            "B4",
            "Sales[@Amount]",
            CellValue::Error(CellError::Value),
        ),
        (
            0,
            "B5",
            "Sales[@Amount]",
            CellValue::Error(CellError::Value),
        ),
        (0, "D2", "[@Amount]", CellValue::Error(CellError::Value)),
        (1, "B2", "[@Amount]", CellValue::Error(CellError::Value)),
    ] {
        let result = run_formula(
            &book,
            sheet,
            address.parse().unwrap(),
            formula,
            Default::default(),
            Default::default(),
        );
        let EvaluationOutcome::Complete { value, .. } = result else {
            panic!("{address}: {formula}: {result:?}");
        };
        assert_eq!(value, expected, "{address}: {formula}");
    }
}

#[test]
fn current_row_formulas_reopen_and_invalidate_only_the_edited_row() {
    use poirot::calculation::EvaluationContext;
    use poirot::model::CellValue;

    let mut book = read_workbook_bytes::<Workbook>(FIXTURE).unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B2", "[@Amount]*2")
        .unwrap();
    let bytes = write_xlsx_to_buffer(&book).unwrap();
    let reopened = read_workbook_bytes::<Workbook>(&bytes).unwrap();
    assert_eq!(
        reopened
            .sheet(0)
            .unwrap()
            .get("B2")
            .unwrap()
            .unwrap()
            .formula
            .as_deref(),
        Some("[@Amount]*2")
    );
    let context = EvaluationContext::default();
    for book in [&book, &reopened] {
        let mut scenario = book.overlay().build().unwrap();
        let input = scenario.target("Data!A2").unwrap();
        let first = scenario.target("Data!B2").unwrap();
        let second = scenario.target("Data!B3").unwrap();
        {
            let mut eval = scenario.evaluation(&context).unwrap();
            assert_eq!(eval.cell(first).unwrap(), CellValue::Number(6.));
            assert_eq!(eval.cell(second).unwrap(), CellValue::Number(14.));
        }
        scenario.set_value(input, 5.).unwrap();
        assert_eq!(
            scenario.cached_value(second).as_deref(),
            Some(&CellValue::Number(14.))
        );
        let mut eval = scenario.evaluation(&context).unwrap();
        assert_eq!(eval.cell(first).unwrap(), CellValue::Number(10.));
        assert_eq!(eval.cell(second).unwrap(), CellValue::Number(14.));
    }
}

#[test]
fn translating_a_current_row_reference_keeps_its_row_selector() {
    use poirot_parsing::formula::{FormulaLocation, TranslationPlan, TranslationPolicy};

    let translated = TranslationPlan::new(
        "[@[Sales Amount]]+A2",
        FormulaLocation { row: 1, column: 1 },
        FormulaLocation { row: 2, column: 1 },
        TranslationPolicy::default(),
    )
    .unwrap();
    assert_eq!(
        translated.replacement().trim_start_matches('='),
        "[[#This Row],[Sales Amount]]+A3"
    );
}
