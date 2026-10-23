//! Original public-facade contracts with explicit caller-provided saved results.
use poirot::{CellValue, EvaluationContext, OverlayPolicy, ProducerPolicy, Workbook};
#[test]
fn workbook_policy_controls_recalculation_and_xlsx_keeps_saved_values() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_formula_with_cached_value("A1".parse().unwrap(), "{1,2}", 10.0.into())
        .unwrap();
    sheet
        .set_spill_extent("A1".parse().unwrap(), "B1".parse().unwrap(), "{1,2}")
        .unwrap();
    sheet
        .set_spill_cached_value(
            "B1".parse().unwrap(),
            "A1".parse().unwrap(),
            "{1,2}",
            20.0.into(),
        )
        .unwrap();
    sheet
        .set_formula("D1".parse().unwrap(), "SUM(A1#)")
        .unwrap();
    let policy = OverlayPolicy {
        producers: ProducerPolicy::TrustSaved,
        ..Default::default()
    };
    book.recalculate_with_policy(EvaluationContext::default(), policy)
        .unwrap();
    assert_eq!(
        book.cell("S", "D1".parse().unwrap()).unwrap().value,
        CellValue::Number(30.0)
    );
    let bytes = book.save_xlsx().to_buffer().unwrap();
    let reopened = Workbook::from_bytes(&bytes).unwrap();
    assert_eq!(
        reopened.cell("S", "B1".parse().unwrap()).unwrap().value,
        CellValue::Number(20.0)
    );
    book.recalculate().unwrap();
    assert_eq!(
        book.cell("S", "D1".parse().unwrap()).unwrap().value,
        CellValue::Number(3.0)
    );
}
#[test]
fn incomplete_saved_array_aborts_workbook_cache_publication_atomically() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_formula_with_cached_value("A1".parse().unwrap(), "{1,2}", 10.0.into())
        .unwrap();
    sheet
        .set_spill_extent("A1".parse().unwrap(), "B1".parse().unwrap(), "{1,2}")
        .unwrap();
    sheet
        .set_formula_with_cached_value("D1".parse().unwrap(), "1+2", 99.0.into())
        .unwrap();
    let policy = OverlayPolicy {
        producers: ProducerPolicy::TrustSaved,
        ..Default::default()
    };
    assert!(
        book.recalculate_with_policy(EvaluationContext::default(), policy)
            .is_err()
    );
    assert_eq!(
        book.cell("S", "A1".parse().unwrap()).unwrap().value,
        CellValue::Number(10.0)
    );
    assert_eq!(
        book.cell("S", "D1".parse().unwrap()).unwrap().value,
        CellValue::Number(99.0)
    );
}
