//! Reference construction from decoded or bound reference geometry.

use super::reference_ref::ReferenceRef;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationFailure, WorkKind};
use crate::eval::ports::DependencyRead;
use crate::eval::session::{NameResolver, ReadRecorder, Session};
use crate::eval::values::{Area, AreaKind, ReferenceAreas, Value, err};
use crate::model::CellError::Ref;
use crate::model::{CellAddress, MAX_COLUMNS, MAX_ROWS};

/// Resolves borrowed reference geometry against workbook sheets into value channels.
pub(in crate::eval) struct ReferenceAtoms<'s, 'a> {
    session: &'s mut Session<'a>,
}

impl<'s, 'a> ReferenceAtoms<'s, 'a> {
    /// Borrow the session whose sheets, reads and budget reference construction uses.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Resolve borrowed reference geometry against workbook sheets into a value channel.
    pub(in crate::eval) fn resolve(
        &mut self,
        s: usize,
        at: CellAddress,
        reference: ReferenceRef<'_>,
    ) -> EvalResult<Value> {
        match reference {
            ReferenceRef::SheetSpan {
                first_sheet,
                last_sheet,
                start,
                end,
            } => {
                ReadRecorder::new(self.session).read_lazy(|| DependencyRead::SheetLayout);
                self.session
                    .state
                    .runtime
                    .budget
                    .text(first_sheet.len().saturating_add(last_sheet.len()))?;
                let first = NameResolver::new(self.session).sheet_named(s, Some(first_sheet))?;
                let last = NameResolver::new(self.session).sheet_named(s, Some(last_sheet))?;
                let (first, last) = match (first, last) {
                    (Some(first), Some(last)) => (first, last),
                    // HSSF's missing first local endpoint makes getExternalSheet
                    // return null. OperationEvaluationContext then selects sheet 0
                    // alone, discarding the surviving last endpoint. The retained
                    // native probe observes SUM(#REF!A1) evaluating its first sheet.
                    (None, Some(_)) => (0, 0),
                    (first, None) => {
                        return Err(EvaluationFailure::UnsupportedCapability {
                            capability: CapabilityId::FunctionAdapter,
                            source: Some(EvaluationTarget { sheet: s, address: at }),
                            detail: if first.is_some() {
                                "pinned HSSF missing last sheet endpoint throws java.lang.IllegalArgumentException during formula authoring"
                            } else {
                                "both sheet-span endpoints unresolved; native classification unavailable"
                            }.into(),
                        });
                    }
                };
                if last < first {
                    return Err(EvaluationFailure::UnsupportedCapability {
                        capability: CapabilityId::FunctionAdapter,
                        source: Some(EvaluationTarget {
                            sheet: s,
                            address: at,
                        }),
                        detail: "pinned HSSF reversed sheet endpoints throw java.lang.NegativeArraySizeException during evaluation"
                            .into(),
                    });
                }
                let count = last - first + 1;
                self.session.state.runtime.budget.allocation(count)?;
                let is_area = end.is_some();
                let end = end.unwrap_or(start);
                let mut areas = Vec::with_capacity(count);
                for sheet in first..=last {
                    self.session
                        .state
                        .runtime
                        .budget
                        .charge(WorkKind::ReferencePair, 1)?;
                    areas.push(Area {
                        kind: if is_area {
                            AreaKind::Area
                        } else {
                            AreaKind::Cell
                        },
                        sheet,
                        r0: start.row.min(end.row),
                        r1: start.row.max(end.row),
                        c0: start.column.min(end.column),
                        c1: start.column.max(end.column),
                    });
                }
                Ok(Value::SheetSpan(areas.into()))
            }
            ReferenceRef::Cell { row, column, sheet } => Ok(
                match NameResolver::new(self.session).sheet_named(s, sheet)? {
                    Some(sheet) => {
                        self.session.state.runtime.budget.allocation(1)?;
                        Value::Refs(ReferenceAreas::One(Area {
                            kind: AreaKind::Cell,
                            sheet,
                            r0: row,
                            r1: row,
                            c0: column,
                            c1: column,
                        }))
                    }
                    None => err(Ref),
                },
            ),
            ReferenceRef::Column { column, sheet } => Ok(
                match NameResolver::new(self.session).sheet_named(s, sheet)? {
                    Some(sheet) => {
                        self.session.state.runtime.budget.allocation(1)?;
                        Value::Refs(ReferenceAreas::One(Area {
                            kind: AreaKind::Area,
                            sheet,
                            r0: 0,
                            r1: MAX_ROWS - 1,
                            c0: column,
                            c1: column,
                        }))
                    }
                    None => err(Ref),
                },
            ),
            ReferenceRef::Row { row, sheet } => Ok(
                match NameResolver::new(self.session).sheet_named(s, sheet)? {
                    Some(sheet) => {
                        self.session.state.runtime.budget.allocation(1)?;
                        Value::Refs(ReferenceAreas::One(Area {
                            kind: AreaKind::Area,
                            sheet,
                            r0: row,
                            r1: row,
                            c0: 0,
                            c1: MAX_COLUMNS - 1,
                        }))
                    }
                    None => err(Ref),
                },
            ),
        }
    }
}
