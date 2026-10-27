use super::super::spill_access::SpillAccess;
use crate::eval::compile::compiler::PreparationPolicy;
use crate::eval::config::EvaluationInputs;
use crate::eval::session::Session;
use crate::model::Workbook;

#[test]
fn parser_resource_refusal_cannot_supply_a_narrow_or_absent_candidate() {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula("A1", "SEQUENCE(3,2)")
        .unwrap();
    let policy = PreparationPolicy {
        parse: poirot_parsing::formula::FormulaParsePolicy {
            max_expressions: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut session = Session::with_context(&book, EvaluationInputs::default());
    session.sources.preparation_policy = Some(&policy);
    let candidates = SpillAccess::new(&mut session)
        .producer_candidates(0)
        .unwrap();
    assert_eq!(candidates.len(), 1);
    let candidate = candidates.first().unwrap();
    assert_eq!(
        (candidate.area.r1, candidate.area.c1),
        (crate::model::MAX_ROWS - 1, crate::model::MAX_COLUMNS - 1)
    );
}

#[test]
fn syntax_failure_has_no_successful_spill_and_registry_failure_is_conservative() {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula("A1", "SEQUENCE(")
        .unwrap();
    let mut session = Session::with_context(&book, EvaluationInputs::default());
    assert!(
        SpillAccess::new(&mut session)
            .producer_candidates(0)
            .unwrap()
            .is_empty()
    );
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula("A1", "SEQUENCE(3)")
        .unwrap();
    let registry = crate::eval::functions::extensions::ExtensionRegistry::new();
    let policy = PreparationPolicy::with_extensions(Default::default(), &registry);
    let mut session = Session::with_context(&book, EvaluationInputs::default());
    session.sources.preparation_policy = Some(&policy);
    let candidates = SpillAccess::new(&mut session)
        .producer_candidates(0)
        .unwrap();
    assert_eq!(candidates.len(), 1);
    let candidate = candidates.first().unwrap();
    assert_eq!(
        (candidate.area.r1, candidate.area.c1),
        (crate::model::MAX_ROWS - 1, crate::model::MAX_COLUMNS - 1)
    );
}
