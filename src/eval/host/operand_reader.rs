//! Requested coordinate and collection reads preserve source order and per-read charges.
use super::call_host::CallHost;
use super::cell_read_plan::CellReadPlan;
use super::operand_description::OperandDescription;
use super::scalar_operand::ScalarOperand;
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationFailure, WorkKind};
use crate::eval::session::{ArgumentReader, CellEvaluator, SparseReader};
use crate::eval::spill::SpillDemand;
use crate::eval::values::{
    Area, AreaKind, FunctionResult, OperandOrigin, VALUE_ERROR, Value,
    convert_legacy_missing_blank, copy_cell,
};
use crate::model::CellError::{Num, Ref};
use crate::model::CellValue::{Blank, Text};
use crate::model::{CellAddress, CellError, CellValue, MAX_COLUMNS, MAX_ROWS};

/// Reads a call's deferred operands by shape, coordinate and collection offset.
pub(in crate::eval) struct OperandReader<'h, 'call, 'workbook> {
    /// The call whose arguments this executor reads.
    host: &'h mut CallHost<'call, 'workbook>,
}

impl<'h, 'call, 'workbook> OperandReader<'h, 'call, 'workbook> {
    /// Borrow the call host for this executor's reads.
    pub(in crate::eval) fn new(host: &'h mut CallHost<'call, 'workbook>) -> Self {
        Self { host }
    }

    /// Inspect whether one resolved operand has area-like provenance.
    pub(in crate::eval) fn is_area(&mut self, index: usize) -> FunctionResult<bool> {
        self.host.prepare(index)?;
        Ok(match &self.host.operand(index).ok_or(VALUE_ERROR)?.value {
            Value::Array(_) => true,
            // SAFETY: Short-circuit evaluation checks the vector length is exactly one before reading element zero.
            Value::SignedRefs(a) => a.len() == 1 && a[0].kind == AreaKind::Area,
            // SAFETY: Short-circuit evaluation checks the vector length is exactly one before reading element zero.
            Value::Refs(areas) => areas.len() == 1 && areas[0].kind == AreaKind::Area,
            Value::SheetSpan(areas) => areas.first().is_some_and(|a| a.kind == AreaKind::Area),
            Value::Missing | Value::Scalar(_) | Value::SelectedScalar(_, _) => false,
        })
    }

    /// Inspect operand shape, channel, sheet count, and omission without member reads.
    pub(in crate::eval) fn describe(&mut self, index: usize) -> FunctionResult<OperandDescription> {
        let (rows, columns) = self.shape(index)?;
        let is_area = self.is_area(index)?;
        let operand = &self.host.operand(index).ok_or(VALUE_ERROR)?;
        Ok(OperandDescription {
            rows,
            columns,
            kind: operand.kind(),
            is_area,
            sheets: match &operand.value {
                Value::SheetSpan(areas) => areas.len(),
                _ => 1,
            },
            missing: operand.missing,
        })
    }

    /// Enumerate stored reference positions, recording membership for future edits.
    /// Arrays and scalars return `None` and retain their existing dense traversal.
    pub(in crate::eval) fn stored_positions(
        &mut self,
        index: usize,
    ) -> FunctionResult<Option<Vec<(usize, usize)>>> {
        self.host.prepare(index)?;
        // External readers may supply values at physically absent source cells.
        // Workbook membership alone cannot prove those cells are blank.
        if self.host.session.ports.cell_reader.is_some() {
            return Ok(None);
        }
        let (area, row, column) = match &self.host.operand(index).ok_or(VALUE_ERROR)?.value {
            Value::Refs(areas) | Value::SheetSpan(areas) if areas.len() == 1 => {
                let area = *areas.first().ok_or(VALUE_ERROR)?;
                (Some(area), i64::from(area.r0), i64::from(area.c0))
            }
            Value::SignedRefs(areas) if areas.len() == 1 => {
                let area = *areas.first().ok_or(VALUE_ERROR)?;
                (area.physical(), i64::from(area.r0), i64::from(area.c0))
            }
            _ => return Ok(None),
        };
        let Some(area) = area else {
            return Ok(Some(Vec::new()));
        };
        let addresses = SparseReader::new(self.host.session).sparse_addresses(area)?;
        self.host.check_values_capacity(addresses.len())?;
        let positions = addresses
            .into_iter()
            .map(|at| {
                Ok((
                    usize::try_from(i64::from(at.row) - row).map_err(|_| Ref)?,
                    usize::try_from(i64::from(at.col) - column).map_err(|_| Ref)?,
                ))
            })
            .collect::<FunctionResult<Vec<_>>>()?;
        Ok(Some(positions))
    }

