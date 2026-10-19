//! Bounded evidence of capabilities actually selected by the evaluator.
use crate::fods::corpus::FixtureEntry;
use crate::fods::{
    gaps::GapKind,
    report::{CaseFailure, FailureKind},
};
use poirot_eval::{
    CellCompletion, CellObserver, CompiledWorkbook, DependencyRead, DependencyReadSink,
    EvaluationFailure, PreparedFormula, ReadEvent,
};
use std::cell::RefCell;

const MAX_NAMES: usize = 16;
const MAX_DETAIL_CHARS: usize = 512;

/// Shared observed runtime evidence used to classify unsupported calls and evaluation failures.
#[derive(Default)]
pub(crate) struct Trace {
    functions: Vec<String>,
    missing_name: Option<String>,
    syntax: Option<String>,
    external_sheet: Option<String>,
    matrix_cell: Option<poirot_eval::EvaluationTarget>,
    omitted_functions: bool,
}

fn detail(value: &str) -> String {
    value.chars().take(MAX_DETAIL_CHARS).collect()
}

impl Trace {
    /// Borrow the unresolved defined name actually consumed during evaluation, if any.
    pub(crate) fn unresolved_name(&self) -> Option<&str> {
        self.missing_name.as_deref()
    }

    /// Build a case diagnostic from observed execution evidence for the named phase.
    pub(crate) fn failure(&self, phase: &str) -> Option<CaseFailure> {
        let (kind, gap_type, message) = if let Some(sheet) = &self.external_sheet {
            (
                FailureKind::WorkbookContext,
                GapKind::UnsupportedWorkbookContext,
                format!("{phase}: selected external linked table {sheet}"),
            )
        } else if let Some(target) = self.matrix_cell {
            (
                FailureKind::MatrixContext,
                GapKind::UnsupportedArrayContext,
                format!(
                    "{phase}: selected unsupported Calc CSE cell at sheet {} row {} column {}",
                    target.sheet, target.address.row, target.address.col
                ),
            )
        } else if let Some(syntax) = &self.syntax {
            (
                FailureKind::SourceSyntax,
                GapKind::UnsupportedFormulaSyntax,
                format!("{phase}: executed unsupported source syntax: {syntax}"),
            )
        } else if !self.functions.is_empty() {
            (
                FailureKind::UnsupportedName,
                if phase == "Correct" {
                    GapKind::UnsupportedCheck
                } else {
                    GapKind::MissingFunction
                },
                format!(
                    "{phase}: executed missing function(s): {}{}",
                    self.functions.join(", "),
                    if self.omitted_functions {
                        " (additional names omitted)"
                    } else {
                        ""
                    }
                ),
            )
        } else {
            return None;
        };
        Some(CaseFailure {
            kind,
            gap_type,
            missing_functions: self.functions.clone(),
            message,
        })
    }
}

/// Dependency observer connecting consumed reads to fixture/name diagnostic context.
pub(crate) struct Reads<'a> {
    /// Records trace contention because the read-sink callback has no error channel.
    pub borrow_failed: &'a std::cell::Cell<bool>,
    /// Shared trace receiving consumed-resolution evidence.
    pub trace: &'a RefCell<Trace>,
    /// Prepared workbook/formula index used to interpret runtime reads.
    pub plan: &'a CompiledWorkbook,
    /// Fixture identity attached to diagnostic evidence.
    pub entry: &'a FixtureEntry,
    /// Canonical fixture workbook used for source-name resolution.
    pub book: &'a poirot_model::Workbook,
    /// Omitted source tables whose values must never become local inputs.
    pub omissions: &'a [crate::fods::xml::model::Omission],
}
impl DependencyReadSink for Reads<'_> {
    fn read(&mut self, event: ReadEvent) {
        let Ok(mut trace) = self.trace.try_borrow_mut() else {
            self.borrow_failed.set(true);
            return;
        };
        match event.read {
            DependencyRead::Sheet {
                name,
                selected: None,
                ..
            } => {
                if self.omissions.iter().any(|omission| {
                    omission.kind == crate::fods::xml::model::OmissionKind::ExternalLinkedTable
                        && omission.sheet.as_deref().is_some_and(|sheet| {
                            poirot_model::identifiers::compare(sheet, &name)
                                == poirot_model::identifiers::IdentifierComparison::Equal
                        })
                }) {
                    trace.external_sheet.get_or_insert_with(|| detail(&name));
                }
            }
            DependencyRead::OpaqueProvider(name) => {
                if intentional_unknown_provider(
                    self.entry,
                    self.plan,
                    self.book,
                    event.consumer,
                    &name,
                ) {
                    return;
                }
                let name = detail(&name);
                if !trace.functions.contains(&name) {
                    if trace.functions.len() < MAX_NAMES {
                        trace.functions.push(name);
                    } else {
                        trace.omitted_functions = true;
                    }
                }
            }
            DependencyRead::Name {
                name,
                selected: None,
                ..
            } => {
                trace.missing_name.get_or_insert_with(|| detail(&name));
            }
            DependencyRead::Name {
                name,
                selected: Some(index),
                ..
            } => {
                if matches!(
                    self.plan.defined_name(index).map(|p| p.as_ref()),
                    Some(PreparedFormula::ParseFailure { error, .. }) if error.kind == poirot_parsing::formula::ParseErrorKind::Syntax
                ) {
                    trace
                        .syntax
                        .get_or_insert_with(|| format!("defined name {}", detail(&name)));
                }
            }
            _ => {}
        }
    }
}

