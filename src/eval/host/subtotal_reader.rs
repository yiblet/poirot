//! Budgeted metadata reads precede value evaluation, so excluded errors stay unevaluated.
use super::call_host::CallHost;
use super::operand_reader::OperandReader;
use super::scalar_operand::ScalarOperand;
use crate::eval::compile::compiler::PreparedFormula;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{EvaluationError, WorkKind};
use crate::eval::ports::DependencyRead;
use crate::eval::session::ReadRecorder;
use crate::eval::values::signed_reference::GridRead;
use crate::eval::values::{FunctionResult, VALUE_ERROR, Value};
use crate::model::CellAddress;
use crate::model::CellError::Ref;

/// Reads SUBTOTAL members after checking row visibility and nested-subtotal metadata.
pub(in crate::eval) struct SubtotalReader<'h, 'call, 'workbook> {
    /// The call whose collection argument is read.
    host: &'h mut CallHost<'call, 'workbook>,
}

impl<'h, 'call, 'workbook> SubtotalReader<'h, 'call, 'workbook> {
    /// Borrow the call host for this executor's reads.
    pub(in crate::eval) fn new(host: &'h mut CallHost<'call, 'workbook>) -> Self {
        Self { host }
    }

    /// Check subtotal metadata before reading a member, leaving excluded cells unevaluated.
    pub(in crate::eval) fn subtotal_cell(
        &mut self,
        index: usize,
        offset: usize,
        exclude_hidden: bool,
    ) -> FunctionResult<Option<ScalarOperand>> {
        self.host.prepare(index)?;
        let slot = self
            .host
            .argument_base
            .checked_add(index)
            .ok_or(EvaluationError::StalePlan)?;
        let runtime = &mut *self.host.session.state.runtime;
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
            self.host.charge_work(1)?;
            ReadRecorder::new(self.host.session).read(DependencyRead::SubtotalMetadata(
                EvaluationTarget { sheet, address },
            ));
            let visibility = self
                .host
                .session
                .sources
                .workbook
                .sheet(sheet)
                .ok_or(Ref)?
                .row_visibility(address.row);
            if visibility.filtered_out || (exclude_hidden && visibility.manually_hidden) {
                return Ok(None);
            }
            let cell = crate::eval::inputs::EffectiveWorkbook::new(
                self.host.session.sources.workbook,
                self.host.session.sources.inputs,
            )
            .cell(sheet, address);
            if let Some(source) = cell.and_then(|cell| cell.formula.as_deref()) {
                self.host.session.state.runtime.budget.text(source.len())?;
                self.host.charge_work(source.len() as u64)?;
                let prepared = self
                    .host
                    .session
                    .sources
                    .plan
                    .and_then(|plan| plan.formula(EvaluationTarget { sheet, address }))
                    .cloned()
                    .unwrap_or_else(|| self.host.session.sources.prepare_source(source));
                let nested = match prepared.as_ref() {
                    PreparedFormula::Ready(formula) => {
                        let program = formula.bytecode();
                        self.host.charge_work(program.calls.len() as u64)?;
                        program.calls.iter().any(|call| {
                            matches!(
                                call.target,
                                crate::eval::compile::bytecode::CallTarget::Builtin(
                                    crate::eval::functions::registry::FunctionId::Criteria(
                                        crate::eval::functions::registry::CriteriaFunction::Subtotal
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
        OperandReader::new(self.host)
            .collection_cell(index, offset)
            .map(Some)
    }
}
