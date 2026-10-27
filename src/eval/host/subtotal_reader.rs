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

/// Metadata exclusions applied before a total reads a referenced cell.
#[derive(Clone, Copy)]
enum TotalFilter {
    /// SUBTOTAL always excludes nested subtotals and filtered rows.
    Subtotal { exclude_hidden: bool },
    /// AGGREGATE can exclude both nested SUBTOTAL and AGGREGATE calls;
    /// filtered rows are always excluded and hidden rows are optional.
    Aggregate {
        exclude_hidden: bool,
        exclude_nested: bool,
    },
}

/// Reads total members after checking row visibility and nested-total metadata.
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
        self.filtered_cell(index, offset, TotalFilter::Subtotal { exclude_hidden })
    }

    /// Validate AGGREGATE's reference form and reject unsupported 3-D references.
    pub(in crate::eval) fn aggregate_source(
        &mut self,
        index: usize,
        require_reference: bool,
    ) -> FunctionResult<()> {
        self.host.prepare(index)?;
        let value = &self.host.operand(index).ok_or(VALUE_ERROR)?.value;
        if matches!(value, Value::SheetSpan(_))
            || (require_reference && !matches!(value, Value::Refs(_) | Value::SignedRefs(_)))
        {
            return Err(VALUE_ERROR.into());
        }
        Ok(())
    }

    /// Apply AGGREGATE exclusions to references; calculated arrays retain every row.
    pub(in crate::eval) fn aggregate_cell(
        &mut self,
        index: usize,
        offset: usize,
        exclude_hidden: bool,
        exclude_nested: bool,
    ) -> FunctionResult<Option<ScalarOperand>> {
        self.filtered_cell(
            index,
            offset,
            TotalFilter::Aggregate {
                exclude_hidden,
                exclude_nested,
            },
        )
    }

    /// Read metadata first so excluded formulas never execute.
    fn filtered_cell(
        &mut self,
        index: usize,
        offset: usize,
        filter: TotalFilter,
    ) -> FunctionResult<Option<ScalarOperand>> {
        let (exclude_hidden, exclude_nested, aggregate) = match filter {
            TotalFilter::Subtotal { exclude_hidden } => (exclude_hidden, true, false),
            TotalFilter::Aggregate {
                exclude_hidden,
                exclude_nested,
            } => (exclude_hidden, exclude_nested, true),
        };
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
            if let Some(source) = cell
                .and_then(|cell| cell.formula.as_deref())
                .filter(|_| exclude_nested)
            {
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
                            ) || aggregate && matches!(
                                call.target,
                                crate::eval::compile::bytecode::CallTarget::Builtin(
                                    crate::eval::functions::registry::FunctionId::Criteria(
                                        crate::eval::functions::registry::CriteriaFunction::Aggregate
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
