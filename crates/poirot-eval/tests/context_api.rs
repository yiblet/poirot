//! One full context crosses the facade, direct, and borrowed-evaluation boundaries.
use poirot_eval::overlay::{Evaluation, Overlay};
use poirot_eval::{EvaluationContext, EvaluationInputs, EvaluationTarget, RunConfiguration};
use poirot_model::{CellValue, Workbook};
use std::time::{Duration, UNIX_EPOCH};

#[test]
fn canonical_context_preserves_run_settings_and_explicit_core_projection() {
    let inputs = EvaluationInputs {
        as_of_time: UNIX_EPOCH - Duration::from_nanos(1),
        randomness_seed: 17,
        ..Default::default()
    };
    let mut context = EvaluationContext::from(inputs);
    context.retention.cells = 8;
    context.budgets.run_work = 100_000;
    context.formatting =
        poirot_eval::FormattingConfiguration::with_currency(poirot_eval::CurrencyContext {
            decimal_separator: ".".into(),
            grouping_separator: ",".into(),
            primary_group: 3,
            secondary_group: 3,
            positive_prefix: "$".into(),
            positive_suffix: "".into(),
            negative_prefix: "($".into(),
            negative_suffix: ")".into(),
        });
    assert_eq!(context.inputs().fingerprint(), inputs.fingerprint());
    let configuration = RunConfiguration::new(context.inputs(), context.formatting.clone());
    assert_eq!(configuration.context().fingerprint(), inputs.fingerprint());
    assert_eq!(configuration.formatting(), &context.formatting);

    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula("A1".parse().unwrap(), "DOLLAR(12.5)", CellValue::Blank)
        .unwrap();
    let target = EvaluationTarget {
        sheet: 0,
        address: "A1".parse().unwrap(),
    };
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    assert_eq!(direct.context().retention.cells, 8);
    assert_eq!(direct.context().budgets.run_work, 100_000);
    let expected = CellValue::Text("$12.50".into());
    assert_eq!(direct.cell(target).unwrap(), expected);
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(target).unwrap(),
        expected
    );
}
