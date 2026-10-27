use super::super::builtin_invocation_observer::BuiltinInvocationObserver;
use super::super::consumed_builtin_invocation::ConsumedBuiltinInvocation;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::functions::registry::FunctionId;
use crate::eval::limits::{BudgetFailure, BudgetKind, EvaluationFailure};
use crate::model::{CellAddress, Workbook};

/// Owned builtin entry evidence used to compare observed call identities.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Entry {
    pub(super) target: EvaluationTarget,
    pub(super) handler: FunctionId,
    pub(super) parsed: String,
    pub(super) resolved: String,
    pub(super) arity: usize,
}

/// Test invocation observer admitting entry and text capacity before copying events.
pub(super) struct Bounded {
    pub(super) entries: Vec<Entry>,
    pub(super) maximum_entries: usize,
    pub(super) maximum_bytes: usize,
    pub(super) retained_bytes: usize,
}

pub(super) const A1: CellAddress = CellAddress { row: 0, col: 0 };

pub(super) const B1: CellAddress = CellAddress { row: 0, col: 1 };

pub(super) fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}

pub(super) fn plan(source: &str) -> crate::eval::compile::bound::FormulaPlan {
    crate::eval::compile::bound::FormulaPlan::bind(
        &poirot_parsing::formula::parse_formula(source).unwrap(),
    )
    .unwrap()
}

pub(super) fn names(observer: &Bounded) -> Vec<&str> {
    observer
        .entries
        .iter()
        .map(|entry| entry.handler.canonical_name())
        .collect()
}

impl Bounded {
    pub(super) fn new(maximum_entries: usize, maximum_bytes: usize) -> Self {
        Self {
            entries: Vec::new(),
            maximum_entries,
            maximum_bytes,
            retained_bytes: 0,
        }
    }
}

impl BuiltinInvocationObserver for Bounded {
    fn consumed(&mut self, call: ConsumedBuiltinInvocation<'_>) -> Result<(), EvaluationFailure> {
        let bytes = call
            .parsed_spelling
            .len()
            .saturating_add(call.resolved_spelling.len());
        let (kind, limit, consumed, requested) = if self.entries.len() >= self.maximum_entries {
            (
                BudgetKind::Materialization,
                self.maximum_entries,
                self.entries.len(),
                1,
            )
        } else if bytes > self.maximum_bytes.saturating_sub(self.retained_bytes) {
            (
                BudgetKind::TextBytes,
                self.maximum_bytes,
                self.retained_bytes,
                bytes,
            )
        } else {
            // Capacity is checked before any allocation/copy.
            self.entries.push(Entry {
                target: call.target,
                handler: call.handler,
                parsed: call.parsed_spelling.to_owned(),
                resolved: call.resolved_spelling.to_owned(),
                arity: call.argument_count,
            });
            self.retained_bytes += bytes;
            return Ok(());
        };
        Err(EvaluationFailure::Budget(BudgetFailure {
            kind,
            limit: limit as u64,
            consumed: consumed as u64,
            requested: requested as u64,
            target: Some(call.target),
        }))
    }
}
