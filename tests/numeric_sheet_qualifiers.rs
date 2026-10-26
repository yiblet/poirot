//! Numeric-leading sheet references follow Microsoft's cell-reference grammar:
//! https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/3c420ebb-6ef1-4b0d-959d-76e88c841c3e
//! Original cases: cell reads, SUM and relative copy expectations derive from the
//! explicit input values and A1 offsets; no upstream fixture or algorithm reused.
use poirot::{
    CellAddress, CellValue, Workbook, eval,
    parsing::formula::{FormulaLocation, TranslationPlan, TranslationPolicy},
};
use std::sync::Arc;

fn at(source: &str) -> CellAddress {
    source.parse().unwrap()
}

#[test]
fn numeric_sheet_references_translate_evaluate_and_round_trip() {
    let mut book = Workbook::new();
    for name in ["2024", "1Q", "1e3", "Results"] {
        book.add_sheet(name).unwrap();
    }
    let source = book.sheet_by_name_mut("2024").unwrap();
    source.set_value(at("A1"), 7.).unwrap();
    source.set_value(at("A2"), 9.).unwrap();
    book.sheet_by_name_mut("1Q")
        .unwrap()
        .set_value(at("B2"), 11.)
        .unwrap();
    book.sheet_by_name_mut("1e3")
        .unwrap()
        .set_value(at("A1"), 24.)
        .unwrap();
    let copied = TranslationPlan::new(
        "2024!A1",
        FormulaLocation { row: 0, column: 1 },
        FormulaLocation { row: 1, column: 1 },
        TranslationPolicy::default(),
    )
    .unwrap();
    let formulas = [
        "2024!A1",
        copied.replacement(),
        "SUM(2024!A1,1Q!$B$2,1e3!A1)+1e3",
    ];
    for (address, formula) in ["B1", "B2", "C1"].into_iter().zip(formulas) {
        book.sheet_by_name_mut("Results")
            .unwrap()
            .set_formula(at(address), formula)
            .unwrap();
    }
    book.recalculate().unwrap();
    let bytes = book.save_xlsx().to_buffer().unwrap();
    let mut reopened = Workbook::from_bytes(&bytes).unwrap();
    for candidate in [&book, &reopened] {
        for ((address, expected), formula) in [("B1", 7.), ("B2", 9.), ("C1", 1042.)]
            .into_iter()
            .zip(formulas)
        {
            let cell = candidate.cell("Results", at(address)).unwrap();
            assert_eq!(cell.value, CellValue::Number(expected));
            assert_eq!(cell.formula.as_deref(), Some(formula));
        }
    }
    reopened
        .sheet_by_name_mut("2024")
        .unwrap()
        .set_value(at("A1"), 13.)
        .unwrap();
    reopened.recalculate().unwrap();
    for (address, expected) in [("B1", 13.), ("B2", 9.), ("C1", 1048.)] {
        assert_eq!(
            reopened.cell("Results", at(address)).unwrap().value,
            CellValue::Number(expected)
        );
    }
}

#[test]
fn compiler_reuses_numeric_qualifiers_without_erasing_sheet_identity() {
    let mut compiler = eval::FormulaCompiler::default();
    let policy = eval::EvaluationPolicy::default();
    let compiled: Vec<_> = [
        "SUM(2024!A1)",
        "SUM(2024!A2)",
        "SUM(2025!A2)",
        "1e3!A1+1e3",
        "1e3!B2+1e3",
    ]
    .into_iter()
    .map(|source| compiler.compile_formula(source, &policy))
    .collect();
    let prepared: Vec<_> = compiled
        .iter()
        .map(|formula| {
            let eval::PreparedFormula::Ready(formula) = formula.as_ref() else {
                panic!("numeric sheet qualifiers must compile")
            };
            formula.bytecode()
        })
        .collect();
    assert!(Arc::ptr_eq(
        &prepared[0].executable,
        &prepared[1].executable
    ));
    assert_ne!(prepared[0].references, prepared[1].references);
    // Sheet identity lives in each formula's bindings even when code is shared.
    assert_ne!(prepared[1].references, prepared[2].references);
    assert!(Arc::ptr_eq(
        &prepared[3].executable,
        &prepared[4].executable
    ));
    assert_ne!(prepared[3].references, prepared[4].references);
}
