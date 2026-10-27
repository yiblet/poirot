//! Typed model failures preserve rejected inputs and lower-layer causes.

use crate::model::model_error::ModelError;
use crate::model::query::{DenseBudget, QueryPlan, RangeMembership, WorkbookView};
use crate::model::{CellAddress, CellValue, MAX_ROWS, Workbook, WorkbookChange, Worksheet};
use poirot_io::ModelError as DataModelError;
use std::error::Error;

#[test]
fn names_indices_and_rows_have_inspectable_context() {
    fn assert_thread_safe<T: Send + Sync + 'static>() {}
    assert_thread_safe::<ModelError>();
    assert!(matches!(
        crate::model::ScalarKinds::try_from(32),
        Err(ModelError::InvalidScalarKindBits { bits: 32 })
    ));
    assert!(
        matches!(Worksheet::new("bad/name"), Err(DataModelError::InvalidWorksheetName { name }) if name == "bad/name")
    );
    let mut book = Workbook::new();
    book.add_sheet("Data").unwrap();
    assert!(
        matches!(book.add_sheet("data"), Err(ModelError::DuplicateWorksheetName { name }) if name == "data")
    );
    assert!(matches!(
        book.remove_sheet(7),
        Err(ModelError::InvalidWorksheetIndex { index: 7 })
    ));
    assert!(matches!(
        book.move_sheet(0, 8),
        Err(ModelError::InvalidWorksheetIndex { index: 8 })
    ));
    assert!(
        matches!(book.define_name("A1", "1", None), Err(ModelError::InvalidDefinedName { name }) if name == "A1")
    );
    book.define_name("Rate", "1", Some(0)).unwrap();
    assert!(
        matches!(book.define_name("rate", "2", Some(0)), Err(ModelError::DuplicateDefinedName { name, sheet_scope: Some(0) }) if name == "rate")
    );
    assert!(matches!(
        book.define_name("Tax", "1", Some(5)),
        Err(ModelError::InvalidWorksheetIndex { index: 5 })
    ));
    assert!(
        matches!(book.sheet_mut(0).unwrap().create_row(MAX_ROWS), Err(DataModelError::InvalidRow { row }) if row == MAX_ROWS)
    );
}

#[test]
fn dependency_causes_retain_their_original_types() {
    let mut book = Workbook::new();
    book.add_sheet("Σ").unwrap();
    let error = book.add_sheet("σ").unwrap_err();
    let cause = error
        .source()
        .unwrap()
        .downcast_ref::<poirot_io::identifiers::IdentifierError>()
        .unwrap();
    assert_eq!(cause.stored, "Σ");
    assert_eq!(cause.requested, "σ");
    assert!(matches!(error, ModelError::Identifier(_)));
    let error = book.stored_cell("Σ", "not a coordinate").unwrap_err();
    assert!(error.source().unwrap().is::<poirot_io::ModelError>());
    assert!(matches!(error, ModelError::Io(_)));
}

#[test]
fn queries_report_geometry_budget_and_missing_sheet() {
    let first = CellAddress { row: 1, col: 0 };
    let last = CellAddress { row: 0, col: 0 };
    assert!(
        matches!(RangeMembership::new(0, first, last), Err(ModelError::UnorderedQueryRange { first: a, last: b }) if a == first && b == last)
    );
    let area = RangeMembership::new(3, last, first).unwrap();
    let query = QueryPlan::new(vec![area]).unwrap();
    assert!(matches!(
        query.dense_size(DenseBudget { max_cells: 1 }),
        Err(ModelError::DenseQueryBudgetExceeded {
            requested: 2,
            limit: 1
        })
    ));
    let book = Workbook::new();
    assert!(matches!(
        WorkbookView::new(&book).stored(area),
        Err(ModelError::InvalidWorksheetIndex { index: 3 })
    ));
}

#[test]
fn cache_failures_preserve_coordinates_and_atomicity() {
    let mut book = Workbook::new();
    book.add_sheet("Data")
        .unwrap()
        .set_formula("A1", "1")
        .unwrap();
    let anchor: CellAddress = "A1".parse().unwrap();
    let child: CellAddress = "B1".parse().unwrap();
    let before = book.clone();
    assert!(
        matches!(book.apply([WorkbookChange::SetCachedValue { sheet: 0, address: child, expected_formula: "1".into(), value: CellValue::Number(2.) }]), Err(ModelError::MissingCachedValueTarget { sheet: 0, address }) if address == child)
    );
    assert_eq!(book, before);
    assert!(
        matches!(book.apply([WorkbookChange::SetCachedValue { sheet: 0, address: anchor, expected_formula: "2".into(), value: CellValue::Number(2.) }]), Err(ModelError::CachedValueFormulaChanged { sheet: 0, address }) if address == anchor)
    );
    assert_eq!(book, before);
    let sheet = book.sheet_mut(0).unwrap();
    assert!(
        matches!(sheet.set_spill_extent(child, anchor, "1"), Err(DataModelError::InvalidSpillExtent { anchor: a, last }) if a == child && last == anchor)
    );
    assert!(
        matches!(sheet.set_spill_cached_value(anchor, anchor, "1", CellValue::Blank), Err(DataModelError::InvalidSpillCacheAnchor { address, owner }) if address == anchor && owner == anchor)
    );
    sheet.set_value(child, 9.).unwrap();
    assert!(
        matches!(sheet.set_spill_cached_value(child, anchor, "1", CellValue::Blank), Err(DataModelError::SpillCacheAuthoredCell { address }) if address == child)
    );
}
