//! Budgeted metadata reads precede value evaluation, so excluded errors stay unevaluated.
use super::*;
use crate::eval::signed_reference::GridRead;

impl CallHost<'_, '_> {
    /// Check subtotal metadata before reading a member, leaving excluded cells unevaluated.
    pub(in crate::eval) fn subtotal_cell(
        &mut self,
        index: usize,
        offset: usize,
        exclude_hidden: bool,
    ) -> FunctionResult<Option<ScalarOperand>> {
        self.prepare(index)?;
        let slot = self
            .argument_base
            .checked_add(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        let runtime = &mut *self.session.runtime;
        let coordinate = match &runtime
            .vm
            .operands
            .get(slot)
            .and_then(Option::as_ref)
            .ok_or(VALUE_ERROR)?
            .value
        {
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                let mut remaining = offset as u64;
                let mut selected = None;
                for area in areas {
                    runtime.budget.charge(WorkKind::ReferencePair, 1)?;
                    if remaining < area.size() {
                        let width = u64::from(area.c1 - area.c0 + 1);
                        selected = Some((
                            area.sheet,
                            CellAddress {
                                row: area.r0 + (remaining / width) as u32,
                                col: area.c0 + (remaining % width) as u32,
                            },
                        ));
                        break;
                    }
                    remaining -= area.size();
                }
                Some(selected.ok_or(Ref)?)
            }
            Value::SignedRefs(areas) => {
                let mut remaining = offset as u64;
                let mut selected = None;
                for area in areas {
                    runtime.budget.charge(WorkKind::ReferencePair, 1)?;
                    if remaining < area.size() {
                        selected = match area
                            .read(remaining / area.width(), remaining % area.width())
                            .ok_or(Ref)?
                        {
                            GridRead::Blank => None,
                            GridRead::Cell { sheet, address } => Some((sheet, address)),
                        };
                        break;
                    }
                    remaining -= area.size();
                }
                selected
            }
            _ => None,
        };
        if let Some((sheet, address)) = coordinate {
            self.charge_work(1)?;
            self.session
                .read(DependencyRead::SubtotalMetadata(EvaluationTarget {
                    sheet,
                    address,
                }));
            let visibility = self
                .session
                .workbook
                .sheet(sheet)
                .ok_or(Ref)?
                .row_visibility(address.row);
            if visibility.filtered_out || (exclude_hidden && visibility.manually_hidden) {
                return Ok(None);
            }
            let cell =
                run_overlay::EffectiveWorkbook::new(self.session.workbook, self.session.inputs)
                    .cell(sheet, address);
            if let Some(source) = cell.and_then(|cell| cell.formula.as_deref()) {
                self.session.budget.text(source.len())?;
                self.charge_work(source.len() as u64)?;
                let prepared = self
                    .session
                    .plan
                    .and_then(|plan| plan.formula(EvaluationTarget { sheet, address }))
                    .cloned()
                    .unwrap_or_else(|| self.session.prepare_source(source));
                let nested = match prepared.as_ref() {
                    PreparedFormula::Ready(formula) => {
                        let program = formula.bytecode();
                        self.charge_work(program.calls.len() as u64)?;
                        program.calls.iter().any(|call| {
                            matches!(
                                call.target,
                                crate::eval::bytecode::CallTarget::Builtin(
                                    crate::eval::registry::FunctionId::Criteria(
                                        crate::eval::registry::CriteriaFunction::Subtotal
                                    )
                                )
                            )
                        })
                    }
                    _ => false,
                };
                if nested {
                    return Ok(None);
                }
            }
        }
        self.collection_cell(index, offset).map(Some)
    }
}
