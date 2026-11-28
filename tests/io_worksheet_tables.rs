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
fn table_insert_row_is_excluded_from_data_after_round_trips() {
    use poirot::model::{CellError, CellValue};

    // The Open XML table definition reserves a distinct row for insertRow.
    // A populated worksheet cell in that slot must not become table data.
    let bytes =
        rewrite_table(|xml| xml.replace(r#"ref="A1:B4""#, r#"ref="A1:B3" insertRow="true""#));
    let mut book = read_workbook_bytes::<Workbook>(&bytes).unwrap();
    book.sheet_mut(0).unwrap().remove("B3").unwrap();
    let xlsx = write_xlsx_to_buffer(&book).unwrap();
    let snapshot = book.save_snapshot().to_buffer().unwrap();
    let restored = Workbook::restore_snapshot()
        .from_bytes(&snapshot)
        .unwrap()
        .into_evaluation(Default::default())
        .unwrap()
        .evaluation
        .into_workbook();

    for mut workbook in [
        book,
        read_workbook_bytes::<Workbook>(&xlsx).unwrap(),
        restored,
    ] {
        let table = &workbook.sheet(0).unwrap().tables()[0];
        assert!(table.insert_row && table.totals_row);
        workbook.recalculate().unwrap();
        for (address, expected) in [
            ("A2", CellValue::Number(3.)),
            ("D1", CellValue::Error(CellError::Ref)),
            ("D2", CellValue::Number(7.)),
        ] {
            assert_eq!(
                workbook
                    .stored_cell("Data", address)
                    .unwrap()
                    .unwrap()
                    .value,
                expected
            );
        }
    }
}

#[test]
fn table_insert_row_must_fit_alongside_headers_and_totals() {
    let bytes = rewrite_table(|xml| xml.replace(r#"ref="A1:B4""#, r#"ref="A1:B2" insertRow="1""#));
    assert!(matches!(
        read_workbook_bytes::<Workbook>(&bytes),
        Err(IoError::Model(
            poirot_io::ModelError::InvalidWorksheetTable { .. }
        ))
    ));
}

#[test]
fn growing_an_empty_table_fills_its_insert_row_first() {
    let bytes = rewrite_table(|xml| {
        xml.replace(r#"ref="A1:B4""#, r#"ref="A1:B2" insertRow="1""#)
            .replace(r#"totalsRowCount="1""#, r#"totalsRowCount="0""#)
            .replace("<tableColumns", r#"<autoFilter ref="A1:B1"/><tableColumns"#)
            .replace(
                r#"<tableColumn id="2" name="Twice"/>"#,
                r#"<tableColumn id="2" name="Twice"><calculatedColumnFormula>A2*2</calculatedColumnFormula></tableColumn>"#,
            )
    });
    let mut book = read_workbook_bytes::<Workbook>(&bytes).unwrap();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.remove("A2").unwrap();
    sheet.remove("B2").unwrap();

    sheet.grow_table_rows("Sales", 1).unwrap();
    let table = &sheet.tables()[0];
    assert!(!table.insert_row);
    assert_eq!(table.last, "B2".parse().unwrap());
    assert_eq!(
        table.auto_filter.as_ref().unwrap().range.unwrap().1,
        table.last
    );
    assert_eq!(
        sheet.get("B2").unwrap().unwrap().formula.as_deref(),
        Some("A2*2")
    );
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

#[test]
fn escaped_table_attributes_preserve_reference_captions_and_filter_values() {
    use poirot::model::CellValue;

    let bytes = rewrite_table(|xml| {
        xml.replace(r#"displayName="Sales""#, r#"displayName="Sales_x005F_x0041_""#)
            .replace(r#" name="Sales""#, r#" name="Internal_x005F_x0042_""#)
            .replace(r#"name="Amount""#, r#"name="_x005F_x0041_""#)
            .replace(
                "<tableColumns",
                r#"<autoFilter ref="A1:B3"><filterColumn colId="0"><filters><filter val="a_x0009_b_x000A_c_x000D_d_x0001_"/></filters></filterColumn></autoFilter><tableColumns"#,
            )
            .replace("</table>", r#"<tableStyleInfo name="Custom_x005F_x0043_"/></table>"#)
    });
    let mut book = read_workbook_bytes::<Workbook>(&bytes).unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("H1", "SUM(Sales_x0041_[_x0041_])")
        .unwrap();
    let table = &book.sheet(0).unwrap().tables()[0];
    assert_eq!(table.name.as_deref(), Some("Internal_x0042_"));
    assert_eq!(table.display_name, "Sales_x0041_");
    assert_eq!(table.columns[0].name, "_x0041_");
    assert_eq!(
        table.style.as_ref().unwrap().name.as_deref(),
        Some("Custom_x0043_")
    );
    let criteria = table.auto_filter.as_ref().unwrap().columns[0]
        .filters
        .as_ref()
        .unwrap();
    assert_eq!(criteria.values, ["a\tb\nc\rd\u{1}"]);

    let exported = write_xlsx_to_buffer(&book).unwrap();
    let mut package = zip::ZipArchive::new(Cursor::new(&exported)).unwrap();
    let mut xml = String::new();
    package
        .by_name("xl/tables/table1.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains(r#"name="_x005F_x0041_""#));
    assert!(xml.contains(r#"val="a_x0009_b_x000A_c_x000D_d_x0001_""#));
    let reopened = read_workbook_bytes::<Workbook>(&exported).unwrap();
    assert_eq!(
        reopened.sheet(0).unwrap().tables(),
        std::slice::from_ref(table)
    );

    for mut workbook in [book, reopened] {
        workbook.recalculate().unwrap();
        assert_eq!(
            workbook.stored_cell("Data", "H1").unwrap().unwrap().value,
            CellValue::Number(10.)
        );
    }
}

#[test]
fn table_attributes_with_lone_surrogate_escapes_are_refused() {
    let bytes = rewrite_table(|xml| xml.replace(r#"name="Amount""#, r#"name="_xD800_""#));
    assert!(matches!(
        read_workbook_bytes::<Workbook>(&bytes),
        Err(IoError::Model(
            poirot_io::ModelError::InvalidWorksheetTable { .. }
        ))
    ));
}

fn rewrite_table(rewrite: impl Fn(&str) -> String) -> Vec<u8> {
    rewrite_fixture_part("xl/tables/table1.xml", rewrite)
}

fn rewrite_fixture_part(path: &str, rewrite: impl Fn(&str) -> String) -> Vec<u8> {
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
        if entry.name().unwrap() == path {
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
        "SUM(Sales[[#Headers],[#Data],[A1]])+A1+ROWS(Sales['#Count])",
        FormulaLocation { row: 0, column: 0 },
        FormulaLocation { row: 1, column: 1 },
        TranslationPolicy::default(),
    )
    .unwrap();
    assert_eq!(
        translated.replacement().trim_start_matches('='),
        "SUM(Sales[[#Headers],[#Data],[A1]])+B2+ROWS(Sales['#Count])"
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

#[test]
fn xlsx_map_over_two_table_columns_recomputes_after_edits() {
    use poirot::calculation::EvaluationContext;
    use poirot::eval::{ArrayResultPolicy, EvaluationPolicy};
    use poirot::model::CellValue;

    // Adapt the Microsoft MAP table-column example to the source fixture's
    // two numeric columns. Each callback adds the corresponding two values.
    // https://support.microsoft.com/en-us/excel/functions/map-function
    let mut book = read_workbook_bytes::<Workbook>(FIXTURE).unwrap();
    book.add_sheet("Summary")
        .unwrap()
        .set_formula("A1", "MAP(Sales[Amount],Sales[Twice],LAMBDA(a,b,a+b))")
        .unwrap();
    let bytes = write_xlsx_to_buffer(&book).unwrap();
    let imported = read_workbook_bytes::<Workbook>(&bytes).unwrap();
    let mut scenario = imported
        .overlay()
        .policy(EvaluationPolicy {
            arrays: ArrayResultPolicy::DynamicSpill,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let source = scenario.target("Data!A3").unwrap();
    let first = scenario.target("Summary!A1").unwrap();
    let second = scenario.target("Summary!A2").unwrap();
    for (value, expected) in [(7., 21.), (10., 30.)] {
        scenario.set_value(source, value).unwrap();
        let mut eval = scenario.evaluation(&context).unwrap();
        assert_eq!(eval.cell(first).unwrap(), CellValue::Number(9.));
        assert_eq!(eval.cell(second).unwrap(), CellValue::Number(expected));
    }
}

#[test]
fn indirect_resolves_intact_table_selectors_in_the_callers_context() {
    use poirot::eval::{EvaluationBudgets, EvaluationError, EvaluationOutcome, run_formula};
    use poirot::model::{CellError, CellValue};

    let bytes =
        rewrite_table(|xml| xml.replace(r#"name="Twice""#, r#"name="Notes!Net: O&apos;Brien""#));
    let mut book = read_workbook_bytes::<Workbook>(&bytes).unwrap();
    book.add_sheet("Summary").unwrap();
    // Microsoft documents A1-mode INDIRECT reference text and bracketed table
    // selectors. The fixture contains Amount 3 and 7 and the doubled column.
    // https://support.microsoft.com/en-us/excel/functions/indirect-function
    // https://support.microsoft.com/en-us/excel/using-structured-references-with-excel-tables
    for (sheet, address, formula, expected) in [
        (1, "A1", r#"SUM(INDIRECT("Sales[Amount]"))"#, 10.0.into()),
        (
            1,
            "A1",
            r#"SUM(INDIRECT("Sales[[Notes!Net: O''Brien]]"))"#,
            20.0.into(),
        ),
        (
            1,
            "A1",
            r#"COLUMNS(INDIRECT("Sales[[Amount]:[Notes!Net: O''Brien]]"))"#,
            2.0.into(),
        ),
        (0, "B3", r#"INDIRECT("[@Amount]")"#, 7.0.into()),
        (0, "D2", r#"INDIRECT("R[0]C[-3]",FALSE)"#, 3.0.into()),
        (
            1,
            "A1",
            r#"INDIRECT("Sales[Amount]",FALSE)"#,
            CellValue::Error(CellError::Ref),
        ),
        (
            1,
            "A1",
            r#"INDIRECT("Sales[Missing]")"#,
            CellValue::Error(CellError::Ref),
        ),
        (
            1,
            "A1",
            r#"INDIRECT("UnknownTable[Amount]")"#,
            CellValue::Error(CellError::Ref),
        ),
        (
            1,
            "A1",
            r#"INDIRECT("Sales[Amount]+A1")"#,
            CellValue::Error(CellError::Ref),
        ),
        (
            1,
            "A1",
            r#"INDIRECT("Sales[Amount")"#,
            CellValue::Error(CellError::Ref),
        ),
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
            panic!("{formula}: {result:?}");
        };
        assert_eq!(value, expected, "{formula}");
    }

    let external = run_formula(
        &book,
        1,
        "A1".parse().unwrap(),
        r#"IFERROR(INDIRECT("[outside.xlsx]Data!Sales[[Notes!Net: O''Brien]]"),99)"#,
        Default::default(),
        Default::default(),
    );
    assert!(matches!(
        external,
        EvaluationOutcome::Aborted {
            failure: EvaluationError::UnsupportedCapability { .. },
            ..
        }
    ));
    let restricted = run_formula(
        &book,
        1,
        "A1".parse().unwrap(),
        r#"IFERROR(INDIRECT("Sales[Amount]"),99)"#,
        Default::default(),
        EvaluationBudgets {
            text_bytes: 30,
            ..Default::default()
        },
    );
    assert!(matches!(
        restricted,
        EvaluationOutcome::Aborted {
            failure: EvaluationError::Budget(_),
            ..
        }
    ));
}

#[test]
fn indirect_tracks_the_consumed_table_column_and_memoizes_within_each_cycle() {
    use poirot::calculation::EvaluationContext;
    use poirot::model::CellValue;

    let mut book = read_workbook_bytes::<Workbook>(FIXTURE).unwrap();
    // Make the two columns independent so no transitive read masks reuse.
    let data = book.sheet_mut(0).unwrap();
    data.set_value("B2", 6.0).unwrap();
    data.set_value("B3", 14.0).unwrap();
    let sheet = book.add_sheet("Summary").unwrap();
    sheet.set_value("A1", "Sales[Amount]").unwrap();
    sheet.set_formula("B1", "SUM(INDIRECT(A1))").unwrap();
    sheet.set_value("C1", 0.0).unwrap();
    let mut scenario = book.overlay().build().unwrap();
    let text = scenario.target("Summary!A1").unwrap();
    let output = scenario.target("Summary!B1").unwrap();
    let unrelated = scenario.target("Summary!C1").unwrap();
    let amount = scenario.target("Data!A2").unwrap();
    let twice = scenario.target("Data!B2").unwrap();
    let context = EvaluationContext::default();
    for (edit, cached_before_cycle, expected) in [
        (None, None, 10.0),
        (Some((unrelated, 1.0.into())), Some(10.0), 10.0),
        (Some((amount, 5.0.into())), None, 12.0),
        (
            Some((text, CellValue::Text("Sales[Twice]".into()))),
            None,
            20.0,
        ),
        (Some((amount, 9.0.into())), Some(20.0), 20.0),
        (Some((twice, 11.0.into())), None, 25.0),
    ] {
        if let Some((target, value)) = edit {
            scenario.set_value(target, value).unwrap();
        }
        assert_eq!(
            scenario.cached_value(output).as_deref(),
            cached_before_cycle.map(CellValue::Number).as_ref()
        );
        // A new cycle recalculates volatile INDIRECT. A second read inside that
        // cycle uses its memoized result without parsing the selector again.
        let mut cycle = scenario.evaluation_with_profiling(&context, true).unwrap();
        for _ in 0..2 {
            assert_eq!(cycle.cell(output).unwrap(), CellValue::Number(expected));
        }
        let usage = cycle.finish().execution.unwrap();
        assert_eq!(usage.formula_executions, 1);
        assert_eq!(usage.dynamic_parse_attempts, 1);
    }
}

#[test]
fn combined_table_sections_roundtrip_and_track_included_rows() {
    use poirot::calculation::EvaluationContext;
    use poirot::model::CellValue;
    use poirot::snapshots::PreparedPolicy;

    let mut book = read_workbook_bytes::<Workbook>(FIXTURE).unwrap();
    let sheet = book.add_sheet("Summary").unwrap();
    let formulas = [
        (
            "A1",
            "ROWS(Sales[[#Headers],[#Data]])",
            CellValue::Number(3.),
        ),
        (
            "A2",
            "INDEX(Sales[[#Headers],[#Data],[Amount]],1)",
            CellValue::Text("Amount".into()),
        ),
        (
            "A3",
            "SUM(Sales[[#Headers],[#Data],[Amount]])",
            CellValue::Number(10.),
        ),
        (
            "A4",
            "SUM(Sales[[#Data],[#Totals],[Amount]])",
            CellValue::Number(20.),
        ),
        (
            "A5",
            "COLUMNS(Sales[[#Data],[#Totals],[Amount]:[Twice]])",
            CellValue::Number(2.),
        ),
    ];
    for (address, formula, _) in &formulas {
        sheet.set_formula(*address, *formula).unwrap();
    }
    let bytes = write_xlsx_to_buffer(&book).unwrap();
    let reopened = read_workbook_bytes::<Workbook>(&bytes).unwrap();
    let mut scenario = reopened.overlay().build().unwrap();
    let context = EvaluationContext::default();
    for (address, formula, expected) in formulas {
        assert_eq!(
            reopened
                .sheet(1)
                .unwrap()
                .get(address)
                .unwrap()
                .unwrap()
                .formula
                .as_deref(),
            Some(formula)
        );
        let target = scenario.target(&format!("Summary!{address}")).unwrap();
        assert_eq!(
            scenario.evaluation(&context).unwrap().cell(target).unwrap(),
            expected
        );
    }
    let data = scenario.target("Data!A2").unwrap();
    let totals = scenario.target("Data!A4").unwrap();
    let headers_and_data = scenario.target("Summary!A3").unwrap();
    let data_and_totals = scenario.target("Summary!A4").unwrap();
    let snapshot = scenario.save_snapshot().to_buffer().unwrap();
    let mut restored = Workbook::restore_snapshot()
        .prepared_policy(PreparedPolicy::RequireCompatible)
        .from_bytes(&snapshot)
        .unwrap()
        .into_overlay()
        .unwrap();
    restored.set_value(totals, 20.).unwrap();
    {
        let mut run = restored.evaluation(&context).unwrap();
        assert_eq!(run.cell(headers_and_data).unwrap(), CellValue::Number(10.));
        assert_eq!(run.cell(data_and_totals).unwrap(), CellValue::Number(30.));
    }
    restored.set_value(data, 5.).unwrap();
    let mut run = restored.evaluation(&context).unwrap();
    assert_eq!(run.cell(headers_and_data).unwrap(), CellValue::Number(12.));
    assert_eq!(run.cell(data_and_totals).unwrap(), CellValue::Number(32.));
}

#[test]
fn combined_data_and_totals_exclude_the_insert_placeholder() {
    use poirot::model::CellValue;

    // MS-OI29500 defines the combined selector as the union of its two sections.
    // The fixture's A2, A3 and A4 contain 3, 7 and 10 respectively.
    let bytes = rewrite_table(|xml| xml.replace(r#"ref="A1:B4""#, r#"ref="A1:B4" insertRow="1""#));
    let mut book = read_workbook_bytes::<Workbook>(&bytes).unwrap();
    let sheet = book.sheet_mut(0).unwrap();
    sheet
        .set_formula("D3", "SUM(Sales[[#Data],[#Totals],[Amount]])")
        .unwrap();
    sheet
        .set_formula(
            "D4",
            r#"SUM(INDIRECT("Sales[[#Data],[#Totals],[Amount]]"))"#,
        )
        .unwrap();
    book.recalculate().unwrap();
    for address in ["D3", "D4"] {
        assert_eq!(
            book.stored_cell("Data", address).unwrap().unwrap().value,
            CellValue::Number(13.)
        );
    }
}

#[test]
fn combined_table_sections_require_each_selected_section() {
    use poirot::eval::{EvaluationOutcome, run_formula};
    use poirot::model::{CellError, CellValue};

    for (from, to, selector) in [
        (
            "headerRowCount=\"1\"",
            "headerRowCount=\"0\"",
            "[#Headers],[#Data]",
        ),
        (
            "totalsRowCount=\"1\"",
            "totalsRowCount=\"0\"",
            "[#Data],[#Totals]",
        ),
        ("A1:B4", "A1:B2", "[#Headers],[#Data]"),
        ("A1:B4", "A1:B2", "[#Data],[#Totals]"),
    ] {
        let bytes = rewrite_table(|xml| xml.replace(from, to));
        let book = read_workbook_bytes::<Workbook>(&bytes).unwrap();
        let formula = format!("ROWS(Sales[{selector}])");
        assert!(
            matches!(
                run_formula(
                    &book,
                    0,
                    "E1".parse().unwrap(),
                    &formula,
                    Default::default(),
                    Default::default()
                ),
                EvaluationOutcome::Complete {
                    value: CellValue::Error(CellError::Ref),
                    ..
                }
            ),
            "{from} -> {to}, {formula}"
        );
    }
}

#[test]
fn table_definitions_survive_xlsx_and_source_snapshots() {
    // Original input values and formula text. SpreadsheetML defines these records
    // separately from sheet cells.
    // https://learn.microsoft.com/en-us/office/open-xml/spreadsheet/working-with-tables
    let bytes = rewrite_table(|xml| {
        xml.replace(
            "<tableColumns",
            r#"<autoFilter ref="A1:B3"><filterColumn colId="0" hiddenButton="1" showButton="false"><filters blank="true" calendarType="gregorian"><filter val=" A&amp;&quot; "/><filter val=""/></filters></filterColumn></autoFilter><tableColumns"#,
        )
        .replace(
            r#"<tableColumn id="2" name="Twice"/>"#,
            r#"<tableColumn id="2" name="Twice"><calculatedColumnFormula array="1">IF([@Amount]&lt;7,&quot;A&amp;B&quot;,&quot;&quot;)&#13;</calculatedColumnFormula></tableColumn>"#,
        )
        .replace(
            "</table>",
            r#"<tableStyleInfo name="TableStyleMedium2" showFirstColumn="1" showLastColumn="0" showRowStripes="true" showColumnStripes="false"/></table>"#,
        )
    });
    let book = read_workbook_bytes::<Workbook>(&bytes).unwrap();
    let table = &book.sheet(0).unwrap().tables()[0];
    let style = table.style.as_ref().unwrap();
    assert_eq!(style.name.as_deref(), Some("TableStyleMedium2"));
    assert_eq!(
        [
            style.show_first_column,
            style.show_last_column,
            style.show_row_stripes,
            style.show_column_stripes
        ],
        [Some(true), Some(false), Some(true), Some(false)]
    );
    let filter = table.auto_filter.as_ref().unwrap();
    assert_eq!(
        filter.range,
        Some(("A1".parse().unwrap(), "B3".parse().unwrap()))
    );
    let column = &filter.columns[0];
    assert_eq!(
        (column.index, column.hidden_button, column.show_button),
        (0, Some(true), Some(false))
    );
    let criteria = column.filters.as_ref().unwrap();
    assert_eq!(criteria.blank, Some(true));
    assert_eq!(criteria.values, [" A&\" ", ""]);
    let formula = table.columns[1].calculated_formula.as_ref().unwrap();
    assert_eq!(formula.array, Some(true));
    assert_eq!(formula.text, concat!(r#"IF([@Amount]<7,"A&B","")"#, "\r"));
    for restored in [
        read_workbook_bytes::<Workbook>(&write_xlsx_to_buffer(&book).unwrap()).unwrap(),
        Workbook::restore_snapshot()
            .from_bytes(&book.save_snapshot().to_buffer().unwrap())
            .unwrap()
            .into_evaluation(Default::default())
            .unwrap()
            .evaluation
            .into_workbook(),
    ] {
        assert_eq!(
            restored.sheet(0).unwrap().tables(),
            std::slice::from_ref(table)
        );
        assert_eq!(
            restored.sheet(0).unwrap().get("B2").unwrap(),
            book.sheet(0).unwrap().get("B2").unwrap()
        );
    }
}

#[test]
fn empty_filters_are_distinct_from_missing_filters_and_unmodeled_criteria_warn() {
    let base = read_workbook_bytes::<Workbook>(FIXTURE).unwrap();
    assert!(base.sheet(0).unwrap().tables()[0].auto_filter.is_none());
    for filter in [
        r#"<autoFilter ref="A1:B3"/>"#,
        r#"<autoFilter ref="A1:B3"><filterColumn colId="0"><dynamicFilter type="today"/></filterColumn></autoFilter>"#,
    ] {
        let bytes =
            rewrite_table(|xml| xml.replace("<tableColumns", &format!("{filter}<tableColumns")));
        let book = read_workbook_bytes::<Workbook>(&bytes).unwrap();
        let table = &book.sheet(0).unwrap().tables()[0];
        assert!(table.auto_filter.is_some());
        if filter.contains("dynamicFilter") {
            assert!(
                book.file_metadata
                    .as_ref()
                    .unwrap()
                    .import_warnings
                    .iter()
                    .any(|warning| warning
                        .detail
                        .contains("unmodeled metadata for Excel table"))
            );
        }
        let reopened =
            read_workbook_bytes::<Workbook>(&write_xlsx_to_buffer(&book).unwrap()).unwrap();
        assert_eq!(
            reopened.sheet(0).unwrap().tables(),
            std::slice::from_ref(table)
        );
    }
}

#[test]
fn malformed_table_filter_geometry_or_columns_are_rejected() {
    for filter in [
        r#"<autoFilter ref="A1:C3"/>"#,
        r#"<autoFilter ref="A1:B3"><filterColumn colId="2"/></autoFilter>"#,
        r#"<autoFilter ref="A1:B3"><filterColumn colId="0"/><filterColumn colId="0"/></autoFilter>"#,
    ] {
        let bytes =
            rewrite_table(|xml| xml.replace("<tableColumns", &format!("{filter}<tableColumns")));
        assert!(
            matches!(
                read_workbook_bytes::<Workbook>(&bytes),
                Err(IoError::Model(
                    poirot_io::ModelError::InvalidWorksheetTable { .. }
                ))
            ),
            "{filter}"
        );
    }
}

#[test]
fn table_definition_payloads_obey_export_and_snapshot_quotas() {
    use poirot::snapshots::{SnapshotError, SnapshotLimits};
    use poirot_io::{ExportLimitError, WriteOptions, plan_xlsx_export};

    let base = read_workbook_bytes::<Workbook>(FIXTURE).unwrap();
    let filter = format!(
        r#"<autoFilter ref="A1:B3"><filterColumn colId="0"><filters><filter val="{}"/></filters></filterColumn></autoFilter>"#,
        "p".repeat(10_000)
    );
    let bytes =
        rewrite_table(|xml| xml.replace("<tableColumns", &format!("{filter}<tableColumns")));
    let book = read_workbook_bytes::<Workbook>(&bytes).unwrap();
    let base_bytes = plan_xlsx_export(&base, WriteOptions::default())
        .unwrap()
        .source_bytes();
    let opts = WriteOptions {
        max_source_bytes: base_bytes,
        ..Default::default()
    };
    assert!(matches!(
        plan_xlsx_export(&book, opts),
        Err(IoError::Limit(ExportLimitError::SourceBytes { .. }))
    ));

    let limits = SnapshotLimits::default().max_decoded_bytes(4_000);
    let base_snapshot = base.save_snapshot().to_buffer().unwrap();
    assert!(
        Workbook::restore_snapshot()
            .limits(limits)
            .from_bytes(&base_snapshot)
            .is_ok()
    );
    let snapshot = book.save_snapshot().to_buffer().unwrap();
    assert!(matches!(
        Workbook::restore_snapshot()
            .limits(limits)
            .from_bytes(&snapshot),
        Err(SnapshotError::Quota(_))
    ));
}

#[test]
fn external_table_relationships_are_refused() {
    use poirot_io::opc::OpcError;

    let bytes = rewrite_fixture_part("xl/worksheets/_rels/sheet1.xml.rels", |xml| {
        xml.replace(
            r#"Target="../tables/table1.xml""#,
            r#"Target="https://example.invalid/table.xml" TargetMode="External""#,
        )
    });
    assert!(matches!(
        read_workbook_bytes::<Workbook>(&bytes),
        Err(IoError::Opc(OpcError::ExternalRelationship { .. }))
    ));
}

fn growable_table(formula: &str, filter_end: &str, array: bool) -> Workbook {
    let bytes = rewrite_table(|xml| {
        xml.replace(r#"ref="A1:B4""#, r#"ref="A1:B3""#)
            .replace(r#"totalsRowCount="1""#, r#"totalsRowCount="0""#)
            .replace("<tableColumns", &format!(r#"<autoFilter ref="A1:{filter_end}"/><tableColumns"#))
            .replace(
                r#"<tableColumn id="2" name="Twice"/>"#,
                &format!(r#"<tableColumn id="2" name="Twice"><calculatedColumnFormula array="{}">{formula}</calculatedColumnFormula></tableColumn>"#, u8::from(array)),
            )
    });
    let mut book = read_workbook_bytes::<Workbook>(&bytes).unwrap();
    book.sheet_mut(0).unwrap().remove("A4").unwrap();
    book
}

#[test]
fn table_calculated_columns_follow_sheet_rename_and_fill_new_rows() {
    use poirot::editing::FormulaSource;
    use poirot::model::CellValue;

    let mut book = growable_table("Inputs!A2", "B3", false);
    let inputs = book.add_sheet("Inputs").unwrap();
    inputs.set_value("A2", 12.).unwrap();
    inputs.set_value("A4", 34.).unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B2", "Inputs!A2")
        .unwrap();

    let outcome = book.rename_sheet(1, "Renamed Inputs").apply().unwrap();
    assert_eq!(
        outcome.changed_sources,
        [
            FormulaSource::Cell {
                sheet: 0,
                address: "B2".parse().unwrap(),
            },
            FormulaSource::CalculatedColumn {
                sheet: 0,
                table: 0,
                column: 1,
            },
        ]
    );

    // Reload before growing to exercise the table definition written to XLSX.
    let mut reopened =
        read_workbook_bytes::<Workbook>(&write_xlsx_to_buffer(&book).unwrap()).unwrap();
    let sheet = reopened.sheet_mut(0).unwrap();
    let formula = sheet.tables()[0].columns[1]
        .calculated_formula
        .as_ref()
        .unwrap();
    assert_eq!(formula.text, "'Renamed Inputs'!A2");
    assert_eq!(formula.array, Some(false));
    sheet.grow_table_rows("Sales", 1).unwrap();
    reopened.recalculate().unwrap();
    for (address, formula, value) in [
        ("B2", "'Renamed Inputs'!A2", 12.),
        ("B4", "'Renamed Inputs'!A4", 34.),
    ] {
        let cell = reopened.stored_cell("Data", address).unwrap().unwrap();
        assert_eq!(cell.formula.as_deref(), Some(formula));
        assert_eq!(cell.value, CellValue::Number(value));
    }
}

#[test]
fn translating_a_calculated_column_changes_its_definition() {
    use poirot::editing::{FormulaLocation, FormulaSource, TranslationRequest};

    let mut book = growable_table("A2*2", "B3", false);
    let source = FormulaSource::CalculatedColumn {
        sheet: 0,
        table: 0,
        column: 1,
    };
    let original = book.stored_cell("Data", "B2").unwrap().cloned();
    let changed = book
        .translate_formulas()
        .request(TranslationRequest {
            source: source.clone(),
            from: FormulaLocation { row: 1, column: 1 },
            to: FormulaLocation { row: 2, column: 1 },
        })
        .apply()
        .unwrap();
    assert_eq!(changed, [source]);
    assert_eq!(book.stored_cell("Data", "B2").unwrap().cloned(), original);
    let table = &book.sheet(0).unwrap().tables()[0];
    let formula = table.columns[1].calculated_formula.as_ref().unwrap();
    assert_eq!(formula.text, "A3*2");
    assert_eq!(formula.array, Some(false));
}

#[test]
fn unsupported_calculated_column_formula_rejects_rename_atomically() {
    use poirot::editing::{EditError, FormulaSource};

    let mut book = growable_table("Inputs!A2+", "B3", false);
    book.add_sheet("Inputs").unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B2", "Inputs!A2")
        .unwrap();
    let before = book.clone();
    assert!(matches!(
        book.rename_sheet(1, "Renamed Inputs").apply(),
        Err(EditError::Formula {
            target: FormulaSource::CalculatedColumn {
                sheet: 0,
                table: 0,
                column: 1,
            },
            ..
        })
    ));
    assert_eq!(book, before);
}

#[test]
fn explicit_table_growth_fills_calculated_columns_and_updates_spills() {
    use poirot::model::{CellStyle, CellValue};

    let mut book = growable_table("A2*2+$E$1", "B3", false);
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_value("E1", 1.).unwrap();
    sheet
        .set_formula("G1", "MAP(Sales[Amount],LAMBDA(x,x*10))")
        .unwrap();
    let style = CellStyle {
        number_format: Some("0.00".into()),
        ..Default::default()
    };
    sheet.set_style("B4", style.clone()).unwrap();
    let original = sheet.get("B2").unwrap().unwrap().formula.clone();
    book.recalculate().unwrap();
    assert_eq!(
        book.sheet(0).unwrap().spill_extent("G1".parse().unwrap()),
        Some("G2".parse().unwrap())
    );
    let sheet = book.sheet_mut(0).unwrap();
    sheet.grow_table_rows("sALES", 2).unwrap();
    let table = &sheet.tables()[0];
    assert_eq!(table.last, "B5".parse().unwrap());
    assert_eq!(
        table.auto_filter.as_ref().unwrap().range.unwrap().1,
        "B5".parse().unwrap()
    );
    assert_eq!(sheet.get("B2").unwrap().unwrap().formula, original);
    for (address, formula) in [("B4", "A4*2+$E$1"), ("B5", "A5*2+$E$1")] {
        assert_eq!(
            sheet.get(address).unwrap().unwrap().formula.as_deref(),
            Some(formula)
        );
    }
    assert_eq!(sheet.get("B4").unwrap().unwrap().style, style);
    sheet.set_value("A4", 8.).unwrap();
    sheet.set_value("A5", 9.).unwrap();
    book.recalculate().unwrap();
    let sheet = book.sheet(0).unwrap();
    assert_eq!(
        sheet.get("D1").unwrap().unwrap().value,
        CellValue::Number(27.)
    );
    assert_eq!(
        sheet.get("B4").unwrap().unwrap().value,
        CellValue::Number(17.)
    );
    assert_eq!(
        sheet.get("G4").unwrap().unwrap().value,
        CellValue::Number(90.)
    );
    assert_eq!(
        sheet.spill_extent("G1".parse().unwrap()),
        Some("G4".parse().unwrap())
    );

    let mut reopened =
        read_workbook_bytes::<Workbook>(&write_xlsx_to_buffer(&book).unwrap()).unwrap();
    assert_eq!(reopened.sheet(0).unwrap().tables(), sheet.tables());
    assert_eq!(
        reopened
            .sheet(0)
            .unwrap()
            .get("B5")
            .unwrap()
            .unwrap()
            .formula
            .as_deref(),
        Some("A5*2+$E$1")
    );
    reopened.recalculate().unwrap();
    assert_eq!(
        reopened.sheet(0).unwrap().get("G4").unwrap().unwrap().value,
        CellValue::Number(90.)
    );
    let mut scenario = reopened.overlay().build().unwrap();
    let input = scenario.target("Data!A5").unwrap();
    let sum = scenario.target("Data!D1").unwrap();
    let calculated = scenario.target("Data!B5").unwrap();
    let context = poirot::calculation::EvaluationContext::default();
    assert_eq!(
        scenario.evaluation(&context).unwrap().cell(sum).unwrap(),
        CellValue::Number(27.)
    );
    scenario.set_value(input, 10.0).unwrap();
    let mut evaluation = scenario.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(sum).unwrap(), CellValue::Number(28.));
    assert_eq!(evaluation.cell(calculated).unwrap(), CellValue::Number(21.));
}

#[test]
fn table_growth_refuses_partial_and_conflicting_edits_atomically() {
    use poirot::model::TableGrowthError as Error;
    use poirot_io::data::SheetSink;

    let mut book = growable_table("A1048574", "B3", false);
    let before = book.clone();
    assert!(matches!(
        book.sheet_mut(0).unwrap().grow_table_rows("Sales", 2),
        Err(Error::Formula { address, .. }) if address == "B5".parse().unwrap()
    ));
    assert_eq!(book, before);

    let mut book = growable_table("A2*2", "B3", false);
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_value("A4", 100.).unwrap();
    assert_eq!(sheet.tables()[0].last, "B3".parse().unwrap());
    let before = sheet.clone();
    assert!(matches!(
        sheet.grow_table_rows("Sales", 1),
        Err(Error::Occupied { address }) if address == "A4".parse().unwrap()
    ));
    assert!(matches!(
        sheet.grow_table_rows("Sales", u32::MAX),
        Err(Error::RowBounds)
    ));
    assert_eq!(*sheet, before);

    sheet.remove("A4").unwrap();
    sheet.set_formula("A4", "SEQUENCE(1,3)").unwrap();
    sheet
        .set_spill_extent(
            "A4".parse().unwrap(),
            "C4".parse().unwrap(),
            "SEQUENCE(1,3)",
        )
        .unwrap();
    let before = sheet.clone();
    assert!(matches!(
        sheet.grow_table_rows("Sales", 1),
        Err(Error::Spill { anchor }) if anchor == "A4".parse().unwrap()
    ));
    assert_eq!(*sheet, before);
    sheet.remove("A4").unwrap();

    let mut peer = sheet.tables()[0].clone();
    peer.id = 2;
    peer.name = None;
    peer.display_name = "NextTable".into();
    peer.first = "A5".parse().unwrap();
    peer.last = "B6".parse().unwrap();
    peer.auto_filter = None;
    SheetSink::import_table(sheet, peer).unwrap();
    let before = sheet.clone();
    assert!(matches!(
        sheet.grow_table_rows("Sales", 2),
        Err(Error::InvalidTable(_))
    ));
    assert_eq!(*sheet, before);

    let mut original = read_workbook_bytes::<Workbook>(FIXTURE).unwrap();
    let mut partial = growable_table("A2*2", "B2", false);
    let mut array = growable_table("A2*2", "B3", true);
    assert!(matches!(
        original.sheet_mut(0).unwrap().grow_table_rows("Sales", 1),
        Err(Error::TotalsRow)
    ));
    assert!(matches!(
        partial.sheet_mut(0).unwrap().grow_table_rows("Sales", 1),
        Err(Error::FilterRange)
    ));
    assert!(matches!(
        array.sheet_mut(0).unwrap().grow_table_rows("Sales", 1),
        Err(Error::ArrayFormula { .. })
    ));
}