/// Cell-completion observer retaining fixture execution diagnostics.
pub(crate) struct Cells<'a> {
    /// Source CSE anchors and continuations requiring unavailable matrix context.
    pub matrix_cells: &'a std::collections::BTreeSet<(usize, poirot_model::CellAddress)>,
    /// Shared trace receiving cell completion/failure evidence.
    pub trace: &'a RefCell<Trace>,
    /// Prepared source index used to identify completed formulas.
    pub plan: &'a CompiledWorkbook,
}
impl CellObserver for Cells<'_> {
    fn completed(&mut self, completion: CellCompletion<'_>) -> Result<(), EvaluationFailure> {
        if self
            .matrix_cells
            .contains(&(completion.target.sheet, completion.target.address))
        {
            self.trace
                .try_borrow_mut()
                .map_err(|_| EvaluationFailure::StalePlan)?
                .matrix_cell
                .get_or_insert(completion.target);
        }
        // A geometry/metadata read is not a cell completion. Only actual source
        // evaluation can turn a parser failure into a synthetic spreadsheet error.
        if completion.result.is_ok()
            && matches!(
                self.plan.formula(completion.target).map(|p| p.as_ref()),
                Some(PreparedFormula::ParseFailure { error, .. }) if error.kind == poirot_parsing::formula::ParseErrorKind::Syntax
            )
        {
            self.trace
                .try_borrow_mut()
                .map_err(|_| EvaluationFailure::StalePlan)?
                .syntax
                .get_or_insert_with(|| {
                    format!(
                        "sheet {} row {} column {}",
                        completion.target.sheet,
                        completion.target.address.row,
                        completion.target.address.col
                    )
                });
        }
        Ok(())
    }
}

// The original source deliberately constructs an unknown function error at F7,
// then exercises ERROR.TYPE. DHFG is absent from the pinned Calc ODFF/English
// opcode maps (formula/inc/core_resource.hrc at cd35bc0879ed5e4b8138b724033ea5937751a84d).
// This source annotation is not an Expected-result shortcut or a global function
// exception: bind the exact immutable producer and original fixture digest.
fn intentional_unknown_provider(
    entry: &FixtureEntry,
    plan: &CompiledWorkbook,
    book: &poirot_model::Workbook,
    consumer: Option<poirot_eval::EvaluationTarget>,
    name: &str,
) -> bool {
    let Some(target) = consumer else {
        return false;
    };
    entry.fixture == "spreadsheet/fods/error.type.fods"
        && entry.sha256 == "d55728ea9c8dd4154120dddfffab26cc72412f6ad197be2f8b244a44115b14e9"
        && target.sheet == 1
        && target.address.row == 6
        && target.address.col == 5
        && name.eq_ignore_ascii_case("DHFG")
        && book
            .try_defined_name(name, target.sheet)
            .is_ok_and(|n| n.is_none())
        && plan
            .formula(target)
            .is_some_and(|p| p.source().eq_ignore_ascii_case("DHFG()"))
}
