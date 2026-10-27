use super::super::producer_execution::ProducerExecution;
use crate::eval::config::ArrayResultPolicy;
use crate::eval::session::Session;
use crate::model::{CellAddress, Workbook};

#[test]
fn held_raw_results_have_no_ordinary_completion_or_ownership() {
    let mut book = Workbook::new();
    let anchor = CellAddress { row: 0, col: 0 };
    book.add_sheet("S")
        .unwrap()
        .set_formula(anchor, "{1,2;3,4}")
        .unwrap();
    let mut session = Session::with_context(&book, Default::default());
    session.policies.array_result_policy = ArrayResultPolicy::DynamicSpill;
    session.reuse.overlay_reads = Some(Default::default());
    session.state.runtime.memo.admitted = Some(Vec::new());
    ProducerExecution::new(&mut session)
        .compute(0, anchor)
        .unwrap();
    assert!(
        session
            .state
            .runtime
            .preparation
            .held
            .contains_key(&(0, anchor))
    );
    assert!(session.state.runtime.preparation.publication().is_err());
    assert!(session.state.runtime.memo.cache.is_empty());
    assert!(session.state.runtime.preparation.roles.is_empty());
    assert!(session.state.runtime.preparation.journal.is_empty());
    assert!(
        session
            .state
            .runtime
            .memo
            .admitted
            .as_ref()
            .unwrap()
            .is_empty()
    );
    assert!(
        session
            .reuse
            .overlay_reads
            .as_ref()
            .unwrap()
            .completed
            .is_empty()
    );
    assert!(
        session
            .reuse
            .overlay_reads
            .as_ref()
            .unwrap()
            .fresh_results
            .is_empty()
    );
    assert_eq!(session.state.runtime.budget.usage.completed_cells, 0);
    ProducerExecution::new(&mut session)
        .finalize(0, anchor)
        .unwrap();
    assert_eq!(session.state.runtime.memo.cache.len(), 4);
    assert_eq!(session.state.runtime.preparation.journal.len(), 4);
    assert_eq!(session.state.runtime.budget.usage.completed_cells, 4);
}