    /// Read one logical member at zero-based offsets, preserving origin and omission.
    pub(in crate::eval) fn read_cell(
        &mut self,
        index: usize,
        row: usize,
        column: usize,
    ) -> FunctionResult<ScalarOperand> {
        self.host.prepare(index)?;
        let slot = self
            .host
            .argument_base
            .checked_add(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        let runtime = &mut *self.host.session.state.runtime;
        let operand = &runtime
            .vm
            .operands
            .get(slot)
            .and_then(Option::as_ref)
            .ok_or(VALUE_ERROR)?;
        let (value, origin, missing) = match &operand.value {
            Value::Missing if row == 0 && column == 0 => {
                (convert_legacy_missing_blank(), OperandOrigin::Direct, true)
            }
            Value::Missing => return Err(Ref.into()),
            Value::SelectedScalar(value, origin) if row == 0 && column == 0 => (
                copy_cell(&mut runtime.budget, value)?,
                *origin,
                operand.missing,
            ),
            Value::Scalar(value) if row == 0 && column == 0 => (
                copy_cell(&mut runtime.budget, value)?,
                OperandOrigin::Direct,
                operand.missing,
            ),
            Value::Scalar(_) | Value::SelectedScalar(_, _) => return Err(Ref.into()),
            Value::Array(rows) => {
                let value = rows.get(row).and_then(|row| row.get(column)).ok_or(Ref)?;
                (
                    copy_cell(&mut runtime.budget, value)?,
                    OperandOrigin::ArrayElement,
                    operand
                        .array_missing
                        .get(row)
                        .and_then(|flags| flags.get(column))
                        .copied()
                        .unwrap_or(false),
                )
            }
            Value::Refs(areas) | Value::SheetSpan(areas) if areas.len() == 1 => {
                // SAFETY: Both reference match arms explicitly require areas.len() == 1.
                let area = areas[0];
                let height = area
                    .r1
                    .checked_sub(area.r0)
                    .and_then(|size| size.checked_add(1))
                    .ok_or(Ref)?;
                let width = area
                    .c1
                    .checked_sub(area.c0)
                    .and_then(|size| size.checked_add(1))
                    .ok_or(Ref)?;
                if row as u64 >= u64::from(height) || column as u64 >= u64::from(width) {
                    return Err(Ref.into());
                }
                (
                    CellEvaluator::new(self.host.session).range_cell(
                        area.sheet,
                        CellAddress {
                            row: area.r0 + row as u32,
                            col: area.c0 + column as u32,
                        },
                    )?,
                    OperandOrigin::ReferencedCell,
                    false,
                )
            }
            Value::SignedRefs(areas) if areas.len() == 1 => {
                // SAFETY: Both reference match arms explicitly require areas.len() == 1.
                let plan = areas[0].read(row as u64, column as u64).ok_or(Ref)?;
                (
                    CellEvaluator::new(self.host.session).signed_range_read(plan)?,
                    OperandOrigin::ReferencedCell,
                    false,
                )
            }
            Value::SignedRefs(_) => return Err(VALUE_ERROR.into()),
            Value::SheetSpan(_) => {
                return Err(self.host.unsupported(
                    CapabilityId::FunctionAdapter,
                    "this coordinate port has no verified multi-sheet selection policy",
                ));
            }
            Value::Refs(_) => return Err(VALUE_ERROR.into()),
        };
        Ok(ScalarOperand {
            value,
            origin,
            missing,
        })
    }

    /// Criteria resize reference ranges from their anchor; arrays retain original bounds.
    pub(in crate::eval) fn read_relative_cell(
        &mut self,
        index: usize,
        row: usize,
        column: usize,
    ) -> FunctionResult<ScalarOperand> {
        self.host.prepare(index)?;
        if let Value::SignedRefs(areas) = &self.host.operand(index).ok_or(VALUE_ERROR)?.value {
            if areas.len() != 1 {
                return Err(VALUE_ERROR.into());
            }
            // SAFETY: Each reference branch returns unless areas.len() == 1 before reading element zero.
            let area = areas[0];
            self.host
                .session
                .state
                .runtime
                .budget
                .charge(WorkKind::ReferencePair, 1)?;
            let plan = area.relative(row, column).ok_or(Ref)?;
            return Ok(ScalarOperand {
                value: CellEvaluator::new(self.host.session).signed_range_read(plan)?,
                origin: OperandOrigin::ReferencedCell,
                missing: false,
            });
        }
        if let Value::Refs(areas) = &self.host.operand(index).ok_or(VALUE_ERROR)?.value {
            if areas.len() != 1 {
                return Err(VALUE_ERROR.into());
            }
            // SAFETY: Each reference branch returns unless areas.len() == 1 before reading element zero.
            let area = areas[0];
            self.host
                .session
                .state
                .runtime
                .budget
                .charge(WorkKind::ReferencePair, 1)?;
            let plan = CellReadPlan::relative(area, row, column)?;
            return Ok(ScalarOperand {
                value: CellEvaluator::new(self.host.session)
                    .range_cell(plan.sheet, plan.address)?,
                origin: OperandOrigin::ReferencedCell,
                missing: false,
            });
        }
        self.read_cell(index, row, column)
    }

    /// Inspect flattened collection length without traversing its cells.
    pub(in crate::eval) fn collection_len(&mut self, index: usize) -> FunctionResult<usize> {
        self.host.prepare(index)?;
        let slot = self
            .host
            .argument_base
            .checked_add(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        let runtime = &mut *self.host.session.state.runtime;
        let count = match &runtime
            .vm
            .operands
            .get(slot)
            .and_then(Option::as_ref)
            .ok_or(VALUE_ERROR)?
            .value
        {
            Value::Missing | Value::Scalar(_) | Value::SelectedScalar(_, _) => 1,
            Value::Array(rows) => rows
                .iter()
                .try_fold(0u64, |count, row| count.checked_add(row.len() as u64))
                .ok_or(Num)?,
            Value::SignedRefs(areas) => {
                let mut count = 0u64;
                for area in areas {
                    runtime.budget.charge(WorkKind::ReferencePair, 1)?;
                    count = count.checked_add(area.size()).ok_or(Num)?;
                }
                count
            }
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                let mut count = 0u64;
                for area in areas {
                    runtime.budget.charge(WorkKind::ReferencePair, 1)?;
                    count = count.checked_add(area.size()).ok_or(Num)?;
                }
                count
            }
        };
        usize::try_from(count).map_err(|_| {
            self.host.unsupported(
                CapabilityId::FunctionAdapter,
                "collection length does not fit this platform",
            )
        })
    }

    /// Read one flattened member in source sheet-first, row-major order.
    pub(in crate::eval) fn collection_cell(
        &mut self,
        index: usize,
        offset: usize,
    ) -> FunctionResult<ScalarOperand> {
        self.host.prepare(index)?;
        let slot = self
            .host
            .argument_base
            .checked_add(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        let runtime = &mut *self.host.session.state.runtime;
        match &runtime
            .vm
            .operands
            .get(slot)
            .and_then(Option::as_ref)
            .ok_or(VALUE_ERROR)?
            .value
        {
            Value::Missing | Value::Scalar(_) | Value::SelectedScalar(_, _) => {
                self.read_cell(index, 0, offset)
            }
            Value::Array(rows) => {
                let columns = rows.first().map_or(0, Vec::len);
                if columns == 0 {
                    return Err(VALUE_ERROR.into());
                }
                self.read_cell(index, offset / columns, offset % columns)
            }
            Value::SignedRefs(areas) => {
                let mut remaining = offset as u64;
                for area in areas {
                    runtime.budget.charge(WorkKind::ReferencePair, 1)?;
                    if remaining < area.size() {
                        let plan = area
                            .read(remaining / area.width(), remaining % area.width())
                            .ok_or(Ref)?;
                        return Ok(ScalarOperand {
                            value: CellEvaluator::new(self.host.session).signed_range_read(plan)?,
                            origin: OperandOrigin::ReferencedCell,
                            missing: false,
                        });
                    }
                    remaining -= area.size();
                }
                Err(Ref.into())
            }
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                let mut remaining = offset as u64;
                for area in areas {
                    runtime.budget.charge(WorkKind::ReferencePair, 1)?;
                    if remaining < area.size() {
                        let width = u64::from(area.c1 - area.c0 + 1);
                        let address = CellAddress {
                            row: area.r0 + (remaining / width) as u32,
                            col: area.c0 + (remaining % width) as u32,
                        };
                        let sheet = area.sheet;
                        return Ok(ScalarOperand {
                            value: CellEvaluator::new(self.host.session)
                                .range_cell(sheet, address)?,
                            origin: OperandOrigin::ReferencedCell,
                            missing: false,
                        });
                    }
                    remaining -= area.size();
                }
                Err(Ref.into())
            }
        }
    }

    /// Resolve rectangular dimensions without reading member values.
    pub(in crate::eval) fn shape(&mut self, index: usize) -> FunctionResult<(usize, usize)> {
        self.host.prepare(index)?;
        match &self.host.operand(index).ok_or(VALUE_ERROR)?.value {
            Value::Missing | Value::Scalar(_) | Value::SelectedScalar(_, _) => Ok((1, 1)),
            Value::Array(rows) => {
                let columns = rows.first().ok_or(VALUE_ERROR)?.len();
                if columns == 0 || rows.iter().any(|row| row.len() != columns) {
                    return Err(VALUE_ERROR.into());
                }
                Ok((rows.len(), columns))
            }
            Value::SheetSpan(areas) | Value::Refs(areas)
                if !areas.is_empty()
                    && (areas.len() == 1
                        || matches!(
                            self.host.operand(index).ok_or(VALUE_ERROR)?.value,
                            Value::SheetSpan(_)
                        )) =>
            {
                // SAFETY: The relevant match guards require a nonempty area vector or exactly one area.
                let area = areas[0];
                if area.r1 >= MAX_ROWS || area.c1 >= MAX_COLUMNS {
                    return Err(Ref.into());
                }
                let rows = area
                    .r1
                    .checked_sub(area.r0)
                    .and_then(|n| n.checked_add(1))
                    .ok_or(Ref)?;
                let columns = area
                    .c1
                    .checked_sub(area.c0)
                    .and_then(|n| n.checked_add(1))
                    .ok_or(Ref)?;
                Ok((rows as usize, columns as usize))
            }
            Value::SignedRefs(areas) if areas.len() == 1 => {
                // SAFETY: The relevant match guards require a nonempty area vector or exactly one area.
                areas[0].positive_shape().ok_or_else(|| {
                    self.host.unsupported(
                        CapabilityId::FunctionAdapter,
                        "shape port cannot represent zero/negative Java source dimensions",
                    )
                })
            }
            Value::SignedRefs(_) | Value::Refs(_) | Value::SheetSpan(_) => Err(VALUE_ERROR.into()),
        }
    }

    /// Resolve the finalized spill footprint of one admitted physical anchor.
    pub(in crate::eval) fn spill_extent(&mut self, area: Area) -> FunctionResult<Area> {
        Ok(SpillDemand::new(self.host.session).extent(area)??)
    }

    /// Materialize a supported value with the evaluator's original matrix admission and reads.
    pub(in crate::eval) fn matrix(
        &mut self,
        value: Value,
    ) -> EvalResult<Result<Vec<Vec<CellValue>>, CellError>> {
        ArgumentReader::new(self.host.session).matrix(value)
    }

    /// Count blank logical positions in a reference using sparse membership and reads.
    /// Nonreference values return None for the caller's scalar error precedence.
    pub(in crate::eval) fn countblank_references(
        &mut self,
        value: &Value,
    ) -> FunctionResult<Option<u64>> {
        let mut count = 0u64;
        match value {
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                for area in areas {
                    let addresses = SparseReader::new(self.host.session).sparse_addresses(*area)?;
                    let mut used = 0u64;
                    for address in addresses {
                        let value = CellEvaluator::new(self.host.session)
                            .range_cell(area.sheet, address)?;
                        if !matches!(value, Blank) && value != Text(String::new()) {
                            used += 1;
                        }
                    }
                    count += area.size() - used;
                }
            }
            Value::SignedRefs(areas) => {
                // Pinned COUNTBLANK accepts RefEval/ThreeDEval, not a RefListEval union.
                if areas.len() != 1 {
                    return Err(VALUE_ERROR.into());
                }
                for area in areas {
                    let mut used = 0u64;
                    if let Some(physical) = area.physical() {
                        for address in
                            SparseReader::new(self.host.session).sparse_addresses(physical)?
                        {
                            let value = CellEvaluator::new(self.host.session)
                                .range_cell(area.sheet, address)?;
                            if !matches!(value, Blank) && value != Text(String::new()) {
                                used += 1;
                            }
                        }
                    }
                    count += area.size() - used;
                }
            }
            _ => return Ok(None),
        }
        Ok(Some(count))
    }
}
